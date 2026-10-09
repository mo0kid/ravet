//! Prepared radix-two complex FFT with caller-owned transform storage.

/// Double-precision complex value used by the FFT; no heap storage.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Complex {
    pub re: f64,
    pub im: f64,
}
impl Complex {
    pub const fn new(re: f64, im: f64) -> Self {
        Self { re, im }
    }
    fn multiply(self, other: Self) -> Self {
        Self::new(
            self.re * other.re - self.im * other.im,
            self.re * other.im + self.im * other.re,
        )
    }
}

/// Preparation/transform failures reject before modifying the input buffer.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FftError {
    InvalidSize,
    LengthMismatch,
}

/// Prepared complex radix-two FFT, 1..=1,048,576 points.
/// Construction allocates twiddles; transforms allocate/deallocate nothing.
/// Forward uses exp(-i*w), inverse exp(+i*w) normalized by 1/N.
/// Nonfinite samples propagate; no real-only packing, windowing or SIMD policy.
#[derive(Clone, Debug)]
pub struct FftPlan {
    size: usize,
    twiddles: Vec<Complex>,
}
impl FftPlan {
    pub fn new(size: usize) -> Result<Self, FftError> {
        if !size.is_power_of_two() || size > 1_048_576 {
            return Err(FftError::InvalidSize);
        }
        let twiddles = (0..size / 2)
            .map(|index| {
                let phase = -std::f64::consts::TAU * index as f64 / size as f64;
                Complex::new(phase.cos(), phase.sin())
            })
            .collect();
        Ok(Self { size, twiddles })
    }
    pub fn size(&self) -> usize {
        self.size
    }
    pub fn forward(&self, samples: &mut [Complex]) -> Result<(), FftError> {
        self.transform(samples, false)
    }
    pub fn inverse(&self, samples: &mut [Complex]) -> Result<(), FftError> {
        self.transform(samples, true)
    }
    fn transform(&self, samples: &mut [Complex], inverse: bool) -> Result<(), FftError> {
        if samples.len() != self.size {
            return Err(FftError::LengthMismatch);
        }
        if self.size == 1 {
            return Ok(());
        }
        let shift = usize::BITS - self.size.trailing_zeros();
        for index in 0..self.size {
            let reversed = index.reverse_bits() >> shift;
            if reversed > index {
                samples.swap(index, reversed);
            }
        }
        let mut width = 2;
        while width <= self.size {
            let half = width / 2;
            let stride = self.size / width;
            for group in samples.chunks_exact_mut(width) {
                for index in 0..half {
                    let mut twiddle = self.twiddles[index * stride];
                    if inverse {
                        twiddle.im = -twiddle.im;
                    }
                    let right = group[index + half].multiply(twiddle);
                    let left = group[index];
                    group[index] = Complex::new(left.re + right.re, left.im + right.im);
                    group[index + half] = Complex::new(left.re - right.re, left.im - right.im);
                }
            }
            width *= 2;
        }
        if inverse {
            for value in samples {
                value.re /= self.size as f64;
                value.im /= self.size as f64;
            }
        }
        Ok(())
    }
}
impl std::fmt::Display for FftError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::InvalidSize => "FFT size must be a power of two from 1 to 1,048,576",
            Self::LengthMismatch => "FFT buffer length does not match its plan",
        })
    }
}
impl std::error::Error for FftError {}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn agrees_with_direct_complex_dft_and_inverse() {
        for size in [1, 2, 4, 8, 32, 128] {
            let plan = FftPlan::new(size).unwrap();
            let original: Vec<_> = (0..size)
                .map(|i| Complex::new((i as f64 * 0.7).sin(), (i as f64 * 0.3).cos()))
                .collect();
            let mut samples = original.clone();
            plan.forward(&mut samples).unwrap();
            for (k, actual) in samples.iter().enumerate() {
                let mut expected = Complex::default();
                for (n, sample) in original.iter().enumerate() {
                    let phase = -std::f64::consts::TAU * k as f64 * n as f64 / size as f64;
                    let term = sample.multiply(Complex::new(phase.cos(), phase.sin()));
                    expected.re += term.re;
                    expected.im += term.im;
                }
                assert!((actual.re - expected.re).abs() < 1e-10);
                assert!((actual.im - expected.im).abs() < 1e-10);
            }
            plan.inverse(&mut samples).unwrap();
            for (actual, expected) in samples.iter().zip(original) {
                assert!((actual.re - expected.re).abs() < 1e-12);
                assert!((actual.im - expected.im).abs() < 1e-12);
            }
        }
    }
    #[test]
    fn mismatched_transform_is_atomic() {
        for size in [0, 3, 1_048_577] {
            assert_eq!(FftPlan::new(size).unwrap_err(), FftError::InvalidSize);
        }
        let mut samples = [Complex::new(1., 2.); 3];
        assert_eq!(
            FftPlan::new(4).unwrap().forward(&mut samples),
            Err(FftError::LengthMismatch)
        );
        assert_eq!(samples, [Complex::new(1., 2.); 3]);
    }
}
