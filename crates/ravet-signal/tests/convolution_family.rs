use ravet_signal::{
    ConvolutionBank32, ConvolutionBank64, ConvolutionPartitioning as Mode,
    ConvolutionPreparationError as Prepare, ConvolutionProcessError as Process, Convolver32,
    Convolver64, PartitionedConvolver,
};
const BUDGET: usize = 256 * 1024 * 1024;
fn modes() -> [Mode; 7] {
    [
        Mode::UniformDelayed { partition: 16 },
        Mode::UniformDelayed { partition: 64 },
        Mode::ZeroLatency { head: 16 },
        Mode::ZeroLatency { head: 64 },
        Mode::NonUniform { head: 16, tail: 64 },
        Mode::NonUniform {
            head: 32,
            tail: 256,
        },
        Mode::NonUniform { head: 64, tail: 64 },
    ]
}
macro_rules! profiles {
 ($full:ident,$banktest:ident,$large:ident,$engine:ident,$bank:ident,$sample:ty,$tol:expr)=>{
    #[test]
    fn $full(){
        let mut maximum_error=0f64;
        for mode in modes(){for taps in [1,15,16,17,63,64,65,255,256,257,1025]{
            let h:Vec<$sample>=(0..taps).map(|i|((i as f64*0.43).cos()*0.5/(1.+i as f64)) as $sample).collect();
            let source:Vec<$sample>=(0..357).map(|i|((i as f64*0.137).sin()*0.37+(i as f64*0.61).cos()*0.21) as $sample).collect();
            let mut engine=$engine::new(&h,mode,67,BUDGET).unwrap();let info=engine.info();
            assert_eq!(info.taps,taps);assert_eq!(info.tail_samples,taps-1+info.latency_samples);
            assert_eq!(info.actual_numeric_bytes,info.declared_numeric_bytes);
            if !matches!(mode,Mode::UniformDelayed{..}){assert_eq!(info.latency_samples,0);}
            let n=source.len()+info.tail_samples;
            let input:Vec<$sample>=(0..n).map(|i|source.get(i).copied().unwrap_or(0.)).collect();
            let expected:Vec<f64>=(0..n).map(|i|{
                i.checked_sub(info.latency_samples).map_or(0.,|at|h.iter().enumerate().filter_map(|(k,h)|at.checked_sub(k).and_then(|j|source.get(j)).map(|x|*h as f64*(*x as f64))).sum())
            }).collect();
            let mut rendered=Vec::with_capacity(n);let mut cursor=0;
            for cycle in 0..2 {
                while cursor<n {
                    // Explicit empty callbacks preserve histories and suffixes.
                    let mut empty=[19 as $sample;3];engine.process(&input[cursor..],&mut empty,0).unwrap();assert_eq!(empty,[19 as $sample;3]);
                    let count=[1,17,67,3,31,2][cursor%6].min(n-cursor);let mut output=[19 as $sample;70];
                    let counter=allocation_counter::measure(||engine.process(&input[cursor..],&mut output,count).unwrap());assert_eq!(counter.count_total,0);assert_eq!(counter.count_current,0);
                    assert!(output[count..].iter().all(|x|*x==19 as $sample));rendered.extend_from_slice(&output[..count]);cursor+=count;
                }
                for (i,(a,b)) in rendered.iter().zip(&expected).enumerate(){let error=(*a as f64-*b).abs();maximum_error=maximum_error.max(error);assert!(error<=$tol*(1.+b.abs()),"mode={mode:?} taps={taps} sample={i} actual={a} oracle={b} error={error}");}
                for (s,w) in info.stages.into_iter().zip(engine.stage_work()){if let (Some(s),Some(w))=(s,w){assert_eq!(w.completed_blocks,(n/s.partition) as u64);assert_eq!(w.pending_source_frames,n%s.partition);assert!(!w.saturated);}}
                let counter=allocation_counter::measure(||engine.reset());assert_eq!(counter.count_total,0);assert_eq!(counter.count_current,0);
                assert!(engine.stage_work().into_iter().flatten().all(|w|w.completed_blocks==0&&w.pending_source_frames==0));
                if cycle==0 {rendered.clear();cursor=0;}
            }
        }}
        println!("native{} seven schedules/eleven lengths independent finite convolution maxabs={maximum_error:e}; selected rounding observation",size_of::<$sample>()*8);
    }
    #[test]
    fn $banktest(){
        let h=[0.3 as $sample,-0.2 as $sample,0.1 as $sample];let responses=[&h[..];64];let mode=Mode::NonUniform{head:16,tail:64};
        let mut bank=$bank::new(&responses,mode,8,BUDGET).unwrap();let mut reference=$bank::new(&responses,mode,8,BUDGET).unwrap();
        assert_eq!(bank.channels(),64);assert_eq!(bank.actual_numeric_bytes(),bank.declared_numeric_bytes());
        let values:[[ $sample;8];64]=std::array::from_fn(|ch|std::array::from_fn(|i|(ch as f64*0.07+i as f64*0.13).sin() as $sample));
        let input=values.each_ref().map(|x|&x[..]);let mut output=[[19 as $sample;11];64];let mut reference_output=output;
        {let mut views=output.each_mut().map(|x|&mut x[..]);let counter=allocation_counter::measure(||bank.process_planar(&input,&mut views,8).unwrap());assert_eq!(counter.count_total,0);assert_eq!(counter.count_current,0);}
        {let mut views=reference_output.each_mut().map(|x|&mut x[..]);reference.process_planar(&input,&mut views,8).unwrap();}assert_eq!(output,reference_output);
        let before=output;let mut badinput=input;badinput[63]=&values[63][..7];
        {let mut views=output.each_mut().map(|x|&mut x[..]);assert_eq!(bank.process_planar(&badinput,&mut views,8),Err(Process::PrefixTooShort));}assert_eq!(output,before);
        assert_eq!(bank.reset_channel(64),Err(Process::InvalidChannel));
        {let mut views=output.each_mut().map(|x|&mut x[..]);bank.process_planar(&input,&mut views,8).unwrap();}
        {let mut views=reference_output.each_mut().map(|x|&mut x[..]);reference.process_planar(&input,&mut views,8).unwrap();}assert_eq!(output,reference_output);
        for (ch,channel) in output.iter().enumerate(){for i in 0..8{let at=i+8;let x=|n:usize|values[ch][n%8];let expected=h.iter().enumerate().filter(|(k,_)|*k<=at).map(|(k,h)|*h as f64*x(at-k) as f64).sum::<f64>();assert!((channel[i] as f64-expected).abs()<=$tol*(1.+expected.abs()));}assert_eq!(&channel[8..],&[19 as $sample;3]);}
        let mut inplace=values;{let mut views=inplace.each_mut().map(|x|&mut x[..]);bank.reset();bank.process_in_place(&mut views,8).unwrap();}
        for ch in 0..64 {for i in 0..8 {let expected=h.iter().enumerate().filter(|(k,_)|*k<=i).map(|(k,h)|*h as f64*values[ch][i-k] as f64).sum::<f64>();assert!((inplace[ch][i] as f64-expected).abs()<=$tol*(1.+expected.abs()));}}
        // NaN is isolated to its independent channel, then explicit reset clears it.
        let mut poison=values;poison[7][0]=<$sample>::NAN;
        {let mut views=poison.each_mut().map(|x|&mut x[..]);bank.process_in_place(&mut views,8).unwrap();}
        assert!(poison[7].iter().any(|x|x.is_nan()));assert!(poison.iter().enumerate().filter(|(ch,_)|*ch!=7).all(|(_,x)|x.iter().all(|x|x.is_finite())));
        bank.reset();let mut zero=[[0 as $sample;8];64];{let mut views=zero.each_mut().map(|x|&mut x[..]);bank.process_in_place(&mut views,8).unwrap();}assert_eq!(zero,[[0 as $sample;8];64]);
    }
    #[test]
    fn $large(){
        let n=262144;let mut h=vec![0 as $sample;n];for (k,v) in [(0,0.3),(15,-0.2),(16,0.4),(17,0.1),(16383,0.2),(16384,-0.3),(16385,0.7),(n-1,-0.5)] {h[k]=v as $sample;}
        let mode=Mode::NonUniform{head:16,tail:16384};let mut engine=$engine::new(&h,mode,65536,BUDGET).unwrap();let info=engine.info();assert_eq!(info.tail_samples,n-1);
        let mut maximum=0f64;let counter=allocation_counter::measure(||{
            for i in 0..n+16 {let y=engine.process_sample(if i==0{1.}else{0.});let expected=h.get(i).copied().unwrap_or(0.);let error=(y as f64-expected as f64).abs();maximum=maximum.max(error);assert!(error<=$tol,"maxIR sample={i} error={error:e}");}
            engine.reset();
        });assert_eq!(counter.count_total,0);assert_eq!(counter.count_current,0);
        println!("native{} maximumIR={n}, tailpartition16384, maxabsolute={maximum:e}, numericbytes={}",size_of::<$sample>()*8,info.actual_numeric_bytes);
    }
 };
}
profiles!(
    native64_complete_modes_partitions_tail_and_reset,
    native64_independent64_bank_and_atomic_late_refusal,
    native64_maximum_response_and_last_tap,
    Convolver64,
    ConvolutionBank64,
    f64,
    3e-11
);
profiles!(
    native32_complete_modes_partitions_tail_and_reset,
    native32_independent64_bank_and_atomic_late_refusal,
    native32_maximum_response_and_last_tap,
    Convolver32,
    ConvolutionBank32,
    f32,
    2e-5
);
#[test]
fn preparation_and_whole_prefix_refusal_before_owners_or_outputs_change() {
    let h = [0.3, -0.2, 0.1];
    for mode in [
        Mode::UniformDelayed { partition: 0 },
        Mode::UniformDelayed { partition: 17 },
        Mode::ZeroLatency { head: 8192 },
        Mode::NonUniform { head: 64, tail: 16 },
        Mode::NonUniform {
            head: 16,
            tail: 32768,
        },
    ] {
        assert!(matches!(
            Convolver64::new(&h, mode, 8, BUDGET),
            Err(Prepare::InvalidPartitioning)
        ));
    }
    let mode = Mode::ZeroLatency { head: 16 };
    for h in [&[][..], &[f64::NAN], &[f64::INFINITY]] {
        assert!(matches!(
            Convolver64::new(h, mode, 8, BUDGET),
            Err(Prepare::InvalidResponse)
        ));
    }
    for maximum in [0, 65537, usize::MAX] {
        assert!(matches!(
            Convolver64::new(&h, mode, maximum, BUDGET),
            Err(Prepare::InvalidMaximum)
        ));
    }
    let requested = Convolver64::new(&h, mode, 8, BUDGET)
        .unwrap()
        .info()
        .declared_numeric_bytes;
    assert!(Convolver64::new(&h, mode, 8, requested).is_ok());
    for budget in [0, requested - 1, BUDGET + 1, usize::MAX] {
        let counter = allocation_counter::measure(|| {
            assert!(matches!(
                Convolver64::new(&h, mode, 8, budget),
                Err(Prepare::Capacity)
            ))
        });
        assert_eq!(counter.count_total, 0);
        assert_eq!(counter.count_current, 0);
    }
    let mut e = Convolver64::new(&h, mode, 8, BUDGET).unwrap();
    let mut reference = Convolver64::new(&h, mode, 8, BUDGET).unwrap();
    for x in [1., 2., 3.] {
        assert_eq!(e.process_sample(x), reference.process_sample(x));
    }
    let mut output = [19.; 10];
    assert_eq!(
        e.process(&[1.; 8], &mut output, 9),
        Err(Process::TooManyFrames)
    );
    assert_eq!(
        e.process(&[1.; 3], &mut output, 8),
        Err(Process::PrefixTooShort)
    );
    assert_eq!(output, [19.; 10]);
    for x in [4., 0., 0., 0.] {
        assert_eq!(e.process_sample(x), reference.process_sample(x));
    }
    assert!(matches!(
        ConvolutionBank64::new(&[], mode, 8, BUDGET),
        Err(Prepare::InvalidChannels)
    ));
    let too_many = [&h[..]; 65];
    assert!(matches!(
        ConvolutionBank64::new(&too_many, mode, 8, BUDGET),
        Err(Prepare::InvalidChannels)
    ));
    let mut late = [&h[..]; 64];
    late[63] = &[f64::NAN];
    let counter = allocation_counter::measure(|| {
        assert!(matches!(
            ConvolutionBank64::new(&late, mode, 8, BUDGET),
            Err(Prepare::InvalidResponse)
        ))
    });
    assert_eq!(counter.count_total, 0);
    assert_eq!(counter.count_current, 0);
}
#[test]
fn uniform_native64_legacy_pcm_and_work_bound_nonuniform() {
    let h: Vec<_> = (0..4097)
        .map(|i| (i as f64 * 0.43).cos() * 0.1 / (1. + i as f64))
        .collect();
    let mut legacy = PartitionedConvolver::new(&h, 64).unwrap();
    let mut uniform =
        Convolver64::new(&h, Mode::UniformDelayed { partition: 64 }, 67, BUDGET).unwrap();
    let mut nonuniform = Convolver64::new(
        &h,
        Mode::NonUniform {
            head: 16,
            tail: 1024,
        },
        67,
        BUDGET,
    )
    .unwrap();
    let n = 1703;
    for i in 0..n {
        let x = (i as f64 * 0.19).sin() * 0.3;
        assert_eq!(
            legacy.process_sample(x).to_bits(),
            uniform.process_sample(x).to_bits()
        );
        nonuniform.process_sample(x);
    }
    let uw = uniform.stage_work()[0].unwrap();
    let nw = nonuniform.stage_work()[1].unwrap();
    assert_eq!(uw.completed_blocks, (n / 64) as u64);
    assert_eq!(nw.completed_blocks, (n / 1024) as u64);
    assert!(nw.completed_blocks < uw.completed_blocks);
    let info = nonuniform.info();
    assert_eq!(info.stages[0].unwrap().original_offset, 16);
    assert_eq!(info.stages[0].unwrap().response_taps, 1008);
    assert_eq!(info.stages[1].unwrap().original_offset, 1024);
    assert_eq!(info.stages[1].unwrap().response_taps, 3073);
}
