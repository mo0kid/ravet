use clack_extensions::params::ParamInfoFlags;
use ravet_clap::{
    ClapParameterError as Error, ClapParameterOptions as Options,
    ClapParameterRegistration as Registration,
};
use ravet_foundation::{
    ParameterDescriptor as Descriptor, ParameterId, ParameterRange as Range,
    ParameterScale as Scale, ParameterSchema as Schema,
};
fn descriptor(
    id: u32,
    name: &str,
    minimum: f32,
    maximum: f32,
    intervals: Option<u16>,
) -> Descriptor<'_> {
    Descriptor {
        id: ParameterId(id),
        name,
        unit: "",
        range: Range::new(minimum, maximum, Scale::Linear, intervals).unwrap(),
        default: minimum,
        automatable: true,
    }
}
#[test]
fn physical_values_complete_metadata_and_empty_registration_are_heap_free() {
    let mut logarithmic = descriptor(17, "Frequency λ", 20., 20000., None);
    logarithmic.range = Range::new(20., 20000., Scale::Logarithmic, None).unwrap();
    logarithmic.default = 440.;
    let mut mode = descriptor(23, "Mode", -2., 2., Some(4));
    mode.automatable = false;
    let descriptors = [logarithmic, mode, descriptor(99, "Bypass", 0., 1., Some(1))];
    let options = [
        Options {
            module: "Oscillator/Tuning",
            requires_process: true,
            bypass: false,
            global_modulation: true,
            ..Options::default()
        },
        Options::default(),
        Options {
            bypass: true,
            ..Options::default()
        },
    ];
    let registration = Registration::new(Schema::new(&descriptors).unwrap(), &options).unwrap();
    let counts = allocation_counter::measure(|| {
        for _ in 0..10000 {
            assert_eq!(registration.count(), 3);
            let frequency = registration.info(0).unwrap();
            assert!(frequency.flags.contains(ParamInfoFlags::IS_MODULATABLE));
            assert!(
                !registration
                    .info(1)
                    .unwrap()
                    .flags
                    .contains(ParamInfoFlags::IS_MODULATABLE)
            );
            assert_eq!(frequency.id.get(), 17);
            assert_eq!(frequency.name, "Frequency λ".as_bytes());
            assert_eq!(frequency.module, b"Oscillator/Tuning");
            assert_eq!(
                (
                    frequency.min_value,
                    frequency.max_value,
                    frequency.default_value
                ),
                (20., 20000., 440.)
            );
            assert_eq!(
                frequency.flags,
                ParamInfoFlags::IS_AUTOMATABLE
                    | ParamInfoFlags::IS_MODULATABLE
                    | ParamInfoFlags::REQUIRES_PROCESS
            );
            assert_eq!(
                registration.info(1).unwrap().flags,
                ParamInfoFlags::IS_STEPPED
            );
            assert_eq!(
                registration.info(2).unwrap().flags,
                ParamInfoFlags::IS_AUTOMATABLE
                    | ParamInfoFlags::IS_STEPPED
                    | ParamInfoFlags::IS_BYPASS
            );
            assert!(registration.info(3).is_none());
            assert!(registration.info(u32::MAX).is_none());
        }
    });
    assert_eq!(counts.count_total, 0);
    assert_eq!(counts.count_current, 0);
    let empty = Registration::new(Schema::new(&[]).unwrap(), &[]).unwrap();
    assert_eq!(empty.count(), 0);
    assert!(empty.info(0).is_none());
}
#[test]
fn invalid_wire_ranges_ids_complete_strings_bypass_and_capacity_reject() {
    for d in [
        descriptor(u32::MAX, "ID", 0., 1., None),
        descriptor(1, "nul\0name", 0., 1., None),
        descriptor(1, &"λ".repeat(128), 0., 1., None),
    ] {
        assert!(matches!(
            Registration::new(Schema::new(&[d]).unwrap(), &[Options::default()]),
            Err(Error::Metadata(_))
        ));
    }
    for module in ["nul\0path".to_string(), "λ".repeat(512)] {
        assert_eq!(
            Registration::new(
                Schema::new(&[descriptor(1, "Name", 0., 1., None)]).unwrap(),
                &[Options {
                    module: &module,
                    ..Options::default()
                }]
            )
            .unwrap_err(),
            Error::Metadata(ParameterId(1))
        );
    }
    // Byte boundaries, complete strings and the largest valid ID survive unchanged.
    let name = "x".repeat(255);
    let module = "x".repeat(1023);
    let descriptors = [descriptor(u32::MAX - 1, &name, 0., 1., None)];
    let options = [Options {
        module: &module,
        ..Options::default()
    }];
    let accepted = Registration::new(Schema::new(&descriptors).unwrap(), &options).unwrap();
    assert_eq!(accepted.info(0).unwrap().name.len(), 255);
    assert_eq!(accepted.info(0).unwrap().module.len(), 1023);
    for d in [
        descriptor(1, "Fractional", 0., 1., Some(2)),
        descriptor(1, "Coarse", 0., 4., Some(2)),
        descriptor(1, "Offset", 0.5, 2.5, Some(2)),
    ] {
        assert_eq!(
            Registration::new(Schema::new(&[d]).unwrap(), &[Options::default()]).unwrap_err(),
            Error::SteppedRange(ParameterId(1))
        );
    }
    for d in [
        descriptor(1, "Continuous", 0., 1., None),
        descriptor(1, "Not binary", -1., 1., Some(2)),
    ] {
        assert_eq!(
            Registration::new(
                Schema::new(&[d]).unwrap(),
                &[Options {
                    bypass: true,
                    ..Options::default()
                }]
            )
            .unwrap_err(),
            Error::Bypass(ParameterId(1))
        );
    }
    let descriptors = [
        descriptor(1, "A", 0., 1., Some(1)),
        descriptor(2, "B", 0., 1., Some(1)),
    ];
    assert_eq!(
        Registration::new(
            Schema::new(&descriptors).unwrap(),
            &[Options {
                bypass: true,
                ..Options::default()
            }; 2]
        )
        .unwrap_err(),
        Error::Bypass(ParameterId(2))
    );
    let descriptors: [_; 1025] =
        std::array::from_fn(|i| descriptor(i as u32, "Many", 0., 1., None));
    assert_eq!(
        Registration::new(
            Schema::new(&descriptors).unwrap(),
            &[Options::default(); 1025]
        )
        .unwrap_err(),
        Error::Capacity
    );
    let boundary: [_; 1024] = std::array::from_fn(|i| descriptor(i as u32, "Many", 0., 1., None));
    assert_eq!(
        Registration::new(Schema::new(&boundary).unwrap(), &[Options::default(); 1024])
            .unwrap()
            .count(),
        1024
    );
}

#[test]
fn all_scoped_metadata_flag_combinations_remain_independent_and_heap_free() {
    use ravet_clap::ClapParameterScopes;
    let mut descriptor = descriptor(7, "Scope", 0., 1., None);
    descriptor.automatable = false;
    let descriptors = [descriptor];
    let schema = Schema::new(&descriptors).unwrap();
    let flags = [
        ParamInfoFlags::IS_AUTOMATABLE_PER_NOTE_ID,
        ParamInfoFlags::IS_AUTOMATABLE_PER_KEY,
        ParamInfoFlags::IS_AUTOMATABLE_PER_CHANNEL,
        ParamInfoFlags::IS_AUTOMATABLE_PER_PORT,
        ParamInfoFlags::IS_MODULATABLE_PER_NOTE_ID,
        ParamInfoFlags::IS_MODULATABLE_PER_KEY,
        ParamInfoFlags::IS_MODULATABLE_PER_CHANNEL,
        ParamInfoFlags::IS_MODULATABLE_PER_PORT,
    ];
    let scopes = |mask: u16| ClapParameterScopes {
        note_id: mask & 1 != 0,
        key: mask & 2 != 0,
        channel: mask & 4 != 0,
        port: mask & 8 != 0,
    };
    assert_eq!(ClapParameterScopes::NONE, ClapParameterScopes::default());
    assert_eq!(ClapParameterScopes::ALL, scopes(15));
    let counts = allocation_counter::measure(|| {
        for _ in 0..10 {
            for mask in 0u16..256 {
                let options = [Options {
                    scoped_automation: scopes(mask),
                    scoped_modulation: scopes(mask >> 4),
                    ..Options::default()
                }];
                let info = Registration::new(schema, &options)
                    .unwrap()
                    .info(0)
                    .unwrap();
                for (index, flag) in flags.into_iter().enumerate() {
                    assert_eq!(info.flags.contains(flag), mask & (1 << index) != 0);
                }
                assert!(!info.flags.contains(ParamInfoFlags::IS_AUTOMATABLE));
                assert!(!info.flags.contains(ParamInfoFlags::IS_MODULATABLE));
                assert!(info.cookie.as_raw().is_null());
            }
        }
    });
    assert_eq!(counts.count_total, 0);
}
