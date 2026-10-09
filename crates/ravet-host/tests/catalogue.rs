use clack_host::prelude::*;
use clack_plugin::entry::SinglePluginEntry;
use ravet_host::{ClapLibrary, HostError, discover_clap};
use std::{fs, path::PathBuf};
struct Temporary(PathBuf);
impl Temporary {
    fn new() -> Self {
        static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let path = std::env::temp_dir().join(format!(
            "ravet-host-{}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos(),
            NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
        ));
        fs::create_dir(&path).unwrap();
        Self(path)
    }
}
impl Drop for Temporary {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.0).unwrap();
    }
}
#[test]
fn explicit_candidates_are_bounded_sorted_unique_and_never_loaded() {
    let dir = Temporary::new();
    fs::write(dir.0.join("z.clap"), b"not native code").unwrap();
    fs::write(dir.0.join("a.CLAP"), b"not native code").unwrap();
    fs::write(dir.0.join("ignore.txt"), b"").unwrap();
    fs::create_dir(dir.0.join("nested")).unwrap();
    fs::write(dir.0.join("nested/hidden.clap"), b"not native code").unwrap();
    #[cfg(unix)]
    {
        std::os::unix::fs::symlink(dir.0.join("z.clap"), dir.0.join("link.clap")).unwrap();
    }
    let paths = discover_clap(&[dir.0.clone(), dir.0.clone()], 2).unwrap();
    assert_eq!(
        paths,
        vec![
            dir.0.join("a.CLAP").canonicalize().unwrap(),
            dir.0.join("z.clap").canonicalize().unwrap()
        ]
    );
    assert!(matches!(
        discover_clap(std::slice::from_ref(&dir.0), 1),
        Err(HostError::Capacity)
    ));
    assert!(discover_clap(&[dir.0.join("missing")], 2).is_err());
    assert!(discover_clap(&[dir.0.join("ignore.txt")], 2).is_err());
    assert!(discover_clap(&[], 0).is_err());
    fs::create_dir(dir.0.join("bundle.clap")).unwrap();
    assert_eq!(
        discover_clap(std::slice::from_ref(&dir.0), 3)
            .unwrap()
            .len(),
        if cfg!(target_os = "macos") { 3 } else { 2 }
    );
    for i in 0..4096 {
        fs::write(dir.0.join(format!("unused-{i}")), b"").unwrap();
    }
    assert!(matches!(
        discover_clap(std::slice::from_ref(&dir.0), 3),
        Err(HostError::Capacity)
    ));
}
struct Host;
struct Shared;
struct Main;
impl HostHandlers for Host {
    type Shared<'a> = Shared;
    type MainThread<'a> = Main;
    type AudioProcessor<'a> = ();
}
impl SharedHandler<'_> for Shared {
    fn request_restart(&self) {}
    fn request_process(&self) {}
    fn request_callback(&self) {}
}
impl MainThreadHandler<'_> for Main {}
#[test]
fn owned_catalogue_and_live_instance_keep_entry_alive_after_wrapper_drop() {
    let entry = PluginEntry::load_from_clack::<SinglePluginEntry<ravet_gain_clap::GainPlugin>>(c"")
        .unwrap();
    let library = ClapLibrary::from_entry(entry).unwrap();
    assert_eq!(library.plugins().len(), 1);
    assert_eq!(
        library.find(c"org.ravet.gain").unwrap().name.as_c_str(),
        c"RAVET Gain"
    );
    assert!(library.find(c"missing").is_none());
    let metadata = library.plugins()[0].clone();
    let host = HostInfo::new("RAVET catalogue fixture", "RAVET", "", "0.1").unwrap();
    let mut instance = PluginInstance::<Host>::new(
        |_| Shared,
        |_| Main,
        library.entry(),
        metadata.id.as_c_str(),
        &host,
    )
    .unwrap();
    drop(library);
    assert_eq!(metadata.id.as_c_str(), c"org.ravet.gain");
    for _ in 0..3 {
        let stopped = instance
            .activate(
                |_, _| (),
                PluginAudioConfiguration {
                    sample_rate: 48000.,
                    min_frames_count: 1,
                    max_frames_count: 16,
                },
            )
            .unwrap();
        let stopped = std::thread::spawn(move || {
            let started = stopped.start_processing().unwrap();
            started.stop_processing()
        })
        .join()
        .unwrap();
        instance.deactivate(stopped);
    }
}

#[test]
fn editor_rejects_absent_extension_and_unsupported_api_before_native_creation() {
    use clack_extensions::gui::{GuiApiType, GuiConfiguration};
    use ravet_host::{EditorError, PluginEditor};
    let entry = PluginEntry::load_from_clack::<SinglePluginEntry<ravet_sine_clap::SinePlugin>>(c"")
        .unwrap();
    let info = HostInfo::new("RAVET editor fixture", "RAVET", "", "0.1").unwrap();
    let mut instance =
        PluginInstance::<()>::new(|_| (), |_| (), &entry, c"org.ravet.sine", &info).unwrap();
    assert!(matches!(
        PluginEditor::create(
            &mut instance,
            GuiConfiguration {
                api_type: GuiApiType::COCOA,
                is_floating: false
            }
        ),
        Err(EditorError::Unavailable)
    ));
    let entry = PluginEntry::load_from_clack::<SinglePluginEntry<ravet_gain_clap::GainPlugin>>(c"")
        .unwrap();
    let mut instance =
        PluginInstance::<()>::new(|_| (), |_| (), &entry, c"org.ravet.gain", &info).unwrap();
    assert!(matches!(
        PluginEditor::create(
            &mut instance,
            GuiConfiguration {
                api_type: GuiApiType(c"RAVET_UNSUPPORTED_UI"),
                is_floating: false
            }
        ),
        Err(EditorError::Unsupported) | Err(EditorError::Unavailable)
    ));
}
