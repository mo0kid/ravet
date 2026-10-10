//! Prepared fixed-delay feedforward and feedback comb filters.
use crate::DelayLine;

/// Selects the signal retained in a comb's delay history.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CombKind {
    /// `y[n] = x[n] + g*x[n-D]`; transfer function `1 + g*z^-D`.
    Feedforward,
    /// `y[n] = x[n] + g*y[n-D]`; transfer function `1/(1 - g*z^-D)`.
    Feedback,
}

/// Invalid comb preparation or coefficient replacement.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CombError {
    /// Delay must be 1..=1048576 whole samples.
    InvalidDelay,
    /// Coefficients must be finite; feedback additionally requires `|g| < 1`.
    InvalidCoefficient,
}

impl std::fmt::Display for CombError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::InvalidDelay => "comb delay must be 1..=1048576 samples",
            Self::InvalidCoefficient => {
                "comb coefficient must be finite, with feedback magnitude strictly below one"
            }
        })
    }
}
impl std::error::Error for CombError {}

/// Prepared one-channel f64 comb with a fixed whole-sample delay.
///
/// Both kinds include the current input at unity gain. Positive and negative
/// coefficients select reinforcing and cancelling delayed contributions.
/// Construction owns one delay allocation of at most 8 MiB, excluding overhead;
/// sample/block processing, coefficient edits and reset allocate/release nothing.
/// Type and delay changes require a new prepared instance. Instantiate separately
/// for each channel. Edits retain history and apply immediately without smoothing.
///
/// No damping, fractional delay, normalization, input sanitation, denormal or
/// native callback-deadline policy is supplied. Nonfinite/extreme finite audio
/// can poison/overflow history; reset clears it. Feedback near unity can require
/// substantial headroom and have a long tail even though `|g| < 1`.
///
/// ```
/// use ravet_signal::{CombFilter, CombKind};
/// let mut comb = CombFilter::new(CombKind::Feedback, 2, 0.5)?;
/// let mut impulse = [1.0, 0.0, 0.0, 0.0, 0.0];
/// comb.process_in_place(&mut impulse);
/// assert_eq!(impulse, [1.0, 0.0, 0.5, 0.0, 0.25]);
/// # Ok::<(), ravet_signal::CombError>(())
/// ```
#[derive(Debug)]
pub struct CombFilter {
    kind: CombKind,
    delay_samples: usize,
    coefficient: f64,
    delay: DelayLine,
}

impl CombFilter {
    /// Validate all arguments before allocating zeroed delay history.
    /// Delay is 1..=1048576 samples; feedforward accepts any finite coefficient,
    /// while feedback requires a finite coefficient strictly between -1 and 1.
    pub fn new(kind: CombKind, delay_samples: usize, coefficient: f64) -> Result<Self, CombError> {
        if !(1..=1048576).contains(&delay_samples) {
            return Err(CombError::InvalidDelay);
        }
        validate_coefficient(kind, coefficient)?;
        Ok(Self {
            kind,
            delay_samples,
            coefficient,
            delay: DelayLine::new(delay_samples).expect("validated positive comb delay"),
        })
    }

    pub fn kind(&self) -> CombKind {
        self.kind
    }

    /// Exact prepared delay length in whole samples.
    pub fn delay_samples(&self) -> usize {
        self.delay_samples
    }

    pub fn coefficient(&self) -> f64 {
        self.coefficient
    }

    /// Reject invalid coefficients before mutation; retain all history.
    /// No allocation or smoothing is performed.
    pub fn set_coefficient(&mut self, coefficient: f64) -> Result<(), CombError> {
        validate_coefficient(self.kind, coefficient)?;
        self.coefficient = coefficient;
        Ok(())
    }

    /// Read the D-sample history before writing and advance exactly once.
    /// A zero coefficient returns input exactly while continuing history.
    pub fn process_sample(&mut self, input: f64) -> f64 {
        let delayed = self.delay.read_integer(0);
        let output = if self.coefficient == 0.0 {
            input
        } else {
            input + self.coefficient * delayed
        };
        self.delay.write(match self.kind {
            CombKind::Feedforward => input,
            CombKind::Feedback => output,
        });
        self.delay.advance();
        output
    }

    /// Process a caller-owned block using prepared history; empty blocks do
    /// nothing and arbitrary partitions preserve sample-processing behavior.
    pub fn process_in_place(&mut self, samples: &mut [f64]) {
        for sample in samples {
            *sample = self.process_sample(*sample);
        }
    }

    /// Clear history and cursor, retaining kind, delay, coefficient and allocation.
    pub fn reset(&mut self) {
        self.delay.reset();
    }
}

fn validate_coefficient(kind: CombKind, coefficient: f64) -> Result<(), CombError> {
    if !coefficient.is_finite() || (kind == CombKind::Feedback && coefficient.abs() >= 1.0) {
        return Err(CombError::InvalidCoefficient);
    }
    Ok(())
}
