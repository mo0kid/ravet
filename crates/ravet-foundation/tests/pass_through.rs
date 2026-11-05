use ravet_foundation::AudioBuffer;

#[test]
fn separate_storage_copies_matching_or_last_input() {
    let left = [1.0f32, 2.0, 3.0, 4.0];
    let right = [5.0f32, 6.0, 7.0, 8.0];
    let mut a = [9.0f32; 4];
    let mut b = [9.0f32; 4];
    let mut c = [9.0f32; 4];
    {
        let inputs = [&left[..], &right[..]];
        let mut outputs = [&mut a[..], &mut b[..], &mut c[..]];
        let mut buffer = AudioBuffer::separate(&inputs, &mut outputs, 3).unwrap();
        buffer.pass_through();
    }
    assert_eq!(a, [1.0, 2.0, 3.0, 9.0], "active frames only");
    assert_eq!(b, [5.0, 6.0, 7.0, 9.0]);
    assert_eq!(
        c,
        [5.0, 6.0, 7.0, 9.0],
        "missing inputs repeat the last one"
    );
}

#[test]
fn mono_input_feeds_every_output() {
    let mono = [0.5f64; 8];
    let mut a = [0.0f64; 8];
    let mut b = [0.0f64; 8];
    let inputs = [&mono[..]];
    let mut outputs = [&mut a[..], &mut b[..]];
    AudioBuffer::separate(&inputs, &mut outputs, 8)
        .unwrap()
        .pass_through();
    assert!(a.iter().chain(&b).all(|s| *s == 0.5));
}

#[test]
fn no_inputs_clear_and_in_place_is_unchanged() {
    let mut a = [3.0f32; 4];
    let mut outputs = [&mut a[..]];
    AudioBuffer::separate(&[], &mut outputs, 4)
        .unwrap()
        .pass_through();
    assert_eq!(a, [0.0; 4]);

    let mut samples = [0.25f32; 4];
    let mut channels = [&mut samples[..]];
    AudioBuffer::in_place(&mut channels, 4)
        .unwrap()
        .pass_through();
    assert_eq!(samples, [0.25; 4]);
}
