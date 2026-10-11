use ravet_native_dsp::{
    DenormalMode, PreparedVectorOps32, PreparedVectorOps64, ScopedDenormalMode,
    VectorBackend as Backend, VectorError as Error,
};
macro_rules! suite {
 ($name:ident,$bank:ident,$large:ident,$ops:ident,$s:ty,$bits:ty,$width:expr,$epsilon:expr) => {
    #[test]
    fn $name() {
        let _restore=ScopedDenormalMode::enter(DenormalMode::Preserve).unwrap();
        for backend in [Backend::Scalar,Backend::native().unwrap()] {for n in 0..=17 {for offset in 0..=5 {
            let ops=$ops::new(backend,17).unwrap();assert_eq!(ops.maximum(),17);assert_eq!(ops.backend(),backend);
            let a:Vec<$s>=(0..offset+n+7).map(|i|(i as $s-8.)*0.125).collect();
            let b:Vec<$s>=(0..offset+n+7).map(|i|(i%7+1) as $s*0.5).collect();
            let c:Vec<$s>=(0..offset+n+7).map(|i|i as $s*0.0625-0.7).collect();
            let (a,b,c)=(&a[offset..],&b[offset..],&c[offset..]);
            let mut out=vec![99 as $s;offset+n+7];
            macro_rules! verify {
                ($index:ident,$call:expr,$expected:expr) => {
                    out.fill(99.);
                    let info=allocation_counter::measure(|| {$call.unwrap();});
                    assert_eq!(info.count_total,0);assert_eq!(info.count_current,0);assert_eq!(info.bytes_total,0);
                    assert!(out[..offset].iter().all(|x|*x==99.));assert!(out[offset+n..].iter().all(|x|*x==99.));
                    for $index in 0..n {let expected:$s=$expected;let actual=out[offset+$index];
                        assert!((actual-expected).abs()<=($epsilon as $s)*(1.+expected.abs()),"backend={backend:?} n={n} offset={offset} lane={}: {actual} vs {expected}",$index);}
                };
            }
            verify!(i,ops.fill(-0.3,&mut out[offset..],n),-0.3);
            verify!(i,ops.clear(&mut out[offset..],n),0.);
            verify!(i,ops.copy(a,&mut out[offset..],n),a[i]);
            verify!(i,ops.add(a,b,&mut out[offset..],n),a[i]+b[i]);
            verify!(i,ops.subtract(a,b,&mut out[offset..],n),a[i]-b[i]);
            verify!(i,ops.multiply(a,b,&mut out[offset..],n),a[i]*b[i]);
            verify!(i,ops.divide(a,b,&mut out[offset..],n),a[i]/b[i]);
            verify!(i,ops.minimum(a,b,&mut out[offset..],n),if a[i]<b[i] {a[i]} else {b[i]});
            verify!(i,ops.maximum_values(a,b,&mut out[offset..],n),if a[i]>b[i] {a[i]} else {b[i]});
            verify!(i,ops.gain(a,-0.37,&mut out[offset..],n),a[i]*(-0.37));
            verify!(i,ops.offset(a,0.42,&mut out[offset..],n),a[i]+0.42);
            verify!(i,ops.absolute(a,&mut out[offset..],n),a[i].abs());
            verify!(i,ops.negate(a,&mut out[offset..],n),-a[i]);
            verify!(i,ops.sqrt(b,&mut out[offset..],n),b[i].sqrt());
            verify!(i,ops.add_product(a,b,c,&mut out[offset..],n),{let product=a[i]*b[i];product+c[i]});
            verify!(i,ops.clip(a,-0.3,0.7,&mut out[offset..],n),a[i].clamp(-0.3,0.7));
            let ints:Vec<i32>=(0..n+3).map(|i|[i32::MIN,i32::MAX,-37,17,65537][i%5]).collect();
            verify!(i,ops.convert_i32(&ints,0.003,&mut out[offset..],n),(ints[i] as $s)*0.003);
            out[offset..offset+n].copy_from_slice(&a[..n]);
            let info=allocation_counter::measure(||ops.gain_in_place(&mut out[offset..],-0.37,n).unwrap());assert_eq!(info.count_total,0);
            for i in 0..n {assert_eq!(out[offset+i],a[i]*(-0.37));}
            ops.add_in_place(b,&mut out[offset..],n).unwrap();for i in 0..n {assert_eq!(out[offset+i],a[i]*(-0.37)+b[i]);}
            assert!(out[offset+n..].iter().all(|x|*x==99.));
            let tree=|x:&[$s]|-> $s {let mut r=0.;let complete=n/$width*$width;
                for block in x[..complete].chunks_exact($width) {r+=if $width==4 {(block[0]+block[1])+(block[2]+block[3])} else {block[0]+block[1]};}
                for x in &x[complete..n] {r+=*x;}r};
            assert_eq!(ops.sum(a,n).unwrap(),tree(a));
            let products:Vec<_>=a[..n].iter().zip(b).map(|(a,b)|*a**b).collect();assert_eq!(ops.dot(a,b,n).unwrap(),tree(&products));
            assert_eq!(ops.minimum_value(a,n).unwrap(),a[..n].iter().copied().reduce(|a,b|if a<b {a}else{b}));
            assert_eq!(ops.maximum_value(a,n).unwrap(),a[..n].iter().copied().reduce(|a,b|if a>b {a}else{b}));
            let mut inter=vec![99 as $s;offset+2*n+5];ops.interleave(a,b,&mut inter[offset..],n).unwrap();
            for i in 0..n {assert_eq!(inter[offset+2*i],a[i]);assert_eq!(inter[offset+2*i+1],b[i]);}
            assert!(inter[..offset].iter().all(|x|*x==99.));assert!(inter[offset+2*n..].iter().all(|x|*x==99.));
            let mut left=vec![99 as $s;offset+n+5];let mut right=left.clone();
            let info=allocation_counter::measure(||ops.deinterleave(&inter[offset..],&mut left[offset..],&mut right[offset..],n).unwrap());assert_eq!(info.count_total,0);
            assert_eq!(&left[offset..offset+n],&a[..n]);assert_eq!(&right[offset..offset+n],&b[..n]);assert!(left[offset+n..].iter().chain(&right[offset+n..]).all(|x|*x==99.));
            let previous=out.clone();assert_eq!(ops.add_product(a,b,&c[..n.saturating_sub(1)],&mut out[offset..],n),if n==0 {Ok(())}else{Err(Error::PrefixTooShort)});assert_eq!(out,previous);
            assert_eq!(ops.gain(a,<$s>::NAN,&mut out[offset..],n),Err(Error::InvalidScalar));assert_eq!(out,previous);
            assert_eq!(ops.clip(a,1.,0.,&mut out[offset..],n),Err(Error::InvalidRange));assert_eq!(out,previous);
            assert_eq!(ops.fill(1.,&mut out,usize::MAX),Err(Error::TooManySamples));assert_eq!(out,previous);
        }}}
        for maximum in [0,1_048_577,usize::MAX] {assert!(matches!($ops::new(Backend::Scalar,maximum),Err(Error::InvalidMaximum)));}
    }
    #[test]
    fn $bank() {
        for backend in [Backend::Scalar,Backend::native().unwrap()] {
            let ops=$ops::new(backend,17).unwrap();let input:Vec<Vec<$s>>=(0..64).map(|ch|(0..20).map(|n|(ch+n) as $s*0.25).collect()).collect();
            let refs:Vec<_>=input.iter().map(|s|s.as_slice()).collect();let gains:Vec<$s>=(0..64).map(|ch|0.5+ch as $s*0.015625).collect();
            let mut output=vec![vec![99 as $s;20];64];
            let mut views:Vec<_>=output.iter_mut().map(|s|s.as_mut_slice()).collect();
            let info=allocation_counter::measure(||ops.gain_planar(&refs,&gains,&mut views,17).unwrap());assert_eq!(info.count_total,0);
            for ch in 0..64 {for n in 0..17 {assert_eq!(views[ch][n],input[ch][n]*gains[ch]);}assert_eq!(&views[ch][17..],&[99.;3]);}
            drop(views);let previous=output.clone();let mut bad_gains=gains.clone();bad_gains[63]=<$s>::NAN;
            let mut views:Vec<_>=output.iter_mut().map(|s|s.as_mut_slice()).collect();assert_eq!(ops.gain_planar(&refs,&bad_gains,&mut views,17),Err(Error::InvalidScalar));drop(views);assert_eq!(output,previous);
            let mut short=refs.clone();short[63]=&input[63][..16];let mut views:Vec<_>=output.iter_mut().map(|s|s.as_mut_slice()).collect();assert_eq!(ops.gain_planar(&short,&gains,&mut views,17),Err(Error::PrefixTooShort));drop(views);assert_eq!(output,previous);
            let mut views:Vec<_>=output.iter_mut().map(|s|s.as_mut_slice()).collect();let info=allocation_counter::measure(||ops.gain_planar_in_place(&mut views,&gains,17).unwrap());assert_eq!(info.count_total,0);for ch in 0..64 {for n in 0..17 {assert_eq!(views[ch][n],(input[ch][n]*gains[ch])*gains[ch]);}}
            drop(views);let previous=output.clone();let mut views:Vec<_>=output.iter_mut().map(|s|s.as_mut_slice()).collect();assert_eq!(ops.gain_planar_in_place(&mut views,&bad_gains,0),Err(Error::InvalidScalar));drop(views);assert_eq!(output,previous);
        }
    }
    #[test]
    fn $large() {
        let count=1_048_576;let input=vec![0.25 as $s;count];let mut output=vec![99 as $s;count+3];
        for backend in [Backend::Scalar,Backend::native().unwrap()] {let ops=$ops::new(backend,count).unwrap();
            let info=allocation_counter::measure(||ops.gain(&input,0.5,&mut output,count).unwrap());assert_eq!(info.count_total,0);assert!(output[..count].iter().all(|x|*x==0.125));assert_eq!(&output[count..],&[99.;3]);
            assert_eq!(ops.gain(&input,0.5,&mut output,count+1),Err(Error::TooManySamples));assert!(output[..count].iter().all(|x|*x==0.125));
        }
    }
 };
}
suite!(
    slice32_full_family_offsets_tails_refusal_and_heap,
    bank32_64_channels_whole_call_admission,
    slice32_actual_maximum,
    PreparedVectorOps32,
    f32,
    u32,
    4,
    3e-7
);
suite!(
    slice64_full_family_offsets_tails_refusal_and_heap,
    bank64_64_channels_whole_call_admission,
    slice64_actual_maximum,
    PreparedVectorOps64,
    f64,
    u64,
    2,
    2e-15
);
