//! Explicit opt-in live state handoff, available with 64-bit atomic mailboxes.
use crate::{
    NativeAudioParameterProcessor, NativeParameterEntryV1, NativeParameterValueV1,
    NativeParameters, NativeProcessor,
};
use ravet_foundation::{
    ParameterError, ParameterSchema, ParameterState, Processor, PublishError, Sample,
    SnapshotMailbox, WordSnapshot,
};
use std::{
    ffi::c_void,
    marker::PhantomData,
    panic::{AssertUnwindSafe, catch_unwind},
    ptr, slice,
    sync::atomic::{AtomicU64, Ordering::Relaxed},
};
/// Coherent publication identity/generation; generation wrap must not occur
/// during a read or conditional reflection. Identities must not wrap per library
/// lifetime. Values are in-process bookkeeping, not persisted product state.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct NativeStateRevisionV1 {
    pub instance: u64,
    pub generation: u64,
}
/// Successful apply result; changed is 1 for a newly accepted publication,
/// 0 when already accepted. Reserved is zero; failed calls leave output unchanged.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct NativeStateApplyV1 {
    pub revision: NativeStateRevisionV1,
    pub changed: u32,
    pub reserved: u32,
}
/// Independent thread-safe publication object and explicit audio-owner functions.
/// Codes: 0 success, 1 rejection, 2 panic/owner poison, 3 Busy, 4 Stale. One-attempt
/// reads/writes never spin; consumers retain previous accepted state on Busy and
/// schedule retry outside callbacks. Intermediate publications can coalesce.
///
/// Opaque publication/DSP pointers use exact paired table types. Create/drop and
/// byte decoding/serialization are control-only. Shared publication calls may run
/// concurrently; apply/reflect exclusively own prepared DSP. All foreign ranges
/// are live/aligned/disjoint; destroy only after every reader/writer/apply/reflect
/// ends. Loaded tables/library remain alive. No host scheduling/retirement implied.
#[repr(C)]
#[derive(Clone, Copy)]
pub struct NativeStateEntryV1 {
    pub version: u32,
    pub byte_size: u32,
    pub sample_bits: u32,
    pub parameter_count: u32,
    pub parameters_entry: extern "C" fn() -> *const NativeParameterEntryV1,
    pub create: unsafe extern "C" fn(*const u8, u32) -> *mut c_void,
    pub destroy: unsafe extern "C" fn(*mut c_void),
    pub publish:
        unsafe extern "C" fn(*const c_void, *const u8, u32, *mut NativeStateRevisionV1) -> i32,
    pub snapshot: unsafe extern "C" fn(
        *const c_void,
        *mut NativeParameterValueV1,
        u32,
        *mut NativeStateRevisionV1,
    ) -> i32,
    pub save_state:
        unsafe extern "C" fn(*const c_void, *mut u8, u32, *mut NativeStateRevisionV1) -> i32,
    pub apply: unsafe extern "C" fn(*mut c_void, *const c_void, *mut NativeStateApplyV1) -> i32,
    pub reflect:
        unsafe extern "C" fn(*mut c_void, *const c_void, *mut NativeStateRevisionV1) -> i32,
}
/// Native schema-bound publication implementation. It owns no processor; its
/// immutable schema is cached on control, with fixed-size atomic words. Consumers
/// explicitly call apply before process and conditional reflect afterward.
pub struct NativeState<P, S, const N: usize> {
    id: u64,
    schema: ParameterSchema<'static, N>,
    mailbox: SnapshotMailbox<N>,
    marker: PhantomData<fn() -> (P, S)>,
}
static NEXT_ID: AtomicU64 = AtomicU64::new(1);
fn aligned<T>(pointer: *const T) -> bool {
    (pointer as usize).is_multiple_of(align_of::<T>())
}
fn code(error: PublishError) -> i32 {
    match error {
        PublishError::Busy => 3,
        PublishError::Stale => 4,
    }
}
impl<
    P: Processor<S> + Default + Send + NativeAudioParameterProcessor<N> + 'static,
    S: Sample + Send + 'static,
    const N: usize,
> NativeState<P, S, N>
{
    const ENTRY: NativeStateEntryV1 = NativeStateEntryV1 {
        version: 1,
        byte_size: size_of::<NativeStateEntryV1>() as u32,
        sample_bits: (size_of::<S>() * 8) as u32,
        parameter_count: N as u32,
        parameters_entry: Self::parameters_entry,
        create: Self::create,
        destroy: Self::destroy,
        publish: Self::publish,
        snapshot: Self::snapshot,
        save_state: Self::save,
        apply: Self::apply,
        reflect: Self::reflect,
    };
    /// Validate the paired immutable schema on control. Invalid metadata returns
    /// None; no implicit opt-in is supplied for arbitrary parameter processors.
    pub fn entry() -> Option<&'static NativeStateEntryV1> {
        NativeParameters::<P, S, N>::entry().map(|_| &Self::ENTRY)
    }
    extern "C" fn parameters_entry() -> *const NativeParameterEntryV1 {
        NativeParameters::<P, S, N>::entry().map_or(ptr::null(), |table| table as *const _)
    }
    fn contain(call: impl FnOnce() -> i32) -> i32 {
        catch_unwind(AssertUnwindSafe(call)).unwrap_or(2)
    }
    fn validate(
        schema: ParameterSchema<'static, N>,
        state: ParameterState<'static, N>,
    ) -> Result<ParameterState<'static, N>, ParameterError> {
        let mut values = [0.; N];
        for (out, descriptor) in values.iter_mut().zip(schema.descriptors()) {
            *out = state
                .get(descriptor.id)
                .ok_or(ParameterError::WrongParameter(descriptor.id))?;
        }
        schema.state(values)
    }
    unsafe fn get<'a>(pointer: *const c_void) -> Option<&'a Self> {
        if pointer.is_null() || !aligned(pointer.cast::<Self>()) {
            None
        } else {
            unsafe { pointer.cast::<Self>().as_ref() }
        }
    }
    fn revision(&self, generation: u64) -> NativeStateRevisionV1 {
        NativeStateRevisionV1 {
            instance: self.id,
            generation,
        }
    }
    fn decode(&self, words: WordSnapshot<N>) -> Result<ParameterState<'static, N>, ParameterError> {
        self.schema.state(words.words.map(f32::from_bits))
    }
    unsafe extern "C" fn create(input: *const u8, length: u32) -> *mut c_void {
        catch_unwind(|| {
            if N > 1024 || length > 8208 {
                return ptr::null_mut();
            }
            let schema = P::parameter_schema();
            let state = if input.is_null() && length == 0 {
                schema.defaults()
            } else {
                if length > 0 && input.is_null() {
                    return ptr::null_mut();
                }
                let input = if length == 0 {
                    &[]
                } else {
                    unsafe { slice::from_raw_parts(input, length as usize) }
                };
                let Ok(state) = P::decode_parameter_snapshot(input)
                    .and_then(|state| Self::validate(schema, state))
                else {
                    return ptr::null_mut();
                };
                state
            };
            let id = NEXT_ID.fetch_add(1, Relaxed);
            if id == 0 {
                return ptr::null_mut();
            }
            Box::into_raw(Box::new(Self {
                id,
                schema,
                mailbox: SnapshotMailbox::new(state.values().map(f32::to_bits)),
                marker: PhantomData,
            }))
            .cast()
        })
        .unwrap_or(ptr::null_mut())
    }
    unsafe extern "C" fn destroy(publication: *mut c_void) {
        if !publication.is_null() {
            drop(unsafe { Box::from_raw(publication.cast::<Self>()) });
        }
    }
    unsafe extern "C" fn publish(
        publication: *const c_void,
        input: *const u8,
        length: u32,
        output: *mut NativeStateRevisionV1,
    ) -> i32 {
        Self::contain(|| {
            let Some(publication) = (unsafe { Self::get(publication) }) else {
                return 1;
            };
            if output.is_null()
                || !aligned(output)
                || length > 8208
                || (length > 0 && input.is_null())
            {
                return 1;
            }
            let input = if length == 0 {
                &[]
            } else {
                unsafe { slice::from_raw_parts(input, length as usize) }
            };
            let Ok(state) = P::decode_parameter_snapshot(input)
                .and_then(|state| Self::validate(publication.schema, state))
            else {
                return 1;
            };
            match publication
                .mailbox
                .try_publish(state.values().map(f32::to_bits))
            {
                Ok(generation) => {
                    unsafe { output.write(publication.revision(generation)) };
                    0
                }
                Err(error) => code(error),
            }
        })
    }
    unsafe extern "C" fn snapshot(
        publication: *const c_void,
        output: *mut NativeParameterValueV1,
        capacity: u32,
        revision: *mut NativeStateRevisionV1,
    ) -> i32 {
        Self::contain(|| {
            let Some(publication) = (unsafe { Self::get(publication) }) else {
                return 1;
            };
            if revision.is_null()
                || !aligned(revision)
                || (capacity as usize) < N
                || capacity > 1024
                || (N > 0 && (output.is_null() || !aligned(output)))
            {
                return 1;
            }
            let Some(snapshot) = publication.mailbox.try_read() else {
                return 3;
            };
            let Ok(state) = publication.decode(snapshot) else {
                return 1;
            };
            for (index, descriptor) in publication.schema.descriptors().iter().enumerate() {
                unsafe {
                    output.add(index).write(NativeParameterValueV1 {
                        id: descriptor.id.0,
                        value: state.values()[index],
                    })
                };
            }
            unsafe { revision.write(publication.revision(snapshot.generation)) };
            0
        })
    }
    unsafe extern "C" fn save(
        publication: *const c_void,
        output: *mut u8,
        capacity: u32,
        revision: *mut NativeStateRevisionV1,
    ) -> i32 {
        Self::contain(|| {
            let Some(publication) = (unsafe { Self::get(publication) }) else {
                return 1;
            };
            let bytes = 16 + 8 * N;
            if output.is_null()
                || (capacity as usize) < bytes
                || capacity > 8208
                || revision.is_null()
                || !aligned(revision)
            {
                return 1;
            }
            let Some(snapshot) = publication.mailbox.try_read() else {
                return 3;
            };
            let Ok(state) = publication.decode(snapshot) else {
                return 1;
            };
            if state
                .encode(P::STATE_PRODUCT, P::STATE_VERSION, unsafe {
                    slice::from_raw_parts_mut(output, bytes)
                })
                .is_err()
            {
                return 1;
            }
            unsafe { revision.write(publication.revision(snapshot.generation)) };
            0
        })
    }
    unsafe extern "C" fn apply(
        handle: *mut c_void,
        publication: *const c_void,
        output: *mut NativeStateApplyV1,
    ) -> i32 {
        NativeProcessor::<P, S>::control(handle, |owner| {
            if owner.poisoned {
                return 2;
            }
            if !owner.is_prepared() || output.is_null() || !aligned(output) {
                return 1;
            }
            let Some(publication) = (unsafe { Self::get(publication) }) else {
                return 1;
            };
            let Some(snapshot) = publication.mailbox.try_read() else {
                return 3;
            };
            let revision = publication.revision(snapshot.generation);
            let changed = owner.state_revision != Some((publication.id, snapshot.generation));
            if changed {
                let Ok(state) = publication.decode(snapshot) else {
                    return 1;
                };
                if owner
                    .processor
                    .apply_audio_parameter_snapshot(&state)
                    .is_err()
                {
                    return 1;
                }
                owner.state_revision = Some((publication.id, snapshot.generation));
            }
            unsafe {
                output.write(NativeStateApplyV1 {
                    revision,
                    changed: u32::from(changed),
                    reserved: 0,
                })
            };
            0
        })
    }
    unsafe extern "C" fn reflect(
        handle: *mut c_void,
        publication: *const c_void,
        output: *mut NativeStateRevisionV1,
    ) -> i32 {
        NativeProcessor::<P, S>::control(handle, |owner| {
            if owner.poisoned {
                return 2;
            }
            if !owner.is_prepared() || output.is_null() || !aligned(output) {
                return 1;
            }
            let Some(publication) = (unsafe { Self::get(publication) }) else {
                return 1;
            };
            let Some((id, generation)) = owner.state_revision else {
                return 4;
            };
            if id != publication.id {
                return 4;
            }
            let Some(current) = publication.mailbox.try_read() else {
                return 3;
            };
            if current.generation != generation {
                return 4;
            }
            let Ok(state) = Self::validate(
                publication.schema,
                owner.processor.audio_parameter_snapshot(),
            ) else {
                return 1;
            };
            let words = state.values().map(f32::to_bits);
            if words == current.words {
                unsafe { output.write(publication.revision(generation)) };
                return 0;
            }
            match publication.mailbox.try_publish_if(generation, words) {
                Ok(generation) => {
                    owner.state_revision = Some((id, generation));
                    unsafe { output.write(publication.revision(generation)) };
                    0
                }
                Err(error) => code(error),
            }
        })
    }
}
/// Export a live-state companion for explicit audio-parameter native facades.
#[macro_export]
macro_rules! export_native_state_v1 {
    ($symbol:ident,$processor32:ty,$processor64:ty,$count:expr) => {
        #[unsafe(no_mangle)]
        pub extern "C" fn $symbol(sample_bits: u32) -> *const $crate::NativeStateEntryV1 {
            let entry = match sample_bits {
                32 => $crate::NativeState::<$processor32, f32, $count>::entry(),
                64 => $crate::NativeState::<$processor64, f64, $count>::entry(),
                _ => None,
            };
            entry.map_or(::std::ptr::null(), |entry| entry as *const _)
        }
    };
}
