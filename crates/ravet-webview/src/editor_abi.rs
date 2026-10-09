//! Audited UI-only native ownership ABI; see include/RavetWebEditor.h.
use crate::{Callbacks, WebView, WebViewError, is_main_thread};
use std::{
    ffi::c_void,
    panic::{AssertUnwindSafe, catch_unwind},
    ptr,
    ptr::NonNull,
    slice,
};
#[repr(C)]
pub struct NativeCallbacks {
    context: *mut c_void,
    message: Option<extern "C" fn(*mut c_void, *const u8, usize)>,
    snapshot: Option<extern "C" fn(*mut c_void, *mut u8, usize) -> usize>,
    release: Option<extern "C" fn(*mut c_void)>,
}
struct ForeignCallbacks {
    native: NativeCallbacks,
    scratch: Vec<u8>,
}
impl Drop for ForeignCallbacks {
    fn drop(&mut self) {
        if let Some(release) = self.native.release {
            release(self.native.context);
        }
    }
}
impl Callbacks for ForeignCallbacks {
    fn message(&mut self, json: &str) {
        if let Some(message) = self.native.message {
            message(self.native.context, json.as_ptr(), json.len());
        }
    }
    fn snapshot(&mut self) -> Option<String> {
        let callback = self.native.snapshot?;
        let count = callback(
            self.native.context,
            self.scratch.as_mut_ptr(),
            self.scratch.len(),
        );
        if count == 0
            || count > 65536
            || count > self.scratch.len()
            || self.scratch[..count].contains(&0)
        {
            return None;
        }
        std::str::from_utf8(&self.scratch[..count])
            .ok()
            .map(str::to_owned)
    }
}
pub struct Editor(WebView<'static>);
fn error(value: WebViewError) -> u32 {
    match value {
        WebViewError::InvalidInput => 1,
        WebViewError::WrongThread => 2,
        WebViewError::CreationFailed => 3,
    }
}
/// # Safety
/// Foreign pointers, unique handles, buffers, callbacks and UI ownership must
/// satisfy the contract in `include/RavetWebEditor.h`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn ravet_editor_create_html(
    html: *const u8,
    length: usize,
    width: f64,
    height: f64,
    native: NativeCallbacks,
    status: *mut u32,
) -> *mut Editor {
    // Construct ownership before validation so every failure consumes context.
    let callbacks = ForeignCallbacks {
        native,
        scratch: Vec::new(),
    };
    let Some(status) = (unsafe { status.as_mut() }) else {
        return ptr::null_mut();
    };
    *status = 1;
    let result = catch_unwind(AssertUnwindSafe(|| {
        if !is_main_thread() {
            return Err(2);
        }
        if html.is_null()
            || length == 0
            || length > 1_048_576
            || !width.is_finite()
            || !height.is_finite()
            || width <= 0.
            || height <= 0.
            || width > 16384.
            || height > 16384.
        {
            return Err(1);
        }
        // SAFETY: caller provides live readable bytes; fixed bounds precede slice.
        let html = std::str::from_utf8(unsafe { slice::from_raw_parts(html, length) })
            .map_err(|_| 1u32)?;
        let mut callbacks = callbacks;
        callbacks.scratch = vec![0; 65536];
        WebView::from_html(html, width, height, callbacks)
            .map(|view| Box::into_raw(Box::new(Editor(view))))
            .map_err(error)
    }));
    match result {
        Ok(Ok(handle)) => {
            *status = 0;
            handle
        }
        Ok(Err(code)) => {
            *status = code;
            ptr::null_mut()
        }
        Err(_) => {
            *status = 3;
            ptr::null_mut()
        }
    }
}
unsafe fn use_editor(
    handle: *mut Editor,
    operation: impl FnOnce(&WebView<'static>) -> bool,
) -> u32 {
    if !is_main_thread() {
        return 2;
    }
    let Some(handle) = (unsafe { handle.as_ref() }) else {
        return 1;
    };
    match catch_unwind(AssertUnwindSafe(|| operation(&handle.0))) {
        Ok(true) => 0,
        Ok(false) => 1,
        Err(_) => 3,
    }
}
/// # Safety
/// Foreign pointers, unique handles, buffers, callbacks and UI ownership must
/// satisfy the contract in `include/RavetWebEditor.h`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn ravet_editor_attach(handle: *mut Editor, parent: *mut c_void) -> u32 {
    unsafe {
        use_editor(handle, |view| {
            let Some(parent) = NonNull::new(parent) else {
                return false;
            };
            view.attach(parent);
            true
        })
    }
}
/// # Safety
/// Foreign pointers, unique handles, buffers, callbacks and UI ownership must
/// satisfy the contract in `include/RavetWebEditor.h`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn ravet_editor_detach(handle: *mut Editor) -> u32 {
    unsafe {
        use_editor(handle, |view| {
            view.detach();
            true
        })
    }
}
/// # Safety
/// Foreign pointers, unique handles, buffers, callbacks and UI ownership must
/// satisfy the contract in `include/RavetWebEditor.h`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn ravet_editor_set_size(
    handle: *mut Editor,
    width: f64,
    height: f64,
) -> u32 {
    unsafe { use_editor(handle, |view| view.set_size(width, height)) }
}
/// # Safety
/// Foreign pointers, unique handles, buffers, callbacks and UI ownership must
/// satisfy the contract in `include/RavetWebEditor.h`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn ravet_editor_set_visible(handle: *mut Editor, visible: u32) -> u32 {
    unsafe {
        use_editor(handle, |view| {
            if visible > 1 {
                return false;
            }
            view.set_visible(visible == 1);
            true
        })
    }
}
/// # Safety
/// Foreign pointers, unique handles, buffers, callbacks and UI ownership must
/// satisfy the contract in `include/RavetWebEditor.h`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn ravet_editor_set_background(
    handle: *mut Editor,
    red: f64,
    green: f64,
    blue: f64,
) -> u32 {
    unsafe { use_editor(handle, |view| view.set_background(red, green, blue)) }
}
/// # Safety
/// Foreign pointers, unique handles, buffers, callbacks and UI ownership must
/// satisfy the contract in `include/RavetWebEditor.h`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn ravet_editor_destroy(handle: *mut Editor) -> u32 {
    if handle.is_null() {
        return 0;
    }
    if !is_main_thread() {
        return 2;
    }
    // SAFETY: caller transfers one unique live handle on the main thread; callbacks
    // are retained by WebView until entry completes, including self-destruction.
    catch_unwind(AssertUnwindSafe(|| drop(unsafe { Box::from_raw(handle) })))
        .map(|_| 0)
        .unwrap_or(3)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::Cell;
    struct Fixture {
        mode: u8,
        releases: Cell<usize>,
    }
    extern "C" fn release(raw: *mut c_void) {
        let fixture = unsafe { &*raw.cast::<Fixture>() };
        fixture.releases.set(fixture.releases.get() + 1);
    }
    extern "C" fn snapshot(raw: *mut c_void, output: *mut u8, capacity: usize) -> usize {
        let fixture = unsafe { &*raw.cast::<Fixture>() };
        let bytes: &[u8] = match fixture.mode {
            0 => b"",
            1 => b"{\"value\":3}",
            2 => &[0xff],
            3 => b"{}\0",
            _ => return capacity + 1,
        };
        unsafe {
            ptr::copy_nonoverlapping(bytes.as_ptr(), output, bytes.len());
        }
        bytes.len()
    }
    fn native(fixture: &mut Fixture) -> NativeCallbacks {
        NativeCallbacks {
            context: (fixture as *mut Fixture).cast(),
            message: None,
            snapshot: Some(snapshot),
            release: Some(release),
        }
    }
    #[test]
    fn foreign_snapshot_bounds_utf8_and_nul_validation_preserve_context_ownership() {
        for mode in 0..5 {
            let mut fixture = Fixture {
                mode,
                releases: Cell::new(0),
            };
            let mut callbacks = ForeignCallbacks {
                native: native(&mut fixture),
                scratch: vec![0; 65536],
            };
            let actual = callbacks.snapshot();
            assert_eq!(
                actual.as_deref(),
                if mode == 1 {
                    Some("{\"value\":3}")
                } else {
                    None
                }
            );
            assert_eq!(fixture.releases.get(), 0);
            drop(callbacks);
            assert_eq!(fixture.releases.get(), 1);
        }
    }
    #[test]
    fn failed_creation_consumes_foreign_context_even_without_status_storage() {
        let mut fixture = Fixture {
            mode: 0,
            releases: Cell::new(0),
        };
        let handle = unsafe {
            ravet_editor_create_html(
                ptr::null(),
                0,
                480.,
                390.,
                native(&mut fixture),
                ptr::null_mut(),
            )
        };
        assert!(handle.is_null());
        assert_eq!(fixture.releases.get(), 1);
        let mut status = 99;
        let handle = unsafe {
            ravet_editor_create_html(
                ptr::null(),
                0,
                480.,
                390.,
                native(&mut fixture),
                &mut status,
            )
        };
        assert!(handle.is_null());
        assert!(status == 1 || status == 2);
        assert_eq!(fixture.releases.get(), 2);
    }
}
