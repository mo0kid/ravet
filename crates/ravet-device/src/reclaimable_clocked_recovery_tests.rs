use super::*;
use crate::reclaimable_clocked::tests::{SimulatedPair, simulated_pair};
use ravet_foundation::{PrepareError, ProcessBlock, ProcessConfig, ProcessStatus};
use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};
struct Child {
    id: usize,
    prepares: Arc<AtomicUsize>,
    deactivates: Arc<AtomicUsize>,
}
impl Child {
    fn new() -> Self {
        Self {
            id: 19,
            prepares: Arc::new(AtomicUsize::new(0)),
            deactivates: Arc::new(AtomicUsize::new(0)),
        }
    }
}
impl Processor for Child {
    fn prepare(&mut self, _: ProcessConfig) -> Result<(), PrepareError> {
        self.prepares.fetch_add(1, Ordering::Relaxed);
        Ok(())
    }
    fn process(&mut self, b: &mut ProcessBlock<'_, '_, '_>) -> ProcessStatus {
        b.clear_outputs();
        ProcessStatus::Silence
    }
    fn reset(&mut self) {}
    fn deactivate(&mut self) {
        self.deactivates.fetch_add(1, Ordering::Relaxed);
    }
}
impl Pair<Child> for SimulatedPair<Child> {
    fn statistics(&self) -> ReclaimableClockedStatistics {
        self.statistics()
    }
    fn play(&self) -> Result<(), cpal::Error> {
        self.play()
    }
    fn stop(self) -> ClockedDuplexRetirement<Child> {
        self.stop()
    }
}
fn no_open(_: Child) -> Result<SimulatedPair<Child>, ReclaimableClockedFailure<Child>> {
    panic!("open while proof pending")
}
fn no_restore(_: &mut Child, _: ClockedRecoveryReceipt) -> Result<(), &'static str> {
    panic!("restore before receipt")
}
fn drain(c: &mut Coordinator<Child, SimulatedPair<Child>>) -> ClockedRecoveryReceipt {
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(3);
    loop {
        match c.service(false, no_open, no_restore).unwrap() {
            ReclaimableClockedRecoveryStatus::Reclaimed { .. } => match &c.state {
                Some(State::Owned {
                    previous: Some(receipt),
                    ..
                }) => return *receipt,
                _ => panic!("missing owner"),
            },
            ReclaimableClockedRecoveryStatus::Retiring => {
                assert!(std::time::Instant::now() < deadline);
                std::thread::yield_now();
            }
            other => panic!("unexpected {other:?}"),
        }
    }
}
#[test]
fn asynchronous_fault_recovery_never_opens_or_restores_pending_then_restores_original_owner_once() {
    let child = Child::new();
    let prepares = Arc::clone(&child.prepares);
    let deactivates = Arc::clone(&child.deactivates);
    let mut c = Coordinator::new(child);
    let mut backend = None;
    assert_eq!(
        c.service(
            true,
            |p| {
                let (s, b) = simulated_pair(p);
                backend = Some(b);
                Ok(s)
            },
            no_restore
        )
        .unwrap(),
        ReclaimableClockedRecoveryStatus::Opened { generation: 1 }
    );
    backend.as_ref().unwrap().fault();
    assert_eq!(
        c.service(true, no_open, no_restore).unwrap(),
        ReclaimableClockedRecoveryStatus::Retiring
    );
    for _ in 0..20 {
        assert_eq!(
            c.service(true, no_open, no_restore).unwrap(),
            ReclaimableClockedRecoveryStatus::Retiring
        );
    }
    assert_eq!(prepares.load(Ordering::Relaxed), 1);
    assert_eq!(deactivates.load(Ordering::Relaxed), 0);
    backend.take().unwrap().release();
    let receipt = drain(&mut c);
    assert_eq!(receipt.statistics.input.backend_errors, 1);
    assert_eq!(deactivates.load(Ordering::Relaxed), 1);
    let mut restored = 0;
    let status = c
        .service(
            true,
            |p| {
                assert_eq!(p.id, 29);
                let (s, b) = simulated_pair(p);
                backend = Some(b);
                Ok(s)
            },
            |p, r| {
                assert_eq!(p.id, 19);
                assert_eq!(r, receipt);
                p.id = 29;
                restored += 1;
                Ok::<_, &'static str>(())
            },
        )
        .unwrap();
    assert_eq!(
        status,
        ReclaimableClockedRecoveryStatus::Opened { generation: 2 }
    );
    assert_eq!(restored, 1);
    assert_eq!(
        c.service(true, no_open, no_restore).unwrap(),
        ReclaimableClockedRecoveryStatus::Running
    );
    c.set_enabled(false);
    backend.take().unwrap().release();
    loop {
        let s = c.service(true, no_open, no_restore).unwrap();
        if s == (ReclaimableClockedRecoveryStatus::Disabled { retiring: false }) {
            break;
        }
        std::thread::yield_now();
    }
    assert_eq!(prepares.load(Ordering::Relaxed), 2);
    assert_eq!(deactivates.load(Ordering::Relaxed), 2);
    match c.into_owner().unwrap() {
        FailedClockedOwner::Immediate(p) => assert_eq!(p.id, 29),
        _ => panic!("no pending proof"),
    }
}
#[test]
fn restoration_error_retains_original_mutated_owner_receipt_and_disabled_pending_has_no_reopen() {
    let mut c = Coordinator::new(Child::new());
    let mut backend = None;
    c.service(
        true,
        |p| {
            let (s, b) = simulated_pair(p);
            backend = Some(b);
            Ok(s)
        },
        no_restore,
    )
    .unwrap();
    assert_eq!(
        c.service(false, no_open, no_restore).unwrap(),
        ReclaimableClockedRecoveryStatus::Retiring
    );
    backend.take().unwrap().release();
    drain(&mut c);
    match c
        .service(true, no_open, |p, _| {
            p.id = 39;
            Err("original restore failure")
        })
        .unwrap_err()
    {
        ClockedRecoveryFailure::Restore(e) => assert_eq!(e, "original restore failure"),
        _ => panic!("wrong error"),
    }
    c.set_enabled(false);
    assert_eq!(
        c.service(true, no_open, no_restore).unwrap(),
        ReclaimableClockedRecoveryStatus::Disabled { retiring: false }
    );
    c.set_enabled(true);
    c.service(
        true,
        |p| {
            assert_eq!(p.id, 49);
            let (s, b) = simulated_pair(p);
            backend = Some(b);
            Ok(s)
        },
        |p, r| {
            assert_eq!(p.id, 39);
            assert_eq!(r.statistics.input.backend_errors, 0);
            p.id = 49;
            Ok::<_, &'static str>(())
        },
    )
    .unwrap();
    c.set_enabled(false);
    assert_eq!(
        c.service(true, no_open, no_restore).unwrap(),
        ReclaimableClockedRecoveryStatus::Disabled { retiring: true }
    );
    c.set_enabled(true);
    assert_eq!(
        c.service(true, no_open, no_restore).unwrap(),
        ReclaimableClockedRecoveryStatus::Retiring
    );
    let pending = c.into_owner().unwrap();
    backend.take().unwrap().release();
    match pending {
        FailedClockedOwner::Pending(proof) => match proof.wait().unwrap() {
            ReclaimedClockedDuplex::Reclaimed { owner, .. } => {
                assert_eq!(owner.into_processor().id, 49)
            }
            _ => panic!("fault"),
        },
        _ => panic!("pending proof needed"),
    }
}

#[test]
fn immediate_and_pending_open_errors_keep_original_owners_generation_and_receipt_gating() {
    use crate::reclaimable_clocked::tests::simulated_open_failure;
    let child = Child::new();
    let prepares = Arc::clone(&child.prepares);
    let deactivates = Arc::clone(&child.deactivates);
    let mut c = Coordinator::new(child);
    let error = c
        .service(
            true,
            |p| {
                let (f, b) = simulated_open_failure(p, false);
                assert!(b.is_none());
                Err(f)
            },
            no_restore,
        )
        .unwrap_err();
    assert!(matches!(
        error,
        ClockedRecoveryFailure::Control(ReclaimableClockedRecoveryError::Open(_))
    ));
    assert_eq!(c.status(), ReclaimableClockedRecoveryStatus::Owned);
    assert_eq!(c.generation, 0);
    assert_eq!(prepares.load(Ordering::Relaxed), 0);
    let mut backend = None;
    let error = c
        .service(
            true,
            |p| {
                let (f, b) = simulated_open_failure(p, true);
                backend = b;
                Err(f)
            },
            no_restore,
        )
        .unwrap_err();
    match error {
        ClockedRecoveryFailure::Control(ReclaimableClockedRecoveryError::Open(e)) => {
            assert!(e.to_string().contains("original recovery open failure"))
        }
        _ => panic!("wrong error"),
    }
    assert_eq!(c.generation, 0);
    assert_eq!(c.status(), ReclaimableClockedRecoveryStatus::Retiring);
    assert_eq!(
        c.service(true, no_open, no_restore).unwrap(),
        ReclaimableClockedRecoveryStatus::Retiring
    );
    backend.take().unwrap().release();
    drain(&mut c);
    assert_eq!(deactivates.load(Ordering::Relaxed), 1);
    assert_eq!(
        c.service(
            true,
            |p| {
                assert_eq!(p.id, 19);
                let (s, b) = simulated_pair(p);
                backend = Some(b);
                Ok(s)
            },
            |p, _| {
                assert_eq!(p.id, 19);
                Ok::<_, &'static str>(())
            }
        )
        .unwrap(),
        ReclaimableClockedRecoveryStatus::Opened { generation: 1 }
    );
    c.request_reopen();
    assert_eq!(c.status(), ReclaimableClockedRecoveryStatus::Retiring);
    assert_eq!(
        c.service(true, no_open, no_restore).unwrap(),
        ReclaimableClockedRecoveryStatus::Retiring
    );
    backend.take().unwrap().release();
    drain(&mut c);
    assert_eq!(prepares.load(Ordering::Relaxed), 2);
    assert_eq!(deactivates.load(Ordering::Relaxed), 2);
    assert!(c.statistics().is_some());
    assert!(matches!(c.into_owner().unwrap(),FailedClockedOwner::Immediate(p) if p.id==19));
}
#[test]
fn invalid_selection_and_prepared_profile_return_original_owner_before_native_or_child_work() {
    let config = cpal::StreamConfig {
        channels: 1,
        sample_rate: 48000,
        buffer_size: cpal::BufferSize::Default,
    };
    let options = ClockedDuplexOptions {
        maximum_frames: 8,
        taps: 16,
        clock: ravet_signal::FifoClockConfig {
            capacity_frames: 32,
            target_frames: 16,
            ..Default::default()
        },
    };
    for invalid_id in [false, true] {
        let child = Child::new();
        let prepares = Arc::clone(&child.prepares);
        let deactivates = Arc::clone(&child.deactivates);
        let mut output = config;
        if !invalid_id {
            output.buffer_size = cpal::BufferSize::Fixed(0);
        }
        let failure = RecoveringReclaimableClockedDuplex::new(
            if invalid_id { "" } else { "explicit input" },
            "explicit output",
            config,
            output,
            32,
            child,
            options,
        )
        .err()
        .unwrap();
        let (child, error) = failure.into_parts();
        assert_eq!(child.id, 19);
        assert!(
            matches!(error, ReclaimableClockedRecoveryError::InvalidSelection)
                | matches!(error, ReclaimableClockedRecoveryError::Open(_))
        );
        assert_eq!(prepares.load(Ordering::Relaxed), 0);
        assert_eq!(deactivates.load(Ordering::Relaxed), 0);
    }
}

#[test]
fn failed_start_preserves_original_error_and_requires_retirement_before_retry() {
    let mut c = Coordinator::new(Child::new());
    let mut backend = None;
    let error = c
        .service(
            true,
            |p| {
                let (mut s, b) = simulated_pair(p);
                s.fail_start();
                backend = Some(b);
                Ok(s)
            },
            no_restore,
        )
        .unwrap_err();
    match error {
        ClockedRecoveryFailure::Control(ReclaimableClockedRecoveryError::Start(e)) => {
            assert!(e.to_string().contains("original paired start failure"))
        }
        _ => panic!("wrong error"),
    }
    assert_eq!(c.generation, 0);
    assert_eq!(c.status(), ReclaimableClockedRecoveryStatus::Retiring);
    assert_eq!(
        c.service(true, no_open, no_restore).unwrap(),
        ReclaimableClockedRecoveryStatus::Retiring
    );
    backend.take().unwrap().release();
    drain(&mut c);
    let status = c
        .service(
            true,
            |p| {
                let (s, b) = simulated_pair(p);
                backend = Some(b);
                Ok(s)
            },
            |p, _| {
                assert_eq!(p.id, 19);
                Ok::<_, &'static str>(())
            },
        )
        .unwrap();
    assert_eq!(
        status,
        ReclaimableClockedRecoveryStatus::Opened { generation: 1 }
    );
    let owner = c.into_owner().unwrap();
    backend.take().unwrap().release();
    assert!(matches!(owner, FailedClockedOwner::Pending(_)));
}
#[test]
fn exceptional_restoration_panic_marks_lost_and_never_forges_recovered_owner() {
    let mut c = Coordinator::new(Child::new());
    let mut backend = None;
    c.service(
        true,
        |p| {
            let (s, b) = simulated_pair(p);
            backend = Some(b);
            Ok(s)
        },
        no_restore,
    )
    .unwrap();
    c.request_reopen();
    backend.take().unwrap().release();
    drain(&mut c);
    assert!(
        std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| c.service(
            true,
            no_open,
            |_, _| -> Result<(), &'static str> { panic!("injected restore panic") }
        )))
        .is_err()
    );
    assert_eq!(c.status(), ReclaimableClockedRecoveryStatus::Lost);
    assert!(matches!(
        c.into_owner(),
        Err(ReclaimableClockedRecoveryError::Lost)
    ));
}

#[test]
fn known_live_callback_faults_retire_before_failing_selection_and_pending_services_never_enumerate()
{
    for kind in 0..4 {
        let mut c = Coordinator::new(Child::new());
        let mut backend = None;
        c.service(
            true,
            |p| {
                let (s, b) = simulated_pair(p);
                backend = Some(b);
                Ok(s)
            },
            no_restore,
        )
        .unwrap();
        let selection_calls = AtomicUsize::new(0);
        let select = || -> Result<bool, ReclaimableClockedRecoveryError> {
            selection_calls.fetch_add(1, Ordering::Relaxed);
            Err(ReclaimableClockedRecoveryError::Open(
                ReclaimableClockedOpenError::InputBackend(cpal::Error::with_message(
                    cpal::ErrorKind::DeviceNotAvailable,
                    "original selection failure",
                )),
            ))
        };
        let error = c
            .service_selected(select, |a| *a, |_, p| no_open(p), no_restore)
            .unwrap_err();
        match error {
            ClockedRecoveryFailure::Control(ReclaimableClockedRecoveryError::Open(e)) => {
                assert!(e.to_string().contains("original selection failure"))
            }
            _ => panic!("wrong original error"),
        }
        assert_eq!(selection_calls.load(Ordering::Relaxed), 1);
        assert_eq!(c.status(), ReclaimableClockedRecoveryStatus::Running);
        backend.as_ref().unwrap().fault_kind(kind);
        assert_eq!(
            c.service_selected(select, |a| *a, |_, p| no_open(p), no_restore)
                .unwrap(),
            ReclaimableClockedRecoveryStatus::Retiring
        );
        for _ in 0..20 {
            assert_eq!(
                c.service_selected(select, |a| *a, |_, p| no_open(p), no_restore)
                    .unwrap(),
                ReclaimableClockedRecoveryStatus::Retiring
            );
        }
        assert_eq!(selection_calls.load(Ordering::Relaxed), 1);
        backend.take().unwrap().release();
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(3);
        loop {
            match c
                .service_selected(select, |a| *a, |_, p| no_open(p), no_restore)
                .unwrap()
            {
                ReclaimableClockedRecoveryStatus::Reclaimed { .. } => break,
                ReclaimableClockedRecoveryStatus::Retiring => {
                    assert!(std::time::Instant::now() < deadline);
                    std::thread::yield_now();
                }
                other => panic!("unexpected {other:?}"),
            }
        }
        assert_eq!(selection_calls.load(Ordering::Relaxed), 1);
        assert!(matches!(c.into_owner().unwrap(),FailedClockedOwner::Immediate(p) if p.id==19));
    }
}
