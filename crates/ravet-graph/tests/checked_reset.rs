use ravet_foundation::{
    AudioBuffer, AudioLayout, PrepareError, ProcessBlock, ProcessConfig, ProcessError,
    ProcessStatus, Processor, Transport,
};
use ravet_graph::{
    Connection, Destination, Graph, GraphNode, GraphStatus, NodeId, Source, TypedChain,
};
use std::sync::{
    Arc,
    atomic::{AtomicBool, AtomicUsize, Ordering},
};
struct Probe {
    refuse: Arc<AtomicBool>,
    attempts: Arc<AtomicUsize>,
    legacy: Arc<AtomicUsize>,
    calls: Arc<AtomicUsize>,
}
impl Probe {
    fn new(refuse: bool) -> Self {
        Self {
            refuse: Arc::new(AtomicBool::new(refuse)),
            attempts: Arc::new(AtomicUsize::new(0)),
            legacy: Arc::new(AtomicUsize::new(0)),
            calls: Arc::new(AtomicUsize::new(0)),
        }
    }
}
macro_rules! processor {
    ($s:ty) => {
        impl Processor<$s> for Probe {
            fn prepare(&mut self, _: ProcessConfig) -> Result<(), PrepareError> {
                Ok(())
            }
            fn process(&mut self, b: &mut ProcessBlock<'_, '_, '_, $s>) -> ProcessStatus {
                self.calls.fetch_add(1, Ordering::Relaxed);
                b.audio.output_mut(0).unwrap().fill(2.);
                ProcessStatus::Continue
            }
            fn reset(&mut self) {
                self.legacy.fetch_add(1, Ordering::Relaxed);
            }
            fn try_reset(&mut self) -> Result<(), ProcessError> {
                self.attempts.fetch_add(1, Ordering::Relaxed);
                if self.refuse.load(Ordering::Relaxed) {
                    Err(ProcessError::RebuildRequired)
                } else {
                    Ok(())
                }
            }
            fn deactivate(&mut self) {}
        }
    };
}
processor!(f32);
processor!(f64);
fn config() -> ProcessConfig {
    ProcessConfig::new(48000., 8, 0, AudioLayout::MONO).unwrap()
}
fn render(g: &mut Graph, out: &mut [f32; 3]) -> GraphStatus {
    let mut views = [&mut out[..]];
    let audio = AudioBuffer::in_place(&mut views, 2).unwrap();
    g.process(&mut ProcessBlock::new(audio, &[], Transport::default()))
}
#[test]
fn execution_order_refusal_preserves_fault_no_legacy_mask_and_success_recovers() {
    let a = Probe::new(false);
    let a_reset = a.attempts.clone();
    let b = Probe::new(true);
    let b_reset = b.attempts.clone();
    let b_legacy = b.legacy.clone();
    let b_refuse = b.refuse.clone();
    let calls = b.calls.clone();
    let c = Probe::new(false);
    let c_reset = c.attempts.clone();
    let routes = [
        Connection {
            source: Source::Node {
                id: NodeId(1),
                channel: 0,
            },
            destination: Destination::Node {
                id: NodeId(2),
                channel: 0,
            },
            gain: 1.,
        },
        Connection {
            source: Source::Node {
                id: NodeId(2),
                channel: 0,
            },
            destination: Destination::Node {
                id: NodeId(3),
                channel: 0,
            },
            gain: 1.,
        },
    ];
    let mut g = Graph::<f32>::new(
        AudioLayout::MONO,
        vec![
            GraphNode {
                id: NodeId(3),
                layout: AudioLayout::MONO,
                processor: Box::new(c),
            },
            GraphNode {
                id: NodeId(2),
                layout: AudioLayout::MONO,
                processor: Box::new(b),
            },
            GraphNode {
                id: NodeId(1),
                layout: AudioLayout::MONO,
                processor: Box::new(a),
            },
        ],
        &routes,
    )
    .unwrap();
    g.prepare(config()).unwrap();
    assert_eq!(
        g.try_reset(),
        Err(GraphStatus::Fault {
            node: Some(NodeId(2)),
            error: ProcessError::RebuildRequired
        })
    );
    assert_eq!(
        (
            a_reset.load(Ordering::Relaxed),
            b_reset.load(Ordering::Relaxed),
            c_reset.load(Ordering::Relaxed),
            b_legacy.load(Ordering::Relaxed)
        ),
        (1, 1, 0, 0)
    );
    g.reset(); // void compatibility must preserve the refusal too
    let mut output = [9., 9., 73.];
    assert!(matches!(
        render(&mut g, &mut output),
        GraphStatus::Fault { .. }
    ));
    assert_eq!(output, [0., 0., 73.]);
    assert_eq!(calls.load(Ordering::Relaxed), 0);
    b_refuse.store(false, Ordering::Relaxed);
    assert_eq!(g.try_reset(), Ok(()));
    assert_eq!(render(&mut g, &mut output), GraphStatus::Continue);
    assert_eq!(calls.load(Ordering::Relaxed), 1);
}
#[test]
fn session_refusal_and_default_successful_reset_remain_distinct() {
    let p = Probe::new(true);
    let mut g = Graph::<f32>::new(
        AudioLayout::MONO,
        vec![GraphNode {
            id: NodeId(8),
            layout: AudioLayout::MONO,
            processor: Box::new(p),
        }],
        &[],
    )
    .unwrap();
    g.prepare(config()).unwrap();
    let mut session = g.start_audio().unwrap();
    assert_eq!(
        session.try_reset(),
        Err(GraphStatus::Fault {
            node: Some(NodeId(8)),
            error: ProcessError::RebuildRequired
        })
    );
    session.reset();
    session.finish();
    struct Legacy(Arc<AtomicUsize>);
    impl Processor for Legacy {
        fn prepare(&mut self, _: ProcessConfig) -> Result<(), PrepareError> {
            Ok(())
        }
        fn reset(&mut self) {
            self.0.fetch_add(1, Ordering::Relaxed);
        }
        fn deactivate(&mut self) {}
        fn process(&mut self, b: &mut ProcessBlock<'_, '_, '_>) -> ProcessStatus {
            b.clear_outputs();
            ProcessStatus::Continue
        }
    }
    let count = Arc::new(AtomicUsize::new(0));
    let mut p = Legacy(count.clone());
    assert_eq!(p.try_reset(), Ok(()));
    assert_eq!(count.load(Ordering::Relaxed), 1);
}
macro_rules! typed {
    ($name:ident,$s:ty) => {
        #[test]
        fn $name() {
            let p = Probe::new(true);
            let legacy = p.legacy.clone();
            let mut chain = TypedChain::<$s, _>::new((p,), 1, 8, 0)
                .unwrap_or_else(|_| panic!("admitted tuple"));
            chain.prepare(config()).unwrap();
            assert_eq!(
                Processor::try_reset(&mut chain),
                Err(ProcessError::RebuildRequired)
            );
            assert_eq!(
                chain.first_fault(),
                Some(GraphStatus::Fault {
                    node: Some(NodeId(0)),
                    error: ProcessError::RebuildRequired
                })
            );
            chain.reset();
            assert_eq!(legacy.load(Ordering::Relaxed), 0);
        }
    };
}
typed!(typed_wrapper_f32_preserves_checked_refusal, f32);
typed!(typed_wrapper_f64_preserves_checked_refusal, f64);
#[test]
fn checked_reset_success_and_refusal_do_not_allocate_or_free() {
    let p = Probe::new(true);
    let refuse = p.refuse.clone();
    let mut chain = TypedChain::<f32, _>::new((p,), 1, 8, 0).unwrap_or_else(|_| panic!("tuple"));
    chain.prepare(config()).unwrap();
    let info = allocation_counter::measure(|| {
        for n in 0..10000 {
            refuse.store(n % 2 == 0, Ordering::Relaxed);
            let result = Processor::try_reset(&mut chain);
            assert_eq!(result.is_err(), n % 2 == 0);
        }
    });
    assert_eq!(info.count_total, 0);
    assert_eq!(info.count_current, 0);
}
