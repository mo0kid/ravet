use ravet_foundation::*;
use ravet_instrument::{INSTRUMENT_LEVEL_ID, PerformancePhase, PerformanceSine};

trait TestSample: Sample + Into<f64> {
    fn value(value: f64) -> Self;
    const TOLERANCE: f64;
}
impl TestSample for f32 {
    fn value(value: f64) -> Self {
        value as f32
    }
    const TOLERANCE: f64 = 1e-7;
}
impl TestSample for f64 {
    fn value(value: f64) -> Self {
        value
    }
    const TOLERANCE: f64 = 1e-13;
}

fn event(offset: usize, data: EventData) -> Event {
    Event {
        frame_offset: offset,
        data,
    }
}
fn note(offset: usize, id: u32, phase: NotePhase, key: u8, velocity: f32) -> Event {
    event(
        offset,
        EventData::NoteWithId {
            id: NoteId(id),
            phase,
            channel: 3,
            key,
            velocity,
        },
    )
}
fn expr(offset: usize, kind: NoteExpression, value: f64) -> Event {
    event(
        offset,
        EventData::NoteExpression {
            target: NoteTarget::default(),
            expression: kind,
            value,
        },
    )
}
fn level(offset: usize, value: f32) -> Event {
    event(
        offset,
        EventData::ParameterValue {
            id: INSTRUMENT_LEVEL_ID,
            value,
        },
    )
}
fn prepared<S: TestSample, const N: usize>(
    rate: f64,
    frames: usize,
    capacity: usize,
    stereo: bool,
) -> PerformanceSine<N>
where
    PerformanceSine<N>: Processor<S>,
{
    let mut synth = PerformanceSine::new().unwrap();
    <PerformanceSine<N> as Processor<S>>::prepare(
        &mut synth,
        ProcessConfig::new(
            rate,
            frames,
            1024,
            if stereo {
                AudioLayout::STEREO_INSTRUMENT
            } else {
                AudioLayout::MONO_INSTRUMENT
            },
        )
        .unwrap()
        .with_output_events(capacity),
    )
    .unwrap();
    synth
}
fn render<S: TestSample, const N: usize>(
    synth: &mut PerformanceSine<N>,
    left: &mut [S],
    right: Option<&mut [S]>,
    frames: usize,
    events: &[Event],
    storage: Option<&mut [Event]>,
) -> (ProcessStatus, usize)
where
    PerformanceSine<N>: Processor<S>,
{
    let mut channels = [left, &mut []];
    let channel_count = if let Some(right) = right {
        channels[1] = right;
        2
    } else {
        1
    };
    let audio = AudioBuffer::separate(&[], &mut channels[..channel_count], frames).unwrap();
    let mut block = ProcessBlock::new(audio, events, Transport::default());
    block.output_events = storage.map(|storage| OutputEvents::new(storage, frames));
    let status = synth.process(&mut block);
    (
        status,
        block.output_events.as_ref().map_or(0, OutputEvents::len),
    )
}
const PLACEHOLDER: Event = Event {
    frame_offset: 0,
    data: EventData::NoteEnd {
        target: NoteTarget {
            channel: None,
            key: None,
            id: None,
        },
    },
};

fn analytic_and_partitioning<S: TestSample>()
where
    PerformanceSine<1>: Processor<S>,
{
    let mut whole = prepared::<S, 1>(48000., 512, 0, true);
    let mut split = prepared::<S, 1>(48000., 512, 0, true);
    let mut mono = prepared::<S, 1>(48000., 512, 0, false);
    let events = [
        note(3, u32::MAX, NotePhase::On, 69, 0.12345679),
        level(3, 0.5),
        expr(3, NoteExpression::Tuning, 12.),
        expr(3, NoteExpression::Volume, 2.),
        expr(3, NoteExpression::Expression, 0.7),
        expr(3, NoteExpression::Pan, 0.),
    ];
    let mut left = [S::value(99.); 302];
    let mut right = [S::value(88.); 302];
    assert_eq!(
        render(&mut whole, &mut left, Some(&mut right), 300, &events, None).0,
        ProcessStatus::Continue
    );
    assert!(right[..300].iter().all(|v| (*v).into() == 0.));
    assert_eq!(left[300].into(), 99.);
    assert_eq!(right[300].into(), 88.);
    for (index, sample) in left[3..243].iter().enumerate() {
        let expected = (std::f64::consts::TAU * 880. / 48000. * index as f64).sin()
            * (index + 1) as f64
            / 240.
            * f64::from(0.12345679f32)
            * 2.
            * 0.7;
        assert!(((*sample).into() - expected).abs() < S::TOLERANCE);
    }
    let mut a = [S::value(0.); 300];
    let mut b = [S::value(0.); 300];
    render(
        &mut split,
        &mut a[..100],
        Some(&mut b[..100]),
        100,
        &events,
        None,
    );
    render(
        &mut split,
        &mut a[100..],
        Some(&mut b[100..]),
        200,
        &[],
        None,
    );
    for i in 0..300 {
        assert_eq!(a[i].into(), left[i].into());
        assert_eq!(b[i].into(), right[i].into());
    }
    render(&mut mono, &mut a, None, 300, &events, None);
    for i in 0..300 {
        assert!((2. * a[i].into() - left[i].into()).abs() < S::TOLERANCE);
    }
    assert_eq!(
        whole.voices()[0].velocity.to_bits(),
        0.12345679f32.to_bits()
    );
    assert_eq!(whole.voices()[0].target.unwrap().id, Some(NoteId(u32::MAX)));
}

fn timed_expression_and_completion<S: TestSample>()
where
    PerformanceSine<1>: Processor<S>,
{
    let mut synth = prepared::<S, 1>(48000., 4096, 16, true);
    let mut reference = prepared::<S, 1>(48000., 4096, 16, true);
    let mut left = [S::value(9.); 300];
    let mut right = [S::value(9.); 300];
    let mut a = [S::value(0.); 300];
    let mut b = [S::value(0.); 300];
    let mut output = [PLACEHOLDER; 16];
    let start = [note(0, 0x8000_0001, NotePhase::On, 69, 1.), level(0, 1.)];
    let edits = [
        start[0],
        start[1],
        expr(50, NoteExpression::Volume, -0.),
        expr(100, NoteExpression::Volume, 1.),
        expr(150, NoteExpression::Pan, 1.),
        expr(200, NoteExpression::Pressure, f64::from_bits(1)),
        expr(200, NoteExpression::Brightness, 0.),
    ];
    assert_eq!(
        render(
            &mut synth,
            &mut left,
            Some(&mut right),
            300,
            &edits,
            Some(&mut output)
        ),
        (ProcessStatus::Continue, 0)
    );
    render(
        &mut reference,
        &mut a,
        Some(&mut b),
        300,
        &start,
        Some(&mut output),
    );
    for i in 0..300 {
        if (50..100).contains(&i) {
            assert_eq!(left[i].into(), 0.);
            assert_eq!(right[i].into(), 0.);
        } else if i >= 150 {
            assert_eq!(left[i].into(), 0.);
            assert!((right[i].into() - 2. * b[i].into()).abs() < S::TOLERANCE);
        } else {
            assert_eq!(left[i].into(), a[i].into());
            assert_eq!(right[i].into(), b[i].into());
        }
    }
    assert_eq!(synth.voices()[0].phase, PerformancePhase::Held);
    let mut release = [S::value(9.); 2402];
    let mut release_right = [S::value(9.); 2402];
    let events = [
        note(0, 0x8000_0001, NotePhase::Off, 69, 0.12345679),
        note(100, 0x8000_0001, NotePhase::Off, 69, 1.),
    ];
    assert_eq!(
        render(
            &mut synth,
            &mut release,
            Some(&mut release_right),
            2400,
            &events,
            Some(&mut output)
        ),
        (ProcessStatus::Continue, 1)
    );
    assert_eq!(
        output[0],
        event(
            2399,
            EventData::NoteEnd {
                target: NoteTarget {
                    channel: Some(3),
                    key: Some(69),
                    id: Some(NoteId(0x8000_0001))
                }
            }
        )
    );
    assert_eq!(synth.voices()[0].phase, PerformancePhase::Idle);
    assert_eq!(release[2400].into(), 9.);
    assert_eq!(release_right[2400].into(), 9.);
    assert_eq!(
        render(
            &mut synth,
            &mut [],
            Some(&mut []),
            0,
            &[],
            Some(&mut output)
        ),
        (ProcessStatus::Continue, 0)
    );
    // Zero velocity still owns an ID, and zero-frame processing does not advance it.
    render(
        &mut synth,
        &mut [],
        Some(&mut []),
        0,
        &[note(0, u32::MAX, NotePhase::On, 60, -0.)],
        Some(&mut output),
    );
    assert_eq!(synth.voices()[0].phase, PerformancePhase::Held);
    assert_eq!(synth.voices()[0].velocity.to_bits(), (-0f32).to_bits());
    assert_eq!(
        render(
            &mut synth,
            &mut left,
            Some(&mut right),
            300,
            &[],
            Some(&mut output)
        ),
        (ProcessStatus::Continue, 0)
    );
    assert!(left.iter().all(|v| (*v).into() == 0.));
    let choke = event(
        0,
        EventData::NoteChoke {
            target: NoteTarget::default(),
        },
    );
    assert_eq!(
        render(
            &mut synth,
            &mut [],
            Some(&mut []),
            0,
            &[choke],
            Some(&mut output)
        ),
        (ProcessStatus::Continue, 1)
    );
    assert!(matches!(
        output[0].data,
        EventData::NoteEnd {
            target: NoteTarget {
                id: Some(NoteId(u32::MAX)),
                ..
            }
        }
    ));
}

fn transactional_faults_and_actual_id_reuse<S: TestSample>()
where
    PerformanceSine<1>: Processor<S>,
{
    let mut synth = prepared::<S, 1>(48000., 4096, 1, false);
    let mut reference = prepared::<S, 1>(48000., 4096, 1, false);
    let mut output = [PLACEHOLDER; 1];
    let mut audio = [S::value(0.); 300];
    let mut expected = [S::value(0.); 300];
    let start = [note(0, 7, NotePhase::On, 69, 1.)];
    render(&mut synth, &mut audio, None, 300, &start, Some(&mut output));
    render(
        &mut reference,
        &mut expected,
        None,
        300,
        &start,
        Some(&mut output),
    );
    let before = *synth.voices();
    let bad = [
        level(0, 0.9),
        expr(1, NoteExpression::Tuning, 12.),
        note(299, 7, NotePhase::On, 70, 1.),
    ];
    audio.fill(S::value(99.));
    assert_eq!(
        render(&mut synth, &mut audio, None, 298, &bad, Some(&mut output)).0,
        ProcessStatus::Fault(ProcessError::Events(EventError::OffsetOutOfRange {
            index: 2,
            offset: 299,
            frames: 298
        }))
    );
    assert!(audio[..298].iter().all(|v| (*v).into() == 0.));
    assert_eq!(audio[298].into(), 99.);
    assert_eq!(
        render(&mut synth, &mut audio, None, 300, &bad, Some(&mut output)),
        (
            ProcessStatus::Fault(ProcessError::Events(EventError::InvalidValue { index: 2 })),
            0
        )
    );
    assert_eq!(*synth.voices(), before);
    assert_eq!(synth.level(), 0.2);
    let overflow = [
        level(0, 1.),
        note(0, 8, NotePhase::On, 70, 1.),
        note(299, 9, NotePhase::On, 71, 1.),
    ];
    assert_eq!(
        render(
            &mut synth,
            &mut audio,
            None,
            300,
            &overflow,
            Some(&mut output)
        ),
        (ProcessStatus::Fault(ProcessError::OutputEventCapacity), 0)
    );
    assert_eq!(*synth.voices(), before);
    assert_eq!(synth.level(), 0.2);
    render(&mut synth, &mut audio, None, 300, &[], Some(&mut output));
    render(
        &mut reference,
        &mut expected,
        None,
        300,
        &[],
        Some(&mut output),
    );
    for i in 0..300 {
        assert_eq!(audio[i].into(), expected[i].into());
    }
    let mut long = [S::value(99.); 3000];
    // The release completes at2399; a different address can reuse that ID at2400.
    let events = [
        note(0, 7, NotePhase::Off, 69, 0.),
        note(2400, 7, NotePhase::On, 70, 0.12345679),
    ];
    assert_eq!(
        render(
            &mut synth,
            &mut long,
            None,
            3000,
            &events,
            Some(&mut output)
        ),
        (ProcessStatus::Continue, 1)
    );
    assert_eq!(output[0].frame_offset, 2399);
    assert!(matches!(
        output[0].data,
        EventData::NoteEnd {
            target: NoteTarget {
                key: Some(69),
                id: Some(NoteId(7)),
                ..
            }
        }
    ));
    assert_eq!(synth.voices()[0].target.unwrap().key, Some(70));
    assert_eq!(synth.voices()[0].phase, PerformancePhase::Held);
    assert_eq!(
        render(
            &mut synth,
            &mut [],
            None,
            0,
            &[note(0, 7, NotePhase::On, 70, 1.)],
            Some(&mut output)
        ),
        (ProcessStatus::Continue, 0)
    );
    // Capacity zero suppresses notifications but not ownership completion.
    let mut silent_notifications = prepared::<S, 1>(48000., 8, 0, false);
    assert_eq!(
        render(
            &mut silent_notifications,
            &mut [],
            None,
            0,
            &[
                note(0, 1, NotePhase::On, 60, 1.),
                note(0, 2, NotePhase::On, 61, 1.)
            ],
            None
        )
        .0,
        ProcessStatus::Continue
    );
    assert_eq!(
        silent_notifications.voices()[0].target.unwrap().id,
        Some(NoteId(2))
    );
}

fn lifecycle_rates_and_heap<S: TestSample>()
where
    PerformanceSine<1>: Processor<S>,
    PerformanceSine<128>: Processor<S>,
{
    let mut synth = PerformanceSine::<1>::new().unwrap();
    let mut samples = [S::value(99.); 16];
    for rate in [f64::from_bits(1), 1e-300, 1., 8000., 48000., 768000.] {
        <PerformanceSine<1> as Processor<S>>::prepare(
            &mut synth,
            ProcessConfig::new(rate, 16, 8, AudioLayout::MONO_INSTRUMENT).unwrap(),
        )
        .unwrap();
        let events = [
            note(0, 1, NotePhase::On, 127, 1.),
            expr(0, NoteExpression::Tuning, 120.),
            expr(0, NoteExpression::Vibrato, 1.),
        ];
        assert_eq!(
            render(&mut synth, &mut samples, None, 16, &events, None).0,
            ProcessStatus::Continue
        );
        assert!(samples.iter().all(|v| (*v).into().is_finite()));
    }
    assert_eq!(
        <PerformanceSine<1> as Processor<S>>::prepare(
            &mut synth,
            ProcessConfig::new(1e20, 16, 8, AudioLayout::MONO_INSTRUMENT).unwrap()
        ),
        Err(PrepareError::InvalidSmoothingTime)
    );
    assert_eq!(
        render(&mut synth, &mut samples, None, 16, &[], None).0,
        ProcessStatus::Fault(ProcessError::NotPrepared)
    );
    assert!(PerformanceSine::<0>::new().is_err());
    assert!(PerformanceSine::<129>::new().is_err());
    for config in [
        ProcessConfig::new(48000., 65537, 8, AudioLayout::MONO_INSTRUMENT).unwrap(),
        ProcessConfig::new(48000., 16, 1025, AudioLayout::MONO_INSTRUMENT).unwrap(),
        ProcessConfig::new(48000., 16, 8, AudioLayout::MONO_INSTRUMENT)
            .unwrap()
            .with_output_events(1025),
    ] {
        assert!(<PerformanceSine<1> as Processor<S>>::prepare(&mut synth, config).is_err());
    }
    assert!(
        <PerformanceSine<1> as Processor<S>>::prepare(
            &mut synth,
            ProcessConfig::new(48000., 16, 8, AudioLayout::MONO).unwrap()
        )
        .is_err()
    );
    assert!(
        <PerformanceSine<1> as Processor<S>>::prepare(
            &mut synth,
            ProcessConfig::new(48000., 16, 8, AudioLayout::MONO_INSTRUMENT)
                .unwrap()
                .with_auxiliary_buses(&[AudioLayout::MONO])
                .unwrap()
        )
        .is_err()
    );
    synth.set_level(-0.).unwrap();
    assert_eq!(synth.level().to_bits(), (-0f32).to_bits());
    for value in [-f32::from_bits(1), 1.00001, f32::NAN, f32::INFINITY] {
        assert!(synth.set_level(value).is_err());
        assert_eq!(synth.level().to_bits(), (-0f32).to_bits());
    }
    let mut synth = prepared::<S, 128>(8000., 512, 128, false);
    let starts: [Event; 128] = std::array::from_fn(|id| {
        note(0, u32::MAX - id as u32, NotePhase::On, id as u8, 0.12345679)
    });
    let off = event(
        0,
        EventData::NoteOffScoped {
            target: NoteTarget::default(),
            velocity: 0.1,
        },
    );
    let mut audio = [S::value(0.); 512];
    let mut output = [PLACEHOLDER; 128];
    let counts = allocation_counter::measure(|| {
        for _ in 0..100 {
            <PerformanceSine<128> as Processor<S>>::reset(&mut synth);
            assert_eq!(
                render(
                    &mut synth,
                    &mut audio,
                    None,
                    128,
                    &starts,
                    Some(&mut output)
                ),
                (ProcessStatus::Continue, 0)
            );
            assert_eq!(
                render(&mut synth, &mut audio, None, 512, &[off], Some(&mut output)),
                (ProcessStatus::Continue, 128)
            );
            for (index, event) in output.iter().enumerate() {
                assert_eq!(event.frame_offset, 399);
                assert!(
                    matches!(event.data, EventData::NoteEnd { target: NoteTarget { id: Some(NoteId(id)), .. } } if id == u32::MAX - index as u32)
                );
            }
        }
    });
    assert_eq!(counts.count_total, 0);
    assert_eq!(counts.count_current, 0);
    let level = synth.level();
    <PerformanceSine<128> as Processor<S>>::deactivate(&mut synth);
    assert_eq!(synth.level(), level);
    assert_eq!(
        render(&mut synth, &mut audio, None, 512, &[], Some(&mut output)).0,
        ProcessStatus::Fault(ProcessError::NotPrepared)
    );
}

macro_rules! tests {
    ($module:ident, $sample:ty) => {
        mod $module {
            use super::*;
            #[test]
            fn analytic_partitioning() {
                analytic_and_partitioning::<$sample>();
            }
            #[test]
            fn timed_expression_completion() {
                timed_expression_and_completion::<$sample>();
            }
            #[test]
            fn faults_actual_id_reuse() {
                transactional_faults_and_actual_id_reuse::<$sample>();
            }
            #[test]
            fn lifecycle_heap() {
                lifecycle_rates_and_heap::<$sample>();
            }
        }
    };
}
tests!(narrow, f32);
tests!(wide, f64);
