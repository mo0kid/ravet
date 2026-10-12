//! Main-thread standalone window ownership and default Audio/MIDI settings.
use crate::accessibility::{AccessibilityError, AccessibilityRevision, AccessibilitySnapshot};
use crate::{Callbacks, ContentConstraints, WebView};
use std::{
    cell::Cell,
    ffi::{CString, c_char, c_void},
    marker::PhantomData,
    ptr::NonNull,
    rc::Rc,
};
unsafe extern "C" {
    fn ravet_standalone_create(
        hidden: bool,
        settings: bool,
        title: *const c_char,
        width: f64,
        height: f64,
        lock_aspect_ratio: bool,
    ) -> *mut c_void;
    fn ravet_standalone_view(handle: *mut c_void) -> *mut c_void;
    fn ravet_standalone_pump(handle: *mut c_void, seconds: f64) -> bool;
    fn ravet_standalone_eval(handle: *mut c_void, script: *const c_char) -> bool;
    fn ravet_standalone_snapshot(handle: *mut c_void, path: *const c_char) -> bool;
    fn ravet_standalone_destroy(handle: *mut c_void);
    fn ravet_standalone_open_settings(handle: *mut c_void) -> bool;
    fn ravet_standalone_set_size(handle: *mut c_void, width: f64, height: f64) -> bool;
    fn ravet_standalone_request_close(handle: *mut c_void) -> bool;
    fn ravet_standalone_state(
        handle: *mut c_void,
        closed: *mut bool,
        visible: *mut bool,
        key: *mut bool,
        miniaturized: *mut bool,
    ) -> bool;
    fn ravet_standalone_fit_editor(handle: *mut c_void);
    fn ravet_standalone_content_constraints(
        handle: *mut c_void,
        enabled: bool,
        minimum_width: f64,
        minimum_height: f64,
        maximum_width: f64,
        maximum_height: f64,
        width: f64,
        height: f64,
    ) -> bool;
    fn ravet_standalone_content_size(
        handle: *mut c_void,
        width: *mut f64,
        height: *mut f64,
    ) -> bool;
}
/// Main-thread standalone window configuration. Audio/MIDI settings are enabled by default.
/// Embedded plug-in editors use `WebView` directly and leave devices to their host.
#[derive(Clone, Debug)]
pub struct StandaloneOptions {
    /// Native window title (1..256 UTF-8 bytes, without NUL).
    /// Also identifies saved Audio/MIDI preferences; keep it stable per application.
    pub title: String,
    /// Initial content width, 320..4096 logical points.
    pub width: f64,
    /// Initial content height, 240..4096 logical points.
    pub height: f64,
    /// Keep the initial content width/height ratio during resizing. Default false.
    /// Programmatic size requests fit the largest matching rectangle within them.
    pub lock_aspect_ratio: bool,
    /// Include the standard title-bar button and settings popup. Set false to opt out.
    pub io_settings: bool,
    /// Create a hidden window for hardware-free main-thread fixtures.
    /// Settings use process-local memory instead of writing user preferences.
    pub hidden: bool,
}
impl Default for StandaloneOptions {
    fn default() -> Self {
        Self {
            title: "RAVET".into(),
            width: 900.,
            height: 780.,
            lock_aspect_ratio: false,
            io_settings: true,
            hidden: false,
        }
    }
}
/// Copied state of this exact owned macOS window, without showing or focusing it.
/// `closed` is monotonic close admission. During synchronous native will-close
/// notifications, the other flags can still reflect the pre-close state; after
/// closure returns, `visible` is false. Hidden live windows have `closed=false`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct StandaloneWindowState {
    /// Terminal local close admission; never reopened through this owner.
    pub closed: bool,
    /// Current Cocoa visibility, distinct from hidden/live versus closed.
    pub visible: bool,
    /// Current native key-window flag; this read does not activate a window.
    pub key: bool,
    /// Current native miniaturized flag; this read does not minimize a window.
    pub miniaturized: bool,
}
/// Owned macOS standalone window; neither Send nor Sync. Pump and drop on main.
/// The standard popup emits device-selection actions; the consumer owns opening,
/// validation, lifecycle and snapshots. It opens no devices on window creation.
/// Consumers supply `io.outputs[].nominal_rate` for the initial rate selection
/// (falling back to `default_rate`) and may publish `sample_rate_mismatch` to
/// show the actual rate and offer an explicit selection before retrying Start.
/// See `docs/native-editor.md` for that snapshot contract.
pub struct StandaloneWindow {
    handle: NonNull<c_void>,
    _main: PhantomData<Rc<()>>,
    options: StandaloneOptions,
    constraints: Cell<Option<ContentConstraints>>,
    operation: Cell<bool>,
}
/// Embedded view borrowing its parent window, preventing premature window destruction.
pub struct StandaloneEditor<'a> {
    view: WebView<'a>,
    _window: &'a StandaloneWindow,
}
struct WindowOperation<'a>(&'a Cell<bool>);
impl Drop for WindowOperation<'_> {
    fn drop(&mut self) {
        self.0.set(false);
    }
}
impl StandaloneWindow {
    fn begin_operation(&self) -> Option<WindowOperation<'_>> {
        if self.operation.replace(true) {
            return None;
        }
        Some(WindowOperation(&self.operation))
    }
    fn content_aspect(&self) -> Option<(f64, f64)> {
        self.options
            .lock_aspect_ratio
            .then_some((self.options.width, self.options.height))
    }
    /// Create a native window with the selected options.
    pub fn new(options: StandaloneOptions) -> Result<Self, Box<dyn std::error::Error>> {
        if !crate::is_main_thread() {
            return Err("Window requires Cocoa main thread".into());
        }
        if options.title.is_empty()
            || options.title.len() > 256
            || !(320.0..=4096.0).contains(&options.width)
            || !(240.0..=4096.0).contains(&options.height)
        {
            return Err("Invalid standalone window options".into());
        }
        let title = CString::new(options.title.as_str())?;
        // SAFETY: creation occurs on main and returns a uniquely owned native handle.
        let handle = NonNull::new(unsafe {
            ravet_standalone_create(
                options.hidden,
                options.io_settings,
                title.as_ptr(),
                options.width,
                options.height,
                options.lock_aspect_ratio,
            )
        })
        .ok_or("Window creation failed")?;
        Ok(Self {
            handle,
            _main: PhantomData,
            options,
            constraints: Cell::new(None),
            operation: Cell::new(false),
        })
    }
    /// Embed a trusted self-contained page. With settings enabled, exactly one
    /// closing `</body>` is required and the standard popup is injected there.
    /// Inline scripts/styles must be allowed by the consumer CSP. The settings
    /// snapshot/action contract is documented in `docs/native-editor.md`.
    /// The embedded page follows the native content size when the window resizes.
    pub fn embed(
        &self,
        html: &str,
        callbacks: impl Callbacks + 'static,
    ) -> Result<StandaloneEditor<'_>, Box<dyn std::error::Error>> {
        if self.state().is_none_or(|state| state.closed) {
            return Err("Cannot embed in a closed standalone window".into());
        }
        let html = settings_document(html, self.options.io_settings)?;
        let web = WebView::from_html(&html, self.options.width, self.options.height, callbacks)?;
        if self.state().is_none_or(|state| state.closed) {
            return Err("Standalone window closed while creating its editor".into());
        }
        // SAFETY: window retains its content NSView until after caller drops the web view.
        let parent = NonNull::new(unsafe { ravet_standalone_view(self.handle.as_ptr()) })
            .ok_or("Standalone window content view unavailable")?;
        unsafe {
            web.attach(parent);
            ravet_standalone_fit_editor(self.handle.as_ptr());
        }
        if self.state().is_none_or(|state| state.closed) {
            return Err("Standalone window closed while attaching its editor".into());
        }
        Ok(StandaloneEditor {
            view: web,
            _window: self,
        })
    }
    /// Dispatch main-thread native events for at most this many seconds (0..1).
    /// Returns whether the window is visible; hidden fixtures may ignore it.
    /// A closed owner can still pump cleanup events but returns false; use
    /// [`Self::state`] to distinguish hidden live from closed.
    pub fn pump(&self, seconds: f64) -> bool {
        if !seconds.is_finite() || !(0.0..=1.0).contains(&seconds) {
            return false;
        }
        // SAFETY: live main-thread handle; event dispatch does not transfer ownership.
        unsafe { ravet_standalone_pump(self.handle.as_ptr(), seconds) }
    }
    /// Evaluate trusted local fixture JavaScript; waits at most three seconds.
    /// Returns true only for a truthy result without a JavaScript error.
    /// Closed owners return false; already-admitted JavaScript is not cancelled.
    pub fn eval(&self, script: &str) -> Result<bool, Box<dyn std::error::Error>> {
        let script = CString::new(script)?;
        // SAFETY: owned live window and stable NUL-terminated trusted fixture script.
        Ok(unsafe { ravet_standalone_eval(self.handle.as_ptr(), script.as_ptr()) })
    }
    /// Write a PNG view snapshot to an explicit UTF-8 path, outside audio.
    /// Closed owners return false without writing the path. Pending native
    /// completion retains the native window and checks closure before file output.
    pub fn snapshot(&self, path: &std::path::Path) -> Result<bool, Box<dyn std::error::Error>> {
        let path = CString::new(path.to_str().ok_or("Screenshot path must be UTF-8")?)?;
        // SAFETY: live main-thread window; native function copies the path before returning.
        Ok(unsafe { ravet_standalone_snapshot(self.handle.as_ptr(), path.as_ptr()) })
    }
}
impl StandaloneWindow {
    /// Read this exact main-thread window without showing or changing focus.
    /// `None` means native state could not be read. Closed and hidden are distinct;
    /// see [`StandaloneWindowState`] for synchronous notification-phase flags.
    pub fn state(&self) -> Option<StandaloneWindowState> {
        let mut state = StandaloneWindowState {
            closed: false,
            visible: false,
            key: false,
            miniaturized: false,
        };
        // SAFETY: main-thread live owner; four distinct exclusive output pointers
        // remain valid synchronously and native code publishes all only on success.
        unsafe {
            ravet_standalone_state(
                self.handle.as_ptr(),
                &mut state.closed,
                &mut state.visible,
                &mut state.key,
                &mut state.miniaturized,
            )
        }
        .then_some(state)
    }
    /// Request closure of this exact window through Cocoa `performClose:`.
    /// Returns false on a native veto, unavailable owner or synchronous operation
    /// reentry. Outside an in-progress operation, already-closed is a true no-op.
    /// Native close admission retires future window effects before notifications.
    /// No process quit, implicit showing/focus or arbitrary callback API is added.
    /// A borrowed editor remains alive until explicitly dropped before its parent;
    /// closure does not cancel already-admitted JavaScript or application callbacks.
    pub fn request_close(&self) -> bool {
        let Some(_operation) = self.begin_operation() else {
            return false;
        };
        // SAFETY: uniquely owned live main-thread native window, retained for all
        // borrowed editors; native admitted closure does not destroy this handle.
        unsafe { ravet_standalone_request_close(self.handle.as_ptr()) }
    }
    /// Resize content to finite 320..4096 by 240..4096 logical points.
    /// Native user-resize minima do not guarantee a programmatic size clamp;
    /// use [`Self::content_size`] after pumping to measure actual content bounds.
    /// With aspect locking enabled, the requested rectangle bounds a fitted size.
    /// An installed content policy clamps the fitted extent; a small request may
    /// grow to its minimum. Synchronous resize/policy reentry is rejected.
    pub fn set_size(&self, width: f64, height: f64) -> bool {
        if !(320.0..=4096.0).contains(&width) || !(240.0..=4096.0).contains(&height) {
            return false;
        }
        let Some(_operation) = self.begin_operation() else {
            return false;
        };
        let (width, height) = if let Some(constraints) = self.constraints.get() {
            let Some(size) = constraints.resolve(width, height, self.content_aspect()) else {
                return false;
            };
            size
        } else {
            (width, height)
        };
        // SAFETY: main-thread live owned window; native resize retains view ownership.
        unsafe { ravet_standalone_set_size(self.handle.as_ptr(), width, height) }
    }
    /// Measure this owned window's current content extent in logical points.
    /// Returns `None` if closed or no finite positive content bounds are available.
    /// Call after [`Self::pump`] to sample actual native fitting and aspect
    /// constraints rather than assuming a [`Self::set_size`] request
    /// became the current size. This read does not resize or emit callbacks.
    pub fn content_size(&self) -> Option<(f64, f64)> {
        let (mut width, mut height) = (0., 0.);
        // SAFETY: live main-thread owner and two exclusive output pointers valid
        // for the synchronous call; native code writes both only on success.
        unsafe { ravet_standalone_content_size(self.handle.as_ptr(), &mut width, &mut height) }
            .then_some((width, height))
    }
    /// Return the last admitted opt-in content policy, without native access.
    /// During a synchronous install callback this remains the prior admitted
    /// policy until the outer install finishes; reentrant installs are rejected.
    /// This copied policy remains readable after closure.
    pub fn content_constraints(&self) -> Option<ContentConstraints> {
        self.constraints.get()
    }
    /// Install explicit content min/max bounds and immediately fit current size.
    /// Aspect incompatibility or unavailable current content rejects before any
    /// native/Rust policy mutation. `None` restores the native user-resize min/max
    /// captured at creation without requesting another content size. With no
    /// policy, the original unconstrained programmatic behavior is preserved.
    ///
    /// Native user constraints and programmatic fitting are distinct from human
    /// drag evidence. Cocoa rounds at logical-point precision; measure actual
    /// content after pumping, allowing one point of native bounds/aspect rounding.
    /// Closure and synchronous resize/policy/close reentry reject. Once native work is admitted,
    /// effects of foreign Cocoa notification observers cannot be rolled back.
    pub fn set_content_constraints(&self, constraints: Option<ContentConstraints>) -> bool {
        let Some(_operation) = self.begin_operation() else {
            return false;
        };
        let Some((width, height)) = self.content_size() else {
            return false;
        };
        let (minimum_width, minimum_height, maximum_width, maximum_height, width, height) =
            if let Some(policy) = constraints {
                let Some((width, height)) = policy.resolve(width, height, self.content_aspect())
                else {
                    return false;
                };
                let (minimum_width, minimum_height) = policy.minimum_size();
                let (maximum_width, maximum_height) = policy.maximum_size();
                (
                    minimum_width,
                    minimum_height,
                    maximum_width,
                    maximum_height,
                    width,
                    height,
                )
            } else {
                (0., 0., 0., 0., width, height)
            };
        // SAFETY: live main-thread owner, fully preflighted copied scalar policy,
        // and operation guard retained through synchronous native notifications.
        if !unsafe {
            ravet_standalone_content_constraints(
                self.handle.as_ptr(),
                constraints.is_some(),
                minimum_width,
                minimum_height,
                maximum_width,
                maximum_height,
                width,
                height,
            )
        } {
            return false;
        }
        self.constraints.set(constraints);
        true
    }
    /// Activate the native settings button. Returns false if disabled or closed.
    pub fn open_settings(&self) -> bool {
        // SAFETY: uniquely owned live main-thread window; action transfers no ownership.
        unsafe { ravet_standalone_open_settings(self.handle.as_ptr()) }
    }
}
impl StandaloneEditor<'_> {
    /// Install explicit native semantics for custom editor content. Closed parent
    /// owners reject new publication; None remains original-owner cleanup.
    /// The standard title-bar settings popup stays outside this hierarchy.
    pub fn set_accessibility(
        &self,
        snapshot: Option<&AccessibilitySnapshot>,
    ) -> Result<AccessibilityRevision, AccessibilityError> {
        if snapshot.is_some() && self._window.state().is_none_or(|state| state.closed) {
            return Err(AccessibilityError::Unavailable);
        }
        self.view.set_accessibility(snapshot)
    }

    /// Show or hide the embedded view while retaining its parent borrow.
    /// After parent closure this is a no-op; it never reopens a closed window.
    pub fn set_visible(&self, visible: bool) {
        if self._window.state().is_none_or(|state| state.closed) {
            return;
        }
        self.view.set_visible(visible);
    }
}
fn settings_document(html: &str, enabled: bool) -> Result<String, Box<dyn std::error::Error>> {
    if !enabled {
        return Ok(html.to_owned());
    }
    if html.matches("</body>").count() != 1 {
        return Err("Standard settings require one closing body tag".into());
    }
    let settings = include_str!("../web/io-settings.html");
    let theme = include_str!("../../ravet-surface/web/theme.css");
    if html
        .len()
        .checked_add(settings.len() + theme.len() + 15)
        .is_none_or(|size| size > 1_048_576)
    {
        return Err("Standalone document exceeds 1 MiB".into());
    }
    Ok(html.replacen(
        "</body>",
        &format!("<style>{theme}</style>{settings}</body>"),
        1,
    ))
}
impl Drop for StandaloneWindow {
    fn drop(&mut self) {
        // SAFETY: unique main-thread handle; web view is already destroyed by its owner.
        unsafe {
            ravet_standalone_destroy(self.handle.as_ptr());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn settings_are_default_and_opt_out_preserves_document() {
        assert!(StandaloneOptions::default().io_settings);
        assert!(!StandaloneOptions::default().lock_aspect_ratio);
        let page = "<body><h1>Consumer</h1></body>";
        assert!(
            settings_document(page, true)
                .unwrap()
                .contains("io-settings")
        );
        assert_eq!(settings_document(page, false).unwrap(), page);
        assert!(settings_document("<body>", true).is_err());
        assert!(settings_document("</body></body>", true).is_err());
        assert!(settings_document(&format!("{} </body>", "x".repeat(1_048_576)), true).is_err());
    }
}
