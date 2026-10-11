//! Trusted in-process mock of the paired v2 instance and editor companion ABI.
use super::*;
use crate::vst3::instances::{
    Block, InstanceRetirement, Library, Profile, Provider as InstanceTable,
};
use crate::vst3::{ClassRecord, FactoryRecord};
use std::{cell::RefCell, mem::offset_of};

#[derive(Default)]
struct Mock {
    calls: Vec<&'static str>,
    create_status: i32,
    create_report: (u32, u32, i32, u32),
    create_handle_on_error: bool,
    initial: (u32, u32),
    attached: bool,
    size: (u32, u32),
    resizable: bool,
    request: Option<(u32, u32)>,
    detach_refuse: bool,
    destroy_retain: Vec<i32>,
    destroy_fail: bool,
    malformed_report: bool,
    faulted: bool,
    stateless_refusal: bool,
    live_editors: i32,
    destroyed_editors: i32,
    destroyed_children: i32,
}
thread_local! {
    static MOCK: RefCell<Mock> = RefCell::new(Mock {
        initial: (400, 300),
        size: (400, 300),
        resizable: true,
        ..Mock::default()
    });
}
fn with<R>(f: impl FnOnce(&mut Mock) -> R) -> R {
    MOCK.with(|mock| f(&mut mock.borrow_mut()))
}
fn reset() {
    with(|m| {
        *m = Mock {
            initial: (400, 300),
            size: (400, 300),
            resizable: true,
            ..Mock::default()
        }
    });
}
fn calls(name: &str) -> usize {
    with(|m| m.calls.iter().filter(|call| **call == name).count())
}
struct Child {
    editor: bool,
}
struct MockEditor {
    child: *mut Child,
}
unsafe fn write(report: *mut RawReport, stage: u32, failure: u32, native: i32, flags: u32) {
    let malformed = with(|m| m.malformed_report);
    unsafe {
        *report = RawReport {
            struct_size: size_of::<RawReport>() as u32,
            stage: if malformed { 999 } else { stage },
            failure,
            flags,
            native_result: native,
            reserved: 0,
        };
    }
}
/// Native owner replies carry STATE plus the true ATTACHED bit.
fn attached_flag() -> u32 {
    FLAG_STATE
        | if with(|m| m.attached) {
            FLAG_ATTACHED
        } else {
            0
        }
}
/// Native owner refusals (fault) or a stateless provider-level refusal.
unsafe fn refusal(report: *mut RawReport) -> Option<i32> {
    if with(|m| m.stateless_refusal) {
        unsafe { write(report, 205, 1, 0, 0) };
        return Some(1);
    }
    if with(|m| m.faulted) {
        unsafe { write(report, 200, 9, 0, attached_flag()) };
        return Some(STATUS_LIFECYCLE);
    }
    None
}
unsafe extern "C" fn open(_: *const u8, _: u32, out: *mut *mut c_void, result: *mut i32) -> i32 {
    unsafe {
        *out = Box::into_raw(Box::new(0u8)).cast();
        *result = 0;
    }
    0
}
unsafe extern "C" fn factory(_: *const c_void, out: *mut FactoryRecord) -> i32 {
    unsafe {
        (*out).class_count = 1;
        (&mut (*out).vendor)[..5].copy_from_slice(b"Mock\0");
    }
    0
}
unsafe extern "C" fn class(_: *const c_void, _: u32, out: *mut ClassRecord) -> i32 {
    unsafe {
        (*out).id = [3; 16];
        (*out).cardinality = 1;
        (&mut (*out).category)[..19].copy_from_slice(b"Audio Module Class\0");
        (&mut (*out).name)[..5].copy_from_slice(b"Mock\0");
        (&mut (*out).vendor)[..5].copy_from_slice(b"Mock\0");
    }
    0
}
unsafe extern "C" fn close(parent: *mut c_void, result: *mut i32) -> i32 {
    unsafe {
        drop(Box::from_raw(parent.cast::<u8>()));
        *result = 0;
    }
    0
}
unsafe extern "C" fn create(
    _: *mut c_void,
    _: u32,
    out: *mut *mut c_void,
    result: *mut i32,
) -> i32 {
    unsafe {
        *out = Box::into_raw(Box::new(Child { editor: false })).cast();
        *result = 0;
    }
    0
}
unsafe extern "C" fn prepare(_: *mut c_void, _: *const Profile, result: *mut i32) -> i32 {
    unsafe { *result = 0 };
    0
}
unsafe extern "C" fn ok(_: *mut c_void, result: *mut i32) -> i32 {
    unsafe { *result = 0 };
    0
}
unsafe extern "C" fn begin(_: *mut c_void, _: *mut *mut c_void, result: *mut i32) -> i32 {
    unsafe { *result = 0 };
    4
}
unsafe extern "C" fn process(_: *mut c_void, _: *const Block, result: *mut i32) -> i32 {
    unsafe { *result = 0 };
    4
}
unsafe extern "C" fn destroy_child(child: *mut c_void, result: *mut i32) -> i32 {
    unsafe { *result = 0 };
    let child = child.cast::<Child>();
    // Native rule: the child cannot retire while its editor lives.
    if unsafe { (*child).editor } {
        return STATUS_LIFECYCLE;
    }
    unsafe { drop(Box::from_raw(child)) };
    with(|m| m.destroyed_children += 1);
    0
}
unsafe extern "C" fn restart(_: *const c_void, out: *mut u32) -> i32 {
    unsafe { *out = 0 };
    0
}
static INSTANCE_TABLE: InstanceProvider = InstanceProvider {
    struct_size: size_of::<InstanceProvider>() as u32,
    version: 2,
    open: Some(open),
    factory: Some(factory),
    class_info: Some(class),
    close: Some(close),
    create: Some(create),
    prepare: Some(prepare),
    start: Some(ok),
    begin_audio: Some(begin),
    process: Some(process),
    end_audio: Some(ok),
    stop: Some(ok),
    destroy: Some(destroy_child),
    restart_requests: Some(restart),
};
static OTHER_TABLE: InstanceProvider = InstanceProvider {
    struct_size: size_of::<InstanceProvider>() as u32,
    version: 2,
    open: Some(open),
    factory: Some(factory),
    class_info: Some(class),
    close: Some(close),
    create: Some(create),
    prepare: Some(prepare),
    start: Some(ok),
    begin_audio: Some(begin),
    process: Some(process),
    end_audio: Some(ok),
    stop: Some(ok),
    destroy: Some(destroy_child),
    restart_requests: Some(restart),
};
unsafe extern "C" fn editor_create(
    child: *mut c_void,
    platform: u32,
    out: *mut *mut c_void,
    initial: *mut RawSize,
    report: *mut RawReport,
) -> i32 {
    with(|m| m.calls.push("create"));
    assert_eq!(platform, 1);
    let (status, (stage, failure, native, flags), on_error, size) = with(|m| {
        (
            m.create_status,
            m.create_report,
            m.create_handle_on_error,
            m.initial,
        )
    });
    let child = child.cast::<Child>();
    if status != 0 && !on_error {
        unsafe { write(report, stage, failure, native, flags) };
        return status;
    }
    unsafe {
        (*child).editor = true;
        *out = Box::into_raw(Box::new(MockEditor { child })).cast();
        (*initial).width = size.0;
        (*initial).height = size.1;
    }
    with(|m| m.live_editors += 1);
    if status != 0 {
        unsafe { write(report, stage, failure, native, flags) };
        return status;
    }
    unsafe { write(report, 204, 0, 0, 0) };
    0
}
unsafe extern "C" fn editor_attach(
    _: *mut c_void,
    parent: *mut c_void,
    applied: *mut RawSize,
    report: *mut RawReport,
) -> i32 {
    with(|m| m.calls.push("attach"));
    assert!(!parent.is_null());
    if let Some(status) = unsafe { refusal(report) } {
        return status;
    }
    if with(|m| m.attached) {
        unsafe { write(report, 205, 9, 0, attached_flag()) };
        return STATUS_LIFECYCLE;
    }
    let size = with(|m| {
        m.attached = true;
        m.size = (500, 350); // a plug-in resize deferred during attached
        m.request = Some(m.size);
        m.size
    });
    unsafe {
        (*applied).width = size.0;
        (*applied).height = size.1;
        write(
            report,
            205,
            0,
            0,
            FLAG_STATE | FLAG_ATTACHED | FLAG_DEFERRED,
        );
    }
    0
}
unsafe extern "C" fn editor_size(_: *mut c_void, out: *mut RawSize, report: *mut RawReport) -> i32 {
    with(|m| m.calls.push("size"));
    if let Some(status) = unsafe { refusal(report) } {
        return status;
    }
    let size = with(|m| m.size);
    unsafe {
        (*out).width = size.0;
        (*out).height = size.1;
        write(report, 204, 0, 0, attached_flag());
    }
    0
}
unsafe extern "C" fn editor_resize(
    _: *mut c_void,
    proposed: *const RawSize,
    applied: *mut RawSize,
    report: *mut RawReport,
) -> i32 {
    with(|m| m.calls.push("resize"));
    if let Some(status) = unsafe { refusal(report) } {
        return status;
    }
    if !with(|m| m.resizable) {
        unsafe { write(report, 206, 8, 0, attached_flag()) };
        return STATUS_UNSUPPORTED;
    }
    let (w, h) = unsafe {
        (
            (*proposed).width.clamp(200, 1200),
            (*proposed).height.clamp(150, 900),
        )
    };
    with(|m| m.size = (w, h));
    unsafe {
        (*applied).width = w;
        (*applied).height = h;
        write(report, 208, 0, 0, attached_flag());
    }
    0
}
unsafe extern "C" fn editor_scale(
    _: *mut c_void,
    _: f32,
    _: *mut RawSize,
    report: *mut RawReport,
) -> i32 {
    with(|m| m.calls.push("scale"));
    if let Some(status) = unsafe { refusal(report) } {
        return status;
    }
    unsafe { write(report, 209, 8, 0, attached_flag()) };
    STATUS_UNSUPPORTED
}
unsafe extern "C" fn editor_detach(_: *mut c_void, report: *mut RawReport) -> i32 {
    with(|m| m.calls.push("detach"));
    if let Some(status) = unsafe { refusal(report) } {
        return status;
    }
    if !with(|m| m.attached) {
        unsafe { write(report, 210, 9, 0, attached_flag()) };
        return STATUS_LIFECYCLE;
    }
    if with(|m| m.detach_refuse) {
        unsafe { write(report, 210, 2, 4242, FLAG_RETURNED | attached_flag()) };
        return STATUS_NATIVE;
    }
    with(|m| m.attached = false);
    unsafe { write(report, 210, 0, 0, attached_flag()) };
    0
}
unsafe extern "C" fn editor_state(_: *const c_void, out: *mut RawState) -> i32 {
    with(|m| m.calls.push("state"));
    let (attached, size, pending) = with(|m| (m.attached, m.size, m.request.is_some()));
    unsafe {
        (*out).flags = u32::from(attached) | 2 | if pending { 8 } else { 0 };
        (*out).width = size.0;
        (*out).height = size.1;
        (*out).frame_calls = 3;
        (*out).frame_applied = 1;
        (*out).frame_refused = 2;
        (*out).frame_reentry_refused = 1;
        (*out).deferred_discarded = 4;
        (*out).size_mismatches = 5;
    }
    0
}
unsafe extern "C" fn editor_take(_: *mut c_void, out: *mut RawSize) -> i32 {
    with(|m| m.calls.push("take"));
    match with(|m| m.request.take()) {
        Some((w, h)) => {
            unsafe {
                (*out).width = w;
                (*out).height = h;
            }
            0
        }
        None => STATUS_EMPTY,
    }
}
unsafe extern "C" fn editor_destroy(editor: *mut c_void, report: *mut RawReport) -> i32 {
    with(|m| m.calls.push("destroy"));
    if let Some(status) = with(|m| m.destroy_retain.pop()) {
        return status; // retained: THREAD/REENTRY write no report
    }
    let fail = with(|m| m.destroy_fail);
    let editor = unsafe { Box::from_raw(editor.cast::<MockEditor>()) };
    unsafe { (*editor.child).editor = false };
    with(|m| {
        m.attached = false;
        m.live_editors -= 1;
        m.destroyed_editors += 1;
    });
    if fail {
        unsafe { write(report, 210, 2, 4242, FLAG_RETURNED | FLAG_STATE) };
        return STATUS_RETIRED_NATIVE;
    }
    unsafe { write(report, 212, 0, 0, FLAG_STATE) };
    0
}
fn editor_table(origin: &'static InstanceProvider) -> RawProvider {
    RawProvider {
        struct_size: size_of::<RawProvider>() as u32,
        version: 1,
        instance_origin: origin,
        create: Some(editor_create),
        attach: Some(editor_attach),
        size: Some(editor_size),
        resize: Some(editor_resize),
        set_scale: Some(editor_scale),
        detach: Some(editor_detach),
        state: Some(editor_state),
        take_request: Some(editor_take),
        destroy: Some(editor_destroy),
    }
}
fn library() -> Library<'static> {
    // SAFETY: trusted in-process mock implementing the exact v2 ABI.
    let provider = unsafe { InstanceTable::from_raw(&INSTANCE_TABLE).unwrap() };
    unsafe { provider.load("mock.vst3").unwrap() }
}
fn table() -> &'static RawProvider {
    Box::leak(Box::new(editor_table(&INSTANCE_TABLE)))
}
fn provider(raw: &'static RawProvider) -> Provider<'static> {
    // SAFETY: trusted in-process mock implementing the exact editor ABI.
    unsafe { Provider::from_raw(raw).unwrap() }
}
fn parent() -> NonNull<c_void> {
    NonNull::dangling() // the mock never dereferences the caller's view
}

#[test]
fn layouts_match_the_c_abi() {
    assert_eq!(size_of::<RawSize>(), 16);
    assert_eq!(size_of::<RawReport>(), 24);
    assert_eq!(offset_of!(RawReport, native_result), 16);
    assert_eq!(size_of::<RawState>(), 40);
    assert_eq!(offset_of!(RawState, frame_reentry_refused), 28);
    assert_eq!(offset_of!(RawState, size_mismatches), 36);
    assert_eq!(size_of::<RawProvider>(), 88);
    assert_eq!(offset_of!(RawProvider, instance_origin), 8);
    assert_eq!(offset_of!(RawProvider, create), 16);
    assert_eq!(offset_of!(RawProvider, destroy), 80);
    assert_eq!(Size::new(0, 1), None);
    assert_eq!(Size::new(1, 16385), None);
    assert_eq!(Size::new(16384, 1).map(Size::width), Some(16384));
}
#[test]
fn table_admission_rejects_every_malformed_field() {
    let valid = editor_table(&INSTANCE_TABLE);
    let check = |mutate: &dyn Fn(&mut RawProvider)| {
        let mut table = editor_table(&INSTANCE_TABLE);
        mutate(&mut table);
        // SAFETY: rejected before any entry is called.
        matches!(unsafe { Provider::from_raw(&table) }, Err(Error::Abi))
    };
    assert!(unsafe { Provider::from_raw(&valid) }.is_ok());
    assert!(check(&|t| t.struct_size = 80));
    assert!(check(&|t| t.version = 2));
    assert!(check(&|t| t.instance_origin = std::ptr::null()));
    assert!(check(&|t| t.create = None));
    assert!(check(&|t| t.attach = None));
    assert!(check(&|t| t.size = None));
    assert!(check(&|t| t.resize = None));
    assert!(check(&|t| t.set_scale = None));
    assert!(check(&|t| t.detach = None));
    assert!(check(&|t| t.state = None));
    assert!(check(&|t| t.take_request = None));
    assert!(check(&|t| t.destroy = None));
}
#[test]
fn foreign_origin_and_poisoned_instance_refuse_before_native_calls() {
    reset();
    let library = library();
    let mut instance = library.create(0).unwrap();
    let foreign = Box::leak(Box::new(editor_table(&OTHER_TABLE)));
    assert!(matches!(
        instance.open_editor(provider(foreign), Platform::NsView),
        Err(Error::Abi)
    ));
    instance.state_fault = true;
    assert!(matches!(
        instance.open_editor(provider(table()), Platform::NsView),
        Err(Error::Poisoned)
    ));
    instance.state_fault = false;
    assert_eq!(calls("create"), 0);
}
#[test]
fn lifecycle_resize_requests_state_and_ordered_close() {
    reset();
    let library = library();
    let mut instance = library.create(0).unwrap();
    let mut editor = instance
        .open_editor(provider(table()), Platform::NsView)
        .unwrap();
    assert_eq!(editor.last_size(), Size::new(400, 300).unwrap());
    assert!(!editor.attached());
    // SAFETY: the mock never dereferences the parent.
    let applied = unsafe { editor.attach(parent()) }.unwrap();
    assert_eq!(applied, Size::new(500, 350).unwrap());
    assert!(editor.attached());
    assert_eq!(editor.take_resize_request().unwrap(), Size::new(500, 350));
    assert_eq!(editor.take_resize_request().unwrap(), None);
    assert_eq!(
        editor.resize(Size::new(100, 100).unwrap()).unwrap(),
        Size::new(200, 150).unwrap()
    );
    assert_eq!(editor.size().unwrap(), Size::new(200, 150).unwrap());
    match editor.set_content_scale(2.0) {
        Err(Error::Unsupported(report)) => {
            assert_eq!(
                (report.stage(), report.failure(), report.native_result()),
                (209, Failure::Unsupported, None)
            );
            assert_eq!(report.attached(), Some(true));
        }
        other => panic!("unexpected {other:?}"),
    }
    let state = editor.state().unwrap();
    assert!(
        state.attached()
            && state.resizable()
            && !state.content_scale_supported()
            && !state.faulted()
    );
    assert_eq!(state.size(), Size::new(200, 150));
    assert_eq!(
        (
            state.frame_calls(),
            state.frame_applied(),
            state.frame_refused(),
            state.frame_reentry_refused()
        ),
        (3, 1, 2, 1)
    );
    assert_eq!(
        (
            state.deferred_discarded(),
            state.size_mismatches(),
            state.deferred_request_pending()
        ),
        (4, 5, false)
    );
    // SAFETY: as above.
    assert!(matches!(
        unsafe { editor.attach(parent()) },
        Err(Error::Lifecycle(Some(_)))
    ));
    assert!(editor.instance().restart_requests().is_ok());
    editor.detach().unwrap();
    assert!(!editor.attached());
    assert_eq!(editor.close().ok(), Some(Closed::Complete));
    assert_eq!(with(|m| (m.live_editors, m.destroyed_editors)), (0, 1));
    assert_eq!(instance.retire().ok(), Some(InstanceRetirement::Complete));
    assert_eq!(with(|m| m.destroyed_children), 1);
}
#[test]
fn drop_destroys_once_even_while_attached() {
    reset();
    let library = library();
    let mut instance = library.create(0).unwrap();
    {
        let mut editor = instance
            .open_editor(provider(table()), Platform::NsView)
            .unwrap();
        // SAFETY: the mock never dereferences the parent.
        unsafe { editor.attach(parent()) }.unwrap();
    }
    assert_eq!(calls("destroy"), 1);
    assert_eq!(with(|m| m.live_editors), 0);
    drop(instance);
    assert_eq!(with(|m| m.destroyed_children), 1);
}
#[test]
fn unavailable_unsupported_and_native_creation_failures_publish_no_guard() {
    reset();
    let library = library();
    let mut instance = library.create(0).unwrap();
    for (status, report, expected) in [
        (STATUS_UNAVAILABLE, (201, 7, 0, 0), "unavailable"),
        (STATUS_UNSUPPORTED, (202, 8, 0, 0), "unsupported"),
        (STATUS_NATIVE, (203, 2, 4242, FLAG_RETURNED), "returned"),
        (STATUS_NATIVE, (201, 3, 0, FLAG_THROWN), "thrown"),
        (STATUS_METADATA, (204, 6, 0, 0), "malformed"),
    ] {
        with(|m| {
            m.create_status = status;
            m.create_report = report;
        });
        let error = instance
            .open_editor(provider(table()), Platform::NsView)
            .err()
            .unwrap();
        match (expected, error) {
            ("unavailable", Error::Unavailable(r)) => {
                assert_eq!((r.stage(), r.failure()), (201, Failure::Unavailable))
            }
            ("unsupported", Error::Unsupported(r)) => assert_eq!(r.stage(), 202),
            ("returned", Error::Native(r)) => assert_eq!(
                (r.failure(), r.native_result()),
                (Failure::Returned, Some(4242))
            ),
            ("thrown", Error::Native(r)) => {
                assert_eq!((r.failure(), r.native_result()), (Failure::Thrown, None))
            }
            ("malformed", Error::Native(r)) => assert_eq!(r.failure(), Failure::Malformed),
            (name, other) => panic!("{name}: {other:?}"),
        }
    }
    assert_eq!(with(|m| m.live_editors), 0);
    // A handle published together with a failure status is still destroyed.
    with(|m| m.create_handle_on_error = true);
    assert!(matches!(
        instance.open_editor(provider(table()), Platform::NsView),
        Err(Error::Native(_))
    ));
    assert_eq!(with(|m| (m.live_editors, m.destroyed_editors)), (0, 1));
}
#[test]
fn malformed_initial_size_or_report_is_destroyed_and_rejected() {
    reset();
    let library = library();
    let mut instance = library.create(0).unwrap();
    for size in [(0, 10), (10, 16385)] {
        with(|m| m.initial = size);
        assert!(matches!(
            instance.open_editor(provider(table()), Platform::NsView),
            Err(Error::Abi)
        ));
    }
    with(|m| {
        m.initial = (400, 300);
        m.malformed_report = true;
    });
    assert!(matches!(
        instance.open_editor(provider(table()), Platform::NsView),
        Err(Error::Abi)
    ));
    assert_eq!(with(|m| (m.live_editors, m.destroyed_editors)), (0, 3));
}
#[test]
fn local_admission_makes_no_native_call() {
    reset();
    let library = library();
    let mut instance = library.create(0).unwrap();
    let mut editor = instance
        .open_editor(provider(table()), Platform::NsView)
        .unwrap();
    for factor in [f32::NAN, f32::INFINITY, 0.2, 8.5, -1.0] {
        assert!(matches!(editor.set_content_scale(factor), Err(Error::Size)));
    }
    assert_eq!(calls("scale"), 0);
    with(|m| m.resizable = false);
    assert!(matches!(
        editor.resize(Size::new(640, 480).unwrap()),
        Err(Error::Unsupported(_))
    ));
    drop(editor);
}
#[test]
fn detach_refusal_stays_attached_and_retryable() {
    reset();
    let library = library();
    let mut instance = library.create(0).unwrap();
    let mut editor = instance
        .open_editor(provider(table()), Platform::NsView)
        .unwrap();
    // SAFETY: the mock never dereferences the parent.
    unsafe { editor.attach(parent()) }.unwrap();
    with(|m| m.detach_refuse = true);
    match editor.detach() {
        Err(Error::Native(report)) => {
            assert_eq!((report.stage(), report.native_result()), (210, Some(4242)));
            assert_eq!(report.attached(), Some(true));
        }
        other => panic!("unexpected {other:?}"),
    }
    assert!(editor.attached());
    with(|m| m.detach_refuse = false);
    editor.detach().unwrap();
    assert!(matches!(editor.detach(), Err(Error::Lifecycle(Some(_)))));
    drop(editor);
}
#[test]
fn retained_close_returns_the_owner_and_failed_native_close_consumes() {
    reset();
    let library = library();
    let mut instance = library.create(0).unwrap();
    let editor = instance
        .open_editor(provider(table()), Platform::NsView)
        .unwrap();
    with(|m| m.destroy_retain = vec![STATUS_THREAD, STATUS_REENTRY]);
    let CloseFailure { owner, error } = *editor.close().err().unwrap();
    assert_eq!(error, Error::Reentry);
    let CloseFailure { owner, error } = *owner.close().err().unwrap();
    assert_eq!(error, Error::Thread);
    assert_eq!(with(|m| m.live_editors), 1);
    with(|m| m.destroy_fail = true);
    match owner.close() {
        Ok(Closed::FailedNative(report)) => {
            assert_eq!(
                (report.stage(), report.failure(), report.native_result()),
                (210, Failure::Returned, Some(4242))
            )
        }
        other => panic!("unexpected {:?}", other.map_err(|f| f.error)),
    }
    assert_eq!(with(|m| (m.live_editors, m.destroyed_editors)), (0, 1));
    assert_eq!(calls("destroy"), 3);
    // The child retires only after its editor is gone (native rule mirrored).
    assert_eq!(instance.retire().ok(), Some(InstanceRetirement::Complete));
}
#[test]
fn native_refusal_of_child_destruction_while_open_is_mirrored() {
    reset();
    let library = library();
    let mut instance = library.create(0).unwrap();
    let editor = instance
        .open_editor(provider(table()), Platform::NsView)
        .unwrap();
    // Rust prevents retiring the borrowed instance; leaking the guard models a
    // forgotten editor: the native child then refuses destruction and the
    // retire failure returns the owner.
    std::mem::forget(editor);
    let failure = instance.retire().err().unwrap();
    assert_eq!(
        failure.error,
        crate::vst3::instances::Error::Native {
            status: STATUS_LIFECYCLE,
            result: 0
        }
    );
    std::mem::forget(failure); // keep the mock child alive for the leaked editor
}
#[test]
fn refusals_never_clear_the_true_attached_state() {
    reset();
    let library = library();
    let mut instance = library.create(0).unwrap();
    let mut editor = instance
        .open_editor(provider(table()), Platform::NsView)
        .unwrap();
    // SAFETY: the mock never dereferences the parent (all attach calls below).
    unsafe { editor.attach(parent()) }.unwrap();
    // Second attach: LIFECYCLE with the authoritative attached state.
    match unsafe { editor.attach(parent()) } {
        Err(Error::Lifecycle(Some(report))) => assert_eq!(report.attached(), Some(true)),
        other => panic!("unexpected {other:?}"),
    }
    assert!(editor.attached());
    // Sticky fault: every operation is refused and the view stays attached.
    with(|m| m.faulted = true);
    assert!(matches!(
        unsafe { editor.attach(parent()) },
        Err(Error::Lifecycle(Some(_)))
    ));
    assert!(matches!(editor.detach(), Err(Error::Lifecycle(Some(_)))));
    assert!(matches!(
        editor.set_content_scale(1.0),
        Err(Error::Lifecycle(Some(_)))
    ));
    assert!(matches!(editor.size(), Err(Error::Lifecycle(Some(_)))));
    assert!(matches!(
        editor.resize(Size::new(500, 400).unwrap()),
        Err(Error::Lifecycle(Some(_)))
    ));
    assert!(editor.attached());
    // A reply without owner state (provider-level refusal) leaves it unchanged.
    with(|m| {
        m.faulted = false;
        m.stateless_refusal = true;
    });
    match unsafe { editor.attach(parent()) } {
        Err(Error::Status {
            status: 1,
            report: Some(report),
        }) => assert_eq!(report.attached(), None),
        other => panic!("unexpected {other:?}"),
    }
    assert!(matches!(
        editor.detach(),
        Err(Error::Status { status: 1, .. })
    ));
    assert!(editor.attached());
    // A truthful detached reply does clear it.
    with(|m| m.stateless_refusal = false);
    editor.detach().unwrap();
    assert!(!editor.attached());
    assert_eq!(editor.close().ok(), Some(Closed::Complete));
}
#[test]
fn report_admission_rejects_attached_without_owner_state() {
    let raw = RawReport {
        struct_size: size_of::<RawReport>() as u32,
        stage: 205,
        failure: 9,
        flags: FLAG_ATTACHED,
        native_result: 0,
        reserved: 0,
    };
    assert_eq!(Report::admit(raw), Err(Error::Abi));
    assert_eq!(
        Report::admit(RawReport {
            flags: FLAG_DEFERRED,
            ..raw
        }),
        Err(Error::Abi)
    );
    assert_eq!(
        Report::admit(RawReport {
            flags: FLAG_STATE | FLAG_ATTACHED,
            ..raw
        })
        .map(|r| r.attached()),
        Ok(Some(true))
    );
    assert_eq!(
        Report::admit(RawReport {
            flags: FLAG_STATE,
            ..raw
        })
        .map(|r| r.attached()),
        Ok(Some(false))
    );
    assert_eq!(
        Report::admit(RawReport { flags: 32, ..raw }),
        Err(Error::Abi)
    );
}
#[test]
fn refused_drop_keeps_the_native_editor_alive_and_is_recorded() {
    reset();
    let library = library();
    let mut instance = library.create(0).unwrap();
    let editor = instance
        .open_editor(provider(table()), Platform::NsView)
        .unwrap();
    let before = refused_drops();
    with(|m| m.destroy_retain = vec![STATUS_REENTRY]);
    drop(editor);
    assert_eq!(refused_drops(), before + 1);
    // Kept alive deterministically: not destroyed, so the child refuses retirement.
    assert_eq!(with(|m| (m.live_editors, m.destroyed_editors)), (1, 0));
    let failure = instance.retire().err().unwrap();
    assert_eq!(
        failure.error,
        crate::vst3::instances::Error::Native {
            status: STATUS_LIFECYCLE,
            result: 0
        }
    );
    std::mem::forget(failure); // the retained native editor still references the mock child
    // A consumed drop does not count.
    reset();
    let mut instance = library.create(0).unwrap();
    let before = refused_drops();
    drop(
        instance
            .open_editor(provider(table()), Platform::NsView)
            .unwrap(),
    );
    assert_eq!(refused_drops(), before);
}
