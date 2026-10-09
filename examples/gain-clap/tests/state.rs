use clack_extensions::{params::PluginParams, state::PluginState};
use clack_host::prelude::*;
use clack_plugin::entry::SinglePluginEntry;
use clack_plugin::events::{
    Event, UnknownEvent, event_types::ParamValueEvent, io::InputEventBuffer,
};
use ravet_gain::state::{GainState, decode_parameters, encode_parameters};
use ravet_gain_clap::{CLAP_GAIN_ID, GainPlugin};
use std::sync::{Barrier, atomic::Ordering, mpsc};

fn entry() -> PluginEntry {
    PluginEntry::load_from_clack::<SinglePluginEntry<GainPlugin>>(c"ravet-gain.clap").unwrap()
}
fn saved(
    state: PluginState,
    instance: &mut PluginInstance<ravet_clap_smoke::SmokeHost>,
) -> Vec<u8> {
    let mut bytes = Vec::new();
    state.save(&instance.plugin_handle(), &mut bytes).unwrap();
    bytes
}

#[test]
fn inactive_restore_is_atomic_notifies_host_and_round_trips_to_another_instance() {
    let entry = entry();
    let mut instance = ravet_clap_smoke::instance(&entry, c"org.ravet.gain").unwrap();
    let state = instance
        .plugin_handle()
        .get_extension::<PluginState>()
        .unwrap();
    let original = GainState::new(0.25).unwrap().encode();
    state
        .load(&instance.plugin_handle(), &mut original.as_slice())
        .unwrap();
    assert_eq!(
        saved(state, &mut instance),
        encode_parameters(&decode_parameters(&original).unwrap()).unwrap()
    );
    assert_eq!(
        instance.access_shared_handler(|s| s.process_requests.load(Ordering::Relaxed)),
        1
    );
    assert_eq!(instance.access_handler(|s| s.value_rescans.get()), 1);
    let mut invalid = Vec::new();
    invalid.push(original[..19].to_vec());
    let mut wrong = original;
    wrong[0] = 0;
    invalid.push(wrong.to_vec());
    wrong = original;
    wrong[8] = 2;
    invalid.push(wrong.to_vec());
    wrong = original;
    wrong[12] = 99;
    invalid.push(wrong.to_vec());
    for gain in [f32::NAN, -1.0, 5.0] {
        wrong = original;
        wrong[16..20].copy_from_slice(&gain.to_le_bytes());
        invalid.push(wrong.to_vec());
    }
    invalid.push([original.as_slice(), &[0; 1024]].concat());
    for bytes in invalid {
        assert!(
            state
                .load(&instance.plugin_handle(), &mut bytes.as_slice())
                .is_err()
        );
        assert_eq!(
            saved(state, &mut instance),
            encode_parameters(&decode_parameters(&original).unwrap()).unwrap()
        );
    }
    assert_eq!(instance.access_handler(|s| s.value_rescans.get()), 1);
    let mut duplicate = ravet_clap_smoke::instance(&entry, c"org.ravet.gain").unwrap();
    let duplicate_state = duplicate
        .plugin_handle()
        .get_extension::<PluginState>()
        .unwrap();
    duplicate_state
        .load(
            &duplicate.plugin_handle(),
            &mut saved(state, &mut instance).as_slice(),
        )
        .unwrap();
    assert_eq!(
        saved(duplicate_state, &mut duplicate),
        encode_parameters(&decode_parameters(&original).unwrap()).unwrap()
    );
}

#[test]
fn active_restore_boundaries_automation_and_reset_have_no_heap_activity() {
    let entry = entry();
    let mut instance = ravet_clap_smoke::instance(&entry, c"org.ravet.gain").unwrap();
    let state = instance
        .plugin_handle()
        .get_extension::<PluginState>()
        .unwrap();
    let params = instance
        .plugin_handle()
        .get_extension::<PluginParams>()
        .unwrap();
    // Also prove inactive restore is used at activation, before any callback.
    state
        .load(
            &instance.plugin_handle(),
            &mut GainState::new(0.25).unwrap().encode().as_slice(),
        )
        .unwrap();
    let stopped = instance
        .activate(
            |_, _| (),
            PluginAudioConfiguration {
                sample_rate: 1000.0,
                min_frames_count: 1,
                max_frames_count: 8,
            },
        )
        .unwrap();
    let (commands, receive) = mpsc::sync_channel::<usize>(0);
    let (complete, completions) = mpsc::sync_channel(0);
    let stopped = std::thread::scope(|scope| {
        let worker = scope.spawn(move || {
            let mut started = stopped.start_processing().unwrap();
            let mut inputs = [[1.0_f32; 8]; 2];
            let mut outputs = [[9.0_f32; 8]; 2];
            let mut input_ports = AudioPorts::with_capacity(2, 1);
            let mut output_ports = AudioPorts::with_capacity(2, 1);
            while let Ok(mode) = receive.recv() {
                let frames = if mode == 1 { 1 } else { 8 };
                let events = [ParamValueEvent::new(
                    0,
                    CLAP_GAIN_ID,
                    Pckn::match_all(),
                    2.0,
                )];
                let info = allocation_counter::measure(|| {
                    match mode {
                        2 => params.flush_active(
                            &mut started.plugin_handle(),
                            &InputEvents::empty(),
                            &mut OutputEvents::void(),
                        ),
                        3 => started.reset(),
                        _ => {}
                    }
                    let input = input_ports.with_input_buffers([AudioPortBuffer {
                        latency: 0,
                        channels: AudioPortBufferType::f32_input_only(
                            inputs
                                .iter_mut()
                                .map(|c| InputChannel::variable(&mut c[..frames])),
                        ),
                    }]);
                    let mut output = output_ports.with_output_buffers([AudioPortBuffer {
                        latency: 0,
                        channels: AudioPortBufferType::f32_output_only(
                            outputs.iter_mut().map(|c| &mut c[..frames]),
                        ),
                    }]);
                    started
                        .process(
                            &input,
                            &mut output,
                            &if mode == 1 {
                                InputEvents::from_buffer(&events)
                            } else {
                                InputEvents::empty()
                            },
                            &mut OutputEvents::void(),
                            None,
                            None,
                        )
                        .unwrap();
                });
                assert_eq!(info.count_total, 0, "restore allocation: {info:?}");
                assert_eq!(info.count_current, 0, "restore deallocation: {info:?}");
                assert_eq!(info.bytes_current, 0);
                for channel in outputs {
                    for (index, actual) in channel[..frames].iter().enumerate() {
                        let expected = if mode == 1 {
                            0.25 + 1.75 * ((index + 1).min(5) as f32 / 5.0)
                        } else {
                            0.25
                        };
                        assert!((actual - expected).abs() < 1e-6);
                    }
                }
                complete.send(()).unwrap();
            }
            started.stop_processing()
        });
        for index in 0..1000 {
            // Repeated identical restores still reset a partially advanced ramp.
            if index != 0 {
                state
                    .load(
                        &instance.plugin_handle(),
                        &mut encode_parameters(
                            &ravet_gain::parameter_schema().state([0.25, 0.0]).unwrap(),
                        )
                        .unwrap()
                        .as_slice(),
                    )
                    .unwrap();
            }
            assert_eq!(
                decode_parameters(&saved(state, &mut instance))
                    .unwrap()
                    .values()[0],
                0.25
            );
            commands.send(index % 4).unwrap();
            completions.recv().unwrap();
            let expected = if index % 4 == 1 { 2.0 } else { 0.25 };
            assert_eq!(
                decode_parameters(&saved(state, &mut instance))
                    .unwrap()
                    .values()[0],
                expected
            );
        }
        drop(commands);
        worker.join().unwrap()
    });
    instance.deactivate(stopped);
}

struct PausedEvent<'a> {
    barrier: &'a Barrier,
    event: ParamValueEvent,
}
impl InputEventBuffer for PausedEvent<'_> {
    fn len(&self) -> u32 {
        1
    }
    fn get(&self, index: u32) -> Option<&UnknownEvent> {
        if index != 0 {
            return None;
        }
        // Deliberately block this test host, never production processing, to
        // place restore after callback snapshot and before callback publication.
        self.barrier.wait();
        self.barrier.wait();
        Some(self.event.as_unknown())
    }
}

#[test]
fn restore_during_callback_wins_and_is_consumed_on_next_callback() {
    let entry = entry();
    let mut instance = ravet_clap_smoke::instance(&entry, c"org.ravet.gain").unwrap();
    let state = instance
        .plugin_handle()
        .get_extension::<PluginState>()
        .unwrap();
    let stopped = instance
        .activate(
            |_, _| (),
            PluginAudioConfiguration {
                sample_rate: 1000.0,
                min_frames_count: 1,
                max_frames_count: 8,
            },
        )
        .unwrap();
    let barrier = Barrier::new(2);
    let stopped = std::thread::scope(|scope| {
        let worker = scope.spawn(|| {
            let mut started = stopped.start_processing().unwrap();
            let paused = PausedEvent {
                barrier: &barrier,
                event: ParamValueEvent::new(0, CLAP_GAIN_ID, Pckn::match_all(), 3.0),
            };
            let mut inputs = [[1.0_f32; 8]; 2];
            let mut outputs = [[9.0_f32; 8]; 2];
            let mut input_ports = AudioPorts::with_capacity(2, 1);
            let mut output_ports = AudioPorts::with_capacity(2, 1);
            for index in 0..2 {
                let input = input_ports.with_input_buffers([AudioPortBuffer {
                    latency: 0,
                    channels: AudioPortBufferType::f32_input_only(
                        inputs.iter_mut().map(InputChannel::variable),
                    ),
                }]);
                let mut output = output_ports.with_output_buffers([AudioPortBuffer {
                    latency: 0,
                    channels: AudioPortBufferType::f32_output_only(
                        outputs.iter_mut().map(|c| c.as_mut_slice()),
                    ),
                }]);
                started
                    .process(
                        &input,
                        &mut output,
                        &if index == 0 {
                            InputEvents::from_buffer(&paused)
                        } else {
                            InputEvents::empty()
                        },
                        &mut OutputEvents::void(),
                        None,
                        None,
                    )
                    .unwrap();
                if index == 1 {
                    assert_eq!(outputs, [[1.0; 8]; 2]);
                }
            }
            started.stop_processing()
        });
        barrier.wait();
        state
            .load(
                &instance.plugin_handle(),
                &mut encode_parameters(&ravet_gain::parameter_schema().state([0.5, 1.0]).unwrap())
                    .unwrap()
                    .as_slice(),
            )
            .unwrap();
        assert_eq!(
            decode_parameters(&saved(state, &mut instance))
                .unwrap()
                .values()[0],
            0.5
        );
        assert_eq!(
            decode_parameters(&saved(state, &mut instance))
                .unwrap()
                .values()[1],
            1.0
        );
        barrier.wait();
        worker.join().unwrap()
    });
    assert_eq!(
        decode_parameters(&saved(state, &mut instance))
            .unwrap()
            .values()[0],
        0.5
    );
    instance.deactivate(stopped);
}

#[test]
fn complete_state_recalls_bypass_and_rejects_partial_or_invalid_records() {
    use ravet_gain::parameter_schema;
    use ravet_gain_clap::CLAP_BYPASS_ID;
    let entry = entry();
    let mut instance = ravet_clap_smoke::instance(&entry, c"org.ravet.gain").unwrap();
    let state = instance
        .plugin_handle()
        .get_extension::<PluginState>()
        .unwrap();
    let params = instance
        .plugin_handle()
        .get_extension::<PluginParams>()
        .unwrap();
    let original = encode_parameters(&parameter_schema().state([0.25, 1.0]).unwrap()).unwrap();
    state
        .load(&instance.plugin_handle(), &mut original.as_slice())
        .unwrap();
    assert_eq!(saved(state, &mut instance), original);
    assert_eq!(
        params.get_value(&instance.plugin_handle(), CLAP_BYPASS_ID),
        Some(1.0)
    );
    for bypass in [0.5, 2.0, f32::NAN] {
        let mut bad = original;
        bad[28..32].copy_from_slice(&bypass.to_le_bytes());
        assert!(
            state
                .load(&instance.plugin_handle(), &mut bad.as_slice())
                .is_err()
        );
        assert_eq!(saved(state, &mut instance), original);
    }
    let stopped = instance
        .activate(
            |_, _| (),
            PluginAudioConfiguration {
                sample_rate: 1000.0,
                min_frames_count: 1,
                max_frames_count: 8,
            },
        )
        .unwrap();
    let stopped = std::thread::scope(|scope| {
        scope
            .spawn(move || {
                let mut started = stopped.start_processing().unwrap();
                let mut inputs = [[0.5_f32; 8], [-1.0; 8]];
                let mut outputs = [[9.0; 8]; 2];
                let mut input_ports = AudioPorts::with_capacity(2, 1);
                let mut output_ports = AudioPorts::with_capacity(2, 1);
                for step in 0..2 {
                    let input = input_ports.with_input_buffers([AudioPortBuffer {
                        latency: 0,
                        channels: AudioPortBufferType::f32_input_only(
                            inputs.iter_mut().map(InputChannel::variable),
                        ),
                    }]);
                    let mut output = output_ports.with_output_buffers([AudioPortBuffer {
                        latency: 0,
                        channels: AudioPortBufferType::f32_output_only(
                            outputs.iter_mut().map(|c| c.as_mut_slice()),
                        ),
                    }]);
                    let events = [ParamValueEvent::new(
                        3,
                        CLAP_BYPASS_ID,
                        Pckn::match_all(),
                        0.0,
                    )];
                    started
                        .process(
                            &input,
                            &mut output,
                            &if step == 0 {
                                InputEvents::empty()
                            } else {
                                InputEvents::from_buffer(&events)
                            },
                            &mut OutputEvents::void(),
                            None,
                            None,
                        )
                        .unwrap();
                    if step == 0 {
                        assert_eq!(outputs, inputs);
                    } else {
                        assert_eq!(
                            outputs[0],
                            [0.5, 0.5, 0.5, 0.125, 0.125, 0.125, 0.125, 0.125]
                        );
                    }
                }
                started.stop_processing()
            })
            .join()
            .unwrap()
    });
    instance.deactivate(stopped);
    assert_eq!(
        decode_parameters(&saved(state, &mut instance))
            .unwrap()
            .values(),
        &[0.25, 0.0]
    );
    // Legacy load clears a previously enabled bypass rather than inheriting it.
    state
        .load(&instance.plugin_handle(), &mut original.as_slice())
        .unwrap();
    state
        .load(
            &instance.plugin_handle(),
            &mut GainState::new(0.5).unwrap().encode().as_slice(),
        )
        .unwrap();
    assert_eq!(
        decode_parameters(&saved(state, &mut instance))
            .unwrap()
            .values(),
        &[0.5, 0.0]
    );
}
