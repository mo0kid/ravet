use ravet_signal::PartitionedConvolver;

#[test]
fn matches_direct_sum_with_declared_delay_and_arbitrary_caller_partitions() {
    for (taps, block) in [(1, 16), (17, 16), (65, 32), (257, 64), (1025, 128)] {
        let impulse: Vec<_> = (0..taps)
            .map(|n| (n as f64 * 0.13).cos() / taps as f64)
            .collect();
        let mut input: Vec<_> = (0..333).map(|n| (n as f64 * 0.37).sin()).collect();
        input.resize(input.len() + taps + block, 0.);
        let expected: Vec<f64> = (0..input.len())
            .map(|n| {
                if n < block {
                    return 0.;
                }
                let position = n - block;
                impulse
                    .iter()
                    .enumerate()
                    .filter(|(k, _)| *k <= position)
                    .map(|(k, h)| h * input[position - k])
                    .sum()
            })
            .collect();
        for chunk in [1, 3, 16, 127, 1024] {
            let mut convolver = PartitionedConvolver::new(&impulse, block).unwrap();
            assert_eq!(convolver.latency_samples(), block);
            assert_eq!(convolver.tail_samples(), taps - 1 + block);
            let mut output = input.clone();
            convolver.process_in_place(&mut []);
            for part in output.chunks_mut(chunk) {
                convolver.process_in_place(part);
            }
            for (actual, expected) in output.iter().zip(&expected) {
                assert!(
                    (actual - expected).abs() < 1e-11,
                    "taps={taps} block={block} chunk={chunk}: {actual} != {expected}"
                );
            }
            convolver.reset();
            let mut silence = vec![0.; block * 3];
            convolver.process_in_place(&mut silence);
            assert!(silence.iter().all(|sample| *sample == 0.));
        }
    }
}

#[test]
fn validates_preparation_and_bounds_long_responses() {
    for impulse in [&[][..], &[f64::NAN], &[f64::INFINITY]] {
        assert!(PartitionedConvolver::new(impulse, 16).is_err());
    }
    for block in [0, 1, 15, 17, 4097, usize::MAX] {
        assert!(PartitionedConvolver::new(&[1.], block).is_err());
    }
    assert!(PartitionedConvolver::new(&vec![0.; 262_145], 16).is_err());
    let mut response = vec![0.; 262_144];
    response[0] = 1.;
    response[262_143] = -0.5;
    let mut convolver = PartitionedConvolver::new(&response, 4096).unwrap();
    for index in 0..266_241 {
        let actual = convolver.process_sample(if index == 0 { 1. } else { 0. });
        let expected = if index == 4096 {
            1.
        } else if index == 266_239 {
            -0.5
        } else {
            0.
        };
        assert!((actual - expected).abs() < 1e-11);
    }
}

#[test]
fn transforms_partial_blocks_and_reset_do_not_allocate_or_deallocate() {
    let mut convolver = PartitionedConvolver::new(&[0.125; 513], 64).unwrap();
    let counts = allocation_counter::measure(|| {
        for index in 0..1000 {
            let mut samples = [0.5; 73];
            convolver.process_in_place(&mut samples);
            std::hint::black_box(samples);
            if index % 11 == 0 {
                convolver.reset();
            }
        }
    });
    assert_eq!(counts.count_total, 0);
    assert_eq!(counts.count_current, 0);
    assert_eq!(counts.bytes_current, 0);
}
