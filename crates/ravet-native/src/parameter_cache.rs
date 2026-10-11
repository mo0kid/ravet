//! Control-prepared immutable global parameter mapping for native audio consumers.
use crate::{
    NativeEventV1, NativeParameterEntryV1, NativeParameterInfoV1, NativeParameterProcessor,
    NativeParameterRoles, NativeParameters, native_event_kind,
};
use ravet_foundation::{ParameterError, ParameterId, ParameterSchema, Processor, Sample};
use std::{ffi::c_void, marker::PhantomData, panic::catch_unwind, ptr, slice};

/// Host normalized global value. Finite 0..=1 f64 intent narrows explicitly to
/// Foundation's normalized f32 contract before mapping to physical f32. This is
/// not a physical/scoped modulation record; sample offsets retain input order.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct NativeNormalizedParameterEventV1 {
    pub offset: u32,
    pub id: u32,
    pub normalized: f64,
}
/// Complete conversion fails before any destination prefix changes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NativeParameterMappingError {
    Capacity,
    InvalidRecord { index: usize },
    OffsetOutOfRange { index: usize },
    Unordered { index: usize },
}
/// Immutable schema/ranges/roles cached on control. Creation/drop may allocate/
/// deallocate; reads and mapping use bounded cached storage without providers,
/// heap actions, locks or I/O. Borrow on multiple threads, retiring only after all
/// borrowers stop. Schema references/library remain alive for its full lifetime.
/// Role declarations do not silently filter read-only/nonautomatable parameters.
pub struct NativeParameterCache<const N: usize> {
    pub(crate) schema: ParameterSchema<'static, N>,
    metadata: Vec<NativeParameterInfoV1>,
    lookup: Vec<(u32, u32)>,
}
impl<const N: usize> NativeParameterCache<N> {
    /// Cache <=1024 valid immutable native descriptors and roles on control.
    /// Names/units use the native 255/63-byte limits. Provider panic is not caught
    /// by this safe Rust constructor; the optional foreign factory contains it.
    pub fn new(
        schema: ParameterSchema<'static, N>,
        mut roles: impl FnMut(ParameterId) -> NativeParameterRoles,
    ) -> Result<Self, ParameterError> {
        if N > 1024 {
            return Err(ParameterError::InvalidMetadata);
        }
        let mut metadata = Vec::with_capacity(N);
        let mut lookup = Vec::with_capacity(N);
        for (index, descriptor) in schema.descriptors().iter().enumerate() {
            metadata.push(
                crate::parameters::info(*descriptor, roles(descriptor.id))
                    .ok_or(ParameterError::InvalidMetadata)?,
            );
            lookup.push((descriptor.id.0, index as u32));
        }
        lookup.sort_unstable_by_key(|pair| pair.0);
        Ok(Self {
            schema,
            metadata,
            lookup,
        })
    }
    /// Cached immutable declared-order descriptor, without provider callbacks.
    pub fn info(&self, index: usize) -> Option<&NativeParameterInfoV1> {
        self.metadata.get(index)
    }
    pub(crate) fn index(&self, id: ParameterId) -> Option<usize> {
        self.lookup
            .binary_search_by_key(&id.0, |pair| pair.0)
            .ok()
            .map(|index| self.lookup[index].1 as usize)
    }
    /// Foundation finite clamp/snap mapping, using control-cached ranges only.
    pub fn normalize(&self, id: ParameterId, physical: f32) -> Option<f32> {
        self.schema.descriptors()[self.index(id)?]
            .range
            .normalize(physical)
    }
    /// Foundation finite normalized clamp and linear/log/discrete conversion.
    pub fn denormalize(&self, id: ParameterId, normalized: f32) -> Option<f32> {
        self.schema.descriptors()[self.index(id)?]
            .range
            .denormalize(normalized)
    }
    fn event(
        &self,
        input: NativeNormalizedParameterEventV1,
        index: usize,
    ) -> Result<NativeEventV1, NativeParameterMappingError> {
        if !input.normalized.is_finite() || !(0. ..=1.).contains(&input.normalized) {
            return Err(NativeParameterMappingError::InvalidRecord { index });
        }
        let physical = self
            .denormalize(ParameterId(input.id), input.normalized as f32)
            .ok_or(NativeParameterMappingError::InvalidRecord { index })?;
        Ok(NativeEventV1 {
            offset: input.offset,
            kind: native_event_kind::PARAMETER_VALUE,
            parameter_id: input.id,
            value: f64::from(physical),
            ..NativeEventV1::default()
        })
    }
    /// Convert <=1024 ordered normalized values for <=65536 frames atomically.
    /// Zero-frame offsets must be zero. Unknown IDs/out-of-range/nonfinite values
    /// reject; consumer format code explicitly filters/routes unsupported IDs.
    /// Equal-offset order and destination tails survive. Two bounded passes,
    /// cached binary ID lookup; no provider callbacks or sort/heap on audio.
    pub fn convert_events(
        &self,
        input: &[NativeNormalizedParameterEventV1],
        frames: usize,
        output: &mut [NativeEventV1],
    ) -> Result<usize, NativeParameterMappingError> {
        if frames > 65536 || input.len() > 1024 || output.len() > 1024 || output.len() < input.len()
        {
            return Err(NativeParameterMappingError::Capacity);
        }
        let mut previous = 0;
        for (index, event) in input.iter().copied().enumerate() {
            if (frames == 0 && event.offset != 0) || (frames > 0 && event.offset as usize >= frames)
            {
                return Err(NativeParameterMappingError::OffsetOutOfRange { index });
            }
            if event.offset < previous {
                return Err(NativeParameterMappingError::Unordered { index });
            }
            self.event(event, index)?;
            previous = event.offset;
        }
        for (index, event) in input.iter().copied().enumerate() {
            output[index] = self.event(event, index)?;
        }
        Ok(input.len())
    }
}

/// Paired immutable cache factory and shared cached reads. Create/drop and paired
/// metadata entry acquisition are control-only; cached callbacks may overlap on
/// any owner. Codes 0 success, 1 rejection; create returns null on provider panic/
/// invalid metadata. Exact originating table/cache/library must remain alive.
/// Destroy after every borrower stops. Foreign ranges are live/aligned/disjoint;
/// failed calls preserve outputs. No DSP borrow or automatic scheduling is implied.
#[repr(C)]
#[derive(Clone, Copy)]
pub struct NativeCachedParameterEntryV1 {
    pub version: u32,
    pub byte_size: u32,
    pub sample_bits: u32,
    pub parameter_count: u32,
    pub parameters_entry: extern "C" fn() -> *const NativeParameterEntryV1,
    pub create: extern "C" fn() -> *mut c_void,
    pub destroy: unsafe extern "C" fn(*mut c_void),
    pub info: unsafe extern "C" fn(*const c_void, u32, *mut NativeParameterInfoV1) -> i32,
    pub normalize: unsafe extern "C" fn(*const c_void, u32, f32, *mut f32) -> i32,
    pub denormalize: unsafe extern "C" fn(*const c_void, u32, f32, *mut f32) -> i32,
    pub convert_events: unsafe extern "C" fn(
        *const c_void,
        *const NativeNormalizedParameterEventV1,
        u32,
        u32,
        *mut NativeEventV1,
        u32,
    ) -> i32,
}
/// Consumer-selected cache companion, without owning a processor. Pure cached
/// callbacks do not invoke P or acquire metadata on audio. Default/Send/precision
/// bounds retain exact pairing with the existing native parameter/processor entry.
pub struct NativeCachedParameters<P, S, const N: usize>(PhantomData<fn() -> (P, S)>);
fn aligned<T>(pointer: *const T) -> bool {
    (pointer as usize).is_multiple_of(align_of::<T>())
}
impl<
    P: Processor<S> + Default + Send + NativeParameterProcessor<N> + 'static,
    S: Sample + Send + 'static,
    const N: usize,
> NativeCachedParameters<P, S, N>
{
    const ENTRY: NativeCachedParameterEntryV1 = NativeCachedParameterEntryV1 {
        version: 1,
        byte_size: size_of::<NativeCachedParameterEntryV1>() as u32,
        sample_bits: (size_of::<S>() * 8) as u32,
        parameter_count: N as u32,
        parameters_entry: Self::parameters_entry,
        create: Self::create,
        destroy: Self::destroy,
        info: Self::info,
        normalize: Self::normalize,
        denormalize: Self::denormalize,
        convert_events: Self::events,
    };
    /// Validate consumer metadata on control before acquiring the companion.
    pub fn entry() -> Option<&'static NativeCachedParameterEntryV1> {
        NativeParameters::<P, S, N>::entry().map(|_| Self::canonical_entry())
    }
    // Keep one address-bearing body per concrete companion, across exports and
    // cross-codegen companion chains. Runtime metadata validation remains above.
    #[inline(never)]
    fn canonical_entry() -> &'static NativeCachedParameterEntryV1 {
        &Self::ENTRY
    }
    extern "C" fn parameters_entry() -> *const NativeParameterEntryV1 {
        NativeParameters::<P, S, N>::entry().map_or(ptr::null(), |entry| entry as *const _)
    }
    extern "C" fn create() -> *mut c_void {
        catch_unwind(|| {
            if N > 1024 || P::STATE_VERSION == 0 {
                return ptr::null_mut();
            }
            NativeParameterCache::new(P::parameter_schema(), P::parameter_roles)
                .map_or(ptr::null_mut(), |cache| {
                    Box::into_raw(Box::new(cache)).cast()
                })
        })
        .unwrap_or(ptr::null_mut())
    }
    unsafe extern "C" fn destroy(cache: *mut c_void) {
        if !cache.is_null() {
            drop(unsafe { Box::from_raw(cache.cast::<NativeParameterCache<N>>()) });
        }
    }
    pub(crate) unsafe fn get<'a>(cache: *const c_void) -> Option<&'a NativeParameterCache<N>> {
        if cache.is_null() || !aligned(cache.cast::<NativeParameterCache<N>>()) {
            None
        } else {
            unsafe { cache.cast::<NativeParameterCache<N>>().as_ref() }
        }
    }
    unsafe extern "C" fn info(
        cache: *const c_void,
        index: u32,
        output: *mut NativeParameterInfoV1,
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
    unsafe fn mapping(
        cache: *const c_void,
        id: u32,
        value: f32,
        output: *mut f32,
        normalize: bool,
    ) -> i32 {
        if output.is_null() || !aligned(output) {
            return 1;
        }
        let Some(cache) = (unsafe { Self::get(cache) }) else {
            return 1;
        };
        let value = if normalize {
            cache.normalize(ParameterId(id), value)
        } else {
            cache.denormalize(ParameterId(id), value)
        };
        let Some(value) = value else {
            return 1;
        };
        unsafe {
            output.write(value);
        }
        0
    }
    unsafe extern "C" fn normalize(
        cache: *const c_void,
        id: u32,
        value: f32,
        output: *mut f32,
    ) -> i32 {
        unsafe { Self::mapping(cache, id, value, output, true) }
    }
    unsafe extern "C" fn denormalize(
        cache: *const c_void,
        id: u32,
        value: f32,
        output: *mut f32,
    ) -> i32 {
        unsafe { Self::mapping(cache, id, value, output, false) }
    }
    unsafe extern "C" fn events(
        cache: *const c_void,
        input: *const NativeNormalizedParameterEventV1,
        count: u32,
        frames: u32,
        output: *mut NativeEventV1,
        capacity: u32,
    ) -> i32 {
        if count > 1024
            || capacity > 1024
            || count > capacity
            || frames > 65536
            || (count > 0 && (input.is_null() || !aligned(input)))
            || (capacity > 0 && (output.is_null() || !aligned(output)))
        {
            return 1;
        }
        let Some(cache) = (unsafe { Self::get(cache) }) else {
            return 1;
        };
        let input = if count == 0 {
            &[]
        } else {
            unsafe { slice::from_raw_parts(input, count as usize) }
        };
        let output = if capacity == 0 {
            &mut []
        } else {
            unsafe { slice::from_raw_parts_mut(output, capacity as usize) }
        };
        if cache.convert_events(input, frames as usize, output).is_ok() {
            0
        } else {
            1
        }
    }
}
/// Export an immutable cached parameter companion for a native facade pair.
#[macro_export]
macro_rules! export_native_cached_parameters_v1 {
    ($symbol:ident,$processor32:ty,$processor64:ty,$count:expr) => {
        #[unsafe(no_mangle)]
        pub extern "C" fn $symbol(sample_bits: u32) -> *const $crate::NativeCachedParameterEntryV1 {
            let entry = match sample_bits {
                32 => $crate::NativeCachedParameters::<$processor32, f32, $count>::entry(),
                64 => $crate::NativeCachedParameters::<$processor64, f64, $count>::entry(),
                _ => None,
            };
            entry.map_or(::std::ptr::null(), |entry| entry as *const _)
        }
    };
}
