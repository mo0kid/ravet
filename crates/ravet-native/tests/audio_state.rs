#![allow(unsafe_code)]
use ravet_foundation::*;
use ravet_native::*;
use std::{cell::Cell, ffi::c_void, ptr};
thread_local! { static PROVIDERS: Cell<usize> = const { Cell::new(0) }; static MODE: Cell<u8> = const { Cell::new(0) }; }
const fn range() -> ParameterRange {
    match ParameterRange::new(-10000., 10000., ParameterScale::Linear, None) {
        Ok(value) => value,
        Err(_) => panic!("range"),
    }
}
static DESCRIPTORS: [ParameterDescriptor<'static>; 3] = [
    ParameterDescriptor {
        id: ParameterId(u32::MAX),
        name: "First",
        unit: "",
        range: range(),
        default: 0.,
        automatable: true,
    },
    ParameterDescriptor {
        id: ParameterId(200),
        name: "Second",
        unit: "",
        range: range(),
        default: 0.,
        automatable: false,
    },
    ParameterDescriptor {
        id: ParameterId(300),
        name: "Third",
        unit: "",
        range: range(),
        default: 0.,
        automatable: true,
    },
];
static REVERSED: [ParameterDescriptor<'static>; 3] =
    [DESCRIPTORS[2], DESCRIPTORS[1], DESCRIPTORS[0]];
static WIDE: [ParameterDescriptor<'static>; 3] = [
    ParameterDescriptor {
        range: match ParameterRange::new(-20000., 20000., ParameterScale::Linear, None) {
            Ok(v) => v,
            Err(_) => panic!("wide"),
        },
        ..DESCRIPTORS[0]
    },
    DESCRIPTORS[1],
    DESCRIPTORS[2],
];
static WRONG: [ParameterDescriptor<'static>; 3] = [
    ParameterDescriptor {
        id: ParameterId(123),
        ..DESCRIPTORS[0]
    },
    DESCRIPTORS[1],
    DESCRIPTORS[2],
];
fn schema() -> ParameterSchema<'static, 3> {
    ParameterSchema::new(&DESCRIPTORS).unwrap()
}
#[derive(Default)]
struct Fixture {
    values: [f32; 3],
}
impl NativeParameterProcessor<3> for Fixture {
    const STATE_PRODUCT: [u8; 8] = *b"RVTAUDIO";
    const STATE_VERSION: u32 = 1;
    fn parameter_schema() -> ParameterSchema<'static, 3> {
        PROVIDERS.with(|v| v.set(v.get() + 1));
        assert_ne!(MODE.with(Cell::get), 9, "provider panic");
        schema()
    }
    fn parameter_roles(id: ParameterId) -> NativeParameterRoles {
        PROVIDERS.with(|v| v.set(v.get() + 1));
        NativeParameterRoles {
            bypass: false,
            read_only: id.0 == 200,
        }
    }
    fn parameter_snapshot(&self) -> ParameterState<'static, 3> {
        panic!("stopped snapshot called on audio")
    }
    fn restore_parameter_snapshot(
        &mut self,
        _: &ParameterState<'static, 3>,
    ) -> Result<(), ParameterError> {
        panic!("stopped restore called on audio")
    }
    fn decode_parameter_snapshot(_: &[u8]) -> Result<ParameterState<'static, 3>, ParameterError> {
        panic!("codec called on audio")
    }
}
impl NativeAudioParameterProcessor<3> for Fixture {
    fn audio_parameter_snapshot(&self) -> ParameterState<'static, 3> {
        match MODE.with(Cell::get) {
            1 => panic!("snapshot panic"),
            2 => ParameterSchema::new(&WRONG)
                .unwrap()
                .state(self.values)
                .unwrap(),
            3 => ParameterSchema::new(&WIDE)
                .unwrap()
                .state([15000., self.values[1], self.values[2]])
                .unwrap(),
            _ => ParameterSchema::new(&REVERSED)
                .unwrap()
                .state([self.values[2], self.values[1], self.values[0]])
                .unwrap(),
        }
    }
    fn apply_audio_parameter_snapshot(
        &mut self,
        state: &ParameterState<'static, 3>,
    ) -> Result<(), ParameterError> {
        assert_ne!(MODE.with(Cell::get), 4, "restore panic");
        if MODE.with(Cell::get) == 5 {
            return Err(ParameterError::InvalidMetadata);
        }
        self.values = *state.values();
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
            fn process(&mut self, block: &mut ProcessBlock<'_, '_, '_, $sample>) -> ProcessStatus {
                for event in block.events {
                    if let EventData::ParameterValue { id, value } = event.data {
                        if let Some(index) = schema().index(id) {
                            self.values[index] = value;
                        }
                    }
                }
                if let Some(output) = block.audio.output_mut(0) {
                    output.fill(self.values[0] as $sample);
                }
                ProcessStatus::Continue
            }
        }
    };
}
processor!(f32);
processor!(f64);
export_native_audio_state_v1!(ravet_test_native_audio_state_v1, Fixture, Fixture, 3);
fn config() -> NativeProcessorConfigV1 {
    let mut buses = [NativeBusLayoutV1::default(); 9];
    buses[0].outputs = 1;
    NativeProcessorConfigV1 {
        version: 1,
        maximum_frames: 8,
        maximum_events: 2,
        maximum_output_events: 0,
        bus_count: 1,
        reserved: 0,
        sample_rate: 48000.,
        buses,
    }
}
const SENTINEL: NativeParameterValueV1 = NativeParameterValueV1 { id: 77, value: 91. };
fn fields(value: f32) -> [NativeParameterValueV1; 3] {
    [
        NativeParameterValueV1 {
            id: 300,
            value: value * 2.,
        },
        NativeParameterValueV1 {
            id: u32::MAX,
            value,
        },
        NativeParameterValueV1 {
            id: 200,
            value: -value,
        },
    ]
}
struct Owners {
    table: &'static NativeAudioStateEntryV1,
    cached: &'static NativeCachedParameterEntryV1,
    core: &'static NativeProcessorEntryV1,
    cache: *mut c_void,
    handle: *mut c_void,
}
impl Owners {
    fn new<S: Sample + Send + 'static>() -> Self
    where
        Fixture: Processor<S>,
    {
        let table = NativeAudioState::<Fixture, S, 3>::entry().unwrap();
        let cached = unsafe { &*(table.cached_parameters_entry)() };
        let params = unsafe { &*(cached.parameters_entry)() };
        let core = unsafe { &*(params.processor_entry)() };
        let cache = (cached.create)();
        let handle = (core.create)();
        assert!(!cache.is_null() && !handle.is_null());
        Self {
            table,
            cached,
            core,
            cache,
            handle,
        }
    }
    fn apply(&self, values: &[NativeParameterValueV1]) -> i32 {
        unsafe {
            (self.table.apply)(
                self.handle,
                self.cache,
                values.as_ptr(),
                values.len() as u32,
            )
        }
    }
    fn snapshot(&self, values: &mut [NativeParameterValueV1]) -> i32 {
        unsafe {
            (self.table.snapshot)(
                self.handle,
                self.cache,
                values.as_mut_ptr(),
                values.len() as u32,
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
            (self.cached.destroy)(self.cache);
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
            assert_eq!(
                owners.table.byte_size as usize,
                size_of::<NativeAudioStateEntryV1>()
            );
            assert_eq!(owners.core.sample_bits, $bits);
            assert_eq!(owners.apply(&fields(1.)), 1);
            let mut values = [SENTINEL; 4];
            assert_eq!(owners.snapshot(&mut values), 1);
            assert_eq!(values, [SENTINEL; 4]);
            owners.prepare();
            let calls = PROVIDERS.with(Cell::get);
            let heap = allocation_counter::measure(|| {
                for cycle in 0..1000 {
                    let value = (cycle % 100) as f32;
                    assert_eq!(owners.apply(&fields(value)), 0);
                    assert_eq!(owners.snapshot(&mut values), 0);
                    assert_eq!(
                        values[0],
                        NativeParameterValueV1 {
                            id: u32::MAX,
                            value
                        }
                    );
                    assert_eq!(
                        values[1],
                        NativeParameterValueV1 {
                            id: 200,
                            value: -value
                        }
                    );
                    assert_eq!(
                        values[2],
                        NativeParameterValueV1 {
                            id: 300,
                            value: value * 2.
                        }
                    );
                    assert_eq!(values[3], SENTINEL);
                }
            });
            assert_eq!(
                (heap.count_total, heap.bytes_total, heap.count_current),
                (0, 0, 0)
            );
            assert_eq!(PROVIDERS.with(Cell::get), calls);
            assert_eq!(unsafe { (owners.core.reset)(owners.handle) }, 0);
            assert_eq!(owners.snapshot(&mut values), 0);
            assert_eq!(values[0].value, 99.);
            // Zero-frame processing still accepts automation; snapshot observes
            // the new base target without a codec, restore or audio output write.
            let mut samples = [91. as $sample; 8];
            let outputs = [samples.as_mut_ptr()];
            let bus = NativeProcessorBusV1 {
                input_channels: 0,
                output_channels: 1,
                inputs: ptr::null(),
                outputs: outputs.as_ptr().cast(),
            };
            let event = NativeEventV1 {
                kind: native_event_kind::PARAMETER_VALUE,
                parameter_id: u32::MAX,
                value: 5.,
                ..NativeEventV1::default()
            };
            let mut result = NativeProcessorResultV1 {
                status: 2,
                event_count: 0,
            };
            assert_eq!(
                unsafe {
                    (owners.core.process)(
                        owners.handle,
                        &bus,
                        1,
                        0,
                        &event,
                        1,
                        ptr::null(),
                        ptr::null_mut(),
                        0,
                        &mut result,
                    )
                },
                0
            );
            assert_eq!(samples, [91. as $sample; 8]);
            assert_eq!(owners.snapshot(&mut values), 0);
            assert_eq!(values[0].value, 5.);
            assert_eq!(
                unsafe {
                    (owners.core.process)(
                        owners.handle,
                        &bus,
                        1,
                        8,
                        ptr::null(),
                        0,
                        ptr::null(),
                        ptr::null_mut(),
                        0,
                        &mut result,
                    )
                },
                0
            );
            assert_eq!(samples, [5. as $sample; 8]);

            assert_eq!(unsafe { (owners.core.deactivate)(owners.handle) }, 0);
            assert_eq!(owners.snapshot(&mut values), 1);
            owners.prepare();
            assert_eq!(owners.snapshot(&mut values), 0);
        }
    };
}
cycle!(
    prepared_f32_complete_reordered_state_heap_free_and_no_providers,
    f32,
    32
);
cycle!(
    prepared_f64_complete_reordered_state_heap_free_and_no_providers,
    f64,
    64
);
#[test]
fn complete_malformed_input_capacity_and_snapshot_errors_are_atomic() {
    let owners = Owners::new::<f32>();
    owners.prepare();
    assert_eq!(owners.apply(&fields(7.)), 0);
    let mut bad = fields(3.);
    for field in [
        NativeParameterValueV1 { id: 123, value: 1. },
        NativeParameterValueV1 {
            id: u32::MAX,
            value: 1.,
        },
        NativeParameterValueV1 {
            id: 300,
            value: f32::NAN,
        },
        NativeParameterValueV1 {
            id: 300,
            value: f32::INFINITY,
        },
        NativeParameterValueV1 {
            id: 300,
            value: 10001.,
        },
    ] {
        bad[0] = field;
        assert_eq!(owners.apply(&bad), 1);
        let mut values = [SENTINEL; 4];
        assert_eq!(owners.snapshot(&mut values), 0);
        assert_eq!(values[0].value, 7.);
    }
    assert_eq!(owners.apply(&fields(2.)[..2]), 1);
    assert_eq!(owners.apply(&[SENTINEL; 4]), 1);
    let mut out = [SENTINEL; 4];
    assert_eq!(owners.snapshot(&mut out[..2]), 1);
    assert_eq!(out, [SENTINEL; 4]);
    for mode in [2, 3] {
        MODE.with(|v| v.set(mode));
        assert_eq!(owners.snapshot(&mut out), 1);
        assert_eq!(out, [SENTINEL; 4]);
    }
    MODE.with(|v| v.set(5));
    assert_eq!(owners.apply(&fields(9.)), 1);
    MODE.with(|v| v.set(0));
    assert_eq!(owners.snapshot(&mut out), 0);
    assert_eq!(out[0].value, 7.);
    assert_eq!(
        unsafe { (owners.table.apply)(owners.handle, ptr::null(), fields(1.).as_ptr(), 3) },
        1
    );
    assert_eq!(
        unsafe { (owners.table.apply)(ptr::null_mut(), owners.cache, fields(1.).as_ptr(), 3) },
        1
    );
    assert_eq!(
        unsafe { (owners.table.apply)(owners.handle, owners.cache, ptr::null(), 3) },
        1
    );
    assert_eq!(
        unsafe { (owners.table.snapshot)(owners.handle, owners.cache, ptr::null_mut(), 4) },
        1
    );
    assert_eq!(
        unsafe { (owners.table.snapshot)(owners.handle, owners.cache, out.as_mut_ptr(), 1025) },
        1
    );
    let mut misaligned = [SENTINEL; 5];
    let pointer = unsafe { misaligned.as_mut_ptr().cast::<u8>().add(1) }.cast();
    assert_eq!(
        unsafe { (owners.table.snapshot)(owners.handle, owners.cache, pointer, 3) },
        1
    );
}
#[test]
fn audio_panics_poison_shared_processor_until_fresh_prepare_but_provider_failure_isolated() {
    let owners = Owners::new::<f64>();
    owners.prepare();
    let mut out = [SENTINEL; 4];
    for mode in [1, 4] {
        MODE.with(|v| v.set(mode));
        assert_eq!(
            if mode == 1 {
                owners.snapshot(&mut out)
            } else {
                owners.apply(&fields(2.))
            },
            2
        );
        assert_eq!(out, [SENTINEL; 4]);
        MODE.with(|v| v.set(0));
        assert_eq!(owners.apply(&fields(1.)), 2);
        assert_eq!(owners.snapshot(&mut out), 2);
        assert_eq!(unsafe { (owners.core.reset)(owners.handle) }, 2);
        let mut timing = NativeProcessorTimingV1::default();
        assert_eq!(
            unsafe { (owners.core.timing)(owners.handle, &mut timing) },
            2
        );
        owners.prepare();
        assert_eq!(owners.apply(&fields(1.)), 0);
    }
    MODE.with(|v| v.set(9));
    assert!(NativeAudioState::<Fixture, f64, 3>::entry().is_none());
    let calls = PROVIDERS.with(Cell::get);
    assert_eq!(owners.apply(&fields(2.)), 0);
    assert_eq!(owners.snapshot(&mut out), 0);
    assert_eq!(PROVIDERS.with(Cell::get), calls);
    MODE.with(|v| v.set(0));
    assert!(ravet_test_native_audio_state_v1(16).is_null());
    assert_eq!(
        size_of::<NativeAudioStateEntryV1>(),
        16 + 3 * size_of::<extern "C" fn()>()
    );
    assert_eq!(
        std::mem::offset_of!(NativeAudioStateEntryV1, cached_parameters_entry),
        16
    );
}
const fn many_descriptors() -> [ParameterDescriptor<'static>; 1024] {
    let mut values = [DESCRIPTORS[0]; 1024];
    let mut index = 0;
    while index < 1024 {
        values[index].id = ParameterId(index as u32 * 4096);
        index += 1;
    }
    values
}
static MANY: [ParameterDescriptor<'static>; 1024] = many_descriptors();
#[derive(Default)]
struct Empty;
struct Many {
    values: [f32; 1024],
}
impl Default for Many {
    fn default() -> Self {
        Self { values: [0.; 1024] }
    }
}
macro_rules! bounded {
    ($name:ty,$n:expr,$descriptors:expr,$read:expr,$write:expr) => {
        impl NativeParameterProcessor<$n> for $name {
            const STATE_PRODUCT: [u8; 8] = *b"RVTBOUND";
            const STATE_VERSION: u32 = 1;
            fn parameter_schema() -> ParameterSchema<'static, $n> {
                ParameterSchema::new($descriptors).unwrap()
            }
            fn parameter_snapshot(&self) -> ParameterState<'static, $n> {
                Self::parameter_schema().defaults()
            }
            fn restore_parameter_snapshot(
                &mut self,
                _: &ParameterState<'static, $n>,
            ) -> Result<(), ParameterError> {
                Ok(())
            }
        }
        impl NativeAudioParameterProcessor<$n> for $name {
            fn audio_parameter_snapshot(&self) -> ParameterState<'static, $n> {
                ParameterSchema::new($descriptors)
                    .unwrap()
                    .state(($read)(self))
                    .unwrap()
            }
            fn apply_audio_parameter_snapshot(
                &mut self,
                state: &ParameterState<'static, $n>,
            ) -> Result<(), ParameterError> {
                ($write)(self, state);
                Ok(())
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
bounded!(Empty, 0, &[], |_: &Empty| [], |_: &mut Empty,
                                         _: &ParameterState<
    'static,
    0,
>| {});
bounded!(
    Many,
    1024,
    &MANY,
    |this: &Many| this.values,
    |this: &mut Many, state: &ParameterState<'static, 1024>| this.values = *state.values()
);
#[test]
fn empty_null_prefix_and_full_capacity_complete_snapshots_preserve_boundaries() {
    let empty = NativeAudioState::<Empty, f32, 0>::entry().unwrap();
    let cached = unsafe { &*(empty.cached_parameters_entry)() };
    let cache = (cached.create)();
    let core = NativeProcessor::<Empty, f32>::entry();
    let handle = (core.create)();
    assert_eq!(unsafe { (core.prepare)(handle, &config()) }, 0);
    assert_eq!(unsafe { (empty.apply)(handle, cache, ptr::null(), 0) }, 0);
    assert_eq!(
        unsafe { (empty.snapshot)(handle, cache, ptr::null_mut(), 0) },
        0
    );
    let mut tail = [SENTINEL; 1];
    assert_eq!(
        unsafe { (empty.snapshot)(handle, cache, tail.as_mut_ptr(), 1) },
        0
    );
    assert_eq!(tail, [SENTINEL; 1]);
    unsafe {
        (cached.destroy)(cache);
        (core.destroy)(handle);
    }
    let table = NativeAudioState::<Many, f32, 1024>::entry().unwrap();
    let cached = unsafe { &*(table.cached_parameters_entry)() };
    let cache = (cached.create)();
    let core = NativeProcessor::<Many, f32>::entry();
    let handle = (core.create)();
    assert_eq!(unsafe { (core.prepare)(handle, &config()) }, 0);
    let input: [_; 1024] = std::array::from_fn(|index| NativeParameterValueV1 {
        id: ((1023 - index) * 4096) as u32,
        value: (1023 - index) as f32,
    });
    let mut output = [SENTINEL; 1024];
    let heap = allocation_counter::measure(|| {
        for _ in 0..8 {
            assert_eq!(
                unsafe { (table.apply)(handle, cache, input.as_ptr(), 1024) },
                0
            );
            assert_eq!(
                unsafe { (table.snapshot)(handle, cache, output.as_mut_ptr(), 1024) },
                0
            );
            for (index, field) in output.iter().enumerate() {
                assert_eq!(
                    *field,
                    NativeParameterValueV1 {
                        id: (index * 4096) as u32,
                        value: index as f32
                    }
                );
            }
        }
    });
    assert_eq!(
        (heap.count_total, heap.bytes_total, heap.count_current),
        (0, 0, 0)
    );
    let mut bad = input;
    bad[1023].id = bad[0].id;
    assert_eq!(
        unsafe { (table.apply)(handle, cache, bad.as_ptr(), 1024) },
        1
    );
    assert_eq!(
        unsafe { (table.snapshot)(handle, cache, output.as_mut_ptr(), 1024) },
        0
    );
    assert_eq!(output[0].value, 0.);
    unsafe {
        (cached.destroy)(cache);
        (core.destroy)(handle);
    }
}
