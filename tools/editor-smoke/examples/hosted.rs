//! Actual-binary guarded editor hosting; trusted compliant bundles only.
#[cfg(target_os = "macos")]
#[allow(unsafe_code)]
mod native {
    use clack_extensions::gui::{GuiApiType, GuiConfiguration, GuiSize, Window};
    use clack_host::prelude::*;
    use ravet_host::{ClapLibrary, EditorError, PluginEditor};
    use std::{ffi::c_void, path::PathBuf};
    unsafe extern "C" {
        fn editor_probe_create() -> *mut c_void;
        fn editor_probe_view(parent: *mut c_void) -> *mut c_void;
        fn editor_probe_resize(parent: *mut c_void, width: f64, height: f64);
        fn editor_probe_children(parent: *mut c_void) -> usize;
        fn editor_probe_pump(seconds: f64);
        fn editor_probe_destroy(parent: *mut c_void);
    }
    struct Parent(*mut c_void);
    impl Drop for Parent {
        fn drop(&mut self) {
            unsafe { editor_probe_destroy(self.0) }
        }
    }
    pub fn run() -> Result<(), Box<dyn std::error::Error>> {
        let path = PathBuf::from(
            std::env::args_os()
                .nth(1)
                .ok_or("usage: hosted <trusted compliant CLAP bundle>")?,
        );
        // SAFETY: this fixture is for explicit caller-selected trusted compliant
        // bundles, with all native parent operations on the process main thread.
        let library = unsafe { ClapLibrary::load(&path) }?;
        let id = library.plugins()[0].id.clone();
        let info = HostInfo::new("RAVET guarded editor fixture", "RAVET", "", "0.1")?;
        let mut instance = PluginInstance::<()>::new(|_| (), |_| (), library.entry(), &id, &info)?;
        drop(library);
        let parent = Parent(unsafe { editor_probe_create() });
        for cycle in 0..3 {
            unsafe {
                editor_probe_pump(0.15);
            }
            assert_eq!(
                unsafe { editor_probe_children(parent.0) },
                0,
                "previous GUI leaked"
            );
            {
                let mut editor = PluginEditor::create(
                    &mut instance,
                    GuiConfiguration {
                        api_type: GuiApiType::COCOA,
                        is_floating: false,
                    },
                )?;
                assert!(matches!(editor.show(), Err(EditorError::ParentRequired)));
                let initial = editor.size().ok_or("missing size")?;
                unsafe {
                    editor_probe_resize(
                        parent.0,
                        f64::from(initial.width),
                        f64::from(initial.height),
                    );
                    editor.set_parent(Window::from_cocoa_nsview(editor_probe_view(parent.0)))?;
                }
                editor.show()?;
                editor.show()?;
                assert!(editor.shown());
                unsafe {
                    editor_probe_pump(0.2);
                }
                assert!(
                    unsafe { editor_probe_children(parent.0) } > 0,
                    "embedded child absent"
                );
                assert!(matches!(
                    editor.resize(GuiSize {
                        width: 0,
                        height: 100
                    }),
                    Err(EditorError::Size)
                ));
                match editor.resize(GuiSize {
                    width: 640,
                    height: 480,
                }) {
                    Ok(size) => {
                        assert!(size.width > 0 && size.height > 0);
                        unsafe {
                            editor_probe_resize(
                                parent.0,
                                f64::from(size.width),
                                f64::from(size.height),
                            );
                        }
                    }
                    Err(EditorError::Unsupported) => (),
                    other => panic!("resize result {other:?}"),
                }
                editor.hide()?;
                editor.hide()?;
                assert!(!editor.shown());
                editor.show()?;
                unsafe {
                    editor_probe_pump(0.15);
                }
                if cycle == 1 {
                    continue;
                } // Drop also cleans an early return path.
            }
            unsafe {
                editor_probe_pump(0.15);
            }
            assert_eq!(
                unsafe { editor_probe_children(parent.0) },
                0,
                "child leaked after guarded destroy"
            );
        }
        unsafe {
            editor_probe_pump(0.15);
        }
        assert_eq!(unsafe { editor_probe_children(parent.0) }, 0);
        println!(
            "Guarded editor hosting: three create/parent/show/resize/hide/drop cycles passed for {}",
            id.to_string_lossy()
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
    eprintln!("Native guarded editor fixture currently requires macOS");
    std::process::exit(1);
}
