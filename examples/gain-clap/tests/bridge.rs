use clack_host::prelude::*;
use clack_plugin::events::event_types::ParamValueEvent;
use clack_plugin::process::Audio;
use ravet_clap::{BridgeError, EffectBridge, MAXIMUM_EVENTS, ParameterEvents};
use ravet_foundation::{AudioLayout, ProcessConfig, ProcessStatus as PortableStatus};
use ravet_gain::GainProcessor;
use ravet_gain_clap::CLAP_GAIN_ID;

fn bridge() -> EffectBridge<GainProcessor> {
    EffectBridge::prepare(
        GainProcessor::new(0.0).unwrap(),
        ProcessConfig::new(48_000.0, 64, MAXIMUM_EVENTS, AudioLayout::STEREO).unwrap(),
    )
    .unwrap()
}

/// Construct legal CLAP aliasing without manufacturing overlapping Rust slices.
/// The raw descriptors remain stack-owned and live only for this callback.
#[allow(unsafe_code)]
fn with_in_place(samples: &mut [[f32; 64]; 2], frames: u32, operation: impl FnOnce(Audio<'_>)) {
    assert!(frames <= 64);
    let mut input_pointers = [samples[0].as_mut_ptr(), samples[1].as_mut_ptr()];
    let mut output_pointers = input_pointers;
    let descriptor = clap_sys::audio_buffer::clap_audio_buffer {
        data32: input_pointers.as_mut_ptr(),
        data64: std::ptr::null_mut(),
        channel_count: 2,
        latency: 0,
        constant_mask: 0,
    };
    let input = [descriptor];
    let mut output = [clap_sys::audio_buffer::clap_audio_buffer {
        data32: output_pointers.as_mut_ptr(),
        ..descriptor
    }];
    // SAFETY: descriptors/pointer array/storage live for the entire operation;
    // lengths are checked, f32 alignment is native, channels are disjoint, and
    // pointer arrays are distinct, and the only sample aliasing is each output
    // with its matching input. The exclusive
    // borrow prevents concurrent Rust access to samples during the callback.
    let audio = unsafe { Audio::from_raw_buffers(&input, &mut output, frames) };
    operation(audio);
}

#[test]
fn in_place_buffers_preserve_event_timing_stereo_and_backing_tails() {
    let mut bridge = bridge();
    let mut samples = [[0.5; 64], [1.0; 64]];
    let events = [ParamValueEvent::new(
        3,
        CLAP_GAIN_ID,
        Pckn::match_all(),
        2.0,
    )];
    with_in_place(&mut samples, 8, |mut audio| {
        assert_eq!(
            bridge.process(&mut audio, &InputEvents::from_buffer(&events)),
            Ok(PortableStatus::Continue)
        );
    });
    assert_eq!(&samples[0][..8], &[0.5, 0.5, 0.5, 1.0, 1.0, 1.0, 1.0, 1.0]);
    assert_eq!(&samples[1][..8], &[1.0, 1.0, 1.0, 2.0, 2.0, 2.0, 2.0, 2.0]);
    assert_eq!(&samples[0][8..], &[0.5; 56]);
    assert_eq!(&samples[1][8..], &[1.0; 56]);
}

#[test]
fn zero_frame_events_update_without_touching_host_samples() {
    let mut bridge = bridge();
    let mut samples = [[3.0; 64]; 2];
    let events = [ParamValueEvent::new(
        0,
        CLAP_GAIN_ID,
        Pckn::match_all(),
        0.5,
    )];
    with_in_place(&mut samples, 0, |mut audio| {
        assert_eq!(
            bridge.process(&mut audio, &InputEvents::from_buffer(&events)),
            Ok(PortableStatus::Continue)
        );
    });
    assert_eq!(bridge.processor().requested_gain(), 0.5);
    assert_eq!(samples, [[3.0; 64]; 2]);
}

#[test]
fn invalid_events_reject_whole_block_silence_and_leave_gain_unchanged() {
    let cases = [
        vec![
            ParamValueEvent::new(0, CLAP_GAIN_ID, Pckn::match_all(), 0.0),
            ParamValueEvent::new(8, CLAP_GAIN_ID, Pckn::match_all(), 2.0),
        ],
        vec![
            ParamValueEvent::new(3, CLAP_GAIN_ID, Pckn::match_all(), 0.0),
            ParamValueEvent::new(1, CLAP_GAIN_ID, Pckn::match_all(), 2.0),
        ],
        vec![ParamValueEvent::new(
            0,
            CLAP_GAIN_ID,
            Pckn::match_all(),
            f64::INFINITY,
        )],
        vec![ParamValueEvent::new(
            0,
            CLAP_GAIN_ID,
            Pckn::match_all(),
            f64::MAX,
        )],
    ];
    for events in cases {
        let mut bridge = bridge();
        let mut samples = [[3.0; 64]; 2];
        with_in_place(&mut samples, 8, |mut audio| {
            assert_eq!(
                bridge.process(&mut audio, &InputEvents::from_buffer(&events)),
                Err(BridgeError::InvalidEvent)
            );
        });
        assert_eq!(bridge.processor().requested_gain(), 1.0);
        for channel in samples {
            assert_eq!(&channel[..8], &[0.0; 8]);
            assert_eq!(&channel[8..], &[3.0; 56]);
        }
    }
}

#[test]
fn event_overflow_is_rejected_and_configured_smaller_capacity_is_honored() {
    let events =
        vec![ParamValueEvent::new(0, CLAP_GAIN_ID, Pckn::match_all(), 0.5); MAXIMUM_EVENTS + 1];
    let mut scratch = ParameterEvents::new();
    assert_eq!(
        scratch.translate(&InputEvents::from_buffer(&events), 1, false),
        Err(BridgeError::TooManyEvents)
    );
    let mut smaller = ParameterEvents::with_capacity(1).unwrap();
    assert_eq!(
        smaller.translate(&InputEvents::from_buffer(&&events[..2]), 1, false),
        Err(BridgeError::TooManyEvents)
    );
    assert!(ParameterEvents::with_capacity(MAXIMUM_EVENTS + 1).is_err());
}

#[test]
fn unsupported_f64_is_silenced_and_does_not_apply_events() {
    let mut bridge = bridge();
    let mut input = [[1.0_f64; 8]; 2];
    let mut output = [[3.0_f64; 8]; 2];
    let mut input_ports = AudioPorts::with_capacity(2, 1);
    let mut output_ports = AudioPorts::with_capacity(2, 1);
    let input = input_ports.with_input_buffers([AudioPortBuffer {
        latency: 0,
        channels: AudioPortBufferType::f64_input_only(input.iter_mut().map(InputChannel::variable)),
    }]);
    let output_buffers = output_ports.with_output_buffers([AudioPortBuffer {
        latency: 0,
        channels: AudioPortBufferType::f64_output_only(output.iter_mut().map(|c| c.as_mut_slice())),
    }]);
    let mut audio = output_buffers.to_plugin_audio_with_inputs(&input);
    let events = [ParamValueEvent::new(
        0,
        CLAP_GAIN_ID,
        Pckn::match_all(),
        0.5,
    )];
    assert_eq!(
        bridge.process(&mut audio, &InputEvents::from_buffer(&events)),
        Err(BridgeError::UnsupportedSampleType)
    );
    assert_eq!(output, [[0.0; 8]; 2]);
    assert_eq!(bridge.processor().requested_gain(), 1.0);
}

#[test]
fn in_place_translation_and_faults_have_no_heap_activity() {
    let mut bridge = bridge();
    let mut samples = [[0.5; 64]; 2];
    let mut failed = false;
    let info = allocation_counter::measure(|| {
        for index in 0..10_000 {
            let events = [ParamValueEvent::new(
                0,
                CLAP_GAIN_ID,
                Pckn::match_all(),
                if index % 17 == 0 { f64::NAN } else { 0.5 },
            )];
            let frames = [0, 1, 3, 64][index % 4];
            with_in_place(&mut samples, frames, |mut audio| {
                let result = bridge.process(&mut audio, &InputEvents::from_buffer(&events));
                if (index % 17 == 0 && result != Err(BridgeError::InvalidEvent))
                    || (index % 17 != 0 && result.is_err())
                {
                    failed = true;
                }
            });
            std::hint::black_box(&samples);
        }
    });
    assert!(!failed);
    assert_eq!(info.count_total, 0, "in-place allocation: {info:?}");
    assert_eq!(info.count_current, 0, "in-place free: {info:?}");
    assert_eq!(info.bytes_current, 0);
}

#[test]
fn event_capacity_is_checked_before_calling_host_event_accessors() {
    use clack_plugin::events::{UnknownEvent, io::InputEventBuffer};
    use std::cell::Cell;
    struct Oversized {
        gets: Cell<usize>,
    }
    impl InputEventBuffer for Oversized {
        fn len(&self) -> u32 {
            (MAXIMUM_EVENTS + 1) as u32
        }
        fn get(&self, _index: u32) -> Option<&UnknownEvent> {
            self.gets.set(self.gets.get() + 1);
            None
        }
    }
    let input = Oversized { gets: Cell::new(0) };
    assert_eq!(
        ParameterEvents::new().translate(&InputEvents::from_buffer(&input), 1, false),
        Err(BridgeError::TooManyEvents)
    );
    assert_eq!(input.gets.get(), 0);
}

#[test]
#[allow(unsafe_code)]
fn truncated_known_parameter_payload_is_rejected() {
    let header = clap_sys::events::clap_event_header {
        size: std::mem::size_of::<clap_sys::events::clap_event_header>() as u32,
        time: 0,
        space_id: 0,
        type_: clap_sys::events::CLAP_EVENT_PARAM_VALUE,
        flags: 0,
    };
    // SAFETY: the aligned header allocation contains exactly the declared byte
    // count and lives until translation returns. No typed parameter reference
    // is created; the translator must reject the missing payload before casting.
    let event = unsafe { clack_plugin::events::UnknownEvent::from_raw(&header) };
    let events = [event];
    assert_eq!(
        ParameterEvents::new().translate(&InputEvents::from_buffer(&events), 1, false),
        Err(BridgeError::InvalidEvent)
    );
}

#[test]
fn mismatched_layout_and_oversized_blocks_are_silent_without_applying_events() {
    for (layout, maximum, error) in [
        (AudioLayout::MONO, 64, BridgeError::LayoutMismatch),
        (AudioLayout::STEREO, 8, BridgeError::TooManyFrames),
    ] {
        let mut bridge = EffectBridge::prepare(
            GainProcessor::new(0.0).unwrap(),
            ProcessConfig::new(48_000.0, maximum, MAXIMUM_EVENTS, layout).unwrap(),
        )
        .unwrap();
        let mut samples = [[3.0; 64]; 2];
        let events = [ParamValueEvent::new(
            0,
            CLAP_GAIN_ID,
            Pckn::match_all(),
            0.5,
        )];
        with_in_place(&mut samples, 16, |mut audio| {
            assert_eq!(
                bridge.process(&mut audio, &InputEvents::from_buffer(&events)),
                Err(error)
            );
        });
        assert_eq!(bridge.processor().requested_gain(), 1.0);
        for channel in samples {
            assert_eq!(&channel[..16], &[0.0; 16]);
            assert_eq!(&channel[16..], &[3.0; 48]);
        }
    }
}

fn host_transport(samples: i64) -> clack_plugin::events::event_types::TransportEvent {
    use clack_plugin::events::{
        EventFlags, EventHeader,
        event_types::{TransportEvent, TransportFlags},
    };
    use clack_plugin::utils::{BeatTime, SecondsTime};
    TransportEvent {
        header: EventHeader::new_core(0, EventFlags::empty()),
        flags: TransportFlags::HAS_TEMPO
            | TransportFlags::HAS_SECONDS_TIMELINE
            | TransportFlags::HAS_BEATS_TIMELINE
            | TransportFlags::IS_PLAYING,
        song_pos_beats: BeatTime::from_float(3.5),
        song_pos_seconds: SecondsTime::from_float(samples as f64 / 48000.0),
        tempo: 123.0,
        tempo_inc: 0.0,
        loop_start_beats: BeatTime::from_float(0.0),
        loop_end_beats: BeatTime::from_float(4.0),
        loop_start_seconds: SecondsTime::from_float(0.0),
        loop_end_seconds: SecondsTime::from_float(2.0),
        bar_start: BeatTime::from_float(0.0),
        bar_number: 0,
        time_signature_numerator: 4,
        time_signature_denominator: 4,
    }
}

#[test]
fn transport_conversion_ignores_unadvertised_fields_and_rejects_invalid_known_values() {
    use clack_plugin::events::event_types::TransportFlags;
    use ravet_clap::translate_transport;
    use ravet_foundation::Transport;
    assert_eq!(translate_transport(None, 48000.0), Ok(Transport::default()));
    let mut event = host_transport(-24000);
    let context = translate_transport(Some(&event), 48000.0).unwrap();
    assert_eq!(context.sample_position, Some(-24000));
    assert_eq!(context.beat_position, Some(3.5));
    assert_eq!(context.tempo, Some(123.0));
    assert_eq!(context.playing, Some(true));
    for tempo in [0.0, -1.0, f64::INFINITY, f64::NAN] {
        event.tempo = tempo;
        assert_eq!(
            translate_transport(Some(&event), 48000.0),
            Err(BridgeError::InvalidTransport)
        );
    }
    event.flags = TransportFlags::empty();
    assert_eq!(
        translate_transport(Some(&event), 48000.0).unwrap(),
        Transport {
            playing: Some(false),
            details: Some(ravet_foundation::TransportDetails::default()),
            ..Transport::default()
        }
    );
    event = host_transport(i64::MAX);
    assert_eq!(
        translate_transport(Some(&event), f64::MAX),
        Err(BridgeError::InvalidTransport)
    );
    assert_eq!(
        translate_transport(None, f64::NAN),
        Err(BridgeError::InvalidTransport)
    );
}

#[test]
fn bridge_forwards_context_and_tracks_seeks_only_after_accepted_blocks() {
    use ravet_foundation::{PrepareError, ProcessBlock, Processor, Transport};
    #[derive(Default)]
    struct Probe {
        context: Transport,
        calls: usize,
    }
    impl Processor for Probe {
        fn prepare(&mut self, _: ProcessConfig) -> Result<(), PrepareError> {
            Ok(())
        }
        fn process(&mut self, block: &mut ProcessBlock<'_, '_, '_>) -> PortableStatus {
            self.context = block.transport;
            self.calls += 1;
            block.audio.clear_outputs();
            PortableStatus::Silence
        }
        fn reset(&mut self) {}
        fn deactivate(&mut self) {}
    }
    let config = ProcessConfig::new(48000.0, 64, 8, AudioLayout::STEREO).unwrap();
    let mut bridge = EffectBridge::prepare(Probe::default(), config).unwrap();
    let mut samples = [[3.0; 64]; 2];
    for (position, discontinuity) in [(0, false), (8, false), (200, true), (208, false)] {
        with_in_place(&mut samples, 8, |mut audio| {
            assert!(
                bridge
                    .process_with_transport(
                        &mut audio,
                        &InputEvents::empty(),
                        Some(&host_transport(position))
                    )
                    .is_ok()
            );
        });
        assert_eq!(bridge.processor().context.discontinuity, discontinuity);
        assert_eq!(bridge.processor().context.sample_position, Some(position));
    }
    let mut bad = host_transport(216);
    bad.tempo = f64::NAN;
    with_in_place(&mut samples, 8, |mut audio| {
        assert_eq!(
            bridge.process_with_transport(&mut audio, &InputEvents::empty(), Some(&bad)),
            Err(BridgeError::InvalidTransport)
        );
    });
    assert_eq!(bridge.processor().calls, 4);
    assert_eq!(&samples[0][..8], &[0.0; 8]);
    with_in_place(&mut samples, 8, |mut audio| {
        assert!(
            bridge
                .process_with_transport(
                    &mut audio,
                    &InputEvents::empty(),
                    Some(&host_transport(216))
                )
                .is_ok()
        );
    });
    assert!(!bridge.processor().context.discontinuity);
    bridge.reset();
    with_in_place(&mut samples, 8, |mut audio| {
        assert!(
            bridge
                .process_with_transport(
                    &mut audio,
                    &InputEvents::empty(),
                    Some(&host_transport(-100))
                )
                .is_ok()
        );
    });
    assert!(!bridge.processor().context.discontinuity);
    with_in_place(&mut samples, 8, |mut audio| {
        bridge.process(&mut audio, &InputEvents::empty()).unwrap();
    });
    assert_eq!(bridge.processor().context, Transport::default());
}
