use super::*;
use std::cell::RefCell;
#[derive(Default)]
struct State {
    mode: i32,
    opens: usize,
    queries: usize,
    closes: usize,
    alive: usize,
}
thread_local! { static STATE: RefCell<State> = RefCell::new(State::default()); }
unsafe extern "C" fn open(_: *const u8, _: u32, output: *mut *mut c_void, result: *mut i32) -> i32 {
    STATE.with(|state| {
        let mut state = state.borrow_mut();
        state.opens += 1;
        if state.mode == 1 {
            unsafe {
                *result = -56;
            }
            return 4;
        }
        state.alive += 1;
        unsafe {
            *output = Box::into_raw(Box::new(123usize)).cast();
            *result = 0;
        }
        0
    })
}
unsafe extern "C" fn factory(_: *const c_void, record: *mut FactoryRecord) -> i32 {
    STATE.with(|state| {
        state.borrow_mut().queries += 1;
    });
    unsafe {
        let record = &mut *record;
        record.vendor[..6].copy_from_slice(b"RAVET\0");
        record.class_count = 2;
    }
    0
}
unsafe extern "C" fn class_info(_: *const c_void, index: u32, record: *mut ClassRecord) -> i32 {
    STATE.with(|state| {
        let mut state = state.borrow_mut();
        state.queries += 1;
        if state.mode == 2 && index == 1 {
            return 3;
        }
        unsafe {
            let record = &mut *record;
            record.id[0] = index as u8 + 1;
            record.cardinality = i32::MAX;
            record.category[..7].copy_from_slice(b"Effect\0");
            record.name[..5].copy_from_slice(b"Gain\0");
            record.vendor[..6].copy_from_slice(b"RAVET\0");
        }
        0
    })
}
unsafe extern "C" fn close(handle: *mut c_void, result: *mut i32) -> i32 {
    STATE.with(|state| {
        let mut state = state.borrow_mut();
        state.closes += 1;
        if state.mode == 4 {
            state.mode = 0;
            unsafe {
                *result = -7;
            }
            return 5;
        }
        state.alive -= 1;
        drop(unsafe { Box::from_raw(handle.cast::<usize>()) });
        unsafe {
            *result = if state.mode == 3 { -42 } else { 0 };
        }
        if state.mode == 3 { 7 } else { 0 }
    })
}
const RAW: RawProvider = RawProvider {
    struct_size: std::mem::size_of::<RawProvider>() as u32,
    version: 1,
    open: Some(open),
    factory: Some(factory),
    class_info: Some(class_info),
    close: Some(close),
};
fn reset(mode: i32) {
    STATE.with(|state| {
        *state.borrow_mut() = State {
            mode,
            ..State::default()
        }
    });
}
fn load() -> Result<Library<'static>, Error> {
    // SAFETY: static compliant fake provider on this test owner thread and only
    // its owned mock handle; no external module code or thread escapes.
    unsafe { Provider::from_raw(&RAW)?.load("selected.vst3") }
}
#[test]
fn complete_cache_executes_no_foreign_getters_and_retires_once() {
    reset(0);
    let library = load().unwrap();
    for _ in 0..100 {
        assert_eq!(library.factory().vendor, "RAVET");
        assert_eq!(library.classes().len(), 2);
        assert_eq!(library.find(&library.classes()[0].id).unwrap().name, "Gain");
    }
    STATE.with(|state| {
        let state = state.borrow();
        assert_eq!(
            (state.opens, state.queries, state.closes, state.alive),
            (1, 3, 0, 1)
        );
    });
    assert!(matches!(
        library.retire(),
        Ok(Retirement::Complete { context_result: 0 })
    ));
    STATE.with(|state| {
        let state = state.borrow();
        assert_eq!((state.closes, state.alive), (1, 0));
    });
}
#[test]
fn native_open_failure_and_late_catalogue_failure_preserve_exact_scope() {
    reset(1);
    assert!(matches!(
        load(),
        Err(Error::Native {
            status: 4,
            result: -56
        })
    ));
    STATE.with(|state| {
        let state = state.borrow();
        assert_eq!((state.queries, state.closes, state.alive), (0, 0, 0));
    });
    reset(2);
    assert!(matches!(
        load(),
        Err(Error::Native {
            status: 3,
            result: 0
        })
    ));
    STATE.with(|state| {
        let state = state.borrow();
        assert_eq!((state.queries, state.closes, state.alive), (3, 1, 0));
    });
}
#[test]
fn failed_native_retirement_is_consumed_and_rejected_retirement_recovers_owner() {
    reset(3);
    let library = load().unwrap();
    assert!(matches!(
        library.retire(),
        Ok(Retirement::FailedNative {
            context_result: -42
        })
    ));
    STATE.with(|state| {
        let state = state.borrow();
        assert_eq!((state.closes, state.alive), (1, 0));
    });
    reset(4);
    let library = load().unwrap();
    let failure = match library.retire() {
        Err(failure) => failure,
        Ok(_) => panic!("expected retained rejection"),
    };
    assert_eq!(
        failure.error,
        Error::Native {
            status: 5,
            result: -7
        }
    );
    assert_eq!(failure.owner.classes().len(), 2);
    STATE.with(|state| {
        let state = state.borrow();
        assert_eq!((state.closes, state.alive), (1, 1));
    });
    assert!(matches!(
        failure.owner.retire(),
        Ok(Retirement::Complete { context_result: 0 })
    ));
    STATE.with(|state| {
        let state = state.borrow();
        assert_eq!((state.closes, state.alive), (2, 0));
    });
}
#[test]
fn invalid_table_and_path_reject_before_any_native_operation() {
    reset(0);
    let mut raw = RAW;
    raw.class_info = None;
    assert!(matches!(
        unsafe { Provider::from_raw(&raw) },
        Err(Error::ProviderAbi)
    ));
    for path in ["", "a\0b"] {
        assert!(matches!(
            unsafe { Provider::from_raw(&RAW).unwrap().load(path) },
            Err(Error::InvalidPath)
        ));
    }
    assert!(matches!(
        unsafe { Provider::from_raw(&RAW).unwrap().load("x".repeat(4097)) },
        Err(Error::InvalidPath)
    ));
    STATE.with(|state| assert_eq!(state.borrow().opens, 0));
}
