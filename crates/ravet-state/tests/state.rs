use ravet_state::{
    Change, Limits, Mutation, Node, NodeId, Notifications, StateError, Store, Value,
};
fn root() -> Node {
    Node::new(NodeId(1), "session")
        .with_property("gain", Value::Real(1.0))
        .with_child(Node::new(NodeId(2), "track").with_child(Node::new(NodeId(3), "effect")))
        .with_child(Node::new(NodeId(4), "track"))
}
fn gain(value: f64) -> Mutation {
    Mutation::Set {
        node: NodeId(1),
        key: "gain".into(),
        value: Value::Real(value),
    }
}
#[test]
fn atomic_edits_undo_redo_and_failed_branch_preservation() {
    let mut store = Store::new(root(), Limits::default()).unwrap();
    let mut cursor = store.subscribe();
    let initial = store.root().clone();
    assert!(
        store
            .commit(&[
                gain(2.),
                Mutation::Move {
                    node: NodeId(3),
                    parent: NodeId(4),
                    index: 0
                }
            ])
            .unwrap()
    );
    assert!(store.root().find(NodeId(2)).unwrap().children().is_empty());
    assert_eq!(
        store.root().find(NodeId(4)).unwrap().children()[0].id(),
        NodeId(3)
    );
    let edited = store.root().clone();
    assert!(store.undo().unwrap());
    assert_eq!(store.root(), &initial);
    let revision = store.revision();
    assert_eq!(
        store.commit(&[gain(3.), Mutation::Remove { node: NodeId(99) }]),
        Err(StateError::MissingNode(NodeId(99)))
    );
    assert_eq!(store.root(), &initial);
    assert_eq!(store.revision(), revision);
    assert!(store.can_redo());
    assert!(!store.commit(&[gain(1.)]).unwrap());
    assert!(store.can_redo());
    assert!(store.redo().unwrap());
    assert_eq!(store.root(), &edited);
    match store.poll(&mut cursor) {
        Notifications::Changes(batches) => {
            assert_eq!(batches.len(), 3);
            assert_eq!(batches[0].revision, 1);
            assert!(matches!(batches[0].change, Change::Edits(_)));
            assert_eq!(batches[1].change, Change::Undo);
            assert_eq!(batches[2].change, Change::Redo);
        }
        _ => panic!("unexpected resync"),
    }
    assert!(matches!(store.poll(&mut cursor), Notifications::Changes(v) if v.is_empty()));
    store.undo().unwrap();
    store.commit(&[gain(0.5)]).unwrap();
    assert!(!store.can_redo());
}
#[test]
fn bounded_history_journal_overrun_and_structural_rejection() {
    let limits = Limits {
        history: 2,
        journal: 2,
        ..Limits::default()
    };
    let mut store = Store::new(root(), limits).unwrap();
    let mut slow = store.subscribe();
    for v in [2., 3., 4.] {
        store.commit(&[gain(v)]).unwrap();
    }
    assert!(
        matches!(store.poll(&mut slow), Notifications::Resync { revision: 3, root } if root == store.root())
    );
    assert!(store.undo().unwrap());
    assert!(store.undo().unwrap());
    assert!(!store.undo().unwrap());
    assert_eq!(store.root().properties()["gain"], Value::Real(2.));
    assert!(store.redo().unwrap());
    assert!(store.redo().unwrap());
    assert!(!store.redo().unwrap());
    let before = store.root().clone();
    let revision = store.revision();
    for edit in [
        Mutation::Move {
            node: NodeId(2),
            parent: NodeId(3),
            index: 0,
        },
        Mutation::Remove { node: NodeId(1) },
        Mutation::Insert {
            parent: NodeId(1),
            index: 99,
            child: Node::new(NodeId(5), "x"),
        },
        Mutation::Insert {
            parent: NodeId(1),
            index: 0,
            child: Node::new(NodeId(3), "duplicate"),
        },
        gain(f64::NAN),
    ] {
        assert!(store.commit(&[edit]).is_err());
        assert_eq!(store.root(), &before);
        assert_eq!(store.revision(), revision);
    }
}
#[test]
fn deterministic_strict_preset_and_every_truncated_prefix() {
    let node = root()
        .with_property("text", Value::Text("音楽".into()))
        .with_property("bytes", Value::Bytes(vec![0, 255, 12]))
        .with_property("null", Value::Null)
        .with_property("flag", Value::Bool(true))
        .with_property("integer", Value::Integer(i64::MIN));
    let store = Store::new(node, Limits::default()).unwrap();
    let bytes = store.encode(*b"TESTTREE");
    let decoded = Store::decode(&bytes, *b"TESTTREE", Limits::default()).unwrap();
    assert_eq!(decoded.root(), store.root());
    assert_eq!(decoded.revision(), 0);
    assert!(!decoded.can_undo());
    assert_eq!(decoded.encode(*b"TESTTREE"), bytes);
    for length in 0..bytes.len() {
        assert!(
            Store::decode(&bytes[..length], *b"TESTTREE", Limits::default()).is_err(),
            "prefix {length}"
        );
    }
    assert!(matches!(
        Store::decode(&bytes, *b"WRONG_ID", Limits::default()),
        Err(StateError::WrongProduct)
    ));
    let mut trailing = bytes.clone();
    trailing.push(0);
    assert!(Store::decode(&trailing, *b"TESTTREE", Limits::default()).is_err());
    let mut version = bytes;
    version[16] = 2;
    assert!(Store::decode(&version, *b"TESTTREE", Limits::default()).is_err());
}
#[test]
fn preset_duplicate_property_nonfinite_and_depth_capacity() {
    // Independent hand-built record: one root property with a nonfinite f64.
    let mut bytes = b"RVETTREETESTTREE".to_vec();
    bytes.extend(1u32.to_le_bytes());
    bytes.extend(1u64.to_le_bytes());
    bytes.extend(1u32.to_le_bytes());
    bytes.push(b'r');
    bytes.extend(1u32.to_le_bytes());
    bytes.extend(1u32.to_le_bytes());
    bytes.push(b'x');
    bytes.push(3);
    bytes.extend(f64::INFINITY.to_le_bytes());
    bytes.extend(0u32.to_le_bytes());
    assert!(Store::decode(&bytes, *b"TESTTREE", Limits::default()).is_err());
    bytes[43..51].copy_from_slice(&2.0f64.to_le_bytes());
    assert!(Store::decode(&bytes, *b"TESTTREE", Limits::default()).is_ok());
    // Duplicate keys are rejected even when finite values match.
    let first = bytes[37..51].to_vec();
    bytes[33..37].copy_from_slice(&2u32.to_le_bytes());
    bytes.splice(51..51, first);
    assert!(Store::decode(&bytes, *b"TESTTREE", Limits::default()).is_err());
    assert!(
        Store::new(
            root(),
            Limits {
                depth: 2,
                ..Limits::default()
            }
        )
        .is_err()
    );
    assert!(
        Store::new(
            root(),
            Limits {
                nodes: 3,
                ..Limits::default()
            }
        )
        .is_err()
    );
    let mut store = Store::new(
        Node::new(NodeId(1), "r"),
        Limits {
            bytes: 128,
            ..Limits::default()
        },
    )
    .unwrap();
    let before = store.encode(*b"TESTTREE");
    assert_eq!(
        store.commit(&[Mutation::Set {
            node: NodeId(1),
            key: "data".into(),
            value: Value::Bytes(vec![0; 128])
        }]),
        Err(StateError::Capacity)
    );
    assert_eq!(store.encode(*b"TESTTREE"), before);
    assert!(
        Store::new(
            root(),
            Limits {
                journal: 0,
                ..Limits::default()
            }
        )
        .is_err()
    );
}

#[test]
fn undoable_preset_restore_and_payload_bounds() {
    let mut store = Store::new(root(), Limits::default()).unwrap();
    let preset = store.encode(*b"TESTTREE");
    store.commit(&[gain(0.25)]).unwrap();
    let edited = store.root().clone();
    assert!(store.restore(&preset, *b"TESTTREE").unwrap());
    assert_eq!(store.root(), &root());
    assert!(!store.restore(&preset, *b"TESTTREE").unwrap());
    store.undo().unwrap();
    assert_eq!(store.root(), &edited);
    let revision = store.revision();
    assert!(store.restore(&preset, *b"WRONG_ID").is_err());
    assert_eq!(store.revision(), revision);
    assert!(store.can_redo());
    let mut bounded = Store::new(
        Node::new(NodeId(1), "r"),
        Limits {
            bytes: 128,
            history: 0,
            ..Limits::default()
        },
    )
    .unwrap();
    assert_eq!(
        bounded.commit(&vec![gain(1.); 20]),
        Err(StateError::Capacity)
    );
    assert_eq!(bounded.revision(), 0);
    bounded.commit(&[gain(2.)]).unwrap();
    assert!(!bounded.can_undo());
    let simple = Store::new(
        Node::new(NodeId(1), "r").with_child(Node::new(NodeId(2), "c")),
        Limits::default(),
    )
    .unwrap();
    let mut duplicate = simple.encode(*b"TESTTREE");
    duplicate[41..49].copy_from_slice(&1u64.to_le_bytes());
    assert!(matches!(
        Store::decode(&duplicate, *b"TESTTREE", Limits::default()),
        Err(StateError::DuplicateId(NodeId(1)))
    ));
}
