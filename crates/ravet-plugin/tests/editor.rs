use ravet_plugin::{
    Category, Parameter, Product, StateFormat,
    editor::{self, EditorMessage, GestureError, Gestures, ProtocolError},
};
use serde_json::{Value, json};

const PARAMETERS: &[Parameter] = &[
    Parameter::linear("gain_db", "Gain", -60.0, 12.0, 0.0).with_unit("dB"),
    Parameter::stepped("mode", "Mode", 0.0, 3.0, 3, 1.0),
    Parameter::logarithmic("cutoff", "Cutoff", 20.0, 20_000.0, 1_000.0).with_id(7),
    Parameter::bypass("bypass", "Bypass"),
    Parameter::linear("level", "Level", 0.0, 1.0, 0.0).read_only(),
];
const PRODUCT: Product = Product::new(
    "Example",
    "Example Audio",
    "1.2.3",
    "com.example.editor",
    Category::Effect,
    StateFormat {
        tag: *b"EXEDIT01",
        version: 1,
    },
)
.with_parameters(PARAMETERS);

#[test]
fn editor_messages_are_validated_by_key() {
    let product = PRODUCT.resolve().unwrap();
    let parse = |text: &str| editor::parse(&product, text);
    assert_eq!(
        parse(r#"{"type":"ready","protocol":1}"#),
        Ok(EditorMessage::Ready)
    );
    assert_eq!(
        parse(r#"{"type":"begin","key":"gain_db"}"#),
        Ok(EditorMessage::Begin { index: 0 })
    );
    assert_eq!(
        parse(r#"{"type":"set","key":"gain_db","value":-6.5}"#),
        Ok(EditorMessage::Set {
            index: 0,
            value: -6.5
        })
    );
    assert_eq!(
        parse(r#"{"type":"end","key":"gain_db"}"#),
        Ok(EditorMessage::End { index: 0 })
    );
    // Physical values clamp and snap; normalized values map through the range.
    assert_eq!(
        parse(r#"{"type":"commit","key":"mode","value":2.4}"#),
        Ok(EditorMessage::Commit {
            index: 1,
            value: 2.0
        })
    );
    assert_eq!(
        parse(r#"{"type":"commit","key":"gain_db","value":100}"#),
        Ok(EditorMessage::Commit {
            index: 0,
            value: 12.0
        })
    );
    assert_eq!(
        parse(r#"{"type":"commit","key":"cutoff","normalized":1}"#),
        Ok(EditorMessage::Commit {
            index: 2,
            value: 20_000.0
        })
    );
    assert_eq!(
        parse(r#"{"type":"product","data":{"preset":"warm"}}"#),
        Ok(EditorMessage::Product(json!({"preset": "warm"})))
    );

    assert_eq!(
        parse(r#"{"type":"ready","protocol":2}"#),
        Err(ProtocolError::UnsupportedVersion(2))
    );
    assert_eq!(parse(r#"{"type":"ready"}"#), Err(ProtocolError::Malformed));
    assert_eq!(parse("[1]"), Err(ProtocolError::Malformed));
    assert_eq!(parse("not json"), Err(ProtocolError::Malformed));
    assert_eq!(
        parse(r#"{"type":"wobble"}"#),
        Err(ProtocolError::UnknownType("wobble".into()))
    );
    assert_eq!(
        parse(r#"{"type":"begin","key":"nope"}"#),
        Err(ProtocolError::UnknownKey("nope".into()))
    );
    assert_eq!(
        parse(r#"{"type":"commit","key":"level","value":0.5}"#),
        Err(ProtocolError::ReadOnly("level".into()))
    );
    assert_eq!(
        parse(r#"{"type":"set","key":"gain_db","value":"loud"}"#),
        Err(ProtocolError::Malformed)
    );
    assert_eq!(
        parse(r#"{"type":"set","key":"gain_db","value":1,"normalized":0.5}"#),
        Err(ProtocolError::Malformed)
    );
    assert_eq!(
        parse(r#"{"type":"set","key":"gain_db"}"#),
        Err(ProtocolError::Malformed)
    );
    assert_eq!(
        parse(&format!(
            r#"{{"type":"product","data":"{}"}}"#,
            "x".repeat(65_536)
        )),
        Err(ProtocolError::TooLarge)
    );
}

#[test]
fn gestures_must_balance_and_close_with_the_editor() {
    let product = PRODUCT.resolve().unwrap();
    let mut gestures = Gestures::new(&product);
    let set = EditorMessage::Set {
        index: 0,
        value: 1.0,
    };
    assert_eq!(gestures.apply(&set), Err(GestureError::NotOpen(0)));
    assert_eq!(
        gestures.apply(&EditorMessage::End { index: 0 }),
        Err(GestureError::NotOpen(0))
    );
    gestures.apply(&EditorMessage::Begin { index: 0 }).unwrap();
    gestures.apply(&EditorMessage::Begin { index: 2 }).unwrap();
    assert_eq!(
        gestures.apply(&EditorMessage::Begin { index: 0 }),
        Err(GestureError::AlreadyOpen(0))
    );
    gestures.apply(&set).unwrap();
    assert_eq!(
        gestures.apply(&EditorMessage::Commit {
            index: 0,
            value: 1.0
        }),
        Err(GestureError::AlreadyOpen(0))
    );
    gestures
        .apply(&EditorMessage::Commit {
            index: 1,
            value: 1.0,
        })
        .unwrap();
    gestures.apply(&EditorMessage::End { index: 0 }).unwrap();
    assert!(gestures.is_open(2));
    assert_eq!(gestures.close_all(), [2]);
    assert!(!gestures.is_open(2));
    assert!(gestures.close_all().is_empty());
}

#[test]
fn outgoing_messages_describe_and_snapshot_the_product() {
    let product = PRODUCT.resolve().unwrap();
    let info: Value = serde_json::from_str(&editor::info(&product)).unwrap();
    assert_eq!(info["type"], "info");
    assert_eq!(info["protocol"], 1);
    assert_eq!(info["product"]["version"], "1.2.3");
    assert_eq!(info["product"]["category"], "effect");
    let parameters = info["parameters"].as_array().unwrap();
    assert_eq!(parameters.len(), 5);
    assert_eq!(parameters[0]["key"], "gain_db");
    assert_eq!(parameters[0]["unit"], "dB");
    assert_eq!(parameters[1]["steps"], 3);
    assert_eq!(parameters[2]["id"], 7);
    assert_eq!(parameters[2]["scale"], "logarithmic");
    assert_eq!(parameters[3]["bypass"], true);
    assert_eq!(parameters[4]["readOnly"], true);

    let snapshot: Value = serde_json::from_str(&editor::snapshot(
        &product,
        5,
        &[-6.0, 2.0, 1_000.0, 1.0, 0.25],
        Some(&json!({"peak": [0.5, 0.25]})),
        Some(&json!("custom")),
    ))
    .unwrap();
    assert_eq!(
        snapshot,
        json!({
            "type": "snapshot",
            "sequence": 5,
            "parameters": {"gain_db": -6.0, "mode": 2.0, "cutoff": 1000.0, "bypass": 1.0, "level": 0.25},
            "telemetry": {"peak": [0.5, 0.25]},
            "product": "custom",
        })
    );

    let changes: Value = serde_json::from_str(&editor::changes(&product, 6, &[(1, 3.0)])).unwrap();
    assert_eq!(
        changes,
        json!({"type": "changes", "sequence": 6, "parameters": {"mode": 3.0}})
    );
    let message: Value = serde_json::from_str(&editor::product_message(&json!([1, 2]))).unwrap();
    assert_eq!(message, json!({"type": "product", "data": [1, 2]}));
}
