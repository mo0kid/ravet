// Native Windows/Linux launchers own this context on their UI thread.
use crate::{Controller, DirectoryRequest};
use std::{
    ffi::{CStr, c_char, c_void},
    panic::{AssertUnwindSafe, catch_unwind},
};

#[unsafe(no_mangle)]
unsafe extern "C" fn ravet_foundry_browse_request(
    message: *const c_char,
    field: *mut c_char,
    initial: *mut c_char,
    title: *mut c_char,
) -> bool {
    catch_unwind(AssertUnwindSafe(|| {
        if message.is_null() || field.is_null() || initial.is_null() || title.is_null() {
            return false;
        }
        // SAFETY: launcher passes the bounded native message and buffers of
        // 64, 4097 and 128 bytes respectively, owned for this call.
        let Ok(text) = unsafe { CStr::from_ptr(message) }.to_str() else {
            return false;
        };
        let Ok(Some(request)) = DirectoryRequest::from_message(text) else {
            return false;
        };
        let initial_path = request.initial_directory().unwrap_or_default();
        let Some(path) = initial_path.to_str() else {
            return false;
        };
        for (value, output, capacity) in [
            (request.field.as_str(), field, 64),
            (path, initial, 4097),
            (request.title(), title, 128),
        ] {
            if value.len() >= capacity {
                return false;
            }
            unsafe {
                std::ptr::copy_nonoverlapping(value.as_ptr(), output.cast(), value.len());
                *output.add(value.len()) = 0;
            }
        }
        true
    }))
    .unwrap_or(false)
}
#[unsafe(no_mangle)]
unsafe extern "C" fn ravet_foundry_directory_selected(
    context: *mut c_void,
    field: *const c_char,
    path: *const c_char,
) {
    let _ = catch_unwind(AssertUnwindSafe(|| {
        if context.is_null() || field.is_null() || path.is_null() {
            return;
        }
        // SAFETY: UI-thread launcher retains the controller and both NUL strings.
        let Ok(field) = unsafe { CStr::from_ptr(field) }.to_str() else {
            return;
        };
        let Ok(path) = unsafe { CStr::from_ptr(path) }.to_str() else {
            return;
        };
        let request = DirectoryRequest {
            field: field.into(),
            initial: Default::default(),
        };
        let _ = unsafe { &mut *context.cast::<Controller>() }
            .directory_selected(&request, Some(path.into()));
    }));
}
#[unsafe(no_mangle)]
extern "C" fn ravet_foundry_create() -> *mut c_void {
    Box::into_raw(Box::new(Controller::default())).cast()
}
#[unsafe(no_mangle)]
unsafe extern "C" fn ravet_foundry_message(context: *mut c_void, message: *const c_char) {
    let _ = catch_unwind(AssertUnwindSafe(|| {
        if context.is_null() || message.is_null() {
            return;
        }
        // SAFETY: native launcher retains a unique live controller and counted
        // native backend supplies a bounded NUL-terminated UTF-8 message.
        let controller = unsafe { &mut *context.cast::<Controller>() };
        if let Ok(text) = unsafe { CStr::from_ptr(message) }.to_str() {
            let _ = controller.message(text);
        }
    }));
}
#[unsafe(no_mangle)]
unsafe extern "C" fn ravet_foundry_snapshot(
    context: *mut c_void,
    output: *mut c_char,
    capacity: usize,
) -> usize {
    catch_unwind(AssertUnwindSafe(|| {
        if context.is_null() || output.is_null() {
            return 0;
        }
        // SAFETY: UI-thread unique context and caller-owned capacity-byte output.
        let text = serde_json::to_string(&unsafe { &mut *context.cast::<Controller>() }.snapshot())
            .unwrap_or_default();
        if text.len() >= capacity || text.len() > 65_536 {
            return 0;
        }
        unsafe {
            std::ptr::copy_nonoverlapping(text.as_ptr(), output.cast(), text.len());
        }
        text.len()
    }))
    .unwrap_or(0)
}
#[unsafe(no_mangle)]
unsafe extern "C" fn ravet_foundry_destroy(context: *mut c_void) {
    if !context.is_null() {
        // SAFETY: native editor is destroyed before this unique controller.
        drop(unsafe { Box::from_raw(context.cast::<Controller>()) });
    }
}
#[unsafe(no_mangle)]
unsafe extern "C" fn ravet_foundry_document(output: *mut c_char, capacity: usize) -> usize {
    let text = crate::document();
    if output.is_null() || text.len() >= capacity {
        return text.len() + 1;
    }
    // SAFETY: caller supplies capacity writable bytes. Include a terminating NUL.
    unsafe {
        std::ptr::copy_nonoverlapping(text.as_ptr(), output.cast(), text.len());
        *output.add(text.len()) = 0;
    }
    text.len()
}
