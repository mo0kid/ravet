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
use instances::parameters::{self, Point};
use instances::state::CaptureCapacity;
use vst3::instances::{self, Profile};
unsafe extern "C" {
    fn ravet_vst3_provider_v2() -> *const instances::RawProvider;
    fn ravet_vst3_parameter_provider_v1() -> *const parameters::RawProvider;
    fn ravet_vst3_state_provider_v1() -> *const instances::state::RawProvider;
}
struct Smoothing {
    current: f32,
    target: f32,
    start: f64,
    elapsed: u32,
    bypass: bool,
}
impl Smoothing {
    fn event(&mut self, p: Point) {
        if p.id == 2 {
            self.bypass = p.value >= 0.5;
            return;
        }
        let target = (p.value * 4.) as f32;
        if target != self.target {
            self.start = self.current as f64;
            self.target = target;
            self.elapsed = 0;
        }
    }
    fn next(&mut self) -> f32 {
        if self.elapsed < 240 {
            self.elapsed += 1;
            self.current = if self.elapsed == 240 {
                self.target
            } else {
                (self.start + (self.target as f64 - self.start) * self.elapsed as f64 / 240.) as f32
            };
        }
        self.current
    }
}
fn main() {
    let paths: Vec<_> = std::env::args_os().skip(1).collect();
    assert_eq!(paths.len(), 2);
    // SAFETY: explicitly linked current authored native provider and actual local
    // public Gain/Performance bundles. Executable/provider remain loaded here.
    let provider = unsafe { instances::Provider::from_raw(&*ravet_vst3_provider_v2()).unwrap() };
    let parameters =
        unsafe { parameters::Provider::from_raw(&*ravet_vst3_parameter_provider_v1()).unwrap() };
    let state =
        unsafe { instances::state::Provider::from_raw(&*ravet_vst3_state_provider_v1()).unwrap() };
    for (module, path) in paths.iter().enumerate() {
        let library = unsafe { provider.load(std::path::Path::new(path)).unwrap() };
        let index = library
            .classes()
            .iter()
            .position(|c| c.category.starts_with(b"Audio Module Class\0"))
            .unwrap() as u32;
        for channels in [1, 2] {
            let mut child = library.create(index).unwrap();
            let catalogue = child.refresh_parameters(parameters).unwrap();
            assert!(!catalogue.parameters().is_empty());
            assert!(
                catalogue
                    .parameters()
                    .iter()
                    .any(|p| p.id == 1 && p.writable())
            );
            let capacity = CaptureCapacity::new(128, 128).unwrap();
            let original = child.capture_state(state, capacity).unwrap().snapshot;
            if module == 0 {
                child.edit_parameter(parameters, 1, 0.75).unwrap();
                assert_eq!(child.read_parameter(parameters, 1).unwrap(), 0.75);
                assert_eq!(child.parameter_to_plain(parameters, 1, 0.75).unwrap(), 3.);
                assert_eq!(
                    child.parameter_to_normalized(parameters, 1, 2.).unwrap(),
                    0.5
                );
                let text = child.format_parameter(parameters, 1, 0.5).unwrap();
                assert!((child.parse_parameter(parameters, 1, &text).unwrap() - 0.5).abs() < 1e-6);
                assert_eq!(
                    child.edit_parameter(parameters, 1, f64::NAN),
                    Err(parameters::Error::Shape)
                );
                assert_eq!(
                    child.parse_parameter(parameters, 1, "x\0y"),
                    Err(parameters::Error::Shape)
                );
            }
            child
                .prepare_parameters(
                    parameters,
                    Profile::new(48000., 64, if module == 0 { channels } else { 0 }, channels),
                )
                .unwrap();
            child.start().unwrap();
            let inputs = [[0.125f32; 70], [-0.25f32; 70]];
            let mut outputs = [[77f32; 70]; 2];
            child
                .with_audio(|mut audio| {
                    std::thread::scope(|scope| {
                        scope
                            .spawn(|| {
                                let points = [
                                    Point {
                                        id: 1,
                                        offset: 0,
                                        value: 0.5,
                                    },
                                    Point {
                                        id: 1,
                                        offset: 3,
                                        value: 0.125,
                                    },
                                    Point {
                                        id: 2,
                                        offset: 5,
                                        value: 1.,
                                    },
                                    Point {
                                        id: 2,
                                        offset: 12,
                                        value: 0.,
                                    },
                                    Point {
                                        id: 1,
                                        offset: 16,
                                        value: 0.25,
                                    },
                                ];
                                let performance = [Point {
                                    id: 1,
                                    offset: 0,
                                    value: 0.3,
                                }];
                                let invalid = [
                                    Point {
                                        id: 1,
                                        offset: 0,
                                        value: 0.5,
                                    },
                                    Point {
                                        id: 1,
                                        offset: 0,
                                        value: 0.3,
                                    },
                                ];
                                let mut smoothing = Smoothing {
                                    current: 1.,
                                    target: 1.,
                                    start: 1.,
                                    elapsed: 240,
                                    bypass: false,
                                };
                                ALLOCS.store(0, Ordering::Relaxed);
                                FREES.store(0, Ordering::Relaxed);
                                COUNTING.store(true, Ordering::Relaxed);
                                let [left, right] = &mut outputs;
                                let mut out = [&mut left[..], &mut right[..]];
                                let input = [&inputs[0][..], &inputs[1][..]];
                                let input = if module == 0 {
                                    &input[..channels as usize]
                                } else {
                                    &[]
                                };
                                assert_eq!(
                                    audio.process_parameters(
                                        input,
                                        &mut out[..channels as usize],
                                        64,
                                        &invalid
                                    ),
                                    Err(parameters::Error::Shape)
                                );
                                assert_eq!(out[0][0], 77.);
                                let first = audio
                                    .process_parameters(
                                        input,
                                        &mut out[..channels as usize],
                                        64,
                                        &[],
                                    )
                                    .unwrap();
                                assert_eq!(first.returned_status(), Some(0));
                                for ch in 0..channels as usize {
                                    for j in 0..64 {
                                        assert_eq!(
                                            out[ch][j],
                                            if module == 0 { inputs[ch][j] } else { 0. }
                                        );
                                    }
                                }
                                for n in 0..10000 {
                                    let frames = if n == 0 {
                                        64
                                    } else {
                                        [0, 1, 7, 32, 64, 13][n % 6]
                                    };
                                    for ch in 0..channels as usize {
                                        out[ch].fill(77.);
                                    }
                                    let points = if n == 0 {
                                        if module == 0 {
                                            &points[..]
                                        } else {
                                            &performance[..]
                                        }
                                    } else {
                                        &[]
                                    };
                                    let report = audio
                                        .process_parameters(
                                            input,
                                            &mut out[..channels as usize],
                                            frames,
                                            points,
                                        )
                                        .unwrap();
                                    assert_eq!(report.returned_status(), Some(0));
                                    assert_eq!(report.failure(), 0);
                                    let mut event = 0;
                                    for j in 0..frames as usize {
                                        while module == 0
                                            && event < points.len()
                                            && points[event].offset == j as u32
                                        {
                                            smoothing.event(points[event]);
                                            event += 1;
                                        }
                                        let gain = smoothing.next();
                                        for ch in 0..channels as usize {
                                            let expected = if module == 0 {
                                                inputs[ch][j]
                                                    * if smoothing.bypass { 1. } else { gain }
                                            } else {
                                                0.
                                            };
                                            assert!((out[ch][j] - expected).abs() < 1e-6);
                                        }
                                    }
                                    for ch in 0..channels as usize {
                                        assert!(
                                            out[ch][frames as usize..].iter().all(|&v| v == 77.)
                                        );
                                    }
                                }
                                COUNTING.store(false, Ordering::Relaxed);
                                assert_eq!(ALLOCS.load(Ordering::Relaxed), 0);
                                assert_eq!(FREES.load(Ordering::Relaxed), 0);
                            })
                            .join()
                            .unwrap()
                    })
                })
                .unwrap();
            child.stop().unwrap();
            if module == 0 {
                assert_eq!(child.read_parameter(parameters, 1).unwrap(), 0.75);
            } else {
                let current = child.capture_state(state, capacity).unwrap().snapshot;
                assert_eq!(current.component().len(), 24);
                assert_eq!(
                    f32::from_le_bytes(current.component()[20..24].try_into().unwrap()),
                    0.3
                );
            }
            // Restore remains a separate complete transaction. Explicit refresh
            // and parameter-aware reprepare are required before the next scope.
            assert!(
                child
                    .restore_state(state, &original)
                    .unwrap()
                    .prepare_required()
            );
            assert!(child.start().is_err());
            child.refresh_parameters(parameters).unwrap();
            child
                .prepare_parameters(
                    parameters,
                    Profile::new(48000., 8, if module == 0 { channels } else { 0 }, channels),
                )
                .unwrap();
            child.start().unwrap();
            child
                .with_audio(|mut audio| {
                    std::thread::scope(|scope| {
                        scope
                            .spawn(|| {
                                let [left, right] = &mut outputs;
                                let mut out = [&mut left[..], &mut right[..]];
                                let input = [&inputs[0][..], &inputs[1][..]];
                                audio
                                    .process_parameters(
                                        if module == 0 {
                                            &input[..channels as usize]
                                        } else {
                                            &[]
                                        },
                                        &mut out[..channels as usize],
                                        8,
                                        &[],
                                    )
                                    .unwrap();
                                for ch in 0..channels as usize {
                                    for j in 0..8 {
                                        assert_eq!(
                                            out[ch][j],
                                            if module == 0 { inputs[ch][j] } else { 0. }
                                        );
                                    }
                                }
                            })
                            .join()
                            .unwrap()
                    })
                })
                .unwrap();
            child.stop().unwrap();
            drop(child);
        }
        match library.retire() {
            Ok(instances::Retirement::Complete { .. }) => {}
            _ => panic!("ordinary parameter core unexpectedly retained"),
        }
    }
    println!(
        "PASS actual safe Rust parameter owner: Gain/Performance four profiles, metadata/control/text, controller/DSP distinction, independent timed Gain smoothing/bypass and Performance serialized Level,10000 variable/zero blocks/profile with zero Rust alloc/free, stopped state recall/explicit refresh/reprepare and joined scopes"
    );
}
