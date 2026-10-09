//! Prepared stereo modulated fractional-delay chorus.
use crate::DelayLine;

/// Live chorus controls, replaced together after validation. Delay modulation
/// is center*(1 + depth*sin(phase)), clamped to at least one sample. This is a
/// linear fractional-delay profile; delay/pitch modulation is not bandlimited.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ChorusControls {
    /// Finite LFO frequency 0..=100 Hz; zero freezes the current phase.
    pub rate_hz: f64,
    /// Finite fractional depth 0..=1, relative to the center delay.
    pub depth: f64,
    /// Finite center delay 1..=100 milliseconds.
    pub center_delay_ms: f64,
    /// Finite per-channel feedback -0.99..=0.99; channels do not crossfeed.
    pub feedback: f64,
    /// Finite linear dry/wet crossfade 0..=1. Dry still advances wet history.
    pub mix: f64,
    /// Right LFO offset relative to left, finite 0..=1 cycles.
    pub stereo_phase_cycles: f64,
}
impl Default for ChorusControls {
    fn default() -> Self {
        Self {
            rate_hz: 1.,
            depth: 0.25,
            center_delay_ms: 8.,
            feedback: 0.1,
            mix: 0.5,
            stereo_phase_cycles: 0.25,
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ChorusError {
    InvalidSampleRate,
    InvalidControls,
    LengthMismatch,
}
impl std::fmt::Display for ChorusError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::InvalidSampleRate => "chorus sample rate must be finite and 8000..=192000 Hz",
            Self::InvalidControls => "chorus controls are nonfinite or out of range",
            Self::LengthMismatch => "chorus channel lengths differ",
        })
    }
}
impl std::error::Error for ChorusError {}
#[derive(Clone, Copy, Debug)]
struct PreparedControls {
    center: f64,
    step: f64,
}
fn prepare(rate: f64, c: ChorusControls) -> Result<PreparedControls, ChorusError> {
    if !c.rate_hz.is_finite()
        || !(0. ..=100.).contains(&c.rate_hz)
        || !c.depth.is_finite()
        || !(0. ..=1.).contains(&c.depth)
        || !c.center_delay_ms.is_finite()
        || !(1. ..=100.).contains(&c.center_delay_ms)
        || !c.feedback.is_finite()
        || !(-0.99..=0.99).contains(&c.feedback)
        || !c.mix.is_finite()
        || !(0. ..=1.).contains(&c.mix)
        || !c.stereo_phase_cycles.is_finite()
        || !(0. ..=1.).contains(&c.stereo_phase_cycles)
    {
        return Err(ChorusError::InvalidControls);
    }
    Ok(PreparedControls {
        center: c.center_delay_ms * (rate * 0.001),
        step: c.rate_hz / rate,
    })
}
/// Prepared two-channel f64 chorus with independent feedback histories and a
/// shared sine LFO plus right phase offset. Construction owns two delay buffers
/// of ceil(rate*0.2)+1 samples each, at most 76802 f64 values total. Processing,
/// live control replacement and reset do not allocate/release heap storage.
/// Controls change immediately without smoothing/crossfades. Nonfinite audio may
/// poison histories until reset; consumers own headroom, denormals and deadlines.
/// No automatic latency/tail reporting, stereo crossfeed or true-peak policy.
#[derive(Debug)]
pub struct StereoChorus {
    rate: f64,
    controls: ChorusControls,
    prepared: PreparedControls,
    delays: [DelayLine; 2],
    length: usize,
    phase: f64,
}
impl StereoChorus {
    /// Prepare finite rate 8000..=192000 Hz and complete bounded controls before
    /// allocating delay storage. All later permitted delay edits fit this storage.
    pub fn new(sample_rate: f64, controls: ChorusControls) -> Result<Self, ChorusError> {
        if !sample_rate.is_finite() || !(8000. ..=192000.).contains(&sample_rate) {
            return Err(ChorusError::InvalidSampleRate);
        }
        let prepared = prepare(sample_rate, controls)?;
        let length = (sample_rate * 0.2).ceil() as usize + 1;
        Ok(Self {
            rate: sample_rate,
            controls,
            prepared,
            delays: std::array::from_fn(|_| DelayLine::new(length).expect("validated length")),
            length,
            phase: 0.,
        })
    }
    pub fn sample_rate(&self) -> f64 {
        self.rate
    }
    pub fn controls(&self) -> ChorusControls {
        self.controls
    }
    /// Delay storage per channel, including the interpolation guard sample.
    pub fn delay_capacity(&self) -> usize {
        self.length
    }
    /// Left LFO phase for the next sample, in cycles 0..1.
    pub fn phase(&self) -> f64 {
        self.phase
    }
    /// Validate every control before mutation; preserve phase, both delay
    /// histories and allocations. Replacement is immediate without smoothing.
    pub fn set_controls(&mut self, controls: ChorusControls) -> Result<(), ChorusError> {
        let prepared = prepare(self.rate, controls)?;
        self.controls = controls;
        self.prepared = prepared;
        Ok(())
    }
    /// Clear histories and reset left phase to zero, retaining rate/controls.
    pub fn reset(&mut self) {
        for delay in &mut self.delays {
            delay.reset();
        }
        self.phase = 0.;
    }
    /// Read each modulated wet delay before feedback writing, then advance once.
    /// Dry/wet endpoints are exact, and dry mode continues the entire wet path.
    pub fn process_sample(&mut self, left: f64, right: f64) -> [f64; 2] {
        let input = [left, right];
        let mut output = [0.; 2];
        let phases = [
            self.phase,
            (self.phase + self.controls.stereo_phase_cycles).fract(),
        ];
        for i in 0..2 {
            let delay_samples = (self.prepared.center
                * (1. + self.controls.depth * (std::f64::consts::TAU * phases[i]).sin()))
            .clamp(1., (self.length - 1) as f64);
            let wet = self.delays[i].read(delay_samples);
            self.delays[i].write(if self.controls.feedback == 0. {
                input[i]
            } else {
                input[i] + self.controls.feedback * wet
            });
            self.delays[i].advance();
            output[i] = if self.controls.mix == 0. {
                input[i]
            } else if self.controls.mix == 1. {
                wet
            } else {
                (1. - self.controls.mix) * input[i] + self.controls.mix * wet
            };
        }
        self.phase += self.prepared.step;
        if self.phase >= 1. {
            self.phase -= 1.;
        }
        output
    }
    /// Equal-length channels only; mismatches preserve all samples and histories.
    /// Empty buffers do not advance phase. Uses only prepared storage.
    pub fn process_in_place(
        &mut self,
        left: &mut [f64],
        right: &mut [f64],
    ) -> Result<(), ChorusError> {
        if left.len() != right.len() {
            return Err(ChorusError::LengthMismatch);
        }
        for (left, right) in left.iter_mut().zip(right) {
            let output = self.process_sample(*left, *right);
            *left = output[0];
            *right = output[1];
        }
        Ok(())
    }
}
