//! Capture a PNG of a built CLAP bundle's embedded web editor for documentation.
//!
//! Usage: `capture <trusted CLAP bundle> <output.png> [page script]`
//!
//! Opens the bundle's first plug-in editor in an off-screen Cocoa parent at the
//! editor's own size, waits for the page to receive product info, optionally
//! evaluates a page script (for example `ravetPlugin.set('mix',35);true`), and
//! writes a WebKit snapshot. Trusted workspace-built bundles only; no host,
//! DAW or device behaviour is exercised.
#[cfg(target_os = "macos")]
#[allow(unsafe_code)]
mod native {
    use clack_extensions::gui::{GuiApiType, GuiConfiguration, Window};
    use clack_host::prelude::*;
    use ravet_host::{ClapLibrary, PluginEditor};
    use std::ffi::{CString, c_char, c_void};
    unsafe extern "C" {
        fn editor_probe_create() -> *mut c_void;
        fn editor_probe_view(parent: *mut c_void) -> *mut c_void;
        fn editor_probe_resize(parent: *mut c_void, width: f64, height: f64);
        fn editor_probe_eval(parent: *mut c_void, script: *const c_char) -> bool;
        fn editor_probe_snapshot(parent: *mut c_void, path: *const c_char) -> bool;
        fn editor_probe_pump(seconds: f64);
        fn editor_probe_destroy(parent: *mut c_void);
    }
    struct Parent(*mut c_void);
    impl Parent {
        fn eval(&self, script: &str) -> Result<bool, Box<dyn std::error::Error>> {
            let script = CString::new(script)?;
            // SAFETY: this fixture owns a live parent and a NUL-terminated script.
            Ok(unsafe { editor_probe_eval(self.0, script.as_ptr()) })
        }
    }
    impl Drop for Parent {
        fn drop(&mut self) {
            // SAFETY: the parent was created by this fixture and is destroyed once.
            unsafe { editor_probe_destroy(self.0) }
        }
    }
    pub fn run() -> Result<(), Box<dyn std::error::Error>> {
        let mut args = std::env::args_os().skip(1);
        let usage = "usage: capture <trusted CLAP bundle> <output.png> [page script]";
        let bundle = args.next().ok_or(usage)?;
        let output = args.next().ok_or(usage)?;
        let script = args.next().map(|s| s.into_string()).transpose();
        let script = script.map_err(|_| "page script must be UTF-8")?;
        if args.next().is_some() {
            return Err(usage.into());
        }
        let output = CString::new(output.to_str().ok_or("output path must be UTF-8")?)?;
        // SAFETY: the caller chooses a trusted, workspace-built bundle; all
        // native parent operations stay on the process main thread.
        let library = unsafe { ClapLibrary::load(std::path::Path::new(&bundle)) }?;
        let id = library
            .plugins()
            .first()
            .ok_or("bundle has no plug-ins")?
            .id
            .clone();
        let info = HostInfo::new("RAVET editor capture", "RAVET", "", "0.1")?;
        let mut instance = PluginInstance::<()>::new(|_| (), |_| (), library.entry(), &id, &info)?;
        drop(library);
        // SAFETY: called on the main thread; the fixture owns its parent window.
        let parent = Parent(unsafe { editor_probe_create() });
        let mut editor = PluginEditor::create(
            &mut instance,
            GuiConfiguration {
                api_type: GuiApiType::COCOA,
                is_floating: false,
            },
        )?;
        let size = editor.size().ok_or("editor reported no size")?;
        // SAFETY: the parent view outlives the editor, which is dropped first.
        unsafe {
            editor_probe_resize(parent.0, f64::from(size.width), f64::from(size.height));
            editor.set_parent(Window::from_cocoa_nsview(editor_probe_view(parent.0)))?;
        }
        editor.show()?;
        let mut ready = false;
        for _ in 0..100 {
            // SAFETY: main-thread run-loop pump.
            unsafe { editor_probe_pump(0.05) };
            if parent.eval("Boolean(window.ravetPlugin?.info?.product?.name)")? {
                ready = true;
                break;
            }
        }
        if !ready {
            return Err("editor page did not receive product info".into());
        }
        if let Some(script) = script
            && !parent.eval(&script)?
        {
            return Err("page script did not return true".into());
        }
        // Let values reach the page and fonts settle before the snapshot.
        unsafe { editor_probe_pump(0.5) };
        // SAFETY: the parent is live and the path is NUL-terminated.
        if !unsafe { editor_probe_snapshot(parent.0, output.as_ptr()) } {
            return Err("WebKit snapshot failed".into());
        }
        drop(editor);
        unsafe { editor_probe_pump(0.15) };
        println!(
            "{}x{} editor captured to {}",
            size.width,
            size.height,
            output.to_string_lossy()
        );
        Ok(())
    }
}
#[cfg(target_os = "macos")]
fn main() -> Result<(), Box<dyn std::error::Error>> {
    native::run()
}
#[cfg(not(target_os = "macos"))]
fn main() {
    eprintln!("Editor capture currently requires macOS.");
    std::process::exit(1);
}
