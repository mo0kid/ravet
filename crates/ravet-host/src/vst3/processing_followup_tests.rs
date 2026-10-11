use super::*;

fn event(kind: Kind<'_>) -> Event<'_> {
    Event {
        header: Header {
            bus: 0,
            offset: 0,
            ppq_position: 0.,
            flags: 0,
        },
        kind,
    }
}

#[test]
fn native_terminated_text_budget_refuses_before_owned_wire_mutation() {
    // Independent native EventWire boundary: 65533 bytes round to 65534,
    // then one empty UTF-16 terminator fills 65536. 65535/65536 cannot fit it.
    for binary_length in [65533, 65535, 65536] {
        for text in [
            Kind::ExpressionText {
                expression_id: 1,
                note_id: 0,
                text: "",
            },
            Kind::Chord {
                root: 0,
                bass: 0,
                mask: 0,
                text: "",
            },
            Kind::Scale {
                root: 0,
                mask: 0,
                text: "",
            },
        ] {
            let bytes = vec![0xa5; binary_length];
            let mut input = InputEvents::new(2, 65536).unwrap();
            input.push(event(Kind::SysEx(&bytes))).unwrap();
            let old_records = input.records.clone();
            let old_payload = input.payload.clone();
            let old_usage = (input.len(), input.used, input.native_used);
            if binary_length == 65533 {
                input.push(event(text)).unwrap();
                assert_eq!(input.native_used, 65536);
                assert_eq!(input.payload_bytes(), 65534);
                assert_eq!(input.bytes()[65533], 0);
            } else {
                assert_eq!(input.push(event(text)), Err(Error::Capacity));
                assert_eq!(input.records, old_records);
                assert_eq!(input.payload, old_payload);
                assert_eq!((input.len(), input.used, input.native_used), old_usage);
            }
        }
    }
    // The wire payload excludes native terminators: its established tiny-buffer
    // contract stays valid while the separate 65536-byte native budget is checked.
    let mut tiny = InputEvents::new(1, 2).unwrap();
    tiny.push(event(Kind::Scale {
        root: 0,
        mask: 0,
        text: "A",
    }))
    .unwrap();
    assert_eq!(tiny.bytes(), &[65, 0]);
    assert_eq!(tiny.native_used, 4);
}

#[test]
fn cumulative_native_text_budget_and_clear_are_independent_of_wire_capacity() {
    let text = "A".repeat(4096);
    let mut input = InputEvents::new(8, 65536).unwrap();
    for _ in 0..7 {
        input
            .push(event(Kind::Scale {
                root: 0,
                mask: 0,
                text: &text,
            }))
            .unwrap();
    }
    assert_eq!(input.payload_bytes(), 57344);
    let records = input.records.clone();
    let payload = input.payload.clone();
    assert_eq!(
        input.push(event(Kind::Scale {
            root: 0,
            mask: 0,
            text: &text
        })),
        Err(Error::Capacity)
    );
    assert_eq!(input.records, records);
    assert_eq!(input.payload, payload);
    assert_eq!(input.native_used, 57358);
    input.clear();
    input
        .push(event(Kind::Scale {
            root: 0,
            mask: 0,
            text: &text,
        }))
        .unwrap();
    assert_eq!(input.native_used, 8194);
}

#[test]
fn control_success_requires_its_own_stage_and_no_audio_publication_facts() {
    for stage in [1, 2] {
        let success = RawReport {
            stage,
            ..RawReport::initial()
        };
        assert_eq!(accepted_control(0, success, stage).unwrap().stage(), stage);
        let candidates = [
            RawReport {
                stage: 3,
                flags: 51,
                ..success
            },
            RawReport {
                stage: 3,
                ..success
            },
            RawReport {
                index: 0,
                ..success
            },
            RawReport {
                event_count: 1,
                ..success
            },
            RawReport {
                payload_used: 1,
                ..success
            },
            RawReport {
                parameter_queue_count: 1,
                ..success
            },
            RawReport {
                parameter_point_count: 1,
                ..success
            },
        ];
        for malformed in candidates {
            assert_eq!(accepted_control(0, malformed, stage), Err(Error::Abi));
        }
        let refusal = RawReport::initial();
        assert!(matches!(
            accepted_control(2, refusal, stage),
            Err(Error::Provider { status: 2, report }) if report.raw == refusal
        ));
    }
}

#[test]
fn maximal_output_backing_has_every_native_publication_capacity() {
    let mut storage = OutputStorage::maximum().unwrap();
    let wire = storage.wire();
    assert_eq!(wire.event_capacity, 2048);
    assert_eq!(wire.payload_capacity, 65536);
    assert_eq!(wire.parameter_queue_capacity, 64);
    assert_eq!(wire.parameter_point_capacity, 2048);
    assert_eq!(
        (
            wire.event_count,
            wire.payload_used,
            wire.parameter_queue_count,
            wire.parameter_point_count
        ),
        (0, 0, 0, 0)
    );
}
