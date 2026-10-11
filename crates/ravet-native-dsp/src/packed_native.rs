//! Audited transient-register boundary. All pointers refer to complete fixed-size
//! local arrays; loads/stores are unaligned-capable, never extend a borrowed prefix
//! and never escape. Native backend constructors admit only the current ordinary
//! OS architecture: AArch64 FP/Advanced SIMD (`target_feature = "neon"`) or
//! x86_64 baseline SSE2 (`target_feature = "sse2"`). No optional
//! AVX feature is assumed. No register-policy changes or allocation occur here.
use crate::packed::{Binary, Comparison};
#[cfg(target_arch = "aarch64")]
use core::arch::aarch64::*;
#[cfg(target_arch = "x86_64")]
use core::arch::x86_64::*;
pub(crate) fn binary32(op: Binary, a: [f32; 4], b: [f32; 4]) -> [f32; 4] {
    #[cfg(any(target_arch = "aarch64", target_arch = "x86_64"))]
    let mut out = [0.; 4];
    // SAFETY: complete fixed-width local arrays and admitted baseline CPU profile;
    // no raw pointer survives this operation, alignment is not assumed.
    #[cfg(target_arch = "aarch64")]
    unsafe {
        let a = vld1q_f32(a.as_ptr());
        let b = vld1q_f32(b.as_ptr());
        let r = match op {
            Binary::Add => vaddq_f32(a, b),
            Binary::Sub => vsubq_f32(a, b),
            Binary::Mul => vmulq_f32(a, b),
            Binary::Div => vdivq_f32(a, b),
        };
        vst1q_f32(out.as_mut_ptr(), r);
        out
    }
    #[cfg(target_arch = "x86_64")]
    unsafe {
        let a = _mm_loadu_ps(a.as_ptr());
        let b = _mm_loadu_ps(b.as_ptr());
        let r = match op {
            Binary::Add => _mm_add_ps(a, b),
            Binary::Sub => _mm_sub_ps(a, b),
            Binary::Mul => _mm_mul_ps(a, b),
            Binary::Div => _mm_div_ps(a, b),
        };
        _mm_storeu_ps(out.as_mut_ptr(), r);
        out
    }
    #[cfg(not(any(target_arch = "aarch64", target_arch = "x86_64")))]
    {
        let _ = (op, a, b);
        unreachable!("native backend was not admitted");
    }
}
pub(crate) fn sqrt32(a: [f32; 4]) -> [f32; 4] {
    #[cfg(any(target_arch = "aarch64", target_arch = "x86_64"))]
    let mut out = [0.; 4];
    // SAFETY: complete fixed-width local arrays and admitted baseline CPU profile;
    // no raw pointer survives this operation, alignment is not assumed.
    #[cfg(target_arch = "aarch64")]
    unsafe {
        vst1q_f32(out.as_mut_ptr(), vsqrtq_f32(vld1q_f32(a.as_ptr())));
        out
    }
    #[cfg(target_arch = "x86_64")]
    unsafe {
        _mm_storeu_ps(out.as_mut_ptr(), _mm_sqrt_ps(_mm_loadu_ps(a.as_ptr())));
        out
    }
    #[cfg(not(any(target_arch = "aarch64", target_arch = "x86_64")))]
    {
        let _ = a;
        unreachable!("native backend was not admitted");
    }
}
pub(crate) fn select32(mask: u8, a: [f32; 4], b: [f32; 4]) -> [f32; 4] {
    #[cfg(any(target_arch = "aarch64", target_arch = "x86_64"))]
    let mut out = [0.; 4];
    // SAFETY: complete fixed-width local arrays and admitted baseline CPU profile;
    // no raw pointer survives this operation, alignment is not assumed.
    #[cfg(target_arch = "aarch64")]
    unsafe {
        let m: [u32; 4] =
            core::array::from_fn(|i| if mask & (1 << i) != 0 { <u32>::MAX } else { 0 });
        vst1q_f32(
            out.as_mut_ptr(),
            vbslq_f32(
                vld1q_u32(m.as_ptr()),
                vld1q_f32(a.as_ptr()),
                vld1q_f32(b.as_ptr()),
            ),
        );
        out
    }
    #[cfg(target_arch = "x86_64")]
    unsafe {
        let m: [u32; 4] =
            core::array::from_fn(|i| if mask & (1 << i) != 0 { <u32>::MAX } else { 0 });
        let m = _mm_castsi128_ps(_mm_loadu_si128(m.as_ptr().cast()));
        let a = _mm_loadu_ps(a.as_ptr());
        let b = _mm_loadu_ps(b.as_ptr());
        _mm_storeu_ps(
            out.as_mut_ptr(),
            _mm_or_ps(_mm_and_ps(m, a), _mm_andnot_ps(m, b)),
        );
        out
    }
    #[cfg(not(any(target_arch = "aarch64", target_arch = "x86_64")))]
    {
        let _ = (mask, a, b);
        unreachable!("native backend was not admitted");
    }
}
pub(crate) fn convert32(a: [i32; 4]) -> [f32; 4] {
    #[cfg(any(target_arch = "aarch64", target_arch = "x86_64"))]
    let mut out = [0.; 4];
    // SAFETY: complete fixed-width local arrays and admitted baseline CPU profile;
    // no raw pointer survives this operation, alignment is not assumed.
    #[cfg(target_arch = "aarch64")]
    unsafe {
        vst1q_f32(out.as_mut_ptr(), vcvtq_f32_s32(vld1q_s32(a.as_ptr())));
        out
    }
    #[cfg(target_arch = "x86_64")]
    unsafe {
        _mm_storeu_ps(
            out.as_mut_ptr(),
            _mm_cvtepi32_ps(_mm_loadu_si128(a.as_ptr().cast())),
        );
        out
    }
    #[cfg(not(any(target_arch = "aarch64", target_arch = "x86_64")))]
    {
        let _ = a;
        unreachable!("native backend was not admitted");
    }
}
pub(crate) fn compare32(op: Comparison, a: [f32; 4], b: [f32; 4]) -> u8 {
    // SAFETY: complete immutable arrays, admitted CPU; result lane masks have
    // only the exact public width. No optional SSE predicate extensions used.
    #[cfg(target_arch = "aarch64")]
    unsafe {
        let a = vld1q_f32(a.as_ptr());
        let b = vld1q_f32(b.as_ptr());
        let r = match op {
            Comparison::Equal => vceqq_f32(a, b),
            Comparison::Less => vcltq_f32(a, b),
            Comparison::LessEqual => vcleq_f32(a, b),
            Comparison::Greater => vcgtq_f32(a, b),
            Comparison::GreaterEqual => vcgeq_f32(a, b),
        };
        let mut lanes = [0; 4];
        vst1q_u32(lanes.as_mut_ptr(), r);
        (0..4).fold(0, |bits, i| bits | ((lanes[i] != 0) as u8) << i)
    }
    #[cfg(target_arch = "x86_64")]
    unsafe {
        let a = _mm_loadu_ps(a.as_ptr());
        let b = _mm_loadu_ps(b.as_ptr());
        let r = match op {
            Comparison::Equal => _mm_cmpeq_ps(a, b),
            Comparison::Less => _mm_cmplt_ps(a, b),
            Comparison::LessEqual => _mm_cmple_ps(a, b),
            Comparison::Greater => _mm_cmpgt_ps(a, b),
            Comparison::GreaterEqual => _mm_cmpge_ps(a, b),
        };
        _mm_movemask_ps(r) as u8
    }
    #[cfg(not(any(target_arch = "aarch64", target_arch = "x86_64")))]
    {
        let _ = (op, a, b);
        unreachable!("native backend was not admitted");
    }
}
pub(crate) fn zip32(a: [f32; 4], b: [f32; 4]) -> ([f32; 4], [f32; 4]) {
    #[cfg(any(target_arch = "aarch64", target_arch = "x86_64"))]
    let (mut lo, mut hi) = ([0.; 4], [0.; 4]);
    // SAFETY: full local arrays, no alignment assumption or pointer escape.
    #[cfg(target_arch = "aarch64")]
    unsafe {
        let a = vld1q_f32(a.as_ptr());
        let b = vld1q_f32(b.as_ptr());
        vst1q_f32(lo.as_mut_ptr(), vzip1q_f32(a, b));
        vst1q_f32(hi.as_mut_ptr(), vzip2q_f32(a, b));
        (lo, hi)
    }
    #[cfg(target_arch = "x86_64")]
    unsafe {
        let a = _mm_loadu_ps(a.as_ptr());
        let b = _mm_loadu_ps(b.as_ptr());
        _mm_storeu_ps(lo.as_mut_ptr(), _mm_unpacklo_ps(a, b));
        _mm_storeu_ps(hi.as_mut_ptr(), _mm_unpackhi_ps(a, b));
        (lo, hi)
    }
    #[cfg(not(any(target_arch = "aarch64", target_arch = "x86_64")))]
    {
        let _ = (a, b);
        unreachable!("native backend was not admitted");
    }
}
pub(crate) fn unzip32(a: [f32; 4], b: [f32; 4]) -> ([f32; 4], [f32; 4]) {
    #[cfg(any(target_arch = "aarch64", target_arch = "x86_64"))]
    let (mut lo, mut hi) = ([0.; 4], [0.; 4]);
    // SAFETY: full local arrays, no alignment assumption or pointer escape.
    #[cfg(target_arch = "aarch64")]
    unsafe {
        let a = vld1q_f32(a.as_ptr());
        let b = vld1q_f32(b.as_ptr());
        vst1q_f32(lo.as_mut_ptr(), vuzp1q_f32(a, b));
        vst1q_f32(hi.as_mut_ptr(), vuzp2q_f32(a, b));
        (lo, hi)
    }
    #[cfg(target_arch = "x86_64")]
    unsafe {
        let a = _mm_loadu_ps(a.as_ptr());
        let b = _mm_loadu_ps(b.as_ptr());
        _mm_storeu_ps(lo.as_mut_ptr(), _mm_shuffle_ps::<0x88>(a, b));
        _mm_storeu_ps(hi.as_mut_ptr(), _mm_shuffle_ps::<0xdd>(a, b));
        (lo, hi)
    }
    #[cfg(not(any(target_arch = "aarch64", target_arch = "x86_64")))]
    {
        let _ = (a, b);
        unreachable!("native backend was not admitted");
    }
}
pub(crate) fn binary64(op: Binary, a: [f64; 2], b: [f64; 2]) -> [f64; 2] {
    #[cfg(any(target_arch = "aarch64", target_arch = "x86_64"))]
    let mut out = [0.; 2];
    // SAFETY: complete fixed-width local arrays and admitted baseline CPU profile;
    // no raw pointer survives this operation, alignment is not assumed.
    #[cfg(target_arch = "aarch64")]
    unsafe {
        let a = vld1q_f64(a.as_ptr());
        let b = vld1q_f64(b.as_ptr());
        let r = match op {
            Binary::Add => vaddq_f64(a, b),
            Binary::Sub => vsubq_f64(a, b),
            Binary::Mul => vmulq_f64(a, b),
            Binary::Div => vdivq_f64(a, b),
        };
        vst1q_f64(out.as_mut_ptr(), r);
        out
    }
    #[cfg(target_arch = "x86_64")]
    unsafe {
        let a = _mm_loadu_pd(a.as_ptr());
        let b = _mm_loadu_pd(b.as_ptr());
        let r = match op {
            Binary::Add => _mm_add_pd(a, b),
            Binary::Sub => _mm_sub_pd(a, b),
            Binary::Mul => _mm_mul_pd(a, b),
            Binary::Div => _mm_div_pd(a, b),
        };
        _mm_storeu_pd(out.as_mut_ptr(), r);
        out
    }
    #[cfg(not(any(target_arch = "aarch64", target_arch = "x86_64")))]
    {
        let _ = (op, a, b);
        unreachable!("native backend was not admitted");
    }
}
pub(crate) fn sqrt64(a: [f64; 2]) -> [f64; 2] {
    #[cfg(any(target_arch = "aarch64", target_arch = "x86_64"))]
    let mut out = [0.; 2];
    // SAFETY: complete fixed-width local arrays and admitted baseline CPU profile;
    // no raw pointer survives this operation, alignment is not assumed.
    #[cfg(target_arch = "aarch64")]
    unsafe {
        vst1q_f64(out.as_mut_ptr(), vsqrtq_f64(vld1q_f64(a.as_ptr())));
        out
    }
    #[cfg(target_arch = "x86_64")]
    unsafe {
        _mm_storeu_pd(out.as_mut_ptr(), _mm_sqrt_pd(_mm_loadu_pd(a.as_ptr())));
        out
    }
    #[cfg(not(any(target_arch = "aarch64", target_arch = "x86_64")))]
    {
        let _ = a;
        unreachable!("native backend was not admitted");
    }
}
pub(crate) fn select64(mask: u8, a: [f64; 2], b: [f64; 2]) -> [f64; 2] {
    #[cfg(any(target_arch = "aarch64", target_arch = "x86_64"))]
    let mut out = [0.; 2];
    // SAFETY: complete fixed-width local arrays and admitted baseline CPU profile;
    // no raw pointer survives this operation, alignment is not assumed.
    #[cfg(target_arch = "aarch64")]
    unsafe {
        let m: [u64; 2] =
            core::array::from_fn(|i| if mask & (1 << i) != 0 { <u64>::MAX } else { 0 });
        vst1q_f64(
            out.as_mut_ptr(),
            vbslq_f64(
                vld1q_u64(m.as_ptr()),
                vld1q_f64(a.as_ptr()),
                vld1q_f64(b.as_ptr()),
            ),
        );
        out
    }
    #[cfg(target_arch = "x86_64")]
    unsafe {
        let m: [u64; 2] =
            core::array::from_fn(|i| if mask & (1 << i) != 0 { <u64>::MAX } else { 0 });
        let m = _mm_castsi128_pd(_mm_loadu_si128(m.as_ptr().cast()));
        let a = _mm_loadu_pd(a.as_ptr());
        let b = _mm_loadu_pd(b.as_ptr());
        _mm_storeu_pd(
            out.as_mut_ptr(),
            _mm_or_pd(_mm_and_pd(m, a), _mm_andnot_pd(m, b)),
        );
        out
    }
    #[cfg(not(any(target_arch = "aarch64", target_arch = "x86_64")))]
    {
        let _ = (mask, a, b);
        unreachable!("native backend was not admitted");
    }
}
pub(crate) fn convert64(a: [i32; 2]) -> [f64; 2] {
    #[cfg(any(target_arch = "aarch64", target_arch = "x86_64"))]
    let mut out = [0.; 2];
    // SAFETY: complete fixed-width local arrays and admitted baseline CPU profile;
    // no raw pointer survives this operation, alignment is not assumed.
    #[cfg(target_arch = "aarch64")]
    unsafe {
        vst1q_f64(
            out.as_mut_ptr(),
            vcvtq_f64_s64(vmovl_s32(vld1_s32(a.as_ptr()))),
        );
        out
    }
    #[cfg(target_arch = "x86_64")]
    unsafe {
        _mm_storeu_pd(
            out.as_mut_ptr(),
            _mm_cvtepi32_pd(_mm_set_epi32(0, 0, a[1], a[0])),
        );
        out
    }
    #[cfg(not(any(target_arch = "aarch64", target_arch = "x86_64")))]
    {
        let _ = a;
        unreachable!("native backend was not admitted");
    }
}
pub(crate) fn compare64(op: Comparison, a: [f64; 2], b: [f64; 2]) -> u8 {
    // SAFETY: complete immutable arrays, admitted CPU; result lane masks have
    // only the exact public width. No optional SSE predicate extensions used.
    #[cfg(target_arch = "aarch64")]
    unsafe {
        let a = vld1q_f64(a.as_ptr());
        let b = vld1q_f64(b.as_ptr());
        let r = match op {
            Comparison::Equal => vceqq_f64(a, b),
            Comparison::Less => vcltq_f64(a, b),
            Comparison::LessEqual => vcleq_f64(a, b),
            Comparison::Greater => vcgtq_f64(a, b),
            Comparison::GreaterEqual => vcgeq_f64(a, b),
        };
        let mut lanes = [0; 2];
        vst1q_u64(lanes.as_mut_ptr(), r);
        (0..2).fold(0, |bits, i| bits | ((lanes[i] != 0) as u8) << i)
    }
    #[cfg(target_arch = "x86_64")]
    unsafe {
        let a = _mm_loadu_pd(a.as_ptr());
        let b = _mm_loadu_pd(b.as_ptr());
        let r = match op {
            Comparison::Equal => _mm_cmpeq_pd(a, b),
            Comparison::Less => _mm_cmplt_pd(a, b),
            Comparison::LessEqual => _mm_cmple_pd(a, b),
            Comparison::Greater => _mm_cmpgt_pd(a, b),
            Comparison::GreaterEqual => _mm_cmpge_pd(a, b),
        };
        _mm_movemask_pd(r) as u8
    }
    #[cfg(not(any(target_arch = "aarch64", target_arch = "x86_64")))]
    {
        let _ = (op, a, b);
        unreachable!("native backend was not admitted");
    }
}
pub(crate) fn zip64(a: [f64; 2], b: [f64; 2]) -> ([f64; 2], [f64; 2]) {
    #[cfg(any(target_arch = "aarch64", target_arch = "x86_64"))]
    let (mut lo, mut hi) = ([0.; 2], [0.; 2]);
    // SAFETY: full local arrays, no alignment assumption or pointer escape.
    #[cfg(target_arch = "aarch64")]
    unsafe {
        let a = vld1q_f64(a.as_ptr());
        let b = vld1q_f64(b.as_ptr());
        vst1q_f64(lo.as_mut_ptr(), vzip1q_f64(a, b));
        vst1q_f64(hi.as_mut_ptr(), vzip2q_f64(a, b));
        (lo, hi)
    }
    #[cfg(target_arch = "x86_64")]
    unsafe {
        let a = _mm_loadu_pd(a.as_ptr());
        let b = _mm_loadu_pd(b.as_ptr());
        _mm_storeu_pd(lo.as_mut_ptr(), _mm_unpacklo_pd(a, b));
        _mm_storeu_pd(hi.as_mut_ptr(), _mm_unpackhi_pd(a, b));
        (lo, hi)
    }
    #[cfg(not(any(target_arch = "aarch64", target_arch = "x86_64")))]
    {
        let _ = (a, b);
        unreachable!("native backend was not admitted");
    }
}
pub(crate) fn unzip64(a: [f64; 2], b: [f64; 2]) -> ([f64; 2], [f64; 2]) {
    #[cfg(any(target_arch = "aarch64", target_arch = "x86_64"))]
    let (mut lo, mut hi) = ([0.; 2], [0.; 2]);
    // SAFETY: full local arrays, no alignment assumption or pointer escape.
    #[cfg(target_arch = "aarch64")]
    unsafe {
        let a = vld1q_f64(a.as_ptr());
        let b = vld1q_f64(b.as_ptr());
        vst1q_f64(lo.as_mut_ptr(), vuzp1q_f64(a, b));
        vst1q_f64(hi.as_mut_ptr(), vuzp2q_f64(a, b));
        (lo, hi)
    }
    #[cfg(target_arch = "x86_64")]
    unsafe {
        let a = _mm_loadu_pd(a.as_ptr());
        let b = _mm_loadu_pd(b.as_ptr());
        _mm_storeu_pd(lo.as_mut_ptr(), _mm_unpacklo_pd(a, b));
        _mm_storeu_pd(hi.as_mut_ptr(), _mm_unpackhi_pd(a, b));
        (lo, hi)
    }
    #[cfg(not(any(target_arch = "aarch64", target_arch = "x86_64")))]
    {
        let _ = (a, b);
        unreachable!("native backend was not admitted");
    }
}

pub(crate) fn scalar_sqrt32(a: f32) -> f32 {
    #[cfg(target_arch = "aarch64")]
    {
        let result: f32;
        // SAFETY: ordinary OS scalar FP instruction, no pointer or feature extension.
        unsafe {
            core::arch::asm!("fsqrt {r:s}, {a:s}",r=out(vreg)result,a=in(vreg)a,options(nostack,preserves_flags));
        }
        result
    }
    #[cfg(target_arch = "x86_64")]
    // SAFETY: admitted baseline SSE, scalar instruction touches only its low lane.
    unsafe {
        _mm_cvtss_f32(_mm_sqrt_ss(_mm_set_ss(a)))
    }
    #[cfg(not(any(target_arch = "aarch64", target_arch = "x86_64")))]
    {
        let _ = a;
        unreachable!("native backend not admitted")
    }
}
pub(crate) fn scalar_sqrt64(a: f64) -> f64 {
    #[cfg(target_arch = "aarch64")]
    {
        let result: f64;
        // SAFETY: ordinary OS scalar FP instruction; no pointer/optional extension.
        unsafe {
            core::arch::asm!("fsqrt {r:d}, {a:d}",r=out(vreg)result,a=in(vreg)a,options(nostack,preserves_flags));
        }
        result
    }
    #[cfg(target_arch = "x86_64")]
    // SAFETY: baseline SSE2, only low lane sqrt executes; no unused-lane exception.
    unsafe {
        _mm_cvtsd_f64(_mm_sqrt_sd(_mm_setzero_pd(), _mm_set_sd(a)))
    }
    #[cfg(not(any(target_arch = "aarch64", target_arch = "x86_64")))]
    {
        let _ = a;
        unreachable!("native backend not admitted")
    }
}

pub(crate) fn bits32(a: [f32; 4], clear_sign: bool) -> [f32; 4] {
    #[cfg(any(target_arch = "aarch64", target_arch = "x86_64"))]
    let mut out = [0.; 4];
    // SAFETY: complete arrays, baseline CPU; integer bit operations preserve
    // signalling NaN payloads and do not execute arithmetic on those payloads.
    #[cfg(target_arch = "aarch64")]
    unsafe {
        let a = vreinterpretq_u32_f32(vld1q_f32(a.as_ptr()));
        let r = if clear_sign {
            vandq_u32(a, vdupq_n_u32(0x7fffffff))
        } else {
            veorq_u32(a, vdupq_n_u32(0x80000000))
        };
        vst1q_f32(out.as_mut_ptr(), vreinterpretq_f32_u32(r));
        out
    }
    #[cfg(target_arch = "x86_64")]
    unsafe {
        let a = _mm_loadu_ps(a.as_ptr());
        let mask = _mm_castsi128_ps(_mm_set1_epi32(if clear_sign {
            0x7fffffff
        } else {
            i32::MIN
        }));
        _mm_storeu_ps(
            out.as_mut_ptr(),
            if clear_sign {
                _mm_and_ps(a, mask)
            } else {
                _mm_xor_ps(a, mask)
            },
        );
        out
    }
    #[cfg(not(any(target_arch = "aarch64", target_arch = "x86_64")))]
    {
        let _ = (a, clear_sign);
        unreachable!("native backend not admitted")
    }
}
pub(crate) fn bits64(a: [f64; 2], clear_sign: bool) -> [f64; 2] {
    #[cfg(any(target_arch = "aarch64", target_arch = "x86_64"))]
    let mut out = [0.; 2];
    // SAFETY: complete arrays and baseline CPU; bit operations never quiet NaNs.
    #[cfg(target_arch = "aarch64")]
    unsafe {
        let a = vreinterpretq_u64_f64(vld1q_f64(a.as_ptr()));
        let r = if clear_sign {
            vandq_u64(a, vdupq_n_u64(0x7fffffffffffffff))
        } else {
            veorq_u64(a, vdupq_n_u64(0x8000000000000000))
        };
        vst1q_f64(out.as_mut_ptr(), vreinterpretq_f64_u64(r));
        out
    }
    #[cfg(target_arch = "x86_64")]
    unsafe {
        let a = _mm_loadu_pd(a.as_ptr());
        let mask = _mm_castsi128_pd(_mm_set1_epi64x(if clear_sign {
            i64::MAX
        } else {
            i64::MIN
        }));
        _mm_storeu_pd(
            out.as_mut_ptr(),
            if clear_sign {
                _mm_and_pd(a, mask)
            } else {
                _mm_xor_pd(a, mask)
            },
        );
        out
    }
    #[cfg(not(any(target_arch = "aarch64", target_arch = "x86_64")))]
    {
        let _ = (a, clear_sign);
        unreachable!("native backend not admitted")
    }
}
