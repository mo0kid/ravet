//! Stateless stereo gate reference with a mono input-only auxiliary sidechain.
//! Fixed detector threshold 0.1; no parameters, persistence or custom editor.
//! Native f32/f64 buses explicitly share one precision per callback. This tests
//! bus mapping rather than claiming a production gate/dynamics algorithm.
use clack_extensions::{audio_ports::*, tail::*};
use clack_plugin::{prelude::*, process::audio::SampleType};
use ravet_clap::MultiBusBridge;
use ravet_foundation::{AudioLayout, ProcessBlock, ProcessConfig, Processor, Sample};
pub const PLUGIN_ID: &str = "org.ravet.sidechain";
/// Sample-specific comparison without converting audio buffers.
pub trait GateSample: Sample {
    fn open(self) -> bool;
}
impl GateSample for f32 {
    fn open(self) -> bool {
        self.is_finite() && self.abs() >= 0.1
    }
}
impl GateSample for f64 {
    fn open(self) -> bool {
        self.is_finite() && self.abs() >= 0.1
    }
}
/// Prepared fixed-threshold stereo reference, driven by auxiliary bus 0.
pub struct SidechainGate {
    config: Option<ProcessConfig>,
}
impl SidechainGate {
    pub fn new() -> Self {
        Self { config: None }
    }
}
impl Default for SidechainGate {
    fn default() -> Self {
        Self::new()
    }
}
impl<S: GateSample> Processor<S> for SidechainGate {
    fn prepare(&mut self, config: ProcessConfig) -> Result<(), ravet_foundation::PrepareError> {
        if config.layout() != AudioLayout::STEREO
            || config.auxiliary_buses() != [AudioLayout::new(1, 0)?]
            || config.maximum_output_events() != 0
        {
            return Err(ravet_foundation::PrepareError::UnsupportedLayout);
        }
        self.config = Some(config);
        Ok(())
    }
    fn reset(&mut self) {}
    fn deactivate(&mut self) {
        self.config = None;
    }
    fn process(
        &mut self,
        block: &mut ProcessBlock<'_, '_, '_, S>,
    ) -> ravet_foundation::ProcessStatus {
        let Some(config) = self.config else {
            block.clear_outputs();
            return ravet_foundation::ProcessStatus::Fault(
                ravet_foundation::ProcessError::NotPrepared,
            );
        };
        if let Err(error) = config.validate_block(block) {
            block.clear_outputs();
            return ravet_foundation::ProcessStatus::Fault(error);
        }
        let sidechain = block.auxiliary.as_deref().unwrap()[0].input(0).unwrap();
        for (frame, detector) in sidechain.iter().enumerate() {
            let open = detector.open();
            for channel in 0..2 {
                let value = if open {
                    block.audio.input(channel).unwrap()[frame]
                } else {
                    S::default()
                };
                block.audio.output_mut(channel).unwrap()[frame] = value;
            }
        }
        ravet_foundation::ProcessStatus::Continue
    }
}
/// Coalesced diagnostic for the last callback, without audio-thread logging.
#[derive(Default)]
pub struct SidechainShared {
    fault: std::sync::atomic::AtomicBool,
}
impl SidechainShared {
    pub fn faulted(&self) -> bool {
        self.fault.load(std::sync::atomic::Ordering::Relaxed)
    }
}
impl PluginShared<'_> for SidechainShared {}
pub struct SidechainPlugin;
impl Plugin for SidechainPlugin {
    type AudioProcessor<'a> = SidechainAudio<'a>;
    type Shared<'a> = SidechainShared;
    type MainThread<'a> = SidechainMain;
    fn declare_extensions(builder: &mut PluginExtensions<Self>, _: Option<&SidechainShared>) {
        builder
            .register::<PluginAudioPorts>()
            .register::<PluginTail>();
    }
}
impl DefaultPluginFactory for SidechainPlugin {
    fn get_descriptor() -> PluginDescriptor {
        use clack_plugin::plugin::features::{AUDIO_EFFECT, GATE, STEREO};
        PluginDescriptor::new(PLUGIN_ID, "RAVET Sidechain")
            .with_vendor("RAVET")
            .with_version(env!("CARGO_PKG_VERSION"))
            .with_description("Fixed-threshold stereo sidechain bus reference")
            .with_features([AUDIO_EFFECT, GATE, STEREO])
    }
    fn new_shared(_: HostSharedHandle<'_>) -> Result<SidechainShared, PluginError> {
        Ok(SidechainShared::default())
    }
    fn new_main_thread<'a>(
        _: HostMainThreadHandle<'a>,
        _: &'a SidechainShared,
    ) -> Result<SidechainMain, PluginError> {
        Ok(SidechainMain)
    }
}
pub struct SidechainMain;
impl PluginMainThread<'_, SidechainShared> for SidechainMain {}
impl PluginAudioPortsImpl for SidechainMain {
    fn count(&self, input: bool) -> u32 {
        if input { 2 } else { 1 }
    }
    fn get(&self, index: u32, input: bool, writer: &mut AudioPortInfoWriter) {
        if index >= self.count(input) {
            return;
        }
        let main = index == 0;
        writer.set(&AudioPortInfo {
            id: ClapId::new(index),
            name: if main {
                b"Stereo main"
            } else {
                b"Mono sidechain"
            },
            channel_count: if main { 2 } else { 1 },
            flags: AudioPortFlags::SUPPORTS_64BITS
                | AudioPortFlags::REQUIRES_COMMON_SAMPLE_SIZE
                | if main {
                    AudioPortFlags::IS_MAIN
                } else {
                    AudioPortFlags::empty()
                },
            port_type: Some(if main {
                AudioPortType::STEREO
            } else {
                AudioPortType::MONO
            }),
            in_place_pair: if main { Some(ClapId::new(0)) } else { None },
        });
    }
}
pub struct SidechainAudio<'a> {
    shared: &'a SidechainShared,
    single: MultiBusBridge<SidechainGate, f32>,
    double: MultiBusBridge<SidechainGate, f64>,
}
impl<'a> PluginAudioProcessor<'a, SidechainShared, SidechainMain> for SidechainAudio<'a> {
    fn activate(
        _: HostAudioProcessorHandle<'a>,
        _: &SidechainMain,
        shared: &'a SidechainShared,
        native: PluginAudioConfiguration,
    ) -> Result<Self, PluginError> {
        let config = ProcessConfig::new(
            native.sample_rate,
            native.max_frames_count as usize,
            1024,
            AudioLayout::STEREO,
        )?
        .with_auxiliary_buses(&[AudioLayout::new(1, 0)?])?;
        let single = MultiBusBridge::prepare(SidechainGate::new(), config, None, None)
            .map_err(|_| PluginError::Message("invalid sidechain activation"))?;
        let double = MultiBusBridge::prepare(SidechainGate::new(), config, None, None)
            .map_err(|_| PluginError::Message("invalid sidechain activation"))?;
        Ok(Self {
            shared,
            single,
            double,
        })
    }
    fn process(
        &mut self,
        process: Process,
        mut audio: Audio,
        events: Events,
    ) -> Result<ProcessStatus, PluginError> {
        let double = audio
            .input_port(0)
            .and_then(|port| port.channels().ok())
            .is_some_and(|channels| matches!(channels, SampleType::F64(_)));
        let result = if double {
            self.double
                .process(&mut audio, events.input, process.transport, events.output)
        } else {
            self.single
                .process(&mut audio, events.input, process.transport, events.output)
        };
        self.shared
            .fault
            .store(result.is_err(), std::sync::atomic::Ordering::Relaxed);
        Ok(ProcessStatus::Continue)
    }
    fn reset(&mut self) {
        self.single.reset();
        self.double.reset();
    }
    fn deactivate(mut self, _: &SidechainMain) {
        self.single.deactivate();
        self.double.deactivate();
    }
}
impl PluginTailImpl for SidechainAudio<'_> {
    fn get(&self) -> TailLength {
        TailLength::Finite(0)
    }
}
#[allow(unsafe_code)]
mod export {
    clack_plugin::clack_export_entry!(
        clack_plugin::entry::SinglePluginEntry<super::SidechainPlugin>
    );
}
