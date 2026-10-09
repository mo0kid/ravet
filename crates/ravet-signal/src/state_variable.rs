use crate::FilterError;

/// Prepared linear trapezoidal state-variable coefficients for one channel.
/// Rate must be finite/positive, cutoff/rate in 1e-6..=0.499999 and Q in
/// 0.1..=100. Design performs transcendental work on the control thread.
/// These deliberately bounded ratios avoid numerically degenerate endpoints.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct StateVariableCoefficients {
    a1: f64,
    a2: f64,
    a3: f64,
    damping: f64,
}
impl StateVariableCoefficients {
    pub fn new(sample_rate: f64, cutoff: f64, q: f64) -> Result<Self, FilterError> {
        let normalized = cutoff / sample_rate;
        if !sample_rate.is_finite()
            || sample_rate <= 0.
            || !cutoff.is_finite()
            || !(1e-6..=0.499999).contains(&normalized)
            || !q.is_finite()
            || !(0.1..=100.).contains(&q)
        {
            return Err(FilterError);
        }
        let g = (std::f64::consts::PI * normalized).tan();
        let damping = 1. / q;
        let a1 = 1. / (1. + g * (g + damping));
        Ok(Self {
            a1,
            a2: g * a1,
            a3: g * g * a1,
            damping,
        })
    }
}

/// Simultaneous outputs from the same state update. Band-pass is the raw
/// integrator output: its gain at cutoff equals Q, rather than unity.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct StateVariableOutputs {
    pub low_pass: f64,
    pub band_pass: f64,
    pub high_pass: f64,
}

/// One-channel f64 trapezoidal state-variable filter. Processing and reset
/// allocate nothing. Coefficient replacement preserves integrator histories;
/// callers supply any desired control smoothing. No nonlinear saturation,
/// self-oscillation, denormal policy, arbitrary modulation stability, nonfinite
/// audio sanitization or callback deadline guarantee is imposed.
#[derive(Clone, Debug)]
pub struct StateVariableFilter {
    coefficients: StateVariableCoefficients,
    state: [f64; 2],
}
impl StateVariableFilter {
    pub fn new(coefficients: StateVariableCoefficients) -> Self {
        Self {
            coefficients,
            state: [0.; 2],
        }
    }
    /// Retain both integrator states while replacing prepared coefficients.
    pub fn set_coefficients(&mut self, coefficients: StateVariableCoefficients) {
        self.coefficients = coefficients;
    }
    /// Clear histories while retaining coefficients.
    pub fn reset(&mut self) {
        self.state = [0.; 2];
    }
    /// Update once and return all three outputs. Nonfinite input propagates and
    /// may poison histories until reset; extreme finite amplitudes may overflow.
    pub fn process_sample(&mut self, input: f64) -> StateVariableOutputs {
        let c = self.coefficients;
        let difference = input - self.state[1];
        let band = c.a1 * self.state[0] + c.a2 * difference;
        let low = self.state[1] + c.a2 * self.state[0] + c.a3 * difference;
        self.state = [2. * band - self.state[0], 2. * low - self.state[1]];
        StateVariableOutputs {
            low_pass: low,
            band_pass: band,
            high_pass: input - c.damping * band - low,
        }
    }
}
