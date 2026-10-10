//! Transactional native-precision reference consuming identified performance.

use crate::{
    INSTRUMENT_LEVEL_ID, PerformanceError, PerformancePhase, PerformanceVoice, PerformanceVoices,
};
use ravet_foundation::{
    AudioLayout, Event, EventData, EventError, NoteExpression, NoteTarget, ParameterSchema,
    PrepareError, ProcessBlock, ProcessConfig, ProcessError, ProcessStatus, Processor, Sample,
    Tail,
};
use ravet_signal::{Adsr, AdsrConfig, EnvelopePhase};

#[derive(Clone)]
struct Slot {
    phase: f64,
    vibrato_phase: f64,
    envelope: Adsr,
}

#[derive(Clone)]
struct State<const N: usize> {
    voices: PerformanceVoices<N>,
    slots: [Slot; N],
    level: f32,
}

/// Independent mono/stereo no-input sine reference at native f32 or f64 precision.
///
/// [`PerformanceVoices`] owns normalized/identified note matching and exact values.
/// New/retriggered/stolen voices reset oscillator/envelope; tuning/vibrato changes
/// retain phase. ADSR is 5 ms attack, 10 ms decay, .8 sustain and 50 ms release.
/// Linear volume/expression multiply unquantized velocity. Stereo balance uses
/// 2*(1-pan), 2*pan (center duplicates mono); mono ignores pan. Vibrato is a
/// 6 Hz sine with depth0..1 semitone, with its rate capped at49% of sample rate.
/// Tuning adds semitones and oscillator frequency is capped at49% of sample rate.
/// Brightness/pressure remain exact readable values without shaping this sine.
/// Mix scales by level/N; level defaults .2 and uses [`INSTRUMENT_LEVEL_ID`].
///
/// Complete generic event validation precedes rendering into prepared private
/// scratch using a fixed copied DSP/voice state. Late identity conflicts or output
/// overflow discard that candidate, silence active outputs and preserve live
/// level/voices/phase/envelopes. This handles actual in-block envelope completion
/// before ID reuse, rather than assuming event-only ownership through the block.
/// Preparation accepts mono/stereo instruments, no auxiliary buses, <=65536
/// frames and <=1024 input/output events. Scratch allocates only at prepare/drop.
/// Process/reset avoid heap actions, locks and I/O; this is no deadline claim.
///
/// A positive output capacity explicitly enables original-target NoteEnd for
/// steals/chokes/envelope completion, in input/sample/slot order. Capacity zero
/// suppresses notifications while retaining internal completion. Completion
/// offsets order delivery, not exact termination timestamps. Supported normalized
/// notes/expressions follow receiver policy; level edits clamp0..1. Other global/
/// scoped parameters, MIDI/UMP and incoming NoteEnd are ignored after generic
/// validation. Transport is ignored. No pedals, MIDI conversion, program/codec,
/// editor, device, format export or click-free stealing is supplied here.
/// Reset/reprepare clear history while retaining level; deactivate becomes inactive.
pub struct PerformanceSine<const N: usize> {
    state: State<N>,
    config: Option<ProcessConfig>,
    audio: Vec<[f64; 2]>,
    completions: Vec<Event>,
}

impl<const N: usize> PerformanceSine<N> {
    pub fn new() -> Result<Self, PerformanceError> {
        let config =
            AdsrConfig::new(48000., 0.005, 0.010, 0.8, 0.050).expect("fixed reference envelope");
        Ok(Self {
            state: State {
                voices: PerformanceVoices::new()?,
                slots: std::array::from_fn(|_| Slot {
                    phase: 0.,
                    vibrato_phase: 0.,
                    envelope: Adsr::new(config),
                }),
                level: 0.2,
            },
            config: None,
            audio: Vec::new(),
            completions: Vec::new(),
        })
    }

    pub fn level(&self) -> f32 {
        self.state.level
    }

    /// Complete strict global physical level edit, without resetting DSP history.
    /// Finite0..1 values preserve f32 bits; invalid values leave level unchanged.
    pub fn set_level(&mut self, value: f32) -> Result<(), PerformanceError> {
        if !value.is_finite() || !(0. ..=1.).contains(&value) {
            return Err(PerformanceError::InvalidValue);
        }
        self.state.level = value;
        Ok(())
    }

    /// Immutable shared reference level metadata; no format registration implied.
    pub fn parameter_schema() -> ParameterSchema<'static, 1> {
        ParameterSchema::new(&crate::synthesis::PARAMETERS).expect("fixed level schema")
    }

    pub fn voices(&self) -> &[PerformanceVoice; N] {
        self.state.voices.voices()
    }

    fn prepare_inner(&mut self, config: ProcessConfig) -> Result<(), PrepareError> {
        self.config = None;
        if config.maximum_frames() > 65536
            || config.maximum_events() > 1024
            || config.maximum_output_events() > 1024
        {
            return Err(PrepareError::InvalidMaximumFrames);
        }
        if ![AudioLayout::MONO_INSTRUMENT, AudioLayout::STEREO_INSTRUMENT]
            .contains(&config.layout())
            || !config.auxiliary_buses().is_empty()
        {
            return Err(PrepareError::UnsupportedLayout);
        }
        let envelope = AdsrConfig::new(config.sample_rate(), 0.005, 0.010, 0.8, 0.050)
            .map_err(|_| PrepareError::InvalidSmoothingTime)?;
        self.audio.resize(config.maximum_frames(), [0.; 2]);
        self.completions.resize(
            config.maximum_output_events(),
            Event {
                frame_offset: 0,
                data: EventData::NoteEnd {
                    target: NoteTarget::default(),
                },
            },
        );
        for slot in &mut self.state.slots {
            slot.envelope = Adsr::new(envelope);
        }
        self.reset_inner();
        self.config = Some(config);
        Ok(())
    }

    fn reset_inner(&mut self) {
        self.state.voices.reset();
        for slot in &mut self.state.slots {
            slot.envelope.reset();
            slot.phase = 0.;
            slot.vibrato_phase = 0.;
        }
    }

    fn stage_completion(
        storage: &mut [Event],
        count: &mut usize,
        enabled: bool,
        target: NoteTarget,
        offset: usize,
    ) -> Result<(), ProcessError> {
        if enabled {
            let event = storage
                .get_mut(*count)
                .ok_or(ProcessError::OutputEventCapacity)?;
            *event = Event {
                frame_offset: offset,
                data: EventData::NoteEnd { target },
            };
            *count += 1;
        }
        Ok(())
    }

    fn apply(
        state: &mut State<N>,
        event: Event,
        index: usize,
        storage: &mut [Event],
        count: &mut usize,
        enabled: bool,
    ) -> Result<(), ProcessError> {
        match event.data {
            EventData::ParameterValue {
                id: INSTRUMENT_LEVEL_ID,
                value,
            } => {
                state.level = value.clamp(0., 1.);
                return Ok(());
            }
            EventData::NoteOn { .. }
            | EventData::NoteOff { .. }
            | EventData::NoteWithId { .. }
            | EventData::NoteOffScoped { .. }
            | EventData::NoteChoke { .. }
            | EventData::NoteExpression { .. } => {}
            _ => return Ok(()),
        }
        let changes = state
            .voices
            .apply(event.data)
            .map_err(|_| ProcessError::Events(EventError::InvalidValue { index }))?;
        for (_, target) in changes.completions() {
            Self::stage_completion(storage, count, enabled, target, event.frame_offset)?;
        }
        for index in 0..N {
            if changes.changed & (1u128 << index) == 0 {
                continue;
            }
            let slot = &mut state.slots[index];
            if state.voices.voices()[index].phase == PerformancePhase::Idle {
                slot.envelope.reset();
                slot.phase = 0.;
                slot.vibrato_phase = 0.;
            }
            if changes.started.is_some_and(|start| start.index == index) {
                slot.phase = 0.;
                slot.vibrato_phase = 0.;
                slot.envelope.reset();
                slot.envelope.note_on();
            }
            if changes.released & (1u128 << index) != 0 {
                slot.envelope.note_off();
                if slot.envelope.phase() == EnvelopePhase::Idle {
                    let target = state
                        .voices
                        .finish(index)
                        .expect("fixed owned slot")
                        .expect("released target");
                    Self::stage_completion(storage, count, enabled, target, event.frame_offset)?;
                }
            }
        }
        Ok(())
    }

    fn render(
        state: &mut State<N>,
        config: ProcessConfig,
        events: &[Event],
        audio: &mut [[f64; 2]],
        storage: &mut [Event],
    ) -> Result<usize, ProcessError> {
        let mut count = 0;
        let enabled = config.maximum_output_events() != 0;
        let mut next = 0;
        if audio.is_empty() {
            for (index, event) in events.iter().copied().enumerate() {
                Self::apply(state, event, index, storage, &mut count, enabled)?;
            }
        }
        let rate = config.sample_rate();
        let vibrato_increment = 6_f64.min(rate * 0.49) / rate;
        for (frame, output) in audio.iter_mut().enumerate() {
            while next < events.len() && events[next].frame_offset == frame {
                Self::apply(state, events[next], next, storage, &mut count, enabled)?;
                next += 1;
            }
            let mut mix = [0.; 2];
            for index in 0..N {
                let voice = state.voices.voices()[index];
                if voice.phase == PerformancePhase::Idle {
                    continue;
                }
                let slot = &mut state.slots[index];
                let amplitude = slot.envelope.process_sample();
                let expression = voice.expression;
                let sample = (slot.phase * std::f64::consts::TAU).sin()
                    * amplitude
                    * f64::from(voice.velocity)
                    * expression.get(NoteExpression::Volume)
                    * expression.get(NoteExpression::Expression);
                let pan = if config.layout().output_channels() == 1 {
                    0.5
                } else {
                    expression.get(NoteExpression::Pan)
                };
                mix[0] += sample * (2. * (1. - pan));
                mix[1] += sample * (2. * pan);
                let vibrato = (slot.vibrato_phase * std::f64::consts::TAU).sin()
                    * expression.get(NoteExpression::Vibrato);
                let pitch = f64::from(
                    voice
                        .target
                        .expect("owned voice")
                        .key
                        .expect("concrete key"),
                ) + expression.get(NoteExpression::Tuning)
                    + vibrato;
                let frequency = (440. * 2_f64.powf((pitch - 69.) / 12.)).min(rate * 0.49);
                slot.phase = (slot.phase + frequency / rate).fract();
                slot.vibrato_phase = (slot.vibrato_phase + vibrato_increment).fract();
                if slot.envelope.phase() == EnvelopePhase::Idle {
                    let target = state
                        .voices
                        .finish(index)
                        .expect("fixed slot")
                        .expect("owned completion");
                    Self::stage_completion(storage, &mut count, enabled, target, frame)?;
                }
            }
            let scale = f64::from(state.level) / N as f64;
            *output = [mix[0] * scale, mix[1] * scale];
        }
        Ok(count)
    }

    fn process_inner<S: Sample>(
        &mut self,
        block: &mut ProcessBlock<'_, '_, '_, S>,
        convert: impl Fn(f64) -> S,
    ) -> ProcessStatus {
        let config = match self
            .config
            .ok_or(ProcessError::NotPrepared)
            .and_then(|config| config.validate_block(block).map(|()| config))
        {
            Ok(config) => config,
            Err(error) => {
                block.clear_outputs();
                return ProcessStatus::Fault(error);
            }
        };
        let frames = block.audio.frames();
        let mut candidate = self.state.clone();
        let count = match Self::render(
            &mut candidate,
            config,
            block.events,
            &mut self.audio[..frames],
            &mut self.completions,
        ) {
            Ok(count) => count,
            Err(error) => {
                block.clear_outputs();
                return ProcessStatus::Fault(error);
            }
        };
        if let Some(output) = &mut block.output_events {
            for event in &self.completions[..count] {
                output
                    .push(*event)
                    .expect("prevalidated complete private output");
            }
        }
        for channel in 0..block.audio.output_channels() {
            for (sample, frame) in block
                .audio
                .output_mut(channel)
                .expect("validated channel")
                .iter_mut()
                .zip(&self.audio[..frames])
            {
                *sample = convert(frame[channel]);
            }
        }
        self.state = candidate;
        ProcessStatus::Continue
    }
}

macro_rules! processor {
    ($sample:ty, $convert:expr) => {
        impl<const N: usize> Processor<$sample> for PerformanceSine<N> {
            fn prepare(&mut self, config: ProcessConfig) -> Result<(), PrepareError> {
                self.prepare_inner(config)
            }
            fn process(&mut self, block: &mut ProcessBlock<'_, '_, '_, $sample>) -> ProcessStatus {
                self.process_inner(block, $convert)
            }
            fn reset(&mut self) {
                self.reset_inner();
            }
            fn deactivate(&mut self) {
                self.reset_inner();
                self.config = None;
            }
            fn tail(&self) -> Tail {
                Tail::Infinite
            }
        }
    };
}
processor!(f32, |value| value as f32);
processor!(f64, |value| value);
