use ravet_signal::{
    DelayInterpolation as I, DryWetGains as G, DryWetMixer as M, DryWetMixer32 as M32,
    DryWetMixerConfig as C, PanningLaw as L, PanningNormalization as N,
};
macro_rules! maximum {
    ($name:ident,$t:ty,$m:ty,$convert:expr,$tol:expr) => {
        #[test]
        fn $name() {
            for policy in [I::Linear, I::Lagrange3rd, I::Thiran1st] {
                for requested in [16.5, 16.999] {
                    let c = C {
                        channels: 1,
                        maximum_frames: 32,
                        fifo_capacity_frames: 37,
                        maximum_latency_samples: 17,
                        interpolation: policy,
                        ramp_frames: 0,
                    };
                    let mut mixer = <$m>::new(
                        c,
                        ($convert)(G::new(0., L::Linear, N::UnityEndpoints).unwrap()),
                    )
                    .unwrap();
                    mixer.set_wet_latency_samples(requested).unwrap();
                    let fraction = mixer.native_fraction() as f64;
                    let a = mixer.native_thiran_coefficient() as f64;
                    let source: Vec<$t> = (0..720)
                        .map(|i| {
                            ((i as f64 * 0.731).sin() * 0.2 + if i % 23 == 0 { 0.17 } else { 0. })
                                as $t
                        })
                        .collect();
                    let mut previous = [0.; 2];
                    let mut expected = vec![];
                    for i in 0..source.len() {
                        let tap = |k: usize| {
                            if i >= 16 + k {
                                source[i - 16 - k] as f64
                            } else {
                                0.
                            }
                        };
                        let value = match policy {
                            I::Linear => (1. - fraction) * tap(0) + fraction * tap(1),
                            I::Lagrange3rd => (0..4)
                                .map(|j| {
                                    tap(j)
                                        * (0..4)
                                            .filter(|&k| k != j)
                                            .map(|k| (fraction - k as f64) / (j as f64 - k as f64))
                                            .product::<f64>()
                                })
                                .sum(),
                            I::Thiran1st => a * tap(0) + previous[0] - a * previous[1],
                            I::Integer => unreachable!(),
                        };
                        previous = [tap(0), value];
                        expected.push(value);
                    }
                    for start in (0..720).step_by(24) {
                        mixer.push_dry(&[&source[start..start + 8]], 8).unwrap();
                        mixer
                            .push_dry(&[&source[start + 8..start + 24]], 16)
                            .unwrap();
                        let mut output = [99. as $t; 27];
                        mixer
                            .mix_wet(&[&[0. as $t; 5]], &mut [&mut output[..5]], 5)
                            .unwrap();
                        mixer.mix_wet(&[&[]], &mut [&mut []], 0).unwrap();
                        mixer
                            .mix_wet(&[&[0. as $t; 19]], &mut [&mut output[5..24]], 19)
                            .unwrap();
                        for i in 0..24 {
                            assert!(
                                (output[i] as f64 - expected[start + i]).abs() <= $tol,
                                "{policy:?} d{requested} frame{}: {} vs {}",
                                start + i,
                                output[i],
                                expected[start + i]
                            );
                        }
                        assert_eq!(output[24..], [99. as $t; 3]);
                        assert_eq!(mixer.accepted_dry_frames(), (start + 24) as u64);
                        assert_eq!(mixer.consumed_mix_frames(), (start + 24) as u64);
                        assert_eq!(mixer.queued_frames(), 0);
                    }
                }
            }
        }
    };
}
maximum!(
    native64_largest_fractional_taps_and_wrapped_different_batches,
    f64,
    M,
    |g: G| g,
    8e-15
);
maximum!(
    native32_largest_fractional_taps_and_wrapped_different_batches,
    f32,
    M32,
    |g: G| g.quantize_f32(),
    6e-7
);
