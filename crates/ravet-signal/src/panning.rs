//! Prepared panning curves and frame-count gain interpolation for an explicit pair.

/// Curve before normalization. Exponents describe the mathematical law rather
/// than a rounded nominal decibel label. All curves have unity endpoints.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PanningLaw {
    Linear,
    Balanced,
    Sine,
    SineThreeHalves,
    SineSquared,
    SquareRoot,
    ThreeQuarterPower,
}
impl PanningLaw {
    /// Ideal coefficient of each channel at pan zero, before normalization.
    pub fn center_gain(self) -> f64 {
        match self {
            Self::Linear | Self::SineSquared => 0.5,
            Self::Balanced => 1.,
            Self::Sine | Self::SquareRoot => std::f64::consts::FRAC_1_SQRT_2,
            Self::SineThreeHalves | Self::ThreeQuarterPower => 2_f64.powf(-0.75),
        }
    }
}
/// Explicit level policy. UnityCenter boosts endpoint levels for every law
/// except Balanced; it is not arbitrary-correlation loudness compensation.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PanningNormalization {
    UnityEndpoints,
    UnityCenter,
}
/// Configuration/preflight failure. Blocks reject before any gain progress or
/// output change. Nonfinite audio itself is multiplied without sanitization.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PanningError {
    InvalidPan,
    InvalidMaximumFrames,
    ChannelCount,
    TooManyFrames,
    BufferTooShort,
}
impl std::fmt::Display for PanningError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::InvalidPan => "pan must be finite and within -1..1",
            Self::InvalidMaximumFrames => "maximum frames must be within 1..65536",
            Self::ChannelCount => "panning requires exactly two output/stereo input channels",
            Self::TooManyFrames => "frame prefix exceeds prepared maximum",
            Self::BufferTooShort => "channel is shorter than active frame prefix",
        })
    }
}
impl std::error::Error for PanningError {}

/// Immutable CONTROL-prepared actual f64 coefficients. Pan is -1..1. The seven
/// curves are linear, balanced, sine, sine^1.5, sine², square-root and power^.75.
/// UnityCenter scales by the rounded reciprocal of the ideal center gain, making
/// both center channels
/// unity and boosting endpoints up to 2. No clipping, input scan or limiter.
/// Endpoints/center are explicit; mirrored pans swap identical actual gains.
/// Trigonometry and powers occur here, never in per-frame processing.
///
/// ```
/// use ravet_signal::{PanningGains, PanningLaw, PanningNormalization, StereoPanner};
/// let left = PanningGains::new(-1., PanningLaw::Sine,
///     PanningNormalization::UnityEndpoints).unwrap();
/// let right = PanningGains::new(1., PanningLaw::Sine,
///     PanningNormalization::UnityEndpoints).unwrap();
/// let mut panner = StereoPanner::new(left, 2, 64).unwrap();
/// panner.set_target(right);
/// assert_eq!(panner.process_mono_sample(1.), [0.5, 0.5]);
/// assert_eq!(panner.process_mono_sample(1.), [0., 1.]);
/// ```
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PanningGains {
    pan: f64,
    law: PanningLaw,
    normalization: PanningNormalization,
    gains: [f64; 2],
}
impl PanningGains {
    pub fn new(
        pan: f64,
        law: PanningLaw,
        normalization: PanningNormalization,
    ) -> Result<Self, PanningError> {
        if !pan.is_finite() || !(-1.0..=1.).contains(&pan) {
            return Err(PanningError::InvalidPan);
        }
        let mut gains = prepare_far((1. - pan.abs()) * 0.5, pan == 0., law, normalization);
        if pan > 0. {
            gains.swap(0, 1);
        }
        Ok(Self {
            pan,
            law,
            normalization,
            gains,
        })
    }
    pub fn pan(&self) -> f64 {
        self.pan
    }
    pub fn law(&self) -> PanningLaw {
        self.law
    }
    pub fn normalization(&self) -> PanningNormalization {
        self.normalization
    }
    pub fn gains(&self) -> [f64; 2] {
        self.gains
    }
    pub fn left(&self) -> f64 {
        self.gains[0]
    }
    pub fn right(&self) -> f64 {
        self.gains[1]
    }
    /// Theoretical endpoint maximum, not a floating execution error certificate.
    pub fn endpoint_gain(&self) -> f64 {
        if self.normalization == PanningNormalization::UnityEndpoints {
            1.
        } else {
            1. / self.law.center_gain()
        }
    }
    /// Explicit coefficient rounding. Requested pan/curve metadata is retained;
    /// actual native f32 gains need not satisfy an ideal power equality exactly.
    pub fn quantize_f32(&self) -> PanningGains32 {
        PanningGains32 {
            prototype: *self,
            gains: self.gains.map(|v| v as f32),
        }
    }
}
fn prepare_far(
    far: f64,
    is_center: bool,
    law: PanningLaw,
    normalization: PanningNormalization,
) -> [f64; 2] {
    let center = law.center_gain();
    let endpoint = match normalization {
        PanningNormalization::UnityEndpoints => 1.,
        PanningNormalization::UnityCenter => 1. / center,
    };
    if is_center {
        [if normalization == PanningNormalization::UnityCenter {
            1.
        } else {
            center
        }; 2]
    } else if far == 0. {
        [endpoint, 0.]
    } else {
        let near = 1. - far;
        let pair = match law {
            PanningLaw::Linear => [near, far],
            PanningLaw::Balanced => [1., 2. * far],
            PanningLaw::Sine | PanningLaw::SineThreeHalves | PanningLaw::SineSquared => {
                let angle = std::f64::consts::FRAC_PI_2 * far;
                let pair = [angle.cos(), angle.sin()];
                match law {
                    PanningLaw::SineThreeHalves => pair.map(|v| v.powf(1.5)),
                    PanningLaw::SineSquared => pair.map(|v| v * v),
                    _ => pair,
                }
            }
            PanningLaw::SquareRoot => [near.sqrt(), far.sqrt()],
            PanningLaw::ThreeQuarterPower => [near.powf(0.75), far.powf(0.75)],
        };
        pair.map(|v| v * endpoint)
    }
}
impl PanningGains {
    // Internal validated unit-coordinate preparation for dry/wet. Direct small
    // wet values do not disappear through the rounded affine pan conversion.
    // Stored affine pan metadata is lossy and must remain private: do not expose
    // or reconstruct these wet-coordinate gains through that pan value.
    pub(crate) fn from_unit(
        unit: f64,
        law: PanningLaw,
        normalization: PanningNormalization,
    ) -> Self {
        let mut gains = prepare_far(
            if unit > 0.5 { 1. - unit } else { unit },
            unit == 0.5,
            law,
            normalization,
        );
        if unit > 0.5 {
            gains.swap(0, 1);
        }
        Self {
            pan: 2. * unit - 1.,
            law,
            normalization,
            gains,
        }
    }
}
/// Explicitly rounded f32 coefficients with original requested curve metadata.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PanningGains32 {
    prototype: PanningGains,
    gains: [f32; 2],
}
impl PanningGains32 {
    pub fn pan(&self) -> f64 {
        self.prototype.pan()
    }
    pub fn law(&self) -> PanningLaw {
        self.prototype.law()
    }
    pub fn normalization(&self) -> PanningNormalization {
        self.prototype.normalization()
    }
    pub fn gains(&self) -> [f32; 2] {
        self.gains
    }
    pub fn left(&self) -> f32 {
        self.gains[0]
    }
    pub fn right(&self) -> f32 {
        self.gains[1]
    }
    /// Ideal endpoint metadata in f64; [`Self::gains`] are actual f32 coefficients.
    pub fn endpoint_gain(&self) -> f64 {
        self.prototype.endpoint_gain()
    }
}
macro_rules! panner {
    ($name:ident,$sample:ty,$profile:ty) => {
        /// Prepared mono-to-stereo or stereo balance, without crossfeed. Controls
        /// contain two immutable actual gains; one frame advances their ramps once.
        /// Both precisions interpolate gain endpoints in f64; coefficient/sample
        /// multiplication uses the named native precision. No allocation/lock/I/O.
        ///
        /// First frame advances 1/N, Nth assigns the exact target; zero duration is
        /// immediate. Retargeting starts from current rounded gains. An identical
        /// actual target pair retains ramp progress, even when metadata changes.
        /// Intermediate pairs may leave the static law or change summed power.
        /// Long ramps may repeat rounded coefficients. No hidden time conversion,
        /// smoothing of pan itself, clipping or correlation/loudness guarantee.
        /// NaN/Inf and boosted finite overflow propagate naturally, including 0*Inf.
        /// Prefix APIs require exactly two output/stereo-input channels and preflight
        /// all lengths/maxima before mutation; backing suffixes remain untouched.
        #[derive(Clone, Copy, Debug)]
        pub struct $name {
            target: $profile,
            current: [$sample; 2],
            start: [f64; 2],
            duration: u32,
            elapsed: u32,
            maximum: usize,
        }
        impl $name {
            pub fn new(
                target: $profile,
                ramp_frames: u32,
                maximum_frames: usize,
            ) -> Result<Self, PanningError> {
                if !(1..=65536).contains(&maximum_frames) {
                    return Err(PanningError::InvalidMaximumFrames);
                }
                let current = target.gains();
                Ok(Self {
                    target,
                    current,
                    start: current.map(|v| v as f64),
                    duration: ramp_frames,
                    elapsed: ramp_frames,
                    maximum: maximum_frames,
                })
            }
            pub fn current_gains(&self) -> [$sample; 2] {
                self.current
            }
            pub fn target(&self) -> $profile {
                self.target
            }
            pub fn remaining_frames(&self) -> u32 {
                self.duration - self.elapsed
            }
            pub fn ramp_frames(&self) -> u32 {
                self.duration
            }
            pub fn maximum_frames(&self) -> usize {
                self.maximum
            }
            pub fn set_target(&mut self, target: $profile) {
                let changed = self.target.gains() != target.gains();
                self.target = target;
                if changed {
                    self.start = self.current.map(|v| v as f64);
                    self.elapsed = 0;
                    if self.duration == 0 {
                        self.current = target.gains();
                    }
                }
            }
            /// Snap both current gains to the latest target; keep configured duration.
            pub fn reset(&mut self) {
                self.current = self.target.gains();
                self.start = self.current.map(|v| v as f64);
                self.elapsed = self.duration;
            }
            fn advance(&mut self) -> [$sample; 2] {
                if self.elapsed < self.duration {
                    self.elapsed += 1;
                    self.current = if self.elapsed == self.duration {
                        self.target.gains()
                    } else {
                        let t = f64::from(self.elapsed) / f64::from(self.duration);
                        let target = self.target.gains();
                        std::array::from_fn(|ch| {
                            (self.start[ch] + (target[ch] as f64 - self.start[ch]) * t) as $sample
                        })
                    };
                }
                self.current
            }
            pub fn process_stereo_sample(&mut self, sample: [$sample; 2]) -> [$sample; 2] {
                let gains = self.advance();
                [sample[0] * gains[0], sample[1] * gains[1]]
            }
            pub fn process_mono_sample(&mut self, sample: $sample) -> [$sample; 2] {
                self.process_stereo_sample([sample; 2])
            }
            fn check(
                &self,
                lengths: impl Iterator<Item = usize>,
                frames: usize,
            ) -> Result<(), PanningError> {
                if frames > self.maximum {
                    return Err(PanningError::TooManyFrames);
                }
                if lengths.into_iter().any(|n| n < frames) {
                    return Err(PanningError::BufferTooShort);
                }
                Ok(())
            }
            pub fn process_stereo(
                &mut self,
                input: &[&[$sample]],
                output: &mut [&mut [$sample]],
                frames: usize,
            ) -> Result<(), PanningError> {
                if input.len() != 2 || output.len() != 2 {
                    return Err(PanningError::ChannelCount);
                }
                self.check(
                    input
                        .iter()
                        .map(|c| c.len())
                        .chain(output.iter().map(|c| c.len())),
                    frames,
                )?;
                for i in 0..frames {
                    let y = self.process_stereo_sample([input[0][i], input[1][i]]);
                    output[0][i] = y[0];
                    output[1][i] = y[1];
                }
                Ok(())
            }
            pub fn process_mono(
                &mut self,
                input: &[$sample],
                output: &mut [&mut [$sample]],
                frames: usize,
            ) -> Result<(), PanningError> {
                if output.len() != 2 {
                    return Err(PanningError::ChannelCount);
                }
                self.check(
                    std::iter::once(input.len()).chain(output.iter().map(|c| c.len())),
                    frames,
                )?;
                for (i, &x) in input.iter().take(frames).enumerate() {
                    let y = self.process_mono_sample(x);
                    output[0][i] = y[0];
                    output[1][i] = y[1];
                }
                Ok(())
            }
            pub fn process_stereo_in_place(
                &mut self,
                channels: &mut [&mut [$sample]],
                frames: usize,
            ) -> Result<(), PanningError> {
                if channels.len() != 2 {
                    return Err(PanningError::ChannelCount);
                }
                self.check(channels.iter().map(|c| c.len()), frames)?;
                for i in 0..frames {
                    let y = self.process_stereo_sample([channels[0][i], channels[1][i]]);
                    channels[0][i] = y[0];
                    channels[1][i] = y[1];
                }
                Ok(())
            }
        }
    };
}
panner!(StereoPanner, f64, PanningGains);
panner!(StereoPanner32, f32, PanningGains32);
