//! Audited opaque handle and guarded callback boundary; no portable raw pointers.
use super::*;
use std::{
    ffi::{CStr, c_char, c_void},
    ptr::NonNull,
};
type Receive = extern "C" fn(*mut c_void, i32, u64, u32, *const u32);
type Release = extern "C" fn(*mut c_void);
unsafe extern "C" {
    fn ravet_cm_input(
        client: *const c_char,
        port: *const c_char,
        protocol: i32,
        source: i32,
        is_virtual: i32,
        context: *mut c_void,
        receive: Receive,
        release: Release,
        error: *mut i32,
    ) -> *mut c_void;
    fn ravet_cm_output(client: *const c_char, destination: i32, error: *mut i32) -> *mut c_void;
    fn ravet_cm_source(
        client: *const c_char,
        name: *const c_char,
        protocol: i32,
        error: *mut i32,
    ) -> *mut c_void;
    fn ravet_cm_close(handle: *mut c_void);
    fn ravet_cm_virtual_id(handle: *mut c_void, id: *mut i32) -> i32;
    fn ravet_cm_overlaps(handle: *mut c_void) -> u64;
    fn ravet_cm_send(
        handle: *mut c_void,
        protocol: i32,
        timestamp: u64,
        count: u32,
        words: *const u32,
    ) -> i32;
    fn ravet_cm_count(input: i32) -> u32;
    fn ravet_cm_info(input: i32, index: u32, id: *mut i32, name: *mut c_char, capacity: u32)
    -> i32;
}
struct Context {
    producer: PreparedUmpInput,
    protocol: UmpProtocol,
    diagnostics: Arc<Diagnostics>,
}
extern "C" fn receive(
    context: *mut c_void,
    native_protocol: i32,
    timestamp: u64,
    count: u32,
    words: *const u32,
) {
    // SAFETY: the native entry gate admits one callback, holds context ownership,
    // and close waits for this entry to finish before control-thread release.
    let context = unsafe { &mut *context.cast::<Context>() };
    if native_protocol != protocol(context.protocol)
        || count > 4096
        || (count != 0 && words.is_null())
    {
        context
            .diagnostics
            .invalid_metadata
            .fetch_add(1, Ordering::Relaxed);
        return;
    }
    // SAFETY: CoreMIDI supplies live numeric words for this callback, native
    // preflight bounds count; empty slices never dereference the native pointer.
    let words = if count == 0 {
        &[]
    } else {
        unsafe { std::slice::from_raw_parts(words, count as usize) }
    };
    let _ = context
        .producer
        .push_words(timestamp, context.protocol, words);
}
extern "C" fn release(context: *mut c_void) {
    // SAFETY: native input consumes one Box, releases exactly once on failure or
    // gated control-thread teardown, after the last permitted Rust callback.
    drop(unsafe { Box::from_raw(context.cast::<Context>()) });
}
pub(super) struct Port(NonNull<c_void>);
impl Port {
    #[allow(clippy::too_many_arguments)]
    pub(super) fn input(
        client: &CStr,
        name: &CStr,
        source: i32,
        is_virtual: bool,
        selected: UmpProtocol,
        producer: PreparedUmpInput,
        diagnostics: Arc<Diagnostics>,
    ) -> Result<Self, CoreMidiError> {
        let context = Box::new(Context {
            producer,
            protocol: selected,
            diagnostics,
        });
        // The foreign callback may run on a CoreMIDI thread; keep that boundary
        // checked if fields are added to Context in a later change.
        fn assert_send<T: Send>(_: &T) {}
        assert_send(context.as_ref());
        let context = Box::into_raw(context).cast::<c_void>();
        let mut error = 0;
        // SAFETY: names live through construction. Native always consumes the
        // context and pairs successful handle ownership with release on Drop.
        let raw = unsafe {
            ravet_cm_input(
                client.as_ptr(),
                name.as_ptr(),
                protocol(selected),
                source,
                i32::from(is_virtual),
                context,
                receive,
                release,
                &mut error,
            )
        };
        NonNull::new(raw)
            .map(Self)
            .ok_or(CoreMidiError::Native(error))
    }
    pub(super) fn output(client: &CStr, destination: i32) -> Result<Self, CoreMidiError> {
        let mut error = 0;
        // SAFETY: synchronous constructor borrows a valid terminated name and
        // returns uniquely owned native storage, with no callbacks/context.
        let raw = unsafe { ravet_cm_output(client.as_ptr(), destination, &mut error) };
        NonNull::new(raw)
            .map(Self)
            .ok_or(CoreMidiError::Native(error))
    }
    pub(super) fn source(
        client: &CStr,
        name: &CStr,
        selected: UmpProtocol,
    ) -> Result<Self, CoreMidiError> {
        let mut error = 0;
        // SAFETY: synchronous construction borrows terminated names and returns
        // a uniquely owned endpoint with no Rust callbacks or borrowed context.
        let raw = unsafe {
            ravet_cm_source(
                client.as_ptr(),
                name.as_ptr(),
                protocol(selected),
                &mut error,
            )
        };
        NonNull::new(raw)
            .map(Self)
            .ok_or(CoreMidiError::Native(error))
    }
    pub(super) fn virtual_id(&self) -> Result<i32, CoreMidiError> {
        let mut id = 0;
        // SAFETY: handle remains live for this synchronous property read.
        status(unsafe { ravet_cm_virtual_id(self.0.as_ptr(), &mut id) })?;
        Ok(id)
    }
    pub(super) fn overlaps(&self) -> u64 {
        // SAFETY: handle owns the callback state; native access is atomic.
        unsafe { ravet_cm_overlaps(self.0.as_ptr()) }
    }
    pub(super) fn send(
        &mut self,
        selected: UmpProtocol,
        timestamp: u64,
        packet: UmpPacket,
    ) -> Result<(), CoreMidiError> {
        // SAFETY: complete <=4 numeric words live until synchronous send returns.
        status(unsafe {
            ravet_cm_send(
                self.0.as_ptr(),
                protocol(selected),
                timestamp,
                packet.as_words().len() as u32,
                packet.as_words().as_ptr(),
            )
        })
    }
}
impl Drop for Port {
    fn drop(&mut self) {
        // SAFETY: unique non-cloneable owner destroys one live handle. Native close
        // gates callback entry and waits before releasing its consumed Rust context.
        unsafe { ravet_cm_close(self.0.as_ptr()) }
    }
}
fn status(value: i32) -> Result<(), CoreMidiError> {
    if value == 0 {
        Ok(())
    } else {
        Err(CoreMidiError::Native(value))
    }
}
pub(super) fn endpoints(input: bool) -> Result<Vec<CoreMidiEndpoint>, CoreMidiError> {
    // SAFETY: enumeration has no borrowed foreign buffers/callbacks.
    let count = unsafe { ravet_cm_count(i32::from(input)) };
    if count > 4096 {
        return Err(CoreMidiError::Capacity);
    }
    let mut endpoints = Vec::with_capacity(count as usize);
    for index in 0..count {
        let mut id = 0;
        let mut bytes = [0u8; 1024];
        // SAFETY: native receives writable exact-sized ID/name storage. Failure
        // rejects the whole snapshot, including concurrent endpoint removal.
        status(unsafe {
            ravet_cm_info(
                i32::from(input),
                index,
                &mut id,
                bytes.as_mut_ptr().cast(),
                bytes.len() as u32,
            )
        })?;
        let end = bytes
            .iter()
            .position(|b| *b == 0)
            .ok_or(CoreMidiError::Name)?;
        let name = std::str::from_utf8(&bytes[..end])
            .map_err(|_| CoreMidiError::Name)?
            .to_owned();
        endpoints.push(CoreMidiEndpoint { id, name });
    }
    Ok(endpoints)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn callback_boundary_preserves_words_rejects_metadata_and_has_no_heap() {
        let (producer, mut receiver) = PreparedUmpInput::new(2).unwrap();
        let diagnostics = Arc::new(Diagnostics::default());
        let mut context = Context {
            producer,
            protocol: UmpProtocol::Midi2,
            diagnostics: Arc::clone(&diagnostics),
        };
        let ptr = (&mut context as *mut Context).cast::<c_void>();
        let words = [0x42913c00, 0x81230000];
        let counts = allocation_counter::measure(|| {
            for timestamp in 0..10_000 {
                receive(ptr, 2, timestamp, 2, words.as_ptr());
                let packet = receiver.pop().unwrap();
                assert_eq!(packet.packet.as_words(), words);
                assert_eq!(packet.timestamp, timestamp);
                receive(ptr, 1, 0, 2, words.as_ptr());
                receive(ptr, 2, 0, 2, std::ptr::null());
                receive(ptr, 2, 0, 4097, words.as_ptr());
                assert_eq!(receiver.pop(), None);
            }
        });
        assert_eq!(counts.count_total, 0);
        assert_eq!(counts.count_current, 0);
        assert_eq!(diagnostics.invalid_metadata.load(Ordering::Relaxed), 30_000);
    }
}
