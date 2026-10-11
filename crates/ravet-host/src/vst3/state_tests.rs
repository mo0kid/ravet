use super::*;
use crate::vst3::instances::{self, Library, Profile};
use crate::vst3::{ClassRecord, FactoryRecord};
use std::{cell::Cell, ptr::NonNull};
struct Child {
    calls: Cell<usize>,
    destroyed: &'static Cell<usize>,
    mode: Cell<u32>,
}
unsafe extern "C" fn unused_open(_: *const u8, _: u32, _: *mut *mut c_void, _: *mut i32) -> i32 {
    1
}
unsafe extern "C" fn unused_factory(_: *const c_void, _: *mut FactoryRecord) -> i32 {
    1
}
unsafe extern "C" fn unused_class(_: *const c_void, _: u32, _: *mut ClassRecord) -> i32 {
    1
}
unsafe extern "C" fn unused_create(
    _: *mut c_void,
    _: u32,
    _: *mut *mut c_void,
    _: *mut i32,
) -> i32 {
    1
}
unsafe extern "C" fn unused_prepare(_: *mut c_void, _: *const Profile, _: *mut i32) -> i32 {
    1
}
unsafe extern "C" fn unused_begin(_: *mut c_void, _: *mut *mut c_void, _: *mut i32) -> i32 {
    1
}
unsafe extern "C" fn unused_process(
    _: *mut c_void,
    _: *const instances::Block,
    _: *mut i32,
) -> i32 {
    1
}
unsafe extern "C" fn unused_requests(_: *const c_void, _: *mut u32) -> i32 {
    1
}
unsafe extern "C" fn stop(_: *mut c_void, result: *mut i32) -> i32 {
    unsafe {
        *result = 0;
    }
    0
}
unsafe extern "C" fn destroy(child: *mut c_void, result: *mut i32) -> i32 {
    let child = unsafe { Box::from_raw(child.cast::<Child>()) };
    child.destroyed.set(child.destroyed.get() + 1);
    unsafe {
        *result = 0;
    }
    0
}
fn instance_table() -> InstanceProvider {
    InstanceProvider {
        struct_size: size_of::<InstanceProvider>() as u32,
        version: 2,
        open: Some(unused_open),
        factory: Some(unused_factory),
        class_info: Some(unused_class),
        close: Some(stop),
        create: Some(unused_create),
        prepare: Some(unused_prepare),
        start: Some(stop),
        begin_audio: Some(unused_begin),
        process: Some(unused_process),
        end_audio: Some(stop),
        stop: Some(stop),
        destroy: Some(destroy),
        restart_requests: Some(unused_requests),
    }
}
unsafe extern "C" fn capture(
    child: *mut c_void,
    record: *mut Record,
    component: *mut u8,
    component_capacity: u32,
    controller: *mut u8,
    controller_capacity: u32,
    report: *mut RawReport,
) -> i32 {
    let child = unsafe { &*child.cast::<Child>() };
    child.calls.set(child.calls.get() + 1);
    assert!(component_capacity >= 4 && controller_capacity >= 4);
    unsafe {
        *report = RawReport {
            struct_size: size_of::<RawReport>() as u32,
            stage: 0,
            stream_failure: 0,
            flags: ATTEMPTED | RETURNED,
            native_result: 0,
            controller_capture_result: 0,
        };
        *record = Record {
            struct_size: size_of::<Record>() as u32,
            controller_mode: 2,
            component_size: 4,
            controller_size: 4,
            class_id: [1; 16],
        };
        std::ptr::copy_nonoverlapping([11u8, 12, 13, 14].as_ptr(), component, 4);
        std::ptr::copy_nonoverlapping([21u8, 22, 23, 24].as_ptr(), controller, 4);
        match child.mode.get() {
            1 => (*record).component_size = component_capacity + 1,
            2 => (*record).class_id = [2; 16],
            3 => (*record).controller_mode = 3,
            4 => (*report).flags |= THROWN,
            5 => {
                (*report).stage = 2;
                (*report).native_result = 4242;
                return 4;
            }
            6 => {
                (*report).stage = 3;
                (*report).native_result = 4242;
                return 4;
            }
            _ => {}
        }
    }
    0
}
unsafe extern "C" fn restore(
    child: *mut c_void,
    record: *const Record,
    component: *const u8,
    controller: *const u8,
    report: *mut RawReport,
) -> i32 {
    let child = unsafe { &*child.cast::<Child>() };
    child.calls.set(child.calls.get() + 1);
    unsafe {
        assert_eq!((*record).component_size, 4);
        assert_eq!((*record).controller_size, 4);
        assert_eq!(std::slice::from_raw_parts(component, 4), [11, 12, 13, 14]);
        assert_eq!(std::slice::from_raw_parts(controller, 4), [21, 22, 23, 24]);
        *report = RawReport {
            struct_size: size_of::<RawReport>() as u32,
            stage: 0,
            stream_failure: 0,
            flags: MUTATING | PREPARE_REQUIRED | ATTEMPTED | RETURNED,
            native_result: 0,
            controller_capture_result: 0,
        };
        match child.mode.get() {
            1 => {
                (*report).stage = 5;
                (*report).flags = MUTATING | POISONED | ATTEMPTED | RETURNED;
                (*report).native_result = 4242;
                return 4;
            }
            2 => {
                (*report).stage = 4;
                (*report).flags = MUTATING | POISONED | ATTEMPTED | THROWN;
                (*report).native_result = -1;
                return 4;
            }
            3 => {
                (*report).stage = 3;
                (*report).flags = MUTATING | POISONED | ATTEMPTED | RETURNED;
                (*report).stream_failure = 3;
                return 4;
            }
            4 => {
                (*report).flags = 0;
                return 2;
            } // preflight budget refusal, no mutation
            5 => (*report).flags = 0x8000,
            6 => {
                (*report).stage = 5;
                (*report).flags = ATTEMPTED | RETURNED;
                (*report).native_result = 4242;
                return 4;
            }
            7 => {
                (*report).stage = 1;
                (*report).flags = 0;
                return 4;
            }
            8 => {
                (*report).flags = ATTEMPTED | RETURNED;
                (*report).native_result = 4242;
                return 4;
            }
            9 => {
                (*report).stage = 3;
                (*report).flags = MUTATING | POISONED;
                return 4;
            }
            10 => {
                (*report).flags = MUTATING | POISONED | ATTEMPTED | RETURNED;
                (*report).native_result = 4242;
                return 4;
            }
            11 => {
                (*report).flags = 0;
                (*report).stream_failure = 1;
                return 4;
            }
            12 => {
                (*report).stage = 3;
                (*report).flags = MUTATING | ATTEMPTED | RETURNED;
                (*report).native_result = 4242;
                return 4;
            }
            13 => {
                (*report).stage = 3;
                (*report).flags = MUTATING | POISONED | ATTEMPTED | RETURNED;
                return 4;
            }
            _ => {}
        }
    }
    0
}
fn companion(table: &InstanceProvider) -> RawProvider {
    RawProvider {
        struct_size: size_of::<RawProvider>() as u32,
        version: 1,
        provider: table,
        capture: Some(capture),
        restore: Some(restore),
    }
}
fn library(table: &InstanceProvider) -> Library<'_> {
    Library {
        provider: unsafe { instances::Provider::from_raw(table).unwrap() },
        handle: None,
        classes: Vec::new(),
        factory: None,
    }
}
fn child<'m, 'a>(library: &'m Library<'a>, destroyed: &'static Cell<usize>) -> Instance<'m, 'a> {
    Instance {
        library: crate::vst3::instances::Parent::Borrowed(library),
        handle: Some(
            NonNull::from(Box::leak(Box::new(Child {
                calls: Cell::new(0),
                destroyed,
                mode: Cell::new(0),
            })))
            .cast(),
        ),
        profile: Some(Profile::new(48000., 16, 1, 1)),
        running: false,
        poisoned: false,
        state_fault: false,
        parameter_provider: None,
        processing_provider: None,
        processing_metadata: None,
        hosted: Default::default(),
        class_id: [1; 16],
    }
}
fn control<'a>(instance: &'a Instance<'_, '_>) -> &'a Child {
    unsafe { &*instance.handle.unwrap().as_ptr().cast::<Child>() }
}
#[test]
fn owned_snapshot_pairing_capacity_and_running_preflight() {
    assert!(CaptureCapacity::new(0, 1).is_err());
    assert!(CaptureCapacity::new(MAXIMUM_BYTES + 1, 1).is_err());
    assert!(CaptureCapacity::new(MAXIMUM_BYTES, MAXIMUM_BYTES).is_ok());
    let table = instance_table();
    let other = instance_table();
    let raw = companion(&table);
    let mismatch = companion(&other);
    let provider = unsafe { Provider::from_raw(&raw).unwrap() };
    let mismatch = unsafe { Provider::from_raw(&mismatch).unwrap() };
    let library = library(&table);
    let destroyed = Box::leak(Box::new(Cell::new(0)));
    let mut instance = child(&library, destroyed);
    let capacity = CaptureCapacity::new(8, 8).unwrap();
    assert_eq!(
        instance.capture_state(mismatch, capacity).unwrap_err(),
        Error::Abi
    );
    instance.running = true;
    assert_eq!(
        instance.capture_state(provider, capacity).unwrap_err(),
        Error::Lifecycle
    );
    instance.running = false;
    assert_eq!(control(&instance).calls.get(), 0);
    let captured = instance.capture_state(provider, capacity).unwrap();
    assert_eq!(captured.snapshot.component(), [11, 12, 13, 14]);
    assert_eq!(
        captured.snapshot.controller(),
        &ControllerState::Bytes(vec![21, 22, 23, 24])
    );
    let snapshot = std::thread::spawn(move || captured.snapshot.clone())
        .join()
        .unwrap();
    let wrong = Snapshot::new(
        [2; 16],
        snapshot.component().to_vec(),
        snapshot.controller().clone(),
    )
    .unwrap();
    assert_eq!(
        instance.restore_state(provider, &wrong).unwrap_err(),
        Error::Shape
    );
    assert_eq!(control(&instance).calls.get(), 1);
    assert!(
        instance
            .restore_state(provider, &snapshot)
            .unwrap()
            .prepare_required()
    );
    assert!(instance.profile.is_none());
    drop(instance);
    assert_eq!(destroyed.get(), 1);
}
#[test]
fn returned_exception_stream_faults_poison_but_retire_once() {
    for mode in [1, 2, 3, 5, 6, 7, 8, 9, 10, 11, 12, 13] {
        let table = instance_table();
        let raw = companion(&table);
        let provider = unsafe { Provider::from_raw(&raw).unwrap() };
        let library = library(&table);
        let destroyed = Box::leak(Box::new(Cell::new(0)));
        let mut instance = child(&library, destroyed);
        let snapshot = instance
            .capture_state(provider, CaptureCapacity::new(8, 8).unwrap())
            .unwrap()
            .snapshot;
        control(&instance).mode.set(mode);
        let error = instance.restore_state(provider, &snapshot).unwrap_err();
        match error {
            Error::Provider { status, report } => {
                assert_eq!(status, 4);
                assert!(report.poisoned());
                assert_eq!(
                    report.returned_status(),
                    if mode == 1 {
                        Some(4242)
                    } else if mode == 2 {
                        None
                    } else {
                        Some(0)
                    }
                );
                assert_eq!(report.threw(), mode == 2);
                assert_eq!(
                    report.stream_failure(),
                    if mode == 3 {
                        Some(StreamFailure::SourceWrite)
                    } else {
                        None
                    }
                );
            }
            Error::Abi => assert!(mode >= 5),
            _ => panic!("wrong state failure"),
        }
        assert!(instance.state_fault && !instance.poisoned && instance.profile.is_none());
        assert_eq!(
            instance
                .prepare(Profile::new(48000., 16, 1, 1))
                .unwrap_err(),
            instances::Error::Poisoned
        );
        assert_eq!(instance.start().unwrap_err(), instances::Error::Poisoned);
        assert_eq!(
            instance
                .capture_state(provider, CaptureCapacity::new(8, 8).unwrap())
                .unwrap_err(),
            Error::Poisoned
        );
        instance.stop().unwrap();
        assert_eq!(control(&instance).calls.get(), 2);
        if mode % 2 == 0 {
            assert_eq!(
                instance.retire().ok(),
                Some(instances::InstanceRetirement::Complete)
            );
        } else {
            drop(instance);
        }
        assert_eq!(destroyed.get(), 1);
    }
}
#[test]
fn non_mutating_budget_refusal_preserves_profile_and_capture_validation() {
    let table = instance_table();
    let raw = companion(&table);
    let provider = unsafe { Provider::from_raw(&raw).unwrap() };
    let library = library(&table);
    let destroyed = Box::leak(Box::new(Cell::new(0)));
    let mut instance = child(&library, destroyed);
    let capacity = CaptureCapacity::new(8, 8).unwrap();
    let snapshot = instance.capture_state(provider, capacity).unwrap().snapshot;
    control(&instance).mode.set(4);
    let error = instance.restore_state(provider, &snapshot).unwrap_err();
    assert!(matches!(error, Error::Provider { status: 2, .. }));
    assert!(!instance.state_fault && instance.profile.is_some());
    for mode in 1..=6 {
        control(&instance).mode.set(mode);
        let error = instance.capture_state(provider, capacity).unwrap_err();
        if mode == 5 {
            assert!(matches!(error, Error::Provider { status: 4, .. }));
        } else {
            assert_eq!(error, Error::Abi);
        }
        assert!(!instance.state_fault && instance.profile.is_some());
    }
    drop(instance);
    assert_eq!(destroyed.get(), 1);
}
