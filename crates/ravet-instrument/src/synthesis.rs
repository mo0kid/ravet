//! Portable synthesis reference consuming prepared voice/envelope contracts.
use crate::{NegotiatedVoices, VoiceError, VoicePhase, ZoneSide};
use ravet_foundation::{
    AudioLayout, EventData, MidiMessage, ParameterDescriptor, ParameterError, ParameterId,
    ParameterProgram, ParameterRange, ParameterScale, ParameterSchema, PrepareError, ProcessBlock,
    ProcessConfig, ProcessError, ProcessStatus, Processor, ProgramBank, ProgramId, Sample, Tail,
};
use ravet_signal::{Adsr, AdsrConfig, EnvelopePhase};
/// Permanent linear amplitude level ID, physical range 0..1, default 0.2.
pub const INSTRUMENT_LEVEL_ID: ParameterId = ParameterId(1);
const LEVEL_RANGE: ParameterRange = match ParameterRange::new(0., 1., ParameterScale::Linear, None)
{
    Ok(range) => range,
    Err(_) => panic!("fixed level range"),
};
pub(super) static PARAMETERS: [ParameterDescriptor<'static>; 1] = [ParameterDescriptor {
    id: INSTRUMENT_LEVEL_ID,
    name: "Level",
    unit: "",
    range: LEVEL_RANGE,
    default: 0.2,
    automatable: true,
}];
/// Public reference programs, zero-based MIDI bank 0/programs 0..2. IDs 1..3
/// remain stable independently of catalogue order. Each contains complete level
/// state only: Default=0.2, Quiet=0.05, Full=1.0; no voice/history/zone reset.
pub static SINE_PROGRAMS: [ParameterProgram<'static, 1>; 3] = [
    ParameterProgram {
        id: ProgramId(1),
        name: "Default",
        midi_bank: 0,
        midi_number: 0,
        values: [0.2],
    },
    ParameterProgram {
        id: ProgramId(2),
        name: "Quiet",
        midi_bank: 0,
        midi_number: 1,
        values: [0.05],
    },
    ParameterProgram {
        id: ProgramId(3),
        name: "Full",
        midi_bank: 0,
        midi_number: 2,
        values: [1.],
    },
];
struct Slot {
    phase: f64,
    increment: f64,
    envelope: Adsr,
    releasing: bool,
}
/// Reference MIDI program policy. The default applies the published complete
/// programs to Level. Disabling application still updates negotiation/request
/// observations and voices; only automatic global Level replacement is skipped.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SineOptions {
    pub apply_midi_programs: bool,
}
impl Default for SineOptions {
    fn default() -> Self {
        Self {
            apply_midi_programs: true,
        }
    }
}

/// Fixed 1..128-voice mono/stereo f32 sine instrument with no audio input.
/// MIDI notes/sustain/sostenuto/stealing, opt-in chosen-side MPE negotiation and combined
/// master/member bend drive phase-continuous oscillators. New/stolen notes reset
/// phase/envelope. ADSR is fixed at 5 ms attack, 10 ms decay, 0.8 sustain and
/// 50 ms release. Mix scales by level/N; stereo duplicates mono. Pressure and
/// timbre remain available as snapshots but do not alter this sine reference.
///
/// Preparation accepts finite positive rates with ADSR counts fitting u32,
/// <=65,536 frames, <=1,024 input events, no
/// auxiliary/output-event buses. Complete blocks validate before mutation.
/// Note events quantize normalized velocity to MIDI seven-bit (positive rounds
/// to at least one); MIDI packets retain their wire velocity. Level clamps to
/// 0..1 without smoothing. Unknown parameters and transport are ignored.
/// Bank/Program Change on the chosen side's master (0 lower, 15 upper), even
/// before MPE negotiation, selects the three complete level programs with the
/// default [`SineOptions::apply_midi_programs`] policy. [`Self::with_options`] can
/// disable this Level replacement while preserving accepted raw bank/program
/// request observations and existing voice behavior. Other channels/unavailable
/// addresses preserve Level. Selection preserves voices/phase,
/// zone/ranges and envelopes; later level edits/requests win in event order.
/// Frequencies clamp to 49% of sample rate to stay below Nyquist. No state
/// codec, editor, device/format export, click-free stealing or deadline claim.
/// Reset/reprepare clear notes/history while preserving level and negotiated
/// configuration, clearing pending bank selectors. Accepted zone configurations
/// also clear selectors; bend-range changes preserve them. Process/reset use no heap,
/// blocking or I/O. There is no persisted current program identity or host menu.
pub struct SineInstrument<const N: usize> {
    voices: NegotiatedVoices<N>,
    slots: [Slot; N],
    config: Option<ProcessConfig>,
    level: f32,
    program_channel: u8,
    programs: ProgramBank<'static, 1>,
    options: SineOptions,
}
impl<const N: usize> SineInstrument<N> {
    pub fn new(side: ZoneSide) -> Result<Self, VoiceError> {
        Self::with_options(side, SineOptions::default())
    }
    /// Construct with explicit program policy, preserving the ordinary voice,
    /// bank/request, pedal and negotiation behavior.
    pub fn with_options(side: ZoneSide, options: SineOptions) -> Result<Self, VoiceError> {
        let voices = NegotiatedVoices::new(side)?;
        let envelope = AdsrConfig::new(48000., 0.005, 0.010, 0.8, 0.050).expect("fixed envelope");
        Ok(Self {
            voices,
            slots: std::array::from_fn(|_| Slot {
                phase: 0.,
                increment: 0.,
                envelope: Adsr::new(envelope),
                releasing: false,
            }),
            config: None,
            options,
            level: 0.2,
            program_channel: if side == ZoneSide::Lower { 0 } else { 15 },
            programs: ProgramBank::new(
                ParameterSchema::new(&PARAMETERS).expect("fixed level schema"),
                &SINE_PROGRAMS,
            )
            .expect("fixed complete programs"),
        })
    }
    pub fn level(&self) -> f32 {
        self.level
    }
    /// Complete immutable Level schema; it does not describe voices/history.
    pub fn parameter_schema() -> ParameterSchema<'static, 1> {
        ParameterSchema::new(&PARAMETERS).expect("fixed level schema")
    }
    /// CONTROL-only exact Level replacement. Invalid/nonfinite or out-of-range
    /// values leave both Level and live synthesis history untouched.
    pub fn set_level(&mut self, value: f32) -> Result<(), ParameterError> {
        Self::parameter_schema().state([value])?;
        self.level = value;
        Ok(())
    }
    /// Last negotiated raw Program Change request, even when automatic complete
    /// program application is disabled. Reset/reprepare clear these observations.
    pub fn last_program_request(&self, channel: u8) -> Option<crate::MidiProgramRequest> {
        self.voices.last_program_request(channel)
    }
    pub fn voices(&self) -> &[crate::Voice; N] {
        self.voices.voices()
    }
    /// Immutable reference catalogue; no mutable selection identity. Custom
    /// level edits change only level, and requests preserve existing voices.
    pub fn programs(&self) -> ProgramBank<'static, 1> {
        self.programs
    }
    fn apply(&mut self, data: EventData) {
        let midi = match data {
            EventData::Midi(message) => message,
            EventData::NoteOn {
                channel,
                key,
                velocity,
            } => MidiMessage::from_bytes(&[
                0x90 | channel,
                key,
                if velocity > 0. {
                    (velocity * 127.).round().max(1.) as u8
                } else {
                    0
                },
            ])
            .expect("validated note"),
            EventData::NoteOff {
                channel,
                key,
                velocity,
            } => MidiMessage::from_bytes(&[0x80 | channel, key, (velocity * 127.).round() as u8])
                .expect("validated note"),
            EventData::ParameterValue {
                id: INSTRUMENT_LEVEL_ID,
                value,
            } => {
                self.level = value.clamp(0., 1.);
                return;
            }
            _ => return,
        };
        let negotiated = self.voices.apply(midi);
        if self.options.apply_midi_programs
            && let Some(request) = negotiated.program_request
            && request.channel == self.program_channel
            && let Some(index) = self.programs.midi_index(request.bank, request.program)
        {
            self.level = self
                .programs
                .state(index)
                .expect("validated program")
                .values()[0];
        }
        let change = negotiated.voices;
        for i in 0..N {
            if change.changed & (1u128 << i) == 0 {
                continue;
            }
            let voice = self.voices.voices()[i];
            let slot = &mut self.slots[i];
            if voice.phase == VoicePhase::Idle {
                slot.envelope.reset();
                slot.phase = 0.;
                slot.releasing = false;
                continue;
            }
            if change.started.is_some_and(|start| start.index == i) {
                slot.phase = 0.;
                slot.envelope.reset();
                slot.envelope.note_on();
                slot.releasing = false;
            }
            if voice.phase == VoicePhase::Released && !slot.releasing {
                slot.envelope.note_off();
                slot.releasing = true;
            }
            let pitch = f64::from(voice.note.expect("occupied voice").key)
                + f64::from(voice.expression.member_bend + voice.expression.master_bend);
            let rate = self.config.expect("active event processing").sample_rate();
            let frequency = (440. * 2_f64.powf((pitch - 69.) / 12.)).min(rate * 0.49);
            slot.increment = frequency / rate;
        }
    }
    fn prepare_shared(&mut self, config: ProcessConfig) -> Result<(), PrepareError> {
        self.config = None;
        if config.maximum_frames() > 65_536 || config.maximum_events() > 1_024 {
            return Err(PrepareError::InvalidMaximumFrames);
        }
        if ![AudioLayout::MONO_INSTRUMENT, AudioLayout::STEREO_INSTRUMENT]
            .contains(&config.layout())
            || !config.auxiliary_buses().is_empty()
            || config.maximum_output_events() != 0
        {
            return Err(PrepareError::UnsupportedLayout);
        }
        let envelope = AdsrConfig::new(config.sample_rate(), 0.005, 0.010, 0.8, 0.050)
            .map_err(|_| PrepareError::InvalidSmoothingTime)?;
        for slot in &mut self.slots {
            slot.envelope = Adsr::new(envelope);
        }
        self.reset_shared();
        self.config = Some(config);
        Ok(())
    }
    fn render<S: Sample>(
        &mut self,
        block: &mut ProcessBlock<'_, '_, '_, S>,
        convert: fn(f64) -> S,
    ) -> ProcessStatus {
        if let Err(error) = self
            .config
            .ok_or(ProcessError::NotPrepared)
            .and_then(|config| config.validate_block(block))
        {
            block.clear_outputs();
            return ProcessStatus::Fault(error);
        }
        let mut next = 0;
        if block.audio.frames() == 0 {
            for event in block.events {
                self.apply(event.data);
            }
        }
        for frame in 0..block.audio.frames() {
            while next < block.events.len() && block.events[next].frame_offset == frame {
                self.apply(block.events[next].data);
                next += 1;
            }
            let mut mix = 0.;
            for i in 0..N {
                let voice = self.voices.voices()[i];
                if voice.phase == VoicePhase::Idle {
                    continue;
                }
                let slot = &mut self.slots[i];
                let amplitude = slot.envelope.process_sample();
                mix += (slot.phase * std::f64::consts::TAU).sin()
                    * amplitude
                    * f64::from(voice.velocity);
                slot.phase = (slot.phase + slot.increment).fract();
                if slot.envelope.phase() == EnvelopePhase::Idle {
                    self.voices.finish(i).expect("fixed slot");
                }
            }
            let sample = convert(mix * f64::from(self.level) / N as f64);
            for channel in 0..block.audio.output_channels() {
                block.audio.output_mut(channel).expect("validated output")[frame] = sample;
            }
        }
        ProcessStatus::Continue
    }
    fn reset_shared(&mut self) {
        self.voices.reset();
        for slot in &mut self.slots {
            slot.envelope.reset();
            slot.phase = 0.;
            slot.increment = 0.;
            slot.releasing = false;
        }
    }
    fn deactivate_shared(&mut self) {
        self.reset_shared();
        self.config = None;
    }
}
impl<const N: usize> Processor for SineInstrument<N> {
    fn prepare(&mut self, config: ProcessConfig) -> Result<(), PrepareError> {
        self.prepare_shared(config)
    }
    fn process(&mut self, block: &mut ProcessBlock<'_, '_, '_>) -> ProcessStatus {
        self.render(block, |sample| sample as f32)
    }
    fn reset(&mut self) {
        self.reset_shared();
    }
    fn deactivate(&mut self) {
        self.deactivate_shared();
    }
    /// A held/sustained note can sound indefinitely until explicit note-off/pedal.
    fn tail(&self) -> Tail {
        Tail::Infinite
    }
}

/// Additive f64 facade over the same synthesis/voice/render kernel. Preparation,
/// Level, envelope and wire velocities keep their existing scalar contracts;
/// oscillator mix is published directly as f64 without an intermediate f32 output.
/// This type leaves existing `SineInstrument::new` inference unchanged.
pub struct SineInstrument64<const N: usize>(SineInstrument<N>);
impl<const N: usize> SineInstrument64<N> {
    pub fn new(side: ZoneSide) -> Result<Self, VoiceError> {
        Self::with_options(side, SineOptions::default())
    }
    pub fn with_options(side: ZoneSide, options: SineOptions) -> Result<Self, VoiceError> {
        SineInstrument::with_options(side, options).map(Self)
    }
    pub fn level(&self) -> f32 {
        self.0.level()
    }
    pub fn set_level(&mut self, value: f32) -> Result<(), ParameterError> {
        self.0.set_level(value)
    }
    pub fn parameter_schema() -> ParameterSchema<'static, 1> {
        SineInstrument::<N>::parameter_schema()
    }
    pub fn voices(&self) -> &[crate::Voice; N] {
        self.0.voices()
    }
    pub fn programs(&self) -> ProgramBank<'static, 1> {
        self.0.programs()
    }
    pub fn last_program_request(&self, channel: u8) -> Option<crate::MidiProgramRequest> {
        self.0.last_program_request(channel)
    }
}
impl<const N: usize> Processor<f64> for SineInstrument64<N> {
    fn prepare(&mut self, config: ProcessConfig) -> Result<(), PrepareError> {
        self.0.prepare_shared(config)
    }
    fn process(&mut self, block: &mut ProcessBlock<'_, '_, '_, f64>) -> ProcessStatus {
        self.0.render(block, |sample| sample)
    }
    fn reset(&mut self) {
        self.0.reset_shared();
    }
    fn deactivate(&mut self) {
        self.0.deactivate_shared();
    }
    fn tail(&self) -> Tail {
        Tail::Infinite
    }
}
