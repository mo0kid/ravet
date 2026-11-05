//! The delay's CLAP export, hosted in process through the CLAP ABI.
use clack_extensions::params::PluginParams;
use clack_host::events::event_types::ParamValueEvent;
use clack_host::prelude::*;
use ravet_delay_plugin::DelayPlugin;
use ravet_plugin::{__private::clack_plugin::entry::SinglePluginEntry, clap::ClapPlugin};

const PLUGIN_ID: &std::ffi::CStr = c"org.ravet.delay";
const MIX: ClapId = ClapId::new(3);
const BLOCK: usize = 256;

fn entry() -> PluginEntry {
    PluginEntry::load_from_clack::<SinglePluginEntry<ClapPlugin<DelayPlugin>>>(c"delay.clap")
        .unwrap()
}

#[test]
fn smoke_host_fixture_passes() {
    assert_eq!(
        ravet_clap_smoke::delay::exercise_delay(&entry()).unwrap(),
        12
    );
}

/// Render one stereo block with `events`; returns (output, allocations).
fn render(
    started: &mut StartedPluginAudioProcessor<ravet_clap_smoke::SmokeHost>,
    #[allow(clippy::ptr_arg)] events: &Vec<ParamValueEvent>,
) -> ([[f32; BLOCK]; 2], u64) {
    let mut input_ports = AudioPorts::with_capacity(2, 1);
    let mut output_ports = AudioPorts::with_capacity(2, 1);
    let mut input = [[0.0_f32; BLOCK]; 2];
    input[0][0] = 1.0;
    input[1][0] = 1.0;
    let mut output = [[f32::NAN; BLOCK]; 2];
    let inputs = input_ports.with_input_buffers([AudioPortBuffer {
        latency: 0,
        channels: AudioPortBufferType::f32_input_only(input.iter_mut().map(InputChannel::variable)),
    }]);
    let mut outputs = output_ports.with_output_buffers([AudioPortBuffer {
        latency: 0,
        channels: AudioPortBufferType::f32_output_only(output.iter_mut().map(|c| c.as_mut_slice())),
    }]);
    let events = InputEvents::from_buffer(events);
    let allocations = allocation_counter::measure(|| {
        started
            .process(
                &inputs,
                &mut outputs,
                &events,
                &mut OutputEvents::void(),
                None,
                None,
            )
            .unwrap();
    })
    .count_total;
    (output, allocations)
}

#[test]
fn oversized_event_blocks_are_silenced_without_allocation() {
    let entry = entry();
    let mut instance = ravet_clap_smoke::instance(&entry, PLUGIN_ID).unwrap();
    let stopped = instance
        .activate(
            |_, _| (),
            PluginAudioConfiguration {
                sample_rate: 48000.0,
                min_frames_count: 1,
                max_frames_count: BLOCK as u32,
            },
        )
        .unwrap();
    let mut started = stopped.start_processing().unwrap();
    let full: Vec<_> = (0..1024)
        .map(|_| ParamValueEvent::new(0, MIX, Pckn::match_all(), 0.0))
        .collect();
    let flood: Vec<_> = (0..1025)
        .map(|_| ParamValueEvent::new(0, MIX, Pckn::match_all(), 100.0))
        .collect();
    render(&mut started, &Vec::new());

    // A full block is accepted: the mix glides towards the dry impulse.
    let (output, allocations) = render(&mut started, &full);
    assert_eq!(allocations, 0);
    assert!(output[0][0] > 0.5);

    // One event more is rejected: silence, and the values are not applied.
    let (output, allocations) = render(&mut started, &flood);
    assert_eq!(allocations, 0);
    assert!(output.iter().flatten().all(|sample| *sample == 0.0));

    // Processing continues with the last accepted values.
    let (output, allocations) = render(&mut started, &Vec::new());
    assert_eq!(allocations, 0);
    assert!(output[0][0] > 0.5);
    let stopped = started.stop_processing();
    instance.deactivate(stopped);
    let handle = instance.plugin_handle();
    let params = handle.get_extension::<PluginParams>().unwrap();
    assert_eq!(params.get_value(&handle, MIX), Some(0.0));
}
