// Ownership tests deliberately drop/forget the non-retiring AUDIO capability.
#![allow(clippy::drop_non_drop, clippy::forget_non_drop)]
use super::*;
use std::sync::Mutex;
use std::sync::atomic::{AtomicI32, AtomicU32, Ordering};
static LOCK: Mutex<()> = Mutex::new(());
static END: AtomicU32 = AtomicU32::new(0);
static DESTROY: AtomicU32 = AtomicU32::new(0);
static PROCESS: AtomicU32 = AtomicU32::new(0);
static CANCEL: AtomicU32 = AtomicU32::new(0);
static END_STATUS: AtomicI32 = AtomicI32::new(0);
static POLL_KIND: AtomicU32 = AtomicU32::new(0);
unsafe extern "C" fn retire(_: *mut c_void, d: *mut Diagnostic) -> i32 {
    unsafe {
        *d = Diagnostic::default();
        (*d).consumed = 1;
    }
    DESTROY.fetch_add(1, Ordering::Relaxed);
    0
}
unsafe extern "C" fn cancel(_: *mut c_void, d: *mut Diagnostic) -> i32 {
    unsafe {
        *d = Diagnostic::default();
        (*d).consumed = 1;
    }
    CANCEL.fetch_add(1, Ordering::Relaxed);
    0
}
unsafe extern "C" fn catalogue(
    _: *const Filter,
    out: *mut *mut c_void,
    count: *mut u32,
    d: *mut Diagnostic,
) -> i32 {
    unsafe {
        *d = Diagnostic::default();
        *out = NonNull::<u64>::dangling().as_ptr().cast();
        *count = 0;
    }
    0
}
unsafe extern "C" fn component(
    _: *const c_void,
    _: u32,
    _: *mut ComponentRecord,
    d: *mut Diagnostic,
) -> i32 {
    unsafe {
        *d = Diagnostic::default();
    }
    2
}
unsafe extern "C" fn create(
    _: *mut c_void,
    _: u32,
    _: u32,
    out: *mut *mut c_void,
    d: *mut Diagnostic,
) -> i32 {
    unsafe {
        *d = Diagnostic::default();
        *out = NonNull::<u64>::dangling().as_ptr().cast();
    }
    0
}
unsafe extern "C" fn poll(
    _: *mut c_void,
    out: *mut *mut c_void,
    info: *mut InstanceInfo,
    d: *mut Diagnostic,
) -> i32 {
    unsafe {
        *d = Diagnostic::default();
    }
    match POLL_KIND.load(Ordering::Relaxed) {
        0 => 1,
        1 => {
            unsafe {
                (*d).consumed = 1;
                (*d).attempted = 1;
                (*d).returned = 1;
                (*d).has_error = 1;
                (*d).error_code = i64::MIN + 9;
                (*d).domain_length = 3;
                (&mut (*d).domain)[..3].copy_from_slice(b"err");
            }
            6
        }
        2 => {
            unsafe {
                *out = NonNull::<u64>::dangling().as_ptr().cast();
                *info = InstanceInfo {
                    byte_size: size_of::<InstanceInfo>() as u32,
                    component_type: 1,
                    subtype: 2,
                    manufacturer: 3,
                    requested_mode: 1,
                    observed_in_process: 1,
                    reserved: [0, 0],
                };
            }
            0
        }
        _ => {
            unsafe {
                (*d).has_error = 1;
                (*d).domain_length = 3;
                (&mut (*d).domain)[..3].copy_from_slice(b"err");
            }
            6
        }
    }
}
unsafe extern "C" fn prepare(_: *mut c_void, _: *const Profile, d: *mut Diagnostic) -> i32 {
    unsafe {
        *d = Diagnostic::default();
    }
    0
}
unsafe extern "C" fn begin(_: *mut c_void, token: *mut *mut c_void, d: *mut Diagnostic) -> i32 {
    unsafe {
        *d = Diagnostic::default();
        *token = NonNull::<u64>::dangling().as_ptr().cast();
    }
    0
}
unsafe extern "C" fn process(_: *mut c_void, b: *const Block, d: *mut Diagnostic) -> i32 {
    PROCESS.fetch_add(1, Ordering::Relaxed);
    unsafe {
        *d = Diagnostic::default();
        (*d).attempted = 1;
        (*d).returned = 1;
        (*d).has_os_status = 1;
        for c in 0..(*b).output_channels as usize {
            let input = &*(*b).inputs.add(c);
            let output = &*(*b).outputs.add(c);
            for f in 0..(*b).frames as usize {
                *output.samples.add(f) = *input.samples.add(f) * 2.;
            }
        }
    }
    0
}
unsafe extern "C" fn end(_: *mut c_void, d: *mut Diagnostic) -> i32 {
    unsafe {
        *d = Diagnostic::default();
    }
    END.fetch_add(1, Ordering::Relaxed);
    END_STATUS.load(Ordering::Relaxed)
}
unsafe extern "C" fn stop(_: *mut c_void, d: *mut Diagnostic) -> i32 {
    unsafe {
        *d = Diagnostic::default();
    }
    0
}
fn raw() -> RawProvider {
    RawProvider {
        byte_size: size_of::<RawProvider>() as u32,
        version: 1,
        catalogue: Some(catalogue),
        component: Some(component),
        close_catalogue: Some(retire),
        create_async: Some(create),
        poll: Some(poll),
        cancel: Some(cancel),
        prepare: Some(prepare),
        begin_audio: Some(begin),
        process: Some(process),
        end_audio: Some(end),
        stop: Some(stop),
        destroy: Some(retire),
    }
}
fn instance(raw: &RawProvider) -> Instance<'_> {
    Instance {
        raw,
        handle: Some(NonNull::<u64>::dangling().cast()),
        info: InstanceInfo::default(),
        profile: None,
        poisoned: false,
        main: PhantomData,
    }
}
fn pending(raw: &RawProvider) -> Pending<'_> {
    Pending {
        raw,
        handle: Some(NonNull::<u64>::dangling().cast()),
        identity: [1, 2, 3],
        mode: 1,
        poisoned: false,
        main: PhantomData,
    }
}
#[test]
fn table_and_profile_complete_preflight() {
    let mut r = raw();
    r.process = None;
    assert!(unsafe { Provider::from_raw(&r) }.is_err());
    r.process = Some(process);
    r.byte_size -= 1;
    assert!(unsafe { Provider::from_raw(&r) }.is_err());
    for p in [
        Profile::new(f64::NAN, 16, 2, 2),
        Profile::new(48000., 0, 2, 2),
        Profile::new(48000., 65537, 2, 2),
        Profile::new(48000., 16, 1, 2),
        Profile::new(48000., 16, 3, 3),
    ] {
        assert!(!p.valid())
    }
    assert!(Profile::new(768000., 65536, 0, 2).valid());
}
#[test]
fn joined_scope_shape_and_forget_unwind_retirement() {
    let _lock = LOCK.lock().unwrap();
    END.store(0, Ordering::Relaxed);
    PROCESS.store(0, Ordering::Relaxed);
    END_STATUS.store(0, Ordering::Relaxed);
    let r = raw();
    let mut i = instance(&r);
    i.prepare(Profile::new(48000., 16, 1, 1)).unwrap();
    let input = [0.25; 20];
    let mut output = [99.; 20];
    i.with_audio(|mut audio| {
        std::thread::scope(|scope| {
            scope
                .spawn(|| {
                    audio
                        .process(&[&input], &mut [&mut output], 16, i64::MAX)
                        .unwrap();
                })
                .join()
                .unwrap()
        })
    })
    .unwrap();
    assert_eq!(output[15], 0.5);
    assert_eq!(output[16], 99.);
    let before = output;
    i.with_audio(|mut audio| {
        assert!(matches!(
            audio.process(&[], &mut [&mut output], 16, 0),
            Err(Error::Shape)
        ))
    })
    .unwrap();
    assert_eq!(before, output);
    assert_eq!(PROCESS.load(Ordering::Relaxed), 1);
    i.with_audio(|a| std::mem::forget(a)).unwrap();
    assert!(
        std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let _ = i.with_audio(|a| {
                drop(a);
                panic!("scope unwind")
            });
        }))
        .is_err()
    );
    assert_eq!(END.load(Ordering::Relaxed), 4);
}
#[test]
fn failed_audio_end_poison_preserves_native_owner() {
    let _lock = LOCK.lock().unwrap();
    DESTROY.store(0, Ordering::Relaxed);
    END_STATUS.store(3, Ordering::Relaxed);
    let r = raw();
    let mut i = instance(&r);
    i.prepare(Profile::new(48000., 16, 1, 1)).unwrap();
    assert!(i.with_audio(|a| drop(a)).is_err());
    assert!(matches!(i.stop(), Err(Error::Poisoned)));
    drop(i);
    assert_eq!(DESTROY.load(Ordering::Relaxed), 0);
    END_STATUS.store(0, Ordering::Relaxed);
}
#[test]
fn pending_exact_terminal_error_and_malformed_ownership() {
    let _lock = LOCK.lock().unwrap();
    CANCEL.store(0, Ordering::Relaxed);
    let r = raw();
    let mut p = pending(&r);
    POLL_KIND.store(0, Ordering::Relaxed);
    assert!(p.poll().unwrap().is_none());
    POLL_KIND.store(1, Ordering::Relaxed);
    match p.poll() {
        Err(Error::Host {
            status: 6,
            diagnostic,
        }) => {
            assert_eq!(diagnostic.error_code, i64::MIN + 9);
            assert_eq!(diagnostic.error_domain(), Some("err"));
            assert_eq!(diagnostic.has_os_status, 0)
        }
        _ => panic!("original error lost"),
    };
    drop(p);
    assert_eq!(CANCEL.load(Ordering::Relaxed), 0);
    POLL_KIND.store(2, Ordering::Relaxed);
    let mut p = pending(&r);
    assert!(matches!(p.poll(), Err(Error::Metadata)));
    assert!(matches!(p.poll(), Err(Error::Poisoned)));
    drop(p);
    assert_eq!(CANCEL.load(Ordering::Relaxed), 0);
    POLL_KIND.store(3, Ordering::Relaxed);
    let mut p = pending(&r);
    assert!(matches!(p.poll(), Err(Error::Metadata)));
    drop(p);
    assert_eq!(CANCEL.load(Ordering::Relaxed), 0);
}
