use crate::accessibility::AccessibilityAction;
#[cfg(target_os = "macos")]
use crate::accessibility::{
    AccessibilityActionKind, AccessibilityError, AccessibilityKind, AccessibilityRevision,
    AccessibilitySnapshot,
};
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
    fn ravet_web_create_assets(
        html: *const c_char,
        assets: *const NativeAsset,
        count: usize,
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
    fn ravet_web_pump();
    #[cfg(target_os = "macos")]
    fn ravet_web_accessibility_set(
        view: *mut c_void,
        label: *const c_char,
        nodes: *const NativeAccessibilityNode,
        count: usize,
        focused: u64,
        context: *mut c_void,
        action: Option<extern "C" fn(*mut c_void, u64, u64, u32) -> bool>,
        revision: *mut u64,
    ) -> u32;
}
#[repr(C)]
struct NativeAsset {
    path: *const c_char,
    mime: *const c_char,
    data: *const c_void,
    size: usize,
}

/// A file served to a page created with [`WebView::from_html_with_assets`].
///
/// The page loads from a local asset origin (`ravet://editor/` on macOS,
/// `ravet-<id>://editor/` on Linux, `https://editor.ravet.invalid/` on
/// Windows) and refers to assets by relative URL, for example an image whose
/// source is the relative path `img/knob.png`. Requests are answered from
/// memory, with no file system or network access.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Asset<'a> {
    /// Relative path: ASCII letters, digits and `._-/`, at most 512 bytes, no
    /// `.` or `..` segment, no empty segment or leading or trailing `/`.
    /// `index.html` is reserved for the page.
    pub path: &'a str,
    /// Content type, such as `image/png`. Empty infers it from the extension
    /// (see [`Asset::content_type`]).
    pub mime: &'a str,
    pub data: &'a [u8],
}

/// Most assets per view.
pub const MAXIMUM_ASSETS: usize = 4096;
/// Largest single asset.
pub const MAXIMUM_ASSET_BYTES: usize = 64 << 20;
/// Largest total of all assets of one view.
pub const MAXIMUM_TOTAL_ASSET_BYTES: usize = 256 << 20;

impl Asset<'_> {
    /// Whether the path, content type and size are accepted.
    pub fn is_valid(&self) -> bool {
        let path = self.path;
        let segments_ok = path
            .split('/')
            .all(|s| !s.is_empty() && s != "." && s != "..");
        !path.is_empty()
            && path.len() <= 512
            && path != "index.html"
            && segments_ok
            && path
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'_' | b'-' | b'/'))
            && self.mime.len() <= 127
            && self.mime.bytes().all(|b| (0x20..=0x7e).contains(&b))
            && self.data.len() <= MAXIMUM_ASSET_BYTES
    }
    /// The content type served: `mime`, or one inferred from the extension
    /// (HTML, CSS, JavaScript, JSON, SVG, PNG, JPEG, GIF, WebP, ICO, WOFF/WOFF2,
    /// TTF/OTF, WASM, WAV, MP3, text), else `application/octet-stream`.
    pub fn content_type(&self) -> &str {
        if !self.mime.is_empty() {
            return self.mime;
        }
        let extension = self.path.rsplit_once('.').map_or("", |(_, e)| e);
        match extension.to_ascii_lowercase().as_str() {
            "html" | "htm" => "text/html; charset=utf-8",
            "css" => "text/css; charset=utf-8",
            "js" | "mjs" => "text/javascript; charset=utf-8",
            "json" | "map" => "application/json",
            "svg" => "image/svg+xml",
            "png" => "image/png",
            "jpg" | "jpeg" => "image/jpeg",
            "gif" => "image/gif",
            "webp" => "image/webp",
            "ico" => "image/x-icon",
            "woff" => "font/woff",
            "woff2" => "font/woff2",
            "ttf" => "font/ttf",
            "otf" => "font/otf",
            "wasm" => "application/wasm",
            "wav" => "audio/wav",
            "mp3" => "audio/mpeg",
            "txt" => "text/plain; charset=utf-8",
            _ => "application/octet-stream",
        }
    }
}

/// Check a whole asset set: every asset valid, unique paths, and the count and
/// total size within [`MAXIMUM_ASSETS`] and [`MAXIMUM_TOTAL_ASSET_BYTES`].
pub fn assets_valid(assets: &[Asset<'_>]) -> bool {
    if assets.len() > MAXIMUM_ASSETS || !assets.iter().all(Asset::is_valid) {
        return false;
    }
    let total = assets.iter().try_fold(0_usize, |total, asset| {
        total
            .checked_add(asset.data.len())
            .filter(|total| *total <= MAXIMUM_TOTAL_ASSET_BYTES)
    });
    let mut paths: Vec<&str> = assets.iter().map(|asset| asset.path).collect();
    paths.sort_unstable();
    total.is_some() && paths.windows(2).all(|pair| pair[0] != pair[1])
}

/// Dispatch pending GTK work for embedded Linux editors. Plug-in hosts call this
/// from their UI run loop (for example a 10 ms timer) while an editor is open.
/// It handles at most 32 ready GLib iterations and is a no-op on macOS/Windows.
pub fn pump() {
    // SAFETY: the native pump owns no borrowed buffers; it is a UI-thread no-op
    // off Linux and dispatches only already-registered GLib sources on Linux.
    unsafe { ravet_web_pump() }
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
    /// Accept a discrete intent from the explicit native companion. Default is
    /// refusal. Acceptance does not publish a new value or create a host gesture;
    /// publish the next complete snapshot after callback unwind. Never audio-thread.
    fn accessibility_action(&mut self, _: AccessibilityAction) -> bool {
        false
    }
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

/// # Safety
/// `raw` must be the `Rc::as_ptr` of a live context, used on the UI thread.
unsafe fn retain_context<'a>(raw: *mut c_void) -> Rc<Context<'a>> {
    let raw = raw.cast::<Context<'a>>();
    // SAFETY: native code invokes callbacks only synchronously on the main thread,
    // while the view holds one Rc. Clone at entry permits destruction from a callback.
    unsafe {
        Rc::increment_strong_count(raw);
        Rc::from_raw(raw)
    }
}
extern "C" fn message(raw: *mut c_void, json: *const c_char) {
    if raw.is_null() || json.is_null() {
        return;
    }
    let _ = catch_unwind(AssertUnwindSafe(|| {
        // SAFETY: backend supplies live context and a NUL-terminated bounded
        // string valid for this call. No borrowed data escapes the callback.
        let context = unsafe { retain_context(raw) };
        // SAFETY: non-null NUL-terminated string, valid for this call (above).
        let text = unsafe { CStr::from_ptr(json) }.to_str();
        if let (Ok(text), Ok(mut callbacks)) = (text, context.callbacks.try_borrow_mut()) {
            callbacks.message(text);
        }
    }));
}
extern "C" fn snapshot(raw: *mut c_void, destination: *mut c_char, capacity: usize) -> usize {
    if raw.is_null() || destination.is_null() {
        return 0;
    }
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
        // SAFETY: text.len() < capacity bytes of non-null caller storage.
        unsafe {
            std::ptr::copy_nonoverlapping(text.as_ptr(), destination.cast(), text.len());
        }
        text.len()
    }))
    .unwrap_or(0)
}
#[cfg(target_os = "macos")]
#[repr(C)]
struct NativeAccessibilityNode {
    id: u64,
    label: *const c_char,
    text: *const c_char,
    x: f64,
    y: f64,
    width: f64,
    height: f64,
    minimum: f64,
    value: f64,
    maximum: f64,
    increment: f64,
    kind: u32,
    enabled: bool,
    hidden: bool,
}
#[cfg(target_os = "macos")]
extern "C" fn accessibility_action(
    raw: *mut c_void,
    node_id: u64,
    revision: u64,
    kind: u32,
) -> bool {
    if raw.is_null() {
        return false;
    }
    catch_unwind(AssertUnwindSafe(|| {
        let kind = match kind {
            0 => AccessibilityActionKind::Press,
            1 => AccessibilityActionKind::Increment,
            2 => AccessibilityActionKind::Decrement,
            _ => return false,
        };
        // SAFETY: native ownership gates guarantee a live original context on the
        // UI thread. The retained Rc survives synchronous callback destruction.
        let context = unsafe { retain_context(raw) };
        let Ok(mut callbacks) = context.callbacks.try_borrow_mut() else {
            return false;
        };
        callbacks.accessibility_action(AccessibilityAction {
            node_id,
            revision: AccessibilityRevision(revision),
            kind,
        })
    }))
    .unwrap_or(false)
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
    /// Like [`WebView::from_html`], but the page loads from a local asset origin
    /// that also serves `assets`, so editors can ship images, fonts, scripts
    /// and styles of any size up to the [asset limits](MAXIMUM_ASSET_BYTES)
    /// without inlining or chunking them. Asset data is not copied when the
    /// view is created (it is copied per request), so it borrows for `'a`; the
    /// slice itself is only read during creation.
    pub fn from_html_with_assets(
        html: &str,
        assets: &[Asset<'a>],
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
            || !assets_valid(assets)
        {
            return Err(WebViewError::InvalidInput);
        }
        let html = CString::new(html).map_err(|_| WebViewError::InvalidInput)?;
        let strings = assets
            .iter()
            .map(|asset| {
                Ok((
                    CString::new(asset.path).map_err(|_| WebViewError::InvalidInput)?,
                    CString::new(asset.content_type()).map_err(|_| WebViewError::InvalidInput)?,
                ))
            })
            .collect::<Result<Vec<_>, WebViewError>>()?;
        let native: Vec<NativeAsset> = assets
            .iter()
            .zip(&strings)
            .map(|(asset, (path, mime))| NativeAsset {
                path: path.as_ptr(),
                mime: mime.as_ptr(),
                data: asset.data.as_ptr().cast(),
                size: asset.data.len(),
            })
            .collect();
        let context = Rc::new(Context {
            callbacks: RefCell::new(Box::new(callbacks)),
        });
        // SAFETY: HTML, descriptors and strings live through creation and the
        // backend copies them; asset data borrows for 'a, which outlives the
        // view. Context remains owned until native teardown removes callbacks.
        let view = NonNull::new(unsafe {
            ravet_web_create_assets(
                html.as_ptr(),
                native.as_ptr(),
                native.len(),
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
    /// Install a complete opt-in native accessibility hierarchy for custom
    /// content, replacing automatic WebKit content traversal. None restores the
    /// original behavior and retires old elements. Requires an attached original
    /// native owner. This does not inspect HTML or move system/page focus.
    ///
    /// Every full publication has a new editor-local revision. Retained elements
    /// from previous revisions keep copied metadata but cannot route actions.
    /// Native action/notification reentry reports Busy. Accepted native intent and
    /// presentation acknowledgement are separate; update after callback unwind.
    #[cfg(target_os = "macos")]
    pub fn set_accessibility(
        &self,
        snapshot: Option<&AccessibilitySnapshot>,
    ) -> Result<AccessibilityRevision, AccessibilityError> {
        if !is_main_thread() {
            return Err(AccessibilityError::WrongThread);
        }
        if self._context.callbacks.try_borrow_mut().is_err() {
            return Err(AccessibilityError::Busy);
        }
        // Snapshot constructors already reject NUL; a failure here is unreachable
        // and is reported as invalid input rather than a panic.
        let invalid = |_| AccessibilityError::InvalidInput;
        let label = snapshot
            .map(|s| CString::new(s.label()))
            .transpose()
            .map_err(invalid)?;
        let labels = snapshot
            .into_iter()
            .flat_map(|s| s.nodes())
            .map(|n| CString::new(n.label()))
            .collect::<Result<Vec<_>, _>>()
            .map_err(invalid)?;
        let texts = snapshot
            .into_iter()
            .flat_map(|s| s.nodes())
            .map(|n| match n.kind() {
                AccessibilityKind::StaticText { value } => CString::new(value.as_str()).map(Some),
                _ => Ok(None),
            })
            .collect::<Result<Vec<_>, _>>()
            .map_err(invalid)?;
        let nodes: Vec<_> = snapshot
            .into_iter()
            .flat_map(|s| s.nodes())
            .enumerate()
            .map(|(i, n)| {
                let (x, y, width, height) = n.frame().rectangle();
                let (kind, minimum, value, maximum, increment) = match n.kind() {
                    AccessibilityKind::StaticText { .. } => (0, 0., 0., 0., 0.),
                    AccessibilityKind::Button => (1, 0., 0., 0., 0.),
                    AccessibilityKind::Slider {
                        minimum,
                        value,
                        maximum,
                        increment,
                    } => (2, *minimum, *value, *maximum, *increment),
                };
                let availability = n.availability();
                NativeAccessibilityNode {
                    id: n.id(),
                    label: labels[i].as_ptr(),
                    text: texts[i].as_ref().map_or(std::ptr::null(), |s| s.as_ptr()),
                    x,
                    y,
                    width,
                    height,
                    minimum,
                    value,
                    maximum,
                    increment,
                    kind,
                    enabled: availability.enabled,
                    hidden: availability.hidden,
                }
            })
            .collect();
        let mut revision = 0;
        // SAFETY: this view/context remains owned on the UI thread; native code
        // copies all bounded buffers before publication. None supplies no context.
        let status = unsafe {
            ravet_web_accessibility_set(
                self.view.as_ptr(),
                label.as_ref().map_or(std::ptr::null(), |s| s.as_ptr()),
                if snapshot.is_some() {
                    nodes.as_ptr()
                } else {
                    std::ptr::null()
                },
                nodes.len(),
                snapshot.and_then(|s| s.focused_id()).unwrap_or(0),
                if snapshot.is_some() {
                    Rc::as_ptr(&self._context).cast_mut().cast()
                } else {
                    std::ptr::null_mut()
                },
                snapshot.map(|_| {
                    accessibility_action as extern "C" fn(*mut c_void, u64, u64, u32) -> bool
                }),
                &mut revision,
            )
        };
        match status {
            0 => Ok(AccessibilityRevision(revision)),
            1 => Err(AccessibilityError::InvalidInput),
            2 => Err(AccessibilityError::Unavailable),
            3 => Err(AccessibilityError::Busy),
            4 => Err(AccessibilityError::Exhausted),
            5 => Err(AccessibilityError::WrongThread),
            _ => Err(AccessibilityError::Unavailable),
        }
    }
    pub fn set_background(&self, red: f64, green: f64, blue: f64) -> bool {
        // SAFETY: self owns a live view on the main thread.
        unsafe { ravet_web_set_background(self.view.as_ptr(), red, green, blue) }
    }
    /// Attach to a host-owned native parent. The host retains ownership.
    ///
    /// # Safety
    /// `parent` must be a live NSView (macOS), HWND (Windows) or X11 Window ID
    /// cast to a pointer (Linux), used on the UI thread. It must
    /// remain valid until detach or WebView destruction; it must not independently
    /// destroy the embedded child or its callback context.
    pub unsafe fn attach(&self, parent: NonNull<c_void>) {
        // SAFETY: self owns a live view; the caller guarantees `parent` (above).
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
    /// Resize the owned view: logical points on macOS and Linux, physical pixels
    /// (the parent HWND's client units) on Windows. Rejects nonfinite,
    /// nonpositive or greater-than-16384 dimensions without changing its size.
    /// The embedded page receives the new viewport through native autoresizing.
    pub fn set_size(&self, width: f64, height: f64) -> bool {
        // SAFETY: self owns the view and stays on the Cocoa main thread.
        unsafe { ravet_web_set_size(self.view.as_ptr(), width, height) }
    }
    /// Native view handle for the standalone window owner on Windows/Linux.
    #[cfg(not(target_os = "macos"))]
    pub(crate) fn raw(&self) -> *mut c_void {
        self.view.as_ptr()
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
    #[cfg(target_os = "macos")]
    use std::cell::Cell;
    #[cfg(target_os = "macos")]
    struct Empty;
    #[cfg(target_os = "macos")]
    impl Callbacks for Empty {
        fn message(&mut self, _: &str) {}
        fn snapshot(&mut self) -> Option<String> {
            None
        }
    }
    #[cfg(target_os = "macos")]
    #[test]
    fn native_action_retains_context_through_callback_self_destruction() {
        type Owner = Rc<RefCell<Option<Rc<Context<'static>>>>>;
        struct Retiring {
            owner: Owner,
            dropped: Rc<Cell<usize>>,
            seen: Rc<Cell<Option<AccessibilityAction>>>,
        }
        impl Callbacks for Retiring {
            fn message(&mut self, _: &str) {}
            fn snapshot(&mut self) -> Option<String> {
                None
            }
            fn accessibility_action(&mut self, action: AccessibilityAction) -> bool {
                self.seen.set(Some(action));
                self.owner.borrow_mut().take();
                true
            }
        }
        impl Drop for Retiring {
            fn drop(&mut self) {
                self.dropped.set(self.dropped.get() + 1);
            }
        }
        let owner: Owner = Rc::new(RefCell::new(None));
        let dropped = Rc::new(Cell::new(0));
        let seen = Rc::new(Cell::new(None));
        *owner.borrow_mut() = Some(Rc::new(Context {
            callbacks: RefCell::new(Box::new(Retiring {
                owner: owner.clone(),
                dropped: dropped.clone(),
                seen: seen.clone(),
            })),
        }));
        let raw = Rc::as_ptr(owner.borrow().as_ref().unwrap())
            .cast_mut()
            .cast();
        assert!(accessibility_action(raw, 9, 31, 1));
        assert!(owner.borrow().is_none());
        assert_eq!(dropped.get(), 1);
        assert_eq!(
            seen.get(),
            Some(AccessibilityAction {
                node_id: 9,
                revision: AccessibilityRevision(31),
                kind: AccessibilityActionKind::Increment
            })
        );
    }
    #[cfg(target_os = "macos")]
    #[test]
    fn native_action_refuses_bad_kind_nested_borrow_and_catches_panic() {
        struct Nested {
            raw: Rc<Cell<*mut c_void>>,
            count: Rc<Cell<usize>>,
        }
        impl Callbacks for Nested {
            fn message(&mut self, _: &str) {}
            fn snapshot(&mut self) -> Option<String> {
                None
            }
            fn accessibility_action(&mut self, _: AccessibilityAction) -> bool {
                self.count.set(self.count.get() + 1);
                assert!(!accessibility_action(self.raw.get(), 1, 1, 0));
                panic!("fixture native action panic")
            }
        }
        let raw = Rc::new(Cell::new(std::ptr::null_mut()));
        let count = Rc::new(Cell::new(0));
        let context = Rc::new(Context {
            callbacks: RefCell::new(Box::new(Nested {
                raw: raw.clone(),
                count: count.clone(),
            })),
        });
        raw.set(Rc::as_ptr(&context).cast_mut().cast());
        assert!(!accessibility_action(raw.get(), 1, 1, 99));
        assert_eq!(count.get(), 0);
        assert!(!accessibility_action(raw.get(), 1, 1, 0));
        assert_eq!(count.get(), 1);
        assert_eq!(Rc::strong_count(&context), 1);
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
    fn null_native_arguments_are_refused_without_dereference() {
        struct Counting(Rc<Cell<usize>>);
        impl Callbacks for Counting {
            fn message(&mut self, _: &str) {
                self.0.set(self.0.get() + 1);
            }
            fn snapshot(&mut self) -> Option<String> {
                Some("{}".to_owned())
            }
        }
        use std::cell::Cell;
        let count = Rc::new(Cell::new(0));
        let context = Rc::new(Context {
            callbacks: RefCell::new(Box::new(Counting(count.clone()))),
        });
        let raw = Rc::as_ptr(&context).cast_mut().cast();
        message(std::ptr::null_mut(), c"{}".as_ptr());
        message(raw, std::ptr::null());
        assert_eq!(count.get(), 0);
        let mut destination = [0_u8; 8];
        assert_eq!(
            snapshot(std::ptr::null_mut(), destination.as_mut_ptr().cast(), 8),
            0
        );
        assert_eq!(snapshot(raw, std::ptr::null_mut(), 8), 0);
        #[cfg(target_os = "macos")]
        assert!(!accessibility_action(std::ptr::null_mut(), 1, 1, 0));
        message(raw, c"{}".as_ptr());
        assert_eq!(count.get(), 1);
        assert_eq!(Rc::strong_count(&context), 1);
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
    fn asset_paths_types_and_limits() {
        fn asset<'a>(path: &'a str, mime: &'a str, data: &'a [u8]) -> Asset<'a> {
            Asset { path, mime, data }
        }
        assert!(asset("img/knob.png", "", b"x").is_valid());
        assert_eq!(asset("img/knob.png", "", b"").content_type(), "image/png");
        assert_eq!(
            asset("a.FONT", "", b"").content_type(),
            "application/octet-stream"
        );
        assert_eq!(asset("a.woff2", "", b"").content_type(), "font/woff2");
        assert_eq!(asset("a.bin", "x/y", b"").content_type(), "x/y");
        for path in [
            "",
            "/a",
            "a/",
            "a//b",
            "../a",
            "a/../b",
            "./a",
            "index.html",
            "a b",
            "a?b",
            "ü",
        ] {
            assert!(!asset(path, "", b"").is_valid(), "{path:?}");
        }
        assert!(!asset(&"a".repeat(513), "", b"").is_valid());
        assert!(!asset("a", "text/\n", b"").is_valid());
        assert!(!asset("a", "bad\u{7f}", b"").is_valid());
        let big = vec![0_u8; MAXIMUM_ASSET_BYTES + 1];
        assert!(!asset("big", "", &big).is_valid());
        assert!(assets_valid(&[asset("a", "", b""), asset("b", "", b"")]));
        assert!(!assets_valid(&[asset("a", "", b""), asset("a", "", b"")]));
        let half = vec![0_u8; MAXIMUM_ASSET_BYTES];
        let many: Vec<_> = ["1", "2", "3", "4", "5"]
            .iter()
            .map(|p| asset(p, "", &half))
            .collect();
        assert!(!assets_valid(&many));
        assert!(assets_valid(&many[..4]));
    }
    // Windows/Linux adopt the first creating thread as their UI thread.
    #[cfg(target_os = "macos")]
    #[test]
    fn worker_thread_cannot_create_a_native_editor() {
        assert!(matches!(
            WebView::from_html("<html></html>", 100.0, 100.0, Empty),
            Err(WebViewError::WrongThread)
        ));
    }
}
