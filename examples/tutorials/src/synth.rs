//! Eight-voice subtractive synthesis with one ADSR per voice shared by VCA and VCF.
use ravet_foundation::{
    AudioLayout, EventData, MidiMessage, PrepareError, ProcessBlock, ProcessConfig, ProcessError,
    ProcessStatus, Processor, Tail,
};
use ravet_instrument::{MpeReceiver, Voice, VoicePhase};
use ravet_signal::{
    Adsr, AdsrConfig, EnvelopePhase, Oscillator, StateVariableCoefficients, StateVariableFilter,
    Waveform,
};

/// Fixed polyphony; mix scales by this capacity.
pub const VOICES: usize = 8;
/// Waveform, level, cutoff Hz, resonance Q, envelope semitones, A ms, D ms, S, R ms.
pub const DEFAULT_VALUES: [f32; 9] = [1., 0.6, 800., 0.7, 36., 10., 150., 0.7, 300.];
/// Physical bounds; waveform must additionally be an integer 0..3.
pub const RANGES: [(f32, f32); 9] = [
    (0., 3.),
    (0., 1.),
    (20., 20_000.),
    (0.5, 8.),
    (0., 72.),
    (0., 5_000.),
    (0., 5_000.),
    (0., 1.),
    (0., 5_000.),
];
const FILTER_STEPS: usize = 256;

/// Invalid physical controls, unsupported rate, or prepared-patch rate mismatch.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PatchError;
impl std::fmt::Display for PatchError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("invalid synth patch or sample rate")
    }
}
impl std::error::Error for PatchError {}

/// Fixed-storage patch designed on the control thread, including a 256-step VCF table.
/// Envelope amount is a positive cutoff excursion in semitones. Member/master
/// CC74 adds up to 24 semitones; the combined excursion clamps below 45% of rate.
/// ADSR settings latch at note-on. Waveform, level and VCF controls also affect
/// sounding notes when a new patch is applied. No parameter crossfade is provided.
#[derive(Clone)]
pub struct SynthPatch {
    values: [f32; 9],
    rate: f64,
    envelope: AdsrConfig,
    coefficients: [StateVariableCoefficients; FILTER_STEPS],
}
impl SynthPatch {
    /// Validate all controls and design the envelope/VCF table outside processing.
    /// Rates are 8000..192000 Hz. Time controls are milliseconds, sustain/level
    /// are linear 0..1, resonance is Q. Saw/square use polyBLEP; triangle may alias.
    pub fn new(rate: f64, values: [f32; 9]) -> Result<Self, PatchError> {
        if !rate.is_finite()
            || !(8_000.0..=192_000.).contains(&rate)
            || values
                .iter()
                .zip(RANGES)
                .any(|(v, (lo, hi))| !v.is_finite() || !(lo..=hi).contains(v))
            || values[0].fract() != 0.
        {
            return Err(PatchError);
        }
        let envelope = AdsrConfig::new(
            rate,
            f64::from(values[5]) / 1000.,
            f64::from(values[6]) / 1000.,
            f64::from(values[7]),
            f64::from(values[8]) / 1000.,
        )
        .map_err(|_| PatchError)?;
        let coefficients = std::array::from_fn(|index| {
            let semitones = f64::from(values[4] + 24.) * index as f64 / (FILTER_STEPS - 1) as f64;
            let cutoff = (f64::from(values[2]) * 2_f64.powf(semitones / 12.)).min(rate * 0.45);
            StateVariableCoefficients::new(rate, cutoff, f64::from(values[3]))
                .expect("validated table bounds")
        });
        Ok(Self {
            values,
            rate,
            envelope,
            coefficients,
        })
    }
    /// Current physical controls in the order documented by `DEFAULT_VALUES`.
    pub fn values(&self) -> [f32; 9] {
        self.values
    }
    fn waveform(&self) -> Waveform {
        [
            Waveform::Sine,
            Waveform::Saw,
            Waveform::Square,
            Waveform::Triangle,
        ][self.values[0] as usize]
    }
}
struct Slot {
    oscillator: Oscillator,
    envelope: Adsr,
    filter: StateVariableFilter,
    releasing: bool,
}
impl Slot {
    fn new(patch: &SynthPatch) -> Self {
        Self {
            oscillator: Oscillator::new(patch.rate, 440., patch.waveform())
                .expect("prepared oscillator"),
            envelope: Adsr::new(patch.envelope),
            filter: StateVariableFilter::new(patch.coefficients[0]),
            releasing: false,
        }
    }
}
/// Example mono/stereo f32 instrument with eight fixed MPE-aware voices.
///
/// Each voice runs oscillator -> low-pass VCF -> VCA. Its single ADSR drives
/// VCA amplitude and VCF cutoff; MIDI velocity scales amplitude, combined bend
/// retunes the oscillator, pressure adds up to 50% amplitude and CC74 opens the
/// filter by up to 24 semitones. One shared voice pool accepts the existing
/// two-zone MPE receiver's RPN/pedal policy. New/stolen voices reset histories.
/// Release finishes when the VCA envelope reaches zero; there is no separate
/// filter tail. Mix scales by level/8, without an output limiter or headroom guarantee.
///
/// Prepared processing/reset allocate nothing. Up to 1024 frames and 128 MIDI
/// events, no inputs/aux/output events, rates 8000..192000. Complete blocks
/// validate before mutation. Parameter and UMP events are ignored; patches are
/// an explicit separate prepared API. No MIDI 2.0 synthesis or plug-in export.
pub struct SubtractiveSynth {
    voices: MpeReceiver<VOICES>,
    slots: [Slot; VOICES],
    patch: SynthPatch,
    config: Option<ProcessConfig>,
}
impl SubtractiveSynth {
    /// Construct an inactive synth with the default saw patch.
    pub fn new() -> Self {
        let patch = SynthPatch::new(48_000., DEFAULT_VALUES).expect("default patch");
        Self {
            voices: MpeReceiver::new().expect("fixed voice count"),
            slots: std::array::from_fn(|_| Slot::new(&patch)),
            patch,
            config: None,
        }
    }
    /// Copy a predesigned patch without heap operations. Reject a mismatched
    /// active rate before mutation. ADSR changes apply only to subsequent notes;
    /// waveform changes retain phase and filter changes retain integrator history.
    pub fn set_patch(&mut self, patch: &SynthPatch) -> Result<(), PatchError> {
        if self.config.is_some_and(|c| c.sample_rate() != patch.rate) {
            return Err(PatchError);
        }
        self.patch.clone_from(patch);
        for slot in &mut self.slots {
            slot.oscillator.set_waveform(patch.waveform());
        }
        Ok(())
    }
    /// Voice expression snapshots; presentation belongs outside processing.
    pub fn voices(&self) -> &[Voice; VOICES] {
        self.voices.voices()
    }
    /// VCA/VCF's shared instantaneous envelope levels, in voice-slot order.
    pub fn envelope_levels(&self) -> [f64; VOICES] {
        self.slots.each_ref().map(|s| s.envelope.level())
    }
    fn apply(&mut self, data: EventData) {
        let message = match data {
            EventData::Midi(m) => m,
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
            _ => return,
        };
        let changes = self.voices.apply(message).voices;
        for i in 0..VOICES {
            if changes.changed & (1 << i) == 0 {
                continue;
            }
            let voice = self.voices.voices()[i];
            let slot = &mut self.slots[i];
            if voice.phase == VoicePhase::Idle {
                slot.envelope.reset();
                slot.filter.reset();
                continue;
            }
            if changes.started.is_some_and(|s| s.index == i) {
                slot.oscillator.reset();
                slot.filter.reset();
                slot.envelope = Adsr::new(self.patch.envelope);
                slot.envelope.note_on();
                slot.releasing = false;
            }
            if voice.phase == VoicePhase::Released && !slot.releasing {
                slot.envelope.note_off();
                slot.releasing = true;
            }
            let pitch = f64::from(voice.note.expect("occupied").key)
                + f64::from(voice.expression.member_bend + voice.expression.master_bend);
            let frequency = (440. * 2_f64.powf((pitch - 69.) / 12.)).min(self.patch.rate * 0.49);
            slot.oscillator
                .set_frequency(frequency)
                .expect("bounded pitch");
        }
    }
}
impl Default for SubtractiveSynth {
    fn default() -> Self {
        Self::new()
    }
}
impl Processor for SubtractiveSynth {
    fn prepare(&mut self, config: ProcessConfig) -> Result<(), PrepareError> {
        self.config = None;
        if config.maximum_frames() > 1024 || config.maximum_events() > 128 {
            return Err(PrepareError::InvalidMaximumFrames);
        }
        if ![AudioLayout::MONO_INSTRUMENT, AudioLayout::STEREO_INSTRUMENT]
            .contains(&config.layout())
            || !config.auxiliary_buses().is_empty()
            || config.maximum_output_events() != 0
        {
            return Err(PrepareError::UnsupportedLayout);
        }
        self.patch = SynthPatch::new(config.sample_rate(), self.patch.values)
            .map_err(|_| PrepareError::InvalidSampleRate)?;
        self.slots = std::array::from_fn(|_| Slot::new(&self.patch));
        self.voices.reset();
        self.config = Some(config);
        Ok(())
    }
    fn process(&mut self, block: &mut ProcessBlock<'_, '_, '_>) -> ProcessStatus {
        if let Err(error) = self
            .config
            .ok_or(ProcessError::NotPrepared)
            .and_then(|c| c.validate_block(block))
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
            for i in 0..VOICES {
                let voice = self.voices.voices()[i];
                if voice.phase == VoicePhase::Idle {
                    continue;
                }
                let slot = &mut self.slots[i];
                let envelope = slot.envelope.process_sample();
                let timbre = f64::from(
                    (voice.expression.timbre + voice.expression.master_timbre).clamp(0., 1.),
                );
                let excursion = envelope * f64::from(self.patch.values[4]) + timbre * 24.;
                let index = (excursion / f64::from(self.patch.values[4] + 24.)
                    * (FILTER_STEPS - 1) as f64)
                    .round() as usize;
                slot.filter
                    .set_coefficients(self.patch.coefficients[index.min(FILTER_STEPS - 1)]);
                let pressure = f64::from(
                    (voice.expression.pressure + voice.expression.master_pressure).clamp(0., 1.),
                );
                mix += slot
                    .filter
                    .process_sample(slot.oscillator.process_sample())
                    .low_pass
                    * envelope
                    * f64::from(voice.velocity)
                    * (1. + 0.5 * pressure);
                if slot.envelope.phase() == EnvelopePhase::Idle {
                    self.voices.finish(i).expect("fixed slot");
                }
            }
            let sample = (mix * f64::from(self.patch.values[1]) / VOICES as f64) as f32;
            for channel in 0..block.audio.output_channels() {
                block.audio.output_mut(channel).expect("output")[frame] = sample;
            }
        }
        ProcessStatus::Continue
    }
    fn reset(&mut self) {
        self.voices.reset();
        for slot in &mut self.slots {
            slot.envelope.reset();
            slot.filter.reset();
            slot.oscillator.reset();
            slot.releasing = false;
        }
    }
    fn deactivate(&mut self) {
        self.reset();
        self.config = None;
    }
    fn tail(&self) -> Tail {
        Tail::Infinite
    }
}
