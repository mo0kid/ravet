use ravet_signal::{Oversampler, OversamplingFactor, Waveshaper, WaveshaperLengthError};
use std::cell::Cell;

#[test]
fn built_in_landmarks_symmetry_monotonicity_and_extreme_inputs() {
    let hard = Waveshaper::hard_clip();
    let tanh = Waveshaper::tanh();
    let cubic = Waveshaper::cubic_soft_clip();
    for x in [-f64::MAX, -2.0, -1.0, -0.5, 0.0, 0.5, 1.0, 2.0, f64::MAX] {
        let clipped = x.clamp(-1.0, 1.0);
        assert_eq!(hard.process_sample(x), clipped);
        // Independent polynomial form, evaluated only inside the knees.
        let expected = if x.abs() >= 1.0 {
            x.signum() * (2.0 / 3.0)
        } else {
            x - x.powi(3) / 3.0
        };
        assert!((cubic.process_sample(x) - expected).abs() < 2e-16);
        for shape in [&hard, &tanh, &cubic] {
            assert_eq!(shape.process_sample(x), -shape.process_sample(-x));
            assert!(shape.process_sample(x).is_finite());
        }
    }
    assert!((tanh.process_sample(1.0) - 0.761_594_155_955_764_9).abs() < 2e-16);
    assert!((tanh.process_sample(0.5) - 0.462_117_157_260_009_8).abs() < 2e-16);
    for shape in [&hard, &tanh, &cubic] {
        assert!(shape.process_sample(f64::NAN).is_nan());
        assert_eq!(
            shape.process_sample(f64::INFINITY),
            shape.process_sample(f64::MAX)
        );
        assert_eq!(
            shape.process_sample(f64::NEG_INFINITY),
            -shape.process_sample(f64::MAX)
        );
        assert_eq!(shape.process_sample(-0.0).to_bits(), (-0.0_f64).to_bits());
        let mut previous = shape.process_sample(-4.0);
        for i in 1..=8000 {
            let output = shape.process_sample(-4.0 + i as f64 / 1000.0);
            assert!(output >= previous && output.abs() <= 1.0);
            previous = output;
        }
        let small = 1e-6;
        assert!((shape.process_sample(small) / small - 1.0).abs() < 1e-12);
    }
    // The cubic knee has a continuous zero slope on both sides.
    let knee = cubic.process_sample(1.0);
    assert!((knee - cubic.process_sample(1.0 - 1e-6)).abs() < 2e-12);
    assert_eq!(knee, cubic.process_sample(1.0 + 1e-6));
}

#[test]
fn custom_functions_preserve_native_precision_and_captured_controls() {
    let offset = 0.125_f32;
    let f32_shape = Waveshaper::new(|x: f32| x + offset);
    let mut samples = [-0.5_f32, 0.0, 0.5];
    f32_shape.process_in_place(&mut samples);
    assert_eq!(samples, [-0.375, 0.125, 0.625]);
    let f64_shape = Waveshaper::new(|x: f64| x);
    let precise = 1.0 + f64::EPSILON;
    assert_eq!(
        f64_shape.process_sample(precise).to_bits(),
        precise.to_bits()
    );
    assert!(f64_shape.process_sample(f64::NAN).is_nan());
    assert_eq!(f64_shape.process_sample(f64::INFINITY), f64::INFINITY);
    let mut output = [0.0; 3];
    f64_shape
        .process_block(&[precise, -precise, 0.0], &mut output)
        .unwrap();
    assert_eq!(output, [precise, -precise, 0.0]);
}

#[test]
fn block_partition_identity_and_atomic_shape_rejection() {
    let visits = Cell::new(0);
    let shape = Waveshaper::new(|x: f64| {
        visits.set(visits.get() + 1);
        x * x
    });
    let mut output = [123.0; 3];
    for input in [&[][..], &[1.0, 2.0][..], &[1.0, 2.0, 3.0, 4.0][..]] {
        assert_eq!(
            shape.process_block(input, &mut output),
            Err(WaveshaperLengthError)
        );
        assert_eq!(output, [123.0; 3]);
        assert_eq!(visits.get(), 0);
    }
    shape.process_in_place(&mut []);
    shape.process_block(&[], &mut []).unwrap();
    assert_eq!(visits.get(), 0);
    // An interior-state observer establishes once-only calls in input order.
    let next = Cell::new(0);
    let ordered = Waveshaper::new(|x: f64| {
        assert_eq!(x, next.get() as f64);
        next.set(next.get() + 1);
        -x
    });
    let mut ordered_output = [0.0; 3];
    ordered
        .process_block(&[0.0, 1.0, 2.0], &mut ordered_output)
        .unwrap();
    assert_eq!(ordered_output, [0.0, -1.0, -2.0]);
    let mut ordered_input = [3.0, 4.0];
    ordered.process_in_place(&mut ordered_input);
    assert_eq!(ordered_input, [-3.0, -4.0]);
    assert_eq!(next.get(), 5);

    let input: [f64; 1024] = std::array::from_fn(|i| ((i as f64) * 0.031).sin() * 2.0);
    for shape in [
        Waveshaper::hard_clip(),
        Waveshaper::tanh(),
        Waveshaper::cubic_soft_clip(),
    ] {
        let mut whole = input;
        let mut split = input;
        let mut separate = [0.0; 1024];
        shape.process_in_place(&mut whole);
        for block in split.chunks_mut(17) {
            shape.process_in_place(block);
        }
        shape.process_block(&input, &mut separate).unwrap();
        assert_eq!(whole, split);
        assert_eq!(whole, separate);
        for (&sample, &output) in input.iter().zip(&whole) {
            assert_eq!(shape.process_sample(sample), output);
        }
    }
}

#[test]
fn explicit_oversampling_composes_at_native_f32_precision() {
    let drive = 2.0_f32;
    let shape = Waveshaper::new(|x: f32| (drive * x).tanh());
    for factor in [
        OversamplingFactor::One,
        OversamplingFactor::Two,
        OversamplingFactor::Four,
        OversamplingFactor::Eight,
    ] {
        let mut actual = Oversampler::new(factor);
        let mut reference = Oversampler::new(factor);
        for i in 0..2048 {
            let input = (i as f32 * 0.13).sin();
            assert_eq!(
                actual.process_sample(input, |x| shape.process_sample(x)),
                reference.process_sample(input, |x| (drive * x).tanh())
            );
        }
    }
}

#[test]
fn construction_sample_and_block_workloads_have_no_heap_actions() {
    let mut samples = [0.25_f64; 32];
    let mut output = [0.0; 32];
    let counts = allocation_counter::measure(|| {
        let custom = Waveshaper::new(|x: f32| (3.0 * x).tanh());
        let shapes = [
            Waveshaper::hard_clip(),
            Waveshaper::tanh(),
            Waveshaper::cubic_soft_clip(),
        ];
        for _ in 0..10000 {
            std::hint::black_box(custom.process_sample(0.25));
            for shape in &shapes {
                std::hint::black_box(shape.process_sample(0.5));
                shape.process_in_place(&mut samples);
                shape.process_block(&samples, &mut output).unwrap();
            }
        }
    });
    assert_eq!(counts.count_total, 0);
    assert_eq!(counts.count_current, 0);
}
