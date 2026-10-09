use ravet_foundation::*;

fn descriptor(id: u32) -> ParameterDescriptor<'static> {
    ParameterDescriptor {
        id: ParameterId(id),
        name: "Level",
        unit: "",
        default: 0.2,
        range: ParameterRange::new(0., 1., ParameterScale::Linear, None).unwrap(),
        automatable: true,
    }
}
fn program(id: u32, bank: u16, number: u8, value: f32) -> ParameterProgram<'static, 1> {
    ParameterProgram {
        id: ProgramId(id),
        name: "Sound",
        midi_bank: bank,
        midi_number: number,
        values: [value],
    }
}

#[test]
fn stable_ids_midi_addresses_and_complete_schema_states_remain_independent() {
    let descriptors = [
        descriptor(91),
        ParameterDescriptor {
            id: ParameterId(3),
            name: "Bypass",
            unit: "",
            default: 0.,
            automatable: true,
            range: ParameterRange::new(0., 1., ParameterScale::Linear, Some(1)).unwrap(),
        },
    ];
    let schema = ParameterSchema::new(&descriptors).unwrap();
    let programs = [
        ParameterProgram {
            id: ProgramId(999),
            name: "Quiet",
            midi_bank: 16383,
            midi_number: 127,
            values: [0.05, -0.],
        },
        ParameterProgram {
            id: ProgramId(1),
            name: "Bypassed",
            midi_bank: 0,
            midi_number: 0,
            values: [0.5, 1.],
        },
    ];
    let bank = ProgramBank::new(schema, &programs).unwrap();
    assert_eq!(bank.index(ProgramId(999)), Some(0));
    assert_eq!(bank.midi_index(0, 0), Some(1));
    assert_eq!(bank.midi_index(16383, 127), Some(0));
    for (b, p) in [(16384, 0), (0, 128), (1, 0)] {
        assert_eq!(bank.midi_index(b, p), None);
    }
    assert_eq!(bank.index(ProgramId(2)), None);
    assert!(bank.state(2).is_none());
    let quiet = bank.state(0).unwrap();
    assert_eq!(quiet.get(ParameterId(91)), Some(0.05));
    assert_eq!(quiet.get(ParameterId(3)).unwrap().to_bits(), 0);
    let mut current = bank.state(1).unwrap();
    current.set(ParameterId(91), 0.75).unwrap();
    assert_eq!(bank.state(1).unwrap().get(ParameterId(91)), Some(0.5));
    assert_eq!(bank.programs()[0].name, "Quiet");
    assert!(current.set(ParameterId(3), 0.5).is_err());
    assert_eq!(current.get(ParameterId(3)), Some(1.));
}

#[test]
fn registration_rejects_whole_invalid_banks_without_clamping_values() {
    let descriptors = [descriptor(1)];
    let schema = ParameterSchema::new(&descriptors).unwrap();
    assert!(matches!(
        ProgramBank::new(schema, &[]),
        Err(ProgramError::Capacity)
    ));
    for name in ["", "  \t", "name\0suffix"] {
        let mut p = program(1, 0, 0, 0.2);
        p.name = name;
        assert!(matches!(
            ProgramBank::new(schema, &[p]),
            Err(ProgramError::Metadata(ProgramId(1)))
        ));
    }
    let max_name = "é".repeat(127) + "x";
    let mut p = program(1, 0, 0, 0.2);
    p.name = &max_name;
    assert!(ProgramBank::new(schema, &[p]).is_ok());
    let over_name = format!("{max_name}x");
    p.name = &over_name;
    assert!(ProgramBank::new(schema, &[p]).is_err());
    for (b, n) in [(16384, 0), (0, 128)] {
        assert!(matches!(
            ProgramBank::new(schema, &[program(1, b, n, 0.2)]),
            Err(ProgramError::Metadata(_))
        ));
    }
    for v in [f32::NAN, f32::INFINITY, -0.1, 1.1] {
        assert!(matches!(
            ProgramBank::new(schema, &[program(1, 0, 0, 0.2), program(2, 0, 1, v)]),
            Err(ProgramError::Values {
                program: ProgramId(2),
                ..
            })
        ));
    }
    assert!(matches!(
        ProgramBank::new(schema, &[program(1, 0, 0, 0.2), program(1, 1, 0, 0.5)]),
        Err(ProgramError::DuplicateId(ProgramId(1)))
    ));
    assert!(matches!(
        ProgramBank::new(schema, &[program(1, 0, 0, 0.2), program(2, 0, 0, 0.5)]),
        Err(ProgramError::DuplicateAddress { bank: 0, number: 0 })
    ));
}

#[test]
fn entry_parameter_and_total_capacity_boundaries() {
    let descriptors = [descriptor(1)];
    let schema = ParameterSchema::new(&descriptors).unwrap();
    let programs: Vec<_> = (0..129)
        .map(|i| program(i, 0, (i % 128) as u8, 0.2))
        .collect();
    assert!(ProgramBank::new(schema, &programs[..128]).is_ok());
    assert!(matches!(
        ProgramBank::new(schema, &programs),
        Err(ProgramError::Capacity)
    ));
    let descriptors: [_; 1024] = std::array::from_fn(|i| descriptor(i as u32));
    let schema = ParameterSchema::new(&descriptors).unwrap();
    let programs: Vec<_> = (0..65)
        .map(|i| ParameterProgram {
            id: ProgramId(i),
            name: "Large",
            midi_bank: 0,
            midi_number: i as u8,
            values: [0.2; 1024],
        })
        .collect();
    assert!(ProgramBank::new(schema, &programs[..64]).is_ok());
    assert!(matches!(
        ProgramBank::new(schema, &programs),
        Err(ProgramError::Capacity)
    ));
    let descriptors: [_; 1025] = std::array::from_fn(|i| descriptor(i as u32));
    assert!(matches!(
        ProgramBank::new(
            ParameterSchema::new(&descriptors).unwrap(),
            &[ParameterProgram {
                id: ProgramId(1),
                name: "Too many",
                midi_bank: 0,
                midi_number: 0,
                values: [0.2; 1025],
            }]
        ),
        Err(ProgramError::Capacity)
    ));
    let empty_schema = ParameterSchema::new(&[]).unwrap();
    assert!(matches!(
        ProgramBank::new(
            empty_schema,
            &[ParameterProgram {
                id: ProgramId(1),
                name: "Empty",
                midi_bank: 0,
                midi_number: 0,
                values: [],
            }]
        ),
        Err(ProgramError::Capacity)
    ));
}
