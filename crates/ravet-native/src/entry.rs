//! Consumer-selected, SDK-independent native processor lifecycle entry.
use crate::{
    NativeAudioAdapter, NativeEventV1, NativePlanarBus, NativeTransportV1, decode_native_events,
    encode_native_events,
};
use ravet_foundation::{
    AudioLayout, Event, EventData, ParameterId, ProcessConfig, ProcessStatus, Processor, Sample,
    Tail, Transport,
};
use std::{
    ffi::c_void,
    panic::{AssertUnwindSafe, catch_unwind},
    ptr, slice,
};
/// Prepared main/auxiliary channel counts; unused fixed-array slots are zero.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct NativeBusLayoutV1 {
    pub inputs: u32,
    pub outputs: u32,
}
/// Fixed configuration: version=1, reserved=0, bus count 1..=9; main first.
/// Rate/capacities/layouts follow NativeAudioAdapter limits. Unused buses are zero.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct NativeProcessorConfigV1 {
    pub version: u32,
    pub maximum_frames: u32,
    pub maximum_events: u32,
    pub maximum_output_events: u32,
    pub bus_count: u32,
    pub reserved: u32,
    pub sample_rate: f64,
    pub buses: [NativeBusLayoutV1; 9],
}
impl NativeProcessorConfigV1 {
    /// Validate the complete native configuration without mutating a processor.
    pub fn to_configuration(self) -> Option<ProcessConfig> {
        if self.version != 1
            || self.reserved != 0
            || !(1..=9).contains(&self.bus_count)
            || !(1..=65536).contains(&self.maximum_frames)
            || self.maximum_events > 1024
            || self.maximum_output_events > 1024
        {
            return None;
        }
        let count = self.bus_count as usize;
        if self.buses[count..]
            .iter()
            .any(|b| *b != NativeBusLayoutV1::default())
        {
            return None;
        }
        let layout =
            |b: NativeBusLayoutV1| AudioLayout::new(b.inputs as usize, b.outputs as usize).ok();
        let main = layout(self.buses[0])?;
        let mut auxiliaries = [AudioLayout::MONO; 8];
        for (out, bus) in auxiliaries.iter_mut().zip(&self.buses[1..count]) {
            *out = layout(*bus)?;
        }
        Some(
            ProcessConfig::new(
                self.sample_rate,
                self.maximum_frames as usize,
                self.maximum_events as usize,
                main,
            )
            .ok()?
            .with_auxiliary_buses(&auxiliaries[..count - 1])
            .ok()?
            .with_output_events(self.maximum_output_events as usize),
        )
    }
}
/// Addresses of native S* pointer arrays for the originating table's precision.
/// This is layout-compatible with typed native planar buses. Null arrays are
/// permitted for zero-frame processing and directions with zero channels.
#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct NativeProcessorBusV1 {
    pub input_channels: u32,
    pub output_channels: u32,
    pub inputs: *const c_void,
    pub outputs: *const c_void,
}
/// A process result is always rewritten when its pointer is valid. Status is
/// 0 Continue, 1 Silence, 2 Fault; event count is zero on every fault.
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct NativeProcessorResultV1 {
    pub status: u32,
    pub event_count: u32,
}
/// Prepared latency/tail declaration, without causing host restart or draining.
/// tail_kind is 0 None, 1 finite samples, 2 Infinite; unused samples are zero.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct NativeProcessorTimingV1 {
    pub latency_samples: u32,
    pub tail_kind: u32,
    pub tail_samples: u64,
}
/// Immutable v1 function table. Call codes: 0 success, 1 rejection/backend fault,
/// 2 panic/poison. All handles originate from this exact table and are exclusively
/// serialized; stop callbacks before prepare/deactivate/destroy and control timing
/// queries. Tables/library
/// remain loaded until handles/callbacks end. See include/RavetNativeProcessor.h.
///
/// Unsafe callbacks require live aligned nonoverlapping record/result/handle
/// storage and valid native bus sample/pointer ranges in this table's precision.
/// Samples may alias inputs; output ranges must be disjoint. Event metadata and
/// result/output-record storage never overlap writable samples. Null record arrays
/// are allowed only for zero count/capacity; null transport means unknown context.
#[repr(C)]
#[derive(Clone, Copy)]
pub struct NativeProcessorEntryV1 {
    pub version: u32,
    pub byte_size: u32,
    pub sample_bits: u32,
    pub reserved: u32,
    pub create: extern "C" fn() -> *mut c_void,
    pub destroy: unsafe extern "C" fn(*mut c_void),
    pub prepare: unsafe extern "C" fn(*mut c_void, *const NativeProcessorConfigV1) -> i32,
    pub reset: unsafe extern "C" fn(*mut c_void) -> i32,
    pub deactivate: unsafe extern "C" fn(*mut c_void) -> i32,
    pub process: unsafe extern "C" fn(
        *mut c_void,
        *const NativeProcessorBusV1,
        u32,
        u32,
        *const NativeEventV1,
        u32,
        *const NativeTransportV1,
        *mut NativeEventV1,
        u32,
        *mut NativeProcessorResultV1,
    ) -> i32,
    pub timing: unsafe extern "C" fn(*mut c_void, *mut NativeProcessorTimingV1) -> i32,
}
/// Consumer-owned opaque handle implementation. Processor must be Send so the
/// stopped handle can transfer to its exclusive audio owner. SDK registration,
/// parameter/state schema, editor and format event/transport mapping are separate.
pub struct NativeProcessor<P: Processor<S> + Default + Send + 'static, S: Sample + Send + 'static> {
    pub(crate) processor: P,
    adapter: Option<NativeAudioAdapter<S>>,
    decoded: Vec<Event>,
    pub(crate) poisoned: bool,
    #[cfg(target_has_atomic = "64")]
    pub(crate) state_revision: Option<(u64, u64)>,
}
fn aligned<T>(p: *const T) -> bool {
    (p as usize).is_multiple_of(align_of::<T>())
}
impl<P: Processor<S> + Default + Send + 'static, S: Sample + Send + 'static> NativeProcessor<P, S> {
    const ENTRY: NativeProcessorEntryV1 = NativeProcessorEntryV1 {
        version: 1,
        byte_size: size_of::<NativeProcessorEntryV1>() as u32,
        sample_bits: (size_of::<S>() * 8) as u32,
        reserved: 0,
        create: Self::create,
        destroy: Self::destroy,
        prepare: Self::prepare,
        reset: Self::reset,
        deactivate: Self::deactivate,
        process: Self::process,
        timing: Self::timing,
    };
    /// Static immutable table for this processor and native sample precision.
    pub const fn entry() -> &'static NativeProcessorEntryV1 {
        &Self::ENTRY
    }
    extern "C" fn create() -> *mut c_void {
        catch_unwind(|| {
            Box::into_raw(Box::new(Self {
                processor: P::default(),
                adapter: None,
                decoded: Vec::new(),
                poisoned: false,
                #[cfg(target_has_atomic = "64")]
                state_revision: None,
            }))
            .cast()
        })
        .unwrap_or(ptr::null_mut())
    }
    unsafe extern "C" fn destroy(handle: *mut c_void) {
        if handle.is_null() {
            return;
        }
        let _ = catch_unwind(AssertUnwindSafe(|| {
            let mut owned = unsafe { Box::from_raw(handle.cast::<Self>()) };
            owned.processor.deactivate();
        }));
    }
    pub(crate) fn is_prepared(&self) -> bool {
        self.adapter.is_some()
    }
    pub(crate) fn control(handle: *mut c_void, operation: impl FnOnce(&mut Self) -> i32) -> i32 {
        let Some(owner) = (unsafe { handle.cast::<Self>().as_mut() }) else {
            return 1;
        };
        match catch_unwind(AssertUnwindSafe(|| operation(owner))) {
            Ok(code) => code,
            Err(_) => {
                owner.poisoned = true;
                2
            }
        }
    }
    unsafe extern "C" fn prepare(
        handle: *mut c_void,
        config: *const NativeProcessorConfigV1,
    ) -> i32 {
        Self::control(handle, |owner| {
            owner.adapter = None;
            #[cfg(target_has_atomic = "64")]
            {
                owner.state_revision = None;
            }
            owner.decoded.clear();
            if config.is_null() || !aligned(config) {
                owner.processor.deactivate();
                return 1;
            }
            let Some(config) = (unsafe { *config }).to_configuration() else {
                owner.processor.deactivate();
                return 1;
            };
            let Ok(adapter) = NativeAudioAdapter::prepare(&mut owner.processor, config) else {
                return 1;
            };
            owner.decoded.resize(
                config.maximum_events(),
                Event {
                    frame_offset: 0,
                    data: EventData::ParameterValue {
                        id: ParameterId(0),
                        value: 0.,
                    },
                },
            );
            owner.adapter = Some(adapter);
            owner.poisoned = false;
            0
        })
    }
    unsafe extern "C" fn reset(handle: *mut c_void) -> i32 {
        Self::control(handle, |owner| {
            if owner.poisoned {
                return 2;
            }
            let Some(adapter) = &mut owner.adapter else {
                return 1;
            };
            if adapter.reset(&mut owner.processor).is_err() {
                owner.poisoned = adapter.is_poisoned();
                return if owner.poisoned { 2 } else { 1 };
            }
            0
        })
    }
    unsafe extern "C" fn deactivate(handle: *mut c_void) -> i32 {
        Self::control(handle, |owner| {
            owner.adapter = None;
            #[cfg(target_has_atomic = "64")]
            {
                owner.state_revision = None;
            }
            owner.decoded.clear();
            owner.processor.deactivate();
            0
        })
    }
    unsafe extern "C" fn timing(handle: *mut c_void, output: *mut NativeProcessorTimingV1) -> i32 {
        Self::control(handle, |owner| {
            if output.is_null() || !aligned(output) || owner.adapter.is_none() {
                return 1;
            }
            if owner.poisoned {
                return 2;
            }
            let (tail_kind, tail_samples) = match owner.processor.tail() {
                Tail::None => (0, 0),
                Tail::Samples(n) => (1, n),
                Tail::Infinite => (2, 0),
            };
            let result = NativeProcessorTimingV1 {
                latency_samples: owner.processor.latency_samples(),
                tail_kind,
                tail_samples,
            };
            unsafe { output.write(result) };
            0
        })
    }
    unsafe extern "C" fn process(
        handle: *mut c_void,
        buses: *const NativeProcessorBusV1,
        bus_count: u32,
        frames: u32,
        input: *const NativeEventV1,
        input_count: u32,
        transport: *const NativeTransportV1,
        output: *mut NativeEventV1,
        output_capacity: u32,
        result: *mut NativeProcessorResultV1,
    ) -> i32 {
        let mut native = [NativePlanarBus {
            input_channels: 0,
            output_channels: 0,
            inputs: ptr::null(),
            outputs: ptr::null(),
        }; 9];
        let count = bus_count as usize;
        // Never dereference an unsupported raw bus count or missing array.
        let valid_buses = count <= 9 && (count == 0 || (!buses.is_null() && aligned(buses)));
        if valid_buses && count > 0 {
            let raw = unsafe { slice::from_raw_parts(buses, count) };
            for (out, bus) in native.iter_mut().zip(raw) {
                *out = NativePlanarBus {
                    input_channels: bus.input_channels,
                    output_channels: bus.output_channels,
                    inputs: bus.inputs.cast(),
                    outputs: bus.outputs.cast(),
                };
            }
        }
        let native = &native[..if valid_buses { count } else { 0 }];
        let valid_result = !result.is_null() && aligned(result);
        if valid_result {
            unsafe {
                result.write(NativeProcessorResultV1 {
                    status: 2,
                    event_count: 0,
                })
            };
        }
        let code = Self::control(handle, |owner| {
            if !valid_buses || !valid_result {
                return 1;
            }
            if owner.poisoned {
                return 2;
            }
            let Some(adapter) = &mut owner.adapter else {
                return 1;
            };
            let config = adapter.configuration();
            if frames as usize > config.maximum_frames()
                || input_count as usize > config.maximum_events()
                || (output_capacity as usize) < config.maximum_output_events()
                || output_capacity > 1024
                || (input_count > 0 && (input.is_null() || !aligned(input)))
                || (output_capacity > 0 && (output.is_null() || !aligned(output)))
            {
                return 1;
            }
            let records = if input_count == 0 {
                &[]
            } else {
                unsafe { slice::from_raw_parts(input, input_count as usize) }
            };
            let Ok(events) = decode_native_events(records, frames as usize, &mut owner.decoded)
            else {
                return 1;
            };
            let transport = if transport.is_null() {
                Transport::default()
            } else {
                if !aligned(transport) {
                    return 1;
                }
                let Ok(value) = (unsafe { *transport }).to_transport() else {
                    return 1;
                };
                value
            };
            let report = unsafe {
                adapter.process_native(
                    &mut owner.processor,
                    native,
                    frames as usize,
                    events,
                    transport,
                )
            };
            if matches!(report.status, ProcessStatus::Fault(_)) {
                owner.poisoned = adapter.is_poisoned();
                return if owner.poisoned { 2 } else { 1 };
            }
            let status = if report.status == ProcessStatus::Silence {
                1
            } else {
                0
            };
            let output = if output_capacity == 0 {
                &mut []
            } else {
                unsafe { slice::from_raw_parts_mut(output, output_capacity as usize) }
            };
            let Ok(records) = encode_native_events(report.events, frames as usize, output) else {
                return 1;
            };
            unsafe {
                result.write(NativeProcessorResultV1 {
                    status,
                    event_count: records.len() as u32,
                })
            };
            0
        });
        if code != 0 {
            unsafe { crate::raw::silence(native, frames as usize) };
        }
        code
    }
}
/// Export one immutable native entry symbol for consumer-selected f32/f64
/// Default+Send processors. Symbol accepts 32/64, returning null otherwise.
/// SDK factories/state/metadata/editor APIs remain consumer-selected. The library
/// must remain loaded until all handles and callbacks have ended.
#[macro_export]
macro_rules! export_native_processor_v1 {
    ($symbol:ident,$processor32:ty,$processor64:ty) => {
        #[unsafe(no_mangle)]
        pub extern "C" fn $symbol(sample_bits: u32) -> *const $crate::NativeProcessorEntryV1 {
            match sample_bits {
                32 => $crate::NativeProcessor::<$processor32, f32>::entry(),
                64 => $crate::NativeProcessor::<$processor64, f64>::entry(),
                _ => ::std::ptr::null(),
            }
        }
    };
}
