//! Native ABI for the public gain reference, sharing its portable DSP and codec.
//!
//! Handles are exclusively owned by the serialized processing/lifecycle caller.
//! Prepare/teardown allocate; normal processing/reset/restore do not. Native
//! pointers must obey `include/RavetGain.h`; malformed foreign pointers cannot
//! be made safe by null checks. Unwinding panics are contained, without a
//! real-time guarantee for exceptional handlers. Prepared planar processing uses
//! `ravet-native`, including its shared legacy parameter decoder; these product
//! C symbols/state remain specific to Gain.

#[allow(unsafe_code)]
mod abi {
    use ravet_foundation::{
        AudioLayout, Event, EventData, ParameterId, ProcessConfig, ProcessStatus, Processor,
        Transport,
    };
    use ravet_gain::{GainProcessor, GainProcessor64, parameter_schema, state};
    pub use ravet_native::NativeParameterEvent as NativeEvent;
    use ravet_native::{NativeAudioAdapter, NativePlanarBus, decode_parameter_events};
    use std::{
        panic::{AssertUnwindSafe, catch_unwind},
        ptr, slice,
    };

    /// C-compatible one-channel meter record, with native f64 levels and
    /// explicit diagnostic/count words. Reserved is always zero.
    #[repr(C)]
    #[derive(Clone, Copy, Debug, Default, PartialEq)]
    pub struct NativeMeterReading {
        pub peak: f64,
        pub rms: f64,
        pub samples: u64,
        pub nonfinite: u32,
        pub reserved: u32,
    }
    /// Complete last successful nonempty block's mono/stereo input/output
    /// meters. Unused channels are zero. Serialized DSP ownership applies;
    /// this record is not an audio-to-UI synchronization primitive.
    #[repr(C)]
    #[derive(Clone, Copy, Debug, Default, PartialEq)]
    pub struct NativeMeters {
        pub input: [NativeMeterReading; 2],
        pub output: [NativeMeterReading; 2],
    }
    const MAX_EVENTS: usize = 1024;
    // One ABI implementation keeps validation, copied-input aliasing and fault
    // behavior identical across native sample precisions. Control/state stays f32.
    macro_rules! native_precision {
        ($module:ident, $sample:ty, $processor:ident, $handle:ident,
         $create:ident, $destroy:ident, $prepare:ident, $reset:ident,
         $restore:ident, $process:ident, $meters:ident) => {
            mod $module {
                use super::*;
                pub struct $handle {
                    processor: $processor,
                    adapter: Option<NativeAudioAdapter<$sample>>,
                    events: Vec<Event>,
                }
                /// Copy coherent meter data under the same exclusive serialized
                /// ownership as processing. Does not allocate or reset readings.
                /// # Safety
                /// Handle/output are live aligned distinct storage, exclusively
                /// owned for this call, per include/RavetGainMeters.h. No concurrent
                /// DSP/control reads, teardown or overlapping output/handle.
                #[unsafe(no_mangle)]
                pub unsafe extern "C" fn $meters(
                    handle: *const $handle,
                    output: *mut NativeMeters,
                ) -> i32 {
                    if handle.is_null() || output.is_null() {
                        return 1;
                    }
                    catch_unwind(AssertUnwindSafe(|| {
                        let readings = unsafe { &*handle }.processor.meters();
                        let meters = NativeMeters {
                            input: readings.input.map(|r| NativeMeterReading {
                                peak: f64::from(r.peak),
                                rms: f64::from(r.rms),
                                samples: r.samples,
                                nonfinite: u32::from(r.nonfinite),
                                reserved: 0,
                            }),
                            output: readings.output.map(|r| NativeMeterReading {
                                peak: f64::from(r.peak),
                                rms: f64::from(r.rms),
                                samples: r.samples,
                                nonfinite: u32::from(r.nonfinite),
                                reserved: 0,
                            }),
                        };
                        unsafe {
                            output.write(meters);
                        }
                        0
                    }))
                    .unwrap_or(2)
                }
                impl $handle {
                    fn new() -> Self {
                        Self {
                            processor: $processor::new(5.0).expect("valid smoothing"),
                            adapter: None,
                            events: Vec::new(),
                        }
                    }
                }
                #[unsafe(no_mangle)]
                pub extern "C" fn $create() -> *mut $handle {
                    #[cfg(all(target_os = "macos", feature = "native-editor"))]
                    ravet_webview::link();
                    catch_unwind(|| Box::into_raw(Box::new($handle::new())))
                        .unwrap_or(ptr::null_mut())
                }
                /// # Safety
                /// Native pointers, ranges and exclusive handle ownership must satisfy
                /// the contract in `include/RavetGain.h` for this operation.
                #[unsafe(no_mangle)]
                pub unsafe extern "C" fn $destroy(handle: *mut $handle) {
                    if !handle.is_null() {
                        let _ = catch_unwind(AssertUnwindSafe(|| {
                            let mut handle = unsafe { Box::from_raw(handle) };
                            handle.processor.deactivate();
                        }));
                    }
                }
                /// # Safety
                /// Native pointers, ranges and exclusive handle ownership must satisfy
                /// the contract in `include/RavetGain.h` for this operation.
                #[unsafe(no_mangle)]
                pub unsafe extern "C" fn $prepare(
                    handle: *mut $handle,
                    rate: f64,
                    channels: u32,
                    frames: u32,
                ) -> i32 {
                    let Some(handle) = (unsafe { handle.as_mut() }) else {
                        return 1;
                    };
                    catch_unwind(AssertUnwindSafe(|| {
                        handle.adapter = None;
                        if !(1..=2).contains(&channels) || !(1..=65536).contains(&frames) {
                            handle.processor.deactivate();
                            return 1;
                        }
                        let Ok(layout) = AudioLayout::new(channels as usize, channels as usize)
                        else {
                            handle.processor.deactivate();
                            return 1;
                        };
                        let Ok(config) =
                            ProcessConfig::new(rate, frames as usize, MAX_EVENTS, layout)
                        else {
                            handle.processor.deactivate();
                            return 1;
                        };
                        let Ok(adapter) =
                            NativeAudioAdapter::prepare(&mut handle.processor, config)
                        else {
                            return 1;
                        };
                        handle.adapter = Some(adapter);
                        handle.events.resize(
                            MAX_EVENTS,
                            Event {
                                frame_offset: 0,
                                data: EventData::ParameterValue {
                                    id: ParameterId(0),
                                    value: 0.,
                                },
                            },
                        );

                        0
                    }))
                    .unwrap_or(2)
                }
                /// # Safety
                /// Native pointers, ranges and exclusive handle ownership must satisfy
                /// the contract in `include/RavetGain.h` for this operation.
                #[unsafe(no_mangle)]
                pub unsafe extern "C" fn $reset(handle: *mut $handle) {
                    if let Some(handle) = unsafe { handle.as_mut() } {
                        let _ = catch_unwind(AssertUnwindSafe(|| {
                            if let Some(adapter) = &mut handle.adapter {
                                let _ = adapter.reset(&mut handle.processor);
                            } else {
                                handle.processor.reset();
                            }
                        }));
                    }
                }
                /// # Safety
                /// Native pointers, ranges and exclusive handle ownership must satisfy
                /// the contract in `include/RavetGain.h` for this operation.
                #[unsafe(no_mangle)]
                pub unsafe extern "C" fn $restore(
                    handle: *mut $handle,
                    gain: f32,
                    bypass: u32,
                ) -> i32 {
                    let Some(handle) = (unsafe { handle.as_mut() }) else {
                        return 1;
                    };
                    if bypass > 1 {
                        return 1;
                    }
                    let Ok(state) = parameter_schema().state([gain, bypass as f32]) else {
                        return 1;
                    };
                    catch_unwind(AssertUnwindSafe(|| {
                        handle
                            .processor
                            .restore_parameters(&state)
                            .map_or(1, |()| 0)
                    }))
                    .unwrap_or(2)
                }
                unsafe fn silence(outputs: *const *mut $sample, channels: usize, frames: usize) {
                    if outputs.is_null() || channels > 2 || frames == 0 {
                        return;
                    }
                    for channel in 0..channels {
                        let output = unsafe { *outputs.add(channel) };
                        if !output.is_null() {
                            unsafe { slice::from_raw_parts_mut(output, frames) }.fill(0.);
                        }
                    }
                }
                /// # Safety
                /// Native pointers, ranges and exclusive handle ownership must satisfy
                /// the contract in `include/RavetGain.h` for this operation.
                #[unsafe(no_mangle)]
                pub unsafe extern "C" fn $process(
                    handle: *mut $handle,
                    inputs: *const *const $sample,
                    outputs: *const *mut $sample,
                    channels: u32,
                    frames: u32,
                    events: *const NativeEvent,
                    count: u32,
                    gain: *mut f32,
                    bypass: *mut u32,
                ) -> i32 {
                    let channels = channels as usize;
                    let frames = frames as usize;
                    let count = count as usize;
                    let result = catch_unwind(AssertUnwindSafe(|| {
                        let Some(handle) = (unsafe { handle.as_mut() }) else {
                            return 1;
                        };
                        let Some(configuration) = handle
                            .adapter
                            .as_ref()
                            .map(|adapter| adapter.configuration())
                        else {
                            return 1;
                        };
                        if channels != configuration.layout().input_channels()
                            || channels == 0
                            || frames > configuration.maximum_frames()
                            || count > MAX_EVENTS
                            || gain.is_null()
                            || bypass.is_null()
                        {
                            return 1;
                        }
                        if frames > 0 && (inputs.is_null() || outputs.is_null()) {
                            return 1;
                        }
                        for channel in 0..channels {
                            if frames == 0 {
                                continue;
                            }
                            let input = unsafe { *inputs.add(channel) };
                            let output = unsafe { *outputs.add(channel) };
                            if input.is_null() || output.is_null() {
                                return 1;
                            }
                        }
                        if count > 0 && events.is_null() {
                            return 1;
                        }
                        let native_events = if count == 0 {
                            &[]
                        } else {
                            unsafe { slice::from_raw_parts(events, count) }
                        };
                        if decode_parameter_events(native_events, frames, &mut handle.events)
                            .is_err()
                        {
                            return 1;
                        }
                        let bus = NativePlanarBus {
                            input_channels: channels as u32,
                            output_channels: channels as u32,
                            inputs,
                            outputs,
                        };
                        let adapter = handle.adapter.as_mut().expect("validated preparation");
                        let report = unsafe {
                            adapter.process_native(
                                &mut handle.processor,
                                &[bus],
                                frames,
                                &handle.events[..count],
                                Transport::default(),
                            )
                        };
                        if matches!(report.status, ProcessStatus::Fault(_)) {
                            return if adapter.is_poisoned() { 2 } else { 1 };
                        }
                        unsafe {
                            *gain = handle.processor.requested_gain();
                            *bypass = u32::from(handle.processor.bypassed());
                        }
                        0
                    }))
                    .unwrap_or(2);
                    if result != 0 {
                        unsafe { silence(outputs, channels, frames) };
                    }
                    result
                }
            }
            pub use $module::*;
        };
    }
    native_precision!(
        precision32,
        f32,
        GainProcessor,
        GainHandle,
        ravet_gain_create,
        ravet_gain_destroy,
        ravet_gain_prepare,
        ravet_gain_reset,
        ravet_gain_restore,
        ravet_gain_process,
        ravet_gain_meters
    );
    native_precision!(
        precision64,
        f64,
        GainProcessor64,
        GainHandle64,
        ravet_gain64_create,
        ravet_gain64_destroy,
        ravet_gain64_prepare,
        ravet_gain64_reset,
        ravet_gain64_restore,
        ravet_gain64_process,
        ravet_gain64_meters
    );
    /// # Safety
    /// Native pointers, ranges and exclusive handle ownership must satisfy
    /// the contract in `include/RavetGain.h` for this operation.
    #[unsafe(no_mangle)]
    pub unsafe extern "C" fn ravet_gain_encode(
        gain: f32,
        bypass: u32,
        bytes: *mut u8,
        length: usize,
    ) -> i32 {
        if bytes.is_null() || length != state::PARAMETERS_STATE_BYTES || bypass > 1 {
            return 1;
        }
        let Ok(values) = parameter_schema().state([gain, bypass as f32]) else {
            return 1;
        };
        let Ok(record) = state::encode_parameters(&values) else {
            return 1;
        };
        unsafe { ptr::copy_nonoverlapping(record.as_ptr(), bytes, record.len()) };
        0
    }
    /// # Safety
    /// Native pointers, ranges and exclusive handle ownership must satisfy
    /// the contract in `include/RavetGain.h` for this operation.
    #[unsafe(no_mangle)]
    pub unsafe extern "C" fn ravet_gain_decode(
        bytes: *const u8,
        length: usize,
        gain: *mut f32,
        bypass: *mut u32,
    ) -> i32 {
        if bytes.is_null()
            || gain.is_null()
            || bypass.is_null()
            || ![state::STATE_BYTES, state::PARAMETERS_STATE_BYTES].contains(&length)
        {
            return 1;
        }
        let Ok(values) = state::decode_parameters(unsafe { slice::from_raw_parts(bytes, length) })
        else {
            return 1;
        };
        unsafe {
            *gain = values.values()[0];
            *bypass = u32::from(values.values()[1] == 1.);
        }
        0
    }
    #[cfg(test)]
    mod tests {
        use super::*;
        #[test]
        fn aliasing_zero_frames_state_and_fault_rejection_follow_portable_contract() {
            unsafe {
                let handle = ravet_gain_create();
                assert!(!handle.is_null());
                assert_eq!(ravet_gain_prepare(handle, 48000., 2, 8), 0);
                assert_eq!(ravet_gain_restore(handle, 2., 0), 0);
                let mut left = [0.25; 9];
                let mut right = [-0.5; 9];
                let inputs = [left.as_ptr(), right.as_ptr()];
                let outputs = [left.as_mut_ptr(), right.as_mut_ptr()];
                let mut gain = 0.;
                let mut bypass = 0;
                assert_eq!(
                    ravet_gain_process(
                        handle,
                        inputs.as_ptr(),
                        outputs.as_ptr(),
                        2,
                        8,
                        ptr::null(),
                        0,
                        &mut gain,
                        &mut bypass
                    ),
                    0
                );
                assert_eq!(&left[..8], &[0.5; 8]);
                assert_eq!(&right[..8], &[-1.; 8]);
                assert_eq!(left[8], 0.25);
                assert_eq!(right[8], -0.5);
                assert_eq!(gain, 2.);
                let event = NativeEvent {
                    offset: 0,
                    id: 2,
                    value: 1.,
                };
                assert_eq!(
                    ravet_gain_process(
                        handle,
                        ptr::null(),
                        ptr::null(),
                        2,
                        0,
                        &event,
                        1,
                        &mut gain,
                        &mut bypass
                    ),
                    0
                );
                assert_eq!(bypass, 1);
                let invalid = NativeEvent {
                    offset: 8,
                    id: 1,
                    value: 3.,
                };
                assert_eq!(
                    ravet_gain_process(
                        handle,
                        inputs.as_ptr(),
                        outputs.as_ptr(),
                        2,
                        8,
                        &invalid,
                        1,
                        &mut gain,
                        &mut bypass
                    ),
                    1
                );
                assert_eq!(&left[..8], &[0.; 8]);
                assert_eq!(left[8], 0.25);
                let mut record = [0; 32];
                assert_eq!(ravet_gain_encode(3., 1, record.as_mut_ptr(), 32), 0);
                assert_eq!(
                    ravet_gain_decode(record.as_ptr(), 32, &mut gain, &mut bypass),
                    0
                );
                assert_eq!((gain, bypass), (3., 1));
                record[0] ^= 1;
                assert_eq!(
                    ravet_gain_decode(record.as_ptr(), 32, &mut gain, &mut bypass),
                    1
                );
                assert_eq!((gain, bypass), (3., 1));
                assert_eq!(ravet_gain_prepare(handle, 48000., 3, 8), 1);
                left.fill(4.);
                right.fill(4.);
                assert_eq!(
                    ravet_gain_process(
                        handle,
                        inputs.as_ptr(),
                        outputs.as_ptr(),
                        2,
                        8,
                        ptr::null(),
                        0,
                        &mut gain,
                        &mut bypass
                    ),
                    1
                );
                assert_eq!(&left[..8], &[0.; 8]);
                assert_eq!(left[8], 4.);
                ravet_gain_destroy(handle);
            }
        }
        #[test]
        fn native_normal_zero_frame_and_rejected_callbacks_allocate_nothing() {
            unsafe {
                let handle = ravet_gain_create();
                assert_eq!(ravet_gain_prepare(handle, 48000., 2, 64), 0);
                let mut left = [0.; 64];
                let mut right = [0.; 64];
                let inputs = [left.as_ptr(), right.as_ptr()];
                let outputs = [left.as_mut_ptr(), right.as_mut_ptr()];
                let mut gain = 1.;
                let mut bypass = 0;
                let counts = allocation_counter::measure(|| {
                    for _ in 0..10000 {
                        assert_eq!(ravet_gain_restore(handle, 1., 0), 0);
                        assert_eq!(
                            ravet_gain_process(
                                handle,
                                inputs.as_ptr(),
                                outputs.as_ptr(),
                                2,
                                64,
                                ptr::null(),
                                0,
                                &mut gain,
                                &mut bypass
                            ),
                            0
                        );
                        assert_eq!(
                            ravet_gain_process(
                                handle,
                                ptr::null(),
                                ptr::null(),
                                2,
                                0,
                                ptr::null(),
                                0,
                                &mut gain,
                                &mut bypass
                            ),
                            0
                        );
                        assert_eq!(
                            ravet_gain_process(
                                handle,
                                inputs.as_ptr(),
                                outputs.as_ptr(),
                                2,
                                64,
                                ptr::null(),
                                1025,
                                &mut gain,
                                &mut bypass
                            ),
                            1
                        );
                        ravet_gain_reset(handle);
                    }
                });
                assert_eq!(counts.count_total, 0);
                assert_eq!(counts.count_current, 0);
                assert_eq!(counts.bytes_current, 0);
                ravet_gain_destroy(handle);
            }
        }
    }
}
pub use abi::{GainHandle, GainHandle64, NativeEvent, NativeMeterReading, NativeMeters};
#[allow(unsafe_code)]
mod programs;
pub use programs::NativeProgramInfo;

#[cfg(test)]
#[allow(unsafe_code)]
mod precision64_tests;

#[cfg(all(target_os = "macos", feature = "native-editor"))]
#[allow(unsafe_code)]
mod editor;

// Thin native facades keep the portable gain processor independent of backends.
#[allow(unsafe_code)]
mod native_entry {
    use ravet_foundation::{
        ParameterError, ParameterId, ParameterSchema, ParameterState, PrepareError, ProcessBlock,
        ProcessConfig, ProcessStatus, Processor, Tail,
    };
    use ravet_native::{NativeParameterProcessor, NativeParameterRoles};
    static TELEMETRY: [ravet_native::NativeTelemetryDescriptor; 12] = {
        let names = [
            "Input L peak",
            "Input L RMS",
            "Input L invalid",
            "Input R peak",
            "Input R RMS",
            "Input R invalid",
            "Output L peak",
            "Output L RMS",
            "Output L invalid",
            "Output R peak",
            "Output R RMS",
            "Output R invalid",
        ];
        let mut fields = [ravet_native::NativeTelemetryDescriptor {
            id: 0,
            name: "",
            unit: "",
            display_minimum: 0.,
            display_maximum: 1.,
        }; 12];
        let mut index = 0;
        while index < 12 {
            fields[index].id = 10 + index as u32;
            fields[index].name = names[index];
            index += 1;
        }
        fields
    };
    macro_rules! facade {
        ($name:ident, $processor:ty, $sample:ty) => {
            #[derive(Default)]
            pub struct $name($processor);
            impl Processor<$sample> for $name {
                fn prepare(&mut self, config: ProcessConfig) -> Result<(), PrepareError> {
                    self.0.prepare(config)
                }
                fn process(
                    &mut self,
                    block: &mut ProcessBlock<'_, '_, '_, $sample>,
                ) -> ProcessStatus {
                    self.0.process(block)
                }
                fn reset(&mut self) {
                    self.0.reset();
                }
                fn deactivate(&mut self) {
                    self.0.deactivate();
                }
                fn latency_samples(&self) -> u32 {
                    self.0.latency_samples()
                }
                fn tail(&self) -> Tail {
                    self.0.tail()
                }
            }
            impl ravet_native::NativeAudioParameterProcessor<2> for $name {
                fn audio_parameter_snapshot(&self) -> ParameterState<'static, 2> {
                    self.0.parameter_state()
                }
                fn apply_audio_parameter_snapshot(
                    &mut self,
                    state: &ParameterState<'static, 2>,
                ) -> Result<(), ParameterError> {
                    self.0.restore_parameters(state)
                }
            }
            impl ravet_native::NativeTelemetryProcessor<12> for $name {
                fn telemetry_descriptors() -> &'static [ravet_native::NativeTelemetryDescriptor; 12]
                {
                    &TELEMETRY
                }
                fn audio_telemetry_snapshot(&self) -> [ravet_native::NativeTelemetryValueV1; 12] {
                    let meters = self.0.meters();
                    let readings = [
                        meters.input[0],
                        meters.input[1],
                        meters.output[0],
                        meters.output[1],
                    ];
                    std::array::from_fn(|index| {
                        let reading = readings[index / 3];
                        ravet_native::NativeTelemetryValueV1 {
                            id: 10 + index as u32,
                            flags: u32::from(reading.nonfinite),
                            value: match index % 3 {
                                0 => f64::from(reading.peak),
                                1 => f64::from(reading.rms),
                                _ => f64::from(u32::from(reading.nonfinite)),
                            },
                            samples: reading.samples,
                        }
                    })
                }
            }
            impl NativeParameterProcessor<2> for $name {
                const STATE_PRODUCT: [u8; 8] = *b"RAVETGN\0";
                const STATE_VERSION: u32 = 2;
                fn parameter_schema() -> ParameterSchema<'static, 2> {
                    ravet_gain::parameter_schema()
                }
                fn parameter_roles(id: ParameterId) -> NativeParameterRoles {
                    NativeParameterRoles {
                        bypass: id == ravet_gain::BYPASS_ID,
                        read_only: false,
                    }
                }
                fn parameter_snapshot(&self) -> ParameterState<'static, 2> {
                    self.0.parameter_state()
                }
                fn restore_parameter_snapshot(
                    &mut self,
                    state: &ParameterState<'static, 2>,
                ) -> Result<(), ParameterError> {
                    self.0.restore_parameters(state)
                }
                fn decode_parameter_snapshot(
                    bytes: &[u8],
                ) -> Result<ParameterState<'static, 2>, ParameterError> {
                    ravet_gain::state::decode_parameters(bytes)
                }
            }
        };
    }
    facade!(GainNative32, ravet_gain::GainProcessor, f32);
    facade!(GainNative64, ravet_gain::GainProcessor64, f64);
    ravet_native::export_native_processor_v1!(
        ravet_native_processor_v1,
        GainNative32,
        GainNative64
    );
    ravet_native::export_native_parameters_v1!(
        ravet_native_parameters_v1,
        GainNative32,
        GainNative64,
        2
    );
    #[cfg(target_has_atomic = "64")]
    ravet_native::export_native_state_v1!(ravet_native_state_v1, GainNative32, GainNative64, 2);
    ravet_native::export_native_telemetry_v1!(
        ravet_native_telemetry_v1,
        GainNative32,
        GainNative64,
        12
    );
    ravet_native::export_native_audio_state_v1!(
        ravet_native_audio_state_v1,
        GainNative32,
        GainNative64,
        2
    );
    ravet_native::export_native_cached_parameters_v1!(
        ravet_native_cached_parameters_v1,
        GainNative32,
        GainNative64,
        2
    );
}
pub use native_entry::{
    ravet_native_audio_state_v1, ravet_native_cached_parameters_v1, ravet_native_parameters_v1,
    ravet_native_processor_v1, ravet_native_telemetry_v1,
};

#[cfg(target_has_atomic = "64")]
pub use native_entry::ravet_native_state_v1;
