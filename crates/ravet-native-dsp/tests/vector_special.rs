use ravet_native_dsp::{
    DenormalMode as Mode, F32x4, F64x2, PreparedVectorOps32, PreparedVectorOps64,
    ScopedDenormalMode, VectorBackend as Backend, current_float_state,
};
#[test]
fn independently_computed_dyadic_sqrt_oracle_full_and_tail_profiles() {
    let mut maximum32 = 0;
    let mut maximum64 = 0;
    for backend in [Backend::Scalar, Backend::native().unwrap()] {
        let _restore = ScopedDenormalMode::enter(Mode::Preserve).unwrap();
        for line in include_str!("data/sqrt-decimal-oracle.csv").lines().skip(1) {
            let p: Vec<_> = line.split(',').collect();
            let input = u64::from_str_radix(p[1], 16).unwrap();
            let expected = u64::from_str_radix(p[2], 16).unwrap();
            if p[0] == "32" {
                let x = f32::from_bits(input as u32);
                let pack = F32x4::broadcast(backend, x).unwrap().sqrt().to_array();
                let mut output = [99.; 7];
                PreparedVectorOps32::new(backend, 5)
                    .unwrap()
                    .sqrt(&[x; 5], &mut output, 5)
                    .unwrap();
                for r in pack.into_iter().chain(output[..5].iter().copied()) {
                    let error = (r.to_bits() as u64).abs_diff(expected);
                    maximum32 = maximum32.max(error);
                    assert!(
                        error <= 1,
                        "{backend:?} bits={input:x} got={:x} expected={expected:x}",
                        r.to_bits()
                    );
                }
                assert_eq!(&output[5..], &[99.; 2]);
            } else {
                let x = f64::from_bits(input);
                let pack = F64x2::broadcast(backend, x).unwrap().sqrt().to_array();
                let mut output = [99.; 5];
                PreparedVectorOps64::new(backend, 3)
                    .unwrap()
                    .sqrt(&[x; 3], &mut output, 3)
                    .unwrap();
                for r in pack.into_iter().chain(output[..3].iter().copied()) {
                    let error = r.to_bits().abs_diff(expected);
                    maximum64 = maximum64.max(error);
                    assert!(
                        error <= 1,
                        "{backend:?} bits={input:x} got={:x} expected={expected:x}",
                        r.to_bits()
                    );
                }
                assert_eq!(&output[3..], &[99.; 2]);
            }
        }
    }
    println!(
        "Decimal120 sqrt36 inputs × selected Scalar/native packed+tails: max observed32={maximum32}ULP64={maximum64}ULP; finite selected profile, not universal rounding certificate"
    );
}
// This oracle uses integer encodings only. Comparison inputs follow the
// observed input-flush profile, but selected results retain original bits.
fn ordered_less(mut a: u64, mut b: u64, width: u32, flush: bool) -> bool {
    let sign = 1u64 << (width - 1);
    let fraction = if width == 32 {
        (1u64 << 23) - 1
    } else {
        (1u64 << 52) - 1
    };
    let exponent = (sign - 1) ^ fraction;
    if [a, b]
        .into_iter()
        .any(|x| x & exponent == exponent && x & fraction != 0)
    {
        return false;
    }
    if flush {
        if a & exponent == 0 {
            a &= sign;
        }
        if b & exponent == 0 {
            b &= sign;
        }
    }
    if a & (sign - 1) == 0 && b & (sign - 1) == 0 {
        return false;
    }
    let mask = if width == 32 {
        u32::MAX as u64
    } else {
        u64::MAX
    };
    let key = |x: u64| if x & sign != 0 { !x & mask } else { x ^ sign };
    key(a) < key(b)
}
macro_rules! special {
 ($name:ident,$ops:ident,$s:ty,$bits:ty,$nan:expr,$sign:expr) => {
    #[test]
    fn $name() {
        let patterns=[<$s>::from_bits($nan),<$s>::from_bits($nan|$sign),0.,-0.,
            <$s>::from_bits(1),<$s>::from_bits(1|$sign),<$s>::INFINITY,<$s>::NEG_INFINITY,
            <$s>::MAX,-<$s>::MAX,1.,-1.];
        for mode in [Mode::Preserve,Mode::FlushBoth] {let _restore=ScopedDenormalMode::enter(mode).unwrap();let flush=current_float_state().unwrap().input_flush();
            for backend in [Backend::Scalar,Backend::native().unwrap()] {for n in 0..=17 {
                let ops=$ops::new(backend,17).unwrap();let a:Vec<$s>=(0..n+3).map(|i|patterns[i%patterns.len()]).collect();
                let b:Vec<$s>=(0..n+3).map(|i|patterns[(i+3)%patterns.len()]).collect();let mut output=vec![99 as $s;n+3];
                ops.absolute(&a,&mut output,n).unwrap();for i in 0..n {assert_eq!(output[i].to_bits(),a[i].to_bits()&(<$bits>::MAX>>1),"absolute {mode:?} {backend:?} n={n} i={i} a={:x}",a[i].to_bits());}
                ops.negate(&a,&mut output,n).unwrap();for i in 0..n {assert_eq!(output[i].to_bits(),a[i].to_bits()^$sign,"negate {mode:?} {backend:?} n={n} i={i} a={:x}",a[i].to_bits());}
                ops.minimum(&a,&b,&mut output,n).unwrap();for i in 0..n {assert_eq!(output[i].to_bits(),(if ordered_less(a[i].to_bits() as u64,b[i].to_bits() as u64,size_of::<$s>() as u32*8,flush) {a[i].to_bits()}else{b[i].to_bits()}),"min {mode:?} {backend:?} n={n} i={i} a={:x} b={:x}",a[i].to_bits(),b[i].to_bits());}
                ops.maximum_values(&a,&b,&mut output,n).unwrap();for i in 0..n {assert_eq!(output[i].to_bits(),(if ordered_less(b[i].to_bits() as u64,a[i].to_bits() as u64,size_of::<$s>() as u32*8,flush) {a[i].to_bits()}else{b[i].to_bits()}),"max {mode:?} {backend:?} n={n} i={i} a={:x} b={:x}",a[i].to_bits(),b[i].to_bits());}
                ops.clip(&a,-0.5,0.5,&mut output,n).unwrap();for i in 0..n {assert_eq!(output[i].to_bits(),(if ordered_less(a[i].to_bits() as u64,(-0.5 as $s).to_bits() as u64,size_of::<$s>() as u32*8,flush) {(-0.5 as $s).to_bits()}else if ordered_less((0.5 as $s).to_bits() as u64,a[i].to_bits() as u64,size_of::<$s>() as u32*8,flush) {(0.5 as $s).to_bits()}else{a[i].to_bits()}),"clip {mode:?} {backend:?} n={n} i={i} a={:x}",a[i].to_bits());}
                let minbits=a[..n].iter().map(|x|x.to_bits()).reduce(|x,y|if ordered_less(x as u64,y as u64,size_of::<$s>() as u32*8,flush){x}else{y});
                let maxbits=a[..n].iter().map(|x|x.to_bits()).reduce(|x,y|if ordered_less(y as u64,x as u64,size_of::<$s>() as u32*8,flush){x}else{y});
                assert_eq!(ops.minimum_value(&a,n).unwrap().map(|x|x.to_bits()),minbits,"minimum reduction {mode:?} {backend:?} n={n}");
                assert_eq!(ops.maximum_value(&a,n).unwrap().map(|x|x.to_bits()),maxbits,"maximum reduction {mode:?} {backend:?} n={n}");
                assert_eq!(&output[n..],&[99.;3]);
                let sub=<$s>::from_bits(1);let positive=[sub;17];ops.sqrt(&positive,&mut output,n).unwrap();
                if backend!=Backend::Scalar&&mode==Mode::FlushBoth {assert!(output[..n].iter().all(|x|x.to_bits()==0));}
                // Scalar libm is an explicitly separate implementation; observe
                // its actual denormal behavior rather than promise hardware parity.
                if n==17 {println!("sqrt native{} mode={mode:?} backend={backend:?}: first={:x}, last={:x}",size_of::<$s>()*8,output[0].to_bits(),output[16].to_bits());}
            }}
        }
    }
 };
}
special!(
    native32_special_slice_policy_and_explicit_denormal_profiles,
    PreparedVectorOps32,
    f32,
    u32,
    0x7fc00123,
    0x80000000
);
special!(
    native64_special_slice_policy_and_explicit_denormal_profiles,
    PreparedVectorOps64,
    f64,
    u64,
    0x7ff8000000000123,
    0x8000000000000000
);
