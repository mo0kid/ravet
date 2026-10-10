#![cfg(target_has_atomic = "64")]
#![allow(unsafe_code)]
use ravet_foundation::*;
use ravet_native::*;
use std::{
    cell::Cell,
    ptr,
    sync::{
        Arc, Barrier,
        atomic::{AtomicBool, Ordering},
    },
    thread,
};

thread_local! { static SCHEMA_CALLS: Cell<usize> = const { Cell::new(0) }; }
const fn range() -> ParameterRange {
    match ParameterRange::new(-10000., 10000., ParameterScale::Linear, None) {
        Ok(value) => value,
        Err(_) => panic!("fixture range"),
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
        automatable: true,
    },
    ParameterDescriptor {
        id: ParameterId(300),
        name: "Marker",
        unit: "",
        range: range(),
        default: 0.,
        automatable: true,
    },
];
static REVERSED: [ParameterDescriptor<'static>; 3] =
    [DESCRIPTORS[2], DESCRIPTORS[1], DESCRIPTORS[0]];
fn schema() -> ParameterSchema<'static, 3> {
    ParameterSchema::new(&DESCRIPTORS).unwrap()
}
#[derive(Default)]
struct Fixture {
    values: [f32; 3],
    restores: u32,
    panic_snapshot: bool,
}
impl NativeParameterProcessor<3> for Fixture {
    const STATE_PRODUCT: [u8; 8] = *b"RVTLIVE0";
    const STATE_VERSION: u32 = 1;
    fn parameter_schema() -> ParameterSchema<'static, 3> {
        SCHEMA_CALLS.with(|calls| calls.set(calls.get() + 1));
        schema()
    }
    fn parameter_snapshot(&self) -> ParameterState<'static, 3> {
        schema().state(self.values).unwrap()
    }
    fn decode_parameter_snapshot(
        input: &[u8],
    ) -> Result<ParameterState<'static, 3>, ParameterError> {
        assert!(
            input.first() != Some(&b'!'),
            "synthetic controller decoder panic"
        );
        ParameterState::decode(
            Self::parameter_schema(),
            Self::STATE_PRODUCT,
            Self::STATE_VERSION,
            input,
        )
    }
    fn restore_parameter_snapshot(
        &mut self,
        state: &ParameterState<'static, 3>,
    ) -> Result<(), ParameterError> {
        self.values = *state.values();
        Ok(())
    }
}
impl NativeAudioParameterProcessor<3> for Fixture {
    fn audio_parameter_snapshot(&self) -> ParameterState<'static, 3> {
        assert!(!self.panic_snapshot, "synthetic audio snapshot panic");
        ParameterSchema::new(&REVERSED)
            .unwrap()
            .state([self.values[2], self.values[1], self.values[0]])
            .unwrap()
    }
    fn apply_audio_parameter_snapshot(
        &mut self,
        state: &ParameterState<'static, 3>,
    ) -> Result<(), ParameterError> {
        let first = state.values()[0];
        assert!(first != 9000., "synthetic audio restore panic");
        if first == -9999. {
            return Err(ParameterError::WrongParameter(ParameterId(123)));
        }
        self.values = *state.values();
        self.restores += 1;
        Ok(())
    }
}
macro_rules! processor {
    ($sample:ty) => {
        impl Processor<$sample> for Fixture {
            fn prepare(&mut self, _: ProcessConfig) -> Result<(), PrepareError> {
                self.panic_snapshot = false;
                Ok(())
            }
            fn reset(&mut self) {}
            fn deactivate(&mut self) {}
            fn process(&mut self, block: &mut ProcessBlock<'_, '_, '_, $sample>) -> ProcessStatus {
                for event in block.events {
                    if let EventData::ParameterValue { id, value } = event.data {
                        match id.0 {
                            400 => self.values = [value, -value, 2. * value],
                            401 => self.panic_snapshot = value != 0.,
                            _ => {}
                        }
                    }
                }
                block
                    .audio
                    .output_mut(0)
                    .unwrap()
                    .fill(self.values[0] as $sample);
                block
                    .audio
                    .output_mut(1)
                    .unwrap()
                    .fill(self.values[1] as $sample);
                block
                    .audio
                    .output_mut(2)
                    .unwrap()
                    .fill(self.restores as $sample);
                ProcessStatus::Continue
            }
        }
    };
}
processor!(f32);
processor!(f64);
export_native_state_v1!(ravet_test_native_state_v1, Fixture, Fixture, 3);
fn config() -> NativeProcessorConfigV1 {
    let mut buses = [NativeBusLayoutV1::default(); 9];
    buses[0].outputs = 3;
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
fn bytes(value: f32) -> [u8; 40] {
    let mut bytes = [0; 40];
    schema()
        .state([value, -value, value * 2.])
        .unwrap()
        .encode(*b"RVTLIVE0", 1, &mut bytes)
        .unwrap();
    bytes
}
fn publish(
    table: &NativeStateEntryV1,
    publication: *const std::ffi::c_void,
    value: f32,
) -> NativeStateRevisionV1 {
    let input = bytes(value);
    let mut revision = NativeStateRevisionV1::default();
    assert_eq!(
        unsafe { (table.publish)(publication, input.as_ptr(), 40, &mut revision) },
        0
    );
    revision
}
const SENTINEL: NativeStateRevisionV1 = NativeStateRevisionV1 {
    instance: 123,
    generation: 999,
};
macro_rules! cycle {
    ($name:ident,$sample:ty,$bits:expr) => {
        #[test]
        fn $name() {
            let table = unsafe { &*ravet_test_native_state_v1($bits) };
            let parameters = unsafe { &*(table.parameters_entry)() };
            let base = unsafe { &*(parameters.processor_entry)() };
            let handle = (base.create)();
            let publication = unsafe { (table.create)(ptr::null(), 0) };
            assert!(!handle.is_null() && !publication.is_null());
            let mut applied = NativeStateApplyV1::default();
            let mut revision = SENTINEL;
            assert_eq!(
                unsafe { (table.apply)(handle, publication, &mut applied) },
                1
            );
            assert_eq!(unsafe { (base.prepare)(handle, &config()) }, 0);
            assert_eq!(
                unsafe { (table.reflect)(handle, publication, &mut revision) },
                4
            );
            assert_eq!(revision, SENTINEL);
            let mut samples = [[91. as $sample; 10]; 3];
            let outputs = samples.each_mut().map(|p| p.as_mut_ptr());
            let bus = NativeProcessorBusV1 {
                input_channels: 0,
                output_channels: 3,
                inputs: ptr::null(),
                outputs: outputs.as_ptr().cast(),
            };
            let mut result = NativeProcessorResultV1 {
                status: 99,
                event_count: 99,
            };
            let mut saved = [0xff; 41];
            let mut values = [NativeParameterValueV1 { id: 17, value: 91. }; 4];
            let calls = SCHEMA_CALLS.with(Cell::get);
            let heap = allocation_counter::measure(|| {
                for cycle in 0..1000 {
                    let expected = publish(table, publication, (cycle % 100) as f32);
                    assert_eq!(
                        unsafe { (table.apply)(handle, publication, &mut applied) },
                        0
                    );
                    assert_eq!(applied.revision, expected);
                    assert_eq!(applied.changed, 1);
                    assert_eq!(applied.reserved, 0);
                    assert_eq!(
                        unsafe { (table.apply)(handle, publication, &mut applied) },
                        0
                    );
                    assert_eq!(applied.changed, 0);
                    let event = NativeEventV1 {
                        kind: 1,
                        parameter_id: 400,
                        value: f64::from((cycle % 100) as f32 + 1.),
                        ..NativeEventV1::default()
                    };
                    let before_audio = SCHEMA_CALLS.with(Cell::get);
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
                    assert_eq!(samples[0][0], ((cycle % 100) as f32 + 1.) as $sample);
                    assert_eq!(samples[1][0], -samples[0][0]);
                    assert_eq!(samples[2][0], (cycle + 1) as $sample);
                    assert_eq!(samples[0][8..], [91. as $sample; 2]);
                    assert_eq!(
                        unsafe { (table.reflect)(handle, publication, &mut revision) },
                        0
                    );
                    assert_eq!(revision.generation, expected.generation + 2);
                    assert_eq!(
                        unsafe { (table.apply)(handle, publication, &mut applied) },
                        0
                    );
                    assert_eq!(applied.changed, 0);
                    assert_eq!(
                        SCHEMA_CALLS.with(Cell::get),
                        before_audio,
                        "audio path must use cached metadata"
                    );
                    assert_eq!(
                        unsafe {
                            (table.snapshot)(publication, values.as_mut_ptr(), 4, &mut revision)
                        },
                        0
                    );
                    assert_eq!(values[0].id, u32::MAX);
                    assert_eq!(values[1].value, -values[0].value);
                    assert_eq!(values[2].value, values[0].value * 2.);
                    assert_eq!(values[3], NativeParameterValueV1 { id: 17, value: 91. });
                    assert_eq!(
                        unsafe {
                            (table.save_state)(publication, saved.as_mut_ptr(), 41, &mut revision)
                        },
                        0
                    );
                    assert_eq!(saved[..40], bytes((cycle % 100) as f32 + 1.));
                    assert_eq!(saved[40], 0xff);
                }
            });
            assert_eq!((heap.count_total, heap.bytes_total), (0, 0));
            assert_eq!(
                SCHEMA_CALLS.with(Cell::get),
                calls + 1000,
                "only control decoder invokes schema provider"
            );
            let old = applied.revision;
            let newer = publish(table, publication, 201.);
            revision = SENTINEL;
            assert_eq!(
                unsafe { (table.reflect)(handle, publication, &mut revision) },
                4
            );
            assert_eq!(revision, SENTINEL);
            assert!(newer.generation > old.generation);
            assert_eq!(
                unsafe { (table.apply)(handle, publication, &mut applied) },
                0
            );
            assert_eq!(applied.revision, newer);
            assert_eq!(
                unsafe { (table.reflect)(handle, publication, &mut revision) },
                0
            );
            assert_eq!(
                revision, newer,
                "unchanged reflection must not create another generation"
            );
            assert_eq!(unsafe { (base.reset)(handle) }, 0);
            assert_eq!(
                unsafe { (table.apply)(handle, publication, &mut applied) },
                0
            );
            assert_eq!(applied.changed, 0);
            assert_eq!(unsafe { (base.deactivate)(handle) }, 0);
            assert_eq!(
                unsafe { (table.apply)(handle, publication, &mut applied) },
                1
            );
            assert_eq!(unsafe { (base.prepare)(handle, &config()) }, 0);
            assert_eq!(
                unsafe { (table.apply)(handle, publication, &mut applied) },
                0
            );
            assert_eq!(applied.changed, 1);
            let other = unsafe { (table.create)(ptr::null(), 0) };
            assert!(!other.is_null());
            revision = SENTINEL;
            assert_eq!(unsafe { (table.reflect)(handle, other, &mut revision) }, 4);
            assert_eq!(revision, SENTINEL);
            assert_eq!(unsafe { (table.apply)(handle, other, &mut applied) }, 0);
            assert_ne!(applied.revision.instance, newer.instance);
            unsafe {
                (table.destroy)(other);
                (table.destroy)(publication);
                (base.destroy)(handle);
            }
        }
    };
}
cycle!(
    f32_apply_reflect_coalescing_cached_schema_heap_free,
    f32,
    32
);
cycle!(
    f64_apply_reflect_coalescing_cached_schema_heap_free,
    f64,
    64
);

#[test]
fn complete_validation_capacity_layout_and_failed_outputs_are_atomic() {
    assert!(ravet_test_native_state_v1(16).is_null());
    assert_eq!(size_of::<NativeStateRevisionV1>(), 16);
    assert_eq!(size_of::<NativeStateApplyV1>(), 24);
    assert_eq!(
        size_of::<NativeStateEntryV1>(),
        16 + 8 * size_of::<extern "C" fn()>()
    );
    assert_eq!(
        std::mem::offset_of!(NativeStateEntryV1, parameters_entry),
        16
    );
    let table = NativeState::<Fixture, f32, 3>::entry().unwrap();
    let input = bytes(12.);
    let publication = unsafe { (table.create)(input.as_ptr(), 40) };
    assert!(!publication.is_null());
    assert!(unsafe { (table.create)(ptr::null(), 40) }.is_null());
    assert!(unsafe { (table.create)(input.as_ptr(), 39) }.is_null());
    let mut revision = SENTINEL;
    let mut output = [NativeParameterValueV1 { id: 99, value: 91. }; 4];
    let mut saved = [0xff; 41];
    assert_eq!(
        unsafe { (table.snapshot)(publication, output.as_mut_ptr(), 2, &mut revision) },
        1
    );
    assert_eq!(output, [NativeParameterValueV1 { id: 99, value: 91. }; 4]);
    assert_eq!(revision, SENTINEL);
    assert_eq!(
        unsafe { (table.save_state)(publication, saved.as_mut_ptr(), 39, &mut revision) },
        1
    );
    assert_eq!(saved, [0xff; 41]);
    assert_eq!(revision, SENTINEL);
    let mut bad_product = input;
    bad_product[0] ^= 1;
    let mut bad_version = input;
    bad_version[8..12].copy_from_slice(&2_u32.to_le_bytes());
    let mut duplicate = input;
    duplicate[24..28].copy_from_slice(&input[16..20]);
    let mut nan = input;
    nan[20..24].copy_from_slice(&f32::NAN.to_le_bytes());
    let mut out_of_range = input;
    out_of_range[20..24].copy_from_slice(&10001_f32.to_le_bytes());
    for invalid in [bad_product, bad_version, duplicate, nan, out_of_range] {
        assert_eq!(
            unsafe { (table.publish)(publication, invalid.as_ptr(), 40, &mut revision) },
            1
        );
        assert_eq!(revision, SENTINEL);
    }
    assert_eq!(
        unsafe { (table.publish)(publication, input.as_ptr(), 39, &mut revision) },
        1
    );
    assert_eq!(revision, SENTINEL);
    assert_eq!(
        unsafe { (table.publish)(publication, ptr::null(), 40, &mut revision) },
        1
    );
    assert_eq!(revision, SENTINEL);
    assert_eq!(
        unsafe { (table.save_state)(publication, saved.as_mut_ptr(), 41, &mut revision) },
        0
    );
    assert_eq!(saved[..40], input);
    assert_eq!(revision.generation, 0);
    assert_eq!(saved[40], 0xff);
    let mut reordered = input;
    reordered[16..24].copy_from_slice(&input[32..40]);
    reordered[32..40].copy_from_slice(&input[16..24]);
    assert_eq!(
        unsafe { (table.publish)(publication, reordered.as_ptr(), 40, &mut revision) },
        0
    );
    assert_eq!(revision.generation, 2);
    unsafe {
        (table.destroy)(publication);
    }
}

#[test]
fn rejected_audio_restore_and_panics_preserve_results_with_shared_owner_recovery() {
    let table = NativeState::<Fixture, f32, 3>::entry().unwrap();
    let parameters = unsafe { &*(table.parameters_entry)() };
    let base = unsafe { &*(parameters.processor_entry)() };
    let handle = (base.create)();
    let publication = unsafe { (table.create)(ptr::null(), 0) };
    assert_eq!(unsafe { (base.prepare)(handle, &config()) }, 0);
    let mut applied = NativeStateApplyV1 {
        revision: SENTINEL,
        changed: 99,
        reserved: 99,
    };
    let sentinel = applied;
    // Rejection and panic values are structurally valid complete snapshots.
    for (value, expected) in [(-9999., 1), (9000., 2)] {
        let mut input = [0; 40];
        schema()
            .state([value, 0., 0.])
            .unwrap()
            .encode(*b"RVTLIVE0", 1, &mut input)
            .unwrap();
        let mut revision = SENTINEL;
        assert_eq!(
            unsafe { (table.publish)(publication, input.as_ptr(), 40, &mut revision) },
            0
        );
        assert_eq!(
            unsafe { (table.apply)(handle, publication, &mut applied) },
            expected
        );
        assert_eq!(applied, sentinel);
    }
    assert_eq!(unsafe { (base.reset)(handle) }, 2);
    let mut revision = SENTINEL;
    assert_eq!(
        unsafe { (table.reflect)(handle, publication, &mut revision) },
        2
    );
    assert_eq!(revision, SENTINEL);
    publish(table, publication, 7.);
    assert_eq!(unsafe { (base.prepare)(handle, &config()) }, 0);
    assert_eq!(
        unsafe { (table.apply)(handle, publication, &mut applied) },
        0
    );
    let mut samples = [[0_f32; 8]; 3];
    let outputs = samples.each_mut().map(|p| p.as_mut_ptr());
    let bus = NativeProcessorBusV1 {
        input_channels: 0,
        output_channels: 3,
        inputs: ptr::null(),
        outputs: outputs.as_ptr().cast(),
    };
    let event = NativeEventV1 {
        kind: 1,
        parameter_id: 401,
        value: 1.,
        ..NativeEventV1::default()
    };
    let mut result = NativeProcessorResultV1 {
        status: 99,
        event_count: 99,
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
    assert_eq!(
        unsafe { (table.reflect)(handle, publication, &mut revision) },
        2
    );
    assert_eq!(revision, SENTINEL);
    assert_eq!(unsafe { (base.reset)(handle) }, 2);
    assert_eq!(unsafe { (base.prepare)(handle, &config()) }, 0);
    assert_eq!(
        unsafe { (table.apply)(handle, publication, &mut applied) },
        0
    );
    assert_eq!(applied.changed, 1);
    let mut input = bytes(3.);
    input[0] = b'!';
    revision = SENTINEL;
    assert!(unsafe { (table.create)(input.as_ptr(), 40) }.is_null());
    assert_eq!(
        unsafe { (table.publish)(publication, input.as_ptr(), 40, &mut revision) },
        2
    );
    assert_eq!(revision, SENTINEL);
    assert_eq!(
        unsafe { (base.reset)(handle) },
        0,
        "controller decoder has no DSP owner to poison"
    );
    assert_eq!(
        unsafe { (table.apply)(handle, publication, &mut applied) },
        0
    );
    assert_eq!(applied.changed, 0);
    unsafe {
        (table.destroy)(publication);
        (base.destroy)(handle);
    }
}

#[test]
fn concurrent_control_restore_audio_apply_and_observer_never_tear_complete_snapshots() {
    let table = NativeState::<Fixture, f32, 3>::entry().unwrap();
    let parameters = unsafe { &*(table.parameters_entry)() };
    let base = unsafe { &*(parameters.processor_entry)() };
    let handle = (base.create)();
    let publication = unsafe { (table.create)(ptr::null(), 0) };
    assert_eq!(unsafe { (base.prepare)(handle, &config()) }, 0);
    let address = publication as usize;
    let barrier = Arc::new(Barrier::new(3));
    let stop = Arc::new(AtomicBool::new(false));
    let writer_barrier = barrier.clone();
    let writer_stop = stop.clone();
    let writer = thread::spawn(move || {
        writer_barrier.wait();
        let mut successes = 0;
        for cycle in 0..30000 {
            let input = bytes((cycle % 1000) as f32);
            let mut revision = SENTINEL;
            let code =
                unsafe { (table.publish)(address as *const _, input.as_ptr(), 40, &mut revision) };
            assert!(matches!(code, 0 | 3 | 4));
            if code == 0 {
                successes += 1;
            } else {
                assert_eq!(revision, SENTINEL);
            }
        }
        writer_stop.store(true, Ordering::Release);
        successes
    });
    let reader_barrier = barrier.clone();
    let reader_stop = stop.clone();
    let observer = thread::spawn(move || {
        reader_barrier.wait();
        let mut successes = 0;
        for _ in 0..50000 {
            let mut values = [NativeParameterValueV1 { id: 77, value: 91. }; 4];
            let mut revision = SENTINEL;
            let code = unsafe {
                (table.snapshot)(address as *const _, values.as_mut_ptr(), 4, &mut revision)
            };
            assert!(matches!(code, 0 | 3));
            if code == 0 {
                assert_eq!(values[1].value, -values[0].value);
                assert_eq!(values[2].value, 2. * values[0].value);
                successes += 1;
            } else {
                assert_eq!(values, [NativeParameterValueV1 { id: 77, value: 91. }; 4]);
                assert_eq!(revision, SENTINEL);
            }
            if reader_stop.load(Ordering::Acquire) && successes > 100 {
                break;
            }
        }
        successes
    });
    barrier.wait();
    let mut samples = [[0_f32; 8]; 3];
    let outputs = samples.each_mut().map(|p| p.as_mut_ptr());
    let bus = NativeProcessorBusV1 {
        input_channels: 0,
        output_channels: 3,
        inputs: ptr::null(),
        outputs: outputs.as_ptr().cast(),
    };
    let mut applied = NativeStateApplyV1::default();
    let mut revision = SENTINEL;
    let mut result = NativeProcessorResultV1 {
        status: 99,
        event_count: 99,
    };
    let mut accepted = 0;
    for cycle in 0..50000 {
        let before = applied;
        let code = unsafe { (table.apply)(handle, publication, &mut applied) };
        assert!(matches!(code, 0 | 3));
        if code == 0 {
            accepted += 1;
        } else {
            assert_eq!(applied, before);
        }
        let event = NativeEventV1 {
            kind: 1,
            parameter_id: 400,
            value: -f64::from(cycle % 1000),
            ..NativeEventV1::default()
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
        assert_eq!(samples[1][0], -samples[0][0]);
        let before = revision;
        let code = unsafe { (table.reflect)(handle, publication, &mut revision) };
        assert!(matches!(code, 0 | 3 | 4));
        if code != 0 {
            assert_eq!(revision, before);
        }
        if stop.load(Ordering::Acquire) && accepted > 100 {
            break;
        }
    }
    assert!(writer.join().unwrap() > 0);
    assert!(observer.join().unwrap() > 0);
    assert!(accepted > 0);
    let expected = publish(table, publication, 1234.);
    assert_eq!(
        unsafe { (table.apply)(handle, publication, &mut applied) },
        0
    );
    assert_eq!(applied.revision, expected);
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
    assert_eq!(samples[0][0], 1234.);
    assert_eq!(samples[1][0], -1234.);
    unsafe {
        (table.destroy)(publication);
        (base.destroy)(handle);
    }
}
