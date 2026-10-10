#![allow(unsafe_code)]
use native_transport_presence as p;
use ravet_foundation::*;
use ravet_native::*;
#[test]
fn all_availability_and_boolean_combinations_validate_dependencies_without_guessing() {
    for present in 0..1024 {
        for flags in 0..32 {
            let has = |bit| present & bit != 0;
            let record = NativeTransportV1 {
                present,
                flags,
                sample_position: if has(p::SAMPLES) { -99 } else { 0 },
                beat_position: if has(p::BEATS) { -2.5 } else { 0. },
                tempo: if has(p::TEMPO) { 137.25 } else { 0. },
                tempo_increment: if has(p::TEMPO_INCREMENT) {
                    -0.00000123456789
                } else {
                    0.
                },
                beat_loop_start: if has(p::BEAT_LOOP) { -4. } else { 0. },
                beat_loop_end: if has(p::BEAT_LOOP) { 12. } else { 0. },
                sample_loop_start: if has(p::SAMPLE_LOOP) { -512 } else { 0 },
                sample_loop_end: if has(p::SAMPLE_LOOP) { 1024 } else { 0 },
                bar_start: if has(p::BAR) { -4. } else { 0. },
                bar_number: if has(p::BAR) { -1 } else { 0 },
                signature_numerator: if has(p::TIME_SIGNATURE) { 7 } else { 0 },
                signature_denominator: if has(p::TIME_SIGNATURE) { 8 } else { 0 },
                reserved: 0,
            };
            let children = present
                & (p::TIME_SIGNATURE | p::TEMPO_INCREMENT | p::BEAT_LOOP | p::SAMPLE_LOOP | p::BAR)
                != 0;
            let invalid = [
                flags & 1 != 0 && !has(p::PLAYING),
                (flags & 28 != 0 || children) && !has(p::DETAILS),
                has(p::TEMPO_INCREMENT) && !has(p::TEMPO),
                has(p::BEAT_LOOP) && (!has(p::BEATS) || flags & 8 == 0),
                has(p::SAMPLE_LOOP) && (!has(p::SAMPLES) || flags & 8 == 0),
                has(p::BAR) && !has(p::BEATS),
            ];
            let valid = !invalid.into_iter().any(|value| value);
            let result = record.to_transport();
            assert_eq!(result.is_ok(), valid, "present={present} flags={flags}");
            if let Ok(transport) = result {
                assert_eq!(
                    NativeTransportV1::from_transport(transport).unwrap(),
                    record
                );
                assert_eq!(transport.playing, has(p::PLAYING).then_some(flags & 1 != 0));
                assert_eq!(transport.details.is_some(), has(p::DETAILS));
            }
        }
    }
    assert_eq!(
        NativeTransportV1::default().to_transport().unwrap(),
        Transport::default()
    );
    let known_false = Transport {
        playing: Some(false),
        details: Some(TransportDetails::default()),
        ..Default::default()
    };
    assert_eq!(
        NativeTransportV1::from_transport(known_false).unwrap(),
        NativeTransportV1 {
            present: 17,
            ..Default::default()
        }
    );
}
fn full() -> Transport {
    Transport {
        playing: None,
        sample_position: Some(i64::MAX),
        beat_position: Some(1e200),
        tempo: Some(1e300),
        discontinuity: true,
        details: Some(TransportDetails {
            recording: true,
            loop_active: true,
            within_pre_roll: true,
            time_signature: Some((u16::MAX, u16::MAX)),
            tempo_increment: Some(-1e200),
            beat_loop: Some((3e199, 2e200)),
            sample_loop: Some((i64::MIN, i64::MAX)),
            bar: Some((-1e200, i32::MIN)),
        }),
    }
}
#[test]
fn wide_precision_unknown_states_negative_positions_and_signed_zero_are_exact() {
    let transport = full();
    let record = NativeTransportV1::from_transport(transport).unwrap();
    assert_eq!(record.to_transport().unwrap(), transport);
    assert_eq!(record.present, 1022);
    assert_eq!(record.flags, 30);
    assert_eq!(record.sample_position, i64::MAX);
    assert_eq!(record.bar_number, i32::MIN);
    assert_eq!(record.beat_position.to_bits(), 1e200_f64.to_bits());
    assert_eq!(record.tempo.to_bits(), 1e300_f64.to_bits());
    let negative_zero = Transport {
        beat_position: Some(-0.),
        tempo: Some(f64::MIN_POSITIVE),
        details: Some(TransportDetails {
            tempo_increment: Some(-0.),
            bar: Some((-0., -1)),
            ..Default::default()
        }),
        ..Default::default()
    };
    let record = NativeTransportV1::from_transport(negative_zero).unwrap();
    let returned = record.to_transport().unwrap();
    assert_eq!(
        returned.beat_position.unwrap().to_bits(),
        (-0_f64).to_bits()
    );
    assert_eq!(
        returned.details.unwrap().tempo_increment.unwrap().to_bits(),
        (-0_f64).to_bits()
    );
    assert_eq!(
        returned.details.unwrap().bar.unwrap().0.to_bits(),
        (-0_f64).to_bits()
    );
}
#[test]
fn malformed_native_and_portable_snapshots_reject_without_normalizing() {
    let zero = NativeTransportV1::default();
    for bad in [
        NativeTransportV1 {
            present: 1024,
            ..zero
        },
        NativeTransportV1 { flags: 32, ..zero },
        NativeTransportV1 {
            reserved: 1,
            ..zero
        },
        NativeTransportV1 {
            sample_position: 1,
            ..zero
        },
        NativeTransportV1 {
            beat_position: -0.,
            ..zero
        },
        NativeTransportV1 { tempo: 1., ..zero },
        NativeTransportV1 {
            tempo_increment: -0.,
            ..zero
        },
        NativeTransportV1 {
            beat_loop_start: 1.,
            ..zero
        },
        NativeTransportV1 {
            beat_loop_end: 1.,
            ..zero
        },
        NativeTransportV1 {
            sample_loop_start: 1,
            ..zero
        },
        NativeTransportV1 {
            sample_loop_end: 1,
            ..zero
        },
        NativeTransportV1 {
            bar_start: 1.,
            ..zero
        },
        NativeTransportV1 {
            bar_number: 1,
            ..zero
        },
        NativeTransportV1 {
            signature_numerator: 1,
            ..zero
        },
        NativeTransportV1 {
            signature_denominator: 1,
            ..zero
        },
    ] {
        assert_eq!(bad.to_transport(), Err(NativeTransportError));
    }
    let good = NativeTransportV1::from_transport(full()).unwrap();
    for bad in [
        NativeTransportV1 {
            beat_position: f64::NAN,
            ..good
        },
        NativeTransportV1 {
            tempo: f64::INFINITY,
            ..good
        },
        NativeTransportV1 { tempo: 0., ..good },
        NativeTransportV1 { tempo: -1., ..good },
        NativeTransportV1 {
            tempo_increment: f64::NAN,
            ..good
        },
        NativeTransportV1 {
            beat_loop_start: f64::NEG_INFINITY,
            ..good
        },
        NativeTransportV1 {
            beat_loop_end: 3e199,
            ..good
        },
        NativeTransportV1 {
            sample_loop_start: i64::MAX,
            ..good
        },
        NativeTransportV1 {
            bar_start: f64::NAN,
            ..good
        },
        NativeTransportV1 {
            signature_numerator: 0,
            ..good
        },
        NativeTransportV1 {
            signature_denominator: 65536,
            ..good
        },
    ] {
        assert_eq!(bad.to_transport(), Err(NativeTransportError));
    }
    let mut bad = full();
    bad.tempo = None;
    assert_eq!(
        NativeTransportV1::from_transport(bad),
        Err(NativeTransportError)
    );
    let mut bad = full();
    bad.beat_position = None;
    assert_eq!(
        NativeTransportV1::from_transport(bad),
        Err(NativeTransportError)
    );
    let mut bad = full();
    bad.sample_position = None;
    assert_eq!(
        NativeTransportV1::from_transport(bad),
        Err(NativeTransportError)
    );
    let mut bad = full();
    bad.details.as_mut().unwrap().loop_active = false;
    assert_eq!(
        NativeTransportV1::from_transport(bad),
        Err(NativeTransportError)
    );
    let bad = Transport {
        details: Some(TransportDetails {
            time_signature: Some((7, 0)),
            ..Default::default()
        }),
        ..Default::default()
    };
    assert_eq!(
        NativeTransportV1::from_transport(bad),
        Err(NativeTransportError)
    );
}
#[derive(Default)]
struct Capture {
    transport: Option<Transport>,
    calls: usize,
}
macro_rules! cycle {
    ($name:ident,$sample:ty) => {
        impl Processor<$sample> for Capture {
            fn prepare(&mut self, _: ProcessConfig) -> Result<(), PrepareError> {
                Ok(())
            }
            fn reset(&mut self) {}
            fn deactivate(&mut self) {}
            fn process(&mut self, block: &mut ProcessBlock<'_, '_, '_, $sample>) -> ProcessStatus {
                self.transport = Some(block.transport);
                self.calls += 1;
                block.audio.output_mut(0).unwrap().fill(0.5);
                ProcessStatus::Continue
            }
        }
        #[test]
        fn $name() {
            let mut processor = Capture::default();
            let config = ProcessConfig::new(48000., 8, 0, AudioLayout::MONO_INSTRUMENT).unwrap();
            let mut adapter =
                NativeAudioAdapter::<$sample>::prepare(&mut processor, config).unwrap();
            let mut output = [91. as $sample; 10];
            let outputs = [output.as_mut_ptr()];
            let bus = NativePlanarBus {
                input_channels: 0,
                output_channels: 1,
                inputs: std::ptr::null(),
                outputs: outputs.as_ptr(),
            };
            let transport = full();
            let raw = NativeTransportV1::from_transport(transport).unwrap();
            let heap = allocation_counter::measure(|| {
                for _ in 0..10000 {
                    let typed = raw.to_transport().unwrap();
                    let report =
                        unsafe { adapter.process_native(&mut processor, &[bus], 8, &[], typed) };
                    assert_eq!(report.status, ProcessStatus::Continue);
                    assert_eq!(processor.transport, Some(transport));
                    assert_eq!(
                        NativeTransportV1::from_transport(processor.transport.unwrap()).unwrap(),
                        raw
                    );
                }
            });
            assert_eq!((heap.count_total, heap.bytes_total), (0, 0));
            assert_eq!(output[..8], [0.5 as $sample; 8]);
            assert_eq!(output[8..], [91. as $sample; 2]);
            assert_eq!(processor.calls, 10000);
            let calls = processor.calls;
            assert!(
                NativeTransportV1 { tempo: -1., ..raw }
                    .to_transport()
                    .is_err()
            );
            assert_eq!(processor.calls, calls);
        }
    };
}
cycle!(native_transport_f32_processing_roundtrip, f32);
cycle!(native_transport_f64_processing_roundtrip, f64);
#[test]
fn native_layout_matches_fixed_c_offsets() {
    use std::mem::{offset_of, size_of};
    assert_eq!(size_of::<NativeTransportV1>(), 96);
    assert_eq!(offset_of!(NativeTransportV1, sample_position), 8);
    assert_eq!(offset_of!(NativeTransportV1, bar_start), 72);
    assert_eq!(offset_of!(NativeTransportV1, bar_number), 80);
    assert_eq!(offset_of!(NativeTransportV1, reserved), 92);
}
