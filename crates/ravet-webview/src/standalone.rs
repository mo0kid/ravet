//! Main-thread standalone window ownership and default Audio/MIDI settings.
use crate::{Callbacks, WebView};
use std::{
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
    fn ravet_standalone_set_size(handle: *mut c_void, width: f64, height: f64);
    fn ravet_standalone_fit_editor(handle: *mut c_void);
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
/// Owned macOS standalone window; neither Send nor Sync. Pump and drop on main.
/// The standard popup emits device-selection actions; the consumer owns opening,
/// validation, lifecycle and snapshots. It opens no devices on window creation.
pub struct StandaloneWindow {
    handle: NonNull<c_void>,
    _main: PhantomData<Rc<()>>,
    options: StandaloneOptions,
}
/// Embedded view borrowing its parent window, preventing premature window destruction.
pub struct StandaloneEditor<'a> {
    view: WebView<'a>,
    _window: &'a StandaloneWindow,
}
impl StandaloneWindow {
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
        let html = settings_document(html, self.options.io_settings)?;
        let web = WebView::from_html(&html, self.options.width, self.options.height, callbacks)?;
        // SAFETY: window retains its content NSView until after caller drops the web view.
        let parent = NonNull::new(unsafe { ravet_standalone_view(self.handle.as_ptr()) })
            .expect("content view");
        unsafe {
            web.attach(parent);
            ravet_standalone_fit_editor(self.handle.as_ptr());
        }
        Ok(StandaloneEditor {
            view: web,
            _window: self,
        })
    }
    /// Dispatch main-thread native events for at most this many seconds (0..1).
    /// Returns whether the window is visible; hidden fixtures may ignore it.
    pub fn pump(&self, seconds: f64) -> bool {
        if !seconds.is_finite() || !(0.0..=1.0).contains(&seconds) {
            return false;
        }
        // SAFETY: live main-thread handle; event dispatch does not transfer ownership.
        unsafe { ravet_standalone_pump(self.handle.as_ptr(), seconds) }
    }
    /// Evaluate trusted local fixture JavaScript; waits at most three seconds.
    /// Returns true only for a truthy result without a JavaScript error.
    pub fn eval(&self, script: &str) -> Result<bool, Box<dyn std::error::Error>> {
        let script = CString::new(script)?;
        // SAFETY: owned live window and stable NUL-terminated trusted fixture script.
        Ok(unsafe { ravet_standalone_eval(self.handle.as_ptr(), script.as_ptr()) })
    }
    /// Write a PNG view snapshot to an explicit UTF-8 path, outside audio.
    pub fn snapshot(&self, path: &std::path::Path) -> Result<bool, Box<dyn std::error::Error>> {
        let path = CString::new(path.to_str().ok_or("Screenshot path must be UTF-8")?)?;
        // SAFETY: live main-thread window; native function copies the path before returning.
        Ok(unsafe { ravet_standalone_snapshot(self.handle.as_ptr(), path.as_ptr()) })
    }
}
impl StandaloneWindow {
    /// Resize content to finite 320..4096 by 240..4096 logical points.
    /// Cocoa's minimum window size may constrain smaller requests.
    /// With aspect locking enabled, the requested rectangle bounds a fitted size.
    pub fn set_size(&self, width: f64, height: f64) -> bool {
        if !(320.0..=4096.0).contains(&width) || !(240.0..=4096.0).contains(&height) {
            return false;
        }
        // SAFETY: main-thread live owned window; native resize retains view ownership.
        unsafe {
            ravet_standalone_set_size(self.handle.as_ptr(), width, height);
        }
        true
    }
    /// Activate the native settings button. Returns false when explicitly disabled.
    pub fn open_settings(&self) -> bool {
        // SAFETY: uniquely owned live main-thread window; action transfers no ownership.
        unsafe { ravet_standalone_open_settings(self.handle.as_ptr()) }
    }
}
impl StandaloneEditor<'_> {
    /// Show or hide the embedded view while retaining its parent borrow.
    pub fn set_visible(&self, visible: bool) {
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
