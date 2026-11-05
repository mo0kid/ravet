//! Development host fixture for the RAVET Delay CLAP product (`ravet-delay-plugin`).
//!
//! Loads the built bundle through the CLAP ABI, checks its parameters, renders an
//! impulse with host tempo on a separate audio thread and checks the synced
//! repeat position, then checks saved-state round trips. It opens no device.
use crate::instance;
use clack_extensions::{
    params::{ParamInfoBuffer, ParamInfoFlags, PluginParams},
    state::PluginState,
};
use clack_host::events::event_types::{ParamValueEvent, TransportEvent, TransportFlags};
use clack_host::events::{EventFlags, EventHeader};
use clack_host::prelude::*;
use clack_host::utils::{BeatTime, SecondsTime};

const PLUGIN_ID: &std::ffi::CStr = c"org.ravet.delay";
const TIME: ClapId = ClapId::new(1);
const FEEDBACK: ClapId = ClapId::new(2);
const MIX: ClapId = ClapId::new(3);
const BYPASS: ClapId = ClapId::new(4);
const SYNC: ClapId = ClapId::new(6);
const DIVISION: ClapId = ClapId::new(7);

const RATE: f64 = 8000.0;
const BLOCK: usize = 256;

fn transport(tempo: f64) -> TransportEvent {
    TransportEvent {
        header: EventHeader::new_core(0, EventFlags::empty()),
        flags: TransportFlags::IS_PLAYING | TransportFlags::HAS_TEMPO,
        song_pos_beats: BeatTime::from_float(0.0),
        song_pos_seconds: SecondsTime::from_float(0.0),
        tempo,
        tempo_inc: 0.0,
        loop_start_beats: BeatTime::from_float(0.0),
        loop_end_beats: BeatTime::from_float(0.0),
        loop_start_seconds: SecondsTime::from_float(0.0),
        loop_end_seconds: SecondsTime::from_float(0.0),
        bar_start: BeatTime::from_float(0.0),
        bar_number: 0,
        time_signature_numerator: 4,
        time_signature_denominator: 4,
    }
}

/// Exercise the delay bundle. Returns the number of processed blocks.
pub fn exercise_delay(entry: &PluginEntry) -> Result<usize, Box<dyn std::error::Error>> {
    let factory = entry.get_plugin_factory().ok_or("missing plugin factory")?;
    let descriptors: Vec<_> = factory.plugin_descriptors().collect();
    assert_eq!(descriptors.len(), 1);
    assert_eq!(descriptors[0].id(), Some(PLUGIN_ID));

    let mut instance = instance(entry, PLUGIN_ID)?;
    let handle = instance.plugin_handle();
    let params = handle
        .get_extension::<PluginParams>()
        .ok_or("missing params")?;
    let state = handle
        .get_extension::<PluginState>()
        .ok_or("missing state")?;

    assert_eq!(params.count(&handle), 8);
    let mut info = ParamInfoBuffer::new();
    for index in 0..8 {
        let param = params
            .get_info(&handle, index, &mut info)
            .ok_or("missing parameter")?;
        assert_eq!(param.id, ClapId::new(index + 1));
        assert!(param.flags.contains(ParamInfoFlags::IS_AUTOMATABLE));
        match index + 1 {
            4 => assert!(param.flags.contains(ParamInfoFlags::IS_BYPASS)),
            7 => {
                assert!(param.flags.contains(ParamInfoFlags::IS_STEPPED));
                assert_eq!(
                    (param.min_value, param.max_value, param.default_value),
                    (0.0, 11.0, 4.0)
                );
            }
            _ => {}
        }
    }
    assert_eq!(params.text_to_value(&handle, TIME, c"250 ms"), Some(250.0));
    assert_eq!(params.text_to_value(&handle, BYPASS, c"On"), Some(1.0));

    // Sync to an eighth note, no feedback, fully wet.
    let setup = [
        ParamValueEvent::new(0, SYNC, Pckn::match_all(), 1.0),
        ParamValueEvent::new(0, DIVISION, Pckn::match_all(), 6.0),
        ParamValueEvent::new(0, FEEDBACK, Pckn::match_all(), 0.0),
        ParamValueEvent::new(0, MIX, Pckn::match_all(), 100.0),
    ];
    params.flush(
        &mut instance
            .inactive_plugin_handle()
            .ok_or("instance still active")?,
        &InputEvents::from_buffer(&setup),
        &mut OutputEvents::void(),
    );
    assert_eq!(
        params.get_value(&instance.plugin_handle(), DIVISION),
        Some(6.0)
    );
    let mut saved = Vec::new();
    state.save(&instance.plugin_handle(), &mut saved)?;

    let stopped = instance.activate(
        |_, _| (),
        PluginAudioConfiguration {
            sample_rate: RATE,
            min_frames_count: 1,
            max_frames_count: BLOCK as u32,
        },
    )?;
    let blocks = 12;
    let stopped = std::thread::scope(|scope| {
        scope
            .spawn(move || {
                let mut started = stopped.start_processing().unwrap();
                let mut input_ports = AudioPorts::with_capacity(2, 1);
                let mut output_ports = AudioPorts::with_capacity(2, 1);
                let host_tempo = transport(120.0);
                let mut render = |started: &mut StartedPluginAudioProcessor<_>, first: f32| {
                    let mut input = [[0.0_f32; BLOCK]; 2];
                    input[0][0] = first;
                    input[1][0] = first;
                    let mut output = [[f32::NAN; BLOCK]; 2];
                    let inputs = input_ports.with_input_buffers([AudioPortBuffer {
                        latency: 0,
                        channels: AudioPortBufferType::f32_input_only(
                            input.iter_mut().map(InputChannel::variable),
                        ),
                    }]);
                    let mut outputs = output_ports.with_output_buffers([AudioPortBuffer {
                        latency: 0,
                        channels: AudioPortBufferType::f32_output_only(
                            output.iter_mut().map(|c| c.as_mut_slice()),
                        ),
                    }]);
                    started
                        .process(
                            &inputs,
                            &mut outputs,
                            &InputEvents::empty(),
                            &mut OutputEvents::void(),
                            None,
                            Some(&host_tempo),
                        )
                        .unwrap();
                    output
                };
                // The first block delivers the tempo; reset completes the glide.
                render(&mut started, 0.0);
                started.reset();
                let mut rendered = [Vec::new(), Vec::new()];
                for block in 0..blocks - 1 {
                    let output = render(&mut started, if block == 0 { 1.0 } else { 0.0 });
                    for (all, channel) in rendered.iter_mut().zip(output) {
                        all.extend_from_slice(&channel);
                    }
                }
                // An eighth note at 120 BPM is 250 ms: 2000 samples at 8 kHz.
                for channel in &rendered {
                    let peaks: Vec<_> = (0..channel.len())
                        .filter(|&i| channel[i].abs() > 1e-6)
                        .collect();
                    assert_eq!(peaks, [2000], "synced repeat position");
                    assert!((channel[2000] - 1.0).abs() < 1e-6);
                }
                started.stop_processing()
            })
            .join()
            .unwrap()
    });
    instance.deactivate(stopped);

    let mut after = Vec::new();
    state.save(&instance.plugin_handle(), &mut after)?;
    assert_eq!(saved, after);
    let change = [ParamValueEvent::new(0, DIVISION, Pckn::match_all(), 2.0)];
    params.flush(
        &mut instance
            .inactive_plugin_handle()
            .ok_or("instance still active")?,
        &InputEvents::from_buffer(&change),
        &mut OutputEvents::void(),
    );
    state.load(&instance.plugin_handle(), &mut saved.as_slice())?;
    assert_eq!(
        params.get_value(&instance.plugin_handle(), DIVISION),
        Some(6.0)
    );
    Ok(blocks)
}
