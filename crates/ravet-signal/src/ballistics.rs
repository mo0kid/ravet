//! Prepared peak/RMS ballistics with explicit native precision and channel ownership.
use crate::DynamicsError;

/// Peak smooths rectified amplitude; RMS smooths energy and returns amplitude.
/// Both are exponential attack/release followers, not finite-window statistics.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BallisticsMode {
    Peak,
    Rms,
}

/// Validated seconds-based timing, prepared on CONTROL without allocation.
/// Finite rate 1..768000 Hz and times 0..60 seconds. Zero explicitly selects
/// instantaneous response. Nonzero times whose alpha or actual complement rounds
/// to an endpoint reject; RMS also checks its rounded zero-input release factor.
/// Ideal time constants describe amplitude for Peak and energy for RMS. Actual
/// native floating-point recurrence/rounding need not settle at ideal times.
///
/// ```
/// use ravet_signal::{BallisticsConfig, BallisticsFilter, BallisticsMode};
/// let config = BallisticsConfig::new(48000., 0.01, 0.1, BallisticsMode::Rms).unwrap();
/// let mut detector = BallisticsFilter::new(config);
/// let level = detector.process_sample(-1.);
/// assert!((level - config.attack_alpha().sqrt()).abs() < 1e-15);
/// assert!(config.quantize_f32().is_ok());
/// ```
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BallisticsConfig {
    rate: f64,
    attack: f64,
    release: f64,
    mode: BallisticsMode,
    attack_alpha: f64,
    release_alpha: f64,
}
fn alpha(rate: f64, time: f64, mode: BallisticsMode) -> Result<f64, DynamicsError> {
    if !time.is_finite() || !(0.0..=60.).contains(&time) {
        return Err(DynamicsError);
    }
    if time == 0. {
        return Ok(1.);
    }
    let a = -(-1. / (rate * time)).exp_m1();
    let c = 1. - a;
    if !a.is_finite()
        || !(0. < a && a < 1. && 0. < c && c < 1.)
        || (mode == BallisticsMode::Rms && c.sqrt() == 1.)
    {
        return Err(DynamicsError);
    }
    Ok(a)
}
impl BallisticsConfig {
    pub fn new(
        rate: f64,
        attack_seconds: f64,
        release_seconds: f64,
        mode: BallisticsMode,
    ) -> Result<Self, DynamicsError> {
        if !rate.is_finite() || !(1.0..=768000.).contains(&rate) {
            return Err(DynamicsError);
        }
        Ok(Self {
            rate,
            attack: attack_seconds,
            release: release_seconds,
            mode,
            attack_alpha: alpha(rate, attack_seconds, mode)?,
            release_alpha: alpha(rate, release_seconds, mode)?,
        })
    }
    pub fn sample_rate(&self) -> f64 {
        self.rate
    }
    pub fn attack_seconds(&self) -> f64 {
        self.attack
    }
    pub fn release_seconds(&self) -> f64 {
        self.release
    }
    pub fn mode(&self) -> BallisticsMode {
        self.mode
    }
    pub fn attack_alpha(&self) -> f64 {
        self.attack_alpha
    }
    pub fn release_alpha(&self) -> f64 {
        self.release_alpha
    }
    pub fn attack_complement(&self) -> f64 {
        1. - self.attack_alpha
    }
    pub fn release_complement(&self) -> f64 {
        1. - self.release_alpha
    }
    /// Explicit f32 coefficient conversion and endpoint validation. Actual native
    /// recurrence can differ from ideal timing or the originating f64 profile.
    pub fn quantize_f32(&self) -> Result<BallisticsConfig32, DynamicsError> {
        let attack = self.attack_alpha as f32;
        let release = self.release_alpha as f32;
        for (a, time) in [(attack, self.attack), (release, self.release)] {
            let c = 1. - a;
            if time != 0.
                && (!(0. < a && a < 1. && 0. < c && c < 1.)
                    || (self.mode == BallisticsMode::Rms && c.sqrt() == 1.))
            {
                return Err(DynamicsError);
            }
        }
        Ok(BallisticsConfig32 {
            prototype: *self,
            attack_alpha: attack,
            release_alpha: release,
        })
    }
}

/// Explicit native f32 coefficients, validated separately from their f64 origin.
/// Time/rate metadata names the requested physical parameters, not a promise that
/// every native rounding profile matches ideal continuous settling exactly.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BallisticsConfig32 {
    prototype: BallisticsConfig,
    attack_alpha: f32,
    release_alpha: f32,
}
impl BallisticsConfig32 {
    pub fn sample_rate(&self) -> f64 {
        self.prototype.rate
    }
    pub fn attack_seconds(&self) -> f64 {
        self.prototype.attack
    }
    pub fn release_seconds(&self) -> f64 {
        self.prototype.release
    }
    pub fn mode(&self) -> BallisticsMode {
        self.prototype.mode
    }
    pub fn attack_alpha(&self) -> f32 {
        self.attack_alpha
    }
    pub fn release_alpha(&self) -> f32 {
        self.release_alpha
    }
    pub fn attack_complement(&self) -> f32 {
        1. - self.attack_alpha
    }
    pub fn release_complement(&self) -> f32 {
        1. - self.release_alpha
    }
}

macro_rules! native {
    ($sample:ty, $config:ty, $filter:ident, $bank:ident, $step:ident) => {
        fn $step(level: $sample, input: $sample, config: &$config) -> $sample {
            // Explicit sticky propagation, independent of native min/max NaN rules.
            if input.is_nan() || level.is_nan() {
                return <$sample>::NAN;
            }
            let target = input.abs();
            if target.is_infinite() || level.is_infinite() {
                return <$sample>::INFINITY;
            }
            if target == level {
                return level;
            }
            let a = if target > level {
                config.attack_alpha()
            } else {
                config.release_alpha()
            };
            if a == 1. {
                return target;
            }
            if config.mode() == BallisticsMode::Peak {
                return level + a * (target - level);
            }
            // Scaled energy recurrence: no full-magnitude square, zero division,
            // precomputed sqrt-weight mass drift, projection or coefficient clamp.
            let hi = level.max(target);
            if hi == 0. {
                return 0.;
            }
            let lo = level.min(target) / hi;
            let v = lo * lo;
            let energy = if target > level {
                v + a * (1. - v)
            } else {
                1. + a * (v - 1.)
            };
            hi * energy.sqrt()
        }

        /// Fixed one-channel amplitude state, native Peak or scaled-energy RMS.
        /// Reset/config replacement is explicit; no window, linking or lookahead.
        /// NaN and infinity poison history until reset (NaN stays NaN; infinity
        /// stays positive infinity). No sanitization or block-boundary snapping.
        /// Native ratio-square/release underflow and rounding remain observable.
        #[derive(Clone, Debug)]
        pub struct $filter {
            config: $config,
            level: $sample,
        }
        impl $filter {
            pub fn new(config: $config) -> Self {
                Self { config, level: 0. }
            }
            pub fn config(&self) -> &$config {
                &self.config
            }
            pub fn level(&self) -> $sample {
                self.level
            }
            pub fn reset(&mut self) {
                self.level = 0.;
            }
            /// Finite nonnegative initial amplitude, in both modes. Invalid values
            /// preserve history. No squaring or silent configuration conversion.
            pub fn reset_to(&mut self, level: $sample) -> Result<(), DynamicsError> {
                if !level.is_finite() || level < 0. {
                    return Err(DynamicsError);
                }
                self.level = if level == 0. { 0. } else { level };
                Ok(())
            }
            pub fn replace_config(&mut self, config: $config) {
                self.config = config;
                self.reset();
            }
            /// Caller-explicit amplitude denormal retirement. NaN/infinity and
            /// normal states are retained; never invoked implicitly in a block.
            pub fn snap_to_zero(&mut self) {
                if self.level.is_subnormal() {
                    self.level = 0.;
                }
            }
            pub fn process_sample(&mut self, input: $sample) -> $sample {
                self.level = $step(self.level, input, &self.config);
                self.level
            }
            pub fn process_in_place(&mut self, samples: &mut [$sample]) {
                for x in samples {
                    *x = self.process_sample(*x);
                }
            }
            /// All lengths preflight before output/history mutation; empty no-op.
            pub fn process_block(
                &mut self,
                input: &[$sample],
                output: &mut [$sample],
            ) -> Result<(), DynamicsError> {
                if input.len() != output.len() {
                    return Err(DynamicsError);
                }
                for (x, y) in input.iter().zip(output.iter_mut()) {
                    *y = self.process_sample(*x);
                }
                Ok(())
            }
        }

        /// Prepared independent channel histories in fixed storage. Channels 1..64
        /// and maximum block 1..65536; no heap in construction/process/reset.
        /// Planar calls preflight EVERY channel/prefix before any history/output;
        /// caller suffixes untouched, no implicit per-block denormal snapping.
        /// Shared immutable timing does not link channels or their amplitude state.
        #[derive(Clone, Debug)]
        pub struct $bank {
            config: $config,
            channels: usize,
            max_block: usize,
            levels: [$sample; 64],
        }
        impl $bank {
            pub fn new(
                config: $config,
                channels: usize,
                max_block: usize,
            ) -> Result<Self, DynamicsError> {
                if !(1..=64).contains(&channels) || !(1..=65536).contains(&max_block) {
                    return Err(DynamicsError);
                }
                Ok(Self {
                    config,
                    channels,
                    max_block,
                    levels: [0.; 64],
                })
            }
            pub fn config(&self) -> &$config {
                &self.config
            }
            pub fn channels(&self) -> usize {
                self.channels
            }
            pub fn maximum_block_frames(&self) -> usize {
                self.max_block
            }
            pub fn level(&self, channel: usize) -> Result<$sample, DynamicsError> {
                if channel >= self.channels {
                    return Err(DynamicsError);
                }
                Ok(self.levels[channel])
            }
            pub fn reset(&mut self) {
                self.levels.fill(0.);
            }
            pub fn reset_to(&mut self, level: $sample) -> Result<(), DynamicsError> {
                if !level.is_finite() || level < 0. {
                    return Err(DynamicsError);
                }
                self.levels.fill(if level == 0. { 0. } else { level });
                Ok(())
            }
            pub fn replace_config(&mut self, config: $config) {
                self.config = config;
                self.reset();
            }
            pub fn snap_to_zero(&mut self) {
                for level in &mut self.levels[..self.channels] {
                    if level.is_subnormal() {
                        *level = 0.;
                    }
                }
            }
            pub fn process_sample(
                &mut self,
                channel: usize,
                input: $sample,
            ) -> Result<$sample, DynamicsError> {
                if channel >= self.channels {
                    return Err(DynamicsError);
                }
                self.levels[channel] = $step(self.levels[channel], input, &self.config);
                Ok(self.levels[channel])
            }
            pub fn process_planar(
                &mut self,
                input: &[&[$sample]],
                output: &mut [&mut [$sample]],
                frames: usize,
            ) -> Result<(), DynamicsError> {
                if input.len() != self.channels
                    || output.len() != self.channels
                    || frames > self.max_block
                    || input.iter().any(|x| x.len() < frames)
                    || output.iter().any(|x| x.len() < frames)
                {
                    return Err(DynamicsError);
                }
                for (channel, (input, output)) in input.iter().zip(output.iter_mut()).enumerate() {
                    for (x, y) in input[..frames].iter().zip(output[..frames].iter_mut()) {
                        self.levels[channel] = $step(self.levels[channel], *x, &self.config);
                        *y = self.levels[channel];
                    }
                }
                Ok(())
            }
            pub fn process_planar_in_place(
                &mut self,
                audio: &mut [&mut [$sample]],
                frames: usize,
            ) -> Result<(), DynamicsError> {
                if audio.len() != self.channels
                    || frames > self.max_block
                    || audio.iter().any(|x| x.len() < frames)
                {
                    return Err(DynamicsError);
                }
                for (channel, samples) in audio.iter_mut().enumerate() {
                    for x in &mut samples[..frames] {
                        self.levels[channel] = $step(self.levels[channel], *x, &self.config);
                        *x = self.levels[channel];
                    }
                }
                Ok(())
            }
        }
    };
}
native!(
    f64,
    BallisticsConfig,
    BallisticsFilter,
    BallisticsBank,
    step64
);
native!(
    f32,
    BallisticsConfig32,
    BallisticsFilter32,
    BallisticsBank32,
    step32
);
