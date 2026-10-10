//! Explicit complete base-state access on an exclusively owned prepared processor.
use crate::{
    NativeAudioParameterProcessor, NativeCachedParameterEntryV1, NativeCachedParameters,
    NativeParameterCache, NativeParameterValueV1, NativeProcessor,
};
use ravet_foundation::{ParameterError, ParameterId, ParameterState, Processor, Sample};
use std::{ffi::c_void, marker::PhantomData, ptr, slice};

/// Optional companion for an exact native processor/cache/precision pair. Entry
/// and cache acquisition/retirement are control-only. Apply/snapshot exclusively
/// borrow the prepared audio owner and share its panic poison/reprepare recovery.
/// Codes 0 success, 1 rejection, 2 panic/poison. Complete arrays use physical f32
/// global base values, not modulation/voices; no codecs/providers run on audio.
///
/// At most 1024 fields/capacity. Restore validates full known/unique IDs and
/// canonical ranges before one atomic consumer hook; caller schedules it only
/// when needed (repeated restoration can restart smoothing). Snapshot validates
/// complete consumer output and reorders to cached declared order before writes;
/// failed output and successful tails survive. Hooks obey explicit real-time
/// obligations. No implicit publication, generations, notifications or retries.
/// Direct calls leave live-mailbox accepted revisions unchanged. Mixing state
/// protocols requires explicit consumer ordering/new publication or reprepare.
/// Foreign ranges must be
/// live/aligned/disjoint from each other, cache and exclusive processor; retain
/// exact originating tables/types/library through all borrows. No atomics needed.
#[repr(C)]
#[derive(Clone, Copy)]
pub struct NativeAudioStateEntryV1 {
    pub version: u32,
    pub byte_size: u32,
    pub sample_bits: u32,
    pub parameter_count: u32,
    pub cached_parameters_entry: extern "C" fn() -> *const NativeCachedParameterEntryV1,
    pub apply:
        unsafe extern "C" fn(*mut c_void, *const c_void, *const NativeParameterValueV1, u32) -> i32,
    pub snapshot:
        unsafe extern "C" fn(*mut c_void, *const c_void, *mut NativeParameterValueV1, u32) -> i32,
}
/// Consumer opt-in via explicit prepared audio-state hooks. Cache is created by
/// the paired cached-parameter entry; this companion allocates no separate owner.
/// Normal validation/apply/snapshot use bounded stack storage and cached metadata
/// without framework heap actions, locks or I/O. Consumer hooks remain responsible
/// for their own real-time contract and atomic failed restores.
pub struct NativeAudioState<P, S, const N: usize>(PhantomData<fn() -> (P, S)>);
fn aligned<T>(pointer: *const T) -> bool {
    (pointer as usize).is_multiple_of(align_of::<T>())
}
impl<
    P: Processor<S> + Default + Send + NativeAudioParameterProcessor<N> + 'static,
    S: Sample + Send + 'static,
    const N: usize,
> NativeAudioState<P, S, N>
{
    const ENTRY: NativeAudioStateEntryV1 = NativeAudioStateEntryV1 {
        version: 1,
        byte_size: size_of::<NativeAudioStateEntryV1>() as u32,
        sample_bits: (size_of::<S>() * 8) as u32,
        parameter_count: N as u32,
        cached_parameters_entry: Self::cached_entry,
        apply: Self::apply,
        snapshot: Self::snapshot,
    };
    /// Validate immutable consumer metadata on control. Provider panic/invalid
    /// metadata returns None, without touching any existing cache or processor.
    pub fn entry() -> Option<&'static NativeAudioStateEntryV1> {
        NativeCachedParameters::<P, S, N>::entry().map(|_| &Self::ENTRY)
    }
    extern "C" fn cached_entry() -> *const NativeCachedParameterEntryV1 {
        NativeCachedParameters::<P, S, N>::entry().map_or(ptr::null(), |table| table as *const _)
    }
    fn complete(
        cache: &NativeParameterCache<N>,
        input: &[NativeParameterValueV1],
    ) -> Result<ParameterState<'static, N>, ParameterError> {
        let mut values = [0.; N];
        let mut seen = [false; N];
        for field in input {
            let id = ParameterId(field.id);
            let index = cache.index(id).ok_or(ParameterError::WrongParameter(id))?;
            if seen[index] {
                return Err(ParameterError::WrongParameter(id));
            }
            seen[index] = true;
            values[index] = field.value;
        }
        cache.schema.state(values)
    }
    unsafe extern "C" fn apply(
        handle: *mut c_void,
        cache: *const c_void,
        input: *const NativeParameterValueV1,
        count: u32,
    ) -> i32 {
        if N > 1024
            || count as usize != N
            || !aligned(handle.cast::<NativeProcessor<P, S>>())
            || (N > 0 && (input.is_null() || !aligned(input)))
        {
            return 1;
        }
        let Some(cache) = (unsafe { NativeCachedParameters::<P, S, N>::get(cache) }) else {
            return 1;
        };
        NativeProcessor::<P, S>::control(handle, |owner| {
            if owner.poisoned {
                return 2;
            }
            if !owner.is_prepared() {
                return 1;
            }
            let input = if N == 0 {
                &[]
            } else {
                unsafe { slice::from_raw_parts(input, N) }
            };
            let Ok(state) = Self::complete(cache, input) else {
                return 1;
            };
            owner
                .processor
                .apply_audio_parameter_snapshot(&state)
                .map_or(1, |()| 0)
        })
    }
    unsafe extern "C" fn snapshot(
        handle: *mut c_void,
        cache: *const c_void,
        output: *mut NativeParameterValueV1,
        capacity: u32,
    ) -> i32 {
        if N > 1024
            || (capacity as usize) < N
            || capacity > 1024
            || !aligned(handle.cast::<NativeProcessor<P, S>>())
            || (N > 0 && (output.is_null() || !aligned(output)))
        {
            return 1;
        }
        let Some(cache) = (unsafe { NativeCachedParameters::<P, S, N>::get(cache) }) else {
            return 1;
        };
        NativeProcessor::<P, S>::control(handle, |owner| {
            if owner.poisoned {
                return 2;
            }
            if !owner.is_prepared() {
                return 1;
            }
            let state = owner.processor.audio_parameter_snapshot();
            let mut values = [0.; N];
            for (value, descriptor) in values.iter_mut().zip(cache.schema.descriptors()) {
                let Some(field) = state.get(descriptor.id) else {
                    return 1;
                };
                *value = field;
            }
            let Ok(state) = cache.schema.state(values) else {
                return 1;
            };
            for (index, descriptor) in cache.schema.descriptors().iter().enumerate() {
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
}
/// Export an optional direct audio-state companion, retaining all earlier table
/// layouts. Unsupported precision/invalid provider metadata returns null.
#[macro_export]
macro_rules! export_native_audio_state_v1 {
    ($symbol:ident,$processor32:ty,$processor64:ty,$count:expr) => {
        #[unsafe(no_mangle)]
        pub extern "C" fn $symbol(sample_bits: u32) -> *const $crate::NativeAudioStateEntryV1 {
            let entry = match sample_bits {
                32 => $crate::NativeAudioState::<$processor32, f32, $count>::entry(),
                64 => $crate::NativeAudioState::<$processor64, f64, $count>::entry(),
                _ => None,
            };
            entry.map_or(::std::ptr::null(), |entry| entry as *const _)
        }
    };
}
