//! Framework translation through owned bank nodes and the mock provider.
//! Called from the single main-thread scenario test in `tests.rs`, because
//! the registry binds to its first accessing thread under unit tests.
use super::super::mock::{self, Capture, Probe};
use super::framework::{
    EventPolicy, FrameworkTranslation, MergePolicy, NoteIdPolicy, NoteRoute, ParameterRoute,
    ParameterScale, Refusal, TranslationError,
};
use super::*;
use crate::vst3::instances::parameters::Point;
use processing::{Event as NativeEvent, Header, Kind, Note};
use ravet_foundation::{
    AudioBuffer, Event, EventData, NoteExpression, NoteId, NotePhase, NoteTarget, ParameterId,
    Transport,
};
use ravet_graph::{Connection, Destination, Graph, GraphNode, GraphStatus, NodeId};
use std::sync::atomic::Ordering;

const STEREO: u64 = 0b11;
const MODES: u32 = mock::PARAMETERS | mock::EVENT_BUSES;
const GAIN: ParameterId = ParameterId(101);
const LEVEL: ParameterId = ParameterId(202);

fn instrument(lanes: Option<LaneOptions>) -> Vst3GraphNodeOptions {
    Vst3GraphNodeOptions {
        class_index: 1,
        inputs: vec![],
        outputs: vec![BusRequest::active(STEREO)],
        event_inputs: 0b01,
        event_outputs: 0,
        lanes,
    }
}
fn builder<S: HostedSample>(probe: &'static Probe, frames: u32) -> Vst3GraphBankBuilder<S> {
    let tables = mock::tables();
    let library = mock::library(&tables, probe);
    // SAFETY: leaked exact-paired trusted mock tables.
    let (parameters, processing, hosted) = unsafe {
        (
            parameters::Provider::from_raw(tables.parameters).unwrap(),
            processing::Provider::from_raw(tables.processing).unwrap(),
            HostedProvider::from_raw(tables.hosted).unwrap(),
        )
    };
    Vst3GraphBankBuilder::new(library, parameters, processing, hosted, 48000., frames).unwrap()
}
fn probe(mode: u32) -> &'static Probe {
    let probe = Probe::leak();
    probe.mode.store(mode, Ordering::SeqCst);
    probe
}
fn translation() -> FrameworkTranslation {
    let mut t = FrameworkTranslation::new(16, 16);
    t.parameters = vec![
        ParameterRoute::new(
            GAIN,
            1,
            ParameterScale::Linear {
                minimum: 0.,
                maximum: 4.,
            },
        ),
        ParameterRoute::new(LEVEL, 5, ParameterScale::Normalized),
    ];
    t.notes = Some(NoteRoute {
        bus: 0,
        ids: NoteIdPolicy::Unidentified,
        expressions: true,
        poly_pressure: true,
    });
    t.transport = true;
    t
}
fn at(frame_offset: usize, data: EventData) -> Event {
    Event { frame_offset, data }
}
fn value(id: ParameterId, value: f32) -> EventData {
    EventData::ParameterValue { id, value }
}
fn transport(samples: i64) -> Transport {
    Transport {
        playing: Some(true),
        sample_position: Some(samples),
        tempo: Some(96.),
        beat_position: Some(2.5),
        ..Transport::default()
    }
}
fn run<S: HostedSample + Sample>(
    node: &mut Vst3GraphNode<S>,
    events: &[Event],
    transport: Transport,
    frames: usize,
    out: &mut [[S; 64]; 2],
) -> ProcessStatus {
    let [l, r] = out;
    let mut outs = [&mut l[..], &mut r[..]];
    let audio = AudioBuffer::separate(&[], &mut outs, frames).unwrap();
    node.process(&mut ProcessBlock::new(audio, events, transport))
}
fn last(probe: &Probe) -> Capture {
    probe.captures().pop().expect("captured native input")
}
fn refusals(control: &Vst3GraphBankControl, child: usize) -> Vec<Refusal> {
    let mut out = Vec::new();
    let drained = control.drain_refusals(child, |r| out.push(r)).unwrap();
    assert_eq!(drained, out.len());
    out
}

pub(super) fn scenarios() {
    audio_only_translation_reaches_native_queues_at_exact_offsets();
    refusals_silence_without_faulting_or_calling_native();
    declarations_refuse_before_any_lease();
    mixed_blocks_merge_or_refuse_with_aligned_lanes();
    graph_broadcast_events_reach_translating_nodes();
    graph_refusal_silences_one_block_and_continues();
    translated_and_merged_blocks_allocate_nothing();
}

fn audio_only_translation_reaches_native_queues_at_exact_offsets() {
    let probe = probe(MODES | mock::CAPTURE);
    let mut builder = builder::<f64>(probe, 64);
    let child = builder.node(instrument(None)).unwrap();
    builder.translate(child, translation()).unwrap();
    let (control, mut nodes) = builder.build().unwrap();
    let mut node = nodes.pop().unwrap();
    node.prepare(ProcessConfig::new(48000., 64, 0, node.layout()).unwrap())
        .unwrap();
    let events = [
        at(0, value(GAIN, 1.)),
        at(
            5,
            EventData::NoteWithId {
                id: NoteId(401),
                phase: NotePhase::On,
                channel: 2,
                key: 69,
                velocity: 0.5,
            },
        ),
        at(
            17,
            EventData::NoteExpression {
                target: NoteTarget {
                    channel: None,
                    key: None,
                    id: Some(NoteId(401)),
                },
                expression: NoteExpression::Volume,
                value: 2.,
            },
        ),
        at(17, value(LEVEL, 0.75)),
        at(
            40,
            EventData::NoteExpression {
                target: NoteTarget {
                    channel: Some(2),
                    key: Some(69),
                    id: Some(NoteId(401)),
                },
                expression: NoteExpression::Pressure,
                value: 0.5,
            },
        ),
        at(
            63,
            EventData::NoteOff {
                channel: 2,
                key: 69,
                velocity: 0.25,
            },
        ),
        at(63, value(GAIN, 3.)),
    ];
    let mut out = [[9f64; 64]; 2];
    assert_eq!(
        run(
            &mut node,
            &events,
            transport(9007199254740993),
            64,
            &mut out
        ),
        ProcessStatus::Continue
    );
    // Independent instrument oracle: channel c outputs f32 0.1 * (c+1).
    let tenth = f64::from(0.1f32);
    assert_eq!(out, [[tenth; 64], [tenth * 2.; 64]]);
    let c = last(probe);
    assert_eq!(c.frames, 64);
    let points: Vec<_> = c.points.iter().map(|p| (p.id, p.offset, p.value)).collect();
    assert_eq!(points, vec![(1, 0, 0.25), (5, 17, 0.75), (1, 63, 0.75)]);
    let shape: Vec<_> = c
        .events
        .iter()
        .map(|r| {
            (
                r.kind,
                r.bus,
                r.offset,
                r.channel,
                r.pitch,
                r.note_id,
                r.expression_id,
                r.value,
                r.velocity,
            )
        })
        .collect();
    assert_eq!(
        shape,
        vec![
            (0, 0, 5, 2, 69, 401, 0, 0., 0.5),
            (4, 0, 17, 0, 0, 401, 0, 0.5, 0.),
            (3, 0, 40, 2, 69, 401, 0, 0., 0.5),
            (1, 0, 63, 2, 69, -1, 0, 0., 0.25),
        ]
    );
    assert!(c.payload.is_empty());
    let context = c.context.unwrap();
    assert_eq!(context.project_time_samples, 9007199254740993);
    assert_eq!(
        context.state,
        processing::Context::PLAYING | processing::Context::TEMPO | processing::Context::MUSIC
    );
    assert_eq!((context.tempo, context.project_time_music), (96., 2.5));
    // A block without events and with absent transport passes no context.
    assert_eq!(
        run(&mut node, &[], Transport::default(), 3, &mut out),
        ProcessStatus::Continue
    );
    let c = last(probe);
    assert_eq!((c.frames, c.events.len(), c.points.len()), (3, 0, 0));
    assert_eq!(c.context, None);
    // Zero-frame block: offset zero is admitted and delivered.
    assert_eq!(
        run(
            &mut node,
            &[at(0, value(LEVEL, 0.5))],
            transport(64),
            0,
            &mut out
        ),
        ProcessStatus::Continue
    );
    assert_eq!(
        last(probe).points,
        vec![Point {
            id: 5,
            offset: 0,
            value: 0.5
        }]
    );
    assert_eq!(control.diagnostic(0).unwrap(), 0);
    drop(node);
    assert_eq!(control.retire().unwrap().released, 1);
}

fn refusals_silence_without_faulting_or_calling_native() {
    let probe = probe(MODES);
    let mut builder = builder::<f32>(probe, 16);
    let refusing = builder.node(instrument(None)).unwrap();
    let ignoring = builder.node(instrument(None)).unwrap();
    builder.translate(refusing, translation()).unwrap();
    let mut quiet = translation();
    quiet.unmapped_parameters = EventPolicy::Ignore;
    quiet.unsupported = EventPolicy::Ignore;
    quiet.notes = None;
    builder.translate(ignoring, quiet).unwrap();
    let (control, mut nodes) = builder.build().unwrap();
    let mut quiet = nodes.pop().unwrap();
    let mut node = nodes.pop().unwrap();
    for n in [&mut node, &mut quiet] {
        n.prepare(ProcessConfig::new(48000., 16, 0, n.layout()).unwrap())
            .unwrap();
    }
    let unmapped = [at(0, value(ParameterId(7), 0.5))];
    let cases: [(&[Event], Transport, Refusal); 4] = [
        (
            &unmapped,
            Transport::default(),
            Refusal::UnmappedParameter { index: 0 },
        ),
        (
            &[at(2, value(LEVEL, 0.1)), at(2, value(LEVEL, 0.2))],
            Transport::default(),
            Refusal::SameOffset { index: 1 },
        ),
        (
            &[at(16, value(LEVEL, 0.1))],
            Transport::default(),
            Refusal::Event(EventError::OffsetOutOfRange {
                index: 0,
                offset: 16,
                frames: 16,
            }),
        ),
        (
            &[],
            Transport {
                tempo: Some(120.),
                ..Transport::default()
            },
            Refusal::Transport,
        ),
    ];
    for (events, transport, _) in cases {
        let mut out = [[5f32; 64]; 2];
        // A refusal silences the block and reports Silence, never a fault.
        assert_eq!(
            run(&mut node, events, transport, 16, &mut out),
            ProcessStatus::Silence
        );
        assert_eq!(out[0][..16], [0.; 16]);
        assert_eq!(out[0][16..], [5.; 48]);
    }
    assert_eq!(probe.count(&probe.processes), 0, "plug-in never ran");
    assert_ne!(
        control.diagnostic(0).unwrap() & diagnostic::FRAMEWORK_REFUSED,
        0
    );
    assert_eq!(control.diagnostic(0).unwrap() & diagnostic::NATIVE_FAULT, 0);
    // Each refusal is counted and recorded with its exact reason, in order.
    assert_eq!(control.refused(0).unwrap(), 4);
    assert_eq!(
        refusals(&control, 0),
        cases.iter().map(|c| c.2).collect::<Vec<_>>()
    );
    assert!(refusals(&control, 0).is_empty());
    // Not a node fault: a valid block runs immediately afterwards.
    let mut out = [[5f32; 64]; 2];
    assert_eq!(
        run(
            &mut node,
            &[at(1, value(LEVEL, 1.))],
            transport(0),
            16,
            &mut out
        ),
        ProcessStatus::Continue
    );
    assert_eq!(out[1][..16], [0.2; 16]);
    assert_eq!(probe.count(&probe.processes), 1);
    // Capacity overflow refuses the whole block before native execution.
    let flood: Vec<_> = (0..17)
        .map(|i| {
            at(
                0,
                EventData::NoteOn {
                    channel: 0,
                    key: i,
                    velocity: 1.,
                },
            )
        })
        .collect();
    assert_eq!(
        run(&mut node, &flood, Transport::default(), 16, &mut out),
        ProcessStatus::Silence
    );
    assert_eq!(
        refusals(&control, 0),
        vec![Refusal::Events {
            count: 17,
            maximum: 16
        }]
    );
    assert_eq!(control.refused(0).unwrap(), 5);
    assert_eq!(probe.count(&probe.processes), 1);
    // Ignoring node: unmapped and unsupported events are skipped and counted.
    let mixed = [
        at(0, value(ParameterId(7), 0.5)),
        at(
            1,
            EventData::NoteOn {
                channel: 0,
                key: 60,
                velocity: 1.,
            },
        ),
        at(2, value(GAIN, 2.)),
    ];
    assert_eq!(
        run(&mut quiet, &mixed, Transport::default(), 16, &mut out),
        ProcessStatus::Continue
    );
    assert_eq!(control.ignored(1).unwrap(), 2);
    assert_eq!(control.ignored(0).unwrap(), 0);
    assert_eq!(control.diagnostic(1).unwrap(), 0);
    assert_eq!(control.refused(1).unwrap(), 0);
    assert!(refusals(&control, 1).is_empty());
    assert_eq!(control.drain_refusals(2, |_| ()), Err(BankError::Shape));
    drop((node, quiet));
    assert_eq!(control.retire().unwrap().released, 1);
}

fn declarations_refuse_before_any_lease() {
    // Unknown, read-only and hidden IDs and an unselected bus refuse in build,
    // after refresh and before any start/begin; everything unwinds.
    let cases = [
        (99, 0b01, TranslationError::UnknownParameter { index: 2 }),
        (7, 0b01, TranslationError::ReadOnlyParameter { index: 2 }),
        (9, 0b01, TranslationError::ReadOnlyParameter { index: 2 }),
        (5, 0b10, TranslationError::EventBus),
    ];
    for (native, mask, error) in cases {
        let probe = probe(MODES);
        let mut builder = builder::<f32>(probe, 8);
        builder.node(instrument(None)).unwrap();
        let mut options = instrument(None);
        options.event_inputs = mask;
        let child = builder.node(options).unwrap();
        let mut t = translation();
        t.parameters.push(ParameterRoute::new(
            ParameterId(3),
            native,
            ParameterScale::Normalized,
        ));
        if native == 5 {
            t.parameters.pop();
        }
        builder.translate(child, t).unwrap();
        assert_eq!(
            builder.build().err(),
            Some(BankError::Translation { child: 1, error })
        );
        assert_eq!(
            (
                probe.count(&probe.starts),
                probe.count(&probe.begins),
                probe.count(&probe.destroys),
                probe.count(&probe.closes)
            ),
            (0, 0, 2, 1)
        );
    }
    // Local shape refusals at declaration time.
    let probe = probe(MODES);
    let mut builder = builder::<f32>(probe, 8);
    let child = builder.node(instrument(None)).unwrap();
    assert_eq!(builder.translate(1, translation()), Err(BankError::Shape));
    let mut duplicate = translation();
    duplicate
        .parameters
        .push(ParameterRoute::new(GAIN, 9, ParameterScale::Normalized));
    assert_eq!(
        builder.translate(child, duplicate),
        Err(BankError::Translation {
            child,
            error: TranslationError::DuplicateGraphId { index: 2 }
        })
    );
    // Merged storage must fit the protocol limit alongside the lane batch.
    let lanes = builder
        .node(instrument(Some(LaneOptions::maximum(1))))
        .unwrap();
    let mut merging = translation();
    merging.merge = MergePolicy::FrameworkFirst;
    assert_eq!(
        builder.translate(lanes, merging),
        Err(BankError::Translation {
            child: lanes,
            error: TranslationError::Capacity
        })
    );
    drop(builder);
}

fn lane() -> LaneOptions {
    LaneOptions {
        batches: 2,
        input_events: 4,
        input_payload: 64,
        input_points: 4,
        output_events: 0,
        output_payload: 0,
        output_queues: 8,
        output_points: 16,
    }
}
fn lane_note(input: &mut LaneInput<'_>, offset: u32) -> Result<(), processing::Error> {
    input.events.push(NativeEvent {
        header: Header {
            bus: 0,
            offset,
            ppq_position: 0.,
            flags: 0,
        },
        kind: Kind::NoteOn(Note {
            channel: 1,
            pitch: 40,
            note_id: 77,
            tuning: 0.,
            velocity: 0.5,
            length: 0,
        }),
    })?;
    input.points.push(Point {
        id: 42,
        offset,
        value: 0.125,
    })
}
fn drain(control: &Vst3GraphBankControl, child: usize) -> Vec<(u64, bool)> {
    let mut sequences = Vec::new();
    while let Some(result) = control
        .receive(child, |b| (b.sequence, b.output.is_ok()))
        .unwrap()
    {
        sequences.push(result);
    }
    sequences
}

fn mixed_blocks_merge_or_refuse_with_aligned_lanes() {
    let probe = probe(MODES | mock::CAPTURE);
    let mut builder = builder::<f64>(probe, 16);
    let mut policies = Vec::new();
    for merge in [
        MergePolicy::FrameworkFirst,
        MergePolicy::LaneFirst,
        MergePolicy::Refuse,
    ] {
        let child = builder.node(instrument(Some(lane()))).unwrap();
        let mut t = translation();
        t.merge = merge;
        builder.translate(child, t).unwrap();
        policies.push(merge);
    }
    let (control, nodes) = builder.build().unwrap();
    let framework = [
        at(
            4,
            EventData::NoteOn {
                channel: 0,
                key: 60,
                velocity: 1.,
            },
        ),
        at(4, value(LEVEL, 0.5)),
    ];
    for (child, (mut node, merge)) in nodes.into_iter().zip(policies).enumerate() {
        node.prepare(ProcessConfig::new(48000., 16, 0, node.layout()).unwrap())
            .unwrap();
        let mut out = [[0f64; 64]; 2];
        let processes = probe.count(&probe.processes);
        control
            .submit(child, 8, |input| lane_note(input, 4))
            .unwrap();
        let status = run(&mut node, &framework, Transport::default(), 8, &mut out);
        if merge == MergePolicy::Refuse {
            let refused = Refusal::Mixed;
            assert_eq!(status, ProcessStatus::Silence);
            assert_eq!(refusals(&control, child), vec![refused]);
            assert_eq!(probe.count(&probe.processes), processes);
            assert_ne!(
                control.diagnostic(child).unwrap() & diagnostic::MIXED_REFUSED,
                0
            );
            // The batch returns as refused; the next sequence still runs.
            let returned = control
                .receive(child, |b| (b.sequence, b.output.err()))
                .unwrap();
            assert_eq!(returned, Some((0, Some(HostedError::Framework(refused)))));
        } else {
            assert_eq!(status, ProcessStatus::Continue);
            let c = last(probe);
            let ids: Vec<_> = c.events.iter().map(|r| r.note_id).collect();
            let points: Vec<_> = c.points.iter().map(|p| p.id).collect();
            if merge == MergePolicy::FrameworkFirst {
                assert_eq!((ids, points), (vec![-1, 77], vec![5, 42]));
            } else {
                assert_eq!((ids, points), (vec![77, -1], vec![42, 5]));
            }
            assert!(c.events.iter().all(|r| r.offset == 4));
            let echoed = control
                .receive(child, |b| b.output.unwrap().parameter_points().len())
                .unwrap();
            assert_eq!(echoed, Some(2));
        }
        // A framework-only block and a lane-only block need no merge.
        control.submit(child, 8, |_| Ok(())).unwrap();
        assert_eq!(
            run(&mut node, &framework, Transport::default(), 8, &mut out),
            ProcessStatus::Continue
        );
        assert_eq!(last(probe).events.len(), 1);
        assert_eq!(drain(&control, child), vec![(1, true)]);
        control
            .submit(child, 8, |input| lane_note(input, 2))
            .unwrap();
        assert_eq!(
            run(&mut node, &[], Transport::default(), 8, &mut out),
            ProcessStatus::Continue
        );
        assert_eq!(last(probe).events[0].note_id, 77);
        assert_eq!(drain(&control, child), vec![(2, true)]);
        // Two contexts refuse whatever the merge policy; the batch returns.
        control
            .submit(child, 8, |input| {
                *input.context = Some(processing::Context::new(48000., 1));
                Ok(())
            })
            .unwrap();
        assert_eq!(
            run(&mut node, &[], transport(0), 8, &mut out),
            ProcessStatus::Silence
        );
        // A translation refusal also consumes and returns its batch.
        control.submit(child, 8, |_| Ok(())).unwrap();
        assert_eq!(
            run(
                &mut node,
                &[at(0, value(ParameterId(7), 0.))],
                Transport::default(),
                8,
                &mut out
            ),
            ProcessStatus::Silence
        );
        assert_eq!(drain(&control, child), vec![(3, false), (4, false)]);
        assert_eq!(
            refusals(&control, child),
            vec![
                Refusal::TwoContexts,
                Refusal::UnmappedParameter { index: 0 }
            ]
        );
        let expected = if merge == MergePolicy::Refuse { 3 } else { 2 };
        assert_eq!(control.refused(child).unwrap(), expected);
        // Still aligned and healthy: the next exact sequence processes.
        assert_eq!(control.submit(child, 8, |_| Ok(())).unwrap(), 5);
        assert_eq!(
            run(&mut node, &[], Transport::default(), 8, &mut out),
            ProcessStatus::Continue
        );
        assert_eq!(control.receive(child, |b| b.sequence).unwrap(), Some(5));
        drop(node);
    }
    assert_eq!(control.retire().unwrap().released, 1);
}

fn graph_broadcast_events_reach_translating_nodes() {
    let probe = probe(MODES | mock::CAPTURE);
    let mut builder = builder::<f32>(probe, 32);
    let child = builder.node(instrument(None)).unwrap();
    let mut t = translation();
    t.unmapped_parameters = EventPolicy::Ignore;
    builder.translate(child, t).unwrap();
    let (control, mut nodes) = builder.build().unwrap();
    let node = nodes.pop().unwrap();
    let id = NodeId(3);
    let mut graph = Graph::new(
        AudioLayout::STEREO_INSTRUMENT,
        vec![GraphNode {
            id,
            layout: node.layout(),
            processor: Box::new(node),
        }],
        &[0, 1].map(|channel| Connection {
            source: ravet_graph::Source::Node { id, channel },
            destination: Destination::Output(channel),
            gain: 1f32,
        }),
    )
    .unwrap();
    graph
        .prepare(ProcessConfig::new(48000., 32, 8, AudioLayout::STEREO_INSTRUMENT).unwrap())
        .unwrap();
    let events = [
        at(3, value(LEVEL, 0.25)),
        at(3, value(ParameterId(999), 0.25)),
        at(
            31,
            EventData::NoteOn {
                channel: 0,
                key: 1,
                velocity: 1.,
            },
        ),
    ];
    let mut out = [[0f32; 32]; 2];
    let [l, r] = &mut out;
    let mut outs = [&mut l[..], &mut r[..]];
    let audio = AudioBuffer::separate(&[], &mut outs, 32).unwrap();
    let status = graph.process(&mut ProcessBlock::new(audio, &events, transport(480)));
    assert_eq!(status, GraphStatus::Continue);
    assert_eq!(out, [[0.1; 32], [0.2; 32]]);
    let c = last(probe);
    assert_eq!(
        c.points,
        vec![Point {
            id: 5,
            offset: 3,
            value: 0.25
        }]
    );
    assert_eq!(c.events.len(), 1);
    assert_eq!((c.events[0].offset, c.events[0].pitch), (31, 1));
    assert_eq!(c.context.unwrap().project_time_samples, 480);
    assert_eq!(control.ignored(0).unwrap(), 1);
    drop(graph);
    assert_eq!(control.retire().unwrap().released, 1);
}

/// A refusal inside a Graph silences only that block. The Graph reports
/// Continue, is not latched, and the following valid blocks run the plug-in.
fn graph_refusal_silences_one_block_and_continues() {
    let probe = probe(MODES | mock::CAPTURE);
    let mut builder = builder::<f32>(probe, 32);
    let child = builder.node(instrument(None)).unwrap();
    // Refusing policies throughout (the default).
    builder.translate(child, translation()).unwrap();
    let (control, mut nodes) = builder.build().unwrap();
    let node = nodes.pop().unwrap();
    let id = NodeId(4);
    let mut graph = Graph::new(
        AudioLayout::STEREO_INSTRUMENT,
        vec![GraphNode {
            id,
            layout: node.layout(),
            processor: Box::new(node),
        }],
        &[0, 1].map(|channel| Connection {
            source: ravet_graph::Source::Node { id, channel },
            destination: Destination::Output(channel),
            gain: 1f32,
        }),
    )
    .unwrap();
    graph
        .prepare(ProcessConfig::new(48000., 32, 8, AudioLayout::STEREO_INSTRUMENT).unwrap())
        .unwrap();
    let mut process = |events: &[Event], transport: Transport| {
        let mut out = [[7f32; 32]; 2];
        let [l, r] = &mut out;
        let mut outs = [&mut l[..], &mut r[..]];
        let audio = AudioBuffer::separate(&[], &mut outs, 32).unwrap();
        let status = graph.process(&mut ProcessBlock::new(audio, events, transport));
        (status, out)
    };
    let valid = [at(3, value(LEVEL, 0.25))];
    let unmapped = [at(5, value(ParameterId(999), 0.25))];
    let bad_transport = Transport {
        tempo: Some(120.),
        ..Transport::default()
    };
    // Block, expected refusal (None: valid), cumulative plug-in calls.
    let plan: [(&[Event], Transport, Option<Refusal>, usize); 7] = [
        (&valid, transport(0), None, 1),
        (
            &unmapped,
            transport(32),
            Some(Refusal::UnmappedParameter { index: 0 }),
            1,
        ),
        (&valid, transport(64), None, 2),
        (&valid, transport(96), None, 3),
        (&[], bad_transport, Some(Refusal::Transport), 3),
        (&valid, transport(128), None, 4),
        (&[], transport(160), None, 5),
    ];
    let mut expected_refusals = Vec::new();
    for (block, (events, transport, refusal, calls)) in plan.into_iter().enumerate() {
        let (status, out) = process(events, transport);
        assert_eq!(status, GraphStatus::Continue, "block {block}");
        if let Some(refusal) = refusal {
            // The refused node's outputs were cleared, so the Graph sums zeros.
            assert_eq!(out, [[0.; 32]; 2], "block {block}");
            expected_refusals.push(refusal);
        } else {
            assert_eq!(out, [[0.1; 32], [0.2; 32]], "block {block}");
            assert_eq!(
                last(probe).context.unwrap().project_time_samples,
                transport.sample_position.unwrap()
            );
        }
        assert_eq!(probe.count(&probe.processes), calls, "block {block}");
    }
    assert_eq!(control.refused(child).unwrap(), 2);
    assert_eq!(refusals(&control, child), expected_refusals);
    let bits = control.diagnostic(child).unwrap();
    assert_ne!(bits & diagnostic::FRAMEWORK_REFUSED, 0);
    assert_eq!(bits & diagnostic::NATIVE_FAULT, 0);
    drop(graph);
    assert_eq!(control.retire().unwrap().released, 1);
}

fn translated_and_merged_blocks_allocate_nothing() {
    let probe = probe(MODES);
    let mut builder = builder::<f64>(probe, 64);
    let plain = builder.node(instrument(None)).unwrap();
    let queued = builder.node(instrument(Some(lane()))).unwrap();
    let mut t = translation();
    t.unsupported = EventPolicy::Ignore;
    builder.translate(plain, t.clone()).unwrap();
    t.merge = MergePolicy::LaneFirst;
    builder.translate(queued, t).unwrap();
    let (control, mut nodes) = builder.build().unwrap();
    let mut queued_node = nodes.pop().unwrap();
    let mut plain_node = nodes.pop().unwrap();
    for n in [&mut plain_node, &mut queued_node] {
        n.prepare(ProcessConfig::new(48000., 64, 0, n.layout()).unwrap())
            .unwrap();
    }
    let mut out = [[0f64; 64]; 2];
    let measured = allocation_counter::measure(|| {
        for i in 0..10000usize {
            let frames = 1 + i % 64;
            let last = frames - 1;
            let events = [
                at(0, value(GAIN, (i % 5) as f32)),
                at(
                    0,
                    EventData::NoteWithId {
                        id: NoteId(i as u32),
                        phase: NotePhase::On,
                        channel: 0,
                        key: (i % 128) as u8,
                        velocity: 0.5,
                    },
                ),
                at(
                    last,
                    EventData::NoteExpression {
                        target: NoteTarget {
                            channel: None,
                            key: None,
                            id: Some(NoteId(i as u32)),
                        },
                        expression: NoteExpression::Tuning,
                        value: -1.,
                    },
                ),
                at(
                    last,
                    EventData::NoteChoke {
                        target: NoteTarget::default(),
                    },
                ),
                at(last, value(LEVEL, 0.5)),
            ];
            let block = transport(i as i64);
            assert_eq!(
                run(&mut plain_node, &events, block, frames, &mut out),
                ProcessStatus::Continue
            );
            control
                .submit(queued, frames as u32, |input| lane_note(input, 0))
                .unwrap();
            assert_eq!(
                run(&mut queued_node, &events, block, frames, &mut out),
                ProcessStatus::Continue
            );
            let echoed = control
                .receive(queued, |b| b.output.unwrap().parameter_points().len())
                .unwrap();
            // Lane point, gain, level, plus the mock's tempo echo.
            assert_eq!(echoed, Some(4));
        }
    });
    assert_eq!(measured.count_total, 0);
    assert_eq!(control.ignored(plain).unwrap(), 10000);
    assert_eq!(probe.count(&probe.processes), 20000);
    drop((plain_node, queued_node));
    assert_eq!(control.retire().unwrap().released, 1);
}
