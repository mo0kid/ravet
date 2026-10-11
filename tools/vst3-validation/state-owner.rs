#[path = "../../crates/ravet-host/src/vst3.rs"]
mod vst3;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
struct CountAllocator;
static COUNTING: AtomicBool = AtomicBool::new(false);
static ALLOCS: AtomicUsize = AtomicUsize::new(0);
static FREES: AtomicUsize = AtomicUsize::new(0);
// This diagnostic preserves System ownership/layout; it observes only Rust
// allocator entry points, not SDK/C++ allocation or a callback deadline.
unsafe impl std::alloc::GlobalAlloc for CountAllocator {
    unsafe fn alloc(&self, layout: std::alloc::Layout) -> *mut u8 {
        if COUNTING.load(Ordering::Relaxed) {
            ALLOCS.fetch_add(1, Ordering::Relaxed);
        }
        unsafe { std::alloc::System.alloc(layout) }
    }
    unsafe fn dealloc(&self, pointer: *mut u8, layout: std::alloc::Layout) {
        if COUNTING.load(Ordering::Relaxed) {
            FREES.fetch_add(1, Ordering::Relaxed);
        }
        unsafe { std::alloc::System.dealloc(pointer, layout) }
    }
    unsafe fn realloc(&self, pointer: *mut u8, layout: std::alloc::Layout, size: usize) -> *mut u8 {
        if COUNTING.load(Ordering::Relaxed) {
            ALLOCS.fetch_add(1, Ordering::Relaxed);
            FREES.fetch_add(1, Ordering::Relaxed);
        }
        unsafe { std::alloc::System.realloc(pointer, layout, size) }
    }
}
#[global_allocator]
static ALLOCATOR: CountAllocator = CountAllocator;
use instances::state::{CaptureCapacity, ControllerState, Snapshot};
use vst3::instances::{self, Profile};
unsafe extern "C" {
    fn ravet_vst3_provider_v2() -> *const instances::RawProvider;
    fn ravet_vst3_state_provider_v1() -> *const instances::state::RawProvider;
}
fn altered(source: &[u8], values: &[(u32, f32)]) -> Vec<u8> {
    let mut bytes = source.to_vec();
    assert!(bytes.len() >= 16);
    assert_eq!(
        u32::from_le_bytes(bytes[12..16].try_into().unwrap()) as usize,
        values.len()
    );
    for pair in bytes[16..].as_chunks_mut::<8>().0 {
        let id = u32::from_le_bytes(pair[..4].try_into().unwrap());
        let value = values
            .iter()
            .find(|(expected, _)| *expected == id)
            .unwrap()
            .1;
        pair[4..].copy_from_slice(&value.to_le_bytes());
    }
    bytes
}
fn snapshot(source: &Snapshot, component: &[(u32, f32)], controller: &[(u32, f32)]) -> Snapshot {
    let ControllerState::Bytes(original) = source.controller() else {
        panic!("actual references have supported own controller state");
    };
    Snapshot::new(
        source.class_id(),
        altered(source.component(), component),
        ControllerState::Bytes(altered(original, controller)),
    )
    .unwrap()
}
fn main() {
    // SAFETY: explicit static caller-linked reviewed provider and code live for
    // process lifetime; this main thread owns all modules/children/control calls.
    let provider = unsafe { instances::Provider::from_raw(&*ravet_vst3_provider_v2()).unwrap() };
    let state =
        unsafe { instances::state::Provider::from_raw(&*ravet_vst3_state_provider_v1()).unwrap() };
    let paths: Vec<_> = std::env::args().skip(1).collect();
    assert_eq!(paths.len(), 2);
    let capacity = CaptureCapacity::new(4096, 4096).unwrap();
    for (selected, path) in paths.iter().enumerate() {
        // SAFETY: explicitly selected public authored reference SDK bundles;
        // no discovery, host configuration or installation is performed.
        let library = unsafe { provider.load(path).unwrap() };
        let index = library
            .classes()
            .iter()
            .position(|class| class.category.starts_with(b"Audio Module Class\0"))
            .unwrap() as u32;
        let mut child = library.create(index).unwrap();
        let original = child.capture_state(state, capacity).unwrap();
        assert_eq!(original.report.returned_status(), Some(0));
        assert_eq!(
            original.snapshot.component().len(),
            if selected == 0 { 32 } else { 24 }
        );
        let values = if selected == 0 {
            vec![(1, 0.5), (2, 0.)]
        } else {
            vec![(1, 0.25)]
        };
        let configured = snapshot(&original.snapshot, &values, &values);
        assert!(
            child
                .restore_state(state, &configured)
                .unwrap()
                .prepare_required()
        );
        let captured = child.capture_state(state, capacity).unwrap().snapshot;
        assert_eq!(captured, configured);
        assert!(child.start().is_err()); // old profile must be explicitly prepared
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
            let mut outputs = [[77f32; 70]; 2];
            let inputs = [[0.125f32; 70], [-0.25f32; 70]];
            child
                .with_audio(|mut audio| {
                    std::thread::scope(|scope| {
                        scope
                            .spawn(|| {
                                ALLOCS.store(0, Ordering::Relaxed);
                                FREES.store(0, Ordering::Relaxed);
                                COUNTING.store(true, Ordering::Relaxed);
                                for _ in 0..10000 {
                                    let (first, last) = outputs.split_at_mut(1);
                                    let mut output = [&mut first[0][..], &mut last[0][..]];
                                    let input = [&inputs[0][..], &inputs[1][..]];
                                    audio
                                        .process(
                                            if selected == 0 {
                                                &input[..channels as usize]
                                            } else {
                                                &[]
                                            },
                                            &mut output[..channels as usize],
                                            64,
                                        )
                                        .unwrap();
                                }
                                COUNTING.store(false, Ordering::Relaxed);
                                assert_eq!(ALLOCS.load(Ordering::Relaxed), 0);
                                assert_eq!(FREES.load(Ordering::Relaxed), 0);
                                for ch in 0..channels as usize {
                                    assert!(outputs[ch][..64].iter().all(|v| {
                                        (*v - if selected == 0 {
                                            inputs[ch][0] * 0.5
                                        } else {
                                            0.
                                        })
                                        .abs()
                                            < 1e-6
                                    }));
                                    assert_eq!(&outputs[ch][64..], &[77.; 6]);
                                }
                            })
                            .join()
                            .unwrap()
                    })
                })
                .unwrap();
            child.stop().unwrap();
            assert_eq!(
                child.capture_state(state, capacity).unwrap().snapshot,
                configured
            );
        }
        // Own controller restore follows component restore and may synchronize
        // a different complete state back to the connected authored component.
        let component = if selected == 0 {
            vec![(1, 2.), (2, 0.)]
        } else {
            vec![(1, 0.25)]
        };
        let controller = if selected == 0 {
            vec![(1, 0.75), (2, 1.)]
        } else {
            vec![(1, 0.75)]
        };
        let diverged = snapshot(&original.snapshot, &component, &controller);
        child.restore_state(state, &diverged).unwrap();
        let captured = child.capture_state(state, capacity).unwrap().snapshot;
        let expected = snapshot(&original.snapshot, &controller, &controller);
        assert_eq!(captured, expected);
        if selected == 0 {
            child.prepare(Profile::new(48000., 64, 1, 1)).unwrap();
            child.start().unwrap();
            child
                .with_audio(|mut audio| {
                    std::thread::scope(|scope| {
                        scope
                            .spawn(|| {
                                let input = [0.125f32; 64];
                                let mut output = [77f32; 64];
                                audio.process(&[&input], &mut [&mut output], 64).unwrap();
                                assert_eq!(output, input);
                            })
                            .join()
                            .unwrap()
                    })
                })
                .unwrap();
            child.stop().unwrap();
        }
        assert_eq!(
            child.retire().ok(),
            Some(instances::InstanceRetirement::Complete)
        );
        // Real controller codec rejection is late; native child becomes faulted
        // after the component setter. A fresh child is explicit recovery.
        let mut child = library.create(index).unwrap();
        let ControllerState::Bytes(bytes) = configured.controller() else {
            unreachable!();
        };
        let mut bad = bytes.clone();
        bad[0] ^= 1;
        let malformed = Snapshot::new(
            configured.class_id(),
            configured.component().to_vec(),
            ControllerState::Bytes(bad),
        )
        .unwrap();
        let error = child.restore_state(state, &malformed).unwrap_err();
        let instances::state::Error::Provider { status, report } = error else {
            panic!("missing exact SDK refusal");
        };
        assert_eq!(status, 4);
        assert_eq!(report.stage(), instances::state::Stage::ControllerRestore);
        assert!(report.poisoned() && report.returned_status().is_some() && !report.threw());
        assert!(
            child
                .prepare(Profile::new(
                    48000.,
                    64,
                    if selected == 0 { 1 } else { 0 },
                    1
                ))
                .is_err()
        );
        child.stop().unwrap();
        drop(child);
        let mut recovered = library.create(index).unwrap();
        recovered.restore_state(state, &configured).unwrap();
        assert_eq!(
            recovered.capture_state(state, capacity).unwrap().snapshot,
            configured
        );
        drop(recovered);
        assert!(matches!(
            library.retire().ok(),
            Some(instances::Retirement::Complete { .. })
        ));
    }
    println!(
        "PASS actual Rust/native Gain/Performance complete component/controller capture/restore, 10000 mono/stereo subsequent blocks/profile with0Rust allocations/deallocations/tails, controller-own order/connected sync, real late codec refusal/poison/safe retirement/new-child recovery"
    );
}
