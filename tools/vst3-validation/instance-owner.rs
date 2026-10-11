#[path = "../../crates/ravet-host/src/vst3.rs"]
mod vst3;
use vst3::instances;
use instances::{InstanceRetirement, Profile, Provider, RawProvider, Retirement};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
struct CountAllocator;
static COUNTING: AtomicBool = AtomicBool::new(false);
static ALLOCS: AtomicUsize = AtomicUsize::new(0);
static FREES: AtomicUsize = AtomicUsize::new(0);
// SAFETY: unchanged System allocator ownership/layout contract; diagnostics only.
unsafe impl std::alloc::GlobalAlloc for CountAllocator {
    unsafe fn alloc(&self, layout: std::alloc::Layout) -> *mut u8 {
        if COUNTING.load(Ordering::Relaxed) {
            ALLOCS.fetch_add(1, Ordering::Relaxed);
        }
        unsafe { std::alloc::System.alloc(layout) }
    }
    unsafe fn dealloc(&self, ptr: *mut u8, layout: std::alloc::Layout) {
        if COUNTING.load(Ordering::Relaxed) {
            FREES.fetch_add(1, Ordering::Relaxed);
        }
        unsafe {
            std::alloc::System.dealloc(ptr, layout);
        }
    }
    unsafe fn realloc(&self, ptr: *mut u8, layout: std::alloc::Layout, size: usize) -> *mut u8 {
        if COUNTING.load(Ordering::Relaxed) {
            ALLOCS.fetch_add(1, Ordering::Relaxed);
            FREES.fetch_add(1, Ordering::Relaxed);
        }
        unsafe { std::alloc::System.realloc(ptr, layout, size) }
    }
}
#[global_allocator]
static ALLOCATOR: CountAllocator = CountAllocator;
unsafe extern "C" {
    fn ravet_vst3_provider_v2() -> *const RawProvider;
}
fn main() {
    // SAFETY: actual caller-linked static provider lives in this process image;
    // current main thread and external SDK/public references are explicitly trusted.
    let provider = unsafe { Provider::from_raw(&*ravet_vst3_provider_v2()) }.unwrap();
    assert_eq!(size_of::<RawProvider>(), 112);
    for (selected, path) in std::env::args().skip(1).enumerate() {
        // SAFETY: selected local public reference and dependencies inspected/tested.
        let library = unsafe { provider.load(path) }.unwrap();
        assert!(library.factory().class_count >= 1);
        let index = library
            .classes()
            .iter()
            .position(|c| c.category.starts_with(b"Audio Module Class\0"))
            .unwrap() as u32;
        let mut child = library.create(index).unwrap();
        let _ = child.restart_requests().unwrap();
        for channels in [1, 2] {
            child
                .prepare(Profile::new(
                    48000.,
                    64,
                    if selected == 0 { channels } else { 0 },
                    channels,
                ))
                .unwrap();
            child.start().unwrap();
            let input = vec![vec![0.25f32; 70]; channels as usize];
            let mut output = vec![vec![77.0f32; 70]; channels as usize];
            child
                .with_audio(|mut audio| {
                    std::thread::scope(|scope| {
                        scope
                            .spawn(move || {
                                let input: Vec<_> = if selected == 0 {
                                    input.iter().map(Vec::as_slice).collect()
                                } else {
                                    Vec::new()
                                };
                                let mut slices: Vec<_> =
                                    output.iter_mut().map(Vec::as_mut_slice).collect();
                                let before = (
                                    ALLOCS.load(Ordering::Relaxed),
                                    FREES.load(Ordering::Relaxed),
                                );
                                COUNTING.store(true, Ordering::Relaxed);
                                assert_eq!(
                                    audio.process(&input, &mut slices, 65),
                                    Err(instances::Error::Shape)
                                );
                                for _ in 0..10000 {
                                    audio.process(&input, &mut slices, 64).unwrap();
                                }
                                COUNTING.store(false, Ordering::Relaxed);
                                assert_eq!(
                                    before,
                                    (
                                        ALLOCS.load(Ordering::Relaxed),
                                        FREES.load(Ordering::Relaxed)
                                    )
                                );
                                for slice in slices {
                                    for sample in &slice[..64] {
                                        assert_eq!(*sample, if selected == 0 { 0.25 } else { 0. });
                                    }
                                    assert_eq!(&slice[64..], &[77.; 6]);
                                }
                            })
                            .join()
                            .unwrap();
                    })
                })
                .unwrap();
            child.stop().unwrap();
            child.start().unwrap();
            let caught = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                child
                    .with_audio(|mut audio| {
                        std::thread::scope(|scope| {
                            scope
                                .spawn(move || {
                                    let inputs = [&[0.0f32; 0][..]; 2];
                                    let mut left = [];
                                    let mut right = [];
                                    let mut outs: [&mut [f32]; 2] = [&mut left, &mut right];
                                    audio
                                        .process(
                                            if selected == 0 {
                                                &inputs[..channels as usize]
                                            } else {
                                                &[]
                                            },
                                            &mut outs[..channels as usize],
                                            0,
                                        )
                                        .unwrap();
                                    panic!("intentional scoped worker panic");
                                })
                                .join()
                                .unwrap();
                        })
                    })
                    .unwrap();
            }));
            assert!(caught.is_err());
            child.stop().unwrap();
            // Forgetting the non-owning audio value does not suppress main scope retirement.
            child.start().unwrap();
            child.with_audio(|audio| std::mem::forget(audio)).unwrap();
            child.stop().unwrap();
        }
        assert!(matches!(
            child.retire().map_err(|f| f.error).unwrap(),
            InstanceRetirement::Complete
        ));
        assert!(matches!(
            library.retire().map_err(|f| f.error).unwrap(),
            Retirement::Complete { .. }
        ));
    }
    println!(
        "PASS actual Rust v2 borrowed children, scoped Send audio10000mono/stereo Gain/Performance blocks with0Rust allocation/deallocation, shape/tails, joined worker panic main retirement and forgotten nonowning capability"
    );
}
