//! Development host fixtures for the RAVET Gain CLAP product.
//! This host exercises lifecycle/automation offline; it does not open a device.
use clack_extensions::{
    audio_ports::{AudioPortInfoBuffer, PluginAudioPorts},
    audio_ports_config::PluginAudioPortsConfig,
    params::{
        HostParams, HostParamsImplMainThread, HostParamsImplShared, ParamClearFlags,
        ParamInfoBuffer, ParamInfoFlags, ParamRescanFlags, PluginParams,
    },
    state::PluginState,
};
use clack_host::events::event_types::ParamValueEvent;
use clack_host::prelude::*;
use std::{
    cell::Cell,
    ffi::CStr,
    sync::atomic::{AtomicUsize, Ordering},
};

pub const GAIN_ID: ClapId = ClapId::new(1);
pub const BYPASS_ID: ClapId = ClapId::new(2);
pub struct SmokeHost;
impl HostHandlers for SmokeHost {
    type Shared<'a> = SmokeShared;
    type MainThread<'a> = SmokeMain;
    type AudioProcessor<'a> = ();
    fn declare_extensions(builder: &mut HostExtensions<Self>, _: &SmokeShared) {
        builder.register::<HostParams>();
    }
}

#[derive(Default)]
pub struct SmokeShared {
    pub process_requests: AtomicUsize,
}
impl SharedHandler<'_> for SmokeShared {
    fn request_restart(&self) {}
    fn request_process(&self) {
        self.process_requests.fetch_add(1, Ordering::Relaxed);
    }
    fn request_callback(&self) {}
}
impl HostParamsImplShared for SmokeShared {
    fn request_flush(&self) {}
}
#[derive(Default)]
pub struct SmokeMain {
    pub value_rescans: Cell<usize>,
}
impl MainThreadHandler<'_> for SmokeMain {}
impl HostParamsImplMainThread for SmokeMain {
    fn rescan(&self, flags: ParamRescanFlags) {
        assert_eq!(flags, ParamRescanFlags::VALUES);
        self.value_rescans.set(self.value_rescans.get() + 1);
    }
    fn clear(&self, _: ClapId, _: ParamClearFlags) {}
}

pub fn instance(
    entry: &PluginEntry,
    plugin_id: &CStr,
) -> Result<PluginInstance<SmokeHost>, Box<dyn std::error::Error>> {
    let info = HostInfo::new(
        "RAVET CLAP Smoke Host",
        "RAVET",
        "",
        env!("CARGO_PKG_VERSION"),
    )?;
    Ok(PluginInstance::new(
        |_| SmokeShared::default(),
        |_| SmokeMain::default(),
        entry,
        plugin_id,
        &info,
    )?)
}

/// Verify a built library through the host ABI, including separate audio-thread
/// execution and main-thread activation/deactivation. Returns the block count.
pub fn exercise(entry: &PluginEntry) -> Result<usize, Box<dyn std::error::Error>> {
    let factory = entry.get_plugin_factory().ok_or("missing plugin factory")?;
    let descriptors: Vec<_> = factory.plugin_descriptors().collect();
    assert_eq!(descriptors.len(), 1);
    assert_eq!(descriptors[0].id(), Some(c"org.ravet.gain"));
    let mut blocks = 0;
    for count in [1_usize, 2] {
        let mut instance = instance(entry, c"org.ravet.gain")?;
        let handle = instance.plugin_handle();
        let ports = handle
            .get_extension::<PluginAudioPorts>()
            .ok_or("missing audio ports")?;
        let configs = handle
            .get_extension::<PluginAudioPortsConfig>()
            .ok_or("missing audio configurations")?;
        let params = handle
            .get_extension::<PluginParams>()
            .ok_or("missing params")?;
        let state = handle
            .get_extension::<PluginState>()
            .ok_or("missing state")?;
        assert_eq!(configs.count(&handle), 2);
        configs.select(&handle, ClapId::new(count as u32))?;
        for is_input in [true, false] {
            assert_eq!(ports.count(&handle, is_input), 1);
            let mut info = AudioPortInfoBuffer::new();
            let port = ports
                .get(&handle, 0, is_input, &mut info)
                .ok_or("missing main port")?;
            assert_eq!(port.channel_count, count as u32);
            assert_eq!(port.in_place_pair, Some(ClapId::new(0)));
        }
        assert_eq!(params.count(&handle), 2);
        let mut param_info = ParamInfoBuffer::new();
        let param = params
            .get_info(&handle, 0, &mut param_info)
            .ok_or("missing gain parameter")?;
        assert_eq!(param.id, GAIN_ID);
        assert_eq!(
            (param.min_value, param.max_value, param.default_value),
            (0.0, 4.0, 1.0)
        );
        assert!(param.flags.contains(ParamInfoFlags::IS_AUTOMATABLE));
        let mut text = [0_u8; 64];
        assert_eq!(
            params.value_to_text(&handle, GAIN_ID, 0.5, &mut text)?,
            b"0.500 x"
        );
        assert_eq!(
            params.text_to_value(&handle, GAIN_ID, c"0.500 x"),
            Some(0.5)
        );

        let mut bypass_info = ParamInfoBuffer::new();
        let bypass = params
            .get_info(&handle, 1, &mut bypass_info)
            .ok_or("missing bypass parameter")?;
        assert_eq!(bypass.id, BYPASS_ID);
        assert_eq!(
            (bypass.min_value, bypass.max_value, bypass.default_value),
            (0.0, 1.0, 0.0)
        );
        assert!(bypass.flags.contains(
            ParamInfoFlags::IS_AUTOMATABLE | ParamInfoFlags::IS_STEPPED | ParamInfoFlags::IS_BYPASS
        ));
        assert_eq!(
            params.value_to_text(&handle, BYPASS_ID, 1.0, &mut text)?,
            b"On"
        );
        assert_eq!(params.text_to_value(&handle, BYPASS_ID, c"Off"), Some(0.0));
        assert_eq!(params.text_to_value(&handle, BYPASS_ID, c"invalid"), None);

        for _cycle in 0..3 {
            let initial = [ParamValueEvent::new(99, GAIN_ID, Pckn::match_all(), 0.5)];
            params.flush(
                &mut instance
                    .inactive_plugin_handle()
                    .ok_or("instance still active")?,
                &InputEvents::from_buffer(&initial),
                &mut OutputEvents::void(),
            );
            assert_eq!(
                params.get_value(&instance.plugin_handle(), GAIN_ID),
                Some(0.5)
            );
            let mut saved = Vec::new();
            state.save(&instance.plugin_handle(), &mut saved)?;
            let stopped = instance.activate(
                |_, _| (),
                PluginAudioConfiguration {
                    sample_rate: 1000.0,
                    min_frames_count: 1,
                    max_frames_count: 64,
                },
            )?;
            let (ready, waiting) = std::sync::mpsc::sync_channel(0);
            let (resume, restored) = std::sync::mpsc::sync_channel(0);
            let stopped = std::thread::scope(|scope| {
                let worker = scope.spawn(move || {
                    let mut started = stopped.start_processing().unwrap();
                    let mut input_samples = [[1.0_f32; 16], [-0.5; 16]];
                    let mut output_samples = [[f32::NAN; 16]; 2];
                    let mut input_ports = AudioPorts::with_capacity(count, 1);
                    let mut output_ports = AudioPorts::with_capacity(count, 1);
                    let events = [
                        ParamValueEvent::new(0, GAIN_ID, Pckn::match_all(), 0.0),
                        ParamValueEvent::new(5, GAIN_ID, Pckn::match_all(), 2.0),
                    ];
                    {
                        let input = input_ports.with_input_buffers([AudioPortBuffer {
                            latency: 0,
                            channels: AudioPortBufferType::f32_input_only(
                                input_samples[..count]
                                    .iter_mut()
                                    .map(InputChannel::variable),
                            ),
                        }]);
                        let mut output = output_ports.with_output_buffers([AudioPortBuffer {
                            latency: 0,
                            channels: AudioPortBufferType::f32_output_only(
                                output_samples[..count].iter_mut().map(|c| c.as_mut_slice()),
                            ),
                        }]);
                        assert_eq!(
                            started
                                .process(
                                    &input,
                                    &mut output,
                                    &InputEvents::from_buffer(&events),
                                    &mut OutputEvents::void(),
                                    None,
                                    None
                                )
                                .unwrap(),
                            ProcessStatus::Continue
                        );
                    }
                    let expected = [
                        0.4_f32, 0.3, 0.2, 0.1, 0.0, 0.4, 0.8, 1.2, 1.6, 2.0, 2.0, 2.0, 2.0, 2.0,
                        2.0, 2.0,
                    ];
                    for (channel, samples) in output_samples.iter().take(count).enumerate() {
                        for (actual, gain) in samples.iter().zip(expected) {
                            let expected = gain * if channel == 0 { 1.0 } else { -0.5 };
                            assert!(
                                (actual - expected).abs() < 1e-6,
                                "gain automation fixture: {actual} != {expected}"
                            );
                        }
                    }
                    // Active flush has no meaningful timestamp. Reset preserves
                    // the requested value and snaps the five-sample ramp.
                    let change = [ParamValueEvent::new(123, GAIN_ID, Pckn::match_all(), 0.25)];
                    params.flush_active(
                        &mut started.plugin_handle(),
                        &InputEvents::from_buffer(&change),
                        &mut OutputEvents::void(),
                    );
                    started.reset();
                    {
                        let input = input_ports.with_input_buffers([AudioPortBuffer {
                            latency: 0,
                            channels: AudioPortBufferType::f32_input_only(
                                input_samples[..count]
                                    .iter_mut()
                                    .map(InputChannel::variable),
                            ),
                        }]);
                        let mut output = output_ports.with_output_buffers([AudioPortBuffer {
                            latency: 0,
                            channels: AudioPortBufferType::f32_output_only(
                                output_samples[..count].iter_mut().map(|c| c.as_mut_slice()),
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
                    assert_eq!(output_samples[0], [0.25; 16]);
                    if count == 2 {
                        assert_eq!(output_samples[1], [-0.125; 16]);
                    }
                    ready.send(()).unwrap();
                    restored.recv().unwrap();
                    {
                        let input = input_ports.with_input_buffers([AudioPortBuffer {
                            latency: 0,
                            channels: AudioPortBufferType::f32_input_only(
                                input_samples[..count]
                                    .iter_mut()
                                    .map(InputChannel::variable),
                            ),
                        }]);
                        let mut output = output_ports.with_output_buffers([AudioPortBuffer {
                            latency: 0,
                            channels: AudioPortBufferType::f32_output_only(
                                output_samples[..count].iter_mut().map(|c| c.as_mut_slice()),
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
                    assert_eq!(output_samples[0], [0.5; 16]);
                    if count == 2 {
                        assert_eq!(output_samples[1], [-0.25; 16]);
                    }
                    started.stop_processing()
                });
                waiting.recv().unwrap();
                let mut changed = Vec::new();
                state.save(&instance.plugin_handle(), &mut changed).unwrap();
                assert_ne!(saved, changed);
                state
                    .load(&instance.plugin_handle(), &mut saved.as_slice())
                    .unwrap();
                resume.send(()).unwrap();
                worker.join().unwrap()
            });
            instance.deactivate(stopped);
            assert_eq!(
                params.get_value(&instance.plugin_handle(), GAIN_ID),
                Some(0.5)
            );
            let mut round_trip = Vec::new();
            state.save(&instance.plugin_handle(), &mut round_trip)?;
            assert_eq!(saved, round_trip);
            blocks += 3;
        }
    }
    Ok(blocks)
}
