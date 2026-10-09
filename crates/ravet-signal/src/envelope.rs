//! Sample-counted linear ADSR envelope, independent of voice/host policy.

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct EnvelopeError;

/// Validated durations in prepared samples; sustain is linear amplitude 0..=1.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct AdsrConfig {
    attack: u32,
    decay: u32,
    sustain: f64,
    release: u32,
}
fn duration_samples(sample_rate: f64, seconds: f64) -> Result<u32, EnvelopeError> {
    if !sample_rate.is_finite() || sample_rate <= 0. || !seconds.is_finite() || seconds < 0. {
        return Err(EnvelopeError);
    }
    let count = (seconds * sample_rate).round();
    if !count.is_finite() || count > f64::from(u32::MAX) {
        return Err(EnvelopeError);
    }
    Ok(count as u32)
}
impl AdsrConfig {
    /// Durations are seconds, finite/nonnegative, rounded to nearest samples.
    /// Counts beyond u32::MAX reject. Construction is control-thread
    /// work; no envelope configuration changes happen implicitly in process.
    pub fn new(
        sample_rate: f64,
        attack: f64,
        decay: f64,
        sustain: f64,
        release: f64,
    ) -> Result<Self, EnvelopeError> {
        if !sustain.is_finite() || !(0.0..=1.0).contains(&sustain) {
            return Err(EnvelopeError);
        }
        Ok(Self {
            attack: duration_samples(sample_rate, attack)?,
            decay: duration_samples(sample_rate, decay)?,
            sustain,
            release: duration_samples(sample_rate, release)?,
        })
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EnvelopePhase {
    Idle,
    Attack,
    Decay,
    Sustain,
    Release,
}

/// Linear ADSR from the current level; retrigger does not reset to zero.
/// The first processed sample advances the ramp; the last snaps to its target.
/// Zero-duration stages transition immediately. Configuration replacement is
/// explicit by constructing a new envelope. Sample/block/reset operations do
/// not allocate. Voice lifetime and note-off/sustain policies belong to callers.
#[derive(Clone, Debug)]
pub struct Adsr {
    config: AdsrConfig,
    phase: EnvelopePhase,
    level: f64,
    remaining: u32,
    target: f64,
    step: f64,
}
impl Adsr {
    pub fn new(config: AdsrConfig) -> Self {
        Self {
            config,
            phase: EnvelopePhase::Idle,
            level: 0.,
            remaining: 0,
            target: 0.,
            step: 0.,
        }
    }
    pub fn phase(&self) -> EnvelopePhase {
        self.phase
    }
    pub fn level(&self) -> f64 {
        self.level
    }
    pub fn reset(&mut self) {
        self.phase = EnvelopePhase::Idle;
        self.level = 0.;
        self.remaining = 0;
        self.target = 0.;
        self.step = 0.;
    }
    pub fn note_on(&mut self) {
        self.enter(EnvelopePhase::Attack);
    }
    pub fn note_off(&mut self) {
        if self.phase != EnvelopePhase::Idle {
            self.enter(EnvelopePhase::Release);
        }
    }
    fn enter(&mut self, phase: EnvelopePhase) {
        self.phase = phase;
        let (target, count) = match phase {
            EnvelopePhase::Attack => (1., self.config.attack),
            EnvelopePhase::Decay => (self.config.sustain, self.config.decay),
            EnvelopePhase::Release => (0., self.config.release),
            EnvelopePhase::Sustain => {
                self.level = self.config.sustain;
                self.remaining = 0;
                return;
            }
            EnvelopePhase::Idle => {
                self.reset();
                return;
            }
        };
        self.target = target;
        self.remaining = count;
        if count == 0 {
            self.level = target;
            self.advance_phase();
        } else {
            self.step = (target - self.level) / count as f64;
        }
    }
    fn advance_phase(&mut self) {
        self.enter(match self.phase {
            EnvelopePhase::Attack => EnvelopePhase::Decay,
            EnvelopePhase::Decay => EnvelopePhase::Sustain,
            EnvelopePhase::Release => EnvelopePhase::Idle,
            phase => phase,
        });
    }
    pub fn process_sample(&mut self) -> f64 {
        if self.remaining > 0 {
            self.remaining -= 1;
            if self.remaining == 0 {
                self.level = self.target;
                let output = self.level;
                self.advance_phase();
                return output;
            } else {
                self.level += self.step;
            }
        }
        self.level
    }
    pub fn process(&mut self, output: &mut [f64]) {
        for sample in output {
            *sample = self.process_sample();
        }
    }
}
impl std::fmt::Display for EnvelopeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("invalid ADSR rate, duration or sustain")
    }
}
impl std::error::Error for EnvelopeError {}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn ramps_hit_exact_targets_and_release_from_current_level() {
        let mut adsr = Adsr::new(AdsrConfig::new(100., 0.04, 0.02, 0.5, 0.02).unwrap());
        adsr.note_on();
        let mut output = [0.; 7];
        adsr.process(&mut output);
        assert_eq!(output, [0.25, 0.5, 0.75, 1., 0.75, 0.5, 0.5]);
        adsr.note_off();
        assert_eq!(adsr.process_sample(), 0.25);
        assert_eq!(adsr.process_sample(), 0.);
        assert_eq!(adsr.phase(), EnvelopePhase::Idle);
        adsr.note_on();
        assert_eq!(adsr.process_sample(), 0.25);
        adsr.note_off();
        assert_eq!(adsr.process_sample(), 0.125);
        assert_eq!(adsr.process_sample(), 0.);
    }
    #[test]
    fn partitioning_and_zero_decay_preserve_attack_endpoint() {
        let config = AdsrConfig::new(100., 0.03, 0.02, 0.4, 0.04).unwrap();
        let mut whole = Adsr::new(config);
        let mut split = Adsr::new(config);
        whole.note_on();
        split.note_on();
        let mut expected = [0.; 17];
        let mut actual = [0.; 17];
        whole.process(&mut expected);
        for chunk in actual.chunks_mut(2) {
            split.process(chunk);
        }
        assert_eq!(actual, expected);
        let mut zero_decay = Adsr::new(AdsrConfig::new(100., 0.02, 0., 0.4, 0.).unwrap());
        zero_decay.note_on();
        assert_eq!(zero_decay.process_sample(), 0.5);
        assert_eq!(zero_decay.process_sample(), 1.);
        assert_eq!(zero_decay.process_sample(), 0.4);
        assert!(AdsrConfig::new(1., f64::from(u32::MAX) + 1., 0., 0.5, 0.).is_err());
    }
    #[test]
    fn zero_durations_retrigger_and_reset_have_explicit_policy() {
        let mut immediate = Adsr::new(AdsrConfig::new(48000., 0., 0., 0.75, 0.).unwrap());
        immediate.note_on();
        assert_eq!(immediate.level(), 0.75);
        assert_eq!(immediate.phase(), EnvelopePhase::Sustain);
        immediate.note_off();
        assert_eq!(immediate.phase(), EnvelopePhase::Idle);
        assert_eq!(immediate.level(), 0.);
        let mut adsr = Adsr::new(AdsrConfig::new(100., 0.02, 0.02, 0.5, 0.02).unwrap());
        adsr.note_on();
        assert_eq!(adsr.process_sample(), 0.5);
        adsr.note_on();
        assert_eq!(adsr.process_sample(), 0.75);
        assert_eq!(adsr.process_sample(), 1.);
        adsr.reset();
        assert_eq!(adsr.process_sample(), 0.);
        for args in [
            (0., 0., 0., 0.5, 0.),
            (48000., -1., 0., 0.5, 0.),
            (48000., 0., 0., 1.1, 0.),
            (48000., 0., f64::NAN, 0.5, 0.),
        ] {
            assert!(AdsrConfig::new(args.0, args.1, args.2, args.3, args.4).is_err());
        }
    }
}
