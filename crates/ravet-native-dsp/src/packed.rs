//! Fixed packed-value family with explicit immutable backend selection.
use crate::packed_native;
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum VectorBackend {
    Scalar,
    Aarch64Neon,
    X86Sse2,
}
impl VectorBackend {
    /// Compile-time admission of the current ordinary OS user-mode profile:
    /// `aarch64` with the `neon` target feature, or `x86_64` with `sse2`.
    /// Any other target, including one built with those features disabled,
    /// returns [`VectorError::UnsupportedBackend`]; there is no silent fallback.
    /// This is not a runtime CPU probe and does not establish privileged,
    /// bare-metal or sandbox register access.
    pub fn native() -> Result<Self, VectorError> {
        #[cfg(all(target_arch = "aarch64", target_feature = "neon"))]
        {
            Ok(Self::Aarch64Neon)
        }
        #[cfg(all(target_arch = "x86_64", target_feature = "sse2"))]
        {
            Ok(Self::X86Sse2)
        }
        #[cfg(not(any(
            all(target_arch = "aarch64", target_feature = "neon"),
            all(target_arch = "x86_64", target_feature = "sse2")
        )))]
        {
            Err(VectorError::UnsupportedBackend)
        }
    }
    pub fn validate(self) -> Result<(), VectorError> {
        if self == Self::Scalar || Self::native() == Ok(self) {
            Ok(())
        } else {
            Err(VectorError::UnsupportedBackend)
        }
    }
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum VectorError {
    UnsupportedBackend,
    BackendMismatch,
    InvalidLane,
    InvalidMask,
    ChannelMismatch,
    InvalidMaximum,
    TooManySamples,
    PrefixTooShort,
    InvalidScalar,
    InvalidRange,
}
impl core::fmt::Display for VectorError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str(match self {
            Self::UnsupportedBackend => {
                "selected packed backend is unavailable on this architecture"
            }
            Self::BackendMismatch => "packed operands require the same selected backend",
            Self::InvalidLane => "packed lane index is outside the declared width",
            Self::ChannelMismatch => "planar views require 1..64 matching channels and gains",
            Self::InvalidMask => "lane mask contains unavailable high bits",
            Self::InvalidMaximum => "prepared sample maximum must be 1..=1048576",
            Self::TooManySamples => "requested sample prefix exceeds the prepared maximum",
            Self::PrefixTooShort => "a complete requested input/output prefix is unavailable",
            Self::InvalidScalar => "control scalar must be finite",
            Self::InvalidRange => "clip endpoints must be finite and ordered",
        })
    }
}
impl core::error::Error for VectorError {}
macro_rules! mask {
    ($name:ident,$width:expr) => {
        /// Fixed-width lane selection; construction rejects unavailable high bits.
        #[derive(Clone, Copy, Debug, Eq, PartialEq)]
        pub struct $name(u8);
        impl $name {
            pub fn from_bits(bits: u8) -> Result<Self, VectorError> {
                if bits & !((1 << $width) - 1) != 0 {
                    Err(VectorError::InvalidMask)
                } else {
                    Ok(Self(bits))
                }
            }
            pub fn bits(self) -> u8 {
                self.0
            }
            pub fn any(self) -> bool {
                self.0 != 0
            }
            pub fn all(self) -> bool {
                self.0 == (1 << $width) - 1
            }
            pub fn count(self) -> u32 {
                self.0.count_ones()
            }
            pub fn lane(self, lane: usize) -> Result<bool, VectorError> {
                if lane >= $width {
                    Err(VectorError::InvalidLane)
                } else {
                    Ok(self.0 & (1 << lane) != 0)
                }
            }
        }
        impl core::ops::BitAnd for $name {
            type Output = Self;
            fn bitand(self, rhs: Self) -> Self {
                Self(self.0 & rhs.0)
            }
        }
        impl core::ops::BitOr for $name {
            type Output = Self;
            fn bitor(self, rhs: Self) -> Self {
                Self(self.0 | rhs.0)
            }
        }
        impl core::ops::BitXor for $name {
            type Output = Self;
            fn bitxor(self, rhs: Self) -> Self {
                Self(self.0 ^ rhs.0)
            }
        }
        impl core::ops::Not for $name {
            type Output = Self;
            fn not(self) -> Self {
                Self(!self.0 & ((1 << $width) - 1))
            }
        }
    };
}
mask!(LaneMask4, 4);
mask!(LaneMask2, 2);
#[derive(Clone, Copy)]
pub(crate) enum Binary {
    Add,
    Sub,
    Mul,
    Div,
}
#[derive(Clone, Copy)]
pub(crate) enum Comparison {
    Equal,
    Less,
    LessEqual,
    Greater,
    GreaterEqual,
}
macro_rules! packed {
    ($name:ident,$sample:ty,$bits:ty,$width:expr,$mask:ident,$binary:ident,$sqrt:ident,$cmp:ident,$select:ident,$zip:ident,$unzip:ident,$convert:ident,$libsqrt:ident,$bitop:ident) => {
        /// Fixed lanes with selected packed arithmetic. No register-residency,
        /// alignment/ABI, compiler-expression or automatic CPU-policy promise.
        /// Arithmetic obeys actual backend instructions; scalar sqrt uses libm.
        #[derive(Clone, Copy, Debug)]
        pub struct $name {
            backend: VectorBackend,
            lanes: [$sample; $width],
        }
        impl $name {
            pub fn new(
                backend: VectorBackend,
                lanes: [$sample; $width],
            ) -> Result<Self, VectorError> {
                backend.validate()?;
                Ok(Self { backend, lanes })
            }
            pub fn broadcast(backend: VectorBackend, value: $sample) -> Result<Self, VectorError> {
                Self::new(backend, [value; $width])
            }
            pub fn load(backend: VectorBackend, input: &[$sample]) -> Result<Self, VectorError> {
                backend.validate()?;
                let prefix = input.get(..$width).ok_or(VectorError::PrefixTooShort)?;
                Ok(Self {
                    backend,
                    lanes: prefix.try_into().expect("checked fixed width"),
                })
            }
            pub fn store(self, output: &mut [$sample]) -> Result<(), VectorError> {
                let prefix = output
                    .get_mut(..$width)
                    .ok_or(VectorError::PrefixTooShort)?;
                prefix.copy_from_slice(&self.lanes);
                Ok(())
            }
            pub fn backend(self) -> VectorBackend {
                self.backend
            }
            pub fn to_array(self) -> [$sample; $width] {
                self.lanes
            }
            pub fn lane(self, lane: usize) -> Result<$sample, VectorError> {
                self.lanes
                    .get(lane)
                    .copied()
                    .ok_or(VectorError::InvalidLane)
            }
            pub fn replace_lane(
                mut self,
                lane: usize,
                value: $sample,
            ) -> Result<Self, VectorError> {
                *self.lanes.get_mut(lane).ok_or(VectorError::InvalidLane)? = value;
                Ok(self)
            }
            fn matched(self, rhs: Self) -> Result<(), VectorError> {
                if self.backend == rhs.backend {
                    Ok(())
                } else {
                    Err(VectorError::BackendMismatch)
                }
            }
            fn binary(self, rhs: Self, op: Binary) -> Result<Self, VectorError> {
                self.matched(rhs)?;
                let lanes = if self.backend == VectorBackend::Scalar {
                    core::array::from_fn(|i| match op {
                        Binary::Add => self.lanes[i] + rhs.lanes[i],
                        Binary::Sub => self.lanes[i] - rhs.lanes[i],
                        Binary::Mul => self.lanes[i] * rhs.lanes[i],
                        Binary::Div => self.lanes[i] / rhs.lanes[i],
                    })
                } else {
                    packed_native::$binary(op, self.lanes, rhs.lanes)
                };
                Ok(Self {
                    backend: self.backend,
                    lanes,
                })
            }
            // Explicit fallible arithmetic rejects mismatched selected backends;
            // a plain Add operator would hide that ownership/profile admission.
            #[allow(clippy::should_implement_trait)]
            pub fn add(self, rhs: Self) -> Result<Self, VectorError> {
                self.binary(rhs, Binary::Add)
            }
            pub fn subtract(self, rhs: Self) -> Result<Self, VectorError> {
                self.binary(rhs, Binary::Sub)
            }
            pub fn multiply(self, rhs: Self) -> Result<Self, VectorError> {
                self.binary(rhs, Binary::Mul)
            }
            pub fn divide(self, rhs: Self) -> Result<Self, VectorError> {
                self.binary(rhs, Binary::Div)
            }
            /// Separate multiplication then addition; deliberately not fused.
            pub fn multiply_add(self, multiplier: Self, addend: Self) -> Result<Self, VectorError> {
                self.matched(multiplier)?;
                self.matched(addend)?;
                self.multiply(multiplier)?.add(addend)
            }
            /// Clear sign bits, preserving all remaining NaN/subnormal payload bits.
            pub fn absolute(self) -> Self {
                Self {
                    backend: self.backend,
                    lanes: if self.backend == VectorBackend::Scalar {
                        self.lanes
                            .map(|v| <$sample>::from_bits(v.to_bits() & (<$bits>::MAX >> 1)))
                    } else {
                        packed_native::$bitop(self.lanes, true)
                    },
                }
            }
            /// Toggle sign bits without arithmetic or quieting NaNs.
            pub fn negate(self) -> Self {
                Self {
                    backend: self.backend,
                    lanes: if self.backend == VectorBackend::Scalar {
                        self.lanes.map(|v| {
                            <$sample>::from_bits(
                                v.to_bits() ^ ((1 as $bits) << (<$bits>::BITS - 1)),
                            )
                        })
                    } else {
                        packed_native::$bitop(self.lanes, false)
                    },
                }
            }
            pub fn sqrt(self) -> Self {
                let lanes = if self.backend == VectorBackend::Scalar {
                    self.lanes.map(libm::$libsqrt)
                } else {
                    packed_native::$sqrt(self.lanes)
                };
                Self {
                    backend: self.backend,
                    lanes,
                }
            }
            fn compare(self, rhs: Self, op: Comparison) -> Result<$mask, VectorError> {
                self.matched(rhs)?;
                let bits = if self.backend == VectorBackend::Scalar {
                    (0..$width).fold(0, |bits, i| {
                        bits | ((match op {
                            Comparison::Equal => self.lanes[i] == rhs.lanes[i],
                            Comparison::Less => self.lanes[i] < rhs.lanes[i],
                            Comparison::LessEqual => self.lanes[i] <= rhs.lanes[i],
                            Comparison::Greater => self.lanes[i] > rhs.lanes[i],
                            Comparison::GreaterEqual => self.lanes[i] >= rhs.lanes[i],
                        }) as u8)
                            << i
                    })
                } else {
                    packed_native::$cmp(op, self.lanes, rhs.lanes)
                };
                Ok($mask(bits))
            }
            pub fn equal(self, rhs: Self) -> Result<$mask, VectorError> {
                self.compare(rhs, Comparison::Equal)
            }
            /// Unordered !=: a NaN operand produces a true lane.
            pub fn not_equal(self, rhs: Self) -> Result<$mask, VectorError> {
                Ok(!self.equal(rhs)?)
            }
            pub fn less(self, rhs: Self) -> Result<$mask, VectorError> {
                self.compare(rhs, Comparison::Less)
            }
            pub fn less_equal(self, rhs: Self) -> Result<$mask, VectorError> {
                self.compare(rhs, Comparison::LessEqual)
            }
            pub fn greater(self, rhs: Self) -> Result<$mask, VectorError> {
                self.compare(rhs, Comparison::Greater)
            }
            pub fn greater_equal(self, rhs: Self) -> Result<$mask, VectorError> {
                self.compare(rhs, Comparison::GreaterEqual)
            }
            /// True lanes choose the left operand; selection preserves exact bits.
            pub fn select(mask: $mask, left: Self, right: Self) -> Result<Self, VectorError> {
                left.matched(right)?;
                let lanes = if left.backend == VectorBackend::Scalar {
                    core::array::from_fn(|i| {
                        <$sample>::from_bits(select_bits(
                            mask.0 & (1 << i) != 0,
                            left.lanes[i].to_bits() as u64,
                            right.lanes[i].to_bits() as u64,
                        ) as $bits)
                    })
                } else {
                    packed_native::$select(mask.0, left.lanes, right.lanes)
                };
                Ok(Self {
                    backend: left.backend,
                    lanes,
                })
            }
            /// Select left only for ordered <; otherwise right bits (ties/NaN included).
            pub fn minimum(self, rhs: Self) -> Result<Self, VectorError> {
                Self::select(self.less(rhs)?, self, rhs)
            }
            /// Select left only for ordered >; otherwise right bits (ties/NaN included).
            pub fn maximum(self, rhs: Self) -> Result<Self, VectorError> {
                Self::select(self.greater(rhs)?, self, rhs)
            }
            pub fn permute(self, indices: [usize; $width]) -> Result<Self, VectorError> {
                if indices.iter().any(|i| *i >= $width) {
                    return Err(VectorError::InvalidLane);
                }
                Ok(Self {
                    backend: self.backend,
                    lanes: indices.map(|i| self.lanes[i]),
                })
            }
            /// Interleaved full-width pair, lower half followed by upper half.
            pub fn interleave(self, rhs: Self) -> Result<(Self, Self), VectorError> {
                self.matched(rhs)?;
                let (a, b) = if self.backend == VectorBackend::Scalar {
                    (
                        core::array::from_fn(|i| {
                            if i % 2 == 0 {
                                self.lanes[i / 2]
                            } else {
                                rhs.lanes[i / 2]
                            }
                        }),
                        core::array::from_fn(|i| {
                            if i % 2 == 0 {
                                self.lanes[$width / 2 + i / 2]
                            } else {
                                rhs.lanes[$width / 2 + i / 2]
                            }
                        }),
                    )
                } else {
                    packed_native::$zip(self.lanes, rhs.lanes)
                };
                Ok((
                    Self {
                        backend: self.backend,
                        lanes: a,
                    },
                    Self {
                        backend: self.backend,
                        lanes: b,
                    },
                ))
            }
            pub fn deinterleave(self, rhs: Self) -> Result<(Self, Self), VectorError> {
                self.matched(rhs)?;
                let (a, b) = if self.backend == VectorBackend::Scalar {
                    (
                        core::array::from_fn(|i| {
                            if 2 * i < $width {
                                self.lanes[2 * i]
                            } else {
                                rhs.lanes[2 * i - $width]
                            }
                        }),
                        core::array::from_fn(|i| {
                            if 2 * i + 1 < $width {
                                self.lanes[2 * i + 1]
                            } else {
                                rhs.lanes[2 * i + 1 - $width]
                            }
                        }),
                    )
                } else {
                    packed_native::$unzip(self.lanes, rhs.lanes)
                };
                Ok((
                    Self {
                        backend: self.backend,
                        lanes: a,
                    },
                    Self {
                        backend: self.backend,
                        lanes: b,
                    },
                ))
            }
            pub fn from_i32(
                backend: VectorBackend,
                input: [i32; $width],
            ) -> Result<Self, VectorError> {
                backend.validate()?;
                Ok(Self {
                    backend,
                    lanes: if backend == VectorBackend::Scalar {
                        input.map(|v| v as $sample)
                    } else {
                        packed_native::$convert(input)
                    },
                })
            }
        }
    };
}
packed!(
    F32x4, f32, u32, 4, LaneMask4, binary32, sqrt32, compare32, select32, zip32, unzip32,
    convert32, sqrtf, bits32
);
packed!(
    F64x2, f64, u64, 2, LaneMask2, binary64, sqrt64, compare64, select64, zip64, unzip64,
    convert64, sqrt, bits64
);
impl F32x4 {
    /// Fixed pairwise ((0+1)+(2+3)), native sample precision, without widening.
    pub fn sum(self) -> f32 {
        (self.lanes[0] + self.lanes[1]) + (self.lanes[2] + self.lanes[3])
    }
    /// Unfused: lane products are rounded first, then summed with the same
    /// fixed `((0+1)+(2+3))` f32 tree as [`F32x4::sum`].
    pub fn dot(self, rhs: Self) -> Result<f32, VectorError> {
        Ok(self.multiply(rhs)?.sum())
    }
}
impl F64x2 {
    /// Fixed `lane0+lane1`, native sample precision, without widening.
    pub fn sum(self) -> f64 {
        self.lanes[0] + self.lanes[1]
    }
    /// Unfused: lane products are rounded first, then summed as
    /// [`F64x2::sum`] (`lane0+lane1`).
    pub fn dot(self, rhs: Self) -> Result<f64, VectorError> {
        Ok(self.multiply(rhs)?.sum())
    }
}

// Keep floating comparisons separate from bit-preserving operand selection.
// Inlining a float if-expression permits x86 MIN/MAX lowering, which can
// replace selected subnormal bits with zero when DAZ is active. This integer
// boundary contains no floating arithmetic and never modifies either operand.
#[inline(never)]
pub(crate) fn select_bits(left: bool, a: u64, b: u64) -> u64 {
    if left { a } else { b }
}
