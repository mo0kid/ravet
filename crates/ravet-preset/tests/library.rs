use ravet_preset::{PresetError, PresetLibrary, validate_name};
use ravet_state::{Limits, Mutation, Node, NodeId, Store, Value};
use std::{fs, path::PathBuf};
struct Directory(PathBuf);
impl Directory {
    fn new() -> Self {
        static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let stamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let sequence = NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!(
            "ravet-preset-test-{}-{stamp}-{sequence}",
            std::process::id()
        ));
        fs::create_dir(&path).unwrap();
        Self(path)
    }
}
impl Drop for Directory {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.0).unwrap();
    }
}
fn state(value: f64) -> Store {
    Store::new(
        Node::new(NodeId(1), "session").with_property("gain", Value::Real(value)),
        Limits::default(),
    )
    .unwrap()
}
#[test]
fn sorted_discovery_atomic_save_and_complete_undoable_load() {
    let directory = Directory::new();
    let library = PresetLibrary::open(&directory.0, *b"PRODUCT1", Limits::default()).unwrap();
    let mut state = state(2.);
    library.save("Warm Pad", &state).unwrap();
    state
        .commit(&[Mutation::Set {
            node: NodeId(1),
            key: "gain".into(),
            value: Value::Real(0.5),
        }])
        .unwrap();
    library.save("Cold Pad", &state).unwrap();
    library.save("Cold Pad", &state).unwrap();
    fs::write(directory.0.join("notes.txt"), b"unrelated").unwrap();
    fs::create_dir(directory.0.join("Nested.ravetpreset")).unwrap();
    #[cfg(unix)]
    std::os::unix::fs::symlink(
        directory.0.join("Cold Pad.ravetpreset"),
        directory.0.join("Link.ravetpreset"),
    )
    .unwrap();
    let entries = library.discover().unwrap();
    assert_eq!(
        entries
            .iter()
            .map(|entry| entry.name.as_str())
            .collect::<Vec<_>>(),
        ["Cold Pad", "Warm Pad"]
    );
    assert!(entries.iter().all(|entry| entry.encoded_bytes > 32));
    assert!(library.load_into("Warm Pad", &mut state).unwrap());
    assert_eq!(state.root().properties()["gain"], Value::Real(2.));
    state.undo().unwrap();
    assert_eq!(state.root().properties()["gain"], Value::Real(0.5));
    let revision = state.revision();
    assert!(!library.load_into("Cold Pad", &mut state).unwrap());
    assert_eq!(state.revision(), revision);
}
#[test]
fn malformed_missing_foreign_and_destination_bounds_preserve_live_history() {
    let directory = Directory::new();
    let library = PresetLibrary::open(&directory.0, *b"PRODUCT1", Limits::default()).unwrap();
    let mut state = state(1.);
    state
        .commit(&[Mutation::Set {
            node: NodeId(1),
            key: "gain".into(),
            value: Value::Real(2.),
        }])
        .unwrap();
    state.undo().unwrap();
    let before = state.encode(*b"PRODUCT1");
    let revision = state.revision();
    fs::write(directory.0.join("Malformed.ravetpreset"), b"incomplete").unwrap();
    fs::write(
        directory.0.join("Foreign.ravetpreset"),
        state.encode(*b"PRODUCT2"),
    )
    .unwrap();
    for name in ["Missing", "Malformed", "Foreign", "../escape"] {
        assert!(library.load_into(name, &mut state).is_err());
        assert_eq!(state.encode(*b"PRODUCT1"), before);
        assert_eq!(state.revision(), revision);
        assert!(state.can_redo());
    }
    let large = Store::new(
        Node::new(NodeId(7), "large").with_property("text", Value::Text("x".repeat(128))),
        Limits::default(),
    )
    .unwrap();
    library.save("Large", &large).unwrap();
    let limits = Limits {
        bytes: 64,
        ..Limits::default()
    };
    let mut small = Store::new(Node::new(NodeId(9), "small"), limits).unwrap();
    let before = small.encode(*b"PRODUCT1");
    assert!(library.load_into("Large", &mut small).is_err());
    assert_eq!(small.encode(*b"PRODUCT1"), before);
    assert_eq!(small.revision(), 0);
    let bounded = PresetLibrary::open(&directory.0, *b"PRODUCT1", limits).unwrap();
    let old = fs::read(directory.0.join("Large.ravetpreset")).unwrap();
    assert!(bounded.save("Large", &large).is_err());
    assert_eq!(
        fs::read(directory.0.join("Large.ravetpreset")).unwrap(),
        old
    );
    assert!(matches!(bounded.discover(), Err(PresetError::Capacity)));
}
#[test]
fn explicit_directory_name_policy_and_total_scan_bound() {
    let directory = Directory::new();
    for name in [
        "", ".", "..", "../a", "a/b", "a\\b", " x", "x ", "NUL", "com1", "LPT9", "a:b", "雪",
    ] {
        assert!(validate_name(name).is_err(), "{name}");
    }
    for name in ["Warm Pad 2", "A_b-C", "COM10", "con-pad"] {
        validate_name(name).unwrap();
    }
    assert!(validate_name(&"x".repeat(129)).is_err());
    assert!(
        PresetLibrary::open(directory.0.join("missing"), *b"PRODUCT1", Limits::default()).is_err()
    );
    let library = PresetLibrary::open(&directory.0, *b"PRODUCT1", Limits::default()).unwrap();
    for index in 0..4097 {
        fs::write(directory.0.join(format!("unrelated-{index}")), b"").unwrap();
    }
    assert!(matches!(library.discover(), Err(PresetError::Capacity)));
}
