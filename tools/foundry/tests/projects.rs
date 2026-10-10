use ravet_foundry::{
    Controller, DirectoryRequest, Format, Kind, PluginType, Project, create, framework_root, load,
    regenerate,
};
use std::{
    fs,
    path::PathBuf,
    sync::atomic::{AtomicU64, Ordering},
    thread,
    time::{Duration, Instant},
};
static NEXT: AtomicU64 = AtomicU64::new(0);
struct Temp(PathBuf);
impl Temp {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!(
            "ravet-foundry-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&path).unwrap();
        Self(path)
    }
    fn project(&self) -> PathBuf {
        self.0.join("consumer")
    }
}

#[test]
fn directory_choices_preserve_cancelled_fields_and_do_not_save_project_settings() {
    let temp = Temp::new();
    let sdk = temp.0.join("SDK folder with spaces and 音");
    fs::create_dir(&sdk).unwrap();
    let root = create(&temp.project(), settings(Kind::Plugin)).unwrap();
    let before = fs::read(root.join("ravet-project.toml")).unwrap();
    let request=DirectoryRequest::from_message(&serde_json::json!({"action":"browse","field":"sdk-vst3","initial":sdk.join("not-yet-created")}).to_string()).unwrap().unwrap();
    assert_eq!(
        request.initial_directory().unwrap(),
        sdk.canonicalize().unwrap()
    );
    let mut controller = Controller::default();
    controller.directory_selected(&request, None).unwrap();
    assert!(controller.snapshot().directory_selection.is_none());
    controller
        .directory_selected(&request, Some(sdk.clone()))
        .unwrap();
    let selected = controller.snapshot().directory_selection.unwrap();
    assert_eq!(selected.path, sdk.canonicalize().unwrap());
    assert_eq!(selected.field, "sdk-vst3");
    controller.directory_selected(&request, None).unwrap();
    assert_eq!(
        controller.snapshot().directory_selection.unwrap().sequence,
        selected.sequence
    );
    controller.directory_selected(&request, Some(sdk)).unwrap();
    assert_eq!(
        controller.snapshot().directory_selection.unwrap().sequence,
        selected.sequence + 1
    );
    assert!(
        controller
            .directory_selected(&request, Some(root.join("ravet-project.toml")))
            .is_err()
    );
    assert_eq!(fs::read(root.join("ravet-project.toml")).unwrap(), before);
    assert!(
        DirectoryRequest::from_message(r#"{"action":"browse","field":"name","initial":"/"}"#)
            .is_err()
    );
    assert!(
        DirectoryRequest::from_message(
            r#"{"action":"browse","field":"framework","initial":"/","extra":true}"#
        )
        .is_err()
    );
    assert!(DirectoryRequest::from_message(&"x".repeat(16_385)).is_err());
    assert!(
        DirectoryRequest::from_message(r#"{"action":"doctor"}"#)
            .unwrap()
            .is_none()
    );
}
impl Drop for Temp {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.0).unwrap();
    }
}
fn settings(kind: Kind) -> Project {
    Project::new("consumer-project", kind)
}
#[test]
fn each_template_is_independent_and_reopens() {
    for kind in [Kind::Offline, Kind::Standalone, Kind::Plugin] {
        let temp = Temp::new();
        let root = create(&temp.project(), settings(kind)).unwrap();
        let project = load(&root).unwrap();
        assert_eq!(project.kind, kind);
        let manifest: toml::Value =
            toml::from_str(&fs::read_to_string(root.join("Cargo.toml")).unwrap()).unwrap();
        assert_eq!(manifest["package"]["publish"].as_bool(), Some(false));
        assert!(manifest["workspace"].as_table().unwrap().is_empty());
        assert!(root.join("LICENSE-RAVET-TEMPLATE").is_file());
        assert!(!root.join("Cargo.lock").exists());
        assert!(!root.join("target").exists());
        assert!(!root.starts_with(framework_root()));
        if kind == Kind::Plugin {
            let source = fs::read_to_string(root.join("src/lib.rs")).unwrap();
            assert!(source.contains("env!(\"RAVET_PLUGIN_ID\")"));
            assert!(!source.contains("mod control_tests"));
        }
        if kind == Kind::Standalone {
            let source = fs::read_to_string(root.join("src/ui/mod.rs")).unwrap();
            assert!(source.contains("env!(\"RAVET_PRODUCT_NAME\")"));
            assert!(source.contains("StandaloneOptions::default()"));
            assert!(
                !source
                    .split("fn probe(")
                    .next()
                    .unwrap()
                    .contains("io_settings: false")
            );
        }
    }
}
#[test]
fn destinations_and_validation_reject_without_overwriting() {
    let temp = Temp::new();
    let root = create(&temp.project(), settings(Kind::Offline)).unwrap();
    let original = fs::read(root.join("src/main.rs")).unwrap();
    assert!(create(&root, settings(Kind::Offline)).is_err());
    assert_eq!(fs::read(root.join("src/main.rs")).unwrap(), original);
    assert!(
        create(
            &framework_root().join("foundry-must-not-exist"),
            settings(Kind::Offline)
        )
        .is_err()
    );
    assert!(!framework_root().join("foundry-must-not-exist").exists());
    assert!(create(&temp.0.join("missing/child"), settings(Kind::Offline)).is_err());
    for name in ["", "../escape", "Uppercase", "a space", "std"] {
        let mut p = settings(Kind::Offline);
        p.name = name.into();
        assert!(create(&temp.0.join("invalid"), p).is_err());
        assert!(!temp.0.join("invalid").exists());
    }
    let mut p = settings(Kind::Offline);
    p.version = "01.0.0".into();
    assert!(create(&temp.0.join("invalid"), p).is_err());
    let mut p = settings(Kind::Offline);
    p.display_name = "bad\nline".into();
    assert!(create(&temp.0.join("invalid"), p).is_err());
}
#[test]
fn settings_escaping_roundtrips_without_injecting_manifests() {
    let temp = Temp::new();
    let mut p = settings(Kind::Plugin);
    p.display_name = "A \"product\" & <sound> \\ 音".into();
    p.company = "Company \\ \"quoted\"".into();
    let root = create(&temp.project(), p.clone()).unwrap();
    assert_eq!(load(&root).unwrap(), p);
    let build = fs::read_to_string(root.join("build.rs")).unwrap();
    assert!(build.contains("\\\"product\\\""));
    let _: toml::Value =
        toml::from_str(&fs::read_to_string(root.join("Cargo.toml")).unwrap()).unwrap();
}
#[test]
fn regeneration_preserves_owned_source_and_lockfile() {
    let temp = Temp::new();
    let root = create(&temp.project(), settings(Kind::Plugin)).unwrap();
    fs::write(root.join("src/lib.rs"), "// My DSP and plugin source\n").unwrap();
    fs::write(root.join("Cargo.lock"), "consumer lockfile").unwrap();
    fs::write(root.join("editor/index.html"), "<body>My UI</body>").unwrap();
    let mut p = load(&root).unwrap();
    p.display_name = "Renamed product".into();
    p.company = "Another company".into();
    p.version = "2.0.1".into();
    p.native_editor = false;
    regenerate(&root, p.clone()).unwrap();
    assert_eq!(load(&root).unwrap(), p);
    assert_eq!(
        fs::read_to_string(root.join("src/lib.rs")).unwrap(),
        "// My DSP and plugin source\n"
    );
    assert_eq!(
        fs::read_to_string(root.join("editor/index.html")).unwrap(),
        "<body>My UI</body>"
    );
    assert_eq!(
        fs::read_to_string(root.join("Cargo.lock")).unwrap(),
        "consumer lockfile"
    );
    let manifest = fs::read_to_string(root.join("Cargo.toml")).unwrap();
    assert!(manifest.contains("version = \"2.0.1\""));
    assert!(manifest.contains("default = []"));
}
#[test]
fn conflict_preflight_preserves_every_file() {
    for file in ["Cargo.toml", "build.rs"] {
        let temp = Temp::new();
        let root = create(&temp.project(), settings(Kind::Offline)).unwrap();
        let config = fs::read(root.join("ravet-project.toml")).unwrap();
        let managed = fs::read(root.join(".foundry/managed.json")).unwrap();
        fs::write(root.join(file), "// Consumer edit").unwrap();
        let mut p = load(&root).unwrap();
        p.company = "Changed".into();
        assert!(
            regenerate(&root, p)
                .unwrap_err()
                .to_string()
                .contains("conflict")
        );
        assert_eq!(fs::read(root.join("ravet-project.toml")).unwrap(), config);
        assert_eq!(
            fs::read(root.join(".foundry/managed.json")).unwrap(),
            managed
        );
        assert_eq!(
            fs::read_to_string(root.join(file)).unwrap(),
            "// Consumer edit"
        );
    }
}
#[test]
fn immutable_identity_unknown_schema_and_operation_lock_reject() {
    let temp = Temp::new();
    let root = create(&temp.project(), settings(Kind::Offline)).unwrap();
    let mut p = load(&root).unwrap();
    p.kind = Kind::Plugin;
    assert!(regenerate(&root, p).is_err());
    let mut p = load(&root).unwrap();
    p.name = "new-name".into();
    assert!(regenerate(&root, p).is_err());
    fs::write(root.join(".foundry/operation.lock"), "").unwrap();
    assert!(regenerate(&root, load(&root).unwrap()).is_err());
    let config = fs::read_to_string(root.join("ravet-project.toml")).unwrap();
    fs::write(
        root.join("ravet-project.toml"),
        config.replace("schema_version = 1", "schema_version = 2"),
    )
    .unwrap();
    assert!(load(&root).is_err());
    fs::write(
        root.join("ravet-project.toml"),
        format!("{config}\nunknown_setting = true\n"),
    )
    .unwrap();
    assert!(load(&root).is_err());
}
#[cfg(unix)]
#[test]
fn symlinks_cannot_redirect_generation_or_managed_writes() {
    use std::os::unix::fs::symlink;
    let temp = Temp::new();
    symlink(framework_root(), temp.0.join("framework-alias")).unwrap();
    assert!(
        create(
            &temp.0.join("framework-alias/no-consumer"),
            settings(Kind::Offline)
        )
        .is_err()
    );
    let root = create(&temp.project(), settings(Kind::Offline)).unwrap();
    let victim = temp.0.join("victim");
    fs::write(&victim, "preserve").unwrap();
    fs::remove_file(root.join("Cargo.toml")).unwrap();
    symlink(&victim, root.join("Cargo.toml")).unwrap();
    assert!(regenerate(&root, load(&root).unwrap()).is_err());
    assert_eq!(fs::read_to_string(&victim).unwrap(), "preserve");
    fs::remove_file(root.join("Cargo.toml")).unwrap();
    fs::remove_dir_all(root.join(".foundry")).unwrap();
    symlink(&temp.0, root.join(".foundry")).unwrap();
    assert!(regenerate(&root, load(&root).unwrap()).is_err());
}
#[test]
fn controller_actions_report_failures_and_preserve_open_project() {
    let temp = Temp::new();
    let mut controller = Controller::default();
    controller.message(&serde_json::json!({"action":"create","destination":temp.project(),"project":settings(Kind::Offline)}).to_string()).unwrap();
    let deadline = Instant::now() + Duration::from_secs(5);
    while controller.snapshot().busy {
        assert!(Instant::now() < deadline);
        thread::sleep(Duration::from_millis(10));
    }
    let state = controller.snapshot();
    assert!(state.error.is_none());
    assert_eq!(state.project.unwrap().name, "consumer-project");
    assert!(controller.message("{bad").is_err());
    assert!(controller.snapshot().error.is_some());
    assert!(controller.snapshot().root.is_some());
    assert!(controller.message(&"x".repeat(16_385)).is_err());
    assert!(
        controller
            .message("{\"action\":\"doctor\",\"unexpected\":true}")
            .is_err()
    );
}

#[test]
fn instruments_generate_native_adapters_and_keep_class_ids_on_metadata_changes() {
    let temp = Temp::new();
    let mut p = settings(Kind::Plugin);
    p.plugin_type = PluginType::Instrument;
    p.formats = vec![Format::Clap, Format::Vst3, Format::Au, Format::Lv2];
    let root = create(&temp.project(), p).unwrap();
    assert!(
        fs::read_to_string(root.join("src/lib.rs"))
            .unwrap()
            .contains("SinePlugin")
    );
    let native = fs::read_to_string(root.join("native/plugin.cpp")).unwrap();
    assert!(native.contains("Instrument|Synth"));
    let header = fs::read_to_string(root.join(".foundry/Product.hpp")).unwrap();
    let ids: Vec<_> = header
        .lines()
        .filter(|s| s.contains("_ID 0x"))
        .map(str::to_owned)
        .collect();
    assert_eq!(ids.len(), 2);
    let mut next = load(&root).unwrap();
    next.formats = vec![Format::Vst3];
    next.display_name = "Renamed instrument".into();
    next.version = "1.2.3".into();
    regenerate(&root, next.clone()).unwrap();
    assert_eq!(
        fs::read_to_string(root.join("native/plugin.cpp")).unwrap(),
        native
    );
    assert_eq!(
        fs::read_to_string(root.join(".foundry/Product.hpp"))
            .unwrap()
            .lines()
            .filter(|s| s.contains("_ID 0x"))
            .collect::<Vec<_>>(),
        ids
    );
    next.plugin_type = PluginType::Effect;
    assert!(regenerate(&root, next).is_err());
    let mut unsupported = load(&root).unwrap();
    unsupported.formats.push(Format::Aax);
    assert!(unsupported.validate().is_err());
}

#[test]
fn missing_native_sdk_fails_before_dependency_resolution() {
    let temp = Temp::new();
    let mut p = settings(Kind::Plugin);
    p.formats = vec![Format::Vst3];
    let root = create(&temp.project(), p).unwrap();
    let mut controller = Controller::default();
    controller
        .message(
            &serde_json::json!({"action":"build","root":root,"format":"vst3","offline":true})
                .to_string(),
        )
        .unwrap();
    let deadline = Instant::now() + Duration::from_secs(5);
    while controller.snapshot().busy {
        assert!(Instant::now() < deadline);
        thread::sleep(Duration::from_millis(10));
    }
    assert!(
        controller
            .snapshot()
            .error
            .unwrap()
            .contains("VST3 SDK path")
    );
    assert!(!root.join("Cargo.lock").exists());
    assert!(!root.join("target").exists());
}
