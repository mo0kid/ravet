//! Translator mapping and refusal checks without the bank registry.
use super::*;
use ravet_foundation::{
    Event as FrameworkEvent, MidiMessage, ParameterGesturePhase, TransportDetails,
};

fn parameter(id: u32, flags: u32) -> parameters::Parameter {
    parameters::Parameter {
        id,
        flags,
        steps: 0,
        unit: 0,
        default_normalized: 0.,
        title: "P".into(),
        short_title: String::new(),
        units: String::new(),
    }
}
fn catalogue() -> Vec<parameters::Parameter> {
    let mut c: Vec<_> = (100..170).map(|id| parameter(id, 1)).collect();
    c.extend([
        parameter(1, 1),
        parameter(5, 0),
        parameter(7, 2),
        parameter(9, 18),
    ]);
    c
}
/// Event input bus 0 has 16 channels, bus 1 has 2; bus 2 is unselected.
const BUSES: [u8; 16] = [16, 2, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0];
const RATE: f64 = 48000.;

fn routes() -> Vec<ParameterRoute> {
    vec![
        ParameterRoute::new(ParameterId(10), 1, ParameterScale::Normalized),
        ParameterRoute::new(
            ParameterId(20),
            5,
            ParameterScale::Linear {
                minimum: -12.,
                maximum: 12.,
            },
        ),
    ]
}
fn translation() -> FrameworkTranslation {
    let mut t = FrameworkTranslation::new(16, 16);
    t.parameters = routes();
    t.notes = Some(NoteRoute {
        bus: 0,
        ids: NoteIdPolicy::Unidentified,
        expressions: true,
        poly_pressure: true,
    });
    t
}
fn prepare(t: FrameworkTranslation) -> Result<Translator, TranslationError> {
    Translator::prepare(t, &catalogue(), &BUSES, None, RATE).map(|(t, _)| t)
}
fn at(frame_offset: usize, data: EventData) -> FrameworkEvent {
    FrameworkEvent { frame_offset, data }
}
fn value(id: u32, value: f32) -> EventData {
    EventData::ParameterValue {
        id: ParameterId(id),
        value,
    }
}
fn invalid(index: usize) -> Refusal {
    Refusal::Event(EventError::InvalidValue { index })
}
fn records(t: &Translator) -> Vec<RawEvent> {
    t.events.records[..t.events.count].to_vec()
}
fn points(t: &Translator) -> Vec<(u32, u32, f64)> {
    t.points
        .points()
        .iter()
        .map(|p| (p.id, p.offset, p.value))
        .collect()
}
fn note(id: i32, channel: u8, pitch: u8, velocity: f32) -> Note {
    Note {
        channel,
        pitch,
        note_id: id,
        tuning: 0.,
        velocity,
        length: 0,
    }
}
fn target(channel: Option<u8>, key: Option<u8>, id: Option<u32>) -> NoteTarget {
    NoteTarget {
        channel,
        key,
        id: id.map(NoteId),
    }
}

#[test]
fn parameter_values_map_to_exact_offsets_and_normalized_values() {
    let mut t = prepare(translation()).unwrap();
    let events = [
        at(0, value(10, 0.25)),
        at(3, value(20, 6.)),
        at(3, value(10, 1.)),
        at(7, value(20, -12.)),
        at(63, value(20, 12.)),
    ];
    assert_eq!(t.translate(&events, Transport::default(), 64), Ok(0));
    assert_eq!(
        points(&t),
        vec![
            (1, 0, 0.25),
            (5, 3, 0.75),
            (1, 3, 1.),
            (5, 7, 0.),
            (5, 63, 1.)
        ]
    );
    assert!(t.events.is_empty() && t.context.is_none());
    // Zero-frame blocks admit offset zero only.
    assert_eq!(
        t.translate(&[at(0, value(10, 0.5))], Transport::default(), 0),
        Ok(0)
    );
    assert_eq!(points(&t), vec![(1, 0, 0.5)]);
    assert_eq!(
        t.translate(&[at(1, value(10, 0.5))], Transport::default(), 0),
        Err(Refusal::Event(EventError::OffsetOutOfRange {
            index: 0,
            offset: 1,
            frames: 0
        }))
    );
}

#[test]
fn parameter_refusals_and_unmapped_policies() {
    let mut t = prepare(translation()).unwrap();
    let block = Transport::default();
    // Out-of-range plain, non-normalized and non-finite values refuse.
    for data in [value(20, 12.5), value(10, 1.25), value(10, f32::NAN)] {
        let events = [at(0, value(10, 0.)), at(1, data)];
        assert_eq!(t.translate(&events, block, 8), Err(invalid(1)));
    }
    // Same native ID at the same offset is a conflict under the default
    // policy, never coalesced.
    assert_eq!(
        t.translate(&[at(2, value(10, 0.1)), at(2, value(10, 0.2))], block, 8),
        Err(Refusal::SameOffset { index: 1 })
    );
    // Unmapped IDs refuse by default and are counted when ignored.
    assert_eq!(
        t.translate(&[at(0, value(99, 0.5))], block, 8),
        Err(Refusal::UnmappedParameter { index: 0 })
    );
    let mut ignoring = translation();
    ignoring.unmapped_parameters = EventPolicy::Ignore;
    let mut t = prepare(ignoring).unwrap();
    let events = [
        at(0, value(99, 0.5)),
        at(1, value(10, 0.5)),
        at(2, value(98, 0.)),
    ];
    assert_eq!(t.translate(&events, block, 8), Ok(2));
    assert_eq!(points(&t), vec![(1, 1, 0.5)]);
    // Unordered and out-of-block offsets refuse before mapping.
    assert_eq!(
        t.translate(&[at(3, value(10, 0.)), at(2, value(10, 0.))], block, 8),
        Err(Refusal::Event(EventError::Unordered { index: 1 }))
    );
    assert_eq!(
        t.translate(&[at(8, value(10, 0.))], block, 8),
        Err(Refusal::Event(EventError::OffsetOutOfRange {
            index: 0,
            offset: 8,
            frames: 8
        }))
    );
}

#[test]
fn capacity_overflow_refuses_the_whole_block() {
    let mut small = translation();
    small.points = 2;
    small.events = 1;
    let mut t = prepare(small).unwrap();
    let block = Transport::default();
    let events = [
        at(0, value(10, 0.)),
        at(1, value(10, 0.)),
        at(2, value(10, 0.)),
    ];
    assert_eq!(
        t.translate(&events, block, 8),
        Err(Refusal::Points {
            count: 3,
            maximum: 2
        })
    );
    let on = |offset| {
        at(
            offset,
            EventData::NoteOn {
                channel: 0,
                key: 60,
                velocity: 1.,
            },
        )
    };
    assert_eq!(
        t.translate(&[on(0), on(1)], block, 8),
        Err(Refusal::Events {
            count: 2,
            maximum: 1
        })
    );
    // A later block starts from cleared storage.
    assert_eq!(t.translate(&[on(4)], block, 8), Ok(0));
    assert_eq!(records(&t).len(), 1);
    assert!(t.points.points().is_empty());
    // The native queue admits at most 64 distinct IDs per block; the 65th is
    // reported as that limit, not as point capacity.
    let mut wide = FrameworkTranslation::new(0, 2048);
    wide.parameters = (0..65)
        .map(|i| ParameterRoute::new(ParameterId(i), 100 + i, ParameterScale::Normalized))
        .collect();
    let mut t = prepare(wide).unwrap();
    let events: Vec<_> = (0..65).map(|i| at(0, value(i, 0.5))).collect();
    assert_eq!(
        t.translate(&events, block, 8),
        Err(Refusal::ParameterIds {
            index: 64,
            maximum: 64
        })
    );
    assert_eq!(t.translate(&events[..64], block, 8), Ok(0));
    // Repeated known IDs at later offsets stay admissible after 64 distinct.
    let mut repeated = events[..64].to_vec();
    repeated.extend((0..64).map(|i| at(1, value(i, 0.25))));
    assert_eq!(t.translate(&repeated, block, 8), Ok(0));
    assert_eq!(t.points.points().len(), 128);
}

#[test]
fn notes_map_with_explicit_identity_policy() {
    let mut t = prepare(translation()).unwrap();
    let block = Transport::default();
    let events = [
        at(
            1,
            EventData::NoteOn {
                channel: 3,
                key: 60,
                velocity: 0.75,
            },
        ),
        at(
            2,
            EventData::NoteWithId {
                id: NoteId(i32::MAX as u32),
                phase: NotePhase::On,
                channel: 15,
                key: 127,
                velocity: 0.,
            },
        ),
        at(
            9,
            EventData::NoteOff {
                channel: 3,
                key: 60,
                velocity: 0.25,
            },
        ),
        at(
            9,
            EventData::NoteWithId {
                id: NoteId(0),
                phase: NotePhase::Off,
                channel: 0,
                key: 0,
                velocity: 1.,
            },
        ),
    ];
    assert_eq!(t.translate(&events, block, 16), Ok(0));
    let r = records(&t);
    let shape = |r: &RawEvent| {
        (
            r.kind,
            r.bus,
            r.offset,
            r.channel,
            r.pitch,
            r.note_id,
            r.velocity,
            r.tuning,
            r.note_length,
            r.flags,
            r.ppq_position,
        )
    };
    assert_eq!(
        r.iter().map(shape).collect::<Vec<_>>(),
        vec![
            (0, 0, 1, 3, 60, -1, 0.75, 0., 0, 0, 0.),
            (0, 0, 2, 15, 127, i32::MAX, 0., 0., 0, 0, 0.),
            (1, 0, 9, 3, 60, -1, 0.25, 0., 0, 0, 0.),
            (1, 0, 9, 0, 0, 0, 1., 0., 0, 0, 0.),
        ]
    );
    // IDs beyond the SDK's nonnegative int32 range refuse.
    let wide = at(
        0,
        EventData::NoteWithId {
            id: NoteId(i32::MAX as u32 + 1),
            phase: NotePhase::On,
            channel: 0,
            key: 0,
            velocity: 1.,
        },
    );
    assert_eq!(t.translate(&[wide], block, 16), Err(invalid(0)));
    // RequireIdentified refuses plain notes but admits NoteWithId.
    let mut identified = translation();
    identified.notes = Some(NoteRoute {
        bus: 1,
        ids: NoteIdPolicy::RequireIdentified,
        expressions: false,
        poly_pressure: false,
    });
    let mut t = prepare(identified).unwrap();
    assert_eq!(t.translate(&events[..1], block, 16), Err(invalid(0)));
    let id_note = |channel| {
        at(
            0,
            EventData::NoteWithId {
                id: NoteId(7),
                phase: NotePhase::On,
                channel,
                key: 64,
                velocity: 0.5,
            },
        )
    };
    assert_eq!(t.translate(&[id_note(1)], block, 16), Ok(0));
    assert_eq!((records(&t)[0].bus, records(&t)[0].note_id), (1, 7));
    // Bus 1 has two channels: channel 2 is outside it.
    assert_eq!(t.translate(&[id_note(2)], block, 16), Err(invalid(0)));
    // Invalid velocity.
    let loud = at(
        0,
        EventData::NoteOn {
            channel: 0,
            key: 1,
            velocity: 1.5,
        },
    );
    let mut t = prepare(translation()).unwrap();
    assert_eq!(t.translate(&[loud], block, 16), Err(invalid(0)));
}

#[test]
fn expressions_and_poly_pressure_use_sdk_units() {
    let mut t = prepare(translation()).unwrap();
    let block = Transport::default();
    let id = Some(401);
    let cases = [
        (NoteExpression::Volume, 2., 0, 0.5),
        (NoteExpression::Volume, 4., 0, 1.),
        (NoteExpression::Pan, 0.25, 1, 0.25),
        (NoteExpression::Tuning, -120., 2, 0.),
        (NoteExpression::Tuning, 12., 2, 0.55),
        (NoteExpression::Vibrato, 0.125, 3, 0.125),
        (NoteExpression::Expression, 0.5, 4, 0.5),
        (NoteExpression::Brightness, 1., 5, 1.),
    ];
    let events: Vec<_> = cases
        .iter()
        .map(|&(expression, value, _, _)| {
            at(
                5,
                EventData::NoteExpression {
                    target: target(None, None, id),
                    expression,
                    value,
                },
            )
        })
        .collect();
    assert_eq!(t.translate(&events, block, 8), Ok(0));
    let got: Vec<_> = records(&t)
        .iter()
        .map(|r| (r.kind, r.offset, r.expression_id, r.note_id, r.value))
        .collect();
    let want: Vec<_> = cases
        .iter()
        .map(|&(_, _, type_id, normalized)| (4, 5, type_id, 401, normalized))
        .collect();
    assert_eq!(got, want);
    // Pressure becomes SDK poly pressure with concrete channel/key, optional ID.
    let pressure = |t: NoteTarget| {
        at(
            6,
            EventData::NoteExpression {
                target: t,
                expression: NoteExpression::Pressure,
                value: 0.375,
            },
        )
    };
    assert_eq!(
        t.translate(
            &[
                pressure(target(Some(2), Some(61), Some(401))),
                pressure(target(Some(2), Some(61), None))
            ],
            block,
            8
        ),
        Ok(0)
    );
    let got: Vec<_> = records(&t)
        .iter()
        .map(|r| (r.kind, r.offset, r.channel, r.pitch, r.note_id, r.velocity))
        .collect();
    assert_eq!(
        got,
        vec![(3, 6, 2, 61, 401, 0.375), (3, 6, 2, 61, -1, 0.375)]
    );
    // Refusals: expression targets need an ID and no channel/key; pressure needs
    // channel and key; out-of-range standard values refuse.
    for bad in [
        at(
            0,
            EventData::NoteExpression {
                target: target(Some(0), None, id),
                expression: NoteExpression::Pan,
                value: 0.5,
            },
        ),
        at(
            0,
            EventData::NoteExpression {
                target: target(None, None, None),
                expression: NoteExpression::Pan,
                value: 0.5,
            },
        ),
        at(
            0,
            EventData::NoteExpression {
                target: target(None, None, id),
                expression: NoteExpression::Volume,
                value: 4.5,
            },
        ),
        pressure(target(Some(0), None, id)),
        pressure(target(Some(16), Some(1), None)),
    ] {
        assert_eq!(t.translate(&[bad], block, 8), Err(invalid(0)));
    }
}

#[test]
fn unsupported_kinds_follow_their_policy() {
    let unsupported = [
        EventData::Midi(MidiMessage::from_bytes(&[0x90, 60, 100]).unwrap()),
        EventData::ParameterModulation {
            id: ParameterId(10),
            amount: 0.1,
        },
        EventData::ParameterGesture {
            id: ParameterId(10),
            phase: ParameterGesturePhase::Begin,
        },
        EventData::NoteChoke {
            target: NoteTarget::default(),
        },
        EventData::NoteEnd {
            target: NoteTarget::default(),
        },
    ];
    let block = Transport::default();
    let mut t = prepare(translation()).unwrap();
    for data in unsupported {
        assert_eq!(
            t.translate(&[at(0, data)], block, 4),
            Err(Refusal::Unsupported { index: 0 })
        );
    }
    // Disabled expression/pressure capabilities and missing note routes are
    // unsupported, not malformed: with Ignore they are skipped and counted.
    let mut quiet = translation();
    quiet.notes = Some(NoteRoute {
        bus: 0,
        ids: NoteIdPolicy::Unidentified,
        expressions: false,
        poly_pressure: false,
    });
    quiet.unsupported = EventPolicy::Ignore;
    let mut t = prepare(quiet.clone()).unwrap();
    let mut events: Vec<_> = unsupported.iter().map(|&d| at(0, d)).collect();
    events.push(at(
        0,
        EventData::NoteExpression {
            target: target(None, None, Some(1)),
            expression: NoteExpression::Pan,
            value: 0.5,
        },
    ));
    events.push(at(
        0,
        EventData::NoteExpression {
            target: target(Some(0), Some(1), None),
            expression: NoteExpression::Pressure,
            value: 0.5,
        },
    ));
    events.push(at(0, value(10, 0.5)));
    assert_eq!(t.translate(&events, block, 4), Ok(7));
    assert_eq!(points(&t), vec![(1, 0, 0.5)]);
    quiet.notes = None;
    let mut t = prepare(quiet).unwrap();
    let on = at(
        0,
        EventData::NoteOn {
            channel: 0,
            key: 1,
            velocity: 1.,
        },
    );
    assert_eq!(t.translate(&[on], block, 4), Ok(1));
    assert!(t.events.is_empty());
}

#[test]
fn transport_maps_to_context_with_exact_flags() {
    assert_eq!(transport_context(Transport::default(), RATE), Ok(None));
    let invalid = Err(ProcessError::InvalidTransport);
    // Playing and sample position must both be known once anything is.
    let tempo_only = Transport {
        tempo: Some(120.),
        ..Transport::default()
    };
    assert_eq!(transport_context(tempo_only, RATE), invalid);
    let no_samples = Transport {
        playing: Some(true),
        ..Transport::default()
    };
    assert_eq!(transport_context(no_samples, RATE), invalid);
    let minimal = Transport {
        playing: Some(false),
        sample_position: Some(-9007199254740993),
        ..Transport::default()
    };
    let c = transport_context(minimal, RATE).unwrap().unwrap();
    assert_eq!(c, Context::new(RATE, -9007199254740993));
    assert_eq!(c.state, 0);
    let full = Transport {
        playing: Some(true),
        sample_position: Some(9007199254740993),
        beat_position: Some(-0.125),
        tempo: Some(123.5),
        discontinuity: true,
        details: Some(TransportDetails {
            recording: true,
            loop_active: true,
            within_pre_roll: true,
            time_signature: Some((7, 8)),
            tempo_increment: Some(0.001),
            beat_loop: Some((4., 8.)),
            sample_loop: Some((96000, 192000)),
            bar: Some((-4., -1)),
        }),
    };
    let c = transport_context(full, RATE).unwrap().unwrap();
    let mut expected = Context::new(RATE, 9007199254740993);
    expected.state = Context::PLAYING
        | Context::RECORDING
        | Context::CYCLE_ACTIVE
        | Context::TEMPO
        | Context::MUSIC
        | Context::TIME_SIGNATURE
        | Context::BAR
        | Context::CYCLE;
    expected.tempo = 123.5;
    expected.project_time_music = -0.125;
    expected.time_signature_numerator = 7;
    expected.time_signature_denominator = 8;
    expected.bar_position_music = -4.;
    expected.cycle_start_music = 4.;
    expected.cycle_end_music = 8.;
    assert_eq!(c, expected);
    // Active loop without bounds keeps kCycleActive only.
    let active = Transport {
        details: Some(TransportDetails {
            loop_active: true,
            ..TransportDetails::default()
        }),
        ..minimal
    };
    assert_eq!(
        transport_context(active, RATE).unwrap().unwrap().state,
        Context::CYCLE_ACTIVE
    );
    // Invalid values refuse.
    let with = |details: TransportDetails| Transport {
        details: Some(details),
        ..minimal
    };
    for bad in [
        Transport {
            tempo: Some(0.),
            ..minimal
        },
        Transport {
            beat_position: Some(f64::INFINITY),
            ..minimal
        },
        with(TransportDetails {
            time_signature: Some((0, 4)),
            ..TransportDetails::default()
        }),
        with(TransportDetails {
            tempo_increment: Some(0.1),
            ..TransportDetails::default()
        }),
        with(TransportDetails {
            bar: Some((0., 0)),
            ..TransportDetails::default()
        }),
        with(TransportDetails {
            loop_active: true,
            beat_loop: Some((1., 2.)),
            ..TransportDetails::default()
        }),
        with(TransportDetails {
            sample_loop: Some((0, 10)),
            ..TransportDetails::default()
        }),
    ] {
        assert_eq!(transport_context(bad, RATE), invalid);
    }
    assert_eq!(transport_context(minimal, f64::NAN), invalid);
    // The translator applies transport only when declared.
    let mut t = prepare(translation()).unwrap();
    assert_eq!(t.translate(&[], full, 4), Ok(0));
    assert!(t.context.is_none());
    let mut declared = translation();
    declared.transport = true;
    let mut t = prepare(declared).unwrap();
    assert_eq!(t.translate(&[], full, 4), Ok(0));
    assert_eq!(t.context, Some(expected));
    assert_eq!(t.translate(&[], tempo_only, 4), Err(Refusal::Transport));
}

#[test]
fn declarations_are_checked_against_catalogue_and_buses() {
    let with_routes = |routes: Vec<ParameterRoute>| {
        let mut t = FrameworkTranslation::new(1, 1);
        t.parameters = routes;
        t
    };
    let route = |g, n| ParameterRoute::new(ParameterId(g), n, ParameterScale::Normalized);
    assert_eq!(
        prepare(with_routes(vec![route(1, 1), route(2, 3)])).err(),
        Some(TranslationError::UnknownParameter { index: 1 })
    );
    for native in [7, 9] {
        assert_eq!(
            prepare(with_routes(vec![route(1, native)])).err(),
            Some(TranslationError::ReadOnlyParameter { index: 0 })
        );
    }
    let scale = |minimum, maximum| {
        ParameterRoute::new(
            ParameterId(1),
            1,
            ParameterScale::Linear { minimum, maximum },
        )
    };
    for (shape, expected) in [
        (
            vec![route(1, 1), route(1, 5)],
            TranslationError::DuplicateGraphId { index: 1 },
        ),
        (
            vec![route(1, 1), route(2, 1)],
            TranslationError::DuplicateNativeId { index: 1 },
        ),
        (
            vec![route(1, 1), route(2, u32::MAX)],
            TranslationError::NativeId { index: 1 },
        ),
        (vec![scale(1., 1.)], TranslationError::Scale { index: 0 }),
        (
            vec![scale(f64::MIN, f64::MAX)],
            TranslationError::Scale { index: 0 },
        ),
    ] {
        assert_eq!(prepare(with_routes(shape)).err(), Some(expected));
    }
    assert_eq!(
        prepare(FrameworkTranslation::new(2049, 0)).err(),
        Some(TranslationError::Capacity)
    );
    assert_eq!(
        prepare(FrameworkTranslation::new(0, 2049)).err(),
        Some(TranslationError::Capacity)
    );
    let mut unselected = FrameworkTranslation::new(1, 0);
    unselected.notes = Some(NoteRoute {
        bus: 2,
        ids: NoteIdPolicy::Unidentified,
        expressions: false,
        poly_pressure: false,
    });
    assert_eq!(
        prepare(unselected.clone()).err(),
        Some(TranslationError::EventBus)
    );
    unselected.notes.as_mut().unwrap().bus = 16;
    assert_eq!(prepare(unselected).err(), Some(TranslationError::NoteBus));
    // Merged storage must fit the protocol limit; Refuse needs none.
    let lanes = LaneOptions::maximum(1);
    let mut merging = FrameworkTranslation::new(1, 0);
    assert!(Translator::prepare(merging.clone(), &catalogue(), &BUSES, Some(lanes), RATE).is_ok());
    merging.merge = MergePolicy::LaneFirst;
    assert_eq!(
        Translator::prepare(merging, &catalogue(), &BUSES, Some(lanes), RATE).err(),
        Some(TranslationError::Capacity)
    );
}

fn lane_storage() -> (InputEvents, InputPoints) {
    (
        InputEvents::new(8, 64).unwrap(),
        InputPoints::new(8).unwrap(),
    )
}
fn merging(policy: MergePolicy) -> Translator {
    let mut t = translation();
    t.merge = policy;
    t.transport = true;
    let lanes = LaneOptions {
        batches: 1,
        input_events: 8,
        input_payload: 64,
        input_points: 8,
        output_events: 0,
        output_payload: 0,
        output_queues: 0,
        output_points: 0,
    };
    Translator::prepare(t, &catalogue(), &BUSES, Some(lanes), RATE)
        .unwrap()
        .0
}

#[test]
fn mixed_blocks_follow_the_declared_merge_order() {
    let (mut lane_events, mut lane_points) = lane_storage();
    let header = |offset| Header {
        bus: 0,
        offset,
        ppq_position: 1.5,
        flags: 1,
    };
    lane_events
        .push(Event {
            header: header(2),
            kind: Kind::NoteOn(note(77, 1, 50, 0.5)),
        })
        .unwrap();
    lane_events
        .push(Event {
            header: header(4),
            kind: Kind::ExpressionText {
                expression_id: 9,
                note_id: 77,
                text: "é𝄞",
            },
        })
        .unwrap();
    lane_points
        .push(Point {
            id: 5,
            offset: 2,
            value: 0.125,
        })
        .unwrap();
    let framework = [
        at(
            2,
            EventData::NoteOn {
                channel: 0,
                key: 60,
                velocity: 1.,
            },
        ),
        at(2, value(10, 0.5)),
        at(5, value(10, 1.)),
    ];
    let block = Transport::default();
    for (policy, first) in [
        (MergePolicy::FrameworkFirst, -1),
        (MergePolicy::LaneFirst, 77),
    ] {
        let mut t = merging(policy);
        assert_eq!(t.translate(&framework, block, 8), Ok(0));
        let (source, context) = t.resolve(&lane_events, &lane_points, None).unwrap();
        assert_eq!((source, context), (Source::Merged, None));
        let (events, points) = t.merged_input();
        let order: Vec<_> = events.records[..events.count]
            .iter()
            .map(|r| (r.offset, r.note_id))
            .collect();
        let other = if first == -1 { 77 } else { -1 };
        assert_eq!(order, vec![(2, first), (2, other), (4, 77)]);
        // Lane text payload survives the copy with its alignment.
        let text = events.records[..events.count]
            .iter()
            .find(|r| r.kind == 5)
            .unwrap();
        let words: Vec<u16> = events.bytes()
            [text.payload_offset as usize..(text.payload_offset + text.payload_bytes) as usize]
            .as_chunks::<2>()
            .0
            .iter()
            .map(|b| u16::from_ne_bytes(*b))
            .collect();
        assert_eq!(String::from_utf16(&words).unwrap(), "é𝄞");
        assert_eq!(text.payload_offset % 2, 0);
        assert!(events.admitted(8, &BUSES));
        let ids: Vec<_> = points.iter().map(|p| (p.offset, p.id)).collect();
        let expected = if first == -1 {
            vec![(2, 1), (2, 5), (5, 1)]
        } else {
            vec![(2, 5), (2, 1), (5, 1)]
        };
        assert_eq!(ids, expected);
    }
    // Single-source blocks need no merging; a lone context is used as given.
    let mut t = merging(MergePolicy::Refuse);
    let transport = Transport {
        playing: Some(true),
        sample_position: Some(64),
        ..Transport::default()
    };
    assert_eq!(t.translate(&[], transport, 8), Ok(0));
    let (source, context) = t.resolve(&lane_events, &lane_points, None).unwrap();
    assert_eq!(source, Source::Lane);
    assert_eq!(context.unwrap().project_time_samples, 64);
    assert_eq!(t.translate(&framework, block, 8), Ok(0));
    let empty = lane_storage();
    let lane_context = Context::new(RATE, 5);
    assert_eq!(
        t.resolve(&empty.0, &empty.1, Some(&lane_context)),
        Ok((Source::Framework, Some(lane_context)))
    );
    // Refuse policy, two contexts and cross-source conflicts refuse, each
    // with its own reason.
    assert_eq!(
        t.resolve(&lane_events, &lane_points, None),
        Err(Refusal::Mixed)
    );
    assert_eq!(t.translate(&[], transport, 8), Ok(0));
    assert_eq!(
        t.resolve(&empty.0, &empty.1, Some(&lane_context)),
        Err(Refusal::TwoContexts)
    );
    let mut t = merging(MergePolicy::FrameworkFirst);
    let mut clash = InputPoints::new(1).unwrap();
    clash
        .push(Point {
            id: 1,
            offset: 2,
            value: 0.,
        })
        .unwrap();
    assert_eq!(t.translate(&framework, block, 8), Ok(0));
    assert_eq!(
        t.resolve(&empty.0, &clash, None),
        Err(Refusal::MergeConflict)
    );
}

#[test]
fn translation_and_merging_allocate_nothing() {
    let mut t = merging(MergePolicy::LaneFirst);
    let (mut lane_events, mut lane_points) = lane_storage();
    lane_events
        .push(Event {
            header: Header {
                bus: 0,
                offset: 1,
                ppq_position: 0.,
                flags: 0,
            },
            kind: Kind::NoteOff(note(3, 0, 10, 0.)),
        })
        .unwrap();
    lane_points
        .push(Point {
            id: 5,
            offset: 0,
            value: 1.,
        })
        .unwrap();
    let events = [
        at(0, value(10, 0.25)),
        at(
            1,
            EventData::NoteWithId {
                id: NoteId(3),
                phase: NotePhase::On,
                channel: 0,
                key: 10,
                velocity: 0.5,
            },
        ),
        at(
            2,
            EventData::NoteExpression {
                target: target(None, None, Some(3)),
                expression: NoteExpression::Tuning,
                value: 1.,
            },
        ),
        at(3, value(20, 0.)),
    ];
    let transport = Transport {
        playing: Some(true),
        sample_position: Some(0),
        tempo: Some(120.),
        ..Transport::default()
    };
    let measured = allocation_counter::measure(|| {
        for i in 0..10000 {
            assert_eq!(t.translate(&events, transport, 4 + i % 60), Ok(0));
            let (source, context) = t.resolve(&lane_events, &lane_points, None).unwrap();
            assert_eq!(source, Source::Merged);
            assert!(context.is_some());
        }
    });
    assert_eq!(measured.count_total, 0);
}

#[test]
fn same_offset_last_wins_coalesces_and_counts() {
    let mut last = translation();
    last.same_offset = SameOffsetPolicy::LastWins;
    last.points = 3;
    let mut t = prepare(last).unwrap();
    let block = Transport::default();
    // Native 1 at offset 2 is replaced (0.25 -> 0.75) without consuming
    // capacity; native 5 (linear -12..12, 6.0 -> 0.75) sits between them.
    let events = [
        at(2, value(10, 0.25)),
        at(2, value(20, 6.)),
        at(2, value(10, 0.75)),
        at(3, value(10, 0.5)),
        at(3, value(10, 0.125)),
    ];
    assert_eq!(t.translate(&events, block, 8), Ok(2));
    assert_eq!(points(&t), vec![(1, 2, 0.75), (5, 2, 0.75), (1, 3, 0.125)]);
    // A replacing value is still validated first.
    assert_eq!(
        t.translate(&[at(2, value(10, 0.25)), at(2, value(10, 1.5))], block, 8),
        Err(invalid(1))
    );
    // Values at different offsets are never coalesced.
    assert_eq!(
        t.translate(&[at(1, value(10, 0.25)), at(2, value(10, 0.25))], block, 8),
        Ok(0)
    );
    assert_eq!(points(&t), vec![(1, 1, 0.25), (1, 2, 0.25)]);
}

#[test]
fn refusals_are_recorded_in_a_bounded_ring() {
    let mut roomy = translation();
    roomy.points = 32;
    let (mut t, mut ring) = Translator::prepare(roomy, &catalogue(), &BUSES, None, RATE).unwrap();
    let block = Transport::default();
    assert_eq!(
        t.translate(&[at(0, value(99, 0.5))], block, 8),
        Err(Refusal::UnmappedParameter { index: 0 })
    );
    assert_eq!(ring.pop(), Ok(Refusal::UnmappedParameter { index: 0 }));
    assert!(ring.pop().is_err());
    // Successful blocks record nothing.
    assert_eq!(t.translate(&[at(0, value(10, 0.5))], block, 8), Ok(0));
    assert!(ring.pop().is_err());
    // Beyond REFUSAL_RECORDS unread records the details are dropped; the
    // oldest are kept in order and pushing never blocks.
    for i in 0..REFUSAL_RECORDS + 4 {
        let events: Vec<_> = (0..=i)
            .map(|k| at(k, value(if k == i { 99 } else { 10 }, 0.5)))
            .collect();
        assert_eq!(
            t.translate(&events, block, 64),
            Err(Refusal::UnmappedParameter { index: i })
        );
    }
    let drained: Vec<_> = std::iter::from_fn(|| ring.pop().ok()).collect();
    assert_eq!(
        drained,
        (0..REFUSAL_RECORDS)
            .map(|index| Refusal::UnmappedParameter { index })
            .collect::<Vec<_>>()
    );
}

#[test]
fn merged_points_keep_the_native_distinct_id_limit() {
    let mut wide = FrameworkTranslation::new(0, 60);
    wide.parameters = (0..40)
        .map(|i| ParameterRoute::new(ParameterId(i), 100 + i, ParameterScale::Normalized))
        .collect();
    wide.merge = MergePolicy::FrameworkFirst;
    let lanes = LaneOptions {
        batches: 1,
        input_events: 0,
        input_payload: 0,
        input_points: 60,
        output_events: 0,
        output_payload: 0,
        output_queues: 0,
        output_points: 0,
    };
    let (mut t, mut ring) =
        Translator::prepare(wide, &catalogue(), &BUSES, Some(lanes), RATE).unwrap();
    let events: Vec<_> = (0..40).map(|i| at(0, value(i, 0.5))).collect();
    assert_eq!(t.translate(&events, Transport::default(), 8), Ok(0));
    let no_events = InputEvents::new(0, 0).unwrap();
    // 40 framework IDs plus 24 other lane IDs fit exactly.
    let mut lane = InputPoints::new(60).unwrap();
    for id in 140..164 {
        lane.push(Point {
            id,
            offset: 1,
            value: 0.25,
        })
        .unwrap();
    }
    assert_eq!(
        t.resolve(&no_events, &lane, None).map(|(s, _)| s),
        Ok(Source::Merged)
    );
    assert_eq!(t.merged_input().1.len(), 64);
    // One more distinct lane ID is the merged distinct-ID limit, not capacity.
    lane.push(Point {
        id: 164,
        offset: 1,
        value: 0.25,
    })
    .unwrap();
    assert_eq!(
        t.resolve(&no_events, &lane, None),
        Err(Refusal::MergedParameterIds)
    );
    assert_eq!(ring.pop(), Ok(Refusal::MergedParameterIds));
}

#[test]
fn pressure_has_no_sdk_note_expression() {
    assert_eq!(expression_value(NoteExpression::Pressure, 0.5), None);
    assert_eq!(expression_value(NoteExpression::Volume, 2.), Some((0, 0.5)));
}
