//! Explicit trusted native-library inspection with a bounded offline render.
use clack_extensions::{
    audio_ports::{AudioPortInfoBuffer, PluginAudioPorts},
    state::PluginState,
};
use clack_host::prelude::*;
use ravet_host::{
    AudioConfigurationRequests, ClapLibrary, HostedParameterError, HostedParameters,
    ParameterMainRequests, ParameterSharedRequests, ProcessingHost, ProcessingMainRequests,
    discover_clap,
};
use std::{ffi::CString, path::PathBuf};
#[allow(unsafe_code)]
fn load(path: &std::path::Path) -> Result<ClapLibrary, ravet_host::HostError> {
    // SAFETY: this command is explicitly for caller-selected trusted compliant
    // libraries. It is not a scanner/sandbox for arbitrary native binaries.
    unsafe { ClapLibrary::load(path) }
}
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = std::env::args_os().skip(1);
    let mode = args.next().ok_or(
        "usage: inspect --discover <directory> | --load/--metadata <trusted CLAP library> [plugin ID]",
    )?;
    let path = PathBuf::from(args.next().ok_or("missing path")?);
    if mode == "--discover" {
        if args.next().is_some() {
            return Err("too many arguments".into());
        }
        for candidate in discover_clap(&[path], 1024)? {
            println!("{}", candidate.display());
        }
        return Ok(());
    }
    if mode != "--load" && mode != "--metadata" {
        return Err("unknown mode".into());
    }
    let selected = args
        .next()
        .map(|id| id.into_string().map(CString::new))
        .transpose()
        .map_err(|_| "plugin ID must be UTF-8")?
        .transpose()?;
    if args.next().is_some() {
        return Err("too many arguments".into());
    }
    let library = load(&path)?;
    for plugin in library.plugins() {
        println!(
            "{}: {}",
            plugin.id.to_string_lossy(),
            plugin.name.to_string_lossy()
        );
    }
    let selected = selected.unwrap_or_else(|| library.plugins()[0].id.clone());
    if library.find(&selected).is_none() {
        return Err("selected plugin absent".into());
    }
    let info = HostInfo::new(
        "RAVET explicit offline host",
        "RAVET",
        "",
        env!("CARGO_PKG_VERSION"),
    )?;
    let requests = ParameterSharedRequests::new();
    let parameter_changes = ParameterMainRequests::new();
    let audio_changes = AudioConfigurationRequests::new();
    let main = ProcessingMainRequests::new(parameter_changes.clone(), audio_changes.clone());
    let mut instance = PluginInstance::<ProcessingHost>::new(
        |_| requests.clone(),
        |_| main.clone(),
        library.entry(),
        &selected,
        &info,
    )?;
    requests.core().service_callback(&mut instance);
    let pending = requests.core().take();
    if pending.restart || pending.process || pending.callback {
        println!("Initialization requests deferred to explicit offline lifecycle: {pending:?}");
    }
    let changes = parameter_changes.take();
    if changes.overflow {
        return Err("parameter reference invalidation overflow".into());
    }
    println!(
        "Initialization parameter rescan {:?}, {} clears (no stored references)",
        changes.rescan,
        changes.clears().len()
    );
    // The render below services pending flush through process; this offline host
    // has no stored automation/modulation references to invalidate.
    let flush_requested = requests.take_flush();
    println!("Initialization parameter flush scheduled by offline process: {flush_requested}");
    drop(library); // The instance now retains the loaded code independently.
    match HostedParameters::new(&mut instance, 4096) {
        Err(HostedParameterError::Unavailable) => (),
        Err(error) => return Err(format!("invalid hosted parameter catalogue: {error:?}").into()),
        Ok(mut parameters) => {
            let ids: Vec<_> = parameters
                .parameters()
                .iter()
                .map(|parameter| parameter.id)
                .collect();
            println!("Parameters: {}", ids.len());
            for id in ids {
                println!(
                    "Parameter {} plain value: {}",
                    id.get(),
                    parameters
                        .value(id)
                        .map_err(|_| "invalid parameter value")?
                );
            }
        }
    }
    let audio = ravet_host::HostedAudioConfiguration::inspect(&mut instance)
        .map_err(|error| format!("invalid hosted audio metadata: {error:?}"))?;
    println!(
        "Audio ports: {} inputs/{} outputs; reported latency {:?}",
        audio.inputs.len(),
        audio.outputs.len(),
        ravet_host::hosted_latency(&mut instance)
    );
    for (direction, ports) in [("input", &audio.inputs), ("output", &audio.outputs)] {
        for port in ports {
            println!(
                "{direction} ID {}: {} channels, {:?}, pair {:?}",
                port.id.get(),
                port.channels,
                port.flags,
                port.in_place_pair
            );
        }
    }
    println!("Initialization audio changes: {:?}", audio_changes.take());
    if mode == "--metadata" {
        return Ok(());
    }
    let handle = instance.plugin_handle();
    let ports = handle
        .get_extension::<PluginAudioPorts>()
        .ok_or("missing audio ports")?;
    let mut buffer = AudioPortInfoBuffer::new();
    if ports.count(&handle, true) > 1 || ports.count(&handle, false) != 1 {
        return Err("example supports only zero/one main input and one output".into());
    }
    let inputs = if ports.count(&handle, true) == 0 {
        0
    } else {
        ports
            .get(&handle, 0, true, &mut buffer)
            .ok_or("missing input")?
            .channel_count as usize
    };
    let outputs = ports
        .get(&handle, 0, false, &mut buffer)
        .ok_or("missing output")?
        .channel_count as usize;
    if inputs > 64 || !(1..=64).contains(&outputs) {
        return Err("bounded channel count exceeded".into());
    }
    let state = handle.get_extension::<PluginState>();
    if let Some(state) = state {
        let mut storage = vec![0; 1_048_576];
        let mut cursor = std::io::Cursor::new(storage.as_mut_slice());
        state.save(&handle, &mut cursor)?;
        let count = cursor.position() as usize;
        let mut bytes = &storage[..count];
        state.load(&handle, &mut bytes)?;
        println!("State roundtrip: {count} bytes");
    }
    let stopped = instance.activate(
        |_, _| audio_changes.clone(),
        PluginAudioConfiguration {
            sample_rate: 48000.,
            min_frames_count: 1,
            max_frames_count: 64,
        },
    )?;
    println!(
        "Post-activation reported latency: {:?}; changes {:?}",
        ravet_host::hosted_latency(&mut instance),
        audio_changes.take()
    );
    let stopped = std::thread::spawn(move || {
        let mut input_samples = vec![vec![0.25; 64]; inputs];
        let mut output_samples = vec![vec![0.; 64]; outputs];
        let mut in_ports = AudioPorts::with_capacity(inputs, 1);
        let mut out_ports = AudioPorts::with_capacity(outputs, 1);
        let mut started = stopped.start_processing().unwrap();
        for _ in 0..3 {
            let input = in_ports.with_input_buffers((inputs > 0).then_some(AudioPortBuffer {
                latency: 0,
                channels: AudioPortBufferType::f32_input_only(
                    input_samples.iter_mut().map(InputChannel::variable),
                ),
            }));
            let mut output = out_ports.with_output_buffers([AudioPortBuffer {
                latency: 0,
                channels: AudioPortBufferType::f32_output_only(
                    output_samples
                        .iter_mut()
                        .map(|samples| samples.as_mut_slice()),
                ),
            }]);
            started
                .process(
                    &input,
                    &mut output,
                    &InputEvents::empty(),
                    &mut OutputEvents::void(),
                    None,
                    None,
                )
                .unwrap();
        }
        assert!(
            output_samples
                .iter()
                .flatten()
                .all(|value| value.is_finite())
        );
        let peak = output_samples
            .iter()
            .flatten()
            .map(|value| value.abs())
            .fold(0., f32::max);
        println!("Three offline f32 blocks: {inputs} inputs/{outputs} outputs, peak {peak}");
        println!(
            "Reported audio-owner tail: {:?}",
            ravet_host::hosted_tail(&started.plugin_handle())
        );
        started.stop_processing()
    })
    .join()
    .map_err(|_| "offline processor panicked")?;
    instance.deactivate(stopped);
    requests.core().service_callback(&mut instance);
    let pending = requests.core().take();
    if pending.restart || pending.process || pending.callback {
        println!("Post-render requests deferred; offline example is stopping: {pending:?}");
    }
    println!(
        "Post-render audio changes deferred; stopping: {:?}",
        audio_changes.take()
    );
    let changes = parameter_changes.take();
    if changes.overflow {
        return Err("post-render parameter reference invalidation overflow".into());
    }
    println!(
        "Post-render parameter rescan {:?}, {} clears and flush {} deferred; stopping",
        changes.rescan,
        changes.clears().len(),
        requests.take_flush()
    );
    Ok(())
}
