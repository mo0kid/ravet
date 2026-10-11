//! Prepared bounded slice operations with whole-call admission and scalar tails.
use crate::{F32x4, F64x2, VectorBackend, VectorError, packed::select_bits};
const MAXIMUM: usize = 1_048_576;
macro_rules! prepared {
    ($name:ident,$sample:ty,$bits:ty,$width:expr,$packed:ident,$sqrt:ident,$native_sqrt:ident) => {
        /// Fixed backend and maximum per-channel sample count. All lengths and
        /// control values are checked before any output edit; no heap is used.
        /// Elementwise native chunks have the fixed register width; remaining
        /// samples use scalar operations. Suffixes are untouched. There is no
        /// automatic denormal scope or universal cross-backend bitwise promise.
        #[derive(Clone, Copy, Debug)]
        pub struct $name {
            backend: VectorBackend,
            maximum: usize,
        }
        impl $name {
            pub fn new(backend: VectorBackend, maximum: usize) -> Result<Self, VectorError> {
                if maximum == 0 || maximum > MAXIMUM {
                    return Err(VectorError::InvalidMaximum);
                }
                backend.validate()?;
                Ok(Self { backend, maximum })
            }
            pub fn backend(self) -> VectorBackend {
                self.backend
            }
            pub fn maximum(self) -> usize {
                self.maximum
            }
            fn preflight(self, n: usize, lengths: &[usize]) -> Result<(), VectorError> {
                if n > self.maximum {
                    return Err(VectorError::TooManySamples);
                }
                if lengths.iter().any(|length| *length < n) {
                    return Err(VectorError::PrefixTooShort);
                }
                Ok(())
            }
            pub fn fill(
                self,
                value: $sample,
                output: &mut [$sample],
                n: usize,
            ) -> Result<(), VectorError> {
                self.preflight(n, &[output.len()])?;
                output[..n].fill(value);
                Ok(())
            }
            pub fn clear(self, output: &mut [$sample], n: usize) -> Result<(), VectorError> {
                self.fill(0., output, n)
            }
            pub fn copy(
                self,
                input: &[$sample],
                output: &mut [$sample],
                n: usize,
            ) -> Result<(), VectorError> {
                self.preflight(n, &[input.len(), output.len()])?;
                output[..n].copy_from_slice(&input[..n]);
                Ok(())
            }
            fn binary(
                self,
                a: &[$sample],
                b: &[$sample],
                out: &mut [$sample],
                n: usize,
                op: u8,
            ) -> Result<(), VectorError> {
                self.preflight(n, &[a.len(), b.len(), out.len()])?;
                let complete = n / $width * $width;
                for i in (0..complete).step_by($width) {
                    let a = $packed::load(self.backend, &a[i..]).expect("admitted complete prefix");
                    let b = $packed::load(self.backend, &b[i..]).expect("admitted complete prefix");
                    let r = match op {
                        0 => a.add(b),
                        1 => a.subtract(b),
                        2 => a.multiply(b),
                        3 => a.divide(b),
                        4 => a.minimum(b),
                        _ => a.maximum(b),
                    }
                    .expect("one selected backend");
                    r.store(&mut out[i..]).expect("admitted complete prefix");
                }
                for i in complete..n {
                    out[i] = match op {
                        0 => a[i] + b[i],
                        1 => a[i] - b[i],
                        2 => a[i] * b[i],
                        3 => a[i] / b[i],
                        4 => <$sample>::from_bits(select_bits(
                            a[i] < b[i],
                            a[i].to_bits() as u64,
                            b[i].to_bits() as u64,
                        ) as $bits),
                        _ => <$sample>::from_bits(select_bits(
                            a[i] > b[i],
                            a[i].to_bits() as u64,
                            b[i].to_bits() as u64,
                        ) as $bits),
                    };
                }
                Ok(())
            }
            pub fn add(
                self,
                a: &[$sample],
                b: &[$sample],
                out: &mut [$sample],
                n: usize,
            ) -> Result<(), VectorError> {
                self.binary(a, b, out, n, 0)
            }
            pub fn subtract(
                self,
                a: &[$sample],
                b: &[$sample],
                out: &mut [$sample],
                n: usize,
            ) -> Result<(), VectorError> {
                self.binary(a, b, out, n, 1)
            }
            pub fn multiply(
                self,
                a: &[$sample],
                b: &[$sample],
                out: &mut [$sample],
                n: usize,
            ) -> Result<(), VectorError> {
                self.binary(a, b, out, n, 2)
            }
            pub fn divide(
                self,
                a: &[$sample],
                b: &[$sample],
                out: &mut [$sample],
                n: usize,
            ) -> Result<(), VectorError> {
                self.binary(a, b, out, n, 3)
            }
            pub fn minimum(
                self,
                a: &[$sample],
                b: &[$sample],
                out: &mut [$sample],
                n: usize,
            ) -> Result<(), VectorError> {
                self.binary(a, b, out, n, 4)
            }
            pub fn maximum_values(
                self,
                a: &[$sample],
                b: &[$sample],
                out: &mut [$sample],
                n: usize,
            ) -> Result<(), VectorError> {
                self.binary(a, b, out, n, 5)
            }
            fn unary(
                self,
                input: &[$sample],
                out: &mut [$sample],
                n: usize,
                op: u8,
                value: $sample,
            ) -> Result<(), VectorError> {
                self.preflight(n, &[input.len(), out.len()])?;
                let complete = n / $width * $width;
                let scalar = $packed::broadcast(self.backend, value).expect("selected backend");
                for i in (0..complete).step_by($width) {
                    let a = $packed::load(self.backend, &input[i..]).expect("complete prefix");
                    let r = match op {
                        0 => a.multiply(scalar).expect("matched"),
                        1 => a.add(scalar).expect("matched"),
                        2 => a.absolute(),
                        3 => a.negate(),
                        _ => a.sqrt(),
                    };
                    r.store(&mut out[i..]).expect("complete prefix");
                }
                for i in complete..n {
                    out[i] = match op {
                        0 => input[i] * value,
                        1 => input[i] + value,
                        2 => <$sample>::from_bits(input[i].to_bits() & (<$bits>::MAX >> 1)),
                        3 => <$sample>::from_bits(
                            input[i].to_bits() ^ ((1 as $bits) << (<$bits>::BITS - 1)),
                        ),
                        _ => {
                            if self.backend == VectorBackend::Scalar {
                                libm::$sqrt(input[i])
                            } else {
                                crate::packed_native::$native_sqrt(input[i])
                            }
                        }
                    };
                }
                Ok(())
            }
            pub fn gain(
                self,
                input: &[$sample],
                gain: $sample,
                out: &mut [$sample],
                n: usize,
            ) -> Result<(), VectorError> {
                if !gain.is_finite() {
                    return Err(VectorError::InvalidScalar);
                }
                self.unary(input, out, n, 0, gain)
            }
            pub fn offset(
                self,
                input: &[$sample],
                offset: $sample,
                out: &mut [$sample],
                n: usize,
            ) -> Result<(), VectorError> {
                if !offset.is_finite() {
                    return Err(VectorError::InvalidScalar);
                }
                self.unary(input, out, n, 1, offset)
            }
            pub fn absolute(
                self,
                input: &[$sample],
                out: &mut [$sample],
                n: usize,
            ) -> Result<(), VectorError> {
                self.unary(input, out, n, 2, 0.)
            }
            pub fn negate(
                self,
                input: &[$sample],
                out: &mut [$sample],
                n: usize,
            ) -> Result<(), VectorError> {
                self.unary(input, out, n, 3, 0.)
            }
            pub fn sqrt(
                self,
                input: &[$sample],
                out: &mut [$sample],
                n: usize,
            ) -> Result<(), VectorError> {
                self.unary(input, out, n, 4, 0.)
            }
            pub fn gain_in_place(
                self,
                output: &mut [$sample],
                gain: $sample,
                n: usize,
            ) -> Result<(), VectorError> {
                if !gain.is_finite() {
                    return Err(VectorError::InvalidScalar);
                }
                self.preflight(n, &[output.len()])?;
                let g = $packed::broadcast(self.backend, gain).expect("selected backend");
                let complete = n / $width * $width;
                for i in (0..complete).step_by($width) {
                    $packed::load(self.backend, &output[i..])
                        .expect("complete prefix")
                        .multiply(g)
                        .expect("matched")
                        .store(&mut output[i..])
                        .expect("complete prefix");
                }
                for x in &mut output[complete..n] {
                    *x *= gain;
                }
                Ok(())
            }
            pub fn add_in_place(
                self,
                input: &[$sample],
                output: &mut [$sample],
                n: usize,
            ) -> Result<(), VectorError> {
                self.preflight(n, &[input.len(), output.len()])?;
                let complete = n / $width * $width;
                for i in (0..complete).step_by($width) {
                    let a = $packed::load(self.backend, &input[i..]).expect("complete prefix");
                    let b = $packed::load(self.backend, &output[i..]).expect("complete prefix");
                    a.add(b)
                        .expect("matched")
                        .store(&mut output[i..])
                        .expect("complete prefix");
                }
                for i in complete..n {
                    output[i] += input[i];
                }
                Ok(())
            }
            /// Separate rounded products then sums; no fused multiply-add.
            pub fn add_product(
                self,
                a: &[$sample],
                b: &[$sample],
                c: &[$sample],
                out: &mut [$sample],
                n: usize,
            ) -> Result<(), VectorError> {
                self.preflight(n, &[a.len(), b.len(), c.len(), out.len()])?;
                let complete = n / $width * $width;
                for i in (0..complete).step_by($width) {
                    let a = $packed::load(self.backend, &a[i..]).expect("complete prefix");
                    let b = $packed::load(self.backend, &b[i..]).expect("complete prefix");
                    let c = $packed::load(self.backend, &c[i..]).expect("complete prefix");
                    a.multiply_add(b, c)
                        .expect("matched")
                        .store(&mut out[i..])
                        .expect("complete prefix");
                }
                for i in complete..n {
                    let product = a[i] * b[i];
                    out[i] = product + c[i];
                }
                Ok(())
            }
            /// NaNs keep their input bits; finite endpoints and low<=high required.
            /// The `low > high` range check is a floating comparison and observes
            /// the live input-flush policy: under an active `FlushBoth` scope,
            /// subnormal endpoints compare as signed zeros, so e.g. `low = +MIN
            /// subnormal`, `high = -MIN subnormal` is admitted even though its bit
            /// order is inverted. Output selection then follows the same flushed
            /// comparisons while copying the original endpoint/input bits.
            pub fn clip(
                self,
                input: &[$sample],
                low: $sample,
                high: $sample,
                out: &mut [$sample],
                n: usize,
            ) -> Result<(), VectorError> {
                if !low.is_finite() || !high.is_finite() || low > high {
                    return Err(VectorError::InvalidRange);
                }
                self.preflight(n, &[input.len(), out.len()])?;
                let lo = $packed::broadcast(self.backend, low).expect("selected backend");
                let hi = $packed::broadcast(self.backend, high).expect("selected backend");
                let complete = n / $width * $width;
                for i in (0..complete).step_by($width) {
                    let a = $packed::load(self.backend, &input[i..]).expect("complete prefix");
                    let below =
                        $packed::select(a.less(lo).expect("matched"), lo, a).expect("matched");
                    let r = $packed::select(below.greater(hi).expect("matched"), hi, below)
                        .expect("matched");
                    r.store(&mut out[i..]).expect("complete prefix");
                }
                for i in complete..n {
                    let below = <$sample>::from_bits(select_bits(
                        input[i] < low,
                        low.to_bits() as u64,
                        input[i].to_bits() as u64,
                    ) as $bits);
                    out[i] = <$sample>::from_bits(select_bits(
                        below > high,
                        high.to_bits() as u64,
                        below.to_bits() as u64,
                    ) as $bits);
                }
                Ok(())
            }
            /// Pairwise packed sums, then left-fold chunk totals and scalar tail.
            /// Empty input returns +0; there is no widening/compensation.
            pub fn sum(self, input: &[$sample], n: usize) -> Result<$sample, VectorError> {
                self.preflight(n, &[input.len()])?;
                let complete = n / $width * $width;
                let mut result = 0.;
                for i in (0..complete).step_by($width) {
                    result += $packed::load(self.backend, &input[i..])
                        .expect("complete prefix")
                        .sum();
                }
                for x in &input[complete..n] {
                    result += *x;
                }
                Ok(result)
            }
            pub fn dot(
                self,
                a: &[$sample],
                b: &[$sample],
                n: usize,
            ) -> Result<$sample, VectorError> {
                self.preflight(n, &[a.len(), b.len()])?;
                let complete = n / $width * $width;
                let mut result = 0.;
                for i in (0..complete).step_by($width) {
                    result += $packed::load(self.backend, &a[i..])
                        .expect("complete prefix")
                        .dot($packed::load(self.backend, &b[i..]).expect("complete prefix"))
                        .expect("matched");
                }
                for i in complete..n {
                    let product = a[i] * b[i];
                    result += product;
                }
                Ok(result)
            }
            /// Sequential right-preference extrema, unlike the pairwise sum tree.
            pub fn minimum_value(
                self,
                input: &[$sample],
                n: usize,
            ) -> Result<Option<$sample>, VectorError> {
                self.preflight(n, &[input.len()])?;
                Ok(input[..n].iter().copied().reduce(|a, b| {
                    <$sample>::from_bits(
                        select_bits(a < b, a.to_bits() as u64, b.to_bits() as u64) as $bits
                    )
                }))
            }
            pub fn maximum_value(
                self,
                input: &[$sample],
                n: usize,
            ) -> Result<Option<$sample>, VectorError> {
                self.preflight(n, &[input.len()])?;
                Ok(input[..n].iter().copied().reduce(|a, b| {
                    <$sample>::from_bits(
                        select_bits(a > b, a.to_bits() as u64, b.to_bits() as u64) as $bits
                    )
                }))
            }
            pub fn convert_i32(
                self,
                input: &[i32],
                scale: $sample,
                out: &mut [$sample],
                n: usize,
            ) -> Result<(), VectorError> {
                if !scale.is_finite() {
                    return Err(VectorError::InvalidScalar);
                }
                self.preflight(n, &[input.len(), out.len()])?;
                let g = $packed::broadcast(self.backend, scale).expect("selected backend");
                let complete = n / $width * $width;
                for i in (0..complete).step_by($width) {
                    let a: [i32; $width] =
                        input[i..i + $width].try_into().expect("complete prefix");
                    $packed::from_i32(self.backend, a)
                        .expect("selected backend")
                        .multiply(g)
                        .expect("matched")
                        .store(&mut out[i..])
                        .expect("complete prefix");
                }
                for i in complete..n {
                    out[i] = (input[i] as $sample) * scale;
                }
                Ok(())
            }
            /// Whole-bank 1..64 channel admission before any channel edit.
            pub fn gain_planar(
                self,
                inputs: &[&[$sample]],
                gains: &[$sample],
                outputs: &mut [&mut [$sample]],
                n: usize,
            ) -> Result<(), VectorError> {
                let channels = inputs.len();
                if channels == 0
                    || channels > 64
                    || outputs.len() != channels
                    || gains.len() != channels
                {
                    return Err(VectorError::ChannelMismatch);
                }
                if gains.iter().any(|gain| !gain.is_finite()) {
                    return Err(VectorError::InvalidScalar);
                }
                for ch in 0..channels {
                    self.preflight(n, &[inputs[ch].len(), outputs[ch].len()])?;
                }
                for ch in 0..channels {
                    self.gain(inputs[ch], gains[ch], outputs[ch], n)
                        .expect("whole bank admitted");
                }
                Ok(())
            }
            pub fn gain_planar_in_place(
                self,
                outputs: &mut [&mut [$sample]],
                gains: &[$sample],
                n: usize,
            ) -> Result<(), VectorError> {
                let channels = outputs.len();
                if channels == 0 || channels > 64 || gains.len() != channels {
                    return Err(VectorError::ChannelMismatch);
                }
                if gains.iter().any(|gain| !gain.is_finite()) {
                    return Err(VectorError::InvalidScalar);
                }
                for channel in outputs.iter() {
                    self.preflight(n, &[channel.len()])?;
                }
                for ch in 0..channels {
                    self.gain_in_place(outputs[ch], gains[ch], n)
                        .expect("whole bank admitted");
                }
                Ok(())
            }
            /// n per-channel frames, 2*n destination samples; checked before edits.
            pub fn interleave(
                self,
                a: &[$sample],
                b: &[$sample],
                out: &mut [$sample],
                n: usize,
            ) -> Result<(), VectorError> {
                self.preflight(n, &[a.len(), b.len()])?;
                let doubled = n.checked_mul(2).ok_or(VectorError::TooManySamples)?;
                if out.len() < doubled {
                    return Err(VectorError::PrefixTooShort);
                }
                let complete = n / $width * $width;
                for i in (0..complete).step_by($width) {
                    let a = $packed::load(self.backend, &a[i..]).expect("complete prefix");
                    let b = $packed::load(self.backend, &b[i..]).expect("complete prefix");
                    let (lo, hi) = a.interleave(b).expect("matched");
                    lo.store(&mut out[2 * i..]).expect("complete prefix");
                    hi.store(&mut out[2 * i + $width..])
                        .expect("complete prefix");
                }
                for i in complete..n {
                    out[2 * i] = a[i];
                    out[2 * i + 1] = b[i];
                }
                Ok(())
            }
            pub fn deinterleave(
                self,
                input: &[$sample],
                a: &mut [$sample],
                b: &mut [$sample],
                n: usize,
            ) -> Result<(), VectorError> {
                self.preflight(n, &[a.len(), b.len()])?;
                let doubled = n.checked_mul(2).ok_or(VectorError::TooManySamples)?;
                if input.len() < doubled {
                    return Err(VectorError::PrefixTooShort);
                }
                let complete = n / $width * $width;
                for i in (0..complete).step_by($width) {
                    let lo = $packed::load(self.backend, &input[2 * i..]).expect("complete prefix");
                    let hi = $packed::load(self.backend, &input[2 * i + $width..])
                        .expect("complete prefix");
                    let (left, right) = lo.deinterleave(hi).expect("matched");
                    left.store(&mut a[i..]).expect("complete prefix");
                    right.store(&mut b[i..]).expect("complete prefix");
                }
                for i in complete..n {
                    a[i] = input[2 * i];
                    b[i] = input[2 * i + 1];
                }
                Ok(())
            }
        }
    };
}
prepared!(
    PreparedVectorOps32,
    f32,
    u32,
    4,
    F32x4,
    sqrtf,
    scalar_sqrt32
);
prepared!(PreparedVectorOps64, f64, u64, 2, F64x2, sqrt, scalar_sqrt64);
