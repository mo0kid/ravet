use ravet_foundation::AudioBuffer;

#[test]
fn in_place_mapping_reads_both_inputs_before_writing_and_keeps_tails() {
    let mut left = [1.0, 2.0, 123.0];
    let mut right = [3.0, 4.0, 456.0];
    let mut channels = [&mut left[..], &mut right[..]];
    let mut audio = AudioBuffer::in_place(&mut channels, 2).unwrap();
    let mut offsets = Vec::new();
    assert!(audio.map_stereo_frames(|frame, [left, right]| {
        offsets.push(frame);
        [right, left]
    }));
    assert_eq!(offsets, [0, 1]);
    assert_eq!(left, [3.0, 4.0, 123.0]);
    assert_eq!(right, [1.0, 2.0, 456.0]);
}

#[test]
fn separate_mapping_supplies_zero_for_missing_inputs() {
    let a = [1.0, 2.0, 3.0];
    let b = [4.0, 5.0, 6.0];
    for inputs in [&[][..], &[&a[..]][..], &[&a[..], &b[..]][..]] {
        let mut left = [99.0; 4];
        let mut right = [99.0; 5];
        let mut outputs = [&mut left[..], &mut right[..]];
        let mut audio = AudioBuffer::separate(inputs, &mut outputs, 3).unwrap();
        assert!(audio.map_stereo_frames(|frame, input| {
            assert_eq!(input[0], inputs.first().map_or(0.0, |v| v[frame]));
            assert_eq!(input[1], inputs.get(1).map_or(0.0, |v| v[frame]));
            [input[0] + 10.0, input[1] + 20.0]
        }));
        assert_eq!(left[3], 99.0);
        assert_eq!(right[3..], [99.0, 99.0]);
    }
}

#[test]
fn zero_frames_and_unsupported_shapes_never_invoke_the_mapper() {
    let input = [1.0; 3];
    for input_count in 0..=3 {
        for output_count in 0..=3 {
            for frames in [0, 3] {
                let inputs = [&input[..]; 3];
                let mut backing = [[77.0; 4]; 3];
                let mut outputs = backing.iter_mut().map(|v| &mut v[..]).collect::<Vec<_>>();
                let mut audio = AudioBuffer::separate(
                    &inputs[..input_count],
                    &mut outputs[..output_count],
                    frames,
                )
                .unwrap();
                let mut count = 0;
                let mapped = audio.map_stereo_frames(|_, _| {
                    count += 1;
                    [12.0, 13.0]
                });
                assert_eq!(mapped, output_count == 2 && input_count <= 2);
                assert_eq!(count, if mapped { frames } else { 0 });
                if !mapped || frames == 0 {
                    assert_eq!(backing, [[77.0; 4]; 3]);
                }
            }
        }
    }
    let mut sample = [77.0];
    let mut channels = [&mut sample[..]];
    let mut audio = AudioBuffer::in_place(&mut channels, 1).unwrap();
    assert!(!audio.map_stereo_frames(|_, _| panic!("unsupported in-place layout")));
    assert_eq!(sample, [77.0]);
}
