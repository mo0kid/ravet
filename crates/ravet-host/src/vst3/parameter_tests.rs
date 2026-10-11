use super::*;

#[test]
fn parameter_abi_owned_metadata_and_controller_identities_are_explicit() {
    assert_eq!(size_of::<Point>(), 16);
    assert_eq!(size_of::<RawRecord>(), 1568);
    assert_eq!(size_of::<RawReport>(), 24);
    assert_eq!(size_of::<RawProvider>(), 72);
    let parameter = Parameter {
        id: 1,
        flags: 1 << 16,
        steps: 1,
        unit: -1,
        default_normalized: 0.,
        title: "Bypass 🎵".to_owned(),
        short_title: String::new(),
        units: String::new(),
    };
    assert!(parameter.writable() && parameter.is_bypass() && !parameter.is_program_selector());
    let mut read_only = parameter.clone();
    read_only.flags = 2 | 16;
    assert!(!read_only.writable());
    let catalogue = Catalogue {
        parameters: vec![parameter],
    };
    let copied = catalogue.clone();
    assert_eq!(copied.parameters()[0].title, "Bypass 🎵");
    assert_eq!(text(&[b'a', 0], true), Ok("a".to_owned()));
    assert_eq!(text(&[0], true), Err(Error::Abi));
    assert_eq!(text(&[0xff, 0], false), Err(Error::Abi));
    assert_eq!(text(b"a", false), Err(Error::Abi));
}

#[test]
fn timed_points_whole_capacity_order_zero_frame_and_nonfinite_preflight() {
    assert!(points_admitted(&[], 0));
    let point = Point {
        id: 1,
        offset: 0,
        value: 0.5,
    };
    assert!(points_admitted(&[point], 0));
    for p in [
        Point {
            id: u32::MAX,
            ..point
        },
        Point { offset: 1, ..point },
        Point {
            value: f64::NAN,
            ..point
        },
        Point {
            value: f64::INFINITY,
            ..point
        },
        Point {
            value: -0.1,
            ..point
        },
        Point {
            value: 1.1,
            ..point
        },
    ] {
        assert!(!points_admitted(&[p], 0));
    }
    assert!(!points_admitted(&[point, point], 1));
    assert!(points_admitted(&[point, Point { id: 2, ..point }], 1));
    assert!(!points_admitted(&[Point { offset: 1, ..point }, point], 2));
    let all: Vec<_> = (0..2048).map(|offset| Point { offset, ..point }).collect();
    assert!(points_admitted(&all, 2048) && !points_admitted(&all, 2047));
    let mut too_many = all;
    too_many.push(Point {
        offset: 2048,
        ..point
    });
    assert!(!points_admitted(&too_many, 2049));
    let mut groups: Vec<_> = (0..64).map(|id| Point { id, ..point }).collect();
    assert!(points_admitted(&groups, 1));
    groups.push(Point { id: 64, ..point });
    assert!(!points_admitted(&groups, 1));
}

#[test]
fn diagnostic_attempt_exception_and_swallowed_queue_fault_remain_independent() {
    let initial = Report::initial();
    assert_eq!(Report::admit(initial).unwrap().returned_status(), None);
    let success = RawReport {
        stage: 8,
        flags: 3,
        ..initial
    };
    assert_eq!(accepted(0, success).unwrap().returned_status(), Some(0));
    let swallowed = RawReport {
        stage: 8,
        failure: 1,
        flags: 11,
        ..initial
    };
    let Error::Provider { status, report } = accepted(3, swallowed).unwrap_err() else {
        panic!()
    };
    assert_eq!(status, 3);
    assert_eq!(report.returned_status(), Some(0));
    assert_eq!(report.failure(), 1);
    assert!(report.invalidated());
    let thrown = RawReport {
        stage: 8,
        flags: 13,
        native_result: -999,
        ..initial
    };
    let Error::Provider { report, .. } = accepted(3, thrown).unwrap_err() else {
        panic!()
    };
    assert!(report.threw());
    assert_eq!(report.returned_status(), None);
    for invalid in [
        RawReport {
            struct_size: 0,
            ..initial
        },
        RawReport {
            flags: 2,
            ..initial
        },
        RawReport {
            flags: 1,
            ..initial
        },
        RawReport {
            flags: 7,
            ..initial
        },
        RawReport {
            flags: 16,
            ..initial
        },
        RawReport {
            stage: 10,
            ..initial
        },
        RawReport {
            stage: 121,
            ..initial
        },
        RawReport {
            failure: 4,
            ..initial
        },
        RawReport {
            failure: 22,
            ..initial
        },
    ] {
        assert_eq!(Report::admit(invalid), Err(Error::Abi));
    }
    assert_eq!(accepted(0, swallowed), Err(Error::Abi));
    let prepare_throw = RawReport {
        stage: 9,
        flags: 5,
        native_result: -999,
        ..initial
    };
    let Error::Provider { report, .. } = accepted(3, prepare_throw).unwrap_err() else {
        panic!()
    };
    assert_eq!(report.stage(), 9);
    assert!(report.threw());
    assert_eq!(report.returned_status(), None);
    assert!(
        Report::admit(RawReport {
            stage: 120,
            ..initial
        })
        .is_ok()
    );
}

#[test]
fn full_point_validation_and_diagnostic_admission_never_touch_heap() {
    let points: [Point; 2048] = std::array::from_fn(|i| Point {
        id: (i % 64) as u32,
        offset: i as u32,
        value: 0.5,
    });
    let counts = allocation_counter::measure(|| {
        for _ in 0..1000 {
            assert!(points_admitted(&points, 2048));
            assert!(
                accepted(
                    0,
                    RawReport {
                        stage: 8,
                        flags: 3,
                        ..Report::initial()
                    }
                )
                .is_ok()
            );
        }
    });
    assert_eq!(counts.count_total, 0);
    assert_eq!(counts.count_current, 0);
    assert_eq!(counts.bytes_current, 0);
}

// Independent trusted provider facade. No SDK/module/context is acquired here;
// the opaque handle borrows this test's stack state and cleanup is a no-op.
struct EditFixture {
    mode: std::cell::Cell<u32>,
    edits: std::cell::Cell<usize>,
    preparations: std::cell::Cell<usize>,
    processes: std::cell::Cell<usize>,
}
unsafe extern "C" fn fixture_open(_: *const u8, _: u32, _: *mut *mut c_void, _: *mut i32) -> i32 {
    1
}
unsafe extern "C" fn fixture_factory(_: *const c_void, _: *mut crate::vst3::FactoryRecord) -> i32 {
    1
}
unsafe extern "C" fn fixture_class(
    _: *const c_void,
    _: u32,
    _: *mut crate::vst3::ClassRecord,
) -> i32 {
    1
}
unsafe extern "C" fn fixture_create(
    _: *mut c_void,
    _: u32,
    _: *mut *mut c_void,
    _: *mut i32,
) -> i32 {
    1
}
unsafe extern "C" fn fixture_ok(_: *mut c_void, output: *mut i32) -> i32 {
    unsafe {
        *output = 0;
    }
    0
}
unsafe extern "C" fn fixture_prepare(_: *mut c_void, _: *const Profile, _: *mut i32) -> i32 {
    1
}
unsafe extern "C" fn fixture_begin(
    handle: *mut c_void,
    output: *mut *mut c_void,
    result: *mut i32,
) -> i32 {
    unsafe {
        *output = handle;
        *result = 0;
    }
    0
}
unsafe extern "C" fn fixture_plain_process(_: *mut c_void, _: *const Block, _: *mut i32) -> i32 {
    1
}
unsafe extern "C" fn fixture_requests(_: *const c_void, _: *mut u32) -> i32 {
    1
}
unsafe extern "C" fn fixture_refresh(_: *mut c_void, _: *mut u32, _: *mut RawReport) -> i32 {
    1
}
unsafe extern "C" fn fixture_info(_: *const c_void, _: u32, _: *mut RawRecord) -> i32 {
    1
}
unsafe extern "C" fn fixture_format(
    _: *mut c_void,
    _: u32,
    _: f64,
    _: *mut u8,
    _: u32,
    _: *mut u32,
    _: *mut RawReport,
) -> i32 {
    1
}
unsafe extern "C" fn fixture_parse(
    _: *mut c_void,
    _: u32,
    _: *const u8,
    _: u32,
    _: *mut f64,
    _: *mut RawReport,
) -> i32 {
    1
}
unsafe extern "C" fn fixture_parameter_prepare(
    handle: *mut c_void,
    _: *const Profile,
    report: *mut RawReport,
) -> i32 {
    let state = unsafe { &*handle.cast::<EditFixture>() };
    state.preparations.set(state.preparations.get() + 1);
    unsafe {
        *report = Report::initial();
    }
    0
}
unsafe extern "C" fn fixture_value(
    handle: *mut c_void,
    _: u32,
    operation: u32,
    input: f64,
    output: *mut f64,
    report: *mut RawReport,
) -> i32 {
    let state = unsafe { &*handle.cast::<EditFixture>() };
    assert_eq!(operation, 3);
    state.edits.set(state.edits.get() + 1);
    unsafe {
        *output = input;
        *report = Report::initial();
        match state.mode.get() {
            1 => (*report).flags = 2,  // Returned without attempted.
            2 => (*report).flags = 16, // Unknown ABI flag.
            3 => *output = f64::NAN,
            4 => *output = 1.1,
            5 => {
                (*report).flags = 3;
                (*report).native_result = 4242;
            }
            _ => (),
        }
    }
    0 // Deliberately claims provider success in every case.
}
unsafe extern "C" fn fixture_parameter_process(
    handle: *mut c_void,
    _: *const Block,
    _: *const Point,
    _: u32,
    report: *mut RawReport,
) -> i32 {
    let state = unsafe { &*handle.cast::<EditFixture>() };
    state.processes.set(state.processes.get() + 1);
    unsafe {
        *report = RawReport {
            stage: 8,
            flags: 3,
            ..Report::initial()
        };
    }
    0
}
fn fixture_instance_table() -> super::super::RawProvider {
    super::super::RawProvider {
        struct_size: size_of::<super::super::RawProvider>() as u32,
        version: 2,
        open: Some(fixture_open),
        factory: Some(fixture_factory),
        class_info: Some(fixture_class),
        close: Some(fixture_ok),
        create: Some(fixture_create),
        prepare: Some(fixture_prepare),
        start: Some(fixture_ok),
        begin_audio: Some(fixture_begin),
        process: Some(fixture_plain_process),
        end_audio: Some(fixture_ok),
        stop: Some(fixture_ok),
        destroy: Some(fixture_ok),
        restart_requests: Some(fixture_requests),
    }
}
fn fixture_parameter_table(table: &super::super::RawProvider) -> RawProvider {
    RawProvider {
        struct_size: size_of::<RawProvider>() as u32,
        version: 1,
        provider: table,
        refresh: Some(fixture_refresh),
        info: Some(fixture_info),
        value: Some(fixture_value),
        format: Some(fixture_format),
        parse: Some(fixture_parse),
        prepare: Some(fixture_parameter_prepare),
        process: Some(fixture_parameter_process),
    }
}
#[test]
fn malformed_successful_foreign_edit_invalidates_preparation_before_audio() {
    use super::super::{Library, Provider as InstanceProvider};
    use std::{cell::Cell, ptr::NonNull};
    let table = fixture_instance_table();
    let raw = fixture_parameter_table(&table);
    // SAFETY: complete trusted mock tables remain borrowed for the entire scope.
    let provider = unsafe { Provider::from_raw(&raw).unwrap() };
    let library = Library {
        provider: unsafe { InstanceProvider::from_raw(&table).unwrap() },
        handle: None,
        classes: Vec::new(),
        factory: None,
    };
    let state = EditFixture {
        mode: Cell::new(0),
        edits: Cell::new(0),
        preparations: Cell::new(0),
        processes: Cell::new(0),
    };
    let mut child = Instance {
        library: crate::vst3::instances::Parent::Borrowed(&library),
        handle: Some(NonNull::from(&state).cast()),
        profile: None,
        running: false,
        poisoned: false,
        state_fault: false,
        parameter_provider: None,
        processing_provider: None,
        processing_metadata: None,
        hosted: Default::default(),
        class_id: [1; 16],
    };
    let profile = Profile::new(48000., 16, 0, 1);
    for mode in 1..=5 {
        child.prepare_parameters(provider, profile).unwrap();
        assert!(child.parameter_provider.is_some());
        state.mode.set(mode);
        assert_eq!(child.edit_parameter(provider, 1, 0.5), Err(Error::Abi));
        assert!(child.parameter_provider.is_none());
        child.start().unwrap();
        child
            .with_audio(|mut audio| {
                let mut samples = [42.; 16];
                assert_eq!(
                    audio.process_parameters(&[], &mut [&mut samples], 16, &[]),
                    Err(Error::Lifecycle)
                );
                assert_eq!(samples, [42.; 16]);
            })
            .unwrap();
        child.stop().unwrap();
    }
    assert_eq!(state.preparations.get(), 5);
    assert_eq!(state.edits.get(), 5);
    assert_eq!(state.processes.get(), 0);

    // Local preflight and successful edits preserve the already admitted table.
    child.prepare_parameters(provider, profile).unwrap();
    for (id, value) in [(u32::MAX, 0.5), (1, f64::NAN), (1, 1.1)] {
        assert_eq!(child.edit_parameter(provider, id, value), Err(Error::Shape));
        assert!(child.parameter_provider.is_some());
    }
    assert_eq!(state.edits.get(), 5);
    state.mode.set(0);
    child.edit_parameter(provider, 1, 0.75).unwrap();
    assert!(child.parameter_provider.is_some());
    child.start().unwrap();
    child
        .with_audio(|mut audio| {
            let mut samples = [42.; 16];
            audio
                .process_parameters(&[], &mut [&mut samples], 16, &[])
                .unwrap();
        })
        .unwrap();
    child.stop().unwrap();
    assert_eq!(state.processes.get(), 1);
}
