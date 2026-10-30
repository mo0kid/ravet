use ravet_foundation::ParameterId;
use ravet_plugin::{
    Category, Parameter, Product, StateFormat,
    state::{self, NoHooks, Restored, SavedState, Source, StateError, StateHooks},
};

const V1: StateFormat = StateFormat {
    tag: *b"EXDELAY1",
    version: 1,
};
const V2: StateFormat = StateFormat { version: 2, ..V1 };
const PARAMETERS: &[Parameter] = &[
    Parameter::linear("time", "Time", 0.0, 2.0, 0.5).with_id(1),
    Parameter::stepped("mode", "Mode", 0.0, 3.0, 3, 0.0).with_id(2),
    Parameter::linear("level", "Level", 0.0, 1.0, 0.0)
        .with_id(3)
        .read_only(),
];
const DELAY: Product = Product::new(
    "Delay",
    "Example",
    "1.0.0",
    "com.example.delay",
    Category::Effect,
    V1,
)
.with_parameters(PARAMETERS);

#[test]
fn envelope_bytes_are_stable_and_roundtrip() {
    let product = DELAY.resolve().unwrap();
    let bytes = state::encode(&product, &[1.25, 2.0, 0.75], b"xy").unwrap();
    let mut expected = Vec::new();
    expected.extend_from_slice(b"RAVETSTA");
    expected.extend_from_slice(&[1, 0, 0, 0]);
    expected.extend_from_slice(b"EXDELAY1");
    expected.extend_from_slice(&1u32.to_le_bytes());
    expected.extend_from_slice(&2u32.to_le_bytes()); // read-only "level" is not saved
    for (id, value) in [(1u32, 1.25f32), (2, 2.0)] {
        expected.extend_from_slice(&id.to_le_bytes());
        expected.extend_from_slice(&value.to_le_bytes());
    }
    expected.extend_from_slice(&2u32.to_le_bytes());
    expected.extend_from_slice(b"xy");
    assert_eq!(bytes, expected);

    let restored = state::restore(&product, &bytes, &NoHooks).unwrap();
    assert_eq!(
        restored,
        Restored {
            values: vec![1.25, 2.0, 0.0],
            extra: b"xy".to_vec(),
            source: Source::Current,
        }
    );
}

#[test]
fn encoding_rejects_values_that_would_not_restore() {
    let product = DELAY.resolve().unwrap();
    assert_eq!(
        state::encode(&product, &[0.5, 0.0], &[]),
        Err(StateError::WrongValueCount)
    );
    assert_eq!(
        state::encode(&product, &[3.0, 0.0, 0.0], &[]),
        Err(StateError::InvalidValue(ParameterId(1)))
    );
    assert_eq!(
        state::encode(&product, &[0.5, 1.5, 0.0], &[]),
        Err(StateError::InvalidValue(ParameterId(2)))
    );
    assert_eq!(
        state::encode(&product, &[f32::NAN, 0.0, 0.0], &[]),
        Err(StateError::InvalidValue(ParameterId(1)))
    );
}

#[test]
fn malformed_foreign_and_newer_envelopes_are_rejected() {
    let product = DELAY.resolve().unwrap();
    let good = state::encode(&product, &[1.0, 1.0, 0.0], &[]).unwrap();
    let check =
        |bytes: &[u8], error| assert_eq!(state::restore(&product, bytes, &NoHooks), Err(error));

    check(b"<xml/>", StateError::Foreign);
    check(&[], StateError::Foreign);
    for length in 8..good.len() {
        check(&good[..length], StateError::Malformed);
    }
    let mut trailing = good.clone();
    trailing.push(0);
    check(&trailing, StateError::Malformed);

    let mut envelope = good.clone();
    envelope[8] = 2;
    check(&envelope, StateError::UnsupportedEnvelope(2));
    let mut flags = good.clone();
    flags[10] = 1;
    check(&flags, StateError::Malformed);
    let mut tag = good.clone();
    tag[12] = b'Z';
    check(&tag, StateError::WrongProduct);
    let mut newer = good.clone();
    newer[20] = 2;
    check(&newer, StateError::TooNew(2));

    let mut duplicate = good.clone();
    duplicate[36..40].copy_from_slice(&1u32.to_le_bytes());
    check(&duplicate, StateError::DuplicateParameter(ParameterId(1)));
    let mut infinite = good.clone();
    infinite[32..36].copy_from_slice(&f32::INFINITY.to_le_bytes());
    check(&infinite, StateError::InvalidValue(ParameterId(1)));
    let mut out_of_range = good;
    out_of_range[32..36].copy_from_slice(&5.0f32.to_le_bytes());
    check(&out_of_range, StateError::InvalidValue(ParameterId(1)));
}

struct Hooks;
impl StateHooks for Hooks {
    fn migrate(&self, saved: &mut SavedState) -> Result<(), StateError> {
        // Version 1 stored time in milliseconds; version 2 uses seconds.
        if saved.extra == b"reject" {
            return Err(StateError::Rejected);
        }
        for (id, value) in &mut saved.parameters {
            if *id == ParameterId(1) {
                *value /= 1000.0;
            }
        }
        Ok(())
    }
    fn import(&self, bytes: &[u8]) -> Option<SavedState> {
        let mode = bytes.strip_prefix(b"legacy:")?;
        Some(SavedState {
            version: 0,
            parameters: vec![(ParameterId(2), f32::from(*mode.first()? - b'0'))],
            extra: Vec::new(),
        })
    }
}

#[test]
fn earlier_versions_migrate_and_removed_parameters_are_dropped() {
    const V1_PARAMETERS: &[Parameter] = &[
        Parameter::linear("time_ms", "Time", 0.0, 2000.0, 500.0).with_id(1),
        Parameter::linear("removed", "Removed", 0.0, 1.0, 0.0).with_id(9),
    ];
    let old = DELAY.with_parameters(V1_PARAMETERS).resolve().unwrap();
    let bytes = state::encode(&old, &[1500.0, 1.0], b"keep").unwrap();

    let current = DELAY.with_parameters(PARAMETERS);
    let v2 = Product::new(
        "Delay",
        "Example",
        "2.0.0",
        "com.example.delay",
        Category::Effect,
        V2,
    )
    .with_parameters(current.parameters())
    .resolve()
    .unwrap();
    let restored = state::restore(&v2, &bytes, &Hooks).unwrap();
    assert_eq!(restored.values, [1.5, 0.0, 0.0]);
    assert_eq!(restored.extra, b"keep");
    assert_eq!(restored.source, Source::Earlier(1));

    // Without migration the old millisecond value is out of range: rejected, not clamped.
    assert_eq!(
        state::restore(&v2, &bytes, &NoHooks),
        Err(StateError::InvalidValue(ParameterId(1)))
    );
    let rejected = state::encode(&old, &[1500.0, 1.0], b"reject").unwrap();
    assert_eq!(
        state::restore(&v2, &rejected, &Hooks),
        Err(StateError::Rejected)
    );
}

#[test]
fn foreign_state_needs_an_import_hook() {
    let product = DELAY.resolve().unwrap();
    assert_eq!(
        state::restore(&product, b"legacy:3", &NoHooks),
        Err(StateError::Foreign)
    );
    let imported = state::restore(&product, b"legacy:3", &Hooks).unwrap();
    assert_eq!(imported.values, [0.5, 3.0, 0.0]);
    assert_eq!(imported.source, Source::Imported);
    assert_eq!(
        state::restore(&product, b"legacy:9", &Hooks),
        Err(StateError::InvalidValue(ParameterId(2)))
    );
    assert_eq!(
        state::restore(&product, b"other", &Hooks),
        Err(StateError::Foreign)
    );
}
