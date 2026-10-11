//! Prepared causal fractional-delay profiles with one temporal output per input.

/// Immutable reconstruction profile; all policies include a zero-delay bypass.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DelayInterpolation {
    /// Truncate fractional delay downward to a whole-sample tap.
    Integer,
    /// Two consecutive causal taps, with frequency-dependent magnitude droop.
    Linear,
    /// Four causal taps and a cubic Lagrange polynomial; can overshoot.
    Lagrange3rd,
    /// Recursive first-order Thiran allpass, unity magnitude for fixed delay.
    Thiran1st,
}

/// Failed capacity/allocation, delay edit or whole-block length preflight.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FractionalDelayError {
    Capacity,
    Allocation,
    InvalidDelay,
    LengthMismatch,
}
impl core::fmt::Display for FractionalDelayError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str(match self {
            Self::Capacity => "fractional delay history exceeds 32 MiB",
            Self::Allocation => "fractional delay history allocation failed",
            Self::InvalidDelay => {
                "delay is nonfinite, out of range or has unrepresentable Thiran stability"
            }
            Self::LengthMismatch => "fractional delay input/output lengths differ",
        })
    }
}
impl std::error::Error for FractionalDelayError {}

/// One-channel f64 causal fractional delay, initially zero, with immutable policy.
///
/// CONTROL construction owns `maximum_delay_samples+4` history values, at most
/// 32 MiB. Extra slots provide cubic support, without a hidden output delay.
/// Destruction releases this storage on CONTROL. Sample/block processing,
/// validated control edits and reset allocate/deallocate nothing and perform no
/// locks/I/O. Reset visits the full prepared ring; work is capacity-dependent.
/// Every channel requires its own independently owned instance.
///
/// For delay d=n+f, n=floor(d), `Integer` uses `x[t-n]`, `Linear` combines that
/// value with `x[t-n-1]`, and `Lagrange3rd` evaluates the four causal nodes
/// `x[t-n-j]`, j=0..3, at coordinate f on nodes 0..3. Cubic support is deliberately
/// one-sided near the newest source; no extra one-sample minimum is imposed.
/// Initial negative-index history is zero. Fractional FIR profiles are not
/// bandlimited and cubic can overshoot; no output clamp/normalization is imposed.
///
/// For `Thiran1st`, `v=x[t-n]` and a=(1-f)/(1+f) give
/// `y=a*v+previous_v-a*previous_y`. At fixed noninteger delay the transfer is
/// `z^-n*(a+z^-1)/(1+a*z^-1)`, with unity magnitude and DC group delay d.
/// Group delay varies with frequency; this is not a broadband ideal delay.
/// Small f has a pole close to -1 and an arbitrarily long alternating tail.
/// Nonzero f whose f64 coefficient rounds to one rejects before any edit.
/// Integer f=0 explicitly bypasses recursion and updates both prior values.
///
/// Exactly one output/history advancement per input sample avoids ambiguous
/// repeated stateful tap reads. Immediate delay edits preserve history without
/// smoothing/crossfades; integer-branch changes retap the source feeding retained
/// Thiran history. Static allpass facts do not describe modulated delay/feedback.
/// Nonfinite input can poison history until reset; callers own headroom,
/// denormals, feedback, tail/drain and host latency/deadline policy.
#[derive(Debug)]
pub struct FractionalDelay {
    samples: Vec<f64>,
    cursor: usize,
    maximum: usize,
    interpolation: DelayInterpolation,
    delay: f64,
    whole: usize,
    fraction: f64,
    coefficient: f64,
    previous_input: f64,
    previous_output: f64,
}
impl FractionalDelay {
    /// CONTROL preparation: maximum 0..=4,194,300 source samples. Validate all
    /// capacity arithmetic before fallible reservation; zero maximum is legal.
    pub fn new(
        maximum_delay_samples: usize,
        interpolation: DelayInterpolation,
    ) -> Result<Self, FractionalDelayError> {
        let length = maximum_delay_samples
            .checked_add(4)
            .filter(|&length| length <= 4_194_304)
            .ok_or(FractionalDelayError::Capacity)?;
        let mut samples = Vec::new();
        samples
            .try_reserve_exact(length)
            .map_err(|_| FractionalDelayError::Allocation)?;
        samples.resize(length, 0.);
        Ok(Self {
            samples,
            cursor: 0,
            maximum: maximum_delay_samples,
            interpolation,
            delay: 0.,
            whole: 0,
            fraction: 0.,
            coefficient: 0.,
            previous_input: 0.,
            previous_output: 0.,
        })
    }
    pub fn maximum_delay_samples(&self) -> usize {
        self.maximum
    }
    pub fn interpolation(&self) -> DelayInterpolation {
        self.interpolation
    }
    pub fn delay_samples(&self) -> f64 {
        self.delay
    }

    /// Finite delay 0..=prepared maximum; immediate history-preserving retune.
    /// Invalid bounds or unrepresentable noninteger Thiran coefficient preserve
    /// requested delay, ring/cursor and recursive history. No allocation.
    pub fn set_delay_samples(&mut self, delay: f64) -> Result<(), FractionalDelayError> {
        if !delay.is_finite() || delay < 0. || delay > self.maximum as f64 {
            return Err(FractionalDelayError::InvalidDelay);
        }
        let whole = delay.floor() as usize;
        let fraction = delay - whole as f64;
        let coefficient = (1. - fraction) / (1. + fraction);
        if self.interpolation == DelayInterpolation::Thiran1st && fraction > 0. && coefficient >= 1.
        {
            return Err(FractionalDelayError::InvalidDelay);
        }
        self.delay = delay;
        self.whole = whole;
        self.fraction = fraction;
        self.coefficient = coefficient;
        Ok(())
    }
    fn read(&self, distance: usize) -> f64 {
        let index = if self.cursor >= distance {
            self.cursor - distance
        } else {
            self.samples.len() - (distance - self.cursor)
        };
        self.samples[index]
    }
    /// Write current input, reconstruct once, then advance once. Zero delay
    /// returns current input exactly. Nonfinite input propagates; no implicit
    /// sanitation, extra read/pop or flush. No heap/locks/I/O.
    pub fn process_sample(&mut self, input: f64) -> f64 {
        self.samples[self.cursor] = input;
        let v = self.read(self.whole);
        let f = self.fraction;
        let output = if f == 0. || self.interpolation == DelayInterpolation::Integer {
            v
        } else {
            match self.interpolation {
                DelayInterpolation::Integer => unreachable!(),
                DelayInterpolation::Linear => (1. - f) * v + f * self.read(self.whole + 1),
                DelayInterpolation::Lagrange3rd => {
                    -v * (f - 1.) * (f - 2.) * (f - 3.) / 6.
                        + self.read(self.whole + 1) * f * (f - 2.) * (f - 3.) / 2.
                        - self.read(self.whole + 2) * f * (f - 1.) * (f - 3.) / 2.
                        + self.read(self.whole + 3) * f * (f - 1.) * (f - 2.) / 6.
                }
                DelayInterpolation::Thiran1st => {
                    self.coefficient * v + self.previous_input
                        - self.coefficient * self.previous_output
                }
            }
        };
        if self.interpolation == DelayInterpolation::Thiran1st {
            self.previous_input = v;
            self.previous_output = output;
        }
        self.cursor += 1;
        if self.cursor == self.samples.len() {
            self.cursor = 0;
        }
        output
    }
    pub fn process_in_place(&mut self, samples: &mut [f64]) {
        for sample in samples {
            *sample = self.process_sample(*sample);
        }
    }
    /// Exact equal lengths required; a mismatch preserves all stream state and
    /// output. Empty slices are a no-op. Only caller-owned blocks are processed.
    ///
    /// ```
    /// use ravet_signal::{DelayInterpolation, FractionalDelay};
    /// let mut delay = FractionalDelay::new(8, DelayInterpolation::Linear).unwrap();
    /// delay.set_delay_samples(1.5).unwrap();
    /// let mut output = [99.; 4];
    /// delay.process_separate(&[1., 0., 0., 0.], &mut output).unwrap();
    /// assert_eq!(output, [0., 0.5, 0.5, 0.]);
    /// ```
    pub fn process_separate(
        &mut self,
        input: &[f64],
        output: &mut [f64],
    ) -> Result<(), FractionalDelayError> {
        if input.len() != output.len() {
            return Err(FractionalDelayError::LengthMismatch);
        }
        for (&input, output) in input.iter().zip(output) {
            *output = self.process_sample(input);
        }
        Ok(())
    }
    /// Clear ring/cursor/recursive history, retaining controls/allocation.
    /// Visits the full prepared capacity; no heap/locks/I/O.
    pub fn reset(&mut self) {
        self.samples.fill(0.);
        self.cursor = 0;
        self.previous_input = 0.;
        self.previous_output = 0.;
    }
}
