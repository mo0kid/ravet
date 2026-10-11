use super::{Error, InstanceRetirement, Profile, Provider, RawProvider, Retirement};
use crate::vst3::{ClassRecord, FactoryRecord};
use std::{
    ffi::c_void,
    sync::atomic::{AtomicI32, AtomicUsize, Ordering},
};

static MODE: AtomicI32 = AtomicI32::new(0);
static OPENS: AtomicUsize = AtomicUsize::new(0);
static CLOSES: AtomicUsize = AtomicUsize::new(0);
static DESTROYS: AtomicUsize = AtomicUsize::new(0);
static ENDS: AtomicUsize = AtomicUsize::new(0);
static PROCESSES: AtomicUsize = AtomicUsize::new(0);
static WRONG_THREAD: AtomicUsize = AtomicUsize::new(0);
static MAIN: std::sync::OnceLock<std::thread::ThreadId> = std::sync::OnceLock::new();
struct Child {
    parent: *mut c_void,
    attached: bool,
}
fn control() {
    if Some(&std::thread::current().id()) != MAIN.get() {
        WRONG_THREAD.fetch_add(1, Ordering::Relaxed);
    }
}
unsafe extern "C" fn open(_: *const u8, _: u32, out: *mut *mut c_void, result: *mut i32) -> i32 {
    control();
    OPENS.fetch_add(1, Ordering::Relaxed);
    unsafe {
        *out = Box::into_raw(Box::new(0usize)).cast();
        *result = 0;
    }
    0
}
unsafe extern "C" fn factory(_: *const c_void, out: *mut FactoryRecord) -> i32 {
    control();
    unsafe {
        (*out).class_count = 1;
        (&mut (*out).vendor)[..8].copy_from_slice(b"Fixture\0");
    }
    0
}
unsafe extern "C" fn class(_: *const c_void, _: u32, out: *mut ClassRecord) -> i32 {
    control();
    unsafe {
        (*out).id = [1; 16];
        (*out).cardinality = 1;
        (&mut (*out).category)[..19].copy_from_slice(b"Audio Module Class\0");
        (&mut (*out).name)[..8].copy_from_slice(b"Fixture\0");
        (&mut (*out).vendor)[..8].copy_from_slice(b"Fixture\0");
        if MODE.load(Ordering::Relaxed) == 4 {
            (*out).name.fill(b'x');
        }
    }
    0
}
unsafe extern "C" fn close(parent: *mut c_void, result: *mut i32) -> i32 {
    control();
    CLOSES.fetch_add(1, Ordering::Relaxed);
    unsafe {
        *result = 4242;
    }
    if MODE
        .compare_exchange(2, 0, Ordering::Relaxed, Ordering::Relaxed)
        .is_ok()
    {
        return 8;
    }
    // Real provider preserves a core for pinned close; this trusted mock consumes
    // only its facade and reports the same public ownership status.
    unsafe {
        drop(Box::from_raw(parent.cast::<usize>()));
    }
    match MODE.load(Ordering::Relaxed) {
        1 => 7,
        3 => 9,
        _ => 0,
    }
}
unsafe extern "C" fn create(
    parent: *mut c_void,
    _: u32,
    out: *mut *mut c_void,
    result: *mut i32,
) -> i32 {
    control();
    unsafe {
        *out = Box::into_raw(Box::new(Child {
            parent,
            attached: false,
        }))
        .cast();
        *result = 0;
    }
    0
}
unsafe extern "C" fn prepare(_: *mut c_void, _: *const Profile, result: *mut i32) -> i32 {
    control();
    unsafe {
        *result = 0;
    }
    0
}
unsafe extern "C" fn ok(_: *mut c_void, result: *mut i32) -> i32 {
    control();
    unsafe {
        *result = 0;
    }
    0
}
unsafe extern "C" fn begin(child: *mut c_void, out: *mut *mut c_void, result: *mut i32) -> i32 {
    control();
    unsafe {
        (*child.cast::<Child>()).attached = true;
        *out = child;
        *result = 0;
    }
    0
}
unsafe extern "C" fn process(_: *mut c_void, _: *const super::Block, result: *mut i32) -> i32 {
    PROCESSES.fetch_add(1, Ordering::Relaxed);
    unsafe {
        *result = 0;
    }
    0
}
unsafe extern "C" fn end(child: *mut c_void, result: *mut i32) -> i32 {
    control();
    ENDS.fetch_add(1, Ordering::Relaxed);
    unsafe {
        *result = 4242;
    }
    if MODE.load(Ordering::Relaxed) == 5 {
        return 10;
    }
    unsafe {
        (*child.cast::<Child>()).attached = false;
    }
    0
}
unsafe extern "C" fn destroy(child: *mut c_void, result: *mut i32) -> i32 {
    control();
    DESTROYS.fetch_add(1, Ordering::Relaxed);
    unsafe {
        *result = 4242;
    }
    if MODE
        .compare_exchange(6, 0, Ordering::Relaxed, Ordering::Relaxed)
        .is_ok()
    {
        return 10;
    }
    // Caller must have ended AUDIO before native child cleanup.
    unsafe {
        let child = Box::from_raw(child.cast::<Child>());
        if child.attached {
            WRONG_THREAD.fetch_add(1, Ordering::Relaxed);
        }
        let _ = child.parent;
    }
    if MODE.load(Ordering::Relaxed) == 1 {
        7
    } else {
        0
    }
}
unsafe extern "C" fn restart(_: *const c_void, out: *mut u32) -> i32 {
    control();
    unsafe {
        *out = 0;
    }
    0
}
static TABLE: RawProvider = RawProvider {
    struct_size: size_of::<RawProvider>() as u32,
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
    end_audio: Some(end),
    stop: Some(ok),
    destroy: Some(destroy),
    restart_requests: Some(restart),
};
fn load() -> super::Library<'static> {
    // SAFETY: authored exact v2 callbacks above, current fixture main thread;
    // provider code/table are in this process image for its entire lifetime.
    unsafe { Provider::from_raw(&TABLE).unwrap().load("fixture") }.unwrap()
}
fn counts() -> (usize, usize, usize) {
    (
        CLOSES.load(Ordering::Relaxed),
        DESTROYS.load(Ordering::Relaxed),
        ENDS.load(Ordering::Relaxed),
    )
}
#[test]
#[allow(clippy::forget_non_drop)] // Deliberately prove forgetting a non-owning token cannot suppress main teardown.
fn ownership_faults_and_scoped_threads() {
    MAIN.set(std::thread::current().id()).unwrap();
    // Consuming cleanup failure never leads to a second native release.
    MODE.store(1, Ordering::Relaxed);
    let old = counts();
    let library = load();
    let child = library.create(0).unwrap();
    assert_eq!(
        child.retire().map_err(|f| f.error).unwrap(),
        InstanceRetirement::FailedNative { result: 4242 }
    );
    assert_eq!(
        library.retire().map_err(|f| f.error).unwrap(),
        Retirement::FailedNative {
            context_result: 4242
        }
    );
    assert_eq!(counts(), (old.0 + 1, old.1 + 1, old.2));
    println!("PASS consuming native cleanup failure released once");

    // Non-consuming rejections preserve the exact owner for explicit retry.
    MODE.store(0, Ordering::Relaxed);
    let library = load();
    let child = library.create(0).unwrap();
    MODE.store(6, Ordering::Relaxed);
    let super::InstanceRetireFailure { owner, error } = *child.retire().err().unwrap();
    assert_eq!(
        error,
        Error::Native {
            status: 10,
            result: 4242
        }
    );
    assert_eq!(
        owner.retire().map_err(|f| f.error).unwrap(),
        InstanceRetirement::Complete
    );
    MODE.store(2, Ordering::Relaxed);
    let failed = library.retire().err().unwrap();
    assert_eq!(
        failed.error,
        Error::Native {
            status: 8,
            result: 4242
        }
    );
    assert!(matches!(
        failed.owner.retire().map_err(|f| f.error).unwrap(),
        Retirement::Complete { .. }
    ));
    println!("PASS rejected child/parent retirement returns owner for retry");

    MODE.store(3, Ordering::Relaxed);
    let old = counts();
    assert_eq!(
        load().retire().map_err(|f| f.error).unwrap(),
        Retirement::ProcessPinned
    );
    assert_eq!(counts().0, old.0 + 1);
    println!("PASS process-pinned close consumes facade once");

    MODE.store(4, Ordering::Relaxed);
    let old = counts();
    let provider = unsafe { Provider::from_raw(&TABLE) }.unwrap();
    assert!(matches!(
        unsafe { provider.load("fixture") },
        Err(Error::Metadata)
    ));
    assert_eq!(counts().0, old.0 + 1);
    println!("PASS malformed complete catalogue retires staged owner");

    MODE.store(0, Ordering::Relaxed);
    let library = load();
    let mut child = library.create(0).unwrap();
    child.prepare(Profile::new(48000., 64, 1, 1)).unwrap();
    child.start().unwrap();
    let old = counts();
    child
        .with_audio(|mut audio| {
            std::thread::scope(|scope| {
                scope
                    .spawn(move || {
                        let input = [0.0f32; 64];
                        let mut output = [0.0f32; 64];
                        assert_eq!(
                            audio.process(&[&input], &mut [&mut output], 65),
                            Err(Error::Shape)
                        );
                        assert_eq!(
                            audio.process(&[&input[..63]], &mut [&mut output], 64),
                            Err(Error::Shape)
                        );
                        audio.process(&[&input], &mut [&mut output], 64).unwrap();
                    })
                    .join()
                    .unwrap();
            })
        })
        .unwrap();
    assert_eq!(counts().2, old.2 + 1);
    assert_eq!(PROCESSES.load(Ordering::Relaxed), 1);
    child.with_audio(|audio| std::mem::forget(audio)).unwrap();
    assert_eq!(counts().2, old.2 + 2);
    let old_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(|_| {}));
    let caught = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let _ = child.with_audio(|audio| {
            std::thread::scope(|s| {
                s.spawn(move || {
                    let _audio = audio;
                    panic!("joined worker fault")
                })
                .join()
                .unwrap();
            })
        });
    }));
    std::panic::set_hook(old_hook);
    assert!(caught.is_err());
    assert_eq!(counts().2, old.2 + 3);
    child.stop().unwrap();
    drop(child);
    drop(library);
    println!("PASS shape preflight, joined panic/forgotten capability end exactly once on main");

    // Failed end must never invoke lifecycle against an unproved AUDIO owner.
    // This contract-fault mock keeps child memory intentionally leaked until exit,
    // matching the fail-closed Rust path; parent close rejection remains separate.
    MODE.store(0, Ordering::Relaxed);
    let library = load();
    let mut child = library.create(0).unwrap();
    child.prepare(Profile::new(48000., 64, 1, 1)).unwrap();
    child.start().unwrap();
    MODE.store(5, Ordering::Relaxed);
    let old = counts();
    assert_eq!(
        child.with_audio(|_audio| ()),
        Err(Error::Native {
            status: 10,
            result: 4242
        })
    );
    assert_eq!(child.stop(), Err(Error::Poisoned));
    let failed = child.retire().err().unwrap();
    assert_eq!(failed.error, Error::Poisoned);
    drop(failed);
    assert_eq!(counts().1, old.1);
    // Actual native close rejects retained children; this mock now chooses that
    // non-consuming rejection and deliberately forgets the returned facade.
    MODE.store(2, Ordering::Relaxed);
    let failed = library.retire().err().unwrap();
    std::mem::forget(failed.owner);
    assert_eq!(WRONG_THREAD.load(Ordering::Relaxed), 0);
    println!("PASS failed AUDIO end poisons child without destructive cleanup");
}
