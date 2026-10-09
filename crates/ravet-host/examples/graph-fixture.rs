//! Public memoryless stereo x2 fixture for native f32/f64 graph validation.
//! Optional known synthetic transport controls playing/stopped output for probes.
//! No parameters, state, GUI, input/output events or product assets.
use clack_extensions::audio_ports::*;
use clack_plugin::{
    prelude::*,
    process::audio::{ChannelPair, SampleType},
};
struct Fixture;
struct Main;
struct Audio;
impl Plugin for Fixture {
    type Shared<'a> = ();
    type MainThread<'a> = Main;
    type AudioProcessor<'a> = Audio;
    fn declare_extensions(builder: &mut PluginExtensions<Self>, _: Option<&()>) {
        builder.register::<PluginAudioPorts>();
    }
}
impl DefaultPluginFactory for Fixture {
    fn get_descriptor() -> PluginDescriptor {
        PluginDescriptor::new(
            "org.ravet.host-graph-fixture",
            "RAVET native graph validation fixture",
        )
    }
    fn new_shared(_: HostSharedHandle<'_>) -> Result<(), PluginError> {
        Ok(())
    }
    fn new_main_thread(_: HostMainThreadHandle<'_>, _: &()) -> Result<Main, PluginError> {
        Ok(Main)
    }
}
impl PluginMainThread<'_, ()> for Main {}
impl PluginAudioPortsImpl for Main {
    fn count(&self, _: bool) -> u32 {
        1
    }
    fn get(&self, index: u32, _: bool, writer: &mut AudioPortInfoWriter) {
        if index == 0 {
            writer.set(&AudioPortInfo {
                id: ClapId::new(0),
                name: b"Synthetic stereo",
                flags: AudioPortFlags::IS_MAIN
                    | AudioPortFlags::SUPPORTS_64BITS
                    | AudioPortFlags::REQUIRES_COMMON_SAMPLE_SIZE,
                channel_count: 2,
                port_type: Some(AudioPortType::STEREO),
                in_place_pair: Some(ClapId::new(0)),
            });
        }
    }
}
impl<'a> clack_plugin::plugin::PluginAudioProcessor<'a, (), Main> for Audio {
    fn activate(
        _: HostAudioProcessorHandle<'a>,
        _: &Main,
        _: &'a (),
        _: PluginAudioConfiguration,
    ) -> Result<Self, PluginError> {
        Ok(Self)
    }
    fn process(
        &mut self,
        process: Process,
        mut audio: clack_plugin::process::Audio,
        _: clack_plugin::process::Events,
    ) -> Result<ProcessStatus, PluginError> {
        let multiplier = if let Some(transport) = process.transport {
            use clack_plugin::events::event_types::TransportFlags;
            let expected = TransportFlags::HAS_TEMPO
                | TransportFlags::HAS_SECONDS_TIMELINE
                | TransportFlags::HAS_BEATS_TIMELINE;
            if transport.flags & !TransportFlags::IS_PLAYING != expected
                || transport.song_pos_seconds.to_float() != -1.
                || transport.song_pos_beats.to_float() != -0.25
                || transport.tempo != 123.5
                || transport.tempo_inc != 0.
                || transport.header.time() != 0
            {
                return Err(PluginError::Message(
                    "unexpected synthetic transport profile",
                ));
            }
            if transport.flags.contains(TransportFlags::IS_PLAYING) {
                2.
            } else {
                0.
            }
        } else {
            2.
        };
        macro_rules! multiply {
            ($channels:expr, $sample:ty) => {
                for pair in $channels.iter_mut() {
                    match pair {
                        ChannelPair::InputOutput(input, output) => {
                            for (output, input) in output.iter_mut().zip(input) {
                                *output = *input * (multiplier as $sample);
                            }
                        }
                        ChannelPair::InPlace(samples) => {
                            for sample in samples {
                                *sample *= multiplier as $sample;
                            }
                        }
                        ChannelPair::OutputOnly(samples) => samples.fill(0.),
                        ChannelPair::InputOnly(_) => (),
                    }
                }
            };
        }
        match audio
            .port_pair(0)
            .ok_or(PluginError::Message("missing fixture port"))?
            .channels()?
        {
            SampleType::F32(mut channels) => multiply!(channels, f32),
            SampleType::F64(mut channels) => multiply!(channels, f64),
            SampleType::Both(_, _) => return Err(PluginError::Message("mixed fixture precision")),
        }
        Ok(ProcessStatus::Continue)
    }
}
#[allow(unsafe_code)]
mod export {
    clack_plugin::clack_export_entry!(clack_plugin::entry::SinglePluginEntry<super::Fixture>);
}
