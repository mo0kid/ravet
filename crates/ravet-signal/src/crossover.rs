//! Fixed-storage fourth-order Linkwitz–Riley electrical crossover.
use crate::{BiquadCascade, BiquadCoefficients, CascadeCoefficients, FilterError, FilterKind};

/// Prepared LR4 low/high branches, each two identical second-order Butterworth
/// sections. Both branches have amplitude 0.5 at cutoff. Their sum has unity
/// magnitude with all-pass phase; it does not reproduce the input waveform.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LinkwitzRileyCoefficients {
    low: CascadeCoefficients<2>,
    high: CascadeCoefficients<2>,
}
impl LinkwitzRileyCoefficients {
    /// Control-thread design at finite positive rate and cutoff strictly between
    /// DC and Nyquist. Unrepresentable stability margins reject. No allocation.
    pub fn design(sample_rate: f64, cutoff: f64) -> Result<Self, FilterError> {
        let q = std::f64::consts::FRAC_1_SQRT_2;
        let low = BiquadCoefficients::design(FilterKind::LowPass, sample_rate, cutoff, q)?;
        let high = BiquadCoefficients::design(FilterKind::HighPass, sample_rate, cutoff, q)?;
        Ok(Self {
            low: CascadeCoefficients::new([low; 2])?,
            high: CascadeCoefficients::new([high; 2])?,
        })
    }
    /// Control-thread linear amplitudes at normalized frequency 0..=0.5.
    pub fn magnitudes_at(self, cycles_per_sample: f64) -> Result<CrossoverOutputs, FilterError> {
        Ok(CrossoverOutputs {
            low: self.low.magnitude_at(cycles_per_sample)?,
            high: self.high.magnitude_at(cycles_per_sample)?,
        })
    }
}

/// Low/high samples or linear response amplitudes of the two crossover branches.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct CrossoverOutputs {
    pub low: f64,
    pub high: f64,
}

/// Input, low output and high output must have exactly equal lengths.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CrossoverLengthError;
impl std::fmt::Display for CrossoverLengthError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("crossover input and output lengths differ")
    }
}
impl std::error::Error for CrossoverLengthError {}

/// One channel of f64 LR4 processing with four fixed-storage biquads. Consumers
/// own channel instances, headroom, denormals and callback deadlines. Nonfinite
/// audio propagates and may poison history until reset. This electrical profile
/// supplies no loudspeaker alignment, automatic routing or crossover modulation.
#[derive(Clone, Debug)]
pub struct LinkwitzRileyCrossover {
    low: BiquadCascade<2>,
    high: BiquadCascade<2>,
}
impl LinkwitzRileyCrossover {
    /// Construct zeroed state from a complete validated control-thread design.
    pub fn new(coefficients: LinkwitzRileyCoefficients) -> Self {
        Self {
            low: BiquadCascade::new(coefficients.low),
            high: BiquadCascade::new(coefficients.high),
        }
    }
    /// Replace both branches while retaining all histories, without smoothing
    /// or a click-free transition. Designing a new cutoff is a separate step.
    pub fn set_coefficients(&mut self, coefficients: LinkwitzRileyCoefficients) {
        self.low.set_coefficients(coefficients.low);
        self.high.set_coefficients(coefficients.high);
    }
    /// Clear all four histories without allocation or coefficient changes.
    pub fn reset(&mut self) {
        self.low.reset();
        self.high.reset();
    }
    /// Split one sample without allocating or releasing heap storage.
    pub fn process_sample(&mut self, sample: f64) -> CrossoverOutputs {
        CrossoverOutputs {
            low: self.low.process_sample(sample),
            high: self.high.process_sample(sample),
        }
    }
    /// Split equal-length buffers. A mismatch leaves both output buffers and
    /// every history unchanged. Empty buffers are a valid no-op. No allocation.
    pub fn process_block(
        &mut self,
        input: &[f64],
        low: &mut [f64],
        high: &mut [f64],
    ) -> Result<(), CrossoverLengthError> {
        if input.len() != low.len() || input.len() != high.len() {
            return Err(CrossoverLengthError);
        }
        for ((sample, low), high) in input.iter().zip(low).zip(high) {
            let outputs = self.process_sample(*sample);
            *low = outputs.low;
            *high = outputs.high;
        }
        Ok(())
    }
}
