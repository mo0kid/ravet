use clack_extensions::{params::*, state::*};
use clack_host::prelude::*;
use clack_plugin::{entry::SinglePluginEntry, events::event_types::MidiEvent};
use ravet_host::{ParameterHost, ParameterMainRequests, ParameterSharedRequests};

#[test]
fn sine_midi_programs_coalesce_main_thread_refresh_and_persist_complete_level() {
    let entry = PluginEntry::load_from_clack::<SinglePluginEntry<ravet_sine_clap::SinePlugin>>(c"")
        .unwrap();
    let shared = ParameterSharedRequests::new();
    let main = ParameterMainRequests::new();
    let info = HostInfo::new("Synthetic program host", "RAVET", "", "0.1").unwrap();
    let mut instance = PluginInstance::<ParameterHost>::new(
        |_| shared.clone(),
        |_| main.clone(),
        &entry,
        c"org.ravet.sine",
        &info,
    )
    .unwrap();
    let handle = instance.plugin_handle();
    let params = handle.get_extension::<PluginParams>().unwrap();
    let state = handle.get_extension::<PluginState>().unwrap();
    for (program, level) in [(1, 0.05_f32), (2, 1.), (0, 0.2)] {
        let stopped = instance
            .activate(
                |_, _| (),
                PluginAudioConfiguration {
                    sample_rate: 48000.,
                    min_frames_count: 1,
                    max_frames_count: 8,
                },
            )
            .unwrap();
        let stopped = std::thread::scope(|scope| {
            scope
                .spawn(move || {
                    let mut started = stopped.start_processing().unwrap();
                    let mut samples = [[99_f32; 8]; 2];
                    let mut ports = AudioPorts::with_capacity(2, 1);
                    let mut output = ports.with_output_buffers([AudioPortBuffer {
                        latency: 0,
                        channels: AudioPortBufferType::f32_output_only(
                            samples.iter_mut().map(|s| s.as_mut_slice()),
                        ),
                    }]);
                    let events = [
                        MidiEvent::new(0, 0, [0xc0, program, 0]),
                        MidiEvent::new(1, 0, [0xc0, program, 0]),
                        MidiEvent::new(2, 0, [0xb0, 0, 127]),
                        MidiEvent::new(3, 0, [0xc0, 2, 0]), // Unavailable bank preserves chosen state.
                    ];
                    let counts = allocation_counter::measure(|| {
                        started
                            .process(
                                &InputAudioBuffers::empty(),
                                &mut output,
                                &InputEvents::from_buffer(&events),
                                &mut OutputEvents::void(),
                                None,
                                None,
                            )
                            .unwrap();
                    });
                    assert_eq!(counts.count_total, 0);
                    assert_eq!(counts.count_current, 0);
                    started.stop_processing()
                })
                .join()
                .unwrap()
        });
        assert!(main.take().rescan.is_empty()); // No audio-thread host rescan.
        assert!(shared.core().service_callback(&mut instance));
        assert_eq!(main.take().rescan, ParamRescanFlags::VALUES);
        assert!(!shared.core().service_callback(&mut instance));
        instance.call_on_main_thread_callback();
        assert!(main.take().rescan.is_empty());
        assert_eq!(
            params.get_value(&instance.plugin_handle(), ravet_sine_clap::LEVEL_ID),
            Some(f64::from(level))
        );
        let mut record = Vec::new();
        state.save(&instance.plugin_handle(), &mut record).unwrap();
        assert_eq!(record, ravet_sine_clap::encode_state(level).unwrap());
        instance.deactivate(stopped);
    }
}
