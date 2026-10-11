//! Prepared first-order trapezoidal LP/HP/allpass with explicit capacitor history.
use crate::FilterError;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FirstOrderTptResponse {
    LowPass,
    HighPass,
    AllPass,
}

/// Immutable CONTROL design: finite rate 1..768000 Hz, strictly positive cutoff
/// below actual Nyquist and representable normalized ratio, gain/complement/pole.
/// No endpoint nudges or coefficient repairs; transcendental design is explicit.
/// Actual coefficient spectrum can differ from the requested cutoff after rounding.
///
/// ```
/// use ravet_signal::{FirstOrderTptDesign, FirstOrderTptFilter, FirstOrderTptResponse};
/// let design = FirstOrderTptDesign::new(48000., 2000.).unwrap();
/// assert_eq!(design.magnitude_at(0., FirstOrderTptResponse::LowPass).unwrap(), 1.);
/// let mut filter = FirstOrderTptFilter::new(design);
/// let output = filter.process_sample(1.);
/// assert_eq!(output.low_pass, design.gain());
/// assert_eq!(filter.state(), 2. * design.gain());
/// ```
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FirstOrderTptDesign {
    rate: f64,
    cutoff: f64,
    gain: f64,
}
impl FirstOrderTptDesign {
    pub fn new(rate: f64, cutoff_hz: f64) -> Result<Self, FilterError> {
        if !rate.is_finite()
            || !(1.0..=768000.).contains(&rate)
            || !cutoff_hz.is_finite()
            || cutoff_hz <= 0.
            || cutoff_hz >= rate * 0.5
        {
            return Err(FilterError);
        }
        let normalized = cutoff_hz / rate;
        if !(0. < normalized && normalized < 0.5) {
            return Err(FilterError);
        }
        let g = (std::f64::consts::PI * normalized).tan();
        let gain = g / (1. + g);
        let complement = 1. - gain;
        let pole = 1. - 2. * gain;
        if !g.is_finite()
            || g <= 0.
            || !gain.is_finite()
            || !(0. < gain && gain < 1. && 0. < complement && complement < 1.)
            || !(-1. < pole && pole < 1.)
        {
            return Err(FilterError);
        }
        Ok(Self {
            rate,
            cutoff: cutoff_hz,
            gain,
        })
    }
    // Private already-admitted Phaser profile only: finite rate8k..768k and
    // whole floating cutoff sweep near normalized1e-6..0.49. This shares exactly
    // the public constructor's coefficient expressions, without callback repair
    // or a fallible per-frame control update. Phaser owns the admission proof.
    pub(crate) fn from_admitted_phaser(rate: f64, cutoff_hz: f64) -> Self {
        let normalized = cutoff_hz / rate;
        let g = (std::f64::consts::PI * normalized).tan();
        Self {
            rate,
            cutoff: cutoff_hz,
            gain: g / (1. + g),
        }
    }
    pub(crate) fn quantize_admitted_phaser(&self) -> FirstOrderTptDesign32 {
        FirstOrderTptDesign32 {
            prototype: *self,
            gain: self.gain as f32,
        }
    }
    pub fn sample_rate(&self) -> f64 {
        self.rate
    }
    pub fn cutoff_hz(&self) -> f64 {
        self.cutoff
    }
    pub fn gain(&self) -> f64 {
        self.gain
    }
    /// Actual-coefficient ideal LTI magnitude, normalized cycles/sample 0..0.5.
    /// This is not the floating recursive execution's universal error bound.
    pub fn magnitude_at(
        &self,
        f: f64,
        response: FirstOrderTptResponse,
    ) -> Result<f64, FilterError> {
        magnitude(self.gain, f, response)
    }
    /// Explicit actual native f32 gain/complement/pole validation. Metadata keeps
    /// the requested rate/cutoff; the new spectrum uses the rounded gain.
    pub fn quantize_f32(&self) -> Result<FirstOrderTptDesign32, FilterError> {
        let gain = self.gain as f32;
        let complement = 1. - gain;
        let pole = 1. - 2. * gain;
        if !(0. < gain && gain < 1. && 0. < complement && complement < 1.)
            || !(-1. < pole && pole < 1.)
        {
            return Err(FilterError);
        }
        Ok(FirstOrderTptDesign32 {
            prototype: *self,
            gain,
        })
    }
}
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FirstOrderTptDesign32 {
    prototype: FirstOrderTptDesign,
    gain: f32,
}
impl FirstOrderTptDesign32 {
    pub fn sample_rate(&self) -> f64 {
        self.prototype.rate
    }
    pub fn cutoff_hz(&self) -> f64 {
        self.prototype.cutoff
    }
    pub fn gain(&self) -> f32 {
        self.gain
    }
    /// Actual f32-coefficient ideal LTI observation evaluated in f64; requested
    /// cutoff metadata is not an implicit f64 spectral/phase guarantee.
    pub fn magnitude_at(
        &self,
        f: f64,
        response: FirstOrderTptResponse,
    ) -> Result<f64, FilterError> {
        magnitude(f64::from(self.gain), f, response)
    }
}
fn magnitude(gain: f64, f: f64, response: FirstOrderTptResponse) -> Result<f64, FilterError> {
    if !f.is_finite() || !(0.0..=0.5).contains(&f) {
        return Err(FilterError);
    }
    if f == 0. {
        return Ok(if response == FirstOrderTptResponse::HighPass {
            0.
        } else {
            1.
        });
    }
    if f == 0.5 {
        return Ok(if response == FirstOrderTptResponse::LowPass {
            0.
        } else {
            1.
        });
    }
    // Factoring z^-1 at the half-angle avoids cancellation of 1-p near
    // admitted endpoints. This evaluates the ideal transfer of actual gain.
    let (s, c) = (std::f64::consts::PI * f).sin_cos();
    let low = gain * c;
    let high = (1. - gain) * s;
    let denominator = low.hypot(high);
    let numerator = match response {
        FirstOrderTptResponse::LowPass => low.abs(),
        FirstOrderTptResponse::HighPass => high.abs(),
        FirstOrderTptResponse::AllPass => denominator,
    };
    let result = numerator / denominator;
    if !result.is_finite() {
        return Err(FilterError);
    }
    Ok(result)
}
macro_rules! native {
    ($sample:ty,$design:ty,$outputs:ident,$filter:ident,$bank:ident,$step:ident) => {
        /// Simultaneous responses from one capacitor update. LP+HP is input in
        /// ideal arithmetic; allpass has frequency-dependent phase, not dry delay.
        #[derive(Clone, Copy, Debug, PartialEq)]
        pub struct $outputs {
            pub low_pass: $sample,
            pub high_pass: $sample,
            pub all_pass: $sample,
        }
        impl $outputs {
            pub fn select(self, response: FirstOrderTptResponse) -> $sample {
                match response {
                    FirstOrderTptResponse::LowPass => self.low_pass,
                    FirstOrderTptResponse::HighPass => self.high_pass,
                    FirstOrderTptResponse::AllPass => self.all_pass,
                }
            }
        }
        fn $step(state: &mut $sample, input: $sample, design: &$design) -> $outputs {
            let v = design.gain() * (input - *state);
            let low = *state + v;
            *state = low + v;
            let high = input - low;
            $outputs {
                low_pass: low,
                high_pass: high,
                all_pass: low - high,
            }
        }
        /// Fixed signed capacitor state, native one-pole trapezoidal LP/HP/allpass.
        /// Prepared design replacement may preserve state or explicitly reset it.
        /// No smoothing, universal modulation stability/inaudibility, sanitization,
        /// aligned dry path or integer latency. Nonfinite/large finite overflow
        /// propagates and may poison history until reset; infinite ideal tail.
        #[derive(Clone, Debug)]
        pub struct $filter {
            design: $design,
            state: $sample,
        }
        impl $filter {
            pub fn new(design: $design) -> Self {
                Self { design, state: 0. }
            }
            pub fn design(&self) -> &$design {
                &self.design
            }
            pub fn state(&self) -> $sample {
                self.state
            }
            /// Preserve the signed capacitor state; no implicit control smoothing.
            pub fn set_design(&mut self, design: $design) {
                self.design = design;
            }
            /// Replace design and reset history; no crossfade.
            pub fn replace_design(&mut self, design: $design) {
                self.design = design;
                self.reset();
            }
            pub fn reset(&mut self) {
                self.state = 0.;
            }
            /// Initial signed capacitor state, not previous LP output. Nonfinite
            /// initial values reject without changing the existing history.
            pub fn reset_to(&mut self, state: $sample) -> Result<(), FilterError> {
                if !state.is_finite() {
                    return Err(FilterError);
                }
                self.state = state;
                Ok(())
            }
            /// Caller-only signed subnormal retirement; no implicit block snap.
            pub fn snap_to_zero(&mut self) {
                if self.state.is_subnormal() {
                    self.state = 0.;
                }
            }
            pub fn process_sample(&mut self, input: $sample) -> $outputs {
                $step(&mut self.state, input, &self.design)
            }
            pub fn process_in_place(
                &mut self,
                audio: &mut [$sample],
                response: FirstOrderTptResponse,
            ) {
                for x in audio {
                    *x = self.process_sample(*x).select(response);
                }
            }
            /// All lengths preflight before any history/output; empty no-op.
            pub fn process_block(
                &mut self,
                input: &[$sample],
                output: &mut [$sample],
                response: FirstOrderTptResponse,
            ) -> Result<(), FilterError> {
                if input.len() != output.len() {
                    return Err(FilterError);
                }
                for (x, y) in input.iter().zip(output.iter_mut()) {
                    *y = self.process_sample(*x).select(response);
                }
                Ok(())
            }
        }
        /// Independent 1..64 signed channel states, shared immutable coefficient
        /// profile and maximum block 1..65536. Fixed storage and no allocation in
        /// construct/process/reset; planar whole-prefix preflight before any edit.
        /// Caller suffixes untouched; empty no-op. No automatic denormal snapping.
        #[derive(Clone, Debug)]
        pub struct $bank {
            design: $design,
            channels: usize,
            max_block: usize,
            states: [$sample; 64],
        }
        impl $bank {
            pub fn new(
                design: $design,
                channels: usize,
                max_block: usize,
            ) -> Result<Self, FilterError> {
                if !(1..=64).contains(&channels) || !(1..=65536).contains(&max_block) {
                    return Err(FilterError);
                }
                Ok(Self {
                    design,
                    channels,
                    max_block,
                    states: [0.; 64],
                })
            }
            pub fn design(&self) -> &$design {
                &self.design
            }
            pub fn channels(&self) -> usize {
                self.channels
            }
            pub fn maximum_block_frames(&self) -> usize {
                self.max_block
            }
            pub fn state(&self, channel: usize) -> Result<$sample, FilterError> {
                if channel >= self.channels {
                    return Err(FilterError);
                }
                Ok(self.states[channel])
            }
            pub fn set_design(&mut self, design: $design) {
                self.design = design;
            }
            pub fn replace_design(&mut self, design: $design) {
                self.design = design;
                self.reset();
            }
            pub fn reset(&mut self) {
                self.states.fill(0.);
            }
            pub fn reset_to(&mut self, state: $sample) -> Result<(), FilterError> {
                if !state.is_finite() {
                    return Err(FilterError);
                }
                self.states.fill(state);
                Ok(())
            }
            pub fn snap_to_zero(&mut self) {
                for state in &mut self.states[..self.channels] {
                    if state.is_subnormal() {
                        *state = 0.;
                    }
                }
            }
            pub fn process_sample(
                &mut self,
                channel: usize,
                input: $sample,
            ) -> Result<$outputs, FilterError> {
                if channel >= self.channels {
                    return Err(FilterError);
                }
                Ok($step(&mut self.states[channel], input, &self.design))
            }
            pub fn process_planar(
                &mut self,
                input: &[&[$sample]],
                output: &mut [&mut [$sample]],
                frames: usize,
                response: FirstOrderTptResponse,
            ) -> Result<(), FilterError> {
                if input.len() != self.channels
                    || output.len() != self.channels
                    || frames > self.max_block
                    || input.iter().any(|x| x.len() < frames)
                    || output.iter().any(|x| x.len() < frames)
                {
                    return Err(FilterError);
                }
                for (ch, (input, output)) in input.iter().zip(output.iter_mut()).enumerate() {
                    for (x, y) in input[..frames].iter().zip(output[..frames].iter_mut()) {
                        *y = $step(&mut self.states[ch], *x, &self.design).select(response);
                    }
                }
                Ok(())
            }
            pub fn process_planar_in_place(
                &mut self,
                audio: &mut [&mut [$sample]],
                frames: usize,
                response: FirstOrderTptResponse,
            ) -> Result<(), FilterError> {
                if audio.len() != self.channels
                    || frames > self.max_block
                    || audio.iter().any(|x| x.len() < frames)
                {
                    return Err(FilterError);
                }
                for (ch, x) in audio.iter_mut().enumerate() {
                    for x in &mut x[..frames] {
                        *x = $step(&mut self.states[ch], *x, &self.design).select(response);
                    }
                }
                Ok(())
            }
        }
    };
}
native!(
    f64,
    FirstOrderTptDesign,
    FirstOrderTptOutputs,
    FirstOrderTptFilter,
    FirstOrderTptBank,
    step64
);
native!(
    f32,
    FirstOrderTptDesign32,
    FirstOrderTptOutputs32,
    FirstOrderTptFilter32,
    FirstOrderTptBank32,
    step32
);
