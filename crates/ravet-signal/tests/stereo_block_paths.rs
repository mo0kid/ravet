use ravet_signal::{
    ChannelLengthMismatch, ConvolutionError, FirConvolver, MAXIMUM_STEREO_FIR_TAPS, Oversampler,
    OversamplerError, OversamplingFactor, StereoFir, StereoOversampler,
};

fn noise(length: usize, seed: u32) -> Vec<f64> {
    let mut state = seed;
    (0..length)
        .map(|_| {
            state = state.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
            (state >> 8) as f64 / (1u32 << 24) as f64 - 0.5
        })
        .collect()
}

/// Sequential double-precision reference: `y[n] = sum h[k] x[n-k]`.
fn reference(impulse: &[f64], input: &[f64]) -> Vec<f64> {
    (0..input.len())
        .map(|n| {
            impulse
                .iter()
                .enumerate()
                .filter(|(k, _)| *k <= n)
                .map(|(k, h)| h * input[n - k])
                .sum()
        })
        .collect()
}

/// Process `input` in blocks whose lengths cycle through `sizes`.
fn run<S: ravet_signal::FirSample>(
    fir: &mut StereoFir<S>,
    left: &[S],
    right: &[S],
    sizes: &[usize],
) -> (Vec<S>, Vec<S>) {
    let (mut l, mut r) = (left.to_vec(), right.to_vec());
    let mut start = 0;
    for &size in sizes.iter().cycle() {
        if start == l.len() {
            break;
        }
        let end = (start + size).min(l.len());
        fir.process(&mut l[start..end], &mut r[start..end]).unwrap();
        start = end;
    }
    (l, r)
}

#[test]
fn stereo_fir_matches_reference_convolution_for_every_tap_count_shape() {
    let input = [noise(700, 1), noise(700, 2)];
    for taps in [1, 2, 7, 8, 9, 15, 16, 17, 64, 255, 256, 1024] {
        let left = noise(taps, 3);
        let right = noise(taps, 4);
        let expected = [reference(&left, &input[0]), reference(&right, &input[1])];
        let mut fir = StereoFir::with_responses(&left, &right, 128).unwrap();
        assert_eq!(fir.taps(), taps);
        assert_eq!(fir.tail_samples(), taps - 1);
        assert_eq!(fir.latency_samples(), 0);
        // Blocks shorter than, equal to and longer than the prepared maximum.
        let (l, r) = run(&mut fir, &input[0], &input[1], &[1, 3, 128, 300, 4, 5]);
        let bound = 1e-12 * (taps as f64).sqrt();
        for (actual, expected) in [(&l, &expected[0]), (&r, &expected[1])] {
            for (a, e) in actual.iter().zip(expected) {
                assert!((a - e).abs() <= bound, "{taps} taps: {a} vs {e}");
            }
        }
        // f32 agrees with the double-precision reference within f32 accuracy.
        let left32: Vec<f32> = left.iter().map(|&v| v as f32).collect();
        let right32: Vec<f32> = right.iter().map(|&v| v as f32).collect();
        let in32 = [0, 1].map(|c| input[c].iter().map(|&v| v as f32).collect::<Vec<_>>());
        let mut fir32 = StereoFir::with_responses(&left32, &right32, 64).unwrap();
        let (l32, r32) = run(&mut fir32, &in32[0], &in32[1], &[64]);
        let reference32 = [
            reference(
                &left32.iter().map(|&v| f64::from(v)).collect::<Vec<_>>(),
                &in32[0].iter().map(|&v| f64::from(v)).collect::<Vec<_>>(),
            ),
            reference(
                &right32.iter().map(|&v| f64::from(v)).collect::<Vec<_>>(),
                &in32[1].iter().map(|&v| f64::from(v)).collect::<Vec<_>>(),
            ),
        ];
        let bound32 = 4e-7 * taps as f64;
        for (actual, expected) in [(&l32, &reference32[0]), (&r32, &reference32[1])] {
            for (a, e) in actual.iter().zip(expected) {
                assert!(
                    (f64::from(*a) - e).abs() <= bound32,
                    "{taps} taps f32: {a} vs {e}"
                );
            }
        }
    }
}

#[test]
fn stereo_fir_is_independent_of_block_partitioning_and_matches_fir_convolver() {
    let impulse = noise(97, 5);
    let input = [noise(1000, 6), noise(1000, 7)];
    let mut whole = StereoFir::new(&impulse, 1000).unwrap();
    let expected = run(&mut whole, &input[0], &input[1], &[1000]);
    for sizes in [&[1][..], &[3, 17, 64], &[999, 1], &[250]] {
        let mut fir = StereoFir::new(&impulse, 250).unwrap();
        assert_eq!(run(&mut fir, &input[0], &input[1], sizes), expected);
    }
    let mut existing = FirConvolver::new(&impulse).unwrap();
    let mut left = input[0].clone();
    existing.process_in_place(&mut left);
    for (a, e) in expected.0.iter().zip(&left) {
        assert!((a - e).abs() < 1e-12);
    }
}

#[test]
fn stereo_fir_impulse_response_reset_and_rejection() {
    let left = [0.5f32, -0.25, 0.125];
    let right = [1.0f32, 2.0, 3.0];
    let mut fir = StereoFir::with_responses(&left, &right, 4).unwrap();
    let mut l = [1.0f32, 0.0, 0.0, 0.0, 0.0];
    let mut r = [1.0f32, 0.0, 0.0, 0.0, 0.0];
    fir.process(&mut l, &mut r).unwrap();
    assert_eq!(l, [0.5, -0.25, 0.125, 0.0, 0.0]);
    assert_eq!(r, [1.0, 2.0, 3.0, 0.0, 0.0]);
    // History carries across blocks until reset.
    let (mut l, mut r) = ([1.0f32], [1.0f32]);
    fir.process(&mut l, &mut r).unwrap();
    let mut l2 = [0.0f32; 2];
    let mut r2 = [0.0f32; 2];
    fir.process(&mut l2, &mut r2).unwrap();
    assert_eq!(l2, [-0.25, 0.125]);
    fir.process(&mut [1.0], &mut [1.0]).unwrap();
    fir.reset();
    let (mut l, mut r) = ([0.0f32; 3], [0.0f32; 3]);
    fir.process(&mut l, &mut r).unwrap();
    assert_eq!((l, r), ([0.0; 3], [0.0; 3]));
    // Mismatched channels reject before touching samples or history.
    let mut short = [9.0f32; 2];
    let mut long = [9.0f32; 3];
    assert_eq!(
        fir.process(&mut short, &mut long),
        Err(ChannelLengthMismatch)
    );
    assert_eq!((short, long), ([9.0; 2], [9.0; 3]));
    let (mut l, mut r) = ([0.0f32; 3], [0.0f32; 3]);
    fir.process(&mut l, &mut r).unwrap();
    assert_eq!((l, r), ([0.0; 3], [0.0; 3]));
    fir.process(&mut [], &mut []).unwrap();
    // Preparation limits.
    assert_eq!(StereoFir::<f32>::new(&[], 4).err(), Some(ConvolutionError));
    assert_eq!(StereoFir::new(&[1.0f32], 0).err(), Some(ConvolutionError));
    assert_eq!(
        StereoFir::new(&[1.0f32], 65_537).err(),
        Some(ConvolutionError)
    );
    assert_eq!(StereoFir::new(&[f32::NAN], 4).err(), Some(ConvolutionError));
    assert_eq!(
        StereoFir::with_responses(&[1.0f32], &[1.0, 2.0], 4).err(),
        Some(ConvolutionError)
    );
    assert!(StereoFir::new(&vec![0.0f32; MAXIMUM_STEREO_FIR_TAPS], 1).is_ok());
    assert!(StereoFir::new(&vec![0.0f32; MAXIMUM_STEREO_FIR_TAPS + 1], 1).is_err());
}

const FACTORS: [OversamplingFactor; 4] = [
    OversamplingFactor::One,
    OversamplingFactor::Two,
    OversamplingFactor::Four,
    OversamplingFactor::Eight,
];

fn shape(sample: f32) -> f32 {
    (sample * 3.0).tanh() * 0.7
}

#[test]
fn stereo_oversampler_is_bit_identical_to_the_per_sample_oversampler() {
    let input = [0, 1].map(|c| {
        noise(1500, 10 + c)
            .into_iter()
            .map(|v| v as f32)
            .collect::<Vec<_>>()
    });
    for factor in FACTORS {
        let expected = [0, 1].map(|channel| {
            let mut oversampler = Oversampler::new(factor);
            input[channel]
                .iter()
                .map(|&s| oversampler.process_sample(s, shape))
                .collect::<Vec<_>>()
        });
        for sizes in [&[1500][..], &[1, 7, 256, 5], &[128]] {
            let mut stereo = StereoOversampler::new(factor, 256).unwrap();
            assert_eq!(stereo.factor(), factor);
            assert_eq!(stereo.maximum_frames(), 256);
            let (mut l, mut r) = (input[0].clone(), input[1].clone());
            let mut start = 0;
            for &size in sizes.iter().cycle() {
                if start == l.len() {
                    break;
                }
                let end = (start + size).min(l.len());
                stereo
                    .process(&mut l[start..end], &mut r[start..end], |hl, hr| {
                        assert!(
                            hl.len() % factor.factor() == 0 && hl.len() <= 256 * factor.factor()
                        );
                        assert_eq!(hl.len(), hr.len());
                        for sample in hl.iter_mut().chain(hr.iter_mut()) {
                            *sample = shape(*sample);
                        }
                    })
                    .unwrap();
                start = end;
            }
            assert_eq!(l, expected[0], "{factor:?} left {sizes:?}");
            assert_eq!(r, expected[1], "{factor:?} right {sizes:?}");
        }
    }
}

#[test]
fn stereo_oversampler_explicit_stages_reset_and_rejection() {
    assert_eq!(
        StereoOversampler::new(OversamplingFactor::Two, 0).err(),
        Some(OversamplerError::InvalidMaximumFrames)
    );
    assert_eq!(
        StereoOversampler::new(OversamplingFactor::Two, 65_537).err(),
        Some(OversamplerError::InvalidMaximumFrames)
    );
    let mut stereo = StereoOversampler::new(OversamplingFactor::Four, 8).unwrap();
    assert_eq!(
        stereo.upsample(&[0.0; 9], &[0.0; 9]).err(),
        Some(OversamplerError::InvalidBlock)
    );
    assert_eq!(
        stereo.upsample(&[0.0; 2], &[0.0; 3]).err(),
        Some(OversamplerError::InvalidBlock)
    );
    let [high_left, high_right] = stereo.upsample(&[1.0; 4], &[-1.0; 4]).unwrap();
    assert_eq!((high_left.len(), high_right.len()), (16, 16));
    let (mut l, mut r) = ([0.0f32; 3], [0.0f32; 4]);
    assert_eq!(
        stereo.downsample(&mut l, &mut r),
        Err(OversamplerError::InvalidBlock)
    );
    let mut l = [0.0f32; 4];
    stereo.downsample(&mut l, &mut r).unwrap();
    assert!(l.iter().all(|s| s.is_finite()) && r.iter().all(|s| s.is_finite()));
    let (mut a, mut b) = (vec![0.5f32; 3], vec![0.5f32; 4]);
    assert_eq!(
        stereo.process(&mut a, &mut b, |_, _| unreachable!()),
        Err(OversamplerError::InvalidBlock)
    );
    // Reset returns to the freshly prepared stream.
    let input = [0.25f32, -0.5, 0.75, 1.0];
    let mut fresh = StereoOversampler::new(OversamplingFactor::Four, 8).unwrap();
    let (mut fl, mut fr) = (input, input);
    fresh.process(&mut fl, &mut fr, |_, _| {}).unwrap();
    stereo.reset();
    let (mut sl, mut sr) = (input, input);
    stereo.process(&mut sl, &mut sr, |_, _| {}).unwrap();
    assert_eq!((sl, sr), (fl, fr));
    // Long DC settles to unity through the cascade.
    let mut dc = StereoOversampler::new(OversamplingFactor::Eight, 512).unwrap();
    let (mut l, mut r) = (vec![0.25f32; 4096], vec![-0.5f32; 4096]);
    dc.process(&mut l, &mut r, |_, _| {}).unwrap();
    assert!((l[4095] - 0.25).abs() < 1e-4 && (r[4095] + 0.5).abs() < 1e-4);
}

#[test]
fn stereo_block_paths_do_not_allocate_while_processing() {
    let mut fir = StereoFir::new(
        &noise(300, 20).iter().map(|&v| v as f32).collect::<Vec<_>>(),
        64,
    )
    .unwrap();
    let mut fir64 = StereoFir::new(&noise(33, 21), 64).unwrap();
    let mut oversampler = StereoOversampler::new(OversamplingFactor::Eight, 64).unwrap();
    let mut left = vec![0.1f32; 200];
    let mut right = vec![-0.1f32; 200];
    let mut left64 = vec![0.1f64; 200];
    let mut right64 = vec![-0.1f64; 200];
    let counts = allocation_counter::measure(|| {
        for block in 0..50 {
            fir.process(&mut left, &mut right).unwrap();
            fir64.process(&mut left64, &mut right64).unwrap();
            oversampler
                .process(&mut left, &mut right, |l, r| {
                    for sample in l.iter_mut().chain(r) {
                        *sample = sample.tanh();
                    }
                })
                .unwrap();
            if block % 10 == 0 {
                fir.reset();
                fir64.reset();
                oversampler.reset();
            }
        }
    });
    assert_eq!(counts.count_total, 0);
    std::hint::black_box((&left, &right, &left64, &right64));
}
