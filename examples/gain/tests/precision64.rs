use ravet::foundation::*;
use ravet_gain::{BYPASS_ID, GAIN_ID, GainProcessor64, parameter_schema};
fn process(
    processor: &mut GainProcessor64,
    samples: &mut [f64],
    frames: usize,
    events: &[Event],
) -> ProcessStatus {
    let mut channels = [samples];
    let audio = AudioBuffer::in_place(&mut channels, frames).unwrap();
    processor.process(&mut ProcessBlock::new(audio, events, Transport::default()))
}
#[test]
fn native_precision_gain_bypass_extreme_meters_zero_frame_and_rejection() {
    let config = ProcessConfig::new(48000., 8, 8, AudioLayout::MONO).unwrap();
    let mut gain = GainProcessor64::new(0.).unwrap();
    gain.prepare(config).unwrap();
    let exact = 1. + f64::EPSILON;
    let mut samples = [exact, -exact, f64::MAX, 0., 99.];
    assert_eq!(
        process(&mut gain, &mut samples, 4, &[]),
        ProcessStatus::Continue
    );
    assert_eq!(samples, [exact, -exact, f64::MAX, 0., 99.]);
    assert!(gain.meters().output[0].rms.is_finite());
    let event = [
        Event {
            frame_offset: 1,
            data: EventData::ParameterValue {
                id: GAIN_ID,
                value: 0.5,
            },
        },
        Event {
            frame_offset: 2,
            data: EventData::ParameterValue {
                id: BYPASS_ID,
                value: 1.,
            },
        },
    ];
    let mut samples = [exact; 4];
    assert_eq!(
        process(&mut gain, &mut samples, 4, &event),
        ProcessStatus::Continue
    );
    assert_eq!(samples, [exact, exact * 0.5, exact, exact]);
    let previous = gain.meters();
    let bad = [Event {
        frame_offset: 4,
        data: EventData::ParameterValue {
            id: GAIN_ID,
            value: 3.,
        },
    }];
    samples.fill(exact);
    assert!(matches!(
        process(&mut gain, &mut samples, 4, &bad),
        ProcessStatus::Fault(_)
    ));
    assert_eq!(samples, [0.; 4]);
    assert_eq!(gain.meters(), previous);
    assert_eq!(gain.requested_gain(), 0.5);
    let event = [Event {
        frame_offset: 0,
        data: EventData::ParameterValue {
            id: GAIN_ID,
            value: 2.,
        },
    }];
    assert_eq!(
        process(&mut gain, &mut samples, 0, &event),
        ProcessStatus::Continue
    );
    assert_eq!(gain.meters(), previous);
    assert_eq!(gain.requested_gain(), 2.);
    gain.restore_parameters(&parameter_schema().state([1., 0.]).unwrap())
        .unwrap();
    assert_eq!(gain.parameter_state().values(), &[1., 0.]);
    gain.deactivate();
    assert!(matches!(
        process(&mut gain, &mut samples, 4, &[]),
        ProcessStatus::Fault(ProcessError::NotPrepared)
    ));
}
#[test]
fn smoothing_advances_during_bypass_and_prepared_callbacks_are_heap_free() {
    let mut gain = GainProcessor64::new(1.).unwrap();
    gain.prepare(ProcessConfig::new(4000., 8, 8, AudioLayout::MONO).unwrap())
        .unwrap();
    let event = [Event {
        frame_offset: 0,
        data: EventData::ParameterValue {
            id: GAIN_ID,
            value: 2.,
        },
    }];
    let mut samples = [1.; 4];
    assert_eq!(
        process(&mut gain, &mut samples, 4, &event),
        ProcessStatus::Continue
    );
    assert_eq!(samples, [1.25, 1.5, 1.75, 2.]);
    let mut samples = [1.; 4];
    let event = [
        Event {
            frame_offset: 0,
            data: EventData::ParameterValue {
                id: GAIN_ID,
                value: 1.,
            },
        },
        Event {
            frame_offset: 0,
            data: EventData::ParameterValue {
                id: BYPASS_ID,
                value: 1.,
            },
        },
    ];
    process(&mut gain, &mut samples, 4, &event);
    assert_eq!(samples, [1.; 4]);
    assert_eq!(gain.effective_gain(), 1.);
    let counts = allocation_counter::measure(|| {
        for _ in 0..10_000 {
            samples.fill(1. + f64::EPSILON);
            assert_eq!(
                process(&mut gain, &mut samples, 4, &event),
                ProcessStatus::Continue
            );
            gain.reset();
            assert_eq!(
                process(&mut gain, &mut samples, 0, &event),
                ProcessStatus::Continue
            );
        }
    });
    assert_eq!(counts.count_total, 0);
    assert_eq!(counts.count_current, 0);
}
