use ravet_embedded::FixedBlockAdapter;
use ravet_embedded_gain::{GAIN_ID, Gain};
use ravet_foundation::*;
#[test]
fn timed_events_flushes_rejection_and_partitioning() {
    let mut audio = FixedBlockAdapter::<_, 2, 2, 64>::new(Gain::new());
    audio
        .prepare(ProcessConfig::new(48000.0, 64, 8, AudioLayout::STEREO).unwrap())
        .unwrap();
    let event = |offset, value| Event {
        frame_offset: offset,
        data: EventData::ParameterValue { id: GAIN_ID, value },
    };
    let mut output = [99.0; 10];
    audio
        .process_interleaved(
            &[1.0, -1.0, 1.0, -1.0, 1.0, -1.0],
            &mut output,
            3,
            &[event(0, 2.0), event(1, 3.0), event(1, 4.0)],
            Transport::default(),
        )
        .unwrap();
    assert_eq!(
        output,
        [2.0, -2.0, 4.0, -4.0, 4.0, -4.0, 99.0, 99.0, 99.0, 99.0]
    );
    assert!(
        audio
            .process_interleaved(
                &[1.0; 6],
                &mut output,
                3,
                &[event(0, 1.0), event(2, 5.0)],
                Transport::default()
            )
            .is_err()
    );
    assert_eq!(audio.processor().gain(), 4.0);
    assert_eq!(output[..6], [0.0; 6]);
    audio
        .process_interleaved(
            &[] as &[f32],
            &mut output,
            0,
            &[event(0, 0.5)],
            Transport::default(),
        )
        .unwrap();
    assert_eq!(audio.processor().gain(), 0.5);
    audio.reset();
    let mut split = [0.0; 6];
    for pair in split.chunks_mut(2) {
        audio
            .process_interleaved(&[1.0, -1.0], pair, 1, &[], Transport::default())
            .unwrap();
    }
    assert_eq!(split, [0.5, -0.5, 0.5, -0.5, 0.5, -0.5]);
}

#[allow(unsafe_code)]
#[test]
fn c_reference_layout_aliasing_rejection_and_heap() {
    use ravet_embedded_gain::*;
    assert_eq!(core::mem::size_of::<Gain>(), 16);
    let mut state = Gain::new();
    let mut data = [1.0_f32, -1.0, 1.0, -1.0, 99.0, 99.0];
    unsafe {
        assert_eq!(ravet_embedded_gain_init(&mut state, 48000.0, 64), 0);
        assert_eq!(ravet_embedded_gain_set(&mut state, 2.0), 0);
        let counts = allocation_counter::measure(|| {
            assert_eq!(
                ravet_embedded_gain_process(&mut state, data.as_ptr(), 6, data.as_mut_ptr(), 6, 2),
                0
            );
        });
        assert_eq!(counts.count_total, 0);
        assert_eq!(counts.count_current, 0);
        assert_eq!(counts.bytes_current, 0);
        assert_eq!(data, [2.0, -2.0, 2.0, -2.0, 99.0, 99.0]);
        assert_eq!(ravet_embedded_gain_set(&mut state, f32::NAN), -1);
        assert_eq!(state.gain(), 2.0);
        assert_eq!(ravet_embedded_gain_init(&mut state, 48000.0, 65), -1);
        assert_eq!(state.gain(), 2.0);
        assert_eq!(
            ravet_embedded_gain_process(&mut state, core::ptr::null(), 4, data.as_mut_ptr(), 6, 2),
            -1
        );
        assert_eq!(data, [0.0, 0.0, 0.0, 0.0, 99.0, 99.0]);
        assert_eq!(
            ravet_embedded_gain_process(
                &mut state,
                core::ptr::null(),
                0,
                core::ptr::null_mut(),
                0,
                0
            ),
            0
        );
        assert_eq!(
            ravet_embedded_gain_process(&mut state, data.as_ptr(), 6, data.as_mut_ptr(), 6, 65),
            -1
        );
    }
}
