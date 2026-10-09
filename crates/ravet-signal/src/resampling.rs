//! Prepared causal windowed-sinc conversion with bounded caller-owned I/O.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ResamplingError;
/// Exact prefix progress. Retry with unconsumed input and fresh output capacity.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ResamplingProgress {
    pub consumed: usize,
    pub produced: usize,
}
/// One-channel f64 arbitrary-ratio streaming converter, rate ratio 1/16..16.
/// Preparation allocates a 1025-phase Blackman-windowed sinc table and history.
/// Processing interpolates table phases and allocates nothing. Filter delay is
/// (taps-1)/2 input samples; initial history is zero. This is a fixed quality
/// profile, not a spectral rejection or callback deadline guarantee. Fixed
/// rates use [`Self::new`]; [`Self::new_variable`] prepares bounded live ppm
/// adjustments with a conservative fixed cutoff and retained stream phase.
#[derive(Clone, Debug)]
pub struct SincResampler {
    step: f64,
    nominal_step: f64,
    maximum_adjustment_ppm: f64,
    distance: f64,
    has_input: bool,
    history: Vec<f64>,
    coefficients: Vec<f64>,
    write: usize,
}
impl SincResampler {
    /// Finite positive rates and even tap count 16..256. Control-thread work.
    /// Rates are expressed in the same units; no hardware/format conversion.
    pub fn new(input_rate: f64, output_rate: f64, taps: usize) -> Result<Self, ResamplingError> {
        Self::prepare(input_rate, output_rate, taps, 0.)
    }
    /// Prepare a live-adjustable clock-drift profile. Maximum adjustment must
    /// be finite in (0,10000] ppm (one percent); the entire permitted input/
    /// output step range must stay within 1/16..16. Filter cutoff uses the
    /// largest permitted step, avoiding live coefficient allocation. This is
    /// not an automatic drift estimator or a spectral/deadline guarantee.
    pub fn new_variable(
        input_rate: f64,
        output_rate: f64,
        taps: usize,
        maximum_adjustment_ppm: f64,
    ) -> Result<Self, ResamplingError> {
        Self::validate_variable(input_rate, output_rate, taps, maximum_adjustment_ppm)?;
        Self::prepare(input_rate, output_rate, taps, maximum_adjustment_ppm)
    }
    /// Check live-adjustable preparation bounds without allocating coefficient
    /// storage or mutating a stream. This does not prepare a converter or prove
    /// native device support. Rates are finite positive values in matching units.
    pub fn validate_variable(
        input_rate: f64,
        output_rate: f64,
        taps: usize,
        maximum_adjustment_ppm: f64,
    ) -> Result<(), ResamplingError> {
        if !maximum_adjustment_ppm.is_finite()
            || maximum_adjustment_ppm <= 0.
            || maximum_adjustment_ppm > 10000.
        {
            return Err(ResamplingError);
        }
        Self::validate_rates(input_rate, output_rate, taps, maximum_adjustment_ppm)
    }
    fn validate_rates(
        input_rate: f64,
        output_rate: f64,
        taps: usize,
        ppm: f64,
    ) -> Result<(), ResamplingError> {
        if !input_rate.is_finite()
            || input_rate <= 0.
            || !output_rate.is_finite()
            || output_rate <= 0.
            || !(16..=256).contains(&taps)
            || !taps.is_multiple_of(2)
        {
            return Err(ResamplingError);
        }
        let step = input_rate / output_rate;
        if !(1. / 16. ..=16.).contains(&(step * (1. - ppm * 1e-6)))
            || !(1. / 16. ..=16.).contains(&(step * (1. + ppm * 1e-6)))
        {
            return Err(ResamplingError);
        }
        Ok(())
    }
    fn prepare(
        input_rate: f64,
        output_rate: f64,
        taps: usize,
        maximum_adjustment_ppm: f64,
    ) -> Result<Self, ResamplingError> {
        Self::validate_rates(input_rate, output_rate, taps, maximum_adjustment_ppm)?;
        let step = input_rate / output_rate;
        let maximum_step = step * (1. + maximum_adjustment_ppm * 1e-6);
        let cutoff = if maximum_adjustment_ppm == 0. {
            0.95 * (output_rate / input_rate).min(1.)
        } else {
            0.95 * (1. / maximum_step).min(1.)
        };
        let delay = (taps - 1) as f64 * 0.5;
        let mut coefficients = vec![0.; 1025 * taps];
        for phase in 0..=1024 {
            let fraction = phase as f64 / 1024.;
            let row = &mut coefficients[phase * taps..(phase + 1) * taps];
            let mut sum = 0.;
            for (tap, coefficient) in row.iter_mut().enumerate() {
                let x = (tap as f64 - delay - fraction) * cutoff;
                let sinc = if x.abs() < 1e-12 {
                    1.
                } else {
                    (std::f64::consts::PI * x).sin() / (std::f64::consts::PI * x)
                };
                let angle = std::f64::consts::TAU * tap as f64 / (taps - 1) as f64;
                let window = 0.42 - 0.5 * angle.cos() + 0.08 * (2. * angle).cos();
                *coefficient = cutoff * sinc * window;
                sum += *coefficient;
            }
            if !sum.is_finite() || sum.abs() < 1e-12 {
                return Err(ResamplingError);
            }
            for coefficient in row {
                *coefficient /= sum;
            }
        }
        Ok(Self {
            step,
            nominal_step: step,
            maximum_adjustment_ppm,
            distance: 1.,
            has_input: false,
            history: vec![0.; taps],
            coefficients,
            write: 0,
        })
    }
    /// Change future output spacing without resetting history or pending phase.
    /// Positive ppm consumes input faster per output sample. The next output's
    /// already scheduled position is retained; new spacing applies afterward.
    /// Reject before mutation. Fixed constructor accepts only zero adjustment.
    /// No smoothing/estimation is implicit; this operation allocates nothing.
    pub fn set_rate_adjustment_ppm(&mut self, ppm: f64) -> Result<(), ResamplingError> {
        if !ppm.is_finite() || ppm.abs() > self.maximum_adjustment_ppm {
            return Err(ResamplingError);
        }
        self.step = self.nominal_step * (1. + ppm * 1e-6);
        Ok(())
    }
    /// Current input samples per output sample, including live adjustment.
    pub fn rate_step(&self) -> f64 {
        self.step
    }
    pub fn maximum_adjustment_ppm(&self) -> f64 {
        self.maximum_adjustment_ppm
    }
    pub fn latency_input_samples(&self) -> f64 {
        (self.history.len() - 1) as f64 * 0.5
    }
    /// Clear stream state and zero history without reallocating coefficients.
    /// Retain the current rate adjustment; set zero explicitly to return nominal.
    pub fn reset(&mut self) {
        self.distance = 1.;
        self.has_input = false;
        self.write = 0;
        self.history.fill(0.);
    }
    /// Consume/produce bounded prefixes, retaining pending output internally.
    /// Zero output capacity consumes nothing. Zero input may drain pending output.
    /// Output beyond `produced` is untouched. To flush a finite stream, supply
    /// explicit trailing zeros; this method does not invent input or final padding.
    /// Nonfinite audio propagates. Fixed construction retains its rate; the
    /// variable profile accepts only its prepared bounded ppm adjustments.
    pub fn process(&mut self, input: &[f64], output: &mut [f64]) -> ResamplingProgress {
        let mut progress = ResamplingProgress::default();
        let taps = self.history.len();
        while progress.produced < output.len() {
            if !self.has_input || self.distance > 0. {
                if progress.consumed == input.len() {
                    break;
                }
                self.history[self.write] = input[progress.consumed];
                self.write = (self.write + 1) % taps;
                self.distance -= 1.;
                self.has_input = true;
                progress.consumed += 1;
                continue;
            }
            let position = (-self.distance).clamp(0., 1.) * 1024.;
            let phase = (position.floor() as usize).min(1023);
            let fraction = position - phase as f64;
            let mut sample = 0.;
            for tap in 0..taps {
                let a = self.coefficients[phase * taps + tap];
                let b = self.coefficients[(phase + 1) * taps + tap];
                let value = self.history[(self.write + taps - 1 - tap) % taps];
                sample += value * (a + fraction * (b - a));
            }
            output[progress.produced] = sample;
            progress.produced += 1;
            self.distance += self.step;
        }
        progress
    }
}
