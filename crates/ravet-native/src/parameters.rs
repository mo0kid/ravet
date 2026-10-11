//! Bounded native metadata and stopped-owner parameter/state operations.
use crate::{NativeProcessor, NativeProcessorEntryV1};
use ravet_foundation::{
    ParameterDescriptor, ParameterError, ParameterId, ParameterScale, ParameterSchema,
    ParameterState, Processor, Sample,
};
use std::{
    ffi::c_void,
    panic::{AssertUnwindSafe, catch_unwind},
    slice,
};
const MAX_PARAMETERS: usize = 1024;
const MAX_STATE: usize = 16 + 8 * MAX_PARAMETERS;
/// Consumer-declared roles. These describe metadata, not implicit event filtering
/// or control permissions. Consumer DSP/state restoration owns their behavior.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct NativeParameterRoles {
    pub bypass: bool,
    pub read_only: bool,
}
/// Optional native facade contract; portable processors need not depend on it.
/// Schema/roles are immutable for the entry lifetime. Snapshots use declared IDs;
/// restore must apply complete validated state atomically or leave state unchanged.
/// State callbacks run exclusively on a stopped owner, without automatic live
/// publication or UI/SDK scheduling. Product/version identify a consumer protocol.
pub trait NativeParameterProcessor<const N: usize> {
    const STATE_PRODUCT: [u8; 8];
    const STATE_VERSION: u32;
    fn parameter_schema() -> ParameterSchema<'static, N>;
    fn parameter_roles(_id: ParameterId) -> NativeParameterRoles {
        NativeParameterRoles::default()
    }
    fn parameter_snapshot(&self) -> ParameterState<'static, N>;
    fn restore_parameter_snapshot(
        &mut self,
        state: &ParameterState<'static, N>,
    ) -> Result<(), ParameterError>;
    /// Default complete little-endian stable-ID record. Explicit consumer
    /// overrides can validate/migrate legacy formats within the 8208-byte cap.
    fn decode_parameter_snapshot(
        bytes: &[u8],
    ) -> Result<ParameterState<'static, N>, ParameterError> {
        ParameterState::decode(
            Self::parameter_schema(),
            Self::STATE_PRODUCT,
            Self::STATE_VERSION,
            bytes,
        )
    }
}
/// Explicit prepared audio-owner contract. Both hooks must avoid allocation,
/// deallocation, blocking and I/O, and operate on global base state rather than
/// transient modulation/voices. Restore rejects atomically or applies complete
/// validated state. Schema/roles remain immutable and are cached on control.
pub trait NativeAudioParameterProcessor<const N: usize>: NativeParameterProcessor<N> {
    /// Snapshot complete global base values on the exclusive prepared audio owner.
    /// Schema identity/ranges are revalidated using control-cached metadata.
    fn audio_parameter_snapshot(&self) -> ParameterState<'static, N>;
    /// Apply a complete validated base snapshot atomically, preserving unrelated
    /// voices/history. Consumers schedule direct recall explicitly; the live
    /// publication companion calls this once per newly accepted publication.
    fn apply_audio_parameter_snapshot(
        &mut self,
        state: &ParameterState<'static, N>,
    ) -> Result<(), ParameterError>;
}
/// Canonical global physical metadata. flags: automatable=1, bypass=2, read-only=4;
/// scale: linear=0/log=1; intervals=0 continuous, otherwise intervals+1 choices.
/// UTF-8 name/unit lengths exclude their trailing zero; unused bytes/reserved zero.
/// Name<=255/unit<=63 bytes, no embedded NUL; oversized text rejects, never clips.
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct NativeParameterInfoV1 {
    pub id: u32,
    pub flags: u32,
    pub scale: u32,
    pub intervals: u32,
    pub minimum: f32,
    pub maximum: f32,
    pub default_value: f32,
    pub name_length: u32,
    pub unit_length: u32,
    pub reserved: u32,
    pub name: [u8; 256],
    pub unit: [u8; 64],
}
impl Default for NativeParameterInfoV1 {
    fn default() -> Self {
        Self {
            id: 0,
            flags: 0,
            scale: 0,
            intervals: 0,
            minimum: 0.,
            maximum: 0.,
            default_value: 0.,
            name_length: 0,
            unit_length: 0,
            reserved: 0,
            name: [0; 256],
            unit: [0; 64],
        }
    }
}
/// Complete snapshot field in declared schema order, with full stable u32 ID.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct NativeParameterValueV1 {
    pub id: u32,
    pub value: f32,
}
/// Optional immutable companion for the exact originating processor table.
/// Call codes 0 success/1 rejection/2 panic or owner poison. Metadata and pure
/// range conversion require no handle. Snapshot/save/load require exclusive
/// stopped ownership; no live cross-thread read or state publication is supplied.
/// All foreign output/input ranges are live/aligned/nonoverlapping with the handle
/// and each other. Successful outputs preserve unused destination tails; rejection
/// leaves them unchanged. Consumer snapshot/restore code owns its own invariants.
#[repr(C)]
#[derive(Clone, Copy)]
pub struct NativeParameterEntryV1 {
    pub version: u32,
    pub byte_size: u32,
    pub sample_bits: u32,
    pub parameter_count: u32,
    pub state_bytes: u32,
    pub state_version: u32,
    pub state_product: [u8; 8],
    pub processor_entry: extern "C" fn() -> *const NativeProcessorEntryV1,
    pub info: unsafe extern "C" fn(u32, *mut NativeParameterInfoV1) -> i32,
    pub normalize: unsafe extern "C" fn(u32, f32, *mut f32) -> i32,
    pub denormalize: unsafe extern "C" fn(u32, f32, *mut f32) -> i32,
    pub snapshot: unsafe extern "C" fn(*mut c_void, *mut NativeParameterValueV1, u32) -> i32,
    pub save_state: unsafe extern "C" fn(*mut c_void, *mut u8, u32) -> i32,
    pub load_state: unsafe extern "C" fn(*mut c_void, *const u8, u32) -> i32,
}
/// Static companion implementation for a native facade and bounded schema.
/// Entry acquisition validates metadata on control, returning None on invalid
/// bounds/text/version or metadata-provider panic. Core processor entry remains
/// separately available. Names/roles/schema must stay stable for its lifetime.
pub struct NativeParameters<P, S, const N: usize>(std::marker::PhantomData<(P, S)>);
fn aligned<T>(pointer: *const T) -> bool {
    (pointer as usize).is_multiple_of(align_of::<T>())
}
pub(crate) fn info(
    descriptor: ParameterDescriptor<'static>,
    roles: NativeParameterRoles,
) -> Option<NativeParameterInfoV1> {
    let name = descriptor.name.as_bytes();
    let unit = descriptor.unit.as_bytes();
    if name.len() > 255 || unit.len() > 63 || name.contains(&0) || unit.contains(&0) {
        return None;
    }
    let mut out = NativeParameterInfoV1 {
        id: descriptor.id.0,
        flags: u32::from(descriptor.automatable)
            | (u32::from(roles.bypass) << 1)
            | (u32::from(roles.read_only) << 2),
        scale: u32::from(descriptor.range.scale() == ParameterScale::Logarithmic),
        intervals: u32::from(descriptor.range.intervals().unwrap_or(0)),
        minimum: descriptor.range.minimum(),
        maximum: descriptor.range.maximum(),
        default_value: descriptor.default,
        name_length: name.len() as u32,
        unit_length: unit.len() as u32,
        ..Default::default()
    };
    out.name[..name.len()].copy_from_slice(name);
    out.unit[..unit.len()].copy_from_slice(unit);
    Some(out)
}
impl<
    P: Processor<S> + Default + Send + NativeParameterProcessor<N> + 'static,
    S: Sample + Send + 'static,
    const N: usize,
> NativeParameters<P, S, N>
{
    const ENTRY: NativeParameterEntryV1 = NativeParameterEntryV1 {
        version: 1,
        byte_size: size_of::<NativeParameterEntryV1>() as u32,
        sample_bits: (size_of::<S>() * 8) as u32,
        parameter_count: N as u32,
        state_bytes: if N <= MAX_PARAMETERS {
            (16 + 8 * N) as u32
        } else {
            0
        },
        state_version: P::STATE_VERSION,
        state_product: P::STATE_PRODUCT,
        processor_entry: Self::processor_entry,
        info: Self::metadata,
        normalize: Self::normalize,
        denormalize: Self::denormalize,
        snapshot: Self::snapshot,
        save_state: Self::save,
        load_state: Self::load,
    };
    /// Validate stable metadata once on control before native registration/use.
    pub fn entry() -> Option<&'static NativeParameterEntryV1> {
        catch_unwind(|| {
            if N > MAX_PARAMETERS || P::STATE_VERSION == 0 {
                return None;
            }
            for descriptor in P::parameter_schema().descriptors() {
                info(*descriptor, P::parameter_roles(descriptor.id))?;
            }
            Some(Self::canonical_entry())
        })
        .ok()
        .flatten()
    }
    // Keep one address-bearing body per concrete companion, across exports and
    // cross-codegen companion chains. Runtime metadata validation remains above.
    #[inline(never)]
    fn canonical_entry() -> &'static NativeParameterEntryV1 {
        &Self::ENTRY
    }
    extern "C" fn processor_entry() -> *const NativeProcessorEntryV1 {
        NativeProcessor::<P, S>::entry()
    }
    fn metadata_call(call: impl FnOnce() -> i32) -> i32 {
        catch_unwind(AssertUnwindSafe(call)).unwrap_or(2)
    }
    unsafe extern "C" fn metadata(index: u32, output: *mut NativeParameterInfoV1) -> i32 {
        Self::metadata_call(|| {
            if output.is_null() || !aligned(output) {
                return 1;
            }
            let schema = P::parameter_schema();
            let Some(descriptor) = schema.descriptors().get(index as usize) else {
                return 1;
            };
            let Some(record) = info(*descriptor, P::parameter_roles(descriptor.id)) else {
                return 1;
            };
            unsafe { output.write(record) };
            0
        })
    }
    unsafe extern "C" fn normalize(id: u32, value: f32, output: *mut f32) -> i32 {
        unsafe { Self::range(id, value, output, false) }
    }
    unsafe extern "C" fn denormalize(id: u32, value: f32, output: *mut f32) -> i32 {
        unsafe { Self::range(id, value, output, true) }
    }
    unsafe fn range(id: u32, value: f32, output: *mut f32, normalized: bool) -> i32 {
        Self::metadata_call(|| {
            if output.is_null() || !aligned(output) {
                return 1;
            }
            let schema = P::parameter_schema();
            let Some(index) = schema.index(ParameterId(id)) else {
                return 1;
            };
            let range = schema.descriptors()[index].range;
            let result = if normalized {
                range.denormalize(value)
            } else {
                range.normalize(value)
            };
            let Some(value) = result else {
                return 1;
            };
            unsafe { output.write(value) };
            0
        })
    }
    fn validate_snapshot(
        state: ParameterState<'static, N>,
    ) -> Result<ParameterState<'static, N>, ParameterError> {
        let schema = P::parameter_schema();
        let mut values = [0.; N];
        for (out, descriptor) in values.iter_mut().zip(schema.descriptors()) {
            *out = state
                .get(descriptor.id)
                .ok_or(ParameterError::WrongParameter(descriptor.id))?;
        }
        schema.state(values)
    }
    unsafe extern "C" fn snapshot(
        handle: *mut c_void,
        output: *mut NativeParameterValueV1,
        capacity: u32,
    ) -> i32 {
        NativeProcessor::<P, S>::control(handle, |owner| {
            if owner.poisoned {
                return 2;
            }
            if (capacity as usize) < N
                || capacity as usize > MAX_PARAMETERS
                || (N > 0 && (output.is_null() || !aligned(output)))
            {
                return 1;
            }
            let Ok(state) = Self::validate_snapshot(owner.processor.parameter_snapshot()) else {
                return 1;
            };
            for (index, descriptor) in P::parameter_schema().descriptors().iter().enumerate() {
                unsafe {
                    output.add(index).write(NativeParameterValueV1 {
                        id: descriptor.id.0,
                        value: state.values()[index],
                    })
                };
            }
            0
        })
    }
    unsafe extern "C" fn save(handle: *mut c_void, output: *mut u8, capacity: u32) -> i32 {
        NativeProcessor::<P, S>::control(handle, |owner| {
            if owner.poisoned {
                return 2;
            }
            let bytes = Self::ENTRY.state_bytes as usize;
            if output.is_null() || (capacity as usize) < bytes || capacity as usize > MAX_STATE {
                return 1;
            }
            let Ok(state) = Self::validate_snapshot(owner.processor.parameter_snapshot()) else {
                return 1;
            };
            let output = unsafe { slice::from_raw_parts_mut(output, bytes) };
            state
                .encode(P::STATE_PRODUCT, P::STATE_VERSION, output)
                .map_or(1, |()| 0)
        })
    }
    unsafe extern "C" fn load(handle: *mut c_void, input: *const u8, length: u32) -> i32 {
        NativeProcessor::<P, S>::control(handle, |owner| {
            if owner.poisoned {
                return 2;
            }
            if length as usize > MAX_STATE || (length > 0 && input.is_null()) {
                return 1;
            }
            let input = if length == 0 {
                &[]
            } else {
                unsafe { slice::from_raw_parts(input, length as usize) }
            };
            let Ok(state) = P::decode_parameter_snapshot(input).and_then(Self::validate_snapshot)
            else {
                return 1;
            };
            owner
                .processor
                .restore_parameter_snapshot(&state)
                .map_or(1, |()| 0)
        })
    }
}
/// Export a schema/state companion symbol for exact paired f32/f64 native facade
/// types and bounded parameter count. Invalid precision/metadata returns null.
#[macro_export]
macro_rules! export_native_parameters_v1 {
    ($symbol:ident,$processor32:ty,$processor64:ty,$count:expr) => {
        #[unsafe(no_mangle)]
        pub extern "C" fn $symbol(sample_bits: u32) -> *const $crate::NativeParameterEntryV1 {
            let entry = match sample_bits {
                32 => $crate::NativeParameters::<$processor32, f32, $count>::entry(),
                64 => $crate::NativeParameters::<$processor64, f64, $count>::entry(),
                _ => None,
            };
            entry.map_or(::std::ptr::null(), |entry| entry as *const _)
        }
    };
}
