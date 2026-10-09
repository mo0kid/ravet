/// Prepared cosine-window families. No post-sampling peak normalization: an
/// even symmetric window may have maximum below one.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WindowKind {
    Rectangular,
    Hann,
    Hamming,
    Blackman,
    BlackmanHarris,
}
/// Symmetric uses N-1 for filter design; periodic uses N for analysis.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WindowConvention {
    Symmetric,
    Periodic,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WindowError {
    InvalidSize,
    LengthMismatch,
}
impl std::fmt::Display for WindowError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::InvalidSize => "window size must be 1..=1048576",
            Self::LengthMismatch => "window and sample lengths must match",
        })
    }
}
impl std::error::Error for WindowError {}

/// Owned f64 window coefficients for 1..=1048576 samples. Construction evaluates
/// cosines/allocates on a control thread; application allocates nothing. N=1 is
/// unity for every family/convention. Tiny negative rounded taps are retained.
/// Coherent gain and energy are computed from actual coefficients; zero-sum
/// windows return no equivalent noise bandwidth. No automatic gain correction,
/// overlap/add policy, nonfinite sanitization or native deadline guarantee.
#[derive(Clone, Debug)]
pub struct WindowPlan {
    taps: Vec<f64>,
    sum: f64,
    energy: f64,
}
impl WindowPlan {
    pub fn new(
        kind: WindowKind,
        convention: WindowConvention,
        size: usize,
    ) -> Result<Self, WindowError> {
        if !(1..=1048576).contains(&size) {
            return Err(WindowError::InvalidSize);
        }
        let coefficients = match kind {
            WindowKind::Rectangular => [1., 0., 0., 0.],
            WindowKind::Hann => [0.5, -0.5, 0., 0.],
            WindowKind::Hamming => [0.54, -0.46, 0., 0.],
            WindowKind::Blackman => [0.42, -0.5, 0.08, 0.],
            WindowKind::BlackmanHarris => [0.35875, -0.48829, 0.14128, -0.01168],
        };
        let denominator = match convention {
            WindowConvention::Symmetric => size - 1,
            WindowConvention::Periodic => size,
        };
        let taps: Vec<f64> = (0..size)
            .map(|i| {
                if size == 1 {
                    return 1.;
                }
                let phase = std::f64::consts::TAU * i as f64 / denominator as f64;
                coefficients
                    .iter()
                    .enumerate()
                    .map(|(harmonic, coefficient)| coefficient * (harmonic as f64 * phase).cos())
                    .sum()
            })
            .collect();
        let sum = taps.iter().sum();
        let energy = taps.iter().map(|x| x * x).sum();
        Ok(Self { taps, sum, energy })
    }
    pub fn coefficients(&self) -> &[f64] {
        &self.taps
    }
    /// Mean coefficient, for coherent-bin amplitude correction on the caller.
    pub fn coherent_gain(&self) -> f64 {
        self.sum / self.taps.len() as f64
    }
    /// Sum of squared coefficients; not mean square.
    pub fn energy(&self) -> f64 {
        self.energy
    }
    /// N*energy/sum^2 in FFT-bin units, or None for exact zero sum.
    pub fn equivalent_noise_bandwidth(&self) -> Option<f64> {
        (self.sum != 0.).then(|| self.taps.len() as f64 * self.energy / (self.sum * self.sum))
    }
    /// Exact length required, rejected before mutation. Nonfinite audio propagates.
    pub fn apply(&self, samples: &mut [f64]) -> Result<(), WindowError> {
        if samples.len() != self.taps.len() {
            return Err(WindowError::LengthMismatch);
        }
        for (sample, tap) in samples.iter_mut().zip(&self.taps) {
            *sample *= tap;
        }
        Ok(())
    }
}
