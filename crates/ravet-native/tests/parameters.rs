#![allow(unsafe_code)]
use ravet_foundation::*;
use ravet_native::*;
use std::ptr;
const fn range(min: f32, max: f32, scale: ParameterScale, steps: Option<u16>) -> ParameterRange {
    match ParameterRange::new(min, max, scale, steps) {
        Ok(value) => value,
        Err(_) => panic!("invalid fixture range"),
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
        name: "Bypass",
        unit: "",
        range: range(0., 1., ParameterScale::Linear, Some(1)),
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
static REVERSED: [ParameterDescriptor<'static>; 3] =
    [DESCRIPTORS[2], DESCRIPTORS[1], DESCRIPTORS[0]];
fn schema() -> ParameterSchema<'static, 3> {
    ParameterSchema::new(&DESCRIPTORS).unwrap()
}
struct Fixture {
    values: [f32; 3],
    panic_snapshot: bool,
    panic_restore: bool,
}
impl Default for Fixture {
    fn default() -> Self {
        Self {
            values: [200., 0., 0.],
            panic_snapshot: false,
            panic_restore: false,
        }
    }
}
impl NativeParameterProcessor<3> for Fixture {
    const STATE_PRODUCT: [u8; 8] = *b"RVTTEST0";
    const STATE_VERSION: u32 = 3;
    fn parameter_schema() -> ParameterSchema<'static, 3> {
        schema()
    }
    fn parameter_roles(id: ParameterId) -> NativeParameterRoles {
        NativeParameterRoles {
            bypass: id.0 == 7,
            read_only: id.0 == 99,
        }
    }
    fn parameter_snapshot(&self) -> ParameterState<'static, 3> {
        assert!(!self.panic_snapshot, "synthetic snapshot panic");
        ParameterSchema::new(&REVERSED)
            .unwrap()
            .state([self.values[2], self.values[1], self.values[0]])
            .unwrap()
    }
    fn restore_parameter_snapshot(
        &mut self,
        state: &ParameterState<'static, 3>,
    ) -> Result<(), ParameterError> {
        assert!(!self.panic_restore, "synthetic restore panic");
        self.values = *state.values();
        Ok(())
    }
}
macro_rules! processor {
    ($sample:ty) => {
        impl Processor<$sample> for Fixture {
            fn prepare(&mut self, _: ProcessConfig) -> Result<(), PrepareError> {
                self.panic_snapshot = false;
                self.panic_restore = false;
                Ok(())
            }
            fn reset(&mut self) {}
            fn deactivate(&mut self) {}
            fn process(&mut self, block: &mut ProcessBlock<'_, '_, '_, $sample>) -> ProcessStatus {
                for e in block.events {
                    if let EventData::ParameterValue { id, value } = e.data {
                        match id.0 {
                            1 => self.panic_snapshot = value != 0.,
                            2 => self.panic_restore = value != 0.,
                            _ => {}
                        }
                    }
                }
                block
                    .audio
                    .output_mut(0)
                    .unwrap()
                    .fill(self.values[0] as $sample);
                ProcessStatus::Continue
            }
        }
    };
}
processor!(f32);
processor!(f64);
export_native_parameters_v1!(ravet_test_native_parameters_v1, Fixture, Fixture, 3);
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
#[test]
fn declared_metadata_utf8_roles_ranges_and_atomic_conversion_match_independent_values() {
    let table = NativeParameters::<Fixture, f32, 3>::entry().unwrap();
    assert_eq!(table.parameter_count, 3);
    assert_eq!(table.state_bytes, 40);
    assert_eq!(table.state_product, *b"RVTTEST0");
    assert_eq!(table.state_version, 3);
    let mut info = NativeParameterInfoV1::default();
    assert_eq!(unsafe { (table.info)(0, &mut info) }, 0);
    assert_eq!(
        (info.id, info.flags, info.scale, info.intervals),
        (u32::MAX, 1, 1, 0)
    );
    assert_eq!(
        (info.minimum, info.maximum, info.default_value),
        (20., 20000., 200.)
    );
    assert_eq!(info.name_length, 11);
    assert_eq!(&info.name[..11], "Cutoff µHz".as_bytes());
    assert!(info.name[11..].iter().all(|b| *b == 0));
    assert_eq!(&info.unit[..2], b"Hz");
    assert_eq!(unsafe { (table.info)(1, &mut info) }, 0);
    assert_eq!((info.flags, info.intervals), (3, 1));
    assert_eq!(unsafe { (table.info)(2, &mut info) }, 0);
    assert_eq!(info.flags, 4);
    let previous = info;
    assert_eq!(unsafe { (table.info)(3, &mut info) }, 1);
    assert_eq!(info, previous);
    let mut value = 91.;
    assert_eq!(unsafe { (table.normalize)(u32::MAX, 200., &mut value) }, 0);
    assert!((value - 1. / 3.).abs() < 1e-6);
    assert_eq!(
        unsafe { (table.denormalize)(u32::MAX, 1. / 3., &mut value) },
        0
    );
    assert!((value - 200.).abs() < 0.001);
    assert_eq!(unsafe { (table.denormalize)(7, 0.5, &mut value) }, 0);
    assert_eq!(value, 1.);
    assert_eq!(unsafe { (table.normalize)(7, -100., &mut value) }, 0);
    assert_eq!(value, 0.);
    assert_eq!(unsafe { (table.denormalize)(99, 0.25, &mut value) }, 0);
    assert_eq!(value, -0.5);
    value = 91.;
    assert_eq!(
        unsafe { (table.normalize)(u32::MAX, f32::NAN, &mut value) },
        1
    );
    assert_eq!(value, 91.);
    assert_eq!(unsafe { (table.denormalize)(0, 0.5, &mut value) }, 1);
    assert_eq!(value, 91.);
    assert!(ravet_test_native_parameters_v1(16).is_null());
}
macro_rules! state_cycle {
    ($name:ident,$sample:ty,$bits:expr) => {
        #[test]
        fn $name() {
            let table = unsafe { &*ravet_test_native_parameters_v1($bits) };
            let base = unsafe { &*(table.processor_entry)() };
            let handle = (base.create)();
            let config = config();
            assert_eq!(unsafe { (base.prepare)(handle, &config) }, 0);
            let state = schema().state([1000., 1., -0.25]).unwrap();
            let mut bytes = [0; 40];
            state.encode(*b"RVTTEST0", 3, &mut bytes).unwrap();
            let mut native = [NativeParameterValueV1 { id: 17, value: 91. }; 4];
            let mut saved = [0xff; 41];
            let mut samples = [91. as $sample; 10];
            let outputs = [samples.as_mut_ptr()];
            let bus = NativeProcessorBusV1 {
                input_channels: 0,
                output_channels: 1,
                inputs: ptr::null(),
                outputs: outputs.as_ptr().cast(),
            };
            let mut result = NativeProcessorResultV1 {
                status: 99,
                event_count: 99,
            };
            let heap = allocation_counter::measure(|| {
                for _ in 0..1000 {
                    assert_eq!(unsafe { (table.load_state)(handle, bytes.as_ptr(), 40) }, 0);
                    assert_eq!(
                        unsafe { (table.snapshot)(handle, native.as_mut_ptr(), 4) },
                        0
                    );
                    assert_eq!(
                        native[..3],
                        [
                            NativeParameterValueV1 {
                                id: u32::MAX,
                                value: 1000.
                            },
                            NativeParameterValueV1 { id: 7, value: 1. },
                            NativeParameterValueV1 {
                                id: 99,
                                value: -0.25
                            }
                        ]
                    );
                    assert_eq!(native[3], NativeParameterValueV1 { id: 17, value: 91. });
                    assert_eq!(
                        unsafe { (table.save_state)(handle, saved.as_mut_ptr(), 41) },
                        0
                    );
                    assert_eq!(saved[..40], bytes);
                    assert_eq!(saved[40], 0xff);
                    assert_eq!(
                        unsafe {
                            (base.process)(
                                handle,
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
                    assert_eq!(samples[..8], [1000. as $sample; 8]);
                    assert_eq!(samples[8..], [91. as $sample; 2]);
                }
            });
            assert_eq!((heap.count_total, heap.bytes_total), (0, 0));
            let previous = native;
            assert_eq!(
                unsafe { (table.snapshot)(handle, native.as_mut_ptr(), 2) },
                1
            );
            assert_eq!(native, previous);
            saved.fill(0xff);
            assert_eq!(
                unsafe { (table.save_state)(handle, saved.as_mut_ptr(), 39) },
                1
            );
            assert_eq!(saved, [0xff; 41]);
            // Field order is not schema identity; reordered complete records load.
            let mut reordered = bytes;
            reordered[16..24].copy_from_slice(&bytes[32..40]);
            reordered[32..40].copy_from_slice(&bytes[16..24]);
            assert_eq!(
                unsafe { (table.load_state)(handle, reordered.as_ptr(), 40) },
                0
            );
            let mut wrong_product = bytes;
            wrong_product[0] ^= 1;
            let mut version = bytes;
            version[8..12].copy_from_slice(&4_u32.to_le_bytes());
            let mut duplicate = bytes;
            duplicate[24..28].copy_from_slice(&bytes[16..20]);
            let mut bad_value = bytes;
            bad_value[20..24].copy_from_slice(&f32::NAN.to_le_bytes());
            for bad in [wrong_product, version, duplicate, bad_value] {
                assert_eq!(unsafe { (table.load_state)(handle, bad.as_ptr(), 40) }, 1);
                assert_eq!(
                    unsafe { (table.snapshot)(handle, native.as_mut_ptr(), 4) },
                    0
                );
                assert_eq!(native, previous);
            }
            assert_eq!(unsafe { (table.load_state)(handle, bytes.as_ptr(), 39) }, 1);
            assert_eq!(unsafe { (table.load_state)(handle, ptr::null(), 40) }, 1);
            // Provider panic poisons the shared base handle; fresh preparation recovers.
            let event = NativeEventV1 {
                kind: 1,
                parameter_id: 1,
                value: 1.,
                ..Default::default()
            };
            assert_eq!(
                unsafe {
                    (base.process)(
                        handle,
                        &bus,
                        1,
                        8,
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
            native.fill(NativeParameterValueV1 { id: 17, value: 91. });
            let sentinel = native;
            assert_eq!(
                unsafe { (table.snapshot)(handle, native.as_mut_ptr(), 4) },
                2
            );
            assert_eq!(native, sentinel);
            assert_eq!(unsafe { (base.reset)(handle) }, 2);
            assert_eq!(unsafe { (base.prepare)(handle, &config) }, 0);
            let event = NativeEventV1 {
                parameter_id: 2,
                ..event
            };
            assert_eq!(
                unsafe {
                    (base.process)(
                        handle,
                        &bus,
                        1,
                        8,
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
            assert_eq!(unsafe { (table.load_state)(handle, bytes.as_ptr(), 40) }, 2);
            assert_eq!(
                unsafe { (table.save_state)(handle, saved.as_mut_ptr(), 41) },
                2
            );
            assert_eq!(unsafe { (base.prepare)(handle, &config) }, 0);
            unsafe { (base.destroy)(handle) };
        }
    };
}
state_cycle!(native_f32_schema_state_processing_roundtrip, f32, 32);
state_cycle!(native_f64_schema_state_processing_roundtrip, f64, 64);
#[test]
fn c_schema_state_record_offsets_and_sizes_are_exact() {
    use std::mem::{offset_of, size_of};
    assert_eq!(size_of::<NativeParameterInfoV1>(), 360);
    assert_eq!(offset_of!(NativeParameterInfoV1, name), 40);
    assert_eq!(offset_of!(NativeParameterInfoV1, unit), 296);
    assert_eq!(size_of::<NativeParameterValueV1>(), 8);
    assert_eq!(offset_of!(NativeParameterEntryV1, processor_entry), 32);
    assert_eq!(
        size_of::<NativeParameterEntryV1>(),
        32 + 7 * size_of::<*const std::ffi::c_void>()
    );
}

const BASIC: ParameterDescriptor<'static> = ParameterDescriptor {
    id: ParameterId(0),
    name: "Value",
    unit: "",
    range: range(0., 4., ParameterScale::Linear, None),
    default: 1.,
    automatable: true,
};
static NAME_BYTES: [u8; 256] = [b'n'; 256];
static UNIT_BYTES: [u8; 64] = [b'u'; 64];
const NAME_TEXT: &str = match std::str::from_utf8(&NAME_BYTES) {
    Ok(v) => v,
    Err(_) => panic!("fixture ASCII"),
};
const UNIT_TEXT: &str = match std::str::from_utf8(&UNIT_BYTES) {
    Ok(v) => v,
    Err(_) => panic!("fixture ASCII"),
};
static BOUND_NAME_BYTES: [u8; 255] = [b'n'; 255];
static BOUND_UNIT_BYTES: [u8; 63] = [b'u'; 63];
const BOUND_NAME: &str = match std::str::from_utf8(&BOUND_NAME_BYTES) {
    Ok(v) => v,
    Err(_) => panic!("fixture ASCII"),
};
const BOUND_UNIT: &str = match std::str::from_utf8(&BOUND_UNIT_BYTES) {
    Ok(v) => v,
    Err(_) => panic!("fixture ASCII"),
};
static BOUND_SCHEMA: [ParameterDescriptor<'static>; 1] = [ParameterDescriptor {
    name: BOUND_NAME,
    unit: BOUND_UNIT,
    ..BASIC
}];
static BASIC_SCHEMA: [ParameterDescriptor<'static>; 1] = [BASIC];
static LARGE_NAME: [ParameterDescriptor<'static>; 1] = [ParameterDescriptor {
    name: NAME_TEXT,
    ..BASIC
}];
static LARGE_UNIT: [ParameterDescriptor<'static>; 1] = [ParameterDescriptor {
    unit: UNIT_TEXT,
    ..BASIC
}];
static NUL_NAME: [ParameterDescriptor<'static>; 1] = [ParameterDescriptor {
    name: "Value\0suffix",
    ..BASIC
}];
static NUL_UNIT: [ParameterDescriptor<'static>; 1] = [ParameterDescriptor {
    unit: "x\0y",
    ..BASIC
}];
static OTHER_ID: [ParameterDescriptor<'static>; 1] = [ParameterDescriptor {
    id: ParameterId(17),
    ..BASIC
}];
static OTHER_RANGE: [ParameterDescriptor<'static>; 1] = [ParameterDescriptor {
    range: range(0., 8., ParameterScale::Linear, None),
    ..BASIC
}];
#[derive(Default)]
struct MetadataFixture<const MODE: usize>;
impl<const MODE: usize> Processor<f32> for MetadataFixture<MODE> {
    fn prepare(&mut self, _: ProcessConfig) -> Result<(), PrepareError> {
        Ok(())
    }
    fn reset(&mut self) {}
    fn deactivate(&mut self) {}
    fn process(&mut self, _: &mut ProcessBlock<'_, '_, '_>) -> ProcessStatus {
        ProcessStatus::Continue
    }
}
impl<const MODE: usize> NativeParameterProcessor<1> for MetadataFixture<MODE> {
    const STATE_PRODUCT: [u8; 8] = *b"RVTTEST0";
    const STATE_VERSION: u32 = if MODE == 6 { 0 } else { 1 };
    fn parameter_schema() -> ParameterSchema<'static, 1> {
        assert!(MODE != 7, "synthetic metadata panic");
        let descriptors = match MODE {
            1 => &LARGE_NAME,
            2 => &LARGE_UNIT,
            3 => &NUL_NAME,
            4 => &NUL_UNIT,
            5 => &BOUND_SCHEMA,
            _ => &BASIC_SCHEMA,
        };
        ParameterSchema::new(descriptors).unwrap()
    }
    fn parameter_snapshot(&self) -> ParameterState<'static, 1> {
        match MODE {
            9 => ParameterSchema::new(&OTHER_ID).unwrap().defaults(),
            10 => ParameterSchema::new(&OTHER_RANGE)
                .unwrap()
                .state([5.])
                .unwrap(),
            _ => Self::parameter_schema().defaults(),
        }
    }
    fn restore_parameter_snapshot(
        &mut self,
        _: &ParameterState<'static, 1>,
    ) -> Result<(), ParameterError> {
        Ok(())
    }
}
impl NativeParameterProcessor<1025> for MetadataFixture<8> {
    const STATE_PRODUCT: [u8; 8] = *b"RVTTEST0";
    const STATE_VERSION: u32 = 1;
    fn parameter_schema() -> ParameterSchema<'static, 1025> {
        panic!("capacity preflight must precede provider")
    }
    fn parameter_snapshot(&self) -> ParameterState<'static, 1025> {
        panic!("unavailable entry")
    }
    fn restore_parameter_snapshot(
        &mut self,
        _: &ParameterState<'static, 1025>,
    ) -> Result<(), ParameterError> {
        panic!("unavailable entry")
    }
}
#[test]
fn native_registration_rejects_oversize_nul_version_capacity_and_metadata_panics() {
    assert!(NativeParameters::<MetadataFixture<0>, f32, 1>::entry().is_some());
    let bound = NativeParameters::<MetadataFixture<5>, f32, 1>::entry().unwrap();
    let mut info = NativeParameterInfoV1::default();
    assert_eq!(unsafe { (bound.info)(0, &mut info) }, 0);
    assert_eq!((info.name_length, info.unit_length), (255, 63));
    assert_eq!((info.name[255], info.unit[63]), (0, 0));
    assert!(NativeParameters::<MetadataFixture<1>, f32, 1>::entry().is_none());
    assert!(NativeParameters::<MetadataFixture<2>, f32, 1>::entry().is_none());
    assert!(NativeParameters::<MetadataFixture<3>, f32, 1>::entry().is_none());
    assert!(NativeParameters::<MetadataFixture<4>, f32, 1>::entry().is_none());
    assert!(NativeParameters::<MetadataFixture<6>, f32, 1>::entry().is_none());
    assert!(NativeParameters::<MetadataFixture<7>, f32, 1>::entry().is_none());
    let measured = allocation_counter::measure(|| {
        assert!(NativeParameters::<MetadataFixture<8>, f32, 1025>::entry().is_none());
    });
    assert_eq!((measured.count_total, measured.bytes_total), (0, 0));
}
fn snapshot_rejection<const MODE: usize>() {
    let table = NativeParameters::<MetadataFixture<MODE>, f32, 1>::entry().unwrap();
    let base = unsafe { &*(table.processor_entry)() };
    let handle = (base.create)();
    let sentinel = NativeParameterValueV1 { id: 77, value: 91. };
    let mut values = [sentinel; 2];
    let mut bytes = [0xff; 25];
    assert_eq!(
        unsafe { (table.snapshot)(handle, values.as_mut_ptr(), 2) },
        1
    );
    assert_eq!(values, [sentinel; 2]);
    assert_eq!(
        unsafe { (table.save_state)(handle, bytes.as_mut_ptr(), 25) },
        1
    );
    assert_eq!(bytes, [0xff; 25]);
    unsafe { (base.destroy)(handle) };
}
#[test]
fn mismatched_snapshot_ids_or_ranges_never_write_a_foreign_prefix() {
    snapshot_rejection::<9>();
    snapshot_rejection::<10>();
}
