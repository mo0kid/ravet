use ravet_foundation::{
    AudioBuffer, AudioLayout, Event, EventData, ParameterId, PrepareError, ProcessBlock,
    ProcessConfig, ProcessError, ProcessStatus, Processor, Tail, Transport,
};
use ravet_graph::{
    GraphStatus, MonoDuplicator, NodeId, SharedMonoKernel, TypedChain, TypedChainError,
};
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicUsize, Ordering},
};
#[derive(Default)]
struct Trace {
    events: Mutex<Vec<(usize, &'static str)>>,
    drops: AtomicUsize,
}
struct Stage {
    id: usize,
    trace: Arc<Trace>,
    config: Option<ProcessConfig>,
    gain: f64,
    calls: usize,
    frames: usize,
    events: usize,
    history: f64,
    fail_prepare: bool,
    fail_start: bool,
    fail_process: bool,
    panic_process: bool,
    panic_prepare: bool,
    panic_deactivate: bool,
    latency: u32,
}
impl Stage {
    fn new(id: usize, trace: &Arc<Trace>) -> Self {
        Self {
            id,
            trace: trace.clone(),
            config: None,
            gain: 2.,
            calls: 0,
            frames: 0,
            events: 0,
            history: 0.,
            fail_prepare: false,
            fail_start: false,
            fail_process: false,
            panic_process: false,
            panic_prepare: false,
            panic_deactivate: false,
            latency: 0,
        }
    }
    fn log(&self, x: &'static str) {
        self.trace.events.lock().unwrap().push((self.id, x));
    }
}
macro_rules! processor {
    ($s:ty) => {
        impl Processor<$s> for Stage {
            fn prepare(&mut self, c: ProcessConfig) -> Result<(), PrepareError> {
                self.log("prepare");
                if self.panic_prepare {
                    panic!("prepare");
                }
                if self.fail_prepare {
                    return Err(PrepareError::InvalidSampleRate);
                }
                self.config = Some(c);
                Ok(())
            }
            fn start_audio(&mut self) -> Result<(), ProcessError> {
                self.log("start");
                if self.fail_start {
                    Err(ProcessError::InvalidTransport)
                } else {
                    Ok(())
                }
            }
            fn stop_audio(&mut self) {
                self.log("stop");
            }
            fn process(&mut self, b: &mut ProcessBlock<'_, '_, '_, $s>) -> ProcessStatus {
                if let Err(error) = self
                    .config
                    .ok_or(ProcessError::NotPrepared)
                    .and_then(|c| c.validate_block(b))
                {
                    b.clear_outputs();
                    return ProcessStatus::Fault(error);
                }
                self.calls += 1;
                if self.panic_process {
                    panic!("process");
                }
                self.frames += b.audio.frames();
                self.events += b.events.len();
                for event in b.events {
                    if let EventData::ParameterValue {
                        id: ParameterId(1),
                        value,
                    } = event.data
                    {
                        self.gain = f64::from(value);
                    }
                }
                for c in 0..b.audio.output_channels() {
                    for f in 0..b.audio.frames() {
                        let x = b.audio.input(c).unwrap()[f];
                        self.history += x as f64;
                        b.audio.output_mut(c).unwrap()[f] = x * self.gain as $s;
                    }
                }
                if self.fail_process {
                    b.clear_outputs();
                    ProcessStatus::Fault(ProcessError::InvalidTransport)
                } else {
                    ProcessStatus::Continue
                }
            }
            fn reset(&mut self) {
                self.log("reset");
                self.history = 0.;
            }
            fn deactivate(&mut self) {
                if self.config.take().is_some() {
                    self.log("deactivate");
                    if self.panic_deactivate {
                        panic!("deactivate");
                    }
                }
            }
            fn latency_samples(&self) -> u32 {
                self.latency
            }
            fn tail(&self) -> Tail {
                Tail::Samples(17)
            }
        }
    };
}
processor!(f32);
processor!(f64);
impl Drop for Stage {
    fn drop(&mut self) {
        self.log("drop");
        self.trace.drops.fetch_add(1, Ordering::SeqCst);
    }
}
fn config(ch: usize, max: usize, events: usize) -> ProcessConfig {
    ProcessConfig::new(48000., max, events, AudioLayout::new(ch, ch).unwrap()).unwrap()
}
fn render32<T: ravet_graph::StageTuple<f32>>(
    chain: &mut TypedChain<f32, T>,
    data: &mut [f32],
    n: usize,
    events: &[Event],
) -> GraphStatus {
    let mut v = [data];
    let a = AudioBuffer::in_place(&mut v, n).unwrap();
    chain.process(&mut ProcessBlock::new(a, events, Transport::default()))
}
fn render_rows<T: ravet_graph::StageTuple<f32>>(
    chain: &mut TypedChain<f32, T>,
    rows: &mut [Vec<f32>],
    n: usize,
) -> GraphStatus {
    let mut v: Vec<&mut [f32]> = rows.iter_mut().map(Vec::as_mut_slice).collect();
    let a = AudioBuffer::in_place(&mut v, n).unwrap();
    chain.process(&mut ProcessBlock::new(a, &[], Transport::default()))
}
fn ids(trace: &Trace, what: &str) -> Vec<usize> {
    trace
        .events
        .lock()
        .unwrap()
        .iter()
        .filter(|x| x.1 == what)
        .map(|x| x.0)
        .collect()
}
fn count(trace: &Trace, what: &str) -> usize {
    ids(trace, what).len()
}
fn panic_message(payload: Box<dyn std::any::Any + Send>) -> String {
    match payload.downcast::<String>() {
        Ok(message) => *message,
        Err(payload) => payload
            .downcast_ref::<&str>()
            .map(|message| message.to_string())
            .unwrap_or_default(),
    }
}
type Eight = (Stage, Stage, Stage, Stage, Stage, Stage, Stage, Stage);
type StageTweak = (usize, fn(&mut Stage));
fn chain8(trace: &Arc<Trace>, tweak: Option<StageTweak>) -> TypedChain<f32, Eight> {
    let mut nodes: [Stage; 8] = std::array::from_fn(|i| Stage::new(i, trace));
    if let Some((index, tweak)) = tweak {
        tweak(&mut nodes[index]);
    }
    let [a, b, c, d, e, f, g, h] = nodes;
    TypedChain::<f32, _>::new((a, b, c, d, e, f, g, h), 1, 8, 0)
        .ok()
        .unwrap()
}
/// Two prepared stages whose audio owner was started through the manual hook.
fn manual_chain(trace: &Arc<Trace>) -> TypedChain<f32, (Stage, Stage)> {
    let mut chain =
        TypedChain::<f32, _>::new((Stage::new(0, trace), Stage::new(1, trace)), 1, 8, 0)
            .ok()
            .unwrap();
    chain.prepare(config(1, 8, 0)).unwrap();
    Processor::start_audio(&mut chain).unwrap();
    chain
}
#[test]
fn all_tuple_arities_and_all36_indices_exact_owners() {
    macro_rules! check {($($i:tt),+)=>{{let trace=Arc::new(Trace::default());let chain=TypedChain::<f64,_>::new(($(Stage::new($i,&trace),)+),1,64,0).ok().unwrap();$(assert_eq!(chain.stage::<$i>().id,$i);)+let owners=chain.into_stages();$(assert_eq!(owners.$i.id,$i);)+assert_eq!(trace.drops.load(Ordering::SeqCst),0);drop(owners);assert_eq!(trace.drops.load(Ordering::SeqCst),[$($i),+].len());}}}
    check!(0);
    check!(0, 1);
    check!(0, 1, 2);
    check!(0, 1, 2, 3);
    check!(0, 1, 2, 3, 4);
    check!(0, 1, 2, 3, 4, 5);
    check!(0, 1, 2, 3, 4, 5, 6);
    check!(0, 1, 2, 3, 4, 5, 6, 7);
}
#[test]
fn cell_getter_callback_exclusive_reprepare_extraction_sequence() {
    let trace = Arc::new(Trace::default());
    let mut chain =
        TypedChain::<f32, _>::new((Stage::new(7, &trace), Stage::new(9, &trace)), 1, 8, 2)
            .ok()
            .unwrap();
    let ptr = chain.stage::<0>() as *const Stage;
    assert_eq!(chain.stage::<0>().gain, 2.);
    chain.prepare(config(1, 8, 2)).unwrap();
    let mut data = [1., 2., 3., 99.];
    assert_eq!(
        render32(&mut chain, &mut data, 3, &[]),
        GraphStatus::Continue
    );
    assert_eq!(data, [4., 8., 12., 99.]);
    assert_eq!(chain.stage::<0>() as *const Stage, ptr);
    chain.stage_mut_stopped::<0>().unwrap().gain = 3.;
    let old = chain.stage::<0>().frames;
    assert_eq!(
        render32(&mut chain, &mut data, 3, &[]),
        GraphStatus::Fault {
            node: None,
            error: ProcessError::NotPrepared
        }
    );
    assert_eq!(chain.stage::<0>().frames, old);
    chain.prepare(config(1, 8, 2)).unwrap();
    data = [1., 2., 3., 99.];
    {
        let mut session = chain.start_audio().unwrap();
        assert_eq!(session.stage::<0>() as *const Stage, ptr);
        let mut v = [&mut data[..]];
        let a = AudioBuffer::in_place(&mut v, 3).unwrap();
        assert_eq!(
            session.process(&mut ProcessBlock::new(a, &[], Transport::default())),
            GraphStatus::Continue
        );
        session.set_bypassed::<1>(true);
        assert!(session.is_bypassed::<1>());
        session.finish();
    }
    assert_eq!(data, [6., 12., 18., 99.]);
    let owners = chain.into_stages();
    assert_eq!(owners.0.gain, 3.);
    assert_eq!(owners.0.frames, 6);
    assert_eq!(trace.drops.load(Ordering::SeqCst), 0);
    drop(owners);
    let log = trace.events.lock().unwrap();
    let first_drop = log.iter().position(|x| x.1 == "drop").unwrap();
    assert!(log[..first_drop].iter().any(|x| x.1 == "stop"));
    assert!(log[..first_drop].iter().any(|x| x.1 == "deactivate"));
}
#[test]
fn events_bypass_freeze_reset_suffix_empty_and_latency_not_compensated() {
    let trace = Arc::new(Trace::default());
    let mut first = Stage::new(0, &trace);
    first.latency = 5;
    let mut chain = TypedChain::<f32, _>::new((first, Stage::new(1, &trace)), 1, 8, 2)
        .ok()
        .unwrap();
    chain.prepare(config(1, 8, 2)).unwrap();
    assert_eq!(chain.latency_samples(), 5);
    assert_eq!(chain.tail(), Tail::Samples(34));
    let e = Event {
        frame_offset: 0,
        data: EventData::ParameterValue {
            id: ParameterId(1),
            value: 3.,
        },
    };
    let mut data = [1., 2., 88.];
    chain.set_bypassed::<0>(true);
    assert_eq!(
        render32(&mut chain, &mut data, 2, &[e]),
        GraphStatus::Continue
    );
    assert_eq!(data, [3., 6., 88.]);
    assert_eq!(chain.stage::<0>().history, 0.);
    assert_eq!(chain.stage::<0>().events, 0);
    assert_eq!(chain.stage::<1>().events, 1);
    chain.set_bypassed::<0>(false);
    let mut empty = [];
    assert_eq!(
        render32(&mut chain, &mut empty, 0, &[e]),
        GraphStatus::Continue
    );
    assert_eq!(chain.stage::<0>().gain, 3.);
    data = [1., 2., 88.];
    render32(&mut chain, &mut data, 2, &[]);
    assert_eq!(data, [9., 18., 88.]);
    chain.set_bypassed::<0>(true);
    chain.reset();
    assert_eq!(chain.stage::<0>().history, 0.);
    assert_eq!(chain.stage::<1>().history, 0.);
}
#[test]
fn canonical_child_fault_retention_outer_mapping_and_poison_no_reexecution() {
    let trace = Arc::new(Trace::default());
    let mut bad = Stage::new(1, &trace);
    bad.fail_process = true;
    let mut chain =
        TypedChain::<f64, _>::new((Stage::new(0, &trace), bad, Stage::new(2, &trace)), 1, 4, 0)
            .ok()
            .unwrap();
    chain.prepare(config(1, 4, 0)).unwrap();
    let mut data = [1., 2., 99.];
    let mut views = [&mut data[..]];
    let audio = AudioBuffer::in_place(&mut views, 2).unwrap();
    let mut block = ProcessBlock::new(audio, &[], Transport::default());
    assert_eq!(
        Processor::process(&mut chain, &mut block),
        ProcessStatus::Fault(ProcessError::BackendFailure)
    );
    assert_eq!(block.audio.output(0).unwrap(), [0., 0.]);
    let original = GraphStatus::Fault {
        node: Some(NodeId(1)),
        error: ProcessError::InvalidTransport,
    };
    assert_eq!(chain.first_fault(), Some(original));
    assert_eq!(chain.stage::<0>().frames, 2);
    assert_eq!(chain.stage::<1>().frames, 2);
    assert_eq!(chain.stage::<2>().frames, 0);
    assert_eq!(
        Processor::process(&mut chain, &mut block),
        ProcessStatus::Fault(ProcessError::BackendFailure)
    );
    assert_eq!(chain.stage::<0>().frames, 2);
    assert_eq!(chain.first_fault(), Some(original));
    chain.stage_mut_stopped::<1>().unwrap().fail_process = false;
    chain.prepare(config(1, 4, 0)).unwrap();
    chain.reset();
    assert_eq!(
        Processor::process(&mut chain, &mut block),
        ProcessStatus::Continue
    );
}
#[test]
fn outer_full_preflight_no_child_mutation_and_no_output_events_profile() {
    let trace = Arc::new(Trace::default());
    let mut chain = TypedChain::<f32, _>::new((Stage::new(0, &trace),), 1, 2, 1)
        .ok()
        .unwrap();
    chain.prepare(config(1, 2, 1)).unwrap();
    let mut data = [1., 2., 3., 99.];
    assert_eq!(
        render32(&mut chain, &mut data, 3, &[]),
        GraphStatus::Fault {
            node: None,
            error: ProcessError::TooManyFrames {
                frames: 3,
                maximum: 2
            }
        }
    );
    assert_eq!(data, [0., 0., 0., 99.]);
    assert_eq!(chain.stage::<0>().calls, 0);
    let late = Event {
        frame_offset: 2,
        data: EventData::ParameterValue {
            id: ParameterId(1),
            value: 5.,
        },
    };
    assert!(matches!(
        render32(&mut chain, &mut data, 2, &[late]),
        GraphStatus::Fault { node: None, .. }
    ));
    assert_eq!(chain.stage::<0>().gain, 2.);
    assert_eq!(chain.first_fault(), None);
    assert_eq!(
        chain.prepare(config(1, 2, 1).with_output_events(1)),
        Err(PrepareError::UnsupportedLayout)
    );
    assert_eq!(chain.stage::<0>().config, None);
    let aux = config(1, 2, 1)
        .with_auxiliary_buses(&[AudioLayout::MONO])
        .unwrap();
    assert_eq!(chain.prepare(aux), Err(PrepareError::UnsupportedLayout));
}
#[test]
fn prepare_failure_every_index_inactive_and_start_reverse_rollback() {
    for failed in 0..8 {
        let trace = Arc::new(Trace::default());
        let mut nodes: [Stage; 8] = std::array::from_fn(|i| Stage::new(i, &trace));
        nodes[failed].fail_prepare = true;
        let [a, b, c, d, e, f, g, h] = nodes;
        let mut chain = TypedChain::<f64, _>::new((a, b, c, d, e, f, g, h), 1, 8, 0)
            .ok()
            .unwrap();
        assert_eq!(
            chain.prepare(config(1, 8, 0)),
            Err(PrepareError::InvalidSampleRate)
        );
        assert!(chain.stage::<0>().config.is_none());
        assert!(chain.stage::<7>().config.is_none());
        let log = trace.events.lock().unwrap();
        assert_eq!(
            log.iter()
                .filter(|x| x.1 == "prepare")
                .map(|x| x.0)
                .collect::<Vec<_>>(),
            (0..=failed).collect::<Vec<_>>()
        );
        drop(log);
        drop(chain);
        assert_eq!(trace.drops.load(Ordering::SeqCst), 8);
    }
    for failed in 0..8 {
        let trace = Arc::new(Trace::default());
        let mut nodes: [Stage; 8] = std::array::from_fn(|i| Stage::new(i, &trace));
        nodes[failed].fail_start = true;
        let [a, b, c, d, e, f, g, h] = nodes;
        let mut chain = TypedChain::<f64, _>::new((a, b, c, d, e, f, g, h), 1, 8, 0)
            .ok()
            .unwrap();
        chain.prepare(config(1, 8, 0)).unwrap();
        assert!(chain.start_audio().is_err());
        let log = trace.events.lock().unwrap();
        assert_eq!(
            log.iter()
                .filter(|x| x.1 == "start")
                .map(|x| x.0)
                .collect::<Vec<_>>(),
            (0..=failed).collect::<Vec<_>>()
        );
        assert_eq!(
            log.iter()
                .filter(|x| x.1 == "stop")
                .map(|x| x.0)
                .collect::<Vec<_>>(),
            (0..failed).rev().collect::<Vec<_>>()
        );
    }
}
#[test]
fn constructor_preflight_exact_recovery_no_prepare() {
    for (ch, f, e, error) in [
        (0, 4, 0, TypedChainError::Layout),
        (65, 4, 0, TypedChainError::Layout),
        (1, 0, 0, TypedChainError::Frames),
        (1, 65537, 0, TypedChainError::Frames),
        (1, 4, 1025, TypedChainError::Events),
    ] {
        let trace = Arc::new(Trace::default());
        let result = TypedChain::<f64, _>::new((Stage::new(77, &trace),), ch, f, e)
            .err()
            .unwrap();
        assert_eq!(result.error, error);
        assert_eq!(result.stages.0.id, 77);
        assert!(trace.events.lock().unwrap().is_empty());
        assert_eq!(trace.drops.load(Ordering::SeqCst), 0);
        drop(result);
        assert_eq!(trace.drops.load(Ordering::SeqCst), 1);
    }
}
#[test]
fn unwind_prepare_process_and_session_drop_before_original_owners() {
    for preparation in [true, false] {
        let trace = Arc::new(Trace::default());
        let mut p = Stage::new(0, &trace);
        p.panic_prepare = preparation;
        p.panic_process = !preparation;
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let mut chain = TypedChain::<f32, _>::new((p, Stage::new(1, &trace)), 1, 4, 0)
                .ok()
                .unwrap();
            chain.prepare(config(1, 4, 0)).unwrap();
            let mut session = chain.start_audio().unwrap();
            let mut data = [1., 2.];
            let mut v = [&mut data[..]];
            let a = AudioBuffer::in_place(&mut v, 2).unwrap();
            session.process(&mut ProcessBlock::new(a, &[], Transport::default()));
        }));
        assert!(result.is_err());
        assert_eq!(trace.drops.load(Ordering::SeqCst), 2);
        let log = trace.events.lock().unwrap();
        if !preparation {
            let first_drop = log.iter().position(|x| x.1 == "drop").unwrap();
            assert_eq!(
                log[..first_drop]
                    .iter()
                    .filter(|x| x.1 == "stop")
                    .map(|x| x.0)
                    .collect::<Vec<_>>(),
                [1, 0]
            );
        }
    }
}
#[test]
fn graph_cell_wrapper_transfer_send_original_identity() {
    let trace = Arc::new(Trace::default());
    let chain = TypedChain::<f32, _>::new((Stage::new(19, &trace),), 1, 8, 0)
        .ok()
        .unwrap();
    let ptr = chain.stage::<0>() as *const Stage as usize;
    let returned = std::thread::spawn(move || {
        let mut chain = chain;
        chain.prepare(config(1, 8, 0)).unwrap();
        let mut x = [1., 2.];
        assert_eq!(render32(&mut chain, &mut x, 2, &[]), GraphStatus::Continue);
        assert_eq!(chain.stage::<0>() as *const Stage as usize, ptr);
        chain
    })
    .join()
    .unwrap();
    let owners = returned.into_stages();
    assert_eq!(owners.0.id, 19);
    assert_eq!(owners.0.frames, 2);
    assert_eq!(trace.drops.load(Ordering::SeqCst), 0);
}
#[test]
fn declared_cell_budget_recovers_original_tuple_without_allocation() {
    struct Huge([u8; 4 * 1024 * 1024]);
    impl Processor<f64> for Huge {
        fn prepare(&mut self, _: ProcessConfig) -> Result<(), PrepareError> {
            panic!("never prepared")
        }
        fn process(&mut self, _: &mut ProcessBlock<'_, '_, '_, f64>) -> ProcessStatus {
            panic!("never executed")
        }
        fn reset(&mut self) {}
        fn deactivate(&mut self) {}
    }
    // A caller-owned 4MiB value plus the bypass flag exceeds record admission.
    // Debug ownership-bearing Results create several large temporary copies.
    // CONTROL creates this caller-owned value on a sufficiently large
    // test stack; the refused constructor must not reserve any record/Graph.
    std::thread::Builder::new()
        .stack_size(512 * 1024 * 1024)
        .spawn(|| {
            let mut result = None;
            let measured = allocation_counter::measure(|| {
                result = Some(
                    TypedChain::<f64, _>::new((Huge([11; 4 * 1024 * 1024]),), 1, 8, 0)
                        .err()
                        .unwrap(),
                );
            });
            assert_eq!(measured.count_total, 0);
            let failure = result.unwrap();
            assert_eq!(failure.error, TypedChainError::Capacity);
            assert_eq!(failure.stages.0.0[0], 11);
        })
        .unwrap()
        .join()
        .unwrap();
}
#[test]
fn actual_gain_timed_events_heterogeneous_stage_bypass_and_stopped_rejection() {
    use ravet_gain::GainProcessor;
    let trace = Arc::new(Trace::default());
    let mut observer = Stage::new(2, &trace);
    observer.gain = 0.5;
    let mut chain = TypedChain::<f32, _>::new(
        (
            GainProcessor::new(0.).unwrap(),
            GainProcessor::new(0.).unwrap(),
            observer,
        ),
        1,
        8,
        2,
    )
    .ok()
    .unwrap();
    chain.prepare(config(1, 8, 2)).unwrap();
    let event = Event {
        frame_offset: 1,
        data: EventData::ParameterValue {
            id: ParameterId(1),
            value: 2.,
        },
    };
    // Both real gain nodes consume the event at frame1. The observer applies
    // its generic event before frame0, producing an exact factor2 throughout.
    let mut data = [1., 2., 3., 99.];
    assert_eq!(
        render32(&mut chain, &mut data, 3, &[event]),
        GraphStatus::Continue
    );
    // Observer applies its generic event before frame0, so factor2 throughout.
    assert_eq!(data, [2., 16., 24., 99.]);
    assert_eq!(chain.stage::<0>().requested_gain(), 2.);
    assert_eq!(chain.stage::<1>().requested_gain(), 2.);
    chain.set_bypassed::<0>(true);
    let event = Event {
        frame_offset: 0,
        data: EventData::ParameterValue {
            id: ParameterId(1),
            value: 3.,
        },
    };
    data = [1., 2., 3., 99.];
    render32(&mut chain, &mut data, 3, &[event]);
    assert_eq!(data, [9., 18., 27., 99.]);
    assert_eq!(chain.stage::<0>().requested_gain(), 2.);
    assert_eq!(chain.stage::<1>().requested_gain(), 3.);
    Processor::start_audio(&mut chain).unwrap();
    assert!(matches!(
        chain.stage_mut_stopped::<0>(),
        Err(PrepareError::AudioSessionActive)
    ));
    assert_eq!(
        chain.prepare(config(1, 8, 2)),
        Err(PrepareError::AudioSessionActive)
    );
    Processor::stop_audio(&mut chain);
    chain.stage_mut_stopped::<2>().unwrap().gain = 1.;
    chain.prepare(config(1, 8, 2)).unwrap();
}
#[test]
fn foreign_deactivate_panic_never_manufactures_extraction_and_drops_each_owner_once() {
    for extraction in [false, true] {
        let trace = Arc::new(Trace::default());
        let mut p = Stage::new(0, &trace);
        p.panic_deactivate = true;
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let mut chain = TypedChain::<f64, _>::new((p, Stage::new(1, &trace)), 1, 8, 0)
                .ok()
                .unwrap();
            chain.prepare(config(1, 8, 0)).unwrap();
            if extraction {
                let _ = chain.into_stages();
            } else {
                drop(chain);
            }
        }));
        assert!(result.is_err());
        assert_eq!(trace.drops.load(Ordering::SeqCst), 2);
        let log = trace.events.lock().unwrap();
        assert_eq!(
            log.iter()
                .filter(|x| x.1 == "drop")
                .map(|x| x.0)
                .collect::<Vec<_>>(),
            [0, 1]
        );
    }
}
#[test]
fn actual_two_sample_latency_bypass_preserves_report_and_freezes_delayed_tail() {
    struct Delay {
        state: [f32; 2],
        cursor: usize,
        c: Option<ProcessConfig>,
    }
    impl Processor for Delay {
        fn prepare(&mut self, c: ProcessConfig) -> Result<(), PrepareError> {
            self.c = Some(c);
            self.reset();
            Ok(())
        }
        fn process(&mut self, b: &mut ProcessBlock<'_, '_, '_>) -> ProcessStatus {
            self.c.unwrap().validate_block(b).unwrap();
            for f in 0..b.audio.frames() {
                let input = b.audio.input(0).unwrap()[f];
                let y = self.state[self.cursor];
                self.state[self.cursor] = input;
                self.cursor = (self.cursor + 1) % 2;
                b.audio.output_mut(0).unwrap()[f] = y;
            }
            ProcessStatus::Continue
        }
        fn reset(&mut self) {
            self.state = [0.; 2];
            self.cursor = 0;
        }
        fn deactivate(&mut self) {
            self.c = None;
        }
        fn latency_samples(&self) -> u32 {
            2
        }
        fn tail(&self) -> Tail {
            Tail::Samples(2)
        }
    }
    let trace = Arc::new(Trace::default());
    let mut chain = TypedChain::<f32, _>::new(
        (
            Delay {
                state: [0.; 2],
                cursor: 0,
                c: None,
            },
            Stage::new(1, &trace),
        ),
        1,
        8,
        0,
    )
    .ok()
    .unwrap();
    chain.prepare(config(1, 8, 0)).unwrap();
    let mut x = [1., 0., 0., 0., 99.];
    assert_eq!(render32(&mut chain, &mut x, 4, &[]), GraphStatus::Continue);
    assert_eq!(x, [0., 0., 2., 0., 99.]);
    assert_eq!(chain.latency_samples(), 2);
    chain.reset();
    chain.set_bypassed::<0>(true);
    x = [1., 0., 0., 0., 99.];
    render32(&mut chain, &mut x, 4, &[]);
    assert_eq!(x, [2., 0., 0., 0., 99.]);
    assert_eq!(chain.latency_samples(), 2);
    assert_eq!(chain.tail(), Tail::Samples(19));
    assert_eq!(chain.stage::<0>().state, [0.; 2]);
    chain.set_bypassed::<0>(false);
    let mut x = [1., 0., 99.];
    render32(&mut chain, &mut x, 1, &[]);
    assert_eq!(chain.stage::<0>().state, [1., 0.]);
    chain.set_bypassed::<0>(true);
    x = [5., 0., 99.];
    render32(&mut chain, &mut x, 1, &[]);
    assert_eq!(x[0], 10.);
    assert_eq!(chain.stage::<0>().state, [1., 0.]);
    chain.set_bypassed::<0>(false);
    x = [0., 0., 99.];
    render32(&mut chain, &mut x, 2, &[]);
    assert_eq!(x, [0., 2., 99.]);
}
// ---------------------------------------------------------------------------
// Manual `Processor` audio hooks: a caller obligation. Every panic below is a
// single, non-nested panic caught by the test; none is raised while already
// unwinding, so no test deliberately aborts the process.
// ---------------------------------------------------------------------------
const ASSERTED: &str = "finish graph audio session before deactivation";
#[test]
fn manual_owner_explicit_deactivate_panics_unchanged_then_owner_thread_stop_recovers() {
    let trace = Arc::new(Trace::default());
    let mut chain = manual_chain(&trace);
    // Both the inherent and the Processor deactivate reach the canonical assertion.
    let inherent = catch_unwind(AssertUnwindSafe(|| chain.deactivate()));
    let message = panic_message(inherent.unwrap_err());
    assert!(message.contains(ASSERTED), "unexpected panic: {message}");
    let via_trait = catch_unwind(AssertUnwindSafe(|| Processor::deactivate(&mut chain)));
    assert!(panic_message(via_trait.unwrap_err()).contains(ASSERTED));
    // The assertion precedes every state change: nothing stopped, deactivated or dropped.
    assert_eq!(count(&trace, "stop"), 0);
    assert_eq!(count(&trace, "deactivate"), 0);
    assert_eq!(trace.drops.load(Ordering::SeqCst), 0);
    assert!(chain.stage::<0>().config.is_some());
    assert!(chain.stage::<1>().config.is_some());
    // prepare and the stopped getter refuse instead of panicking while the owner is live.
    assert_eq!(
        chain.prepare(config(1, 8, 0)),
        Err(PrepareError::AudioSessionActive)
    );
    assert!(matches!(
        chain.stage_mut_stopped::<1>(),
        Err(PrepareError::AudioSessionActive)
    ));
    // The chain is still prepared and renders.
    let mut data = [1., 2., 99.];
    assert_eq!(
        render32(&mut chain, &mut data, 2, &[]),
        GraphStatus::Continue
    );
    assert_eq!(data, [4., 8., 99.]);
    // Owner-thread stop recovers; a second stop is a no-op.
    Processor::stop_audio(&mut chain);
    assert_eq!(ids(&trace, "stop"), [1, 0]);
    Processor::stop_audio(&mut chain);
    assert_eq!(count(&trace, "stop"), 2);
    // Only now is deactivation, mutation and extraction legal.
    chain.deactivate();
    assert_eq!(count(&trace, "deactivate"), 2);
    chain.stage_mut_stopped::<0>().unwrap().gain = 3.;
    let owners = chain.into_stages();
    assert_eq!(owners.0.gain, 3.);
    assert_eq!(trace.drops.load(Ordering::SeqCst), 0);
    drop(owners);
    assert_eq!(trace.drops.load(Ordering::SeqCst), 2);
}
#[test]
fn manual_owner_drop_and_extraction_panic_and_each_stage_drops_exactly_once_unstopped() {
    for extraction in [false, true] {
        let trace = Arc::new(Trace::default());
        let result = catch_unwind(AssertUnwindSafe(|| {
            let chain = manual_chain(&trace);
            if extraction {
                let _ = chain.into_stages();
            } else {
                drop(chain);
            }
        }));
        let message = panic_message(result.unwrap_err());
        assert!(message.contains(ASSERTED), "unexpected panic: {message}");
        // Unwinding destroyed both stages once, in field order, with no stop and no
        // deactivate: the caller's missing stop_audio is not repaired by the facade.
        assert_eq!(trace.drops.load(Ordering::SeqCst), 2);
        assert_eq!(ids(&trace, "drop"), [0, 1]);
        assert_eq!(ids(&trace, "start"), [0, 1]);
        assert_eq!(count(&trace, "stop"), 0);
        assert_eq!(count(&trace, "deactivate"), 0);
    }
}
#[test]
fn manual_owner_stop_on_another_thread_panics_and_the_owner_thread_still_stops() {
    let trace = Arc::new(Trace::default());
    let chain = manual_chain(&trace);
    // Transferring a started chain is the caller's violation; the first check that
    // notices is the owner-thread assertion in stop.
    let (mut chain, message) = std::thread::spawn(move || {
        let mut chain = chain;
        let result = catch_unwind(AssertUnwindSafe(|| Processor::stop_audio(&mut chain)));
        (chain, panic_message(result.unwrap_err()))
    })
    .join()
    .unwrap();
    assert!(
        message.contains("stop graph on its audio owner"),
        "unexpected panic: {message}"
    );
    assert_eq!(count(&trace, "stop"), 0);
    // The failed stop changed nothing: the owner is still live on the original thread.
    assert_eq!(
        chain.prepare(config(1, 8, 0)),
        Err(PrepareError::AudioSessionActive)
    );
    Processor::stop_audio(&mut chain);
    assert_eq!(ids(&trace, "stop"), [1, 0]);
    chain.prepare(config(1, 8, 0)).unwrap();
}
#[test]
fn forgotten_guard_is_misuse_that_leaves_the_owner_live_but_a_finished_guard_is_valid_use() {
    let trace = Arc::new(Trace::default());
    let mut chain =
        TypedChain::<f32, _>::new((Stage::new(0, &trace), Stage::new(1, &trace)), 1, 8, 0)
            .ok()
            .unwrap();
    chain.prepare(config(1, 8, 0)).unwrap();
    // Valid use: finishing the guard stops every stage at once, in reverse order.
    chain.start_audio().unwrap().finish();
    assert_eq!(ids(&trace, "stop"), [1, 0]);
    assert!(chain.stage_mut_stopped::<0>().is_ok());
    chain.prepare(config(1, 8, 0)).unwrap();
    // Misuse: a leaked guard never stops anything and the owner stays live.
    std::mem::forget(chain.start_audio().unwrap());
    assert_eq!(count(&trace, "stop"), 2);
    assert_eq!(
        chain.prepare(config(1, 8, 0)),
        Err(PrepareError::AudioSessionActive)
    );
    assert!(matches!(
        chain.stage_mut_stopped::<0>(),
        Err(PrepareError::AudioSessionActive)
    ));
    // Dropping or extracting now would panic exactly like an unpaired manual start.
    // This test restores its own state with the manual hook on the owner thread; that is
    // not a supported recovery pattern for a leaked guard.
    Processor::stop_audio(&mut chain);
    assert_eq!(ids(&trace, "stop"), [1, 0, 1, 0]);
    chain.prepare(config(1, 8, 0)).unwrap();
}
#[test]
fn start_failure_payload_first_fault_and_poisoned_retry_mapping_at_every_index() {
    let original = |failed: usize| GraphStatus::Fault {
        node: Some(NodeId(failed as u32)),
        error: ProcessError::InvalidTransport,
    };
    for via_trait in [false, true] {
        for failed in 0..8 {
            let trace = Arc::new(Trace::default());
            let mut chain = chain8(&trace, Some((failed, |s| s.fail_start = true)));
            chain.prepare(config(1, 8, 0)).unwrap();
            if via_trait {
                assert_eq!(
                    Processor::start_audio(&mut chain),
                    Err(ProcessError::InvalidTransport)
                );
            } else {
                assert_eq!(chain.start_audio().err(), Some(original(failed)));
            }
            // The first executed fault is retained and the started prefix rolled back in reverse.
            assert_eq!(chain.first_fault(), Some(original(failed)));
            assert_eq!(ids(&trace, "stop"), (0..failed).rev().collect::<Vec<_>>());
            // A retry on the faulted graph is `NotPrepared` without a node and does not
            // replace the retained first fault.
            if via_trait {
                assert_eq!(
                    Processor::start_audio(&mut chain),
                    Err(ProcessError::NotPrepared)
                );
            } else {
                assert_eq!(
                    chain.start_audio().err(),
                    Some(GraphStatus::Fault {
                        node: None,
                        error: ProcessError::NotPrepared
                    })
                );
            }
            assert_eq!(chain.first_fault(), Some(original(failed)));
            assert_eq!(ids(&trace, "start"), (0..=failed).collect::<Vec<_>>());
            // Rendering is poisoned too: the Processor facade maps it to BackendFailure.
            let mut data = [1., 2., 99.];
            let mut v = [&mut data[..]];
            let audio = AudioBuffer::in_place(&mut v, 2).unwrap();
            let mut block = ProcessBlock::new(audio, &[], Transport::default());
            assert_eq!(
                Processor::process(&mut chain, &mut block),
                ProcessStatus::Fault(ProcessError::BackendFailure)
            );
            assert_eq!(data, [0., 0., 99.]);
            assert_eq!(chain.stage::<0>().calls, 0);
            // Reset clears the observation and re-enables the graph.
            chain.reset();
            assert_eq!(chain.first_fault(), None);
        }
    }
}
#[test]
fn start_failure_recovers_through_stopped_getter_reprepare_and_clears_observation() {
    let trace = Arc::new(Trace::default());
    let mut chain = chain8(&trace, Some((3, |s| s.fail_start = true)));
    chain.prepare(config(1, 8, 0)).unwrap();
    assert_eq!(
        Processor::start_audio(&mut chain),
        Err(ProcessError::InvalidTransport)
    );
    assert!(chain.first_fault().is_some());
    // The failed start left no live owner, so the stopped getter is available.
    chain.stage_mut_stopped::<3>().unwrap().fail_start = false;
    assert_eq!(chain.first_fault(), None);
    chain.prepare(config(1, 8, 0)).unwrap();
    Processor::start_audio(&mut chain).unwrap();
    Processor::stop_audio(&mut chain);
    let mut data = [1., 2., 99.];
    assert_eq!(
        render32(&mut chain, &mut data, 2, &[]),
        GraphStatus::Continue
    );
    assert_eq!(data, [256., 512., 99.]);
}
#[test]
fn prepare_failure_deactivates_exactly_the_earlier_prepared_stages_and_leaves_all_inactive() {
    for failed in 0..8 {
        let trace = Arc::new(Trace::default());
        let mut chain = chain8(&trace, Some((failed, |s| s.fail_prepare = true)));
        assert_eq!(
            chain.prepare(config(1, 8, 0)),
            Err(PrepareError::InvalidSampleRate)
        );
        let mut data = [1., 2., 99.];
        assert_eq!(
            render32(&mut chain, &mut data, 2, &[]),
            GraphStatus::Fault {
                node: None,
                error: ProcessError::NotPrepared
            }
        );
        let owners = chain.into_stages();
        for stage in [
            &owners.0, &owners.1, &owners.2, &owners.3, &owners.4, &owners.5, &owners.6, &owners.7,
        ] {
            assert!(stage.config.is_none());
        }
        drop(owners);
        // Stages after the failure were never prepared; each earlier one was deactivated
        // exactly once by the failed prepare, and the later teardown finds nothing active.
        assert_eq!(ids(&trace, "prepare"), (0..=failed).collect::<Vec<_>>());
        let mut deactivated = ids(&trace, "deactivate");
        deactivated.sort_unstable();
        assert_eq!(deactivated, (0..failed).collect::<Vec<_>>());
        assert_eq!(trace.drops.load(Ordering::SeqCst), 8);
    }
}
#[test]
fn normal_session_finish_drop_and_manual_stop_stop_eight_stages_in_reverse_once() {
    let trace = Arc::new(Trace::default());
    let mut chain = chain8(&trace, None);
    chain.prepare(config(1, 8, 0)).unwrap();
    let forward: Vec<usize> = (0..8).collect();
    let reverse: Vec<usize> = (0..8).rev().collect();
    chain.start_audio().unwrap().finish();
    assert_eq!(ids(&trace, "start"), forward);
    assert_eq!(ids(&trace, "stop"), reverse);
    {
        let _session = chain.start_audio().unwrap();
        assert_eq!(count(&trace, "stop"), 8);
    }
    assert_eq!(ids(&trace, "stop"), reverse.repeat(2));
    Processor::start_audio(&mut chain).unwrap();
    Processor::stop_audio(&mut chain);
    Processor::stop_audio(&mut chain);
    assert_eq!(ids(&trace, "start"), forward.repeat(3));
    assert_eq!(ids(&trace, "stop"), reverse.repeat(3));
    // Stopping never deactivated anything; deactivation then reaches each stage once.
    assert_eq!(count(&trace, "deactivate"), 0);
    chain.deactivate();
    let mut deactivated = ids(&trace, "deactivate");
    deactivated.sort_unstable();
    assert_eq!(deactivated, forward);
}
struct Ctl {
    gain: f32,
}
struct SharedKernel {
    state: f32,
    active: bool,
    seen: usize,
    calls: usize,
}
impl SharedMonoKernel<f32, Ctl> for SharedKernel {
    fn prepare(&mut self, _: f64, _: usize, c: &Ctl) -> Result<(), PrepareError> {
        self.active = true;
        self.seen = c as *const Ctl as usize;
        Ok(())
    }
    fn process_sample(&mut self, x: f32, c: &Ctl) -> f32 {
        assert!(self.active);
        assert_eq!(self.seen, c as *const Ctl as usize);
        self.calls += 1;
        let y = x * c.gain + self.state;
        self.state = x;
        y
    }
    fn reset(&mut self, _: &Ctl) {
        self.state = 0.;
    }
    fn deactivate(&mut self) {
        self.active = false;
    }
}
#[test]
fn shared_control_mutation_through_typed_stopped_getter_requires_reprepare_on_one_object() {
    let mono = MonoDuplicator::<f32, _, _>::new(Ctl { gain: 2. }, 2, 8, |_, _| {
        Ok::<_, ()>(SharedKernel {
            state: 0.,
            active: false,
            seen: 0,
            calls: 0,
        })
    })
    .ok()
    .unwrap();
    let trace = Arc::new(Trace::default());
    let mut pass_through = Stage::new(1, &trace);
    pass_through.gain = 1.;
    let mut chain = TypedChain::<f32, _>::new((mono, pass_through), 2, 8, 0)
        .ok()
        .unwrap();
    chain.prepare(config(2, 8, 0)).unwrap();
    let address = chain.stage::<0>().shared_controls() as *const Ctl as usize;
    // y = gain * x + previous x, per channel.
    let mut rows = vec![vec![1., 2., 99.], vec![10., 20., 99.]];
    assert_eq!(render_rows(&mut chain, &mut rows, 2), GraphStatus::Continue);
    assert_eq!(rows, vec![vec![2., 5., 99.], vec![20., 50., 99.]]);
    // Mutating the shared controls through the typed stopped getter invalidates readiness.
    chain
        .stage_mut_stopped::<0>()
        .unwrap()
        .shared_controls_mut_stopped()
        .gain = 3.;
    assert_eq!(chain.stage::<0>().shared_controls().gain, 3.);
    assert_eq!(
        chain.stage::<0>().shared_controls() as *const Ctl as usize,
        address
    );
    let mut stale = vec![vec![1., 99.], vec![1., 99.]];
    assert_eq!(
        render_rows(&mut chain, &mut stale, 1),
        GraphStatus::Fault {
            node: None,
            error: ProcessError::NotPrepared
        }
    );
    assert_eq!(stale, vec![vec![0., 99.], vec![0., 99.]]);
    assert!(!chain.stage::<0>().kernel(0).unwrap().active);
    assert_eq!(chain.stage::<0>().kernel(0).unwrap().calls, 2);
    // Reprepare: both kernels see the same, updated control object, histories continue.
    chain.prepare(config(2, 8, 0)).unwrap();
    for channel in 0..2 {
        let kernel = chain.stage::<0>().kernel(channel).unwrap();
        assert!(kernel.active);
        assert_eq!(kernel.seen, address);
    }
    let mut rows = vec![vec![3., 99.], vec![30., 99.]];
    assert_eq!(render_rows(&mut chain, &mut rows, 1), GraphStatus::Continue);
    assert_eq!(rows, vec![vec![11., 99.], vec![110., 99.]]);
}
#[test]
fn typed_chain_admits_and_renders_up_to_sixty_four_channels_with_per_channel_signals() {
    for channels in [1usize, 2, 7, 33, 64] {
        let trace = Arc::new(Trace::default());
        let mut chain = TypedChain::<f32, _>::new(
            (Stage::new(0, &trace), Stage::new(1, &trace)),
            channels,
            8,
            0,
        )
        .ok()
        .unwrap();
        chain.prepare(config(channels, 8, 0)).unwrap();
        let mut rows: Vec<Vec<f32>> = (0..channels).map(|c| vec![c as f32 + 1., 99.]).collect();
        assert_eq!(render_rows(&mut chain, &mut rows, 1), GraphStatus::Continue);
        for (c, row) in rows.iter().enumerate() {
            assert_eq!(row, &vec![4. * (c as f32 + 1.), 99.]);
        }
        // Every channel of every stage was seen once: history is the channel-value sum.
        let sum: f64 = (1..=channels).map(|c| c as f64).sum();
        assert_eq!(chain.stage::<0>().history, sum);
        assert_eq!(chain.stage::<1>().history, 2. * sum);
    }
}
