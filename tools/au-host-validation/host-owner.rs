//! Actual caller-linked AUv3 host consumer for the optional `au-hosting` owners.
//! Built and run only by `check-au-host.py`, never by the default workspace.
// FFI fixture: this is the explicit trusted native boundary and documents each
// SAFETY obligation.
#![allow(unsafe_code)]
#[path = "../../crates/ravet-host/src/au.rs"]
mod au;
use au::{Diagnostic, Filter, Instance, LoadMode, Pending, Profile, Provider, RawProvider};
use std::alloc::{GlobalAlloc, Layout, System};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::time::{Duration, Instant};
// Fixture-only counting allocator: armed solely around the joined prepared
// AUDIO loop, which runs on one worker while MAIN blocks in the join.
struct Counting;
static ARMED: AtomicBool = AtomicBool::new(false);
static ALLOCATIONS: AtomicUsize = AtomicUsize::new(0);
// SAFETY: forwards every request unchanged to the system allocator.
unsafe impl GlobalAlloc for Counting {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        if ARMED.load(Ordering::Relaxed) {
            ALLOCATIONS.fetch_add(1, Ordering::Relaxed);
        }
        unsafe { System.alloc(layout) }
    }
    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        unsafe { System.dealloc(ptr, layout) }
    }
}
#[global_allocator]
static COUNTING: Counting = Counting;
unsafe extern "C" {
    fn ravet_au_host_v1() -> *const RawProvider;
    fn ravet_auh_fixture_register_gain();
    fn ravet_auh_fixture_main_service();
    fn ravet_auh_fixture_gain_two() -> bool;
    fn ravet_auh_fixture_run(run: extern "C" fn() -> i32) -> i32;
}
fn completed(mut request: Pending<'static>) -> Instance<'static> {
    let limit = Instant::now() + Duration::from_secs(10);
    loop {
        if let Some(instance) = request.poll().unwrap() {
            return instance;
        }
        assert!(Instant::now() < limit, "async completion timeout");
        // SAFETY: explicit test-only main queue service, never part of host poll.
        unsafe { ravet_auh_fixture_main_service() };
    }
}
// Deliberate ownership probes exercise forgotten/non-owning AUDIO capability;
// fixed inline diagnostic results must not allocate on AUDIO.
#[allow(
    clippy::result_large_err,
    clippy::forget_non_drop,
    clippy::drop_non_drop
)]
fn contracts() {
    // SAFETY: genuine caller-linked process-lifetime native provider, current
    // platform main, explicit trusted public Gain test-only class registration.
    let provider = unsafe {
        ravet_auh_fixture_register_gain();
        Provider::from_raw(&*ravet_au_host_v1()).unwrap()
    };
    let filter = Filter::new(
        u32::from_be_bytes(*b"aufx"),
        u32::from_be_bytes(*b"RvG3"),
        u32::from_be_bytes(*b"Ravt"),
        4,
    );
    for channels in [1, 2] {
        let catalogue = provider.catalogue(filter).unwrap();
        assert_eq!(catalogue.components().len(), 1);
        let pending = catalogue.create(0, LoadMode::RequireInProcess).unwrap();
        catalogue.retire().unwrap(); // no borrowed catalogue escapes into async callback
        let mut instance = completed(pending);
        assert_eq!(instance.info().observed_in_process, 1);
        assert!(unsafe { ravet_auh_fixture_gain_two() });
        let profile = Profile::new(48000., 512, channels, channels);
        instance.prepare(profile).unwrap();
        let left = [0.25; 520];
        let right = [-0.125; 520];
        let mut out_left = [99.; 520];
        let mut out_right = [99.; 520];
        let diagnostic: Diagnostic = instance
            .with_audio(|mut audio| {
                std::thread::scope(|scope| {
                    scope
                        .spawn(|| {
                            if channels == 1 {
                                audio.process(&[&left], &mut [&mut out_left], 512, 1000)
                            } else {
                                audio.process(
                                    &[&left, &right],
                                    &mut [&mut out_left, &mut out_right],
                                    512,
                                    1000,
                                )
                            }
                        })
                        .join()
                        .unwrap()
                        .unwrap()
                })
            })
            .unwrap();
        assert_eq!(diagnostic.returned, 1);
        assert_eq!(diagnostic.os_status, 0);
        assert!((out_left[511] - 0.5).abs() < 1e-6);
        if channels == 2 {
            assert!((out_right[511] + 0.25).abs() < 1e-6)
        }
        assert_eq!(out_left[512], 99.);
        assert_eq!(out_right[512], 99.);
        // Prepared safe AUDIO path: 2000 actual public renderBlock calls perform
        // no Rust heap allocation (native C++ new is counted by the CTest fixture).
        let counted = instance
            .with_audio(|mut audio| {
                std::thread::scope(|scope| {
                    scope
                        .spawn(|| {
                            ALLOCATIONS.store(0, Ordering::Relaxed);
                            ARMED.store(true, Ordering::Relaxed);
                            for _ in 0..2000 {
                                let ok = if channels == 1 {
                                    audio.process(&[&left], &mut [&mut out_left], 512, 0)
                                } else {
                                    audio.process(
                                        &[&left, &right],
                                        &mut [&mut out_left, &mut out_right],
                                        512,
                                        0,
                                    )
                                };
                                if ok.is_err() {
                                    break;
                                }
                            }
                            ARMED.store(false, Ordering::Relaxed);
                            ALLOCATIONS.load(Ordering::Relaxed)
                        })
                        .join()
                        .unwrap()
                })
            })
            .unwrap();
        assert_eq!(counted, 0, "prepared safe AUDIO loop allocated");
        assert!((out_left[511] - 0.5).abs() < 1e-6);
        assert_eq!(out_left[512], 99.);
        let before = out_left;
        instance
            .with_audio(|mut audio| {
                assert!(audio.process(&[], &mut [&mut out_left], 512, 0).is_err());
            })
            .unwrap();
        assert_eq!(before, out_left);
        instance
            .with_audio(|audio| std::mem::forget(audio))
            .unwrap(); // forgotten token still ends on Main
        let unwind = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let _ = instance.with_audio(|audio| {
                std::thread::scope(|scope| {
                    scope
                        .spawn(move || {
                            drop(audio);
                            panic!("qualified joined worker unwind")
                        })
                        .join()
                        .unwrap();
                })
            });
        }));
        assert!(unwind.is_err());
        instance.stop().unwrap();
        instance.prepare(profile).unwrap();
        instance.retire().unwrap();
    }
    // Safe owner Drop transfers bounded deferred retirement while a sibling
    // AUDIO geometry is frozen, without releasing native code on a worker.
    let catalogue = provider.catalogue(filter).unwrap();
    let mut active = completed(catalogue.create(0, LoadMode::RequireInProcess).unwrap());
    let stopped = completed(catalogue.create(0, LoadMode::RequireInProcess).unwrap());
    let cancelled = catalogue.create(0, LoadMode::RequireInProcess).unwrap();
    active.prepare(Profile::new(48000., 16, 2, 2)).unwrap();
    active
        .with_audio(|audio| {
            drop(stopped);
            drop(cancelled);
            drop(catalogue);
            drop(audio)
        })
        .unwrap();
    active.retire().unwrap();
    for _ in 0..50 {
        unsafe { ravet_auh_fixture_main_service() }
    }
    println!(
        "PASS safe Rust actual async Gain mono/stereo, joined AUDIO/unwind/forget, 2000-call prepared AUDIO without Rust allocation, independent pending/catalogue and deferred sibling Drop"
    );
}
extern "C" fn run() -> i32 {
    std::panic::set_hook(Box::new(|_| {}));
    if std::panic::catch_unwind(contracts).is_ok() {
        0
    } else {
        1
    }
}
fn main() {
    assert_eq!(
        unsafe { ravet_auh_fixture_run(run) },
        0,
        "actual Rust/native owner contracts"
    )
}
