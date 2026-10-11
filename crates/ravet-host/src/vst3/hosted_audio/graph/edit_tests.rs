//! Controller edit gestures and Graph-bank child editors through owned bank
//! nodes and the mock provider (decision 0276). Called from the single
//! main-thread scenario test in `tests.rs`, because the registry binds to its
//! first accessing thread under unit tests.
use super::super::mock::{self, Probe};
use super::edits::{
    self, EditEvent, EditKind, EditOptions, LATENCY_CHANGED, PARAMETER_VALUES_CHANGED, RawGesture,
};
use super::framework::{EventPolicy, FrameworkTranslation, ParameterRoute, ParameterScale};
use super::*;
use ravet_foundation::{
    AudioBuffer, Event, EventData, ParameterGesturePhase, ParameterId, Transport,
};
use std::sync::atomic::Ordering;

const STEREO: u64 = 0b11;
const GAIN: ParameterId = ParameterId(101);
const BEGIN: u32 = 1;
const PERFORM: u32 = 2;
const END: u32 = 3;
const RESTART: u32 = 4;
const RESTART_REFUSED: u32 = 5;

fn instrument() -> Vst3GraphNodeOptions {
    Vst3GraphNodeOptions {
        class_index: 1,
        inputs: vec![],
        outputs: vec![BusRequest::active(STEREO)],
        event_inputs: 0,
        event_outputs: 0,
        lanes: None,
    }
}
struct Bench {
    probe: &'static Probe,
    tables: mock::Tables,
}
fn bench(mode: u32) -> Bench {
    let probe = Probe::leak();
    probe.mode.store(mode, Ordering::SeqCst);
    Bench {
        probe,
        tables: mock::tables(),
    }
}
impl Bench {
    fn builder(&self, frames: u32) -> Vst3GraphBankBuilder<f64> {
        let library = mock::library(&self.tables, self.probe);
        // SAFETY: leaked exact-paired trusted mock tables.
        let (parameters, processing, hosted) = unsafe {
            (
                parameters::Provider::from_raw(self.tables.parameters).unwrap(),
                processing::Provider::from_raw(self.tables.processing).unwrap(),
                HostedProvider::from_raw(self.tables.hosted).unwrap(),
            )
        };
        Vst3GraphBankBuilder::new(library, parameters, processing, hosted, 48000., frames).unwrap()
    }
    fn edits(&self) -> edits::Provider<'static> {
        // SAFETY: leaked trusted mock table paired with this bench's instance table.
        unsafe { edits::Provider::from_raw(self.tables.edits).unwrap() }
    }
    fn inject(&self, child: u32, kind: u32, id: u32, value: f64) -> bool {
        mock::inject(self.probe, child, kind, id, 0, value)
    }
    fn points(&self) -> Vec<(u32, u32, f64)> {
        let capture = self.probe.captures().pop().expect("captured native input");
        capture
            .points
            .iter()
            .map(|p| (p.id, p.offset, p.value))
            .collect()
    }
    fn position(&self, entry: &str) -> usize {
        self.probe
            .entries()
            .iter()
            .position(|e| e == entry)
            .unwrap_or_else(|| panic!("missing {entry}: {:?}", self.probe.entries()))
    }
}
fn run(node: &mut Vst3GraphNode<f64>, events: &[Event], frames: usize) -> ProcessStatus {
    let mut out = [[0f64; 64]; 2];
    let [l, r] = &mut out;
    let mut outs = [&mut l[..], &mut r[..]];
    let audio = AudioBuffer::separate(&[], &mut outs, frames).unwrap();
    node.process(&mut ProcessBlock::new(audio, events, Transport::default()))
}
fn prepared(node: &mut Vst3GraphNode<f64>) {
    node.prepare(ProcessConfig::new(48000., 64, 0, node.layout()).unwrap())
        .unwrap();
}
fn drained(control: &Vst3GraphBankControl, child: usize) -> Vec<EditEvent> {
    let mut out = Vec::new();
    let n = control.drain_edits(child, |e| out.push(e)).unwrap();
    assert_eq!(n, out.len());
    out
}
fn gain_translation() -> FrameworkTranslation {
    let mut t = FrameworkTranslation::new(0, 16);
    t.parameters = vec![ParameterRoute::new(
        GAIN,
        1,
        ParameterScale::Linear {
            minimum: 0.,
            maximum: 4.,
        },
    )];
    t
}

pub(super) fn scenarios() {
    edits_reach_audio_at_offset_zero_of_the_next_block();
    coalescing_superseding_and_framework_identity();
    id_limit_defers_and_refused_blocks_keep_values();
    backpressure_never_drops_and_overflow_is_counted();
    restarts_and_malformed_records();
    declarations_and_enable_refusals_unwind();
    edits_allocate_nothing_on_audio();
    continuous_offset_zero_automation_never_masks_edits();
    failed_dispatch_keeps_values_pending();
    returned_node_refuses_pump();
    phantom_id_zero_does_not_bypass_the_id_limit();
    failed_lane_dispatch_keeps_values_pending();
    #[cfg(feature = "vst3-editor")]
    bank_editor_defers_reclamation_and_orders_teardown();
}

fn edits_reach_audio_at_offset_zero_of_the_next_block() {
    let b = bench(mock::PARAMETERS | mock::CAPTURE);
    let mut builder = b.builder(64);
    let child = builder.node(instrument()).unwrap();
    let plain = builder.node(instrument()).unwrap();
    builder
        .edit_gestures(child, b.edits(), EditOptions::default())
        .unwrap();
    let (control, mut nodes) = builder.build().unwrap();
    // Enabled on the prepared child, before start and before any lease.
    assert!(b.position("edits enable 0") < b.position("start 0"));
    assert!(b.position("edits enable 0") < b.position("begin 0"));
    let mut other = nodes.pop().unwrap();
    let mut node = nodes.pop().unwrap();
    prepared(&mut node);
    prepared(&mut other);
    assert_eq!(
        control.pump_edits(plain),
        Err(BankError::NoEdits { child: plain })
    );
    assert_eq!(run(&mut node, &[], 64), ProcessStatus::Continue);
    assert!(b.points().is_empty());
    // Captured natively, not yet pumped: AUDIO sees nothing.
    assert!(b.inject(0, BEGIN, 1, 0.));
    assert!(b.inject(0, PERFORM, 1, 0.25));
    assert!(b.inject(0, END, 1, 0.));
    assert_eq!(run(&mut node, &[], 64), ProcessStatus::Continue);
    assert!(b.points().is_empty());
    assert_eq!(control.edit_state(child).unwrap().native_pending, 3);
    assert_eq!(control.pump_edits(child).unwrap(), 3);
    let state = control.edit_state(child).unwrap();
    assert_eq!(
        (state.native_pending, state.queued, state.values_in_flight),
        (0, 3, 1)
    );
    // The very next dispatched block carries the value at sample 0.
    assert_eq!(run(&mut node, &[], 64), ProcessStatus::Continue);
    assert_eq!(b.points(), vec![(1, 0, 0.25)]);
    assert_eq!(run(&mut node, &[], 64), ProcessStatus::Continue);
    assert!(b.points().is_empty());
    // A zero-frame block also delivers at offset 0.
    assert!(b.inject(0, PERFORM, 5, 1.0));
    assert_eq!(control.pump_edits(child).unwrap(), 1);
    assert_eq!(run(&mut node, &[], 0), ProcessStatus::Continue);
    assert_eq!(b.points(), vec![(5, 0, 1.0)]);
    let events = drained(&control, child);
    assert_eq!(
        events
            .iter()
            .map(|e| (e.sequence, e.kind))
            .collect::<Vec<_>>(),
        vec![
            (0, EditKind::Begin { id: 1 }),
            (
                1,
                EditKind::Perform {
                    id: 1,
                    normalized: 0.25
                }
            ),
            (2, EditKind::End { id: 1 }),
            (
                3,
                EditKind::Perform {
                    id: 5,
                    normalized: 1.0
                }
            ),
        ]
    );
    // Without a translation there is no framework identity.
    assert!(events.iter().all(|e| e.framework.is_none()));
    let state = control.edit_state(child).unwrap();
    assert_eq!((state.applied, state.pumped, state.queued), (2, 4, 0));
    assert_eq!(control.diagnostic(child).unwrap(), 0);
    drop((node, other));
    assert_eq!(control.retire().unwrap().released, 1);
}

fn coalescing_superseding_and_framework_identity() {
    let b = bench(mock::PARAMETERS | mock::CAPTURE);
    let mut builder = b.builder(64);
    let child = builder.node(instrument()).unwrap();
    builder.translate(child, gain_translation()).unwrap();
    builder
        .edit_gestures(child, b.edits(), EditOptions::default())
        .unwrap();
    let (control, mut nodes) = builder.build().unwrap();
    let mut node = nodes.pop().unwrap();
    prepared(&mut node);
    // Last value per id wins before delivery.
    for (id, value) in [(1, 0.1), (1, 0.2), (5, 0.3), (1, 0.75)] {
        assert!(b.inject(0, PERFORM, id, value));
    }
    assert_eq!(control.pump_edits(child).unwrap(), 4);
    assert_eq!(run(&mut node, &[], 64), ProcessStatus::Continue);
    assert_eq!(b.points(), vec![(1, 0, 0.75), (5, 0, 0.3)]);
    let state = control.edit_state(child).unwrap();
    assert_eq!((state.applied, state.coalesced), (2, 2));
    // The edit wins over a framework point for the same id at offset 0; the
    // block's later point for that id still applies.
    assert!(b.inject(0, BEGIN, 1, 0.));
    assert!(b.inject(0, PERFORM, 1, 0.5));
    assert!(b.inject(0, PERFORM, 5, 0.25));
    assert!(b.inject(0, END, 1, 0.));
    assert_eq!(control.pump_edits(child).unwrap(), 4);
    let framework = [
        Event {
            frame_offset: 0,
            data: EventData::ParameterValue {
                id: GAIN,
                value: 3.,
            },
        },
        Event {
            frame_offset: 9,
            data: EventData::ParameterValue {
                id: GAIN,
                value: 1.,
            },
        },
    ];
    assert_eq!(run(&mut node, &framework, 64), ProcessStatus::Continue);
    assert_eq!(b.points(), vec![(1, 0, 0.5), (5, 0, 0.25), (1, 9, 0.25)]);
    let state = control.edit_state(child).unwrap();
    assert_eq!((state.applied, state.overridden), (4, 1));
    // An edit for an id the block touches only later is still placed at 0.
    assert!(b.inject(0, PERFORM, 1, 0.125));
    assert_eq!(control.pump_edits(child).unwrap(), 1);
    assert_eq!(run(&mut node, &framework[1..], 64), ProcessStatus::Continue);
    assert_eq!(b.points(), vec![(1, 0, 0.125), (1, 9, 0.25)]);
    // Routed ids carry framework identity in physical units; others do not.
    let events = drained(&control, child);
    let framework: Vec<_> = events.iter().map(|e| e.framework).collect();
    assert_eq!(
        framework[4..],
        [
            Some(EventData::ParameterGesture {
                id: GAIN,
                phase: ParameterGesturePhase::Begin
            }),
            Some(EventData::ParameterValue {
                id: GAIN,
                value: 2.
            }),
            None,
            Some(EventData::ParameterGesture {
                id: GAIN,
                phase: ParameterGesturePhase::End
            }),
            Some(EventData::ParameterValue {
                id: GAIN,
                value: 0.5
            }),
        ]
    );
    assert_eq!(
        framework[0..4],
        [
            Some(EventData::ParameterValue {
                id: GAIN,
                value: 0.4
            }),
            Some(EventData::ParameterValue {
                id: GAIN,
                value: 0.8
            }),
            None,
            Some(EventData::ParameterValue {
                id: GAIN,
                value: 3.
            }),
        ]
    );
    drop(node);
    assert_eq!(control.retire().unwrap().released, 1);
}

fn id_limit_defers_and_refused_blocks_keep_values() {
    let b = bench(mock::PARAMETERS | mock::CAPTURE);
    let mut builder = b.builder(64);
    let child = builder.node(instrument()).unwrap();
    let mut t = gain_translation();
    t.unmapped_parameters = EventPolicy::Refuse;
    builder.translate(child, t).unwrap();
    builder
        .edit_gestures(child, b.edits(), EditOptions::default())
        .unwrap();
    let (control, mut nodes) = builder.build().unwrap();
    let mut node = nodes.pop().unwrap();
    prepared(&mut node);
    // 71 distinct ids exceed the 64-id native queue: 7 are deferred.
    for id in 100..171 {
        assert!(b.inject(0, PERFORM, id, 0.5));
    }
    assert_eq!(control.pump_edits(child).unwrap(), 71);
    // A refused framework block is silent and keeps every value pending.
    let unmapped = [Event {
        frame_offset: 0,
        data: EventData::ParameterValue {
            id: ParameterId(999),
            value: 1.,
        },
    }];
    let captures = b.probe.captures().len();
    assert_eq!(run(&mut node, &unmapped, 64), ProcessStatus::Silence);
    assert_eq!(b.probe.captures().len(), captures);
    assert_eq!(control.refused(child).unwrap(), 1);
    assert_eq!(run(&mut node, &[], 64), ProcessStatus::Continue);
    let first = b.points();
    assert_eq!(first.len(), 64);
    assert!(
        first
            .iter()
            .all(|&(_, offset, value)| offset == 0 && value == 0.5)
    );
    assert_eq!(run(&mut node, &[], 64), ProcessStatus::Continue);
    let second = b.points();
    assert_eq!(
        second.iter().map(|p| p.0).collect::<Vec<_>>(),
        (164..171).collect::<Vec<_>>()
    );
    let state = control.edit_state(child).unwrap();
    assert_eq!((state.applied, state.deferred_blocks), (71, 1));
    drop(node);
    assert_eq!(control.retire().unwrap().released, 1);
}

fn backpressure_never_drops_and_overflow_is_counted() {
    let b = bench(mock::PARAMETERS | mock::CAPTURE);
    let mut builder = b.builder(64);
    let child = builder.node(instrument()).unwrap();
    builder
        .edit_gestures(
            child,
            b.edits(),
            EditOptions {
                events: 4,
                values: 3,
            },
        )
        .unwrap();
    let (control, mut nodes) = builder.build().unwrap();
    let mut node = nodes.pop().unwrap();
    prepared(&mut node);
    for i in 0..10 {
        assert!(b.inject(0, PERFORM, 1 + (i % 2) * 4, f64::from(i) / 10.));
    }
    // The AUDIO ring (3) limits the move; nothing leaves the native ring early.
    assert_eq!(control.pump_edits(child).unwrap(), 3);
    assert_eq!(control.pump_edits(child).unwrap(), 0);
    assert_eq!(control.edit_state(child).unwrap().native_pending, 7);
    assert_eq!(run(&mut node, &[], 64), ProcessStatus::Continue);
    assert_eq!(b.points(), vec![(1, 0, 0.2), (5, 0, 0.1)]);
    // Now the host queue (4) is the limit until it is drained.
    assert_eq!(control.pump_edits(child).unwrap(), 1);
    assert_eq!(control.pump_edits(child).unwrap(), 0);
    assert_eq!(drained(&control, child).len(), 4);
    assert_eq!(control.pump_edits(child).unwrap(), 2);
    assert_eq!(run(&mut node, &[], 64), ProcessStatus::Continue);
    assert_eq!(control.pump_edits(child).unwrap(), 2);
    assert_eq!(drained(&control, child).len(), 4);
    assert_eq!(run(&mut node, &[], 64), ProcessStatus::Continue);
    assert_eq!(control.pump_edits(child).unwrap(), 2);
    assert_eq!(drained(&control, child).len(), 2);
    let state = control.edit_state(child).unwrap();
    assert_eq!(
        (state.native_pending, state.pumped, state.queued),
        (0, 10, 0)
    );
    // A full native ring refuses the plug-in's next edit, counted.
    for i in 0..edits::NATIVE_CAPACITY {
        assert!(b.inject(0, PERFORM, 1, (i % 2) as f64));
    }
    assert!(!b.inject(0, PERFORM, 1, 0.5));
    let state = control.edit_state(child).unwrap();
    assert_eq!((state.native_pending, state.overflowed), (256, 1));
    // Every accepted record still arrives, in order, without gaps.
    let mut sequences = Vec::new();
    while control.edit_state(child).unwrap().native_pending > 0 {
        control.pump_edits(child).unwrap();
        assert_eq!(run(&mut node, &[], 64), ProcessStatus::Continue);
        control
            .drain_edits(child, |e| sequences.push(e.sequence))
            .unwrap();
    }
    assert_eq!(sequences, (10..10 + 256).collect::<Vec<_>>());
    drop(node);
    assert_eq!(control.retire().unwrap().released, 1);
}

fn restarts_and_malformed_records() {
    let b = bench(mock::PARAMETERS);
    let mut builder = b.builder(64);
    let child = builder.node(instrument()).unwrap();
    builder
        .edit_gestures(child, b.edits(), EditOptions::default())
        .unwrap();
    let (control, mut nodes) = builder.build().unwrap();
    let mut node = nodes.pop().unwrap();
    prepared(&mut node);
    assert!(mock::inject(
        b.probe,
        0,
        RESTART,
        0,
        PARAMETER_VALUES_CHANGED,
        0.
    ));
    assert!(mock::inject(
        b.probe,
        0,
        RESTART,
        0,
        LATENCY_CHANGED | 16,
        0.
    ));
    assert!(mock::inject(b.probe, 0, RESTART_REFUSED, 0, 2, 0.));
    assert_eq!(control.pump_edits(child).unwrap(), 3);
    assert_eq!(
        drained(&control, child)
            .iter()
            .map(|e| e.kind)
            .collect::<Vec<_>>(),
        vec![
            EditKind::Restart {
                flags: PARAMETER_VALUES_CHANGED,
                accepted: true,
                rebuild_required: false
            },
            EditKind::Restart {
                flags: LATENCY_CHANGED | 16,
                accepted: true,
                rebuild_required: true
            },
            EditKind::Restart {
                flags: 2,
                accepted: false,
                rebuild_required: false
            },
        ]
    );
    // Cached latency is unchanged by an accepted latency restart.
    assert_eq!(node.timing(), control.timing(child).unwrap());
    // A sequence gap, a supported flag marked refused, or a bad value is an
    // ABI violation reported to the caller.
    let raw = |kind, flags, value, sequence| RawGesture {
        struct_size: size_of::<RawGesture>() as u32,
        kind,
        id: if kind == PERFORM { 1 } else { 0 },
        flags,
        value,
        sequence,
    };
    // A synthesised End (editor closed mid-gesture) is published as such.
    assert!(mock::inject(b.probe, 0, END, 1, edits::SYNTHESIZED, 0.));
    assert_eq!(control.pump_edits(child).unwrap(), 1);
    let synthesized = drained(&control, child);
    assert_eq!(
        (synthesized[0].kind, synthesized[0].synthesized),
        (EditKind::End { id: 1 }, true)
    );
    let no_param = RawGesture {
        id: u32::MAX,
        ..raw(PERFORM, 0, 0.5, 4)
    };
    for bad in [
        no_param,
        raw(PERFORM, 0, 0.5, 5),
        raw(RESTART_REFUSED, PARAMETER_VALUES_CHANGED, 0., 4),
        raw(PERFORM, 0, 1.5, 4),
        raw(9, 0, 0., 4),
    ] {
        mock::inject_raw(b.probe, 0, bad);
        assert_eq!(
            control.pump_edits(child),
            Err(BankError::Edit {
                child,
                error: edits::Error::Abi
            })
        );
    }
    drop(node);
    assert_eq!(control.retire().unwrap().released, 1);
}

fn declarations_and_enable_refusals_unwind() {
    let b = bench(mock::PARAMETERS);
    let other = mock::tables();
    let mut builder = b.builder(64);
    let child = builder.node(instrument()).unwrap();
    // SAFETY: leaked trusted mock table of a different instance origin.
    let foreign = unsafe { edits::Provider::from_raw(other.edits).unwrap() };
    assert_eq!(
        builder.edit_gestures(child, foreign, EditOptions::default()),
        Err(BankError::Edit {
            child,
            error: edits::Error::Abi
        })
    );
    for bad in [
        EditOptions {
            events: 0,
            values: 1,
        },
        EditOptions {
            events: 1,
            values: 257,
        },
        EditOptions {
            events: 4097,
            values: 1,
        },
    ] {
        assert_eq!(
            builder.edit_gestures(child, b.edits(), bad),
            Err(BankError::Edit {
                child,
                error: edits::Error::Shape
            })
        );
    }
    assert_eq!(
        builder.edit_gestures(9, b.edits(), EditOptions::default()),
        Err(BankError::Shape)
    );
    let mut table = RawProviderCopy::from(b.tables.edits);
    table.0.drain = None;
    // SAFETY: the malformed table is rejected before any call.
    assert!(matches!(
        unsafe { edits::Provider::from_raw(&table.0) },
        Err(edits::Error::Abi)
    ));
    drop(builder);
    // A native enable refusal fails build before any lease and unwinds.
    let b = bench(mock::PARAMETERS | mock::FAIL_ENABLE);
    let mut builder = b.builder(64);
    let child = builder.node(instrument()).unwrap();
    builder.node(instrument()).unwrap();
    builder
        .edit_gestures(child, b.edits(), EditOptions::default())
        .unwrap();
    assert_eq!(
        builder.build().err(),
        Some(BankError::Edit {
            child,
            error: edits::Error::Lifecycle
        })
    );
    assert_eq!(b.probe.count(&b.probe.begins), 0);
    assert_eq!(b.probe.count(&b.probe.starts), 0);
    assert_eq!(b.probe.count(&b.probe.destroys), 1);
    assert_eq!(b.probe.count(&b.probe.closes), 1);
}
struct RawProviderCopy(edits::RawProvider);
impl From<&edits::RawProvider> for RawProviderCopy {
    fn from(raw: &edits::RawProvider) -> Self {
        Self(edits::RawProvider {
            struct_size: raw.struct_size,
            version: raw.version,
            instance_origin: raw.instance_origin,
            enable: raw.enable,
            drain: raw.drain,
            state: raw.state,
        })
    }
}

fn edits_allocate_nothing_on_audio() {
    let b = bench(mock::PARAMETERS);
    let mut builder = b.builder(64);
    let child = builder.node(instrument()).unwrap();
    builder.translate(child, gain_translation()).unwrap();
    builder
        .edit_gestures(child, b.edits(), EditOptions::default())
        .unwrap();
    let (control, mut nodes) = builder.build().unwrap();
    let mut node = nodes.pop().unwrap();
    prepared(&mut node);
    let mut out = [[0f64; 64]; 2];
    let mut audio_allocations = 0;
    for i in 0..10_000u32 {
        // MAIN: capture, pump and publish (may allocate; not measured).
        if i % 3 == 0 {
            assert!(b.inject(0, BEGIN, 1, 0.));
            assert!(b.inject(0, PERFORM, 1, f64::from(i % 7) / 7.));
            assert!(b.inject(0, PERFORM, 100 + i % 70, 0.5));
            assert!(b.inject(0, END, 1, 0.));
            control.pump_edits(child).unwrap();
            control.drain_edits(child, |_| {}).unwrap();
        }
        let frames = 1 + (i as usize) % 64;
        let events = [Event {
            frame_offset: 0,
            data: EventData::ParameterValue {
                id: GAIN,
                value: (i % 5) as f32,
            },
        }];
        let used = if i % 2 == 0 { &events[..] } else { &[] };
        // AUDIO: measured per block.
        let measured = allocation_counter::measure(|| {
            let [l, r] = &mut out;
            let mut outs = [&mut l[..], &mut r[..]];
            let audio = AudioBuffer::separate(&[], &mut outs, frames).unwrap();
            assert_eq!(
                node.process(&mut ProcessBlock::new(audio, used, Transport::default())),
                ProcessStatus::Continue
            );
        });
        audio_allocations += measured.count_total;
    }
    assert_eq!(audio_allocations, 0);
    // 3334 rounds of two values, every one applied; on the 1667 even rounds
    // the id-1 edit replaces the framework point at offset 0.
    let state = control.edit_state(child).unwrap();
    assert_eq!(
        (
            state.applied,
            state.overridden,
            state.coalesced,
            state.values_pending
        ),
        (6668, 1667, 0, 0)
    );
    drop(node);
    assert_eq!(control.retire().unwrap().released, 1);
}

#[cfg(feature = "vst3-editor")]
fn bank_editor_defers_reclamation_and_orders_teardown() {
    use crate::vst3::instances::editor::{self, Closed, Platform, Size};
    use crate::vst3::instances::parameters::Point;
    let b = bench(mock::PARAMETERS | mock::CAPTURE);
    let mut builder = b.builder(64);
    let child = builder.node(instrument()).unwrap();
    builder.node(instrument()).unwrap();
    builder
        .edit_gestures(child, b.edits(), EditOptions::default())
        .unwrap();
    let (control, nodes) = builder.build().unwrap();
    // SAFETY: leaked trusted mock editor table paired with the instance table.
    let provider = unsafe { editor::Provider::from_raw(b.tables.editor).unwrap() };
    let other = mock::tables();
    // SAFETY: leaked trusted mock table of a different instance origin.
    let foreign = unsafe { editor::Provider::from_raw(other.editor).unwrap() };
    assert_eq!(
        control.open_editor(child, foreign, Platform::NsView).err(),
        Some(BankError::Editor {
            child,
            error: editor::Error::Abi
        })
    );
    assert_eq!(
        control.open_editor(7, provider, Platform::NsView).err(),
        Some(BankError::Shape)
    );
    // AUDIO keeps running on a worker while the editor lives on MAIN.
    let (go, wait) = std::sync::mpsc::channel::<bool>();
    let (done, finished) = std::sync::mpsc::channel::<usize>();
    let worker = std::thread::spawn(move || {
        let mut nodes = nodes;
        for node in &mut nodes {
            prepared(node);
        }
        let mut blocks = 0;
        while wait.recv().unwrap() {
            for node in &mut nodes {
                assert_eq!(run(node, &[], 64), ProcessStatus::Continue);
            }
            blocks += 1;
            done.send(blocks).unwrap();
        }
        drop(nodes); // return every bundle from the worker
        done.send(blocks).unwrap();
    });
    let mut editor = control
        .open_editor(child, provider, Platform::NsView)
        .unwrap();
    assert_eq!((editor.bank(), editor.child()), (control.id(), child));
    assert_eq!(
        control.open_editor(child, provider, Platform::NsView).err(),
        Some(BankError::EditorOpen { child })
    );
    let mut parent = 0u8;
    // SAFETY: the mock never dereferences the parent pointer.
    let size = unsafe { editor.attach(NonNull::from(&mut parent).cast()) }.unwrap();
    assert!(editor.attached());
    assert_eq!(size, Size::new(400, 300).unwrap());
    go.send(true).unwrap();
    assert_eq!(finished.recv().unwrap(), 1);
    assert_eq!(
        editor.resize(Size::new(640, 480).unwrap()).unwrap(),
        Size::new(640, 480).unwrap()
    );
    assert!(matches!(
        editor.set_content_scale(2.),
        Err(editor::Error::Unsupported(_))
    ));
    assert_eq!(editor.take_resize_request().unwrap(), None);
    assert!(editor.state().unwrap().attached());
    // An edit captured while the editor is open reaches the next block.
    assert!(b.inject(0, PERFORM, 1, 0.5));
    assert_eq!(control.pump_edits(child).unwrap(), 1);
    go.send(true).unwrap();
    assert_eq!(finished.recv().unwrap(), 2);
    assert!(b.probe.captures().iter().any(|c| c.points
        == [Point {
            id: 1,
            offset: 0,
            value: 0.5
        }]));
    // Nodes returned and control dropped: reclamation is deferred, not forced.
    go.send(false).unwrap();
    finished.recv().unwrap();
    worker.join().unwrap();
    let id = control.id();
    drop(control);
    let report = reclaim_vst3_graph_banks(Some(id)).unwrap();
    assert_eq!(
        (report.released, report.retained, report.editors),
        (0, 1, 1)
    );
    assert_eq!(b.probe.count(&b.probe.ends), 0);
    assert_eq!(b.probe.count(&b.probe.stops), 0);
    // A refused close returns the guard; the retry consumes it.
    b.probe
        .mode
        .fetch_or(mock::REFUSE_EDITOR_DESTROY, Ordering::SeqCst);
    let failure = editor.close().expect_err("refused close");
    assert_eq!(failure.error, editor::Error::Thread);
    let editor = failure.owner;
    assert!(editor.attached());
    assert_eq!(editor.close().ok(), Some(Closed::Complete));
    let report = reclaim_vst3_graph_banks(Some(id)).unwrap();
    assert_eq!((report.released, report.editors), (1, 0));
    // View released before any lease end, stop or child retirement.
    let destroyed = b.position("editor destroy 0");
    assert!(b.position("editor removed 0") < destroyed);
    for later in ["end 0", "end 1", "stop 0", "destroy 0", "close"] {
        assert!(destroyed < b.position(later), "{later}");
    }
    assert!(
        !b.probe
            .entries()
            .iter()
            .any(|e| e.starts_with("destroy refused"))
    );
    assert_eq!(b.probe.count(&b.probe.control_during_audio), 0);
    // Dropping a guard (instead of close) also re-enables reclamation.
    let b = bench(mock::PARAMETERS);
    let mut builder = b.builder(64);
    let child = builder.node(instrument()).unwrap();
    let (control, nodes) = builder.build().unwrap();
    // SAFETY: as above.
    let provider = unsafe { editor::Provider::from_raw(b.tables.editor).unwrap() };
    let editor = control
        .open_editor(child, provider, Platform::NsView)
        .unwrap();
    drop(nodes);
    let id = control.id();
    drop(control);
    assert_eq!(reclaim_vst3_graph_banks(Some(id)).unwrap().editors, 1);
    drop(editor);
    assert_eq!(reclaim_vst3_graph_banks(Some(id)).unwrap().released, 1);
    assert!(b.position("editor destroy 0") < b.position("destroy 0"));
}

/// A drop-time destroy refused before teardown keeps the native editor and the
/// bank retained for the process lifetime. Leaks one bank, so the main
/// scenario runs it after the absolute bank-count assertions.
#[cfg(feature = "vst3-editor")]
pub(super) fn refused_editor_drop_keeps_bank_retained() {
    use crate::vst3::instances::editor::{self, Platform};
    let b = bench(mock::PARAMETERS);
    let mut builder = b.builder(64);
    let child = builder.node(instrument()).unwrap();
    let (control, nodes) = builder.build().unwrap();
    // SAFETY: leaked trusted mock editor table paired with the instance table.
    let provider = unsafe { editor::Provider::from_raw(b.tables.editor).unwrap() };
    let editor = control
        .open_editor(child, provider, Platform::NsView)
        .unwrap();
    drop(nodes);
    let id = control.id();
    drop(control);
    let before = editor::refused_drops();
    b.probe
        .mode
        .fetch_or(mock::REFUSE_EDITOR_DESTROY, Ordering::SeqCst);
    drop(editor);
    assert_eq!(editor::refused_drops(), before + 1);
    // The native view lives on: the bank is never released out of order.
    for _ in 0..3 {
        let report = reclaim_vst3_graph_banks(Some(id)).unwrap();
        assert_eq!((report.released, report.editors), (0, 1));
    }
    assert_eq!(b.probe.count(&b.probe.ends), 0);
    assert_eq!(b.probe.count(&b.probe.destroys), 0);
}

#[test]
fn edit_gesture_abi_layout_matches_native() {
    use std::mem::{align_of, offset_of};
    // Same values as the C11 edit-gesture-abi-tests (64-bit macOS profile).
    assert_eq!((size_of::<RawGesture>(), align_of::<RawGesture>()), (32, 8));
    assert_eq!(
        (
            offset_of!(RawGesture, value),
            offset_of!(RawGesture, sequence)
        ),
        (16, 24)
    );
    assert_eq!(size_of::<edits::RawState>(), 104);
    assert_eq!(
        (
            offset_of!(edits::RawState, open),
            offset_of!(edits::RawState, accepted),
            offset_of!(edits::RawState, unpaired)
        ),
        (16, 24, 96)
    );
    assert_eq!(
        (edits::OPEN_GESTURES, edits::MAXIMUM_EDIT_VALUES),
        (64, 256)
    );
    assert_eq!(size_of::<edits::RawProvider>(), 40);
    assert_eq!(
        (
            offset_of!(edits::RawProvider, instance_origin),
            offset_of!(edits::RawProvider, enable),
            offset_of!(edits::RawProvider, state)
        ),
        (8, 16, 32)
    );
    assert_eq!(edits::SUPPORTED_RESTART, 28);
    assert_eq!(edits::NATIVE_CAPACITY, 256);
}

/// F8: framework automation writes the same id at offset 0 in EVERY block. Each
/// edit is still applied (it replaces that block's offset-0 point), and the
/// automation resumes on the following block.
fn continuous_offset_zero_automation_never_masks_edits() {
    let b = bench(mock::PARAMETERS | mock::CAPTURE);
    let mut builder = b.builder(64);
    let child = builder.node(instrument()).unwrap();
    builder.translate(child, gain_translation()).unwrap();
    builder
        .edit_gestures(child, b.edits(), EditOptions::default())
        .unwrap();
    let (control, mut nodes) = builder.build().unwrap();
    let mut node = nodes.pop().unwrap();
    prepared(&mut node);
    let automation = |value: f32| {
        [
            Event {
                frame_offset: 0,
                data: EventData::ParameterValue { id: GAIN, value },
            },
            Event {
                frame_offset: 32,
                data: EventData::ParameterValue { id: GAIN, value },
            },
        ]
    };
    let mut edits = 0;
    for block in 0..200u32 {
        let edit = block % 3 == 0;
        if edit {
            let value = f64::from(block % 7) / 8.;
            assert!(b.inject(0, PERFORM, 1, value));
            assert_eq!(control.pump_edits(child).unwrap(), 1);
            edits += 1;
        }
        assert_eq!(run(&mut node, &automation(2.), 64), ProcessStatus::Continue);
        let points = b.points();
        if edit {
            // The edit at 0 replaces the automation's offset-0 point.
            assert_eq!(
                points,
                vec![(1, 0, f64::from(block % 7) / 8.), (1, 32, 0.5)]
            );
        } else {
            assert_eq!(points, vec![(1, 0, 0.5), (1, 32, 0.5)]);
        }
    }
    let state = control.edit_state(child).unwrap();
    assert_eq!(
        (state.applied, state.overridden, state.values_pending),
        (edits, edits, 0)
    );
    drop(node);
    assert_eq!(control.retire().unwrap().released, 1);
}

/// F9: a value placed in a block whose native dispatch fails stays pending and
/// is not counted as applied.
fn failed_dispatch_keeps_values_pending() {
    let b = bench(mock::PARAMETERS | mock::CAPTURE);
    let mut builder = b.builder(64);
    let child = builder.node(instrument()).unwrap();
    builder
        .edit_gestures(child, b.edits(), EditOptions::default())
        .unwrap();
    let (control, mut nodes) = builder.build().unwrap();
    let mut node = nodes.pop().unwrap();
    prepared(&mut node);
    // The mock fails every dispatch after the first.
    assert_eq!(run(&mut node, &[], 64), ProcessStatus::Continue);
    b.probe.mode.fetch_or(mock::FAIL_PROCESS, Ordering::SeqCst);
    assert!(b.inject(0, PERFORM, 1, 0.25));
    assert!(b.inject(0, PERFORM, 5, 0.75));
    assert_eq!(control.pump_edits(child).unwrap(), 2);
    assert!(matches!(run(&mut node, &[], 64), ProcessStatus::Fault(_)));
    let state = control.edit_state(child).unwrap();
    assert_eq!((state.applied, state.values_pending), (0, 2));
    assert_ne!(
        control.diagnostic(child).unwrap() & (diagnostic::NATIVE_FAULT | diagnostic::ADMISSION),
        0
    );
    drop(node);
    assert_eq!(control.retire().unwrap().released, 1);
}

/// F4: once the node has returned to MAIN, pump_edits refuses and moves
/// nothing, before and after the bundle is gathered.
fn returned_node_refuses_pump() {
    let b = bench(mock::PARAMETERS);
    let mut builder = b.builder(64);
    let child = builder.node(instrument()).unwrap();
    builder
        .edit_gestures(child, b.edits(), EditOptions::default())
        .unwrap();
    let (control, mut nodes) = builder.build().unwrap();
    assert!(b.inject(0, PERFORM, 1, 0.5));
    drop(nodes.pop().unwrap());
    let abandoned = Err(BankError::Edit {
        child,
        error: edits::Error::Abandoned,
    });
    assert_eq!(control.pump_edits(child), abandoned);
    assert_eq!(control.poll_returns().unwrap().returned, 1);
    assert_eq!(control.pump_edits(child), abandoned);
    let state = control.edit_state(child).unwrap();
    assert_eq!((state.native_pending, state.pumped), (1, 0));
    assert_eq!(control.retire().unwrap().released, 1);
}

fn lane_instrument(points: usize) -> Vst3GraphNodeOptions {
    Vst3GraphNodeOptions {
        lanes: Some(LaneOptions {
            batches: 2,
            input_events: 0,
            input_payload: 0,
            input_points: points,
            output_events: 0,
            output_payload: 0,
            // The mock echoes every input point to the output queues.
            output_queues: 64,
            output_points: 128,
        }),
        ..instrument()
    }
}
/// Submit one lane batch whose points use 63 distinct ids (1000..1063) at
/// offset 5.
fn submit_63(control: &Vst3GraphBankControl, child: usize) {
    control
        .submit(child, 64, |input| {
            for id in 1000..1063 {
                input
                    .points
                    .push(crate::vst3::instances::parameters::Point {
                        id,
                        offset: 5,
                        value: 0.5,
                    })?;
            }
            Ok(())
        })
        .unwrap();
}

/// N1 (R3): a block with 63 distinct ids leaves room for exactly one new edit
/// id. Native id 0 must be counted as a new id like any other, not matched
/// against an unwritten slot, so the 64-id limit holds: one edit applies, the
/// other is deferred to the next block, and the node never faults.
fn phantom_id_zero_does_not_bypass_the_id_limit() {
    let b = bench(mock::PARAMETERS | mock::CAPTURE);
    let mut builder = b.builder(64);
    let child = builder.node(lane_instrument(64)).unwrap();
    builder
        .edit_gestures(child, b.edits(), EditOptions::default())
        .unwrap();
    let (control, mut nodes) = builder.build().unwrap();
    let mut node = nodes.pop().unwrap();
    prepared(&mut node);
    // A non-zero new id first, then id 0.
    assert!(b.inject(0, PERFORM, 7, 0.25));
    assert!(b.inject(0, PERFORM, 0, 0.75));
    assert_eq!(control.pump_edits(child).unwrap(), 2);
    let edit_points = |points: &[(u32, u32, f64)]| {
        points
            .iter()
            .filter(|p| p.1 == 0)
            .copied()
            .collect::<Vec<_>>()
    };
    submit_63(&control, child);
    assert_eq!(run(&mut node, &[], 64), ProcessStatus::Continue);
    let first = b.points();
    assert_eq!(first.len(), 64);
    assert_eq!(edit_points(&first), vec![(7, 0, 0.25)]);
    submit_63(&control, child);
    assert_eq!(run(&mut node, &[], 64), ProcessStatus::Continue);
    let second = b.points();
    assert_eq!(edit_points(&second), vec![(0, 0, 0.75)]);
    let state = control.edit_state(child).unwrap();
    assert_eq!(
        (state.applied, state.deferred_blocks, state.values_pending),
        (2, 1, 0)
    );
    assert_eq!(control.diagnostic(child).unwrap(), 0);
    for _ in 0..2 {
        assert!(
            control
                .receive(child, |o| o.output.is_ok())
                .unwrap()
                .unwrap()
        );
    }
    drop(node);
    assert_eq!(control.retire().unwrap().released, 1);
}

/// F9 on the queued-lane path: a failed dispatch keeps placed values pending.
fn failed_lane_dispatch_keeps_values_pending() {
    let b = bench(mock::PARAMETERS | mock::CAPTURE);
    let mut builder = b.builder(64);
    let child = builder.node(lane_instrument(64)).unwrap();
    builder
        .edit_gestures(child, b.edits(), EditOptions::default())
        .unwrap();
    let (control, mut nodes) = builder.build().unwrap();
    let mut node = nodes.pop().unwrap();
    prepared(&mut node);
    control.submit(child, 64, |_| Ok(())).unwrap();
    assert_eq!(run(&mut node, &[], 64), ProcessStatus::Continue);
    b.probe.mode.fetch_or(mock::FAIL_PROCESS, Ordering::SeqCst);
    assert!(b.inject(0, PERFORM, 1, 0.25));
    assert_eq!(control.pump_edits(child).unwrap(), 1);
    control.submit(child, 64, |_| Ok(())).unwrap();
    assert!(matches!(run(&mut node, &[], 64), ProcessStatus::Fault(_)));
    let state = control.edit_state(child).unwrap();
    assert_eq!((state.applied, state.values_pending), (0, 1));
    drop(node);
    assert_eq!(control.retire().unwrap().released, 1);
}
