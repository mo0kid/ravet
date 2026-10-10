//! Cached metadata and explicit prepared audio-owner observations, separate from state.
use crate::{NativeProcessor, NativeProcessorEntryV1};
use ravet_foundation::{Processor, Sample};
use std::{ffi::c_void, marker::PhantomData, panic::catch_unwind, ptr};

/// Immutable stable-ID readout description. Display bounds must be finite and
/// ordered; they are presentation hints, not observation clipping/validation
/// limits. Names/units use native UTF-8 255/63-byte caps without NUL. Descriptors
/// remain alive/immutable with their library; telemetry is not parameter state.
#[derive(Clone, Copy, Debug)]
pub struct NativeTelemetryDescriptor {
    pub id: u32,
    pub name: &'static str,
    pub unit: &'static str,
    pub display_minimum: f64,
    pub display_maximum: f64,
}
/// Canonical cached C metadata; unused text bytes and reserved field are zero.
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct NativeTelemetryInfoV1 {
    pub id: u32,
    pub reserved: u32,
    pub display_minimum: f64,
    pub display_maximum: f64,
    pub name_length: u32,
    pub unit_length: u32,
    pub name: [u8; 256],
    pub unit: [u8; 64],
}
impl Default for NativeTelemetryInfoV1 {
    fn default() -> Self {
        Self {
            id: 0,
            reserved: 0,
            display_minimum: 0.,
            display_maximum: 0.,
            name_length: 0,
            unit_length: 0,
            name: [0; 256],
            unit: [0; 64],
        }
    }
}
/// One finite physical observation; raw f64 extremes/subnormals are preserved.
/// flags currently permits only NONFINITE_SAMPLES=1 (sample input was nonfinite,
/// not this observed value). samples retains an exact consumer-declared u64
/// measurement-window count; zero/flags/window/retention policy is explicit.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct NativeTelemetryValueV1 {
    pub id: u32,
    pub flags: u32,
    pub value: f64,
    pub samples: u64,
}
/// Nonfinite input samples occurred in the consumer's observation window.
pub const NATIVE_TELEMETRY_NONFINITE_SAMPLES: u32 = 1;
/// Optional native facade observations, independent of global parameter state.
/// Descriptors are immutable and acquired on control only. The explicit snapshot
/// exclusively borrows prepared audio DSP, must avoid allocation/deallocation,
/// blocking/I/O, and supplies every declared ID once (any order). No implicit
/// cross-thread read, synchronization/publication, clear or host/UI scheduling.
pub trait NativeTelemetryProcessor<const N: usize> {
    fn telemetry_descriptors() -> &'static [NativeTelemetryDescriptor; N];
    fn audio_telemetry_snapshot(&self) -> [NativeTelemetryValueV1; N];
}
/// Invalid immutable telemetry schema/metadata. Construction rejects before
/// allocation for excessive count and preflights all descriptors before caching.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NativeTelemetryError {
    Capacity,
    InvalidDescriptor { index: usize },
    DuplicateId { id: u32 },
}
/// Immutable control-created telemetry metadata and sorted ID/index cache.
/// Shared info reads use no callbacks/heap/locks/I/O. Construction/drop may
/// allocate/deallocate; retain library, retire only after every borrower stops.
pub struct NativeTelemetryCache<const N: usize> {
    info: Vec<NativeTelemetryInfoV1>,
    lookup: Vec<(u32, u32)>,
}
impl<const N: usize> NativeTelemetryCache<N> {
    /// Cache <=1024 unique full-u32 IDs and valid native text/display hints.
    pub fn new(descriptors: &[NativeTelemetryDescriptor; N]) -> Result<Self, NativeTelemetryError> {
        if N > 1024 {
            return Err(NativeTelemetryError::Capacity);
        }
        for (index, d) in descriptors.iter().enumerate() {
            if !d.display_minimum.is_finite()
                || !d.display_maximum.is_finite()
                || d.display_minimum >= d.display_maximum
                || d.name.is_empty()
                || d.name.len() > 255
                || d.unit.len() > 63
                || d.name.contains('\0')
                || d.unit.contains('\0')
            {
                return Err(NativeTelemetryError::InvalidDescriptor { index });
            }
            if descriptors[..index].iter().any(|other| other.id == d.id) {
                return Err(NativeTelemetryError::DuplicateId { id: d.id });
            }
        }
        let mut info = Vec::with_capacity(N);
        let mut lookup = Vec::with_capacity(N);
        for (index, d) in descriptors.iter().enumerate() {
            let mut field = NativeTelemetryInfoV1 {
                id: d.id,
                display_minimum: d.display_minimum,
                display_maximum: d.display_maximum,
                name_length: d.name.len() as u32,
                unit_length: d.unit.len() as u32,
                ..NativeTelemetryInfoV1::default()
            };
            field.name[..d.name.len()].copy_from_slice(d.name.as_bytes());
            field.unit[..d.unit.len()].copy_from_slice(d.unit.as_bytes());
            info.push(field);
            lookup.push((d.id, index as u32));
        }
        lookup.sort_unstable_by_key(|field| field.0);
        Ok(Self { info, lookup })
    }
    /// Cached declared-order read, safe for concurrent borrowers.
    pub fn info(&self, index: usize) -> Option<&NativeTelemetryInfoV1> {
        self.info.get(index)
    }
    fn index(&self, id: u32) -> Option<usize> {
        self.lookup
            .binary_search_by_key(&id, |field| field.0)
            .ok()
            .map(|index| self.lookup[index].1 as usize)
    }
    fn validate(&self, input: [NativeTelemetryValueV1; N]) -> Option<[NativeTelemetryValueV1; N]> {
        let mut output = [NativeTelemetryValueV1::default(); N];
        let mut seen = [false; N];
        for field in input {
            let index = self.index(field.id)?;
            if seen[index]
                || field.flags & !NATIVE_TELEMETRY_NONFINITE_SAMPLES != 0
                || !field.value.is_finite()
            {
                return None;
            }
            seen[index] = true;
            output[index] = field;
        }
        Some(output)
    }
}
/// Optional paired native telemetry table. Acquire table/processor_entry and
/// create/destroy cache on control; info permits shared cached reads; snapshot
/// exclusively owns prepared DSP. Exact table/types/precision/library and live
/// aligned disjoint foreign ranges/cache/owner remain caller obligations.
/// Codes 0 success/1 rejection/2 snapshot panic or shared DSP poison until fresh
/// stopped preparation. Cache factory returns null on invalid metadata/provider
/// panic. Full snapshot validation precedes writes; tails/failed outputs survive.
/// No automatic publication, locking, reset, event delivery, state or scheduling.
#[repr(C)]
#[derive(Clone, Copy)]
pub struct NativeTelemetryEntryV1 {
    pub version: u32,
    pub byte_size: u32,
    pub sample_bits: u32,
    pub telemetry_count: u32,
    pub processor_entry: extern "C" fn() -> *const NativeProcessorEntryV1,
    pub create_cache: extern "C" fn() -> *mut c_void,
    pub destroy_cache: unsafe extern "C" fn(*mut c_void),
    pub info: unsafe extern "C" fn(*const c_void, u32, *mut NativeTelemetryInfoV1) -> i32,
    pub snapshot:
        unsafe extern "C" fn(*mut c_void, *const c_void, *mut NativeTelemetryValueV1, u32) -> i32,
}
/// Consumer-selected telemetry companion without separate DSP or mailbox owners.
/// No atomic-width gate is needed. Consumer explicit audio hook owns measurement
/// and real-time behavior; metadata is control-cached for all normal reads.
pub struct NativeTelemetry<P, S, const N: usize>(PhantomData<fn() -> (P, S)>);
fn aligned<T>(pointer: *const T) -> bool {
    (pointer as usize).is_multiple_of(align_of::<T>())
}
impl<
    P: Processor<S> + Default + Send + NativeTelemetryProcessor<N> + 'static,
    S: Sample + Send + 'static,
    const N: usize,
> NativeTelemetry<P, S, N>
{
    const ENTRY: NativeTelemetryEntryV1 = NativeTelemetryEntryV1 {
        version: 1,
        byte_size: size_of::<NativeTelemetryEntryV1>() as u32,
        sample_bits: (size_of::<S>() * 8) as u32,
        telemetry_count: N as u32,
        processor_entry: Self::processor_entry,
        create_cache: Self::create_cache,
        destroy_cache: Self::destroy_cache,
        info: Self::info,
        snapshot: Self::snapshot,
    };
    /// Validate complete immutable descriptors on control. Invalid metadata or
    /// provider panic returns None without altering existing caches/processors.
    /// This acquisition may allocate; retain the result instead of calling on audio.
    pub fn entry() -> Option<&'static NativeTelemetryEntryV1> {
        if N > 1024 {
            return None;
        }
        catch_unwind(|| {
            NativeTelemetryCache::new(P::telemetry_descriptors()).ok()?;
            Some(&Self::ENTRY)
        })
        .ok()
        .flatten()
    }
    extern "C" fn processor_entry() -> *const NativeProcessorEntryV1 {
        NativeProcessor::<P, S>::entry()
    }
    extern "C" fn create_cache() -> *mut c_void {
        if N > 1024 {
            return ptr::null_mut();
        }
        catch_unwind(|| {
            NativeTelemetryCache::new(P::telemetry_descriptors()).map_or(ptr::null_mut(), |cache| {
                Box::into_raw(Box::new(cache)).cast()
            })
        })
        .unwrap_or(ptr::null_mut())
    }
    unsafe extern "C" fn destroy_cache(cache: *mut c_void) {
        if !cache.is_null() {
            drop(unsafe { Box::from_raw(cache.cast::<NativeTelemetryCache<N>>()) });
        }
    }
    unsafe fn get<'a>(cache: *const c_void) -> Option<&'a NativeTelemetryCache<N>> {
        if cache.is_null() || !aligned(cache.cast::<NativeTelemetryCache<N>>()) {
            None
        } else {
            unsafe { cache.cast::<NativeTelemetryCache<N>>().as_ref() }
        }
    }
    unsafe extern "C" fn info(
        cache: *const c_void,
        index: u32,
        output: *mut NativeTelemetryInfoV1,
    ) -> i32 {
        if output.is_null() || !aligned(output) {
            return 1;
        }
        let Some(cache) = (unsafe { Self::get(cache) }) else {
            return 1;
        };
        let Some(info) = cache.info(index as usize) else {
            return 1;
        };
        unsafe {
            output.write(*info);
        }
        0
    }
    unsafe extern "C" fn snapshot(
        handle: *mut c_void,
        cache: *const c_void,
        output: *mut NativeTelemetryValueV1,
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
        let Some(cache) = (unsafe { Self::get(cache) }) else {
            return 1;
        };
        NativeProcessor::<P, S>::control(handle, |owner| {
            if owner.poisoned {
                return 2;
            }
            if !owner.is_prepared() {
                return 1;
            }
            let Some(values) = cache.validate(owner.processor.audio_telemetry_snapshot()) else {
                return 1;
            };
            for (index, field) in values.into_iter().enumerate() {
                unsafe {
                    output.add(index).write(field);
                }
            }
            0
        })
    }
}
/// Export an optional telemetry companion for exact f32/f64 native facade types.
#[macro_export]
macro_rules! export_native_telemetry_v1 {
    ($symbol:ident,$processor32:ty,$processor64:ty,$count:expr) => {
        #[unsafe(no_mangle)]
        pub extern "C" fn $symbol(sample_bits: u32) -> *const $crate::NativeTelemetryEntryV1 {
            let entry = match sample_bits {
                32 => $crate::NativeTelemetry::<$processor32, f32, $count>::entry(),
                64 => $crate::NativeTelemetry::<$processor64, f64, $count>::entry(),
                _ => None,
            };
            entry.map_or(::std::ptr::null(), |entry| entry as *const _)
        }
    };
}
