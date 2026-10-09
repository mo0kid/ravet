use clack_extensions::params::PluginParams;
use clack_host::prelude::*;
use clack_plugin::entry::SinglePluginEntry;
use clack_plugin::events::{event_types::ParamValueEvent, io::InputEvents};
use ravet_gain_clap::{CLAP_GAIN_ID, GainPlugin};

fn entry() -> PluginEntry {
    PluginEntry::load_from_clack::<SinglePluginEntry<GainPlugin>>(c"ravet-gain.clap").unwrap()
}

#[test]
fn host_abi_metadata_automation_flush_and_lifecycle() {
    assert_eq!(ravet_clap_smoke::exercise(&entry()).unwrap(), 18);
}

#[test]
fn inactive_flush_rejects_partial_invalid_updates_and_ignores_unknown_scopes() {
    let entry = entry();
    let mut instance = ravet_clap_smoke::instance(&entry, c"org.ravet.gain").unwrap();
    let params = instance
        .plugin_handle()
        .get_extension::<PluginParams>()
        .unwrap();
    let good = [ParamValueEvent::new(
        0,
        CLAP_GAIN_ID,
        Pckn::match_all(),
        0.5,
    )];
    params.flush(
        &mut instance.inactive_plugin_handle().unwrap(),
        &InputEvents::from_buffer(&good),
        &mut OutputEvents::void(),
    );
    let bad = [
        ParamValueEvent::new(0, CLAP_GAIN_ID, Pckn::match_all(), 2.0),
        ParamValueEvent::new(0, CLAP_GAIN_ID, Pckn::match_all(), f64::NAN),
    ];
    params.flush(
        &mut instance.inactive_plugin_handle().unwrap(),
        &InputEvents::from_buffer(&bad),
        &mut OutputEvents::void(),
    );
    assert_eq!(
        params.get_value(&instance.plugin_handle(), CLAP_GAIN_ID),
        Some(0.5)
    );
    let scoped = [
        ParamValueEvent::new(0, CLAP_GAIN_ID, Pckn::new(0u16, 0u16, 60u16, 1u32), 3.0),
        ParamValueEvent::new(0, ClapId::new(99), Pckn::match_all(), 4.0),
    ];
    params.flush(
        &mut instance.inactive_plugin_handle().unwrap(),
        &InputEvents::from_buffer(&scoped),
        &mut OutputEvents::void(),
    );
    assert_eq!(
        params.get_value(&instance.plugin_handle(), CLAP_GAIN_ID),
        Some(0.5)
    );
}

#[test]
fn abi_processing_including_faults_and_active_flush_does_not_allocate_or_free() {
    let entry = entry();
    let mut instance = ravet_clap_smoke::instance(&entry, c"org.ravet.gain").unwrap();
    let params = instance
        .plugin_handle()
        .get_extension::<PluginParams>()
        .unwrap();
    let stopped = instance
        .activate(
            |_, _| (),
            PluginAudioConfiguration {
                sample_rate: 48_000.0,
                min_frames_count: 1,
                max_frames_count: 64,
            },
        )
        .unwrap();
    let stopped = std::thread::scope(|scope| {
        scope
            .spawn(move || {
                let mut started = stopped.start_processing().unwrap();
                let mut inputs = [[0.5_f32; 64]; 2];
                let mut outputs = [[9.0_f32; 64]; 2];
                let mut input_ports = AudioPorts::with_capacity(2, 1);
                let mut output_ports = AudioPorts::with_capacity(2, 1);
                let mut failed = false;
                let info = allocation_counter::measure(|| {
                    for index in 0..10_000 {
                        let frames = [1, 3, 16, 64][index % 4];
                        let events = [ParamValueEvent::new(
                            0,
                            CLAP_GAIN_ID,
                            Pckn::match_all(),
                            if index % 17 == 0 {
                                f64::NAN
                            } else {
                                (index % 5) as f64
                            },
                        )];
                        let input = input_ports.with_input_buffers([AudioPortBuffer {
                            latency: 0,
                            channels: AudioPortBufferType::f32_input_only(
                                inputs
                                    .iter_mut()
                                    .map(|channel| InputChannel::variable(&mut channel[..frames])),
                            ),
                        }]);
                        let mut output = output_ports.with_output_buffers([AudioPortBuffer {
                            latency: 0,
                            channels: AudioPortBufferType::f32_output_only(
                                outputs.iter_mut().map(|channel| &mut channel[..frames]),
                            ),
                        }]);
                        if started
                            .process(
                                &input,
                                &mut output,
                                &InputEvents::from_buffer(&events),
                                &mut OutputEvents::void(),
                                None,
                                None,
                            )
                            .is_err()
                        {
                            failed = true;
                        }
                        if index % 37 == 0 {
                            let flushed = [ParamValueEvent::new(
                                0,
                                CLAP_GAIN_ID,
                                Pckn::match_all(),
                                0.5,
                            )];
                            params.flush_active(
                                &mut started.plugin_handle(),
                                &InputEvents::from_buffer(&flushed),
                                &mut OutputEvents::void(),
                            );
                            started.reset();
                        }
                        std::hint::black_box(&outputs);
                    }
                });
                assert!(!failed);
                assert_eq!(info.count_total, 0, "ABI callback allocation: {info:?}");
                assert_eq!(info.count_current, 0, "ABI callback free: {info:?}");
                assert_eq!(info.bytes_current, 0);
                started.stop_processing()
            })
            .join()
            .unwrap()
    });
    instance.deactivate(stopped);
}
