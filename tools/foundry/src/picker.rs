//! Private native directory selection boundary; called only from the UI thread.
use crate::{DirectoryRequest, Result};
use std::{
    ffi::{CStr, CString, c_char},
    path::PathBuf,
};
unsafe extern "C" {
    fn ravet_foundry_choose_directory(
        title: *const c_char,
        initial: *const c_char,
        probe: bool,
    ) -> *mut c_char;
    fn ravet_foundry_free_directory(path: *mut c_char);
}
pub(crate) fn choose(request: &DirectoryRequest, probe: bool) -> Result<Option<PathBuf>> {
    let title = CString::new(request.title())?;
    let initial = CString::new(
        request
            .initial_directory()
            .unwrap_or_default()
            .to_str()
            .ok_or("Directory must be UTF-8")?,
    )?;
    // SAFETY: native frontend calls on initialized Cocoa main thread; strings live
    // across the modal loop, and the result is owned until freed by its allocator.
    let selected =
        unsafe { ravet_foundry_choose_directory(title.as_ptr(), initial.as_ptr(), probe) };
    if selected.is_null() {
        return Ok(None);
    }
    let result = unsafe { CStr::from_ptr(selected) }
        .to_str()
        .map(PathBuf::from)
        .map_err(Into::into);
    unsafe { ravet_foundry_free_directory(selected) };
    result.map(Some)
}
