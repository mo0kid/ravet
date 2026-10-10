#![allow(unsafe_code)]
use ravet_foundation::*;
use ravet_native::*;
use std::{cell::Cell, ptr, thread};
thread_local! { static CALLS: Cell<usize> = const { Cell::new(0) }; static PANIC: Cell<u8> = const { Cell::new(0) }; }
const fn range(min: f32, max: f32, scale: ParameterScale, steps: Option<u16>) -> ParameterRange {
    match ParameterRange::new(min, max, scale, steps) {
        Ok(value) => value,
        Err(_) => panic!("fixture range"),
    }
}
static DESCRIPTORS: [ParameterDescriptor<'static>; 3] = [
    ParameterDescriptor {
        id: ParameterId(u32::MAX),
        name: "Cutoff µHz",
        unit: "Hz",
        range: range(20., 20000., ParameterScale::Logarithmic, None),
        default: 200.,
        automatable: true,
    },
    ParameterDescriptor {
        id: ParameterId(7),
        name: "Choices",
        unit: "",
        range: range(-1., 1., ParameterScale::Linear, Some(4)),
        default: 0.,
        automatable: true,
    },
    ParameterDescriptor {
        id: ParameterId(99),
        name: "Readout",
        unit: "%",
        range: range(-1., 1., ParameterScale::Linear, None),
        default: 0.,
        automatable: false,
    },
];
fn schema() -> ParameterSchema<'static, 3> {
    ParameterSchema::new(&DESCRIPTORS).unwrap()
}
fn roles(id: ParameterId) -> NativeParameterRoles {
    NativeParameterRoles {
        bypass: false,
        read_only: id.0 == 99,
    }
}
#[derive(Default)]
struct Fixture;
impl NativeParameterProcessor<3> for Fixture {
    const STATE_PRODUCT: [u8; 8] = *b"RVTCACHE";
    const STATE_VERSION: u32 = 1;
    fn parameter_schema() -> ParameterSchema<'static, 3> {
        CALLS.with(|v| v.set(v.get() + 1));
        assert_ne!(PANIC.with(Cell::get), 1, "schema panic");
        schema()
    }
    fn parameter_roles(id: ParameterId) -> NativeParameterRoles {
        CALLS.with(|v| v.set(v.get() + 1));
        assert_ne!(PANIC.with(Cell::get), 2, "roles panic");
        roles(id)
    }
    fn parameter_snapshot(&self) -> ParameterState<'static, 3> {
        schema().defaults()
    }
    fn restore_parameter_snapshot(
        &mut self,
        _: &ParameterState<'static, 3>,
    ) -> Result<(), ParameterError> {
        Ok(())
    }
}
macro_rules! processor {
    ($sample:ty) => {
        impl Processor<$sample> for Fixture {
            fn prepare(&mut self, _: ProcessConfig) -> Result<(), PrepareError> {
                Ok(())
            }
            fn reset(&mut self) {}
            fn deactivate(&mut self) {}
            fn process(&mut self, _: &mut ProcessBlock<'_, '_, '_, $sample>) -> ProcessStatus {
                ProcessStatus::Silence
            }
        }
    };
}
processor!(f32);
processor!(f64);
export_native_cached_parameters_v1!(ravet_test_native_cached_parameters_v1, Fixture, Fixture, 3);
const SENTINEL: NativeEventV1 = NativeEventV1 {
    offset: 91,
    kind: 99,
    parameter_id: 123,
    note_id: 0,
    scope: 0,
    channel: 0,
    key: 0,
    detail: 0,
    value: 91.,
    words: [0; 4],
};
fn event(id: u32, normalized: f64, offset: u32) -> NativeNormalizedParameterEventV1 {
    NativeNormalizedParameterEventV1 {
        offset,
        id,
        normalized,
    }
}
#[test]
fn safe_cached_metadata_ranges_order_roles_and_normalized_records_are_exact() {
    let cache = NativeParameterCache::new(schema(), roles).unwrap();
    assert_eq!(cache.info(0).unwrap().id, u32::MAX);
    assert_eq!(cache.info(2).unwrap().flags, 4);
    assert!(cache.info(3).is_none());
    assert!((cache.normalize(ParameterId(u32::MAX), 200.).unwrap() - 1. / 3.).abs() < 1e-6);
    assert!((cache.denormalize(ParameterId(u32::MAX), 0.5).unwrap() - 632.4555).abs() < 0.001);
    assert_eq!(cache.denormalize(ParameterId(7), 0.375), Some(0.));
    assert_eq!(cache.normalize(ParameterId(7), 0.25), Some(0.75));
    assert_eq!(
        cache.denormalize(ParameterId(99), 0.75),
        Some(0.5),
        "roles do not implicitly suppress read-only values"
    );
    assert_eq!(cache.denormalize(ParameterId(7), -100.), Some(-1.));
    assert_eq!(cache.denormalize(ParameterId(7), 100.), Some(1.));
    assert_eq!(cache.normalize(ParameterId(0), 0.), None);
    assert_eq!(cache.denormalize(ParameterId(7), f32::INFINITY), None);
    let mut output = [SENTINEL; 5];
    let input = [
        event(7, 0.375, 0),
        event(u32::MAX, 0.5, 0),
        event(99, 0.75, 7),
    ];
    assert_eq!(cache.convert_events(&input, 8, &mut output), Ok(3));
    assert_eq!((output[0].parameter_id, output[0].value), (7, 0.));
    assert_eq!(output[1].offset, 0);
    assert_eq!(output[2].value, 0.5);
    assert_eq!(output[3..], [SENTINEL; 2]);
    for native in &output[..3] {
        assert_eq!(native.kind, 1);
        assert!(native.to_event().is_ok());
    }
    assert_eq!(size_of::<NativeNormalizedParameterEventV1>(), 16);
    assert_eq!(
        std::mem::offset_of!(NativeNormalizedParameterEventV1, normalized),
        8
    );
    assert_eq!(
        size_of::<NativeCachedParameterEntryV1>(),
        16 + 7 * size_of::<extern "C" fn()>()
    );
    assert_eq!(
        std::mem::offset_of!(NativeCachedParameterEntryV1, parameters_entry),
        16
    );
    assert!(ravet_test_native_cached_parameters_v1(16).is_null());
}
#[test]
fn whole_mapping_timing_capacity_and_unknown_id_failures_preserve_destination() {
    let cache = NativeParameterCache::new(schema(), roles).unwrap();
    for bad in [
        event(0, 0.5, 1),
        event(7, f64::NAN, 1),
        event(7, f64::INFINITY, 1),
        event(7, -0.001, 1),
        event(7, 1.001, 1),
        event(7, 0.5, 8),
    ] {
        let input = [event(7, 0.5, 0), bad];
        let mut output = [SENTINEL; 3];
        assert!(cache.convert_events(&input, 8, &mut output).is_err());
        assert_eq!(output, [SENTINEL; 3]);
    }
    let mut output = [SENTINEL; 3];
    assert_eq!(
        cache.convert_events(&[event(7, 0.5, 3), event(7, 0.5, 2)], 8, &mut output),
        Err(NativeParameterMappingError::Unordered { index: 1 })
    );
    assert_eq!(output, [SENTINEL; 3]);
    assert_eq!(
        cache.convert_events(&[event(7, 0.5, 1)], 0, &mut output),
        Err(NativeParameterMappingError::OffsetOutOfRange { index: 0 })
    );
    assert_eq!(output, [SENTINEL; 3]);
    assert_eq!(
        cache.convert_events(&[event(7, 0.5, 0)], 65537, &mut output),
        Err(NativeParameterMappingError::Capacity)
    );
    assert_eq!(
        cache.convert_events(&[event(7, 0.5, 0); 4], 8, &mut output),
        Err(NativeParameterMappingError::Capacity)
    );
    assert_eq!(output, [SENTINEL; 3]);
    assert_eq!(
        cache.convert_events(&[event(7, 0.5, 0); 1025], 8, &mut output),
        Err(NativeParameterMappingError::Capacity)
    );
    assert_eq!(
        cache.convert_events(&[event(7, 0.5, 0)], 0, &mut output),
        Ok(1)
    );
    let empty_schema = ParameterSchema::new(&[]).unwrap();
    let empty =
        NativeParameterCache::new(empty_schema, |_| NativeParameterRoles::default()).unwrap();
    assert_eq!(empty.convert_events(&[], 0, &mut output), Ok(0));
    assert!(empty.info(0).is_none());
    let descriptors = Box::leak(Box::new(std::array::from_fn::<_, 1025, _>(|index| {
        ParameterDescriptor {
            id: ParameterId(index as u32),
            ..DESCRIPTORS[2]
        }
    })));
    assert!(matches!(
        NativeParameterCache::new(ParameterSchema::new(descriptors).unwrap(), |_| panic!(
            "roles must not run for oversize cache"
        )),
        Err(ParameterError::InvalidMetadata)
    ));
}
macro_rules! cycle {
    ($name:ident,$sample:ty,$bits:expr) => {
        #[test]
        fn $name() {
            let table = NativeCachedParameters::<Fixture, $sample, 3>::entry().unwrap();
            assert_eq!(table.sample_bits, $bits);
            let paired = unsafe { &*(table.parameters_entry)() };
            assert_eq!(paired.sample_bits, $bits);
            assert_eq!(paired.parameter_count, 3);
            let cache = (table.create)();
            assert!(!cache.is_null());
            let before = CALLS.with(Cell::get);
            let mut info = NativeParameterInfoV1::default();
            let mut scalar = 91.;
            let input: [_; 1024] = std::array::from_fn(|index| {
                event([7, u32::MAX, 99][index % 3], 0.5, (index / 2) as u32)
            });
            let mut output = [SENTINEL; 1024];
            let heap = allocation_counter::measure(|| {
                for _ in 0..1000 {
                    assert_eq!(unsafe { (table.info)(cache, 2, &mut info) }, 0);
                    assert_eq!(info.id, 99);
                    assert_eq!(info.flags, 4);
                    assert_eq!(
                        unsafe { (table.normalize)(cache, u32::MAX, 200., &mut scalar) },
                        0
                    );
                    assert!((scalar - 1. / 3.).abs() < 1e-6);
                    assert_eq!(
                        unsafe { (table.denormalize)(cache, 7, 0.375, &mut scalar) },
                        0
                    );
                    assert_eq!(scalar, 0.);
                    assert_eq!(
                        unsafe {
                            (table.convert_events)(
                                cache,
                                input.as_ptr(),
                                1024,
                                512,
                                output.as_mut_ptr(),
                                1024,
                            )
                        },
                        0
                    );
                    for (index, value) in output.iter().enumerate() {
                        assert_eq!(value.offset, (index / 2) as u32);
                        assert_eq!(value.parameter_id, input[index].id);
                        assert_eq!(value.kind, 1);
                        if value.parameter_id == u32::MAX {
                            assert!((value.value - 632.4555).abs() < 0.001);
                        } else {
                            assert_eq!(value.value, 0.);
                        }
                    }
                }
            });
            assert_eq!((heap.count_total, heap.bytes_total), (0, 0));
            assert_eq!(CALLS.with(Cell::get), before);
            let previous = info;
            assert_eq!(unsafe { (table.info)(cache, 3, &mut info) }, 1);
            assert_eq!(info, previous);
            scalar = 91.;
            assert_eq!(unsafe { (table.normalize)(cache, 0, 0., &mut scalar) }, 1);
            assert_eq!(scalar, 91.);
            output.fill(SENTINEL);
            assert_eq!(
                unsafe {
                    (table.convert_events)(
                        cache,
                        input.as_ptr(),
                        1024,
                        512,
                        output.as_mut_ptr(),
                        1023,
                    )
                },
                1
            );
            assert_eq!(output, [SENTINEL; 1024]);
            assert_eq!(
                unsafe { (table.convert_events)(cache, ptr::null(), 0, 0, ptr::null_mut(), 0) },
                0
            );
            unsafe {
                (table.destroy)(cache);
            }
        }
    };
}
cycle!(
    cached_f32_companion_full_capacity_heap_free_no_providers,
    f32,
    32
);
cycle!(
    cached_f64_companion_full_capacity_heap_free_no_providers,
    f64,
    64
);
#[test]
fn control_provider_panics_do_not_invalidate_existing_cache_or_run_on_cached_calls() {
    let table = NativeCachedParameters::<Fixture, f32, 3>::entry().unwrap();
    let cache = (table.create)();
    assert!(!cache.is_null());
    for mode in [1, 2] {
        PANIC.with(|v| v.set(mode));
        assert!(NativeCachedParameters::<Fixture, f32, 3>::entry().is_none());
        assert!((table.create)().is_null());
        let calls = CALLS.with(Cell::get);
        let mut output = 91.;
        assert_eq!(
            unsafe { (table.denormalize)(cache, 7, 0.375, &mut output) },
            0
        );
        assert_eq!(output, 0.);
        assert_eq!(CALLS.with(Cell::get), calls);
    }
    PANIC.with(|v| v.set(0));
    unsafe {
        (table.destroy)(cache);
    }
}
#[test]
fn shared_cached_foreign_readers_keep_independent_results_without_heap_actions() {
    let table = NativeCachedParameters::<Fixture, f32, 3>::entry().unwrap();
    let cache = (table.create)();
    assert!(!cache.is_null());
    let address = cache as usize;
    let readers: Vec<_> = (0..4)
        .map(|_| {
            thread::spawn(move || {
                let mut output = 91.;
                let input = [event(7, 0.375, 0), event(u32::MAX, 0.5, 0)];
                let mut events = [SENTINEL; 3];
                let heap = allocation_counter::measure(|| {
                    for _ in 0..10000 {
                        assert_eq!(
                            unsafe {
                                (table.denormalize)(address as *const _, 7, 0.375, &mut output)
                            },
                            0
                        );
                        assert_eq!(output, 0.);
                        assert_eq!(
                            unsafe {
                                (table.convert_events)(
                                    address as *const _,
                                    input.as_ptr(),
                                    2,
                                    1,
                                    events.as_mut_ptr(),
                                    3,
                                )
                            },
                            0
                        );
                        assert_eq!(events[2], SENTINEL);
                    }
                });
                assert_eq!((heap.count_total, heap.bytes_total), (0, 0));
                assert_eq!(CALLS.with(Cell::get), 0);
            })
        })
        .collect();
    for reader in readers {
        reader.join().unwrap();
    }
    unsafe {
        (table.destroy)(cache);
    }
}
