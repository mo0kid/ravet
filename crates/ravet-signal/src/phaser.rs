//! Six-stage native allpass/LFO/delayed-feedback phaser with explicit sweep bounds.
//!
//! ```
//! use ravet_signal::{Phaser, PhaserConfig, PhaserControls};
//! let config=PhaserConfig { sample_rate:48000.,channels:2,maximum_frames:64 };
//! let controls=PhaserControls { rate_hz:1.,depth:0.5,center_frequency_hz:1000.,
//!     feedback:0.2,wet_mix:0.,phase_offset_cycles:0.25 };
//! let mut phaser=Phaser::new(config,controls).unwrap();
//! let mut output=[99.;2];
//! phaser.process_frame(&[0.3,-0.7],&mut output).unwrap();
//! assert_eq!(output,[0.3,-0.7]); // dry output, wet history still advanced
//! assert!(phaser.phase_cycles()>0.);
//! ```
use crate::{FirstOrderTptDesign, FirstOrderTptFilter, FirstOrderTptFilter32};
/// Own independent modulation policy. Depth is a four-octave maximum half-span,
/// fc=center*2^(4*depth*sin(2*pi*phase)). Adjacent channel offsets are in cycles;
/// channels never crossfeed. No implicit control smoothing/default values.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PhaserControls {
    /// Zero freezes base phase; otherwise finite .001..100Hz.
    pub rate_hz: f64,
    pub depth: f64,
    pub center_frequency_hz: f64,
    pub feedback: f64,
    pub wet_mix: f64,
    pub phase_offset_cycles: f64,
}
/// Finite sample rate8000..768000, channels1..64, block1..65536.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PhaserConfig {
    pub sample_rate: f64,
    pub channels: usize,
    pub maximum_frames: usize,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PhaserError {
    InvalidSampleRate,
    InvalidControls,
    InvalidChannelCount,
    InvalidMaximumFrames,
    ChannelCount,
    BufferTooShort,
    TooManyFrames,
    InvalidPhase,
}
impl std::fmt::Display for PhaserError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::InvalidSampleRate => "phaser sample rate must be finite8000..768000Hz",
            Self::InvalidControls => {
                "phaser controls or complete native floating sweep unavailable"
            }
            Self::InvalidChannelCount => "phaser channels must be1..64",
            Self::InvalidMaximumFrames => "phaser maximum frames must be1..65536",
            Self::ChannelCount => "phaser channels differ from prepared layout",
            Self::BufferTooShort => "phaser channel shorter than active prefix",
            Self::TooManyFrames => "phaser prefix exceeds prepared maximum",
            Self::InvalidPhase => "phaser reset phase must be finite0..1 excluding1",
        })
    }
}
impl std::error::Error for PhaserError {}
#[derive(Clone, Copy, Debug)]
struct Prepared {
    step: f64,
    span: f64,
    low: f64,
    high: f64,
}
fn validate_config(c: PhaserConfig) -> Result<(), PhaserError> {
    if !c.sample_rate.is_finite() || !(8000.0..=768000.).contains(&c.sample_rate) {
        return Err(PhaserError::InvalidSampleRate);
    }
    if !(1..=64).contains(&c.channels) {
        return Err(PhaserError::InvalidChannelCount);
    }
    if !(1..=65536).contains(&c.maximum_frames) {
        return Err(PhaserError::InvalidMaximumFrames);
    }
    Ok(())
}
fn prepare(rate: f64, c: PhaserControls, native32: bool) -> Result<Prepared, PhaserError> {
    if !c.rate_hz.is_finite()
        || !(c.rate_hz == 0. || (0.001..=100.).contains(&c.rate_hz))
        || !c.depth.is_finite()
        || !(0.0..=1.).contains(&c.depth)
        || !c.center_frequency_hz.is_finite()
        || c.center_frequency_hz <= 0.
        || !c.feedback.is_finite()
        || !(-0.99..=0.99).contains(&c.feedback)
        || !c.wet_mix.is_finite()
        || !(0.0..=1.).contains(&c.wet_mix)
        || !c.phase_offset_cycles.is_finite()
        || !(0.0..=1.).contains(&c.phase_offset_cycles)
    {
        return Err(PhaserError::InvalidControls);
    }
    let span = 4. * c.depth;
    let low = c.center_frequency_hz * (-span).exp2();
    let high = c.center_frequency_hz * span.exp2();
    // Original floating Hz and normalized extrema both preflight; no clamps or
    // endpoint nudges. This interior profile is far from native pole endpoints.
    if !low.is_finite()
        || !high.is_finite()
        || low < rate * 1e-6
        || high > rate * 0.49
        || !(1e-6..=0.49).contains(&(low / rate))
        || !(1e-6..=0.49).contains(&(high / rate))
    {
        return Err(PhaserError::InvalidControls);
    }
    // Check the exact extrema expression, its inward-adjacent exponent values
    // and center through public design/quantization. A callback uses the same
    // finite sin/exp2/prewarp expressions, not a projected coefficient curve.
    for exponent in [
        -span,
        span,
        0.,
        -span + span * f64::EPSILON,
        span - span * f64::EPSILON,
    ] {
        let cutoff = c.center_frequency_hz * exponent.exp2();
        let design =
            FirstOrderTptDesign::new(rate, cutoff).map_err(|_| PhaserError::InvalidControls)?;
        if native32 {
            design
                .quantize_f32()
                .map_err(|_| PhaserError::InvalidControls)?;
        }
    }
    Ok(Prepared {
        step: c.rate_hz / rate,
        span,
        low,
        high,
    })
}
fn cutoff(c: PhaserControls, p: Prepared, phase: f64, ch: usize) -> f64 {
    let phase = (phase + (ch as f64 * c.phase_offset_cycles).fract()).fract();
    c.center_frequency_hz * (p.span * (std::f64::consts::TAU * phase).sin()).exp2()
}
macro_rules! phaser {
    ($name:ident,$sample:ty,$filter:ty,$quantize:expr,$native32:expr) => {
        /// Fixed six native TPT allpasses per independent channel, shared f64
        /// base LFO phase with explicit offsets, one-sample signed wet feedback.
        /// Whole admissible sweep near normalized1e-6..0.49 is CONTROL validated.
        /// F64 sine/exp2/tan/phase are explicit per frame/channel; coefficient and
        /// recurrence/audio arithmetic are native. No deadline/cheap-LFO claim.
        /// Construction/control/process/reset use fixed storage, no heap/lock/I/O.
        ///
        /// Cascade input is x+beta*previous_wet (beta0 bypasses multiplication),
        /// then six allpass updates and retained wet. Mix0/1 returns exact dry/wet
        /// branches; dry still advances all wet histories and phase. Each frame
        /// advances the base phase once regardless of channel count. Controls
        /// replace atomically retaining phase/histories; reset clears all.
        ///
        /// At fixed actual coefficients A=(-p+z^-1)/(1-p*z^-1),p=1-2G;
        /// W=A^6/(1-beta*z^-1*A^6). Static ideal |beta|<1 small-gain stability is
        /// distinct from arbitrary modulation/floating overflow/inaudibility.
        /// Recursive tail/denormals/NaN/Inf poison, aliasing/abrupt retunes and
        /// feedback headroom are caller policy; no limiter/normalization repair,
        /// implicit EOF/constant latency/host report/SIMD/hardware qualification.
        #[derive(Clone, Debug)]
        pub struct $name {
            config: PhaserConfig,
            controls: PhaserControls,
            prepared: Prepared,
            filters: [[$filter; 6]; 64],
            previous_wet: [$sample; 64],
            phase: f64,
            feedback: $sample,
            mix: $sample,
        }
        impl $name {
            pub fn new(
                config: PhaserConfig,
                controls: PhaserControls,
            ) -> Result<Self, PhaserError> {
                validate_config(config)?;
                let prepared = prepare(config.sample_rate, controls, $native32)?;
                let design = ($quantize)(FirstOrderTptDesign::from_admitted_phaser(
                    config.sample_rate,
                    controls.center_frequency_hz,
                ));
                Ok(Self {
                    config,
                    controls,
                    prepared,
                    filters: std::array::from_fn(|_| {
                        std::array::from_fn(|_| <$filter>::new(design))
                    }),
                    previous_wet: [0.; 64],
                    phase: 0.,
                    feedback: controls.feedback as $sample,
                    mix: controls.wet_mix as $sample,
                })
            }
            pub fn config(&self) -> PhaserConfig {
                self.config
            }
            pub fn controls(&self) -> PhaserControls {
                self.controls
            }
            pub fn phase_cycles(&self) -> f64 {
                self.phase
            }
            pub fn native_feedback(&self) -> $sample {
                self.feedback
            }
            pub fn native_wet_mix(&self) -> $sample {
                self.mix
            }
            pub fn sweep_endpoints_hz(&self) -> [f64; 2] {
                [self.prepared.low, self.prepared.high]
            }
            /// CONTROL observation at the phase of the next frame, no history edit.
            pub fn next_cutoff_hz(&self, ch: usize) -> Result<f64, PhaserError> {
                if ch >= self.config.channels {
                    return Err(PhaserError::ChannelCount);
                }
                Ok(cutoff(self.controls, self.prepared, self.phase, ch))
            }
            pub fn next_gain(&self, ch: usize) -> Result<$sample, PhaserError> {
                let cutoff = self.next_cutoff_hz(ch)?;
                Ok(($quantize)(FirstOrderTptDesign::from_admitted_phaser(
                    self.config.sample_rate,
                    cutoff,
                ))
                .gain())
            }
            pub fn set_controls(&mut self, controls: PhaserControls) -> Result<(), PhaserError> {
                let prepared = prepare(self.config.sample_rate, controls, $native32)?;
                self.controls = controls;
                self.prepared = prepared;
                self.feedback = controls.feedback as $sample;
                self.mix = controls.wet_mix as $sample;
                Ok(())
            }
            pub fn reset(&mut self) {
                self.clear(0.);
            }
            /// Validate before clearing any history; phase is sampled before step.
            pub fn reset_to_phase(&mut self, phase: f64) -> Result<(), PhaserError> {
                if !phase.is_finite() || !(0.0..1.).contains(&phase) {
                    return Err(PhaserError::InvalidPhase);
                }
                self.clear(phase);
                Ok(())
            }
            fn clear(&mut self, phase: f64) {
                for channel in &mut self.filters {
                    for stage in channel {
                        stage.reset();
                    }
                }
                self.previous_wet.fill(0.);
                self.phase = phase;
            }
            fn channel(&mut self, ch: usize, x: $sample) -> $sample {
                let fc = cutoff(self.controls, self.prepared, self.phase, ch);
                let design = ($quantize)(FirstOrderTptDesign::from_admitted_phaser(
                    self.config.sample_rate,
                    fc,
                ));
                let mut wet = if self.feedback == 0. {
                    x
                } else {
                    x + self.feedback * self.previous_wet[ch]
                };
                for stage in &mut self.filters[ch] {
                    stage.set_design(design);
                    wet = stage.process_sample(wet).all_pass;
                }
                self.previous_wet[ch] = wet;
                if self.mix == 0. {
                    x
                } else if self.mix == 1. {
                    wet
                } else {
                    (1. - self.mix) * x + self.mix * wet
                }
            }
            fn advance(&mut self) {
                self.phase += self.prepared.step;
                if self.phase >= 1. {
                    self.phase -= 1.;
                }
            }
            pub fn process_frame(
                &mut self,
                input: &[$sample],
                output: &mut [$sample],
            ) -> Result<(), PhaserError> {
                if input.len() != self.config.channels || output.len() != self.config.channels {
                    return Err(PhaserError::ChannelCount);
                }
                for (ch, (&x, y)) in input.iter().zip(output).enumerate() {
                    *y = self.channel(ch, x);
                }
                self.advance();
                Ok(())
            }
            fn preflight(
                &self,
                lengths: impl Iterator<Item = usize>,
                frames: usize,
            ) -> Result<(), PhaserError> {
                if frames > self.config.maximum_frames {
                    return Err(PhaserError::TooManyFrames);
                }
                if lengths.into_iter().any(|n| n < frames) {
                    return Err(PhaserError::BufferTooShort);
                }
                Ok(())
            }
            pub fn process_planar(
                &mut self,
                input: &[&[$sample]],
                output: &mut [&mut [$sample]],
                frames: usize,
            ) -> Result<(), PhaserError> {
                if input.len() != self.config.channels || output.len() != self.config.channels {
                    return Err(PhaserError::ChannelCount);
                }
                self.preflight(
                    input
                        .iter()
                        .map(|c| c.len())
                        .chain(output.iter().map(|c| c.len())),
                    frames,
                )?;
                for i in 0..frames {
                    for ch in 0..self.config.channels {
                        output[ch][i] = self.channel(ch, input[ch][i]);
                    }
                    self.advance();
                }
                Ok(())
            }
            pub fn process_planar_in_place(
                &mut self,
                channels: &mut [&mut [$sample]],
                frames: usize,
            ) -> Result<(), PhaserError> {
                if channels.len() != self.config.channels {
                    return Err(PhaserError::ChannelCount);
                }
                self.preflight(channels.iter().map(|c| c.len()), frames)?;
                for i in 0..frames {
                    for (ch, channel) in channels.iter_mut().enumerate() {
                        channel[i] = self.channel(ch, channel[i]);
                    }
                    self.advance();
                }
                Ok(())
            }
        }
    };
}
phaser!(
    Phaser,
    f64,
    FirstOrderTptFilter,
    |d: FirstOrderTptDesign| d,
    false
);
phaser!(
    Phaser32,
    f32,
    FirstOrderTptFilter32,
    |d: FirstOrderTptDesign| d.quantize_admitted_phaser(),
    true
);
