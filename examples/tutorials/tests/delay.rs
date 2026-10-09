use ravet_examples::delay::DelayProcessor;
use ravet_foundation::{
    AudioBuffer, AudioLayout, Event, EventData, ParameterId, PrepareError, ProcessBlock,
    ProcessConfig, ProcessStatus, Processor, Transport,
};

fn parameter(frame_offset: usize, id: u32, value: f32) -> Event {
    Event {
        frame_offset,
        data: EventData::ParameterValue {
            id: ParameterId(id),
            value,
        },
    }
}
fn prepare(values: [f32; 5]) -> DelayProcessor {
    let mut processor = DelayProcessor::default();
    processor.set_controls(values).unwrap();
    processor
        .prepare(ProcessConfig::new(8000., 128, 16, AudioLayout::STEREO).unwrap())
        .unwrap();
    processor
}
fn process(
    processor: &mut DelayProcessor,
    input: &[Vec<f32>; 2],
    events: &[Event],
) -> ([Vec<f32>; 2], ProcessStatus) {
    let frames = input[0].len();
    let mut output = [vec![99.; frames], vec![99.; frames]];
    let inputs = input.each_ref().map(|c| &c[..]);
    let mut outputs = output.each_mut().map(|c| &mut c[..]);
    let buffer = AudioBuffer::separate(&inputs, &mut outputs, frames).unwrap();
    let status = processor.process(&mut ProcessBlock::new(buffer, events, Transport::default()));
    (output, status)
}
fn render(
    values: [f32; 5],
    input: &[Vec<f32>; 2],
    events: &[Event],
    block: usize,
) -> [Vec<f32>; 2] {
    let mut processor = prepare(values);
    let frames = input[0].len();
    let mut result = [Vec::new(), Vec::new()];
    for start in (0..frames).step_by(block) {
        let end = (start + block).min(frames);
        let clip = input.each_ref().map(|c| c[start..end].to_vec());
        let local: Vec<_> = events
            .iter()
            .filter(|e| (start..end).contains(&e.frame_offset))
            .map(|e| Event {
                frame_offset: e.frame_offset - start,
                ..*e
            })
            .collect();
        let (output, status) = process(&mut processor, &clip, &local);
        assert_eq!(status, ProcessStatus::Continue);
        for c in 0..2 {
            result[c].extend_from_slice(&output[c]);
        }
    }
    result
}

#[test]
fn stereo_repeats_and_ping_pong_cross_feedback_have_exact_timing() {
    let mut input = [vec![0.; 321], vec![0.; 321]];
    input[0][0] = 1.;
    input[1][7] = 0.25;
    for cross in [0., 1.] {
        let output = render([10., 0.5, 1., 0., cross], &input, &[], 37);
        assert_eq!(output[0][80], 1.);
        assert_eq!(output[1][87], 0.25);
        assert_eq!(output[if cross == 0. { 0 } else { 1 }][160], 0.5);
        assert_eq!(output[if cross == 0. { 1 } else { 0 }][167], 0.125);
        assert_eq!(output[0][240], 0.25);
        assert_eq!(output[1][247], 0.0625);
        assert_eq!(output.iter().flatten().filter(|s| **s != 0.).count(), 7);
    }
}

#[test]
fn automation_is_partition_independent_and_bypass_keeps_the_tail() {
    let input = [vec![0.25; 1200], vec![-0.125; 1200]];
    let events = [
        parameter(113, 1, 17.),
        parameter(117, 2, 0.8),
        parameter(129, 3, 0.9),
        parameter(203, 5, 1.),
        parameter(319, 4, 1.),
        parameter(557, 4, 0.),
    ];
    let values = [10., 0.5, 0.3, 0., 0.];
    let first = render(values, &input, &events, 128);
    assert_eq!(first, render(values, &input, &events, 19));
    for c in 0..2 {
        assert_eq!(first[c][319..557], input[c][319..557]);
    }

    let mut impulse = [vec![0.; 241], vec![0.; 241]];
    impulse[0][0] = 1.;
    let output = render(
        [10., 0.5, 1., 0., 0.],
        &impulse,
        &[parameter(79, 4, 1.), parameter(81, 4, 0.)],
        31,
    );
    assert_eq!(output[0][80], 0.);
    assert_eq!(output[0][160], 0.5);
    assert_eq!(output[0][240], 0.25);
}

#[test]
fn invalid_late_event_rejects_whole_block_and_preserves_history() {
    let mut processor = prepare([10., 0.5, 1., 0., 0.]);
    let mut reference = prepare([10., 0.5, 1., 0., 0.]);
    let mut input = [vec![0.; 64], vec![0.; 64]];
    input[0][0] = 1.;
    assert_eq!(
        process(&mut processor, &input, &[]),
        process(&mut reference, &input, &[])
    );
    let zero = [vec![0.; 64], vec![0.; 64]];
    let values = processor.controls();
    let (output, status) = process(
        &mut processor,
        &zero,
        &[parameter(0, 3, 0.2), parameter(63, 2, 1.)],
    );
    assert!(matches!(status, ProcessStatus::Fault(_)));
    assert!(output.iter().flatten().all(|s| *s == 0.));
    assert_eq!(processor.controls(), values);
    assert_eq!(
        process(&mut processor, &zero, &[]),
        process(&mut reference, &zero, &[])
    );
    for invalid in [
        [0., 0.5, 1., 0., 0.],
        [2001., 0.5, 1., 0., 0.],
        [10., f32::NAN, 1., 0., 0.],
        [10., 0.5, 1., 0.5, 0.],
    ] {
        assert!(processor.set_controls(invalid).is_err());
        assert_eq!(processor.controls(), values);
    }
    processor.reset();
    assert!(
        process(&mut processor, &zero, &[])
            .0
            .iter()
            .flatten()
            .all(|s| *s == 0.)
    );
    assert_eq!(
        processor.prepare(ProcessConfig::new(384000., 128, 16, AudioLayout::STEREO).unwrap()),
        Err(PrepareError::InvalidSampleRate)
    );
    assert!(matches!(
        process(&mut processor, &zero, &[]).1,
        ProcessStatus::Fault(_)
    ));
}

#[test]
fn prepared_processing_control_changes_and_reset_have_no_heap_actions() {
    let mut processor = prepare([10., 0.5, 0.5, 0., 0.]);
    let input = [[0.1f32; 128], [-0.1; 128]];
    let inputs = input.each_ref().map(|c| &c[..]);
    let mut output = [[0.; 128]; 2];
    let measured = allocation_counter::measure(|| {
        for i in 0..1000 {
            processor
                .set_controls([1. + (i % 2000) as f32, 0.95, 0.75, 0., (i % 2) as f32])
                .unwrap();
            let mut outputs = output.each_mut().map(|c| &mut c[..]);
            let buffer = AudioBuffer::separate(&inputs, &mut outputs, 128).unwrap();
            assert_eq!(
                processor.process(&mut ProcessBlock::new(buffer, &[], Transport::default())),
                ProcessStatus::Continue
            );
        }
        processor.reset();
    });
    assert_eq!(measured.count_total, 0);
    assert_eq!(measured.count_current, 0);
    assert!(output.iter().flatten().all(|s| s.is_finite()));

    // Maximum two-second delay and the mono layout use the same prepared path.
    let mut mono = DelayProcessor::default();
    mono.set_controls([2000., 0., 1., 0., 1.]).unwrap();
    mono.prepare(ProcessConfig::new(8000., 128, 0, AudioLayout::MONO).unwrap())
        .unwrap();
    let mut samples = [0.; 128];
    samples[0] = 1.;
    for block in 0..126 {
        let mut channels = [&mut samples[..]];
        let buffer = AudioBuffer::in_place(&mut channels, 128).unwrap();
        assert_eq!(
            mono.process(&mut ProcessBlock::new(buffer, &[], Transport::default())),
            ProcessStatus::Continue
        );
        assert_eq!(samples[0], if block == 125 { 1. } else { 0. });
        assert!(samples[1..].iter().all(|s| *s == 0.));
        samples.fill(0.);
    }
}
