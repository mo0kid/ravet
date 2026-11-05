//! Exact instrument processing through CLAP, including bounded host MIDI output.
use clack_host::{
    events::{event_types::MidiEvent, io::EventBuffer},
    prelude::*,
};
use ravet_plugin::{__private::clack_plugin::entry::SinglePluginEntry, clap::ClapPlugin};
use ravet_synth_plugin::SynthPlugin;

#[test]
fn clap_audio_and_midi_echo_keep_offsets_without_allocation() {
    let entry =
        PluginEntry::load_from_clack::<SinglePluginEntry<ClapPlugin<SynthPlugin>>>(c"synth.clap")
            .unwrap();
    let mut instance = ravet_clap_smoke::instance(&entry, c"org.ravet.evidence-synth").unwrap();
    let handle = instance.plugin_handle();
    let ports = handle
        .get_extension::<clack_extensions::note_ports::PluginNotePorts>()
        .unwrap();
    assert_eq!(ports.count(&handle, true), 1);
    assert_eq!(ports.count(&handle, false), 1);
    let latency = handle
        .get_extension::<clack_extensions::latency::PluginLatency>()
        .unwrap();
    let stopped = instance
        .activate(
            |_, _| (),
            PluginAudioConfiguration {
                sample_rate: 48000.0,
                min_frames_count: 1,
                max_frames_count: 128,
            },
        )
        .unwrap();
    let mut started = stopped.start_processing().unwrap();
    assert_eq!(latency.get(&instance.plugin_handle()), 32);
    let handle = started.plugin_handle();
    let tail = handle
        .get_extension::<clack_extensions::tail::PluginTail>()
        .unwrap();
    assert_eq!(
        tail.get(&handle),
        clack_extensions::tail::TailLength::Finite(96)
    );
    let mut output_ports = AudioPorts::with_capacity(2, 1);
    let inputs = clack_host::process::audio_buffers::InputAudioBuffers::empty();
    let mut audio = [[99.0f32; 128]; 2];
    let mut outputs = output_ports.with_output_buffers([AudioPortBuffer {
        latency: 0,
        channels: AudioPortBufferType::f32_output_only(audio.iter_mut().map(|c| c.as_mut_slice())),
    }]);
    let messages = [
        MidiEvent::new(7, 0, [0x90, 60, 127]),
        MidiEvent::new(15, 0, [0xb0, 1, 64]),
        MidiEvent::new(17, 0, [0x80, 60, 0]),
    ];
    let input = InputEvents::from_buffer(&messages);
    let mut generated = EventBuffer::with_capacity(1024);
    let allocations = allocation_counter::measure(|| {
        started
            .process(
                &inputs,
                &mut outputs,
                &input,
                &mut generated.as_output(),
                None,
                None,
            )
            .unwrap();
    });
    assert_eq!(allocations.count_total, 0);
    assert_eq!(generated.len(), 3);
    for (event, expected) in generated.iter().zip(messages) {
        let event = event.as_event::<MidiEvent>().unwrap();
        assert_eq!(event.as_raw().header.time, expected.as_raw().header.time);
        assert_eq!(event.data(), expected.data());
    }
    assert_eq!(audio[0], audio[1]);
    assert!(audio[0][..39].iter().all(|v| *v == 0.0));
    assert_eq!(audio[0][39], 0.25);
    assert_eq!(audio[0][47], 0.25 * 64.0 / 127.0);
    assert!(audio[0][103..].iter().all(|v| *v == 0.0));
    let stopped = started.stop_processing();
    instance.deactivate(stopped);
}

#[test]
fn activation_reports_latency_after_delivering_cached_controls() {
    use clack_extensions::{latency::PluginLatency, params::PluginParams};
    use clack_host::events::event_types::ParamValueEvent;
    let entry =
        PluginEntry::load_from_clack::<SinglePluginEntry<ClapPlugin<SynthPlugin>>>(c"synth.clap")
            .unwrap();
    let mut instance = ravet_clap_smoke::instance(&entry, c"org.ravet.evidence-synth").unwrap();
    let handle = instance.plugin_handle();
    let params = handle.get_extension::<PluginParams>().unwrap();
    let latency = handle.get_extension::<PluginLatency>().unwrap();
    for (value, expected) in [(0.0, 0), (1.0, 32), (0.0, 0)] {
        let events = [ParamValueEvent::new(
            0,
            ClapId::new(2),
            Pckn::match_all(),
            value,
        )];
        params.flush(
            &mut instance.inactive_plugin_handle().unwrap(),
            &InputEvents::from_buffer(&events),
            &mut OutputEvents::void(),
        );
        let stopped = instance
            .activate(
                |_, _| (),
                PluginAudioConfiguration {
                    sample_rate: 48000.0,
                    min_frames_count: 1,
                    max_frames_count: 128,
                },
            )
            .unwrap();
        assert_eq!(latency.get(&instance.plugin_handle()), expected);
        instance.deactivate(stopped);
    }
}
