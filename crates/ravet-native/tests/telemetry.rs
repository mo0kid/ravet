#![allow(unsafe_code)]
use ravet_foundation::*;
use ravet_native::*;
use std::{cell::Cell, ffi::c_void, ptr, thread};
thread_local! { static CALLS: Cell<usize> = const { Cell::new(0) }; static MODE: Cell<u8> = const { Cell::new(0) }; }
static DESCRIPTORS: [NativeTelemetryDescriptor; 3] = [
    NativeTelemetryDescriptor {
        id: u32::MAX,
        name: "Peak µ",
        unit: "amp",
        display_minimum: 0.,
        display_maximum: 1.,
    },
    NativeTelemetryDescriptor {
        id: 7,
        name: "DC",
        unit: "",
        display_minimum: -1.,
        display_maximum: 1.,
    },
    NativeTelemetryDescriptor {
        id: 99,
        name: "Tiny",
        unit: "",
        display_minimum: 0.,
        display_maximum: 1.,
    },
];
#[derive(Default)]
struct Fixture {
    reset: bool,
}
impl NativeTelemetryProcessor<3> for Fixture {
    fn telemetry_descriptors() -> &'static [NativeTelemetryDescriptor; 3] {
        CALLS.with(|v| v.set(v.get() + 1));
        assert_ne!(MODE.with(Cell::get), 9, "provider panic");
        &DESCRIPTORS
    }
    fn audio_telemetry_snapshot(&self) -> [NativeTelemetryValueV1; 3] {
        let mut values = [
            NativeTelemetryValueV1 {
                id: 99,
                flags: 0,
                value: f64::from_bits(1),
                samples: 0,
            },
            NativeTelemetryValueV1 {
                id: u32::MAX,
                flags: NATIVE_TELEMETRY_NONFINITE_SAMPLES,
                value: if self.reset { 0. } else { f64::MAX },
                samples: u64::MAX,
            },
            NativeTelemetryValueV1 {
                id: 7,
                flags: 0,
                value: -0.,
                samples: 1,
            },
        ];
        match MODE.with(Cell::get) {
            1 => panic!("snapshot panic"),
            2 => values[2].id = 12,
            3 => values[2].id = 99,
            4 => values[2].value = f64::INFINITY,
            5 => values[2].flags = 2,
            6 => values[2].value = f64::NAN,
            _ => {}
        }
        values
    }
}
macro_rules! processor {
    ($sample:ty) => {
        impl Processor<$sample> for Fixture {
            fn prepare(&mut self, _: ProcessConfig) -> Result<(), PrepareError> {
                Ok(())
            }
            fn reset(&mut self) {
                self.reset = true;
            }
            fn deactivate(&mut self) {}
            fn process(&mut self, _: &mut ProcessBlock<'_, '_, '_, $sample>) -> ProcessStatus {
                ProcessStatus::Silence
            }
        }
    };
}
processor!(f32);
processor!(f64);
export_native_telemetry_v1!(ravet_test_native_telemetry_v1, Fixture, Fixture, 3);
fn config() -> NativeProcessorConfigV1 {
    let mut buses = [NativeBusLayoutV1::default(); 9];
    buses[0].outputs = 1;
    NativeProcessorConfigV1 {
        version: 1,
        maximum_frames: 8,
        maximum_events: 0,
        maximum_output_events: 0,
        bus_count: 1,
        reserved: 0,
        sample_rate: 48000.,
        buses,
    }
}
const SENTINEL: NativeTelemetryValueV1 = NativeTelemetryValueV1 {
    id: 77,
    flags: 99,
    value: 91.,
    samples: 91,
};
struct Owners {
    table: &'static NativeTelemetryEntryV1,
    core: &'static NativeProcessorEntryV1,
    cache: *mut c_void,
    handle: *mut c_void,
}
impl Owners {
    fn new<S: Sample + Send + 'static>() -> Self
    where
        Fixture: Processor<S>,
    {
        let table = NativeTelemetry::<Fixture, S, 3>::entry().unwrap();
        let core = unsafe { &*(table.processor_entry)() };
        let cache = (table.create_cache)();
        let handle = (core.create)();
        assert!(!cache.is_null() && !handle.is_null());
        Self {
            table,
            core,
            cache,
            handle,
        }
    }
    fn snapshot(&self, output: &mut [NativeTelemetryValueV1]) -> i32 {
        unsafe {
            (self.table.snapshot)(
                self.handle,
                self.cache,
                output.as_mut_ptr(),
                output.len() as u32,
            )
        }
    }
    fn prepare(&self) {
        assert_eq!(unsafe { (self.core.prepare)(self.handle, &config()) }, 0);
    }
}
impl Drop for Owners {
    fn drop(&mut self) {
        unsafe {
            (self.table.destroy_cache)(self.cache);
            (self.core.destroy)(self.handle);
        }
    }
}
macro_rules! cycle {
    ($name:ident,$sample:ty,$bits:expr) => {
        #[test]
        fn $name() {
            let owners = Owners::new::<$sample>();
            assert_eq!(owners.table.sample_bits, $bits);
            assert_eq!(owners.table.telemetry_count, 3);
            assert_eq!(owners.core.sample_bits, $bits);
            let mut output = [SENTINEL; 4];
            assert_eq!(owners.snapshot(&mut output), 1);
            assert_eq!(output, [SENTINEL; 4]);
            owners.prepare();
            let calls = CALLS.with(Cell::get);
            let mut info = NativeTelemetryInfoV1::default();
            let heap = allocation_counter::measure(|| {
                for _ in 0..1000 {
                    assert_eq!(
                        unsafe { (owners.table.info)(owners.cache, 0, &mut info) },
                        0
                    );
                    assert_eq!(info.id, u32::MAX);
                    assert_eq!(info.name_length, 7);
                    assert_eq!(info.unit_length, 3);
                    assert_eq!(info.reserved, 0);
                    assert_eq!(info.name[7..], [0; 249]);
                    assert_eq!(owners.snapshot(&mut output), 0);
                    assert_eq!(
                        output[0],
                        NativeTelemetryValueV1 {
                            id: u32::MAX,
                            flags: 1,
                            value: f64::MAX,
                            samples: u64::MAX
                        }
                    );
                    assert_eq!(output[1].id, 7);
                    assert_eq!(output[1].value.to_bits(), (-0_f64).to_bits());
                    assert_eq!(output[1].samples, 1);
                    assert_eq!(output[2].id, 99);
                    assert_eq!(output[2].value.to_bits(), 1);
                    assert_eq!(output[3], SENTINEL);
                }
            });
            assert_eq!(
                (heap.count_total, heap.count_current, heap.bytes_total),
                (0, 0, 0)
            );
            assert_eq!(CALLS.with(Cell::get), calls);
            assert_eq!(unsafe { (owners.core.reset)(owners.handle) }, 0);
            assert_eq!(owners.snapshot(&mut output), 0);
            assert_eq!(output[0].value, 0.);
            assert_eq!(unsafe { (owners.core.deactivate)(owners.handle) }, 0);
            assert_eq!(owners.snapshot(&mut output), 1);
            owners.prepare();
            assert_eq!(owners.snapshot(&mut output), 0);
        }
    };
}
cycle!(
    f32_prepared_telemetry_preserves_raw_f64_counts_and_zero_heap_provider_calls,
    f32,
    32
);
cycle!(
    f64_prepared_telemetry_preserves_raw_f64_counts_and_zero_heap_provider_calls,
    f64,
    64
);
#[test]
fn metadata_text_display_duplicate_and_count_limits_preflight_before_cache_allocations() {
    let cache = NativeTelemetryCache::new(&DESCRIPTORS).unwrap();
    assert_eq!(cache.info(2).unwrap().id, 99);
    assert!(cache.info(3).is_none());
    let mut bad = DESCRIPTORS;
    for descriptor in [
        NativeTelemetryDescriptor {
            id: 7,
            ..DESCRIPTORS[0]
        },
        NativeTelemetryDescriptor {
            name: "",
            ..DESCRIPTORS[0]
        },
        NativeTelemetryDescriptor {
            name: "A\0B",
            ..DESCRIPTORS[0]
        },
        NativeTelemetryDescriptor {
            unit: "\0",
            ..DESCRIPTORS[0]
        },
        NativeTelemetryDescriptor {
            display_minimum: f64::NAN,
            ..DESCRIPTORS[0]
        },
        NativeTelemetryDescriptor {
            display_maximum: f64::INFINITY,
            ..DESCRIPTORS[0]
        },
        NativeTelemetryDescriptor {
            display_minimum: 1.,
            ..DESCRIPTORS[0]
        },
        NativeTelemetryDescriptor {
            display_minimum: 2.,
            ..DESCRIPTORS[0]
        },
    ] {
        bad[0] = descriptor;
        let heap =
            allocation_counter::measure(|| assert!(NativeTelemetryCache::new(&bad).is_err()));
        assert_eq!(heap.count_total, 0);
    }
    let name255 = Box::leak("µ".repeat(127).to_string().into_boxed_str());
    let name255 = Box::leak(format!("{name255}x").into_boxed_str());
    let unit63 = Box::leak("x".repeat(63).into_boxed_str());
    bad[0] = NativeTelemetryDescriptor {
        name: name255,
        unit: unit63,
        ..DESCRIPTORS[0]
    };
    assert!(NativeTelemetryCache::new(&bad).is_ok());
    bad[0].name = Box::leak("x".repeat(256).into_boxed_str());
    assert!(NativeTelemetryCache::new(&bad).is_err());
    bad[0].name = "Valid";
    bad[0].unit = Box::leak("x".repeat(64).into_boxed_str());
    assert!(NativeTelemetryCache::new(&bad).is_err());
    let too_many = [DESCRIPTORS[0]; 1025];
    let heap = allocation_counter::measure(|| {
        assert!(matches!(
            NativeTelemetryCache::new(&too_many),
            Err(NativeTelemetryError::Capacity)
        ))
    });
    assert_eq!(heap.count_total, 0);
    assert!(NativeTelemetryCache::new(&[]).is_ok());
    assert_eq!(size_of::<NativeTelemetryInfoV1>(), 352);
    assert_eq!(std::mem::offset_of!(NativeTelemetryInfoV1, name), 32);
    assert_eq!(size_of::<NativeTelemetryValueV1>(), 24);
    assert_eq!(std::mem::offset_of!(NativeTelemetryValueV1, samples), 16);
    assert_eq!(
        size_of::<NativeTelemetryEntryV1>(),
        16 + 5 * size_of::<extern "C" fn()>()
    );
    assert_eq!(
        std::mem::offset_of!(NativeTelemetryEntryV1, processor_entry),
        16
    );
    assert!(ravet_test_native_telemetry_v1(16).is_null());
}
#[test]
fn malformed_snapshot_prefix_pointers_capacity_and_provider_failures_preserve_outputs() {
    let owners = Owners::new::<f32>();
    owners.prepare();
    let mut output = [SENTINEL; 4];
    for mode in 2..=6 {
        MODE.with(|v| v.set(mode));
        assert_eq!(owners.snapshot(&mut output), 1);
        assert_eq!(output, [SENTINEL; 4]);
    }
    MODE.with(|v| v.set(0));
    assert_eq!(owners.snapshot(&mut output[..2]), 1);
    assert_eq!(output, [SENTINEL; 4]);
    assert_eq!(
        unsafe { (owners.table.snapshot)(owners.handle, owners.cache, output.as_mut_ptr(), 1025) },
        1
    );
    assert_eq!(
        unsafe { (owners.table.snapshot)(owners.handle, owners.cache, ptr::null_mut(), 3) },
        1
    );
    assert_eq!(
        unsafe { (owners.table.snapshot)(owners.handle, ptr::null(), output.as_mut_ptr(), 4) },
        1
    );
    assert_eq!(
        unsafe { (owners.table.snapshot)(ptr::null_mut(), owners.cache, output.as_mut_ptr(), 4) },
        1
    );
    let misaligned = unsafe { output.as_mut_ptr().cast::<u8>().add(1) }.cast();
    assert_eq!(
        unsafe { (owners.table.snapshot)(owners.handle, owners.cache, misaligned, 3) },
        1
    );
    assert_eq!(output, [SENTINEL; 4]);
    let mut info = NativeTelemetryInfoV1 {
        id: 77,
        ..NativeTelemetryInfoV1::default()
    };
    assert_eq!(
        unsafe { (owners.table.info)(owners.cache, 3, &mut info) },
        1
    );
    assert_eq!(info.id, 77);
    MODE.with(|v| v.set(9));
    assert!(NativeTelemetry::<Fixture, f32, 3>::entry().is_none());
    assert!((owners.table.create_cache)().is_null());
    let calls = CALLS.with(Cell::get);
    assert_eq!(owners.snapshot(&mut output), 0);
    assert_eq!(
        unsafe { (owners.table.info)(owners.cache, 0, &mut info) },
        0
    );
    assert_eq!(CALLS.with(Cell::get), calls);
    MODE.with(|v| v.set(0));
}
#[test]
fn telemetry_panic_poison_is_shared_with_processor_and_fresh_prepare_recovers() {
    let owners = Owners::new::<f64>();
    owners.prepare();
    let mut output = [SENTINEL; 4];
    MODE.with(|v| v.set(1));
    assert_eq!(owners.snapshot(&mut output), 2);
    assert_eq!(output, [SENTINEL; 4]);
    MODE.with(|v| v.set(0));
    assert_eq!(owners.snapshot(&mut output), 2);
    assert_eq!(unsafe { (owners.core.reset)(owners.handle) }, 2);
    let mut timing = NativeProcessorTimingV1::default();
    assert_eq!(
        unsafe { (owners.core.timing)(owners.handle, &mut timing) },
        2
    );
    owners.prepare();
    assert_eq!(owners.snapshot(&mut output), 0);
}
#[test]
fn concurrent_cached_info_reads_are_provider_and_heap_free() {
    let owners = Owners::new::<f32>();
    let address = owners.cache as usize;
    let table = owners.table;
    let readers: Vec<_> = (0..4)
        .map(|_| {
            thread::spawn(move || {
                let mut info = NativeTelemetryInfoV1::default();
                let heap = allocation_counter::measure(|| {
                    for _ in 0..10000 {
                        assert_eq!(
                            unsafe { (table.info)(address as *const _, 2, &mut info) },
                            0
                        );
                        assert_eq!(info.id, 99);
                    }
                });
                assert_eq!((heap.count_total, heap.count_current), (0, 0));
                assert_eq!(CALLS.with(Cell::get), 0);
            })
        })
        .collect();
    for reader in readers {
        reader.join().unwrap();
    }
}
const fn many_descriptors() -> [NativeTelemetryDescriptor; 1024] {
    let mut fields = [DESCRIPTORS[0]; 1024];
    let mut index = 0;
    while index < 1024 {
        fields[index].id = index as u32;
        index += 1;
    }
    fields
}
static MANY: [NativeTelemetryDescriptor; 1024] = many_descriptors();
#[derive(Default)]
struct Empty;
#[derive(Default)]
struct Many;
macro_rules! bounded {
    ($name:ty,$n:expr,$descriptors:expr) => {
        impl NativeTelemetryProcessor<$n> for $name {
            fn telemetry_descriptors() -> &'static [NativeTelemetryDescriptor; $n] {
                $descriptors
            }
            fn audio_telemetry_snapshot(&self) -> [NativeTelemetryValueV1; $n] {
                std::array::from_fn(|index| NativeTelemetryValueV1 {
                    id: ($n as usize).wrapping_sub(1 + index) as u32,
                    flags: 0,
                    value: index as f64,
                    samples: u64::MAX - index as u64,
                })
            }
        }
        impl Processor<f32> for $name {
            fn prepare(&mut self, _: ProcessConfig) -> Result<(), PrepareError> {
                Ok(())
            }
            fn reset(&mut self) {}
            fn deactivate(&mut self) {}
            fn process(&mut self, _: &mut ProcessBlock<'_, '_, '_, f32>) -> ProcessStatus {
                ProcessStatus::Silence
            }
        }
    };
}
bounded!(Empty, 0, &[]);
bounded!(Many, 1024, &MANY);
#[test]
fn empty_null_prefix_and_full_capacity_snapshots_use_complete_bounded_storage() {
    let table = NativeTelemetry::<Empty, f32, 0>::entry().unwrap();
    let cache = (table.create_cache)();
    let core = unsafe { &*(table.processor_entry)() };
    let handle = (core.create)();
    assert_eq!(unsafe { (core.prepare)(handle, &config()) }, 0);
    assert_eq!(
        unsafe { (table.snapshot)(handle, cache, ptr::null_mut(), 0) },
        0
    );
    let mut output = [SENTINEL; 1];
    assert_eq!(
        unsafe { (table.snapshot)(handle, cache, output.as_mut_ptr(), 1) },
        0
    );
    assert_eq!(output, [SENTINEL; 1]);
    unsafe {
        (table.destroy_cache)(cache);
        (core.destroy)(handle);
    }
    let table = NativeTelemetry::<Many, f32, 1024>::entry().unwrap();
    let cache = (table.create_cache)();
    let core = unsafe { &*(table.processor_entry)() };
    let handle = (core.create)();
    assert_eq!(unsafe { (core.prepare)(handle, &config()) }, 0);
    let mut output = [SENTINEL; 1024];
    let heap = allocation_counter::measure(|| {
        for _ in 0..1000 {
            assert_eq!(
                unsafe { (table.snapshot)(handle, cache, output.as_mut_ptr(), 1024) },
                0
            );
            for (index, field) in output.iter().enumerate() {
                assert_eq!(
                    *field,
                    NativeTelemetryValueV1 {
                        id: index as u32,
                        flags: 0,
                        value: (1023 - index) as f64,
                        samples: u64::MAX - (1023 - index) as u64
                    }
                );
            }
        }
    });
    assert_eq!(
        (heap.count_total, heap.count_current, heap.bytes_total),
        (0, 0, 0)
    );
    output.fill(SENTINEL);
    assert_eq!(
        unsafe { (table.snapshot)(handle, cache, output.as_mut_ptr(), 1023) },
        1
    );
    assert_eq!(output, [SENTINEL; 1024]);
    unsafe {
        (table.destroy_cache)(cache);
        (core.destroy)(handle);
    }
}
