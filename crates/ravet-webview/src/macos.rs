use std::{
    cell::RefCell,
    ffi::{CStr, CString, c_char, c_void},
    marker::PhantomData,
    panic::{AssertUnwindSafe, catch_unwind},
    ptr::NonNull,
    rc::Rc,
};
unsafe extern "C" {
    fn ravet_web_is_main_thread() -> bool;
    fn ravet_web_create_html(
        html: *const c_char,
        width: f64,
        height: f64,
        context: *mut c_void,
        message: extern "C" fn(*mut c_void, *const c_char),
        snapshot: extern "C" fn(*mut c_void, *mut c_char, usize) -> usize,
    ) -> *mut c_void;
    fn ravet_web_set_background(view: *mut c_void, r: f64, g: f64, b: f64) -> bool;
    fn ravet_web_attach(view: *mut c_void, parent: *mut c_void);
    fn ravet_web_detach(view: *mut c_void);
    fn ravet_web_set_size(view: *mut c_void, width: f64, height: f64) -> bool;
    fn ravet_web_set_visible(view: *mut c_void, visible: bool);
    fn ravet_web_destroy(view: *mut c_void);
}
/// Query the Cocoa UI thread without creating a view. All native editor ownership
/// operations belong to that thread; this predicate does not initialize AppKit.
pub fn is_main_thread() -> bool {
    // SAFETY: native predicate owns no borrowed buffers or callbacks.
    unsafe { ravet_web_is_main_thread() }
}
/// Owned main-thread callback state. May allocate; never called from audio.
/// Native messages are bounded UTF-8 JSON. Snapshots must be object/array JSON.
/// Panics are caught at the FFI boundary; reentrant callbacks are skipped.
pub trait Callbacks {
    fn message(&mut self, json: &str);
    fn snapshot(&mut self) -> Option<String>;
}
/// Creation fails without owning a native view.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WebViewError {
    WrongThread,
    InvalidInput,
    CreationFailed,
}
impl std::fmt::Display for WebViewError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "web editor error: {self:?}")
    }
}
impl std::error::Error for WebViewError {}
struct Context<'a> {
    callbacks: RefCell<Box<dyn Callbacks + 'a>>,
}

// SAFETY: native code invokes callbacks only synchronously on the main thread,
// while the view holds one Rc. Clone at entry permits destruction from a callback.
unsafe fn retain_context<'a>(raw: *mut c_void) -> Rc<Context<'a>> {
    let raw = raw.cast::<Context<'a>>();
    unsafe {
        Rc::increment_strong_count(raw);
        Rc::from_raw(raw)
    }
}
extern "C" fn message(raw: *mut c_void, json: *const c_char) {
    let _ = catch_unwind(AssertUnwindSafe(|| {
        // SAFETY: backend supplies live context and a NUL-terminated bounded
        // string valid for this call. No borrowed data escapes the callback.
        let context = unsafe { retain_context(raw) };
        let text = unsafe { CStr::from_ptr(json) }.to_str();
        if let (Ok(text), Ok(mut callbacks)) = (text, context.callbacks.try_borrow_mut()) {
            callbacks.message(text);
        }
    }));
}
extern "C" fn snapshot(raw: *mut c_void, destination: *mut c_char, capacity: usize) -> usize {
    catch_unwind(AssertUnwindSafe(|| {
        // SAFETY: native code supplies this destination for capacity bytes.
        // Copy into caller-owned storage before dropping the retained context,
        // so self-destruction cannot leave a dangling snapshot string.
        let context = unsafe { retain_context(raw) };
        let Ok(mut callbacks) = context.callbacks.try_borrow_mut() else {
            return 0;
        };
        let Some(text) = callbacks.snapshot() else {
            return 0;
        };
        if text.is_empty()
            || text.len() > 65_536
            || text.len() >= capacity
            || text.as_bytes().contains(&0)
        {
            return 0;
        }
        unsafe {
            std::ptr::copy_nonoverlapping(text.as_ptr(), destination.cast(), text.len());
        }
        text.len()
    }))
    .unwrap_or(0)
}
/// Owned embedded NSView and callback context, destroyed in that order.
/// `WebView` is neither Send nor Sync. Creation checks the Cocoa main thread;
/// every subsequent operation and destruction stays on that thread.
pub struct WebView<'a> {
    view: NonNull<c_void>,
    _context: Rc<Context<'a>>,
    _main_thread: PhantomData<Rc<()>>,
}
impl<'a> WebView<'a> {
    /// Embed trusted self-contained HTML. Native polling is 50 ms and coalesced.
    /// External navigation is blocked; page code must use its own network policy
    /// (for example CSP). No system window or run loop is created by this API.
    pub fn from_html(
        html: &str,
        width: f64,
        height: f64,
        callbacks: impl Callbacks + 'a,
    ) -> Result<Self, WebViewError> {
        if !is_main_thread() {
            return Err(WebViewError::WrongThread);
        }
        if !width.is_finite()
            || !height.is_finite()
            || width <= 0.0
            || height <= 0.0
            || html.is_empty()
            || html.len() > 1_048_576
        {
            return Err(WebViewError::InvalidInput);
        }
        let html = CString::new(html).map_err(|_| WebViewError::InvalidInput)?;
        let context = Rc::new(Context {
            callbacks: RefCell::new(Box::new(callbacks)),
        });
        // SAFETY: HTML/context remain live through creation; backend copies HTML.
        // Context remains owned until native teardown removes all callbacks.
        let view = NonNull::new(unsafe {
            ravet_web_create_html(
                html.as_ptr(),
                width,
                height,
                Rc::as_ptr(&context).cast_mut().cast(),
                message,
                snapshot,
            )
        })
        .ok_or(WebViewError::CreationFailed)?;
        Ok(Self {
            view,
            _context: context,
            _main_thread: PhantomData,
        })
    }
    pub fn set_background(&self, red: f64, green: f64, blue: f64) -> bool {
        // SAFETY: self owns a live view on the main thread.
        unsafe { ravet_web_set_background(self.view.as_ptr(), red, green, blue) }
    }
    /// Attach to a host-owned Cocoa view. The host retains ownership.
    ///
    /// # Safety
    /// `parent` must point to a live NSView on the Cocoa main thread. It must
    /// remain valid until detach or WebView destruction; it must not independently
    /// destroy the embedded child or its callback context.
    pub unsafe fn attach(&self, parent: NonNull<c_void>) {
        unsafe {
            ravet_web_attach(self.view.as_ptr(), parent.as_ptr());
        }
    }
    pub fn detach(&self) {
        // SAFETY: self owns the native view; detach preserves its lifetime.
        unsafe {
            ravet_web_detach(self.view.as_ptr());
        }
    }
    /// Resize the owned view in logical Cocoa pixels. Rejects nonfinite,
    /// nonpositive or greater-than-16384 dimensions without changing its size.
    /// The embedded page receives the new viewport through native autoresizing.
    pub fn set_size(&self, width: f64, height: f64) -> bool {
        // SAFETY: self owns the view and stays on the Cocoa main thread.
        unsafe { ravet_web_set_size(self.view.as_ptr(), width, height) }
    }
    pub fn set_visible(&self, visible: bool) {
        // SAFETY: visibility changes do not transfer native ownership.
        unsafe {
            ravet_web_set_visible(self.view.as_ptr(), visible);
        }
    }
}
impl Drop for WebView<'_> {
    fn drop(&mut self) {
        // SAFETY: !Send/!Sync and creation check enforce main-thread ownership.
        // Native destruction invalidates timers/handlers before Rc context drops.
        unsafe {
            ravet_web_destroy(self.view.as_ptr());
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    struct Empty;
    impl Callbacks for Empty {
        fn message(&mut self, _: &str) {}
        fn snapshot(&mut self) -> Option<String> {
            None
        }
    }
    #[test]
    fn snapshot_copies_before_callback_context_is_destroyed() {
        use std::cell::Cell;
        type Owner = Rc<RefCell<Option<Rc<Context<'static>>>>>;
        struct Destroying {
            owner: Owner,
            dropped: Rc<Cell<usize>>,
        }
        impl Callbacks for Destroying {
            fn message(&mut self, _: &str) {}
            fn snapshot(&mut self) -> Option<String> {
                self.owner.borrow_mut().take();
                Some("{\"gain\":1}".to_owned())
            }
        }
        impl Drop for Destroying {
            fn drop(&mut self) {
                self.dropped.set(self.dropped.get() + 1);
            }
        }
        let owner: Owner = Rc::new(RefCell::new(None));
        let dropped = Rc::new(Cell::new(0));
        let context = Rc::new(Context {
            callbacks: RefCell::new(Box::new(Destroying {
                owner: owner.clone(),
                dropped: dropped.clone(),
            })),
        });
        let raw = Rc::as_ptr(&context).cast_mut().cast();
        *owner.borrow_mut() = Some(context.clone());
        drop(context);
        let mut destination = [0_u8; 64];
        let length = snapshot(raw, destination.as_mut_ptr().cast(), destination.len());
        assert_eq!(&destination[..length], b"{\"gain\":1}");
        assert!(owner.borrow().is_none());
        assert_eq!(dropped.get(), 1);
    }
    #[test]
    fn snapshot_bounds_and_reentrant_borrows_leave_destination_untouched() {
        struct TooLarge;
        impl Callbacks for TooLarge {
            fn message(&mut self, _: &str) {}
            fn snapshot(&mut self) -> Option<String> {
                Some("x".repeat(65_537))
            }
        }
        let context = Rc::new(Context {
            callbacks: RefCell::new(Box::new(TooLarge)),
        });
        let raw = Rc::as_ptr(&context).cast_mut().cast();
        let mut destination = [7_u8; 64];
        assert_eq!(
            snapshot(raw, destination.as_mut_ptr().cast(), destination.len()),
            0
        );
        assert_eq!(destination, [7; 64]);
        let _guard = context.callbacks.borrow_mut();
        assert_eq!(
            snapshot(raw, destination.as_mut_ptr().cast(), destination.len()),
            0
        );
        assert_eq!(destination, [7; 64]);
    }
    #[test]
    fn worker_thread_cannot_create_a_native_editor() {
        assert!(matches!(
            WebView::from_html("<html></html>", 100.0, 100.0, Empty),
            Err(WebViewError::WrongThread)
        ));
    }
}
