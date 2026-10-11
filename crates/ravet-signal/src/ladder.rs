//! Four-stage trapezoidal ladder with a single feedback-summing tanh saturator.
//!
//! ```
//! use ravet_signal::{LadderDesign, LadderFilter, LadderMode};
//! let d=LadderDesign::new(48000.,1200.,0.7,2.).unwrap();
//! let mut filter=LadderFilter::new(d);
//! let y=filter.process_sample(0.1);
//! assert!(y.select(LadderMode::LowPass24).is_finite());
//! assert!(filter.last_solve().iterations<=64);
//! ```
/// Six modal combinations of one shared four-stage history. Lower-order modes
/// retain four-stage feedback poles. Bandpass12/24 name total modal order, not
/// per-side slopes or normalized peaks.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LadderMode {
    LowPass12,
    LowPass24,
    HighPass12,
    HighPass24,
    BandPass12,
    BandPass24,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LadderError {
    SampleRate,
    Cutoff,
    Resonance,
    Drive,
    NativePrecision,
    ChannelCount,
    MaximumFrames,
    TooManyFrames,
    BufferLength,
}
impl std::fmt::Display for LadderError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::SampleRate => "ladder sample rate must be finite 8000..768000 Hz",
            Self::Cutoff => {
                "ladder original and normalized cutoff must be representable 1e-5..0.45 of rate"
            }
            Self::Resonance => "ladder resonance must be finite 0..1",
            Self::Drive => "ladder input drive must be finite 1..32",
            Self::NativePrecision => "ladder native coefficients unavailable",
            Self::ChannelCount => "ladder channel count must match prepared 1..64 layout",
            Self::MaximumFrames => "ladder maximum frames must be 1..65536",
            Self::TooManyFrames => "ladder block exceeds prepared maximum",
            Self::BufferLength => "ladder buffer length or active prefix differs",
        })
    }
}
impl std::error::Error for LadderError {}
/// Immutable requested physical controls and actual f64 coefficients. Finite
/// rate 8000..768000, cutoff/rate 1e-5..0.45 (both original Hz and divided ratio),
/// resonance 0..1 maps k=4*r, drive 1..32 multiplies input before saturation.
/// No hidden gain compensation, smoothing, oversampling or distributed-stage
/// transistor/OTA model. k = 4 is the marginal small-signal endpoint, not strict decay.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LadderDesign {
    rate: f64,
    cutoff: f64,
    resonance: f64,
    drive: f64,
    gain: f64,
    feedback: f64,
}
impl LadderDesign {
    pub fn new(rate: f64, cutoff: f64, resonance: f64, drive: f64) -> Result<Self, LadderError> {
        if !rate.is_finite() || !(8000.0..=768000.).contains(&rate) {
            return Err(LadderError::SampleRate);
        }
        if !cutoff.is_finite()
            || cutoff < rate * 1e-5
            || cutoff > rate * 0.45
            || !(1e-5..=0.45).contains(&(cutoff / rate))
        {
            return Err(LadderError::Cutoff);
        }
        if !resonance.is_finite() || !(0.0..=1.).contains(&resonance) {
            return Err(LadderError::Resonance);
        }
        if !drive.is_finite() || !(1.0..=32.).contains(&drive) {
            return Err(LadderError::Drive);
        }
        let g = (std::f64::consts::PI * (cutoff / rate)).tan();
        let gain = g / (1. + g);
        let feedback = 4. * resonance;
        if !valid_coefficients(gain, 1. - gain, 1. - 2. * gain, feedback, drive) {
            return Err(LadderError::NativePrecision);
        }
        Ok(Self {
            rate,
            cutoff,
            resonance,
            drive,
            gain,
            feedback,
        })
    }
    pub fn sample_rate(self) -> f64 {
        self.rate
    }
    pub fn cutoff_hz(self) -> f64 {
        self.cutoff
    }
    pub fn resonance(self) -> f64 {
        self.resonance
    }
    pub fn input_drive(self) -> f64 {
        self.drive
    }
    pub fn gain(self) -> f64 {
        self.gain
    }
    pub fn feedback(self) -> f64 {
        self.feedback
    }
    pub fn pole(self) -> f64 {
        1. - 2. * self.gain
    }
    pub fn quantize_f32(self) -> Result<LadderDesign32, LadderError> {
        let gain = self.gain as f32;
        let feedback = self.feedback as f32;
        let drive = self.drive as f32;
        if !valid_coefficients(
            gain as f64,
            (1. - gain) as f64,
            (1. - 2. * gain) as f64,
            feedback as f64,
            drive as f64,
        ) || (self.resonance > 0. && feedback == 0.)
        {
            return Err(LadderError::NativePrecision);
        }
        Ok(LadderDesign32 {
            prototype: self,
            gain,
            feedback,
            drive,
        })
    }
}
fn valid_coefficients(g: f64, c: f64, p: f64, k: f64, d: f64) -> bool {
    g.is_finite()
        && g > 0.
        && g < 1.
        && c > 0.
        && c < 1.
        && p.abs() < 1.
        && k.is_finite()
        && (0.0..=4.).contains(&k)
        && d.is_finite()
        && (1.0..=32.).contains(&d)
}
/// Explicit native32 coefficient rounding; original requested controls remain
/// available through prototype(). Actual gain/complement/pole and nonzero k
/// representability qualify before construction, without nudge or clamp.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LadderDesign32 {
    prototype: LadderDesign,
    gain: f32,
    feedback: f32,
    drive: f32,
}
impl LadderDesign32 {
    pub fn prototype(self) -> LadderDesign {
        self.prototype
    }
    pub fn gain(self) -> f32 {
        self.gain
    }
    pub fn feedback(self) -> f32 {
        self.feedback
    }
    pub fn input_drive(self) -> f32 {
        self.drive
    }
    pub fn pole(self) -> f32 {
        1. - 2. * self.gain
    }
}
macro_rules! native {
    ($sample:ty,$design:ty,$outputs:ident,$info:ident,$filter:ident,$bank:ident,$budget:expr) => {
        /// Simultaneous six modal outputs from one capacitor update. Under explicit
        /// bypass every named output is dry source and history/last solve remain frozen.
        #[derive(Clone, Copy, Debug, PartialEq)]
        pub struct $outputs {
            pub low_pass12: $sample,
            pub low_pass24: $sample,
            pub high_pass12: $sample,
            pub high_pass24: $sample,
            pub band_pass12: $sample,
            pub band_pass24: $sample,
        }
        impl $outputs {
            pub fn select(self, mode: LadderMode) -> $sample {
                match mode {
                    LadderMode::LowPass12 => self.low_pass12,
                    LadderMode::LowPass24 => self.low_pass24,
                    LadderMode::HighPass12 => self.high_pass12,
                    LadderMode::HighPass24 => self.high_pass24,
                    LadderMode::BandPass12 => self.band_pass12,
                    LadderMode::BandPass24 => self.band_pass24,
                }
            }
            fn dry(x: $sample) -> Self {
                Self {
                    low_pass12: x,
                    low_pass24: x,
                    high_pass12: x,
                    high_pass24: x,
                    band_pass12: x,
                    band_pass24: x,
                }
            }
        }
        /// Native observations of the immediately preceding non-bypassed sample.
        /// Scalar residual uses rounded affine A/B; staged residual uses actual four
        /// trapezoidal outputs. They need not agree bitwise. No coherence/clock claim.
        #[derive(Clone, Copy, Debug, PartialEq)]
        pub struct $info {
            pub iterations: u32,
            pub scalar_residual: $sample,
            pub staged_residual: $sample,
            pub saturated_input: $sample,
            pub taps: [$sample; 4],
        }
        impl $info {
            fn zero() -> Self {
                Self {
                    iterations: 0,
                    scalar_residual: 0.,
                    staged_residual: 0.,
                    saturated_input: 0.,
                    taps: [0.; 4],
                }
            }
        }
        /// Native single-channel nonlinear feedback ladder. Saturator model:
        /// u=tanh(d*x-k*y4), four RC stages prewarped/trapezoidal. Selected modal taps
        /// share history; no implicit output normalization or sample interpolation.
        ///
        /// Instantaneous F=u-tanh(c-lambda*u) is monotone (lambda=k*G^4>=0), solved
        /// on the amplitude-scaled [0,tanh(abs(c))] bracket with odd symmetry and
        /// bounded 64/32 iterations. Native equation/tanh rounding and actual staged
        /// residual are observations, not exact circuit algebra. The solve uses at
        /// most 67/35 tanh evaluations; the staged residual adds one (68/36 total).
        /// No allocation, locks or I/O, no callback deadline guarantee.
        ///
        /// k = 0 explicitly skips feedback history; poisoned stages still return NaN.
        /// Source +/-Inf intentionally saturates with finite history; NaN/history
        /// poison propagates until reset. Finite drive overflow to infinity saturates.
        /// No universal arbitrary-modulation, overflow/headroom or alias-free claim.
        /// set_design retains trapezoidal integrator memory; replace_design resets it.
        /// Caller owns smoothing, headroom, nonlinear aliasing and denormal policy.
        /// tanh uses platform native math: bit-exact cross-platform results are not
        /// promised. State is trapezoidal integrator memory s=y+v, not voltage y;
        /// retuning retains s. Native32 low-cutoff rounding floors are qualified
        /// separately; there is no implicit precision lift or denormal flush.
        #[derive(Clone, Debug)]
        pub struct $filter {
            design: $design,
            state: [$sample; 4],
            last: $info,
            bypassed: bool,
        }
        impl $filter {
            pub fn new(design: $design) -> Self {
                Self {
                    design,
                    state: [0.; 4],
                    last: $info::zero(),
                    bypassed: false,
                }
            }
            pub fn design(&self) -> $design {
                self.design
            }
            pub fn capacitor_state(&self) -> [$sample; 4] {
                self.state
            }
            pub fn last_solve(&self) -> $info {
                self.last
            }
            pub fn is_bypassed(&self) -> bool {
                self.bypassed
            }
            pub fn set_bypassed(&mut self, bypassed: bool) {
                self.bypassed = bypassed;
            }
            pub fn set_design(&mut self, design: $design) {
                self.design = design;
            }
            pub fn replace_design(&mut self, design: $design) {
                self.design = design;
                self.reset();
            }
            /// Clears capacitor/observation history, retaining design and bypass policy.
            pub fn reset(&mut self) {
                self.state = [0.; 4];
                self.last = $info::zero();
            }
            /// Explicitly clears only strictly subnormal capacitor states. No implicit
            /// callback-boundary snap; normal and nonfinite values remain unchanged.
            pub fn snap_to_zero(&mut self) {
                for s in &mut self.state {
                    if s.is_subnormal() {
                        *s = 0.;
                    }
                }
            }
            fn solve(c: $sample, lambda: $sample) -> ($sample, u32, $sample) {
                if c.is_nan() {
                    return (<$sample>::NAN, 0, <$sample>::NAN);
                }
                if c == 0. {
                    return (c, 0, c);
                }
                let a = c.abs();
                let sign = c.signum();
                let mut low: $sample = 0.;
                let mut high = a.tanh();
                if lambda == 0. {
                    return (sign * high, 0, 0.);
                }
                let mut iterations = 0;
                for _ in 0..$budget {
                    let mid = low + (high - low) * 0.5;
                    if mid == low || mid == high {
                        break;
                    }
                    iterations += 1;
                    let residual = mid - (a - lambda * mid).tanh();
                    if residual == 0. {
                        return (sign * mid, iterations, sign * residual);
                    }
                    if residual < 0. {
                        low = mid;
                    } else {
                        high = mid;
                    }
                }
                let fl = low - (a - lambda * low).tanh();
                let fh = high - (a - lambda * high).tanh();
                if fl.abs() < fh.abs() {
                    (sign * low, iterations, sign * fl)
                } else {
                    (sign * high, iterations, sign * fh)
                }
            }
            pub fn process_sample(&mut self, x: $sample) -> $outputs {
                if self.bypassed {
                    return $outputs::dry(x);
                }
                let g = self.design.gain();
                let k = self.design.feedback();
                let d = self.design.input_drive();
                // Zero resonance bypasses feedback history entirely, avoiding 0*NaN.
                let (c, lambda) = if k == 0. {
                    (d * x, 0.)
                } else {
                    let s = self.state.map(|v| (1. - g) * v);
                    let b = ((s[0] * g + s[1]) * g + s[2]) * g + s[3];
                    (d * x - k * b, k * (((g * g) * g) * g))
                };
                let (u, iterations, scalar_residual) = Self::solve(c, lambda);
                let mut taps = [0.; 4];
                let mut input = u;
                for (state, y) in self.state.iter_mut().zip(&mut taps) {
                    let v = g * (input - *state);
                    *y = *state + v;
                    *state = *y + v;
                    input = *y;
                }
                let staged_residual = if k == 0. {
                    u - (d * x).tanh()
                } else {
                    u - (d * x - k * taps[3]).tanh()
                };
                self.last = $info {
                    iterations,
                    scalar_residual,
                    staged_residual,
                    saturated_input: u,
                    taps,
                };
                let [y1, y2, y3, y4] = taps;
                $outputs {
                    low_pass12: y2,
                    low_pass24: y4,
                    high_pass12: u - 2. * y1 + y2,
                    high_pass24: u - 4. * y1 + 6. * y2 - 4. * y3 + y4,
                    band_pass12: y1 - y2,
                    band_pass24: y2 - 2. * y3 + y4,
                }
            }
            /// Equal-length scalar-channel block, capped at 65536 samples.
            /// Full admission before history/output; empty preserves last solve.
            pub fn process(
                &mut self,
                input: &[$sample],
                output: &mut [$sample],
                mode: LadderMode,
            ) -> Result<(), LadderError> {
                if input.len() != output.len() {
                    return Err(LadderError::BufferLength);
                }
                if input.len() > 65536 {
                    return Err(LadderError::TooManyFrames);
                }
                for (&x, y) in input.iter().zip(output) {
                    *y = self.process_sample(x).select(mode);
                }
                Ok(())
            }
            pub fn process_in_place(
                &mut self,
                buffer: &mut [$sample],
                mode: LadderMode,
            ) -> Result<(), LadderError> {
                if buffer.len() > 65536 {
                    return Err(LadderError::TooManyFrames);
                }
                for x in buffer {
                    *x = self.process_sample(*x).select(mode);
                }
                Ok(())
            }
        }
        /// Fixed 1..64 channel independently evolving ladder histories with shared
        /// immutable coefficient copies and maximum block 1..65536. All shape/prefix/frame
        /// validation completes before output/history. Empty calls do not advance;
        /// suffixes remain untouched. Per-channel scalar access returns all modes once.
        #[derive(Clone, Debug)]
        pub struct $bank {
            filters: [$filter; 64],
            channels: usize,
            maximum: usize,
        }
        impl $bank {
            pub fn new(
                design: $design,
                channels: usize,
                maximum_frames: usize,
            ) -> Result<Self, LadderError> {
                if !(1..=64).contains(&channels) {
                    return Err(LadderError::ChannelCount);
                }
                if !(1..=65536).contains(&maximum_frames) {
                    return Err(LadderError::MaximumFrames);
                }
                Ok(Self {
                    filters: std::array::from_fn(|_| $filter::new(design)),
                    channels,
                    maximum: maximum_frames,
                })
            }
            pub fn channels(&self) -> usize {
                self.channels
            }
            pub fn maximum_frames(&self) -> usize {
                self.maximum
            }
            pub fn design(&self) -> $design {
                self.filters[0].design()
            }
            pub fn last_solve(&self, channel: usize) -> Result<$info, LadderError> {
                if channel >= self.channels {
                    return Err(LadderError::ChannelCount);
                }
                Ok(self.filters[channel].last_solve())
            }
            pub fn capacitor_state(&self, channel: usize) -> Result<[$sample; 4], LadderError> {
                if channel >= self.channels {
                    return Err(LadderError::ChannelCount);
                }
                Ok(self.filters[channel].capacitor_state())
            }
            pub fn set_design(&mut self, design: $design) {
                for f in &mut self.filters[..self.channels] {
                    f.set_design(design);
                }
            }
            pub fn replace_design(&mut self, design: $design) {
                for f in &mut self.filters[..self.channels] {
                    f.replace_design(design);
                }
            }
            pub fn set_bypassed(&mut self, bypassed: bool) {
                for f in &mut self.filters[..self.channels] {
                    f.set_bypassed(bypassed);
                }
            }
            pub fn is_bypassed(&self) -> bool {
                self.filters[0].is_bypassed()
            }
            pub fn reset(&mut self) {
                for f in &mut self.filters[..self.channels] {
                    f.reset();
                }
            }
            pub fn snap_to_zero(&mut self) {
                for f in &mut self.filters[..self.channels] {
                    f.snap_to_zero();
                }
            }
            pub fn process_channel(
                &mut self,
                channel: usize,
                x: $sample,
            ) -> Result<$outputs, LadderError> {
                if channel >= self.channels {
                    return Err(LadderError::ChannelCount);
                }
                Ok(self.filters[channel].process_sample(x))
            }
            pub fn process_frame(
                &mut self,
                input: &[$sample],
                output: &mut [$sample],
                mode: LadderMode,
            ) -> Result<(), LadderError> {
                if input.len() != self.channels || output.len() != self.channels {
                    return Err(LadderError::ChannelCount);
                }
                for (f, (&x, y)) in self.filters[..self.channels]
                    .iter_mut()
                    .zip(input.iter().zip(output))
                {
                    *y = f.process_sample(x).select(mode);
                }
                Ok(())
            }
            fn preflight(&self, input: &[&[$sample]], frames: usize) -> Result<(), LadderError> {
                if input.len() != self.channels {
                    return Err(LadderError::ChannelCount);
                }
                if frames > self.maximum {
                    return Err(LadderError::TooManyFrames);
                }
                if input.iter().any(|x| x.len() < frames) {
                    return Err(LadderError::BufferLength);
                }
                Ok(())
            }
            pub fn process(
                &mut self,
                input: &[&[$sample]],
                output: &mut [&mut [$sample]],
                frames: usize,
                mode: LadderMode,
            ) -> Result<(), LadderError> {
                self.preflight(input, frames)?;
                if output.len() != self.channels {
                    return Err(LadderError::ChannelCount);
                }
                if output.iter().any(|x| x.len() < frames) {
                    return Err(LadderError::BufferLength);
                }
                for (ch, (input, output)) in input.iter().zip(output.iter_mut()).enumerate() {
                    for (&x, y) in input[..frames].iter().zip(&mut output[..frames]) {
                        *y = self.filters[ch].process_sample(x).select(mode);
                    }
                }
                Ok(())
            }
            pub fn process_in_place(
                &mut self,
                buffer: &mut [&mut [$sample]],
                frames: usize,
                mode: LadderMode,
            ) -> Result<(), LadderError> {
                if buffer.len() != self.channels {
                    return Err(LadderError::ChannelCount);
                }
                if frames > self.maximum {
                    return Err(LadderError::TooManyFrames);
                }
                if buffer.iter().any(|x| x.len() < frames) {
                    return Err(LadderError::BufferLength);
                }
                for (ch, channel) in buffer.iter_mut().enumerate() {
                    for x in &mut channel[..frames] {
                        *x = self.filters[ch].process_sample(*x).select(mode);
                    }
                }
                Ok(())
            }
        }
    };
}
native!(
    f64,
    LadderDesign,
    LadderOutputs,
    LadderSolveInfo,
    LadderFilter,
    LadderBank,
    64
);
native!(
    f32,
    LadderDesign32,
    LadderOutputs32,
    LadderSolveInfo32,
    LadderFilter32,
    LadderBank32,
    32
);
