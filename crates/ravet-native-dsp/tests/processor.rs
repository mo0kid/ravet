#![cfg(feature = "processor")]
use ravet_foundation::{
    AudioBuffer, AudioLayout, PrepareError, ProcessBlock, ProcessConfig, ProcessError,
    ProcessStatus, Processor, Transport,
};
use ravet_native_dsp::{DenormalMode, DenormalProcessor, current_float_state};
use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};
struct Child {
    id: usize,
    prepares: usize,
    calls: usize,
    committed: usize,
    resets: usize,
    starts: usize,
    stops: usize,
    deactivations: usize,
    action: Arc<AtomicUsize>,
    session: bool,
}
impl Processor<f64> for Child {
    fn prepare(&mut self, _: ProcessConfig) -> Result<(), PrepareError> {
        self.prepares += 1;
        match self.action.load(Ordering::Relaxed) {
            3 => Err(PrepareError::UnsupportedLayout),
            4 => panic!("controlled foreign prepare panic"),
            _ => Ok(()),
        }
    }
    fn start_audio(&mut self) -> Result<(), ProcessError> {
        self.starts += 1;
        self.session = true;
        Ok(())
    }
    fn stop_audio(&mut self) {
        self.stops += 1;
        self.session = false;
    }
    fn process(&mut self, b: &mut ProcessBlock<'_, '_, '_, f64>) -> ProcessStatus {
        self.calls += 1;
        self.committed += b.audio.frames();
        let state = current_float_state().unwrap();
        assert!(state.input_flush() && state.result_flush());
        let frames = b.audio.frames();
        for sample in b.audio.output_mut(0).unwrap().iter_mut().take(frames) {
            *sample = 42.;
        }
        let a = std::hint::black_box(f64::MAX);
        std::hint::black_box(a * a); // scope-created status, restored including child refusal/unwind
        match self.action.load(Ordering::Relaxed) {
            1 => ProcessStatus::Fault(ProcessError::BackendFailure),
            2 => panic!("controlled foreign process panic"),
            _ => ProcessStatus::Continue,
        }
    }
    fn reset(&mut self) {
        self.resets += 1;
        self.committed = 0;
    }
    fn deactivate(&mut self) {
        assert!(!self.session);
        self.deactivations += 1;
    }
}
fn child(action: Arc<AtomicUsize>) -> Child {
    Child {
        id: 9876,
        prepares: 0,
        calls: 0,
        committed: 0,
        resets: 0,
        starts: 0,
        stops: 0,
        deactivations: 0,
        action,
        session: false,
    }
}
fn config() -> ProcessConfig {
    ProcessConfig::new(48000., 3, 0, AudioLayout::new(1, 1).unwrap()).unwrap()
}
fn render(adapter: &mut DenormalProcessor<Child>, output: &mut [f64]) -> ProcessStatus {
    let input = [1., 2., 3.];
    let inputs = [&input[..]];
    let mut outputs = [output];
    let audio = AudioBuffer::separate(&inputs, &mut outputs, 3).unwrap();
    adapter.process(&mut ProcessBlock::new(audio, &[], Transport::default()))
}
#[test]
fn failed_prepare_retains_exact_owner_and_manual_session_refuses_early_deactivation() {
    let action = Arc::new(AtomicUsize::new(3));
    let mut adapter = DenormalProcessor::new(child(action.clone()), DenormalMode::FlushBoth)
        .ok()
        .unwrap();
    assert_eq!(
        adapter.prepare(config()),
        Err(PrepareError::UnsupportedLayout)
    );
    let original = adapter.into_inner().ok().unwrap();
    assert_eq!(original.id, 9876);
    assert_eq!(original.prepares, 1);
    assert_eq!(original.calls, 0);
    assert_eq!(original.deactivations, 0);
    action.store(0, Ordering::Relaxed);
    let mut adapter = DenormalProcessor::new(original, DenormalMode::FlushBoth)
        .ok()
        .unwrap();
    adapter.prepare(config()).unwrap();
    let mut adapter = adapter
        .into_inner()
        .err()
        .expect("prepared extraction must refuse");
    adapter.start_audio().unwrap();
    assert_eq!(
        adapter.prepare(config()),
        Err(PrepareError::AudioSessionActive)
    );
    assert!(
        std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| adapter.deactivate())).is_err()
    );
    assert_eq!(adapter.processor().deactivations, 0);
    let mut adapter = adapter
        .into_inner()
        .err()
        .expect("live extraction must refuse");
    adapter.stop_audio();
    adapter.stop_audio();
    adapter.deactivate();
    adapter.deactivate();
    let owner = adapter.into_inner().ok().unwrap();
    assert_eq!(
        (owner.id, owner.starts, owner.stops, owner.deactivations),
        (9876, 1, 1, 1)
    );
}
#[test]
fn foreign_prepare_panic_never_becomes_a_successful_inactive_owner_receipt() {
    let action = Arc::new(AtomicUsize::new(4));
    let mut adapter = DenormalProcessor::new(child(action.clone()), DenormalMode::FlushBoth)
        .ok()
        .unwrap();
    assert!(
        std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| adapter.prepare(config())))
            .is_err()
    );
    let mut adapter = adapter
        .into_inner()
        .err()
        .expect("uncertain preparation must retain owner");
    assert_eq!(
        adapter.prepare(config()),
        Err(PrepareError::AudioSessionActive)
    );
    assert_eq!(adapter.processor().prepares, 1);
    adapter.stop_audio();
    let mut adapter = adapter
        .into_inner()
        .err()
        .expect("stop alone cannot prove inactive preparation");
    adapter.deactivate();
    let original = adapter.into_inner().ok().unwrap();
    assert_eq!(original.id, 9876);
    assert_eq!(original.deactivations, 1);
}
#[test]
fn actual_child_fault_and_unwind_restore_full_status_and_preserve_committed_history() {
    for failure in [1, 2] {
        let action = Arc::new(AtomicUsize::new(failure));
        let mut adapter = DenormalProcessor::new(child(action.clone()), DenormalMode::FlushBoth)
            .ok()
            .unwrap();
        adapter.prepare(config()).unwrap();
        let before = current_float_state().unwrap();
        let mut output = [99.; 5];
        if failure == 1 {
            assert_eq!(
                render(&mut adapter, &mut output),
                ProcessStatus::Fault(ProcessError::BackendFailure)
            );
            assert_eq!(output, [0., 0., 0., 99., 99.]);
        } else {
            assert!(
                std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| render(
                    &mut adapter,
                    &mut output
                )))
                .is_err()
            );
        }
        assert_eq!(current_float_state().unwrap(), before);
        assert_eq!(adapter.first_native_error(), None);
        assert_eq!(
            (adapter.processor().calls, adapter.processor().committed),
            (1, 3)
        );
        action.store(0, Ordering::Relaxed);
        assert_eq!(
            render(&mut adapter, &mut output),
            ProcessStatus::Fault(ProcessError::BackendFailure)
        );
        assert_eq!(output, [0., 0., 0., 99., 99.]);
        assert_eq!(adapter.processor().calls, 1);
        adapter.reset();
        let before = current_float_state().unwrap();
        let info = allocation_counter::measure(|| {
            assert_eq!(render(&mut adapter, &mut output), ProcessStatus::Continue)
        });
        assert_eq!(info.count_total, 0);
        assert_eq!(info.count_current, 0);
        assert_eq!(output, [42., 42., 42., 99., 99.]);
        assert_eq!(current_float_state().unwrap(), before);
        assert_eq!(
            (adapter.processor().calls, adapter.processor().committed),
            (2, 3)
        );
        adapter.deactivate();
        let owner = adapter.into_inner().ok().unwrap();
        assert_eq!(owner.id, 9876);
    }
}
