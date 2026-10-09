use crate::{Complex, FftError, FftPlan};

/// Real-transform validation errors occur before caller storage is modified.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RealFftError {
    Transform(FftError),
    /// DC and (when present) Nyquist imaginary components must equal zero.
    InvalidEndpoints,
}
impl std::fmt::Display for RealFftError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Transform(e) => e.fmt(f),
            Self::InvalidEndpoints => {
                f.write_str("real spectrum endpoints must have zero imaginary components")
            }
        }
    }
}
impl std::error::Error for RealFftError {}
impl From<FftError> for RealFftError {
    fn from(error: FftError) -> Self {
        Self::Transform(error)
    }
}

/// Prepared real f64 transform with N/2+1 positive-frequency complex bins, DC
/// first and Nyquist last (N=1 has only DC). N is power-of-two 1..=1048576.
/// Internally uses the full complex FFT and N complex scratch values: this is
/// a convenient half-spectrum API, not an optimized real-only algorithm.
/// Construction allocates; forward/inverse reuse storage without heap work.
/// No windowing, SIMD, nonfinite sanitization or native deadline is implicit.
#[derive(Clone, Debug)]
pub struct RealFftPlan {
    fft: FftPlan,
    scratch: Vec<Complex>,
}
impl RealFftPlan {
    pub fn new(size: usize) -> Result<Self, RealFftError> {
        let fft = FftPlan::new(size)?;
        Ok(Self {
            fft,
            scratch: vec![Complex::default(); size],
        })
    }
    pub fn size(&self) -> usize {
        self.fft.size()
    }
    pub fn bins(&self) -> usize {
        self.size() / 2 + 1
    }
    /// Unnormalized exp(-i*w). Endpoints' imaginary components are explicitly
    /// zeroed, including nonfinite input. Other nonfinite values propagate.
    pub fn forward(&mut self, input: &[f64], output: &mut [Complex]) -> Result<(), RealFftError> {
        if input.len() != self.size() || output.len() != self.bins() {
            return Err(FftError::LengthMismatch.into());
        }
        for (scratch, input) in self.scratch.iter_mut().zip(input) {
            *scratch = Complex::new(*input, 0.);
        }
        self.fft.forward(&mut self.scratch)?;
        output.copy_from_slice(&self.scratch[..self.bins()]);
        output[0].im = 0.;
        if self.size() > 1 {
            output[self.size() / 2].im = 0.;
        }
        Ok(())
    }
    /// Reconstruct negative bins by conjugation, then exp(+i*w)/N. Requires
    /// exact zero imaginary endpoints (including rejecting NaN/Inf there);
    /// nonfinite real endpoints/interior bins propagate. Rejects before output
    /// mutation. Caller supplies N real output samples.
    pub fn inverse(&mut self, input: &[Complex], output: &mut [f64]) -> Result<(), RealFftError> {
        if input.len() != self.bins() || output.len() != self.size() {
            return Err(FftError::LengthMismatch.into());
        }
        if input[0].im != 0. || (self.size() > 1 && input[self.size() / 2].im != 0.) {
            return Err(RealFftError::InvalidEndpoints);
        }
        self.scratch[..input.len()].copy_from_slice(input);
        for (i, bin) in input.iter().enumerate().skip(1) {
            if i * 2 < self.size() {
                let mirrored = self.size() - i;
                self.scratch[mirrored] = Complex::new(bin.re, -bin.im);
            }
        }
        self.fft.inverse(&mut self.scratch)?;
        for (output, scratch) in output.iter_mut().zip(&self.scratch) {
            *output = scratch.re;
        }
        Ok(())
    }
}
