//! Prepared native single-precision radix-two complex FFT.
use crate::fft::check_fft_storage;
use crate::{FftError, FftPreparationError};

/// Single-precision complex value used by the native f32 FFT; no heap storage.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Complex32 {
    /// Real part.
    pub re: f32,
    /// Imaginary part.
    pub im: f32,
}
impl Complex32 {
    /// Construct from real and imaginary parts.
    pub const fn new(re: f32, im: f32) -> Self {
        Self { re, im }
    }
    fn multiply(self, other: Self) -> Self {
        Self::new(
            self.re * other.re - self.im * other.im,
            self.re * other.im + self.im * other.re,
        )
    }
}

/// Prepared complex radix-two FFT, 1..=1,048,576 points.
/// Fallible construction allocates native f32 twiddles after budget admission;
/// phase/cos/sin use f64 on CONTROL followed by explicit f32 quantization.
/// Transform butterflies, products, sums and inverse scaling execute in f32.
/// Transforms allocate/deallocate nothing.
/// Forward uses exp(-i*w), inverse exp(+i*w) normalized by 1/N.
/// Nonfinite samples propagate; no real-only packing, windowing or SIMD policy.
#[derive(Clone, Debug)]
pub struct FftPlan32 {
    size: usize,
    twiddles: Vec<Complex32>,
}
impl FftPlan32 {
    /// Fallible CONTROL preparation with an explicit twiddle-storage byte bound.
    /// Numeric Vec capacity is included; allocator overhead and caller transform
    /// buffers are excluded. Size 1 requires no twiddle bytes and admits budget 0.
    /// Invalid size/budget refuses before allocation or trigonometric preparation.
    pub fn try_new(size: usize, maximum_twiddle_bytes: usize) -> Result<Self, FftPreparationError> {
        if !size.is_power_of_two() || size > 1_048_576 {
            return Err(FftPreparationError::InvalidSize);
        }
        let count = size / 2;
        check_fft_storage::<Complex32>(count, maximum_twiddle_bytes)?;
        let mut twiddles = Vec::new();
        twiddles
            .try_reserve_exact(count)
            .map_err(|_| FftPreparationError::Allocation)?;
        check_fft_storage::<Complex32>(twiddles.capacity(), maximum_twiddle_bytes)?;
        let twiddles = Self::fill_twiddles(size, twiddles);
        Ok(Self { size, twiddles })
    }
    fn fill_twiddles(size: usize, mut twiddles: Vec<Complex32>) -> Vec<Complex32> {
        // Reserve the complete count before coefficient math.
        for index in 0..size / 2 {
            let phase = -std::f64::consts::TAU * index as f64 / size as f64;
            twiddles.push(Complex32::new(phase.cos() as f32, phase.sin() as f32));
        }
        twiddles
    }
    /// Numeric twiddle Vec capacity only, excluding allocator/control overhead.
    pub fn twiddle_storage_bytes(&self) -> usize {
        self.twiddles.capacity() * size_of::<Complex32>()
    }
    /// Transform size in complex points.
    pub fn size(&self) -> usize {
        self.size
    }
    /// In-place forward transform; refuses a mismatched length before mutation.
    pub fn forward(&self, samples: &mut [Complex32]) -> Result<(), FftError> {
        self.transform(samples, false)
    }
    /// In-place inverse transform scaled by `1/N`; refuses a mismatched length
    /// before mutation.
    pub fn inverse(&self, samples: &mut [Complex32]) -> Result<(), FftError> {
        self.transform(samples, true)
    }
    fn transform(&self, samples: &mut [Complex32], inverse: bool) -> Result<(), FftError> {
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
                    group[index] = Complex32::new(left.re + right.re, left.im + right.im);
                    group[index + half] = Complex32::new(left.re - right.re, left.im - right.im);
                }
            }
            width *= 2;
        }
        if inverse {
            for value in samples {
                value.re /= self.size as f32;
                value.im /= self.size as f32;
            }
        }
        Ok(())
    }
}
