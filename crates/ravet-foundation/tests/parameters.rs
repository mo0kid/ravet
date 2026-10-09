use ravet_foundation::*;

fn descriptor(id: u32, default: f32) -> ParameterDescriptor<'static> {
    ParameterDescriptor {
        id: ParameterId(id),
        name: "Test",
        unit: "x",
        range: ParameterRange::new(0.0, 4.0, ParameterScale::Linear, None).unwrap(),
        default,
        automatable: true,
    }
}

#[test]
fn mappings_clamp_and_preserve_extreme_endpoints() {
    for (min, max, scale) in [
        (0.0, 4.0, ParameterScale::Linear),
        (20.0, 20_000.0, ParameterScale::Logarithmic),
        (-f32::MAX, f32::MAX, ParameterScale::Linear),
        (f32::MIN_POSITIVE, f32::MAX, ParameterScale::Logarithmic),
    ] {
        let range = ParameterRange::new(min, max, scale, None).unwrap();
        assert_eq!(range.denormalize(-1.0), Some(min));
        assert_eq!(range.denormalize(2.0), Some(max));
        assert_eq!(range.normalize(min), Some(0.0));
        assert_eq!(range.normalize(max), Some(1.0));
        for n in [0.0, 0.1, 0.5, 0.9, 1.0] {
            let value = range.denormalize(n).unwrap();
            assert!(value.is_finite());
            assert!((range.normalize(value).unwrap() - n).abs() < 1e-5);
        }
        assert_eq!(range.normalize(f32::NAN), None);
        assert_eq!(range.denormalize(f32::INFINITY), None);
    }
    let frequency = ParameterRange::new(20.0, 20_000.0, ParameterScale::Logarithmic, None).unwrap();
    assert!((frequency.denormalize(0.5).unwrap() - 632.4555).abs() < 0.001);
}

#[test]
fn invalid_ranges_and_discrete_mapping() {
    for (min, max, scale, steps) in [
        (1.0, 1.0, ParameterScale::Linear, None),
        (f32::NAN, 1.0, ParameterScale::Linear, None),
        (0.0, f32::INFINITY, ParameterScale::Linear, None),
        (0.0, 4.0, ParameterScale::Logarithmic, None),
        (1.0, 4.0, ParameterScale::Logarithmic, Some(2)),
        (0.0, 1.0, ParameterScale::Linear, Some(0)),
    ] {
        assert_eq!(
            ParameterRange::new(min, max, scale, steps),
            Err(ParameterError::InvalidRange)
        );
    }
    let boolean = ParameterRange::new(0.0, 1.0, ParameterScale::Linear, Some(1)).unwrap();
    assert_eq!(boolean.canonicalize(0.499), Some(0.0));
    assert_eq!(boolean.canonicalize(0.5), Some(1.0));
    let choices = ParameterRange::new(-1.0, 1.0, ParameterScale::Linear, Some(4)).unwrap();
    assert_eq!(choices.denormalize(0.6), Some(0.0));
    assert_eq!(choices.canonicalize(0.25), Some(0.5));
}

#[test]
fn schemas_reject_duplicates_and_invalid_defaults() {
    let descriptors = [descriptor(1, 1.0), descriptor(1, 2.0)];
    assert!(matches!(
        ParameterSchema::new(&descriptors),
        Err(ParameterError::DuplicateId(ParameterId(1)))
    ));
    let mut descriptors = [descriptor(1, 5.0)];
    assert!(matches!(
        ParameterSchema::new(&descriptors),
        Err(ParameterError::InvalidValue(ParameterId(1)))
    ));
    descriptors[0].default = 1.0;
    descriptors[0].name = "";
    assert!(matches!(
        ParameterSchema::new(&descriptors),
        Err(ParameterError::InvalidMetadata)
    ));
}

#[test]
fn state_updates_and_migration_are_strict_and_use_ids() {
    let old = [descriptor(1, 1.0), descriptor(2, 0.0)];
    let mut state = ParameterSchema::new(&old)
        .unwrap()
        .state([3.0, 2.0])
        .unwrap();
    assert!(state.set(ParameterId(1), f32::NAN).is_err());
    assert!(state.set(ParameterId(99), 1.0).is_err());
    assert_eq!(state.values(), &[3.0, 2.0]);
    let new = [descriptor(2, 0.0), descriptor(3, 1.0)];
    let migrated = state.migrate(ParameterSchema::new(&new).unwrap()).unwrap();
    assert_eq!(migrated.values(), &[2.0, 1.0]);
    let mut narrowed = new;
    narrowed[0].range = ParameterRange::new(0.0, 1.0, ParameterScale::Linear, None).unwrap();
    assert!(matches!(
        state.migrate(ParameterSchema::new(&narrowed).unwrap()),
        Err(ParameterError::InvalidValue(ParameterId(2)))
    ));
}

#[test]
fn record_is_order_independent_and_rejects_malformed_fields() {
    let descriptors = [descriptor(1, 1.0), descriptor(2, 0.0)];
    let schema = ParameterSchema::new(&descriptors).unwrap();
    let state = schema.state([0.5, 2.0]).unwrap();
    let mut bytes = [0; 32];
    state.encode(*b"TESTPARM", 3, &mut bytes).unwrap();
    assert_eq!(&bytes[8..16], &[3, 0, 0, 0, 2, 0, 0, 0]);
    let decode = |bytes: &[u8]| ParameterState::decode(schema, *b"TESTPARM", 3, bytes);
    assert_eq!(decode(&bytes).unwrap().values(), &[0.5, 2.0]);
    let mut reordered = bytes;
    reordered[16..24].copy_from_slice(&bytes[24..32]);
    reordered[24..32].copy_from_slice(&bytes[16..24]);
    assert_eq!(decode(&reordered).unwrap().values(), &[0.5, 2.0]);
    for len in 0..32 {
        assert!(decode(&bytes[..len]).is_err());
    }
    assert!(decode(&[0; 33]).is_err());
    for offset in [0, 8, 12, 16] {
        let mut bad = bytes;
        bad[offset] = 99;
        assert!(decode(&bad).is_err());
    }
    let mut bad = bytes;
    bad[24..28].copy_from_slice(&1_u32.to_le_bytes());
    assert!(matches!(
        decode(&bad),
        Err(ParameterError::DuplicateId(ParameterId(1)))
    ));
    for value in [f32::NAN, f32::INFINITY, -1.0, 5.0] {
        bad = bytes;
        bad[20..24].copy_from_slice(&value.to_le_bytes());
        assert!(matches!(
            decode(&bad),
            Err(ParameterError::InvalidValue(ParameterId(1)))
        ));
    }
    let mut destination = [7; 31];
    assert!(state.encode(*b"TESTPARM", 3, &mut destination).is_err());
    assert_eq!(destination, [7; 31]);
    let mut destination = [7; 32];
    assert!(state.encode(*b"TESTPARM", 0, &mut destination).is_err());
    assert_eq!(destination, [7; 32]);
}

#[test]
fn discrete_state_rejects_unsnapped_records() {
    let mut d = descriptor(1, 0.0);
    d.range = ParameterRange::new(0.0, 1.0, ParameterScale::Linear, Some(1)).unwrap();
    let ds = [d];
    let schema = ParameterSchema::new(&ds).unwrap();
    assert!(schema.state([0.5]).is_err());
    assert_eq!(schema.state([-0.0]).unwrap().values()[0].to_bits(), 0);
}
