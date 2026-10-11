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

/// Fallible plan preparation failures; distinct from unchanged transform errors.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FftPreparationError {
    /// Size is not a power of two in 1..=1,048,576.
    InvalidSize,
    /// Requested or actual twiddle capacity exceeds the byte budget.
    Capacity,
    /// The allocator refused the fallible twiddle reservation.
    Allocation,
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
        let twiddles = Self::fill_twiddles(size, Vec::with_capacity(size / 2));
        Ok(Self { size, twiddles })
    }
    /// Fallible CONTROL preparation with an explicit twiddle-storage byte bound.
    /// Numeric Vec capacity is included; allocator overhead and caller transform
    /// buffers are excluded. Size 1 requires no twiddle bytes and admits budget 0.
    /// Invalid size/budget refuses before allocation or trigonometric preparation.
    /// The legacy `new` constructor retains its infallible allocation policy.
    pub fn try_new(size: usize, maximum_twiddle_bytes: usize) -> Result<Self, FftPreparationError> {
        if !size.is_power_of_two() || size > 1_048_576 {
            return Err(FftPreparationError::InvalidSize);
        }
        let count = size / 2;
        check_fft_storage::<Complex>(count, maximum_twiddle_bytes)?;
        let mut twiddles = Vec::new();
        twiddles
            .try_reserve_exact(count)
            .map_err(|_| FftPreparationError::Allocation)?;
        check_fft_storage::<Complex>(twiddles.capacity(), maximum_twiddle_bytes)?;
        let twiddles = Self::fill_twiddles(size, twiddles);
        Ok(Self { size, twiddles })
    }
    fn fill_twiddles(size: usize, mut twiddles: Vec<Complex>) -> Vec<Complex> {
        // Both constructors reserve the complete count before coefficient math.
        for index in 0..size / 2 {
            let phase = -std::f64::consts::TAU * index as f64 / size as f64;
            twiddles.push(Complex::new(phase.cos(), phase.sin()));
        }
        twiddles
    }
    /// Numeric twiddle Vec capacity only, excluding allocator/control overhead.
    pub fn twiddle_storage_bytes(&self) -> usize {
        self.twiddles.capacity() * size_of::<Complex>()
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
impl core::fmt::Display for FftPreparationError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str(match self {
            Self::InvalidSize => "FFT size must be a power of two from 1 to 1,048,576",
            Self::Capacity => "FFT twiddle storage exceeds its numeric byte budget",
            Self::Allocation => "FFT twiddle storage allocation failed",
        })
    }
}
impl core::error::Error for FftPreparationError {}

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

// Used both before reservation and on actual allocated Vec capacity.
pub(crate) fn check_fft_storage<T>(
    count: usize,
    budget: usize,
) -> Result<usize, FftPreparationError> {
    let bytes = count
        .checked_mul(size_of::<T>())
        .ok_or(FftPreparationError::Capacity)?;
    if bytes > budget {
        Err(FftPreparationError::Capacity)
    } else {
        Ok(bytes)
    }
}
#[cfg(test)]
mod fallible_tests {
    use super::*;
    #[test]
    fn actual_overallocated_capacity_rejects_and_legacy_coefficients_match() {
        let v = Vec::<Complex>::with_capacity(17);
        assert_eq!(
            check_fft_storage::<Complex>(v.capacity(), 16 * size_of::<Complex>()),
            Err(FftPreparationError::Capacity)
        );
        assert_eq!(
            check_fft_storage::<Complex>(usize::MAX, usize::MAX),
            Err(FftPreparationError::Capacity)
        );
        for size in [1, 2, 4, 16, 128, 4096, 1_048_576] {
            let legacy = FftPlan::new(size).unwrap();
            let bounded = FftPlan::try_new(size, size / 2 * size_of::<Complex>()).unwrap();
            assert_eq!(legacy.twiddles.len(), bounded.twiddles.len());
            for (a, b) in legacy.twiddles.iter().zip(&bounded.twiddles) {
                assert_eq!(
                    (a.re.to_bits(), a.im.to_bits()),
                    (b.re.to_bits(), b.im.to_bits())
                );
            }
        }
    }
}
