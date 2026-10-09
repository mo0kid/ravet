//! Explicit trusted stereo f32 effect lifecycle/handoff validation, without devices.
use clack_host::prelude::*;
use ravet_host::{
    ClapLibrary, HostRequests, HostedAudioConfiguration, HostedAudioItem, HostedAudioSwap,
    RequestHost, hosted_audio_handoff,
};
use std::{ffi::CString, path::PathBuf};
#[allow(unsafe_code)]
fn load(path: &std::path::Path) -> Result<ClapLibrary, ravet_host::HostError> {
    // SAFETY: explicit caller-selected trusted compliant native library. No
    // discovery/scanning, sandbox or trust inference from filenames occurs.
    unsafe { ClapLibrary::load(path) }
}
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = std::env::args_os().skip(1).collect();
    if !(2..=3).contains(&args.len()) {
        return Err(
            "usage: audio-handoff <trusted CLAP library> <plugin ID> [expected default gain]"
                .into(),
        );
    }
    let selected = CString::new(args[1].to_str().ok_or("plugin ID must be UTF-8")?)?;
    let expected: f32 = args
        .get(2)
        .map(|s| s.to_str().ok_or("gain must be UTF-8"))
        .transpose()?
        .unwrap_or("1")
        .parse()?;
    if !expected.is_finite() || !(0. ..=4.).contains(&expected) {
        return Err("expected gain must be finite 0..4".into());
    }
    let library = load(&PathBuf::from(&args[0]))?;
    if library.find(&selected).is_none() {
        return Err("selected plugin absent".into());
    }
    let info = HostInfo::new(
        "RAVET explicit audio handoff validation",
        "RAVET",
        "",
        env!("CARGO_PKG_VERSION"),
    )?;
    let mut instances = Vec::new();
    let mut items = Vec::new();
    for tag in 0..2 {
        let mut instance = PluginInstance::<RequestHost>::new(
            |_| HostRequests::new(),
            |_| (),
            library.entry(),
            &selected,
            &info,
        )?;
        let ports = HostedAudioConfiguration::inspect(&mut instance)
            .map_err(|e| format!("audio metadata: {e:?}"))?;
        if ports.inputs.len() != 1
            || ports.outputs.len() != 1
            || ports.inputs[0].channels != 2
            || ports.outputs[0].channels != 2
        {
            return Err("probe requires one stereo input/output port".into());
        }
        let processor = instance.activate(
            |_, _| (),
            PluginAudioConfiguration {
                sample_rate: 48000.,
                min_frames_count: 1,
                max_frames_count: 16,
            },
        )?;
        items.push(HostedAudioItem { tag, processor });
        instances.push(instance);
    }
    drop(library); // Main instances retain the trusted library through shutdown.
    let candidate = items.pop().unwrap();
    let initial = items.pop().unwrap();
    let (mut control, ready) = hosted_audio_handoff(initial, 1).unwrap();
    control.offer(candidate).unwrap();
    let ready = std::thread::spawn(move || {
        let mut engine = ready.start().unwrap();
        let mut input = [[0.25_f32; 16], [-0.5; 16]];
        let mut output = [[f32::NAN; 16]; 2];
        let mut inputs = AudioPorts::with_capacity(2, 1);
        let mut outputs = AudioPorts::with_capacity(2, 1);
        let allocations = allocation_counter::measure(|| {
            for block in 0..2000 {
                if block == 1000 {
                    assert_eq!(
                        engine.apply_pending(),
                        HostedAudioSwap::Applied {
                            previous: 0,
                            current: 1
                        }
                    );
                }
                let input_ports = inputs.with_input_buffers([AudioPortBuffer {
                    latency: 0,
                    channels: AudioPortBufferType::f32_input_only(
                        input.iter_mut().map(InputChannel::variable),
                    ),
                }]);
                let mut output_ports = outputs.with_output_buffers([AudioPortBuffer {
                    latency: 0,
                    channels: AudioPortBufferType::f32_output_only(
                        output.iter_mut().map(|c| c.as_mut_slice()),
                    ),
                }]);
                engine
                    .process(
                        &input_ports,
                        &mut output_ports,
                        &InputEvents::empty(),
                        &mut OutputEvents::void(),
                        Some((block % 1000 * 16) as u64),
                        None,
                    )
                    .unwrap();
                for (actual, input) in output.iter().flatten().zip(input.iter().flatten()) {
                    assert!((*actual - *input * expected).abs() < 1e-6);
                }
            }
        });
        assert_eq!(allocations.count_total, 0);
        assert_eq!(allocations.count_current, 0);
        engine.finish()
    })
    .join()
    .map_err(|_| "audio-owner validation panicked")?;
    let retired = control.take_retired().ok_or("missing retired processor")?;
    assert_eq!(retired.tag, 0);
    instances[0].deactivate(retired.processor);
    let current = ready
        .into_current()
        .ok()
        .ok_or("unexpected pending processor")?;
    assert_eq!(current.tag, 1);
    instances[1].deactivate(current.processor);
    assert!(control.take_retired().is_none());
    println!(
        "PASS 2000 stereo f32 blocks/32000 frames, two native audio-owner starts/stops, bounded swap, zero measured allocations and main-thread deactivation"
    );
    Ok(())
}
