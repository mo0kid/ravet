//! Narrow ABI for Rust-owned files, HTTP and worker lifetimes.
//!
//! Native callers supply valid NUL-terminated UTF-8 paths/URLs, readable byte
//! slices, aligned writable output pointers and live opaque handles. Handles
//! must not be destroyed concurrently with submission or another destruction.
//! Every callback and callback destructor must be nonthrowing. Callback contexts
//! must remain valid until their destructor runs; task/timer functions always
//! consume context, including on rejection. All calls occur off audio threads.
//! See `include/RavetRuntime.h` for the native declarations.

/// Ensure this ABI crate is included in a consumer's Rust static library.
pub fn link() {}
pub mod system;

#[allow(unsafe_code)]
mod abi {
    use ravet_runtime::{
        files::{self, FileError},
        http::{self, HttpError, HttpOptions},
        tasks::{PeriodicTask, SerialExecutor, SubmitError},
    };
    use std::{
        ffi::{CStr, c_char, c_void},
        panic::{AssertUnwindSafe, catch_unwind},
        ptr, slice,
        time::Duration,
    };

    #[repr(C)]
    pub struct Bytes {
        pub data: *mut u8,
        pub len: usize,
    }
    fn error(error: FileError) -> u32 {
        match error {
            FileError::TooLarge => 7,
            FileError::InvalidLimit => 1,
            FileError::Io(_) => 2,
        }
    }
    unsafe fn text<'a>(value: *const c_char) -> Option<&'a str> {
        if value.is_null() {
            None
        } else {
            unsafe { CStr::from_ptr(value) }.to_str().ok()
        }
    }
    unsafe fn input<'a>(data: *const u8, len: usize) -> Option<&'a [u8]> {
        if len > isize::MAX as usize {
            return None;
        }
        if len == 0 {
            Some(&[])
        } else if data.is_null() {
            None
        } else {
            Some(unsafe { slice::from_raw_parts(data, len) })
        }
    }
    fn bytes(value: Vec<u8>) -> Bytes {
        if value.is_empty() {
            return Bytes {
                data: ptr::null_mut(),
                len: 0,
            };
        }
        let owned = value.into_boxed_slice();
        let len = owned.len();
        Bytes {
            data: Box::into_raw(owned).cast(),
            len,
        }
    }
    #[unsafe(no_mangle)]
    unsafe extern "C" fn ravet_runtime_clock_now(output: *mut i64) -> u32 {
        let Some(output) = (unsafe { output.as_mut() }) else {
            return 1;
        };
        *output = 0;
        let Some(now) = ravet_runtime::clock::now_milliseconds() else {
            return 2;
        };
        *output = now;
        0
    }
    #[unsafe(no_mangle)]
    unsafe extern "C" fn ravet_runtime_location(
        kind: u32,
        address: *const c_void,
        output: *mut Bytes,
    ) -> u32 {
        let Some(output) = (unsafe { output.as_mut() }) else {
            return 1;
        };
        *output = Bytes {
            data: ptr::null_mut(),
            len: 0,
        };
        catch_unwind(AssertUnwindSafe(|| {
            let path = match kind {
                0 => ravet_runtime::paths::home(),
                1 => ravet_runtime::paths::application_data(),
                2 => super::system::module_path(address),
                _ => return 1,
            };
            let Some(path) = path.and_then(|p| p.to_str().map(str::to_owned)) else {
                return 2;
            };
            *output = bytes(path.into_bytes());
            0
        }))
        .unwrap_or(2)
    }
    #[unsafe(no_mangle)]
    unsafe extern "C" fn ravet_runtime_hardware(kind: u32, output: *mut Bytes) -> u32 {
        let Some(output) = (unsafe { output.as_mut() }) else {
            return 1;
        };
        *output = Bytes {
            data: ptr::null_mut(),
            len: 0,
        };
        catch_unwind(AssertUnwindSafe(|| {
            let value = match kind {
                0 => super::system::hardware_uuid().map(String::into_bytes),
                1 => super::system::smbios_firmware(),
                _ => return 1,
            };
            let Some(value) = value else {
                return 2;
            };
            *output = bytes(value);
            0
        }))
        .unwrap_or(2)
    }
    #[unsafe(no_mangle)]
    unsafe extern "C" fn ravet_runtime_bytes_free(value: Bytes) {
        if !value.data.is_null() {
            drop(unsafe { Box::from_raw(ptr::slice_from_raw_parts_mut(value.data, value.len)) });
        }
    }
    #[unsafe(no_mangle)]
    unsafe extern "C" fn ravet_runtime_read(
        path: *const c_char,
        maximum: usize,
        output: *mut Bytes,
    ) -> u32 {
        let Some(output) = (unsafe { output.as_mut() }) else {
            return 1;
        };
        *output = Bytes {
            data: ptr::null_mut(),
            len: 0,
        };
        let Some(path) = (unsafe { text(path) }) else {
            return 1;
        };
        catch_unwind(AssertUnwindSafe(|| match files::read(path, maximum) {
            Ok(value) => {
                *output = bytes(value);
                0
            }
            Err(e) => error(e),
        }))
        .unwrap_or(2)
    }
    #[unsafe(no_mangle)]
    unsafe extern "C" fn ravet_runtime_write(
        path: *const c_char,
        data: *const u8,
        len: usize,
    ) -> u32 {
        let (Some(path), Some(data)) = (unsafe { text(path) }, unsafe { input(data, len) }) else {
            return 1;
        };
        catch_unwind(AssertUnwindSafe(|| {
            files::atomic_write(path, data).map_or_else(error, |()| 0)
        }))
        .unwrap_or(2)
    }
    #[unsafe(no_mangle)]
    unsafe extern "C" fn ravet_runtime_post_json(
        url: *const c_char,
        data: *const u8,
        len: usize,
        timeout_ms: u64,
        maximum: usize,
        user_agent: *const c_char,
        output: *mut Bytes,
        status: *mut u16,
    ) -> u32 {
        let (Some(output), Some(status)) = (unsafe { output.as_mut() }, unsafe { status.as_mut() })
        else {
            return 1;
        };
        *output = Bytes {
            data: ptr::null_mut(),
            len: 0,
        };
        *status = 0;
        let (Some(url), Some(body), Some(user_agent)) =
            (unsafe { text(url) }, unsafe { input(data, len) }, unsafe {
                text(user_agent)
            })
        else {
            return 1;
        };
        catch_unwind(AssertUnwindSafe(|| {
            let options = HttpOptions {
                timeout: Duration::from_millis(timeout_ms),
                maximum_response_bytes: maximum,
                user_agent: user_agent.into(),
            };
            match http::post_json(url, body, &options) {
                Ok(value) => {
                    *status = value.status;
                    *output = bytes(value.body);
                    0
                }
                Err(HttpError::InvalidRequest) => 1,
                Err(HttpError::Timeout) => 5,
                Err(HttpError::TooLarge) => 7,
                Err(HttpError::Transport) => 6,
            }
        }))
        .unwrap_or(6)
    }
    struct Context {
        ptr: usize,
        call: Option<unsafe extern "C" fn(*mut c_void)>,
        destroy: Option<unsafe extern "C" fn(*mut c_void)>,
    }
    impl Context {
        fn invoke(&self) {
            if let Some(call) = self.call {
                unsafe { call(self.ptr as *mut c_void) };
            }
        }
    }
    impl Drop for Context {
        fn drop(&mut self) {
            if let Some(destroy) = self.destroy {
                unsafe { destroy(self.ptr as *mut c_void) };
            }
        }
    }
    #[unsafe(no_mangle)]
    extern "C" fn ravet_runtime_executor_create(capacity: usize) -> *mut c_void {
        catch_unwind(|| {
            SerialExecutor::new(capacity)
                .ok()
                .map_or(ptr::null_mut(), |value| {
                    Box::into_raw(Box::new(value)).cast()
                })
        })
        .unwrap_or(ptr::null_mut())
    }
    #[unsafe(no_mangle)]
    unsafe extern "C" fn ravet_runtime_executor_destroy(handle: *mut c_void) {
        if !handle.is_null() {
            drop(unsafe { Box::from_raw(handle.cast::<SerialExecutor>()) });
        }
    }
    #[unsafe(no_mangle)]
    unsafe extern "C" fn ravet_runtime_executor_submit(
        handle: *mut c_void,
        call: Option<unsafe extern "C" fn(*mut c_void)>,
        destroy: Option<unsafe extern "C" fn(*mut c_void)>,
        context: *mut c_void,
    ) -> u32 {
        let context = Context {
            ptr: context as usize,
            call,
            destroy,
        };
        let Some(executor) = (unsafe { handle.cast::<SerialExecutor>().as_ref() }) else {
            return 1;
        };
        if call.is_none() {
            return 1;
        }
        catch_unwind(AssertUnwindSafe(|| {
            executor.submit(move || context.invoke()).map_or_else(
                |e| match e {
                    SubmitError::Full => 3,
                    SubmitError::Stopped => 4,
                },
                |()| 0,
            )
        }))
        .unwrap_or(4)
    }
    #[unsafe(no_mangle)]
    unsafe extern "C" fn ravet_runtime_timer_create(
        interval_ms: u64,
        call: Option<unsafe extern "C" fn(*mut c_void)>,
        destroy: Option<unsafe extern "C" fn(*mut c_void)>,
        context: *mut c_void,
    ) -> *mut c_void {
        let context = Context {
            ptr: context as usize,
            call,
            destroy,
        };
        if call.is_none() {
            return ptr::null_mut();
        }
        catch_unwind(AssertUnwindSafe(|| {
            PeriodicTask::start(Duration::from_millis(interval_ms), move || context.invoke())
                .ok()
                .map_or(ptr::null_mut(), |value| {
                    Box::into_raw(Box::new(value)).cast()
                })
        }))
        .unwrap_or(ptr::null_mut())
    }
    #[unsafe(no_mangle)]
    unsafe extern "C" fn ravet_runtime_timer_destroy(handle: *mut c_void) {
        if !handle.is_null() {
            drop(unsafe { Box::from_raw(handle.cast::<PeriodicTask>()) });
        }
    }

    #[cfg(test)]
    mod tests {
        use super::*;
        use std::{
            ffi::CString,
            sync::{
                Arc,
                atomic::{AtomicUsize, Ordering},
                mpsc,
            },
        };
        struct Probe {
            calls: Arc<AtomicUsize>,
            drops: Arc<AtomicUsize>,
            started: Option<mpsc::Sender<()>>,
            release: Option<mpsc::Receiver<()>>,
        }
        unsafe extern "C" fn invoke(context: *mut c_void) {
            let probe = unsafe { &mut *context.cast::<Probe>() };
            probe.calls.fetch_add(1, Ordering::SeqCst);
            if let Some(started) = probe.started.take() {
                started.send(()).unwrap();
            }
            if let Some(release) = probe.release.take() {
                release.recv().unwrap();
            }
        }
        unsafe extern "C" fn destroy(context: *mut c_void) {
            let probe = unsafe { Box::from_raw(context.cast::<Probe>()) };
            probe.drops.fetch_add(1, Ordering::SeqCst);
        }
        #[test]
        fn abi_consumes_each_context_once_including_queue_rejection() {
            let calls = Arc::new(AtomicUsize::new(0));
            let drops = Arc::new(AtomicUsize::new(0));
            let (started, has_started) = mpsc::channel();
            let (release, gate) = mpsc::channel();
            let make = || {
                Box::into_raw(Box::new(Probe {
                    calls: calls.clone(),
                    drops: drops.clone(),
                    started: None,
                    release: None,
                }))
                .cast()
            };
            unsafe {
                assert_eq!(
                    ravet_runtime_executor_submit(
                        ptr::null_mut(),
                        Some(invoke),
                        Some(destroy),
                        make()
                    ),
                    1
                );
                assert_eq!(drops.load(Ordering::SeqCst), 1);
                let queue = ravet_runtime_executor_create(1);
                assert!(!queue.is_null());
                let gate = Box::into_raw(Box::new(Probe {
                    calls: calls.clone(),
                    drops: drops.clone(),
                    started: Some(started),
                    release: Some(gate),
                }))
                .cast();
                assert_eq!(
                    ravet_runtime_executor_submit(queue, Some(invoke), Some(destroy), gate),
                    0
                );
                has_started.recv_timeout(Duration::from_secs(2)).unwrap();
                assert_eq!(
                    ravet_runtime_executor_submit(queue, Some(invoke), Some(destroy), make()),
                    0
                );
                assert_eq!(
                    ravet_runtime_executor_submit(queue, Some(invoke), Some(destroy), make()),
                    3
                );
                release.send(()).unwrap();
                ravet_runtime_executor_destroy(queue);
            }
            assert_eq!(calls.load(Ordering::SeqCst), 2);
            assert_eq!(drops.load(Ordering::SeqCst), 4);
        }
        #[test]
        fn abi_outputs_are_cleared_on_failure_and_locations_do_not_expose_identity() {
            let mut output = Bytes {
                data: ptr::null_mut(),
                len: 0,
            };
            let mut status = 999;
            unsafe {
                assert_eq!(
                    ravet_runtime_read(c"/missing/ravet-runtime-test".as_ptr(), 5, &mut output),
                    2
                );
                assert!(output.data.is_null());
                assert_eq!(output.len, 0);
                assert_eq!(
                    ravet_runtime_write(
                        c"/unused/ravet-runtime-test".as_ptr(),
                        ptr::null(),
                        usize::MAX
                    ),
                    1
                );
                assert_eq!(
                    ravet_runtime_post_json(
                        c"http://localhost/".as_ptr(),
                        ptr::null(),
                        0,
                        100,
                        100,
                        c"RAVET test".as_ptr(),
                        &mut output,
                        &mut status
                    ),
                    1
                );
                assert_eq!(status, 0);
                assert!(output.data.is_null());
                assert_eq!(
                    ravet_runtime_location(2, ravet_runtime_read as *const c_void, &mut output),
                    0
                );
                assert!(!output.data.is_null());
                assert!(output.len > 0);
                ravet_runtime_bytes_free(output);
                let home =
                    std::env::temp_dir().join(format!("ravet-abi-{}.bin", std::process::id()));
                let path = CString::new(home.to_str().unwrap()).unwrap();
                assert_eq!(ravet_runtime_write(path.as_ptr(), b"a\0b".as_ptr(), 3), 0);
                let mut output = Bytes {
                    data: ptr::null_mut(),
                    len: 0,
                };
                assert_eq!(ravet_runtime_read(path.as_ptr(), 3, &mut output), 0);
                assert_eq!(slice::from_raw_parts(output.data, output.len), b"a\0b");
                ravet_runtime_bytes_free(output);
                std::fs::remove_file(home).unwrap();
            }
        }
    }
}
