//! Peak detection, soft-knee compression and downward gating; no lookahead.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DynamicsError;
fn coefficient(rate: f64, seconds: f64) -> Result<f64, DynamicsError> {
    if !rate.is_finite()
        || rate <= 0.
        || !seconds.is_finite()
        || !(0.0..=60.).contains(&seconds)
        || !(rate * seconds).is_finite()
    {
        return Err(DynamicsError);
    }
    Ok(if seconds == 0. {
        0.
    } else {
        (-1. / (rate * seconds)).exp()
    })
}
/// Rectified one-pole peak envelope. Time constants are seconds (63.2% settling).
/// NaN/infinite samples are not sanitized and may poison history until reset.
#[derive(Clone, Debug)]
pub struct EnvelopeFollower {
    attack: f64,
    release: f64,
    level: f64,
}
impl EnvelopeFollower {
    pub fn new(rate: f64, attack: f64, release: f64) -> Result<Self, DynamicsError> {
        Ok(Self {
            attack: coefficient(rate, attack)?,
            release: coefficient(rate, release)?,
            level: 0.,
        })
    }
    pub fn level(&self) -> f64 {
        self.level
    }
    pub fn reset(&mut self) {
        self.level = 0.;
    }
    pub fn process_sample(&mut self, sample: f64) -> f64 {
        let target = sample.abs();
        let coefficient = if target > self.level {
            self.attack
        } else {
            self.release
        };
        self.level = target + coefficient * (self.level - target);
        self.level
    }
}
/// Validated soft-knee compressor configuration, prepared on control threads.
#[derive(Clone, Copy, Debug)]
pub struct CompressorConfig {
    threshold: f64,
    slope: f64,
    knee: f64,
    makeup: f64,
    rate: f64,
    attack: f64,
    release: f64,
}
impl CompressorConfig {
    /// Threshold -120..24 dB, ratio 1..100, knee 0..48 dB, makeup -60..60 dB.
    /// Attack/release 0..60 seconds; zero selects an instantaneous detector.
    pub fn new(
        rate: f64,
        threshold_db: f64,
        ratio: f64,
        knee_db: f64,
        attack: f64,
        release: f64,
        makeup_db: f64,
    ) -> Result<Self, DynamicsError> {
        if !threshold_db.is_finite()
            || !(-120.0..=24.).contains(&threshold_db)
            || !ratio.is_finite()
            || !(1.0..=100.).contains(&ratio)
            || !knee_db.is_finite()
            || !(0.0..=48.).contains(&knee_db)
            || !makeup_db.is_finite()
            || !(-60.0..=60.).contains(&makeup_db)
        {
            return Err(DynamicsError);
        }
        coefficient(rate, attack)?;
        coefficient(rate, release)?;
        Ok(Self {
            threshold: threshold_db,
            slope: 1. / ratio - 1.,
            knee: knee_db,
            makeup: 10f64.powf(makeup_db / 20.),
            rate,
            attack,
            release,
        })
    }
    /// Static linear gain for a finite nonnegative peak level.
    pub fn gain_for_level(&self, level: f64) -> Result<f64, DynamicsError> {
        if !level.is_finite() || level < 0. {
            return Err(DynamicsError);
        }
        Ok(self.gain(level))
    }
    fn gain(self, level: f64) -> f64 {
        if level == 0. {
            return self.makeup;
        }
        let above = 20. * level.log10() - self.threshold;
        let reduction = if self.knee == 0. {
            self.slope * above.max(0.)
        } else if above < -self.knee * 0.5 {
            0.
        } else if above > self.knee * 0.5 {
            self.slope * above
        } else {
            self.slope * (above + self.knee * 0.5).powi(2) / (2. * self.knee)
        };
        self.makeup * 10f64.powf(reduction / 20.)
    }
}
/// One-channel peak compressor with explicit sidechain. No lookahead/latency,
/// channel linking, hard brickwall limit or automatic makeup. Processing uses
/// per-sample log/pow and allocates nothing; nonfinite audio is not sanitized.
#[derive(Clone, Debug)]
pub struct Compressor {
    config: CompressorConfig,
    detector: EnvelopeFollower,
}
impl Compressor {
    pub fn new(config: CompressorConfig) -> Self {
        Self {
            detector: EnvelopeFollower::new(config.rate, config.attack, config.release)
                .expect("validated config"),
            config,
        }
    }
    pub fn reset(&mut self) {
        self.detector.reset();
    }
    pub fn detector_level(&self) -> f64 {
        self.detector.level()
    }
    pub fn process_sample(&mut self, input: f64, sidechain: f64) -> f64 {
        let level = self.detector.process_sample(sidechain);
        input * self.config.gain(level)
    }
}

/// Prepared downward-expansion gate configuration with a finite attenuation floor.
/// The ratio multiplies input dB below threshold; one leaves the signal unchanged.
#[derive(Clone, Copy, Debug)]
pub struct NoiseGateConfig {
    threshold: f64,
    slope: f64,
    knee: f64,
    floor: f64,
    rate: f64,
    attack: f64,
    release: f64,
}
impl NoiseGateConfig {
    /// Threshold -120..24 dB, expansion ratio 1..100, knee 0..48 dB,
    /// attenuation floor -120..0 dB and detector times 0..60 seconds.
    /// A zero detector time is instantaneous; the floor does not hard-mute audio.
    pub fn new(
        rate: f64,
        threshold_db: f64,
        ratio: f64,
        knee_db: f64,
        attack: f64,
        release: f64,
        floor_db: f64,
    ) -> Result<Self, DynamicsError> {
        if !threshold_db.is_finite()
            || !(-120.0..=24.).contains(&threshold_db)
            || !ratio.is_finite()
            || !(1.0..=100.).contains(&ratio)
            || !knee_db.is_finite()
            || !(0.0..=48.).contains(&knee_db)
            || !floor_db.is_finite()
            || !(-120.0..=0.).contains(&floor_db)
        {
            return Err(DynamicsError);
        }
        coefficient(rate, attack)?;
        coefficient(rate, release)?;
        Ok(Self {
            threshold: threshold_db,
            slope: ratio - 1.,
            knee: knee_db,
            floor: floor_db,
            rate,
            attack,
            release,
        })
    }
    /// Static linear gain for a finite nonnegative detector level.
    /// Silence reaches the attenuation floor, except ratio one remains unity.
    pub fn gain_for_level(&self, level: f64) -> Result<f64, DynamicsError> {
        if !level.is_finite() || level < 0. {
            return Err(DynamicsError);
        }
        Ok(self.gain(level))
    }
    fn gain(self, level: f64) -> f64 {
        if self.slope == 0. {
            return 1.;
        }
        let reduction = if level == 0. {
            self.floor
        } else {
            let below = 20. * level.log10() - self.threshold;
            if self.knee == 0. {
                self.slope * below.min(0.)
            } else if below <= -self.knee * 0.5 {
                self.slope * below
            } else if below >= self.knee * 0.5 {
                0.
            } else {
                -self.slope * (below - self.knee * 0.5).powi(2) / (2. * self.knee)
            }
        };
        10f64.powf(reduction.max(self.floor) / 20.)
    }
}
/// One-channel finite-floor noise gate using an explicit rectified sidechain.
/// Attack/release smooth the detector, rather than a separate gain envelope.
/// No lookahead, hold, hysteresis, channel linking or hard mute. Processing/reset
/// allocate nothing; per-sample log/pow and nonfinite audio are not sanitized.
#[derive(Clone, Debug)]
pub struct NoiseGate {
    config: NoiseGateConfig,
    detector: EnvelopeFollower,
}
impl NoiseGate {
    pub fn new(config: NoiseGateConfig) -> Self {
        Self {
            detector: EnvelopeFollower::new(config.rate, config.attack, config.release)
                .expect("validated config"),
            config,
        }
    }
    /// Clear detector history; configuration remains unchanged.
    pub fn reset(&mut self) {
        self.detector.reset();
    }
    pub fn detector_level(&self) -> f64 {
        self.detector.level()
    }
    pub fn process_sample(&mut self, input: f64, sidechain: f64) -> f64 {
        let level = self.detector.process_sample(sidechain);
        input * self.config.gain(level)
    }
}

/// Prepared one-channel sample-peak limiter, with instantaneous attack and a
/// rectified peak detector's exponential release. No lookahead or true-peak/
/// intersample guarantee; independent channels are not linked. Per-sample/reset
/// paths allocate nothing. Nonfinite audio is not validated: NaN propagates;
/// infinity passes the final clamp, and either can poison detector history until
/// reset. Hard ceiling clipping protects final rounding.
#[derive(Clone, Debug)]
pub struct PeakLimiter {
    ceiling: f64,
    detector: EnvelopeFollower,
    gain: f64,
}
impl PeakLimiter {
    /// Ceiling -120..24 dBFS, finite/positive rate, release 0..60 seconds.
    /// Zero release gives memoryless limiting; configuration is prepared here.
    pub fn new(rate: f64, ceiling_db: f64, release: f64) -> Result<Self, DynamicsError> {
        if !ceiling_db.is_finite() || !(-120.0..=24.0).contains(&ceiling_db) {
            return Err(DynamicsError);
        }
        Ok(Self {
            ceiling: 10f64.powf(ceiling_db / 20.),
            detector: EnvelopeFollower::new(rate, 0., release)?,
            gain: 1.,
        })
    }
    /// Linear amplitude ceiling, not an intersample/true-peak bound.
    pub fn ceiling(&self) -> f64 {
        self.ceiling
    }
    /// Last applied detector gain before the final rounding clamp.
    pub fn gain(&self) -> f64 {
        self.gain
    }
    pub fn detector_level(&self) -> f64 {
        self.detector.level()
    }
    /// Clear detector history and restore unity gain; ceiling is retained.
    pub fn reset(&mut self) {
        self.detector.reset();
        self.gain = 1.;
    }
    pub fn process_sample(&mut self, input: f64) -> f64 {
        let level = self.detector.process_sample(input);
        self.gain = if level > self.ceiling {
            self.ceiling / level
        } else {
            1.
        };
        (input * self.gain).clamp(-self.ceiling, self.ceiling)
    }
    pub fn process_in_place(&mut self, samples: &mut [f64]) {
        for sample in samples {
            *sample = self.process_sample(*sample);
        }
    }
}
