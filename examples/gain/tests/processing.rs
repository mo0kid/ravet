use ravet::foundation::*;
use ravet_gain::{GAIN_ID, GainProcessor};

fn gain_event(offset: usize, value: f32) -> Event {
    Event {
        frame_offset: offset,
        data: EventData::ParameterValue { id: GAIN_ID, value },
    }
}
fn config(rate: f64, frames: usize, events: usize, layout: AudioLayout) -> ProcessConfig {
    ProcessConfig::new(rate, frames, events, layout).unwrap()
}
fn process(processor: &mut GainProcessor, samples: &mut [f32], events: &[Event]) -> ProcessStatus {
    let frames = samples.len();
    let mut channels = [samples];
    let audio = AudioBuffer::in_place(&mut channels, frames).unwrap();
    processor.process(&mut ProcessBlock::new(audio, events, Transport::default()))
}

#[test]
fn mono_fixture_applies_events_before_the_exact_sample_and_preserves_order() {
    let mut processor = GainProcessor::new(0.0).unwrap();
    processor
        .prepare(config(48_000.0, 6, 4, AudioLayout::MONO))
        .unwrap();
    let mut samples = [1.0, -1.0, 2.0, -2.0, 0.5, 0.0];
    let events = [
        gain_event(0, 0.5),
        gain_event(2, 0.0),
        gain_event(2, 2.0),
        gain_event(5, 4.0),
    ];
    assert_eq!(
        process(&mut processor, &mut samples, &events),
        ProcessStatus::Continue
    );
    assert_eq!(samples, [0.5, -0.5, 4.0, -4.0, 1.0, 0.0]);
    assert_eq!(processor.requested_gain(), 4.0);
}

#[test]
fn separate_stereo_matches_in_place_and_leaves_input_and_tails_untouched() {
    let left = [1.0, -2.0, 0.25, 7.0];
    let right = [0.5, 2.0, -0.5, 8.0];
    let inputs = [&left[..], &right[..]];
    let mut out_left = [f32::NAN, f32::NAN, f32::NAN, 9.0];
    let mut out_right = [f32::NAN, f32::NAN, f32::NAN, 10.0, 11.0];
    let mut in_left = left;
    let mut in_right = right;
    let events = [gain_event(0, 0.0)];
    let mut separate = GainProcessor::new(3.0).unwrap();
    let mut inplace = GainProcessor::new(3.0).unwrap();
    let prepared = config(1000.0, 4, 4, AudioLayout::STEREO);
    separate.prepare(prepared).unwrap();
    inplace.prepare(prepared).unwrap();
    let mut outputs = [&mut out_left[..], &mut out_right[..]];
    let audio = AudioBuffer::separate(&inputs, &mut outputs, 3).unwrap();
    assert_eq!(
        separate.process(&mut ProcessBlock::new(audio, &events, Transport::default())),
        ProcessStatus::Continue
    );
    let mut channels = [&mut in_left[..], &mut in_right[..]];
    let audio = AudioBuffer::in_place(&mut channels, 3).unwrap();
    assert_eq!(
        inplace.process(&mut ProcessBlock::new(audio, &events, Transport::default())),
        ProcessStatus::Continue
    );
    assert_eq!(&out_left[..3], &in_left[..3]);
    assert_eq!(&out_right[..3], &in_right[..3]);
    assert_eq!(out_left[2], 0.0);
    assert_eq!(out_right[2], 0.0);
    assert_eq!(out_left[3], 9.0);
    assert_eq!(&out_right[3..], &[10.0, 11.0]);
    assert_eq!(left, [1.0, -2.0, 0.25, 7.0]);
    assert_eq!(right, [0.5, 2.0, -0.5, 8.0]);
}

#[test]
fn zero_frames_apply_parameters_without_advancing_smoothing() {
    let mut processor = GainProcessor::new(4.0).unwrap();
    processor
        .prepare(config(1000.0, 4, 4, AudioLayout::MONO))
        .unwrap();
    assert_eq!(
        process(&mut processor, &mut [], &[gain_event(0, 0.0)]),
        ProcessStatus::Continue
    );
    assert_eq!(processor.requested_gain(), 0.0);
    assert_eq!(processor.effective_gain(), 1.0);
    let mut samples = [1.0; 4];
    process(&mut processor, &mut samples, &[]);
    assert_eq!(samples, [0.75, 0.5, 0.25, 0.0]);
}

#[test]
fn malformed_block_is_atomic_silent_and_does_not_reset_a_ramp() {
    let mut processor = GainProcessor::new(4.0).unwrap();
    processor
        .prepare(config(1000.0, 4, 4, AudioLayout::MONO))
        .unwrap();
    let mut first = [1.0];
    process(&mut processor, &mut first, &[gain_event(0, 0.0)]);
    assert_eq!(first, [0.75]);
    let mut bad = [9.0; 4];
    assert_eq!(
        process(
            &mut processor,
            &mut bad,
            &[gain_event(0, 2.0), gain_event(4, 3.0)]
        ),
        ProcessStatus::Fault(ProcessError::Events(EventError::OffsetOutOfRange {
            index: 1,
            offset: 4,
            frames: 4
        }))
    );
    assert_eq!(bad, [0.0; 4]);
    assert_eq!(processor.requested_gain(), 0.0);
    assert_eq!(processor.effective_gain(), 0.75);
    let mut rest = [1.0; 3];
    process(&mut processor, &mut rest, &[]);
    assert_eq!(rest, [0.5, 0.25, 0.0]);
}

#[test]
fn invalid_values_order_capacity_and_frames_silence_outputs() {
    let cases = [
        (
            vec![gain_event(0, f32::NAN)],
            2,
            ProcessError::Events(EventError::InvalidValue { index: 0 }),
        ),
        (
            vec![gain_event(0, f32::INFINITY)],
            2,
            ProcessError::Events(EventError::InvalidValue { index: 0 }),
        ),
        (
            vec![gain_event(1, 0.5), gain_event(0, 2.0)],
            2,
            ProcessError::Events(EventError::Unordered { index: 1 }),
        ),
        (
            vec![gain_event(0, 0.5); 3],
            2,
            ProcessError::Events(EventError::TooMany {
                count: 3,
                maximum: 2,
            }),
        ),
        (
            vec![],
            5,
            ProcessError::TooManyFrames {
                frames: 5,
                maximum: 4,
            },
        ),
    ];
    for (events, frames, error) in cases {
        let mut processor = GainProcessor::new(0.0).unwrap();
        processor
            .prepare(config(48_000.0, 4, 2, AudioLayout::MONO))
            .unwrap();
        let mut samples = vec![f32::NAN; frames];
        assert_eq!(
            process(&mut processor, &mut samples, &events),
            ProcessStatus::Fault(error)
        );
        assert_eq!(samples, vec![0.0; frames]);
        assert_eq!(processor.requested_gain(), 1.0);
    }
}

#[test]
fn lifecycle_reprepare_reset_and_failed_prepare() {
    let mut processor = GainProcessor::new(4.0).unwrap();
    let mut samples = [9.0];
    assert_eq!(
        process(&mut processor, &mut samples, &[]),
        ProcessStatus::Fault(ProcessError::NotPrepared)
    );
    assert_eq!(samples, [0.0]);
    processor
        .prepare(config(1000.0, 4, 4, AudioLayout::MONO))
        .unwrap();
    process(&mut processor, &mut [1.0], &[gain_event(0, 0.0)]);
    processor.reset();
    assert_eq!(processor.requested_gain(), 0.0);
    assert_eq!(processor.effective_gain(), 0.0);
    processor.deactivate();
    assert_eq!(
        process(&mut processor, &mut samples, &[]),
        ProcessStatus::Fault(ProcessError::NotPrepared)
    );
    processor
        .prepare(config(2000.0, 8, 4, AudioLayout::MONO))
        .unwrap();
    let mut at_new_rate = [1.0; 8];
    process(&mut processor, &mut at_new_rate, &[gain_event(0, 1.0)]);
    assert_eq!(
        at_new_rate,
        [0.125, 0.25, 0.375, 0.5, 0.625, 0.75, 0.875, 1.0]
    );
    assert_eq!(
        processor.prepare(config(48_000.0, 4, 4, AudioLayout::MONO_INSTRUMENT)),
        Err(PrepareError::UnsupportedLayout)
    );
    assert_eq!(
        process(&mut processor, &mut samples, &[]),
        ProcessStatus::Fault(ProcessError::NotPrepared)
    );
}

#[test]
fn mismatched_layout_is_silent_and_unknown_ids_and_notes_are_ignored() {
    let mut processor = GainProcessor::new(0.0).unwrap();
    processor
        .prepare(config(48_000.0, 4, 4, AudioLayout::STEREO))
        .unwrap();
    let mut samples = [3.0];
    assert_eq!(
        process(&mut processor, &mut samples, &[]),
        ProcessStatus::Fault(ProcessError::LayoutMismatch)
    );
    assert_eq!(samples, [0.0]);
    processor
        .prepare(config(48_000.0, 4, 4, AudioLayout::MONO))
        .unwrap();
    let events = [
        Event {
            frame_offset: 0,
            data: EventData::ParameterValue {
                id: ParameterId(99),
                value: 0.0,
            },
        },
        Event {
            frame_offset: 0,
            data: EventData::NoteOn {
                channel: 0,
                key: 60,
                velocity: 1.0,
            },
        },
    ];
    samples = [3.0];
    assert_eq!(
        process(&mut processor, &mut samples, &events),
        ProcessStatus::Continue
    );
    assert_eq!(samples, [3.0]);
}

#[test]
fn finite_gain_is_clamped_and_bad_smoothing_is_rejected() {
    let mut processor = GainProcessor::new(0.0).unwrap();
    processor
        .prepare(config(48_000.0, 4, 4, AudioLayout::MONO))
        .unwrap();
    let mut samples = [1.0; 2];
    process(
        &mut processor,
        &mut samples,
        &[gain_event(0, -10.0), gain_event(1, 10.0)],
    );
    assert_eq!(samples, [0.0, 4.0]);
    for duration in [-1.0, f64::NAN, f64::INFINITY] {
        assert!(GainProcessor::new(duration).is_err());
    }
    assert_eq!(
        processor.prepare(config(f64::MAX, 4, 4, AudioLayout::MONO)),
        Ok(())
    );
    let mut slow = GainProcessor::new(5.0).unwrap();
    assert_eq!(
        slow.prepare(config(f64::MAX, 4, 4, AudioLayout::MONO)),
        Err(PrepareError::InvalidSmoothingTime)
    );
}

fn render_partition(partitions: &[usize], separate: bool, smoothing_ms: f64) -> Vec<f32> {
    let total: usize = partitions.iter().sum();
    let mut processor = GainProcessor::new(smoothing_ms).unwrap();
    processor
        .prepare(config(48_000.0, total, 32, AudioLayout::MONO))
        .unwrap();
    let absolute_events = [
        gain_event(0, 0.0),
        gain_event(7, 2.0),
        gain_event(7, 0.5),
        gain_event(16, 0.5),
        gain_event(23, 4.0),
        gain_event(37, 1.0),
        gain_event(total - 1, 0.0),
    ];
    let source: Vec<f32> = (0..total).map(|i| ((i % 11) as f32 - 5.0) / 5.0).collect();
    let mut output = source.clone();
    let mut position = 0;
    for &frames in partitions {
        let events: Vec<Event> = absolute_events
            .iter()
            .filter(|e| e.frame_offset >= position && e.frame_offset < position + frames)
            .map(|e| Event {
                frame_offset: e.frame_offset - position,
                data: e.data,
            })
            .collect();
        let status = if separate {
            let inputs = [&source[position..position + frames]];
            let mut outputs = [&mut output[position..position + frames]];
            let audio = AudioBuffer::separate(&inputs, &mut outputs, frames).unwrap();
            processor.process(&mut ProcessBlock::new(audio, &events, Transport::default()))
        } else {
            process(
                &mut processor,
                &mut output[position..position + frames],
                &events,
            )
        };
        assert_eq!(status, ProcessStatus::Continue);
        position += frames;
    }
    output
}

#[test]
fn event_and_smoothing_results_are_exactly_invariant_under_block_partitioning() {
    for smoothing_ms in [0.0, 0.25, 5.0] {
        let expected = render_partition(&[64], false, smoothing_ms);
        for separate in [false, true] {
            for partition in [
                vec![1; 64],
                vec![7, 9, 7, 14, 27],
                vec![0, 16, 0, 16, 0, 32, 0],
                vec![3, 5, 2, 17, 1, 36],
            ] {
                assert_eq!(
                    render_partition(&partition, separate, smoothing_ms),
                    expected,
                    "partition {partition:?}, smoothing {smoothing_ms}"
                );
            }
        }
    }
}

#[test]
fn extended_framework_layouts_remain_explicitly_unsupported_by_gain() {
    let base = config(48_000.0, 8, 0, AudioLayout::MONO);
    let unsupported = [
        config(48_000.0, 8, 0, AudioLayout::new(4, 4).unwrap()),
        config(48_000.0, 8, 0, AudioLayout::MONO_INSTRUMENT),
        base.with_auxiliary_buses(&[AudioLayout::new(1, 0).unwrap()])
            .unwrap(),
        base.with_output_events(1),
    ];
    let mut gain = GainProcessor::new(0.).unwrap();
    for config in unsupported {
        gain.prepare(base).unwrap();
        assert_eq!(gain.prepare(config), Err(PrepareError::UnsupportedLayout));
        let mut samples = [3.; 8];
        assert_eq!(
            process(&mut gain, &mut samples, &[]),
            ProcessStatus::Fault(ProcessError::NotPrepared)
        );
        assert_eq!(samples, [0.; 8]);
    }
}
