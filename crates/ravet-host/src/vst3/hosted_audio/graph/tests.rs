//! Bank ownership scenarios. The registry binds to its first accessing thread
//! under unit tests, so every scenario runs sequentially in ONE test function.
use super::super::mock::{self, Probe};
use super::*;
use crate::vst3::instances::parameters::Point;
use ravet_foundation::{AudioBuffer, Transport};
use ravet_graph::{Connection, Destination, Graph, GraphNode, GraphStatus, NodeId, Source};
use std::{panic::AssertUnwindSafe, sync::atomic::Ordering};

const STEREO: u64 = 0b11;
const MONO: u64 = 0b1;

fn effect(lanes: Option<LaneOptions>) -> Vst3GraphNodeOptions {
    Vst3GraphNodeOptions {
        class_index: 0,
        inputs: vec![
            BusRequest::active(STEREO),
            BusRequest::active(MONO),
            BusRequest::inactive(STEREO),
        ],
        outputs: vec![BusRequest::active(STEREO), BusRequest::inactive(MONO)],
        event_inputs: 0,
        event_outputs: 0,
        lanes,
    }
}
fn instrument(lanes: Option<LaneOptions>) -> Vst3GraphNodeOptions {
    Vst3GraphNodeOptions {
        class_index: 1,
        inputs: vec![],
        outputs: vec![BusRequest::active(STEREO)],
        event_inputs: 0,
        event_outputs: 0,
        lanes,
    }
}
struct Fixture {
    tables: mock::Tables,
    probe: &'static Probe,
}
fn fixture(mode: u32) -> Fixture {
    let probe = Probe::leak();
    probe.mode.store(mode, Ordering::SeqCst);
    Fixture {
        tables: mock::tables(),
        probe,
    }
}
impl Fixture {
    fn builder<S: HostedSample>(&self, frames: u32) -> Vst3GraphBankBuilder<S> {
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
    fn n(&self, counter: &std::sync::atomic::AtomicUsize) -> usize {
        counter.load(Ordering::SeqCst)
    }
    fn position(&self, entry: &str) -> usize {
        self.probe
            .entries()
            .iter()
            .position(|e| e == entry)
            .unwrap_or_else(|| panic!("missing {entry}: {:?}", self.probe.entries()))
    }
}
fn live_banks() -> usize {
    with_registry(|r| r.banks.iter().filter(|b| b.is_some()).count()).unwrap()
}
fn process<'c, 's, S: HostedSample + Sample>(
    node: &mut Vst3GraphNode<S>,
    inputs: &'c [&'s [S]],
    outputs: &'c mut [&'s mut [S]],
    frames: usize,
) -> ProcessStatus {
    let audio = AudioBuffer::separate(inputs, outputs, frames).unwrap();
    node.process(&mut ProcessBlock::new(audio, &[], Transport::default()))
}

#[test]
fn graph_bank_ownership_scenarios_on_one_main_thread() {
    registry_is_bound_to_main();
    canonical_graph_pipeline_prepares_all_before_first_begin();
    control_dropped_before_worker_returns_keeps_module_loaded();
    checked_reset_requires_fresh_bank();
    native_fault_then_node_drop_returns_bundle();
    worker_panic_after_begin_returns_bundle();
    panic_in_setup_before_begin_releases_reservation();
    start_failure_unwinds_without_any_lease();
    begin_failure_ends_earlier_leases_before_any_stop();
    end_failure_retains_owners_until_successful_retry();
    rich_lanes_round_trip_and_refuse_before_native_mutation();
    lanes_and_audio_ten_thousand_cycles_allocate_nothing();
    super::translation_tests::scenarios();
    forgotten_node_and_refused_return_retain_bank();
    child_and_bank_capacity_and_generation_exhaustion();
}

fn registry_is_bound_to_main() {
    assert_eq!(live_banks(), 0);
    let other = std::thread::spawn(|| reclaim_vst3_graph_banks(None))
        .join()
        .unwrap();
    assert_eq!(other, Err(BankError::Thread));
}

fn canonical_graph_pipeline_prepares_all_before_first_begin() {
    let f = fixture(0);
    let mut builder = f.builder::<f32>(64);
    builder.node(effect(None)).unwrap();
    builder.node(instrument(None)).unwrap();
    let (control, mut nodes) = builder.build().unwrap();
    // Prepare everything FIRST, start all, begin every lease LAST.
    let last_prepare = f.position("prepare 1 bits=32");
    assert!(f.position("prepare 0 bits=32") < last_prepare);
    assert!(last_prepare < f.position("start 0"));
    assert!(f.position("start 1") < f.position("begin 0"));
    assert_eq!(f.n(&f.probe.control_during_audio), 0);
    assert_eq!(control.timing(0).unwrap().latency_samples, 3);
    assert_eq!(control.timing(0).unwrap().tail, Tail::Samples(480));
    assert_eq!(control.timing(1).unwrap().tail, Tail::Infinite);
    assert_eq!(control.profile(0).unwrap().map().input_channels(), 3);
    let instrument = nodes.pop().unwrap();
    let effect = nodes.pop().unwrap();
    assert_eq!(effect.layout(), AudioLayout::new(3, 2).unwrap());
    assert_eq!(instrument.layout(), AudioLayout::STEREO_INSTRUMENT);
    let (a, b) = (NodeId(1), NodeId(2));
    // Host stereo -> effect main; instrument ch0 -> effect packed aux channel 2;
    // effect main -> host. A disconnected-input instrument still runs.
    let connections = [
        (Source::Input(0), Destination::Node { id: a, channel: 0 }),
        (Source::Input(1), Destination::Node { id: a, channel: 1 }),
        (
            Source::Node { id: b, channel: 0 },
            Destination::Node { id: a, channel: 2 },
        ),
        (Source::Node { id: a, channel: 0 }, Destination::Output(0)),
        (Source::Node { id: a, channel: 1 }, Destination::Output(1)),
    ]
    .map(|(source, destination)| Connection {
        source,
        destination,
        gain: 1f32,
    });
    let mut graph = Graph::new(
        AudioLayout::STEREO,
        vec![
            GraphNode {
                id: a,
                layout: effect.layout(),
                processor: Box::new(effect),
            },
            GraphNode {
                id: b,
                layout: instrument.layout(),
                processor: Box::new(instrument),
            },
        ],
        &connections,
    )
    .unwrap();
    graph
        .prepare(ProcessConfig::new(48000., 64, 0, AudioLayout::STEREO).unwrap())
        .unwrap();
    assert_eq!(graph.latency_samples(), 3);
    assert_eq!(graph.tail(), Tail::Infinite);
    // Worker owns and finally drops the graph: no native teardown on AUDIO.
    let outputs = std::thread::spawn(move || {
        let mut results = Vec::new();
        for frames in [0usize, 1, 17, 64] {
            let left: Vec<f32> = (0..frames).map(|i| i as f32 * 0.01 - 0.2).collect();
            let right: Vec<f32> = (0..frames).map(|i| 0.5 - i as f32 * 0.003).collect();
            let mut out = [vec![9f32; 64], vec![9f32; 64]];
            let [l, r] = &mut out;
            let ins = [&left[..], &right[..]];
            let mut outs = [&mut l[..], &mut r[..]];
            let audio = AudioBuffer::separate(&ins, &mut outs, frames).unwrap();
            let status = graph.process(&mut ProcessBlock::new(audio, &[], Transport::default()));
            results.push((frames, left, right, out, status));
        }
        drop(graph);
        results
    })
    .join()
    .unwrap();
    for (frames, left, right, out, status) in outputs {
        assert_eq!(status, GraphStatus::Continue);
        for i in 0..frames {
            // Independent oracle: effect main*1.5 + instrument ch0 (0.1f32)*0.25.
            assert_eq!(out[0][i], left[i] * 1.5 + 0.1f32 * 0.25);
            assert_eq!(out[1][i], right[i] * 1.5 + 0.1f32 * 0.25);
        }
        assert_eq!(out[0][frames..], vec![9f32; 64 - frames][..]);
    }
    // Nodes returned; module still loaded until explicit MAIN reclaim.
    assert_eq!(control.poll_returns().unwrap().returned, 2);
    assert_eq!((f.n(&f.probe.ends), f.n(&f.probe.closes)), (0, 0));
    let report = control.retire().unwrap();
    assert_eq!(report.released, 1);
    assert!(f.position("end 1") < f.position("end 0"));
    assert!(f.position("end 0") < f.position("stop 1"));
    assert!(f.position("stop 0") < f.position("destroy 1"));
    assert!(f.position("destroy 0") < f.position("close"));
    assert_eq!(f.n(&f.probe.closes), 1);
    assert_eq!(f.n(&f.probe.control_during_audio), 0);
    assert_eq!(f.n(&f.probe.wrong_thread), 0);
    assert_eq!(live_banks(), 0);
}

fn control_dropped_before_worker_returns_keeps_module_loaded() {
    let f = fixture(0);
    let mut builder = f.builder::<f64>(32);
    builder.node(effect(None)).unwrap();
    let (control, mut nodes) = builder.build().unwrap();
    let id = control.id();
    let mut node = nodes.pop().unwrap();
    node.prepare(ProcessConfig::new(48000., 32, 0, node.layout()).unwrap())
        .unwrap();
    let (go, wait) = std::sync::mpsc::channel::<bool>();
    let worker = std::thread::spawn(move || {
        let main = [[0.1f64; 8], [0.3; 8]];
        let aux = [0.7f64; 8];
        let mut out = [[0f64; 8]; 2];
        let mut checks = 0;
        while wait.recv().unwrap() {
            let [l, r] = &mut out;
            let status = process(&mut node, &[&main[0], &main[1], &aux], &mut [l, r], 8);
            assert_eq!(status, ProcessStatus::Continue);
            assert_eq!(out[0][0].to_bits(), (0.1f64 * 1.5 + 0.7 * 0.25).to_bits());
            assert_ne!(out[0][0], f64::from(0.1f32 * 1.5 + 0.7f32 * 0.25));
            checks += 1;
        }
        drop(node);
        checks
    });
    go.send(true).unwrap();
    drop(control);
    // Orphaned but outstanding: everything stays retained, no native end.
    let report = reclaim_vst3_graph_banks(Some(id)).unwrap();
    assert_eq!((report.released, report.retained), (0, 1));
    assert_eq!((f.n(&f.probe.ends), f.n(&f.probe.closes)), (0, 0));
    // Valid processing continues after the control is gone.
    go.send(true).unwrap();
    go.send(false).unwrap();
    assert_eq!(worker.join().unwrap(), 2);
    let report = reclaim_vst3_graph_banks(None).unwrap();
    assert_eq!((report.returned, report.released), (1, 1));
    assert_eq!(
        (
            f.n(&f.probe.ends),
            f.n(&f.probe.destroys),
            f.n(&f.probe.closes)
        ),
        (1, 1, 1)
    );
    assert_eq!(live_banks(), 0);
}

fn checked_reset_requires_fresh_bank() {
    let f = fixture(0);
    let mut builder = f.builder::<f32>(16);
    builder.node(instrument(None)).unwrap();
    let (control, nodes) = builder.build().unwrap();
    let node = nodes.into_iter().next().unwrap();
    let id = NodeId(7);
    let mut graph = Graph::new(
        AudioLayout::STEREO_INSTRUMENT,
        vec![GraphNode {
            id,
            layout: node.layout(),
            processor: Box::new(node),
        }],
        &[
            Connection {
                source: Source::Node { id, channel: 0 },
                destination: Destination::Output(0),
                gain: 1.,
            },
            Connection {
                source: Source::Node { id, channel: 1 },
                destination: Destination::Output(1),
                gain: 1.,
            },
        ],
    )
    .unwrap();
    graph
        .prepare(ProcessConfig::new(48000., 16, 0, AudioLayout::STEREO_INSTRUMENT).unwrap())
        .unwrap();
    let render = |graph: &mut Graph<f32>| {
        let mut out = [[5f32; 4]; 2];
        let [l, r] = &mut out;
        let mut outs = [&mut l[..], &mut r[..]];
        let audio = AudioBuffer::separate(&[], &mut outs, 4).unwrap();
        (
            graph.process(&mut ProcessBlock::new(audio, &[], Transport::default())),
            out,
        )
    };
    assert_eq!(
        render(&mut graph),
        (GraphStatus::Continue, [[0.1; 4], [0.2; 4]])
    );
    let processes = f.n(&f.probe.processes);
    let refused = GraphStatus::Fault {
        node: Some(id),
        error: ProcessError::RebuildRequired,
    };
    assert_eq!(graph.try_reset(), Err(refused));
    // Legacy reset cannot clear the refusal either; no foreign call occurred.
    graph.reset();
    let (status, out) = render(&mut graph);
    assert_ne!(status, GraphStatus::Continue);
    assert_eq!(out, [[0.; 4]; 2]);
    assert_eq!(f.n(&f.probe.processes), processes);
    assert_ne!(
        control.diagnostic(0).unwrap() & diagnostic::RESET_REFUSED,
        0
    );
    // Rebuild: drop graph (node returns), MAIN reclaim, fresh accepted bank.
    drop(graph);
    assert_eq!(control.retire().unwrap().released, 1);
    let mut builder = f.builder::<f32>(16);
    builder.node(instrument(None)).unwrap();
    let (control, mut nodes) = builder.build().unwrap();
    let mut node = nodes.pop().unwrap();
    node.prepare(ProcessConfig::new(48000., 16, 0, node.layout()).unwrap())
        .unwrap();
    let mut out = [[0f32; 4]; 2];
    let [l, r] = &mut out;
    assert_eq!(
        process(&mut node, &[], &mut [l, r], 4),
        ProcessStatus::Continue
    );
    assert_eq!(out[1], [0.2; 4]);
    drop(node);
    assert_eq!(control.retire().unwrap().released, 1);
    assert_eq!(f.n(&f.probe.closes), 2);
}

fn native_fault_then_node_drop_returns_bundle() {
    let f = fixture(mock::FAIL_PROCESS);
    let mut builder = f.builder::<f64>(8);
    builder.node(instrument(None)).unwrap();
    let (control, mut nodes) = builder.build().unwrap();
    let mut node = nodes.pop().unwrap();
    node.prepare(ProcessConfig::new(48000., 8, 0, node.layout()).unwrap())
        .unwrap();
    let mut out = [[3f64; 8]; 2];
    let [l, r] = &mut out;
    assert_eq!(
        process(&mut node, &[], &mut [l, r], 8),
        ProcessStatus::Continue
    );
    let [l, r] = &mut out;
    assert_eq!(
        process(&mut node, &[], &mut [l, r], 8),
        ProcessStatus::Fault(ProcessError::BackendFailure)
    );
    assert_eq!(out, [[0.; 8]; 2]);
    let [l, r] = &mut out;
    assert_eq!(
        process(&mut node, &[], &mut [l, r], 8),
        ProcessStatus::Fault(ProcessError::BackendFailure)
    );
    assert_eq!(
        f.n(&f.probe.processes),
        2,
        "faulted node makes no further native call"
    );
    assert_ne!(control.diagnostic(0).unwrap() & diagnostic::NATIVE_FAULT, 0);
    std::thread::spawn(move || drop(node)).join().unwrap();
    assert_eq!(control.retire().unwrap().released, 1);
    assert_eq!(f.n(&f.probe.closes), 1);
}

fn worker_panic_after_begin_returns_bundle() {
    let f = fixture(0);
    let mut builder = f.builder::<f32>(8);
    builder.node(instrument(None)).unwrap();
    let (control, mut nodes) = builder.build().unwrap();
    let mut node = nodes.pop().unwrap();
    node.prepare(ProcessConfig::new(48000., 8, 0, node.layout()).unwrap())
        .unwrap();
    let joined = std::thread::spawn(move || {
        let mut out = [[0f32; 8]; 2];
        let [l, r] = &mut out;
        assert_eq!(
            process(&mut node, &[], &mut [l, r], 8),
            ProcessStatus::Continue
        );
        panic!("worker failure after begin");
    })
    .join();
    assert!(joined.is_err());
    assert_eq!(control.poll_returns().unwrap().returned, 1);
    assert_eq!(control.retire().unwrap().released, 1);
    assert_eq!((f.n(&f.probe.ends), f.n(&f.probe.closes)), (1, 1));
}

fn panic_in_setup_before_begin_releases_reservation() {
    let f = fixture(0);
    let mut builder = f.builder::<f32>(8);
    builder.node(effect(None)).unwrap();
    builder
        .node_with_setup(instrument(None), |_| panic!("setup failure"))
        .unwrap();
    let result = std::panic::catch_unwind(AssertUnwindSafe(|| builder.build()));
    assert!(result.is_err());
    assert_eq!(live_banks(), 0);
    assert_eq!(f.n(&f.probe.begins), 0);
    assert_eq!((f.n(&f.probe.creates), f.n(&f.probe.destroys)), (2, 2));
    assert_eq!(f.n(&f.probe.closes), 1);
    // A refused setup is an ordinary error with the same complete unwind.
    let f = fixture(0);
    let mut builder = f.builder::<f32>(8);
    builder
        .node_with_setup(instrument(None), |child| {
            assert!(child.hosted_profile().is_none());
            Err(BankError::Shape)
        })
        .unwrap();
    assert_eq!(builder.build().err(), Some(BankError::Setup { child: 0 }));
    assert_eq!((f.n(&f.probe.destroys), f.n(&f.probe.closes)), (1, 1));
    assert_eq!(live_banks(), 0);
}

fn start_failure_unwinds_without_any_lease() {
    let f = fixture(mock::FAIL_START_SECOND);
    let mut builder = f.builder::<f32>(8);
    builder.node(instrument(None)).unwrap();
    builder.node(instrument(None)).unwrap();
    let error = builder.build().err().unwrap();
    assert_eq!(
        error,
        BankError::Instance {
            child: 1,
            error: InstanceError::Native {
                status: 4,
                result: 1
            }
        }
    );
    assert_eq!(f.n(&f.probe.begins), 0);
    assert!(f.position("start 0") < f.position("stop 0"));
    assert_eq!((f.n(&f.probe.destroys), f.n(&f.probe.closes)), (2, 1));
    assert_eq!(live_banks(), 0);
}

fn begin_failure_ends_earlier_leases_before_any_stop() {
    let f = fixture(mock::FAIL_BEGIN_SECOND);
    let mut builder = f.builder::<f64>(8);
    builder.node(effect(None)).unwrap();
    builder.node(instrument(None)).unwrap();
    let error = builder.build().err().unwrap();
    assert_eq!(
        error,
        BankError::Instance {
            child: 1,
            error: InstanceError::Native {
                status: 4,
                result: 2
            }
        }
    );
    let end = f.position("end 0");
    assert!(f.position("begin 0") < end);
    assert!(end < f.position("stop 1") && end < f.position("stop 0"));
    assert!(f.position("stop 0") < f.position("destroy 1"));
    assert_eq!((f.n(&f.probe.destroys), f.n(&f.probe.closes)), (2, 1));
    assert_eq!(f.n(&f.probe.control_during_audio), 0);
    assert_eq!(live_banks(), 0);
}

fn end_failure_retains_owners_until_successful_retry() {
    let f = fixture(0);
    let mut builder = f.builder::<f32>(8);
    builder.node(instrument(None)).unwrap();
    let (control, nodes) = builder.build().unwrap();
    let id = control.id();
    drop(nodes);
    f.probe.mode.store(mock::FAIL_END, Ordering::SeqCst);
    let report = control.retire().unwrap();
    assert_eq!((report.released, report.retained), (0, 1));
    assert_eq!(
        report.failure,
        Some((
            id,
            BankError::Instance {
                child: 0,
                error: InstanceError::Native {
                    status: 4,
                    result: 3
                }
            }
        ))
    );
    // Never guessed ended: no stop, destroy or module exit.
    assert_eq!(
        (
            f.n(&f.probe.ends),
            f.n(&f.probe.stops),
            f.n(&f.probe.destroys),
            f.n(&f.probe.closes)
        ),
        (0, 0, 0, 0)
    );
    f.probe.mode.store(0, Ordering::SeqCst);
    let report = reclaim_vst3_graph_banks(Some(id)).unwrap();
    assert_eq!(report.released, 1);
    assert_eq!(
        (
            f.n(&f.probe.ends),
            f.n(&f.probe.stops),
            f.n(&f.probe.destroys),
            f.n(&f.probe.closes)
        ),
        (1, 1, 1, 1)
    );
    assert_eq!(live_banks(), 0);
}

fn rich_lanes_round_trip_and_refuse_before_native_mutation() {
    let f = fixture(0);
    let lanes = LaneOptions {
        batches: 2,
        input_events: 4,
        input_payload: 64,
        input_points: 8,
        output_events: 4,
        output_payload: 64,
        output_queues: 4,
        output_points: 8,
    };
    let mut builder = f.builder::<f64>(16);
    builder.node(instrument(Some(lanes))).unwrap();
    let (control, mut nodes) = builder.build().unwrap();
    let mut node = nodes.pop().unwrap();
    node.prepare(ProcessConfig::new(48000., 16, 0, node.layout()).unwrap())
        .unwrap();
    let mut out = [[0f64; 16]; 2];
    let sequence = control
        .submit(0, 8, |input| {
            input.points.push(Point {
                id: 5,
                offset: 3,
                value: 0.5,
            })?;
            input.points.push(Point {
                id: 5,
                offset: 7,
                value: 1.,
            })?;
            let mut context = Context::new(48000., 96);
            context.state = Context::TEMPO;
            context.tempo = 90.;
            *input.context = Some(context);
            Ok(())
        })
        .unwrap();
    assert_eq!(sequence, 0);
    // Zero-frame complete empty batch is valid as the next sequence.
    assert_eq!(control.submit(0, 0, |_| Ok(())).unwrap(), 1);
    // Both batches are in flight: no free batch remains.
    assert_eq!(control.submit(0, 4, |_| Ok(())), Err(BankError::Empty));
    let [l, r] = &mut out;
    assert_eq!(
        process(&mut node, &[], &mut [l, r], 8),
        ProcessStatus::Continue
    );
    let [l, r] = &mut out;
    assert_eq!(
        process(&mut node, &[], &mut [l, r], 0),
        ProcessStatus::Continue
    );
    let first = control
        .receive(0, |batch| {
            let output = batch.output.unwrap();
            (
                batch.sequence,
                batch.frames,
                output.parameter_ids().to_vec(),
                output
                    .parameter_points()
                    .iter()
                    .map(|p| (p.id, p.offset, p.value))
                    .collect::<Vec<_>>(),
            )
        })
        .unwrap()
        .unwrap();
    assert_eq!(
        first,
        (
            0,
            8,
            vec![9999, 5],
            vec![(9999, 0, 0.09), (5, 3, 0.5), (5, 7, 1.)]
        )
    );
    let second = control
        .receive(0, |batch| {
            (
                batch.sequence,
                batch.output.unwrap().parameter_points().len(),
            )
        })
        .unwrap();
    assert_eq!(second, Some((1, 0)));
    assert_eq!(control.receive(0, |_| ()).unwrap(), None);
    // Frame mismatch refuses BEFORE native mutation and faults the node.
    assert_eq!(control.submit(0, 4, |_| Ok(())).unwrap(), 2);
    let processes = f.n(&f.probe.processes);
    out = [[7.; 16]; 2];
    let [l, r] = &mut out;
    assert_eq!(
        process(&mut node, &[], &mut [l, r], 8),
        ProcessStatus::Fault(ProcessError::NotPrepared)
    );
    assert_eq!(out[0][..8], [0.; 8]);
    assert_eq!(out[0][8..], [7.; 8]);
    assert_eq!(f.n(&f.probe.processes), processes);
    assert_ne!(control.diagnostic(0).unwrap() & diagnostic::LANE_FRAMES, 0);
    drop(node);
    assert_eq!(control.retire().unwrap().released, 1);

    // Missing batch in queued mode, and portable events refusal.
    let f = fixture(0);
    let mut builder = f.builder::<f32>(16);
    builder.node(instrument(Some(lanes))).unwrap();
    builder.node(instrument(None)).unwrap();
    let (control, mut nodes) = builder.build().unwrap();
    let mut plain = nodes.pop().unwrap();
    let mut queued = nodes.pop().unwrap();
    for node in [&mut plain, &mut queued] {
        node.prepare(ProcessConfig::new(48000., 16, 1, node.layout()).unwrap())
            .unwrap();
    }
    let mut out = [[0f32; 4]; 2];
    let [l, r] = &mut out;
    assert_eq!(
        process(&mut queued, &[], &mut [l, r], 4),
        ProcessStatus::Fault(ProcessError::NotPrepared)
    );
    assert_ne!(control.diagnostic(0).unwrap() & diagnostic::LANE_MISSING, 0);
    let [l, r] = &mut out;
    let mut outs = [&mut l[..], &mut r[..]];
    let audio = AudioBuffer::separate(&[], &mut outs, 4).unwrap();
    let events = [ravet_foundation::Event {
        frame_offset: 0,
        data: ravet_foundation::EventData::ParameterValue {
            id: ravet_foundation::ParameterId(1),
            value: 0.5,
        },
    }];
    assert_eq!(
        plain.process(&mut ProcessBlock::new(audio, &events, Transport::default())),
        ProcessStatus::Fault(ProcessError::Events(EventError::TooMany {
            count: 1,
            maximum: 0
        }))
    );
    assert_ne!(
        control.diagnostic(1).unwrap() & diagnostic::EVENTS_REFUSED,
        0
    );
    assert_eq!(f.n(&f.probe.processes), 0);
    drop((plain, queued));
    assert_eq!(control.retire().unwrap().released, 1);
    assert_eq!(live_banks(), 0);
}

fn lanes_and_audio_ten_thousand_cycles_allocate_nothing() {
    let f = fixture(0);
    let lanes = LaneOptions {
        batches: 1,
        input_events: 0,
        input_payload: 0,
        input_points: 2,
        output_events: 0,
        output_payload: 0,
        output_queues: 2,
        output_points: 4,
    };
    let mut builder = f.builder::<f64>(64);
    builder.node(effect(Some(lanes))).unwrap();
    let (control, mut nodes) = builder.build().unwrap();
    let mut node = nodes.pop().unwrap();
    node.prepare(ProcessConfig::new(48000., 64, 0, node.layout()).unwrap())
        .unwrap();
    let main = [[0.25f64; 64], [0.5; 64]];
    let aux = [1f64; 64];
    let mut out = [[0f64; 64]; 2];
    let measured = allocation_counter::measure(|| {
        for i in 0..10000u32 {
            let frames = i % 65;
            control
                .submit(0, frames, |input| {
                    if frames > 0 {
                        input.points.push(Point {
                            id: 1,
                            offset: frames - 1,
                            value: 0.75,
                        })?;
                    }
                    Ok(())
                })
                .unwrap();
            let [l, r] = &mut out;
            let status = process(
                &mut node,
                &[&main[0], &main[1], &aux],
                &mut [l, r],
                frames as usize,
            );
            assert_eq!(status, ProcessStatus::Continue);
            let points = control
                .receive(0, |batch| batch.output.unwrap().parameter_points().len())
                .unwrap();
            assert_eq!(points, Some(usize::from(frames > 0)));
        }
    });
    assert_eq!(measured.count_total, 0);
    assert_eq!(out[1][0], 0.5 * 1.5 + 0.25);
    drop(node);
    assert_eq!(control.retire().unwrap().released, 1);
    assert_eq!(f.n(&f.probe.processes), 10000);
}

fn forgotten_node_and_refused_return_retain_bank() {
    // Forgotten node: never proves termination; module stays retained.
    let f = fixture(0);
    let mut builder = f.builder::<f32>(8);
    builder.node(instrument(None)).unwrap();
    let (control, mut nodes) = builder.build().unwrap();
    std::mem::forget(nodes.pop().unwrap());
    let report = control.retire().unwrap();
    assert_eq!((report.released, report.retained), (0, 1));
    assert_eq!((f.n(&f.probe.ends), f.n(&f.probe.closes)), (0, 0));
    // Injected return-slot refusal: bundle retained, bank permanently stranded.
    let f = fixture(0);
    let mut builder = f.builder::<f64>(8);
    builder.node(instrument(None)).unwrap();
    let (control, mut nodes) = builder.build().unwrap();
    let mut node = nodes.pop().unwrap();
    assert!(node.returns.push(Box::new(())).is_ok());
    drop(node);
    assert_ne!(
        control.diagnostic(0).unwrap() & diagnostic::RETURN_REFUSED,
        0
    );
    assert_eq!(
        control.poll_returns().unwrap(),
        ReturnStatus {
            returned: 0,
            outstanding: 0,
            stranded: 1
        }
    );
    let report = control.retire().unwrap();
    assert_eq!(
        (report.released, report.retained, report.stranded),
        (0, 1, 1)
    );
    assert_eq!((f.n(&f.probe.ends), f.n(&f.probe.closes)), (0, 0));
    assert_eq!(live_banks(), 2);
}

fn child_and_bank_capacity_and_generation_exhaustion() {
    let f = fixture(0);
    let mut builder = f.builder::<f32>(8);
    for _ in 0..MAXIMUM_BANK_CHILDREN {
        builder.node(instrument(None)).unwrap();
    }
    assert_eq!(builder.node(instrument(None)), Err(BankError::Capacity));
    drop(builder);
    let retained = live_banks();
    let f = fixture(0);
    let mut banks = Vec::new();
    for _ in retained..MAXIMUM_BANKS {
        let mut builder = f.builder::<f32>(8);
        builder.node(instrument(None)).unwrap();
        banks.push(builder.build().unwrap());
    }
    let creates = f.n(&f.probe.creates);
    let mut builder = f.builder::<f32>(8);
    builder.node(instrument(None)).unwrap();
    assert_eq!(builder.build().err(), Some(BankError::Capacity));
    assert_eq!(
        f.n(&f.probe.creates),
        creates,
        "refused before any native create"
    );
    for (control, nodes) in banks {
        drop(nodes);
        assert_eq!(control.retire().unwrap().released, 1);
    }
    assert_eq!(live_banks(), retained);
    // Generations never wrap: the last representable increment refuses.
    with_registry(|r| r.next_generation = u64::MAX - 1).unwrap();
    let mut builder = f.builder::<f32>(8);
    builder.node(instrument(None)).unwrap();
    let (control, nodes) = builder.build().unwrap();
    assert_eq!(control.id().generation(), u64::MAX - 1);
    let mut builder = f.builder::<f32>(8);
    builder.node(instrument(None)).unwrap();
    assert_eq!(builder.build().err(), Some(BankError::Capacity));
    drop(nodes);
    assert_eq!(control.retire().unwrap().released, 1);
}
