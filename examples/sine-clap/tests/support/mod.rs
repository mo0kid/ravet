use clack_extensions::{audio_ports::*, note_ports::*, params::*, state::*, tail::*};
use clack_host::prelude::*;
use clack_plugin::events::{
    event_types::{MidiEvent, ParamValueEvent},
    io::EventBuffer,
};
use ravet_sine_clap::{LEVEL_ID, encode_state};
/// Trusted reference-only ABI exercise, shared by linked and built-bundle checks.
pub fn exercise(entry: &PluginEntry) -> Result<usize, Box<dyn std::error::Error>> {
    let factory = entry.get_plugin_factory().ok_or("missing factory")?;
    let descriptors: Vec<_> = factory.plugin_descriptors().collect();
    assert_eq!(descriptors.len(), 1);
    assert_eq!(descriptors[0].id(), Some(c"org.ravet.sine"));
    for _ in 0..3 {
        let mut instance = ravet_clap_smoke::instance(entry, c"org.ravet.sine")?;
        let handle = instance.plugin_handle();
        let ports = handle
            .get_extension::<PluginAudioPorts>()
            .ok_or("audio ports missing")?;
        let notes = handle
            .get_extension::<PluginNotePorts>()
            .ok_or("MIDI port missing")?;
        let params = handle
            .get_extension::<PluginParams>()
            .ok_or("params missing")?;
        let state = handle
            .get_extension::<PluginState>()
            .ok_or("state missing")?;
        let tail = handle.get_extension::<PluginTail>().ok_or("tail missing")?;
        assert_eq!(ports.count(&handle, true), 0);
        assert_eq!(ports.count(&handle, false), 1);
        let mut audio_info = AudioPortInfoBuffer::new();
        let port = ports.get(&handle, 0, false, &mut audio_info).unwrap();
        assert_eq!(port.channel_count, 2);
        assert_eq!(port.in_place_pair, None);
        assert_eq!(notes.count(&handle, true), 1);
        assert_eq!(notes.count(&handle, false), 0);
        let mut note_info = NotePortInfoBuffer::new();
        let port = notes.get(&handle, 0, true, &mut note_info).unwrap();
        assert_eq!(port.supported_dialects, NoteDialects::MIDI);
        assert_eq!(port.preferred_dialect, Some(NoteDialect::Midi));
        assert_eq!(params.count(&handle), 1);
        assert_eq!(
            params.get_value(&handle, LEVEL_ID),
            Some(f64::from(0.2_f32))
        );
        let valid = encode_state(0.8).unwrap();
        state.load(&handle, &mut valid.as_slice())?;
        for length in 0..16 {
            assert!(state.load(&handle, &mut valid[..length].as_ref()).is_err());
        }
        let mut bad = valid;
        bad[15] = 1;
        assert!(state.load(&handle, &mut bad.as_slice()).is_err());
        let mut bytes = Vec::new();
        state.save(&handle, &mut bytes)?;
        assert_eq!(bytes, valid);
        let stopped = instance.activate(
            |_, _| (),
            PluginAudioConfiguration {
                sample_rate: 48000.,
                min_frames_count: 1,
                max_frames_count: 4096,
            },
        )?;
        let (ready, waiting) = std::sync::mpsc::sync_channel(0);
        let (resume, restored) = std::sync::mpsc::sync_channel(0);
        let (program_ready, program_waiting) = std::sync::mpsc::sync_channel(0);
        let (program_resume, program_resumed) = std::sync::mpsc::sync_channel(0);
        let stopped = std::thread::scope(|scope| {
            let worker = scope.spawn(move || {
                let mut started = stopped.start_processing().unwrap();
                assert_eq!(tail.get(&started.plugin_handle()), TailLength::Infinite);
                let mut samples = [[99_f32; 4096]; 2];
                let mut output_ports = AudioPorts::with_capacity(2, 1);
                let empty = InputAudioBuffers::empty();
                let notes = [
                    MidiEvent::new(4, 0, [0x90, 69, 127]),
                    MidiEvent::new(1024, 0, [0x80, 69, 0]),
                ];
                {
                    let mut output = output_ports.with_output_buffers([AudioPortBuffer {
                        latency: 0,
                        channels: AudioPortBufferType::f32_output_only(
                            samples.iter_mut().map(|c| c.as_mut_slice()),
                        ),
                    }]);
                    started
                        .process(
                            &empty,
                            &mut output,
                            &InputEvents::from_buffer(&notes),
                            &mut OutputEvents::void(),
                            None,
                            None,
                        )
                        .unwrap();
                }
                assert_eq!(samples[0], samples[1]);
                assert_eq!(samples[0][..5], [0.; 5]);
                assert!(samples[0][240..1024].iter().any(|s| s.abs() > 0.03));
                assert!(samples[0][3424..].iter().all(|s| *s == 0.));
                // Actual MIDI ABI: sostenuto retains a key; sustain can take
                // over before CC66 releases; the final pedal starts release.
                let pedal_blocks = [
                    &[
                        MidiEvent::new(0, 0, [0x90, 69, 127]),
                        MidiEvent::new(10, 0, [0xb0, 66, 64]),
                        MidiEvent::new(1024, 0, [0x80, 69, 0]),
                    ][..],
                    &[
                        MidiEvent::new(0, 0, [0xb0, 64, 64]),
                        MidiEvent::new(7, 0, [0xb0, 66, 63]),
                    ][..],
                    &[MidiEvent::new(11, 0, [0xb0, 64, 63])][..],
                ];
                for (stage, events) in pedal_blocks.iter().enumerate() {
                    {
                        let mut output = output_ports.with_output_buffers([AudioPortBuffer {
                            latency: 0,
                            channels: AudioPortBufferType::f32_output_only(
                                samples.iter_mut().map(|c| c.as_mut_slice()),
                            ),
                        }]);
                        let counts = allocation_counter::measure(|| {
                            started
                                .process(
                                    &empty,
                                    &mut output,
                                    &InputEvents::from_buffer(events),
                                    &mut OutputEvents::void(),
                                    None,
                                    None,
                                )
                                .unwrap();
                        });
                        assert_eq!(counts.count_total, 0);
                        assert_eq!(counts.count_current, 0);
                    }
                    assert_eq!(samples[0], samples[1]);
                    if stage < 2 {
                        assert!(samples[0][3500..].iter().any(|s| s.abs() > 0.03));
                    } else {
                        assert!(samples[0][..2410].iter().any(|s| s.abs() > 0.03));
                        assert!(samples[0][2410..].iter().all(|s| *s == 0.));
                    }
                }
                // Program selection travels through raw MIDI, at exact offsets,
                // preserves the sounding oscillator, and reflects persisted level.
                let programs = [
                    MidiEvent::new(0, 0, [0xc0, 2, 0]),
                    MidiEvent::new(0, 0, [0x90, 69, 127]),
                    MidiEvent::new(1024, 0, [0xc0, 1, 0]),
                    MidiEvent::new(2048, 0, [0xc0, 0, 0]),
                    MidiEvent::new(3072, 0, [0xc1, 2, 0]), // Wrong program channel.
                    MidiEvent::new(3200, 0, [0xb0, 0, 127]),
                    MidiEvent::new(3201, 0, [0xc0, 2, 0]), // Unavailable bank.
                    MidiEvent::new(3300, 0, [0xb0, 0, 0]),
                    MidiEvent::new(3301, 0, [0xc0, 127, 0]), // Unavailable program.
                    MidiEvent::new(3500, 0, [0x80, 69, 0]),
                ];
                {
                    let mut output = output_ports.with_output_buffers([AudioPortBuffer {
                        latency: 0,
                        channels: AudioPortBufferType::f32_output_only(
                            samples.iter_mut().map(|c| c.as_mut_slice()),
                        ),
                    }]);
                    let counts = allocation_counter::measure(|| {
                        started
                            .process(
                                &empty,
                                &mut output,
                                &InputEvents::from_buffer(&programs),
                                &mut OutputEvents::void(),
                                None,
                                None,
                            )
                            .unwrap();
                    });
                    assert_eq!(counts.count_total, 0);
                    assert_eq!(counts.count_current, 0);
                }
                assert_eq!(samples[0], samples[1]);
                for (range, level) in [(720..1024, 1.), (1400..2048, 0.05), (2400..3500, 0.2)] {
                    for i in range {
                        let expected =
                            (std::f64::consts::TAU * 440. / 48000. * i as f64).sin() * 0.8 * level
                                / 16.;
                        assert!((f64::from(samples[0][i]) - expected).abs() < 1e-7);
                    }
                }
                program_ready.send(()).unwrap();
                program_resumed.recv().unwrap();
                {
                    let mut output = output_ports.with_output_buffers([AudioPortBuffer {
                        latency: 0,
                        channels: AudioPortBufferType::f32_output_only(
                            samples.iter_mut().map(|c| c.as_mut_slice()),
                        ),
                    }]);
                    let restore = [ParamValueEvent::new(0, LEVEL_ID, Pckn::match_all(), 0.8)];
                    started
                        .process(
                            &empty,
                            &mut output,
                            &InputEvents::from_buffer(&restore),
                            &mut OutputEvents::void(),
                            None,
                            None,
                        )
                        .unwrap();
                }
                assert!(samples[0][1804..].iter().all(|s| *s == 0.));
                // Parameter preceding an invalid note must not change persistent level.
                let mut invalid = EventBuffer::with_capacity(2);
                invalid.push(&ParamValueEvent::new(0, LEVEL_ID, Pckn::match_all(), 0.1));
                invalid.push(&MidiEvent::new(0, 0, [0x90, 255, 100]));
                {
                    let mut output = output_ports.with_output_buffers([AudioPortBuffer {
                        latency: 0,
                        channels: AudioPortBufferType::f32_output_only(
                            samples.iter_mut().map(|c| c.as_mut_slice()),
                        ),
                    }]);
                    started
                        .process(
                            &empty,
                            &mut output,
                            &invalid.as_input(),
                            &mut OutputEvents::void(),
                            None,
                            None,
                        )
                        .unwrap();
                }
                assert_eq!(samples, [[0.; 4096]; 2]);
                params.flush_active(
                    &mut started.plugin_handle(),
                    &InputEvents::from_buffer(&[ParamValueEvent::new(
                        500,
                        LEVEL_ID,
                        Pckn::match_all(),
                        0.5,
                    )]),
                    &mut OutputEvents::void(),
                );
                started.reset();
                {
                    let mut output = output_ports.with_output_buffers([AudioPortBuffer {
                        latency: 0,
                        channels: AudioPortBufferType::f32_output_only(
                            samples.iter_mut().map(|c| &mut c[..32]),
                        ),
                    }]);
                    let events = [MidiEvent::new(0, 0, [0x90, 69, 127])];
                    let counts = allocation_counter::measure(|| {
                        for _ in 0..1000 {
                            started.reset();
                            started
                                .process(
                                    &empty,
                                    &mut output,
                                    &InputEvents::from_buffer(&events),
                                    &mut OutputEvents::void(),
                                    None,
                                    None,
                                )
                                .unwrap();
                        }
                    });
                    assert_eq!(counts.count_total, 0);
                    assert_eq!(counts.count_current, 0);
                }
                ready.send(()).unwrap();
                restored.recv().unwrap();
                // Active restore resets the held note before audio runs again.
                {
                    let mut output = output_ports.with_output_buffers([AudioPortBuffer {
                        latency: 0,
                        channels: AudioPortBufferType::f32_output_only(
                            samples.iter_mut().map(|c| c.as_mut_slice()),
                        ),
                    }]);
                    started
                        .process(
                            &empty,
                            &mut output,
                            &InputEvents::empty(),
                            &mut OutputEvents::void(),
                            None,
                            None,
                        )
                        .unwrap();
                }
                assert_eq!(samples, [[0.; 4096]; 2]);
                started.stop_processing()
            });
            program_waiting.recv().unwrap();
            assert_eq!(
                params.get_value(&instance.plugin_handle(), LEVEL_ID),
                Some(f64::from(0.2_f32))
            );
            let mut program_record = Vec::new();
            state
                .save(&instance.plugin_handle(), &mut program_record)
                .unwrap();
            assert_eq!(program_record, encode_state(0.2).unwrap());
            instance.call_on_main_thread_callback();
            program_resume.send(()).unwrap();
            waiting.recv().unwrap();
            state
                .load(
                    &instance.plugin_handle(),
                    &mut encode_state(0.3).unwrap().as_slice(),
                )
                .unwrap();
            resume.send(()).unwrap();
            worker.join().unwrap()
        });
        instance.deactivate(stopped);
        let mut bytes = Vec::new();
        state.save(&instance.plugin_handle(), &mut bytes)?;
        assert_eq!(bytes, encode_state(0.3).unwrap());
    }
    Ok(3024)
}
