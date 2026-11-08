//! Auxiliary buses through the CLAP export, hosted in process through the CLAP ABI.
#![cfg(feature = "clap")]
use clack_extensions::audio_ports::{AudioPortFlags, AudioPortInfoBuffer, PluginAudioPorts};
use clack_host::prelude::*;
use ravet_foundation::{PrepareError, ProcessBlock, ProcessConfig, ProcessStatus, Processor};
use ravet_plugin::{
    __private::clack_plugin::entry::SinglePluginEntry, AuxiliaryBus, Category, PluginExport,
    Product, StateFormat, clap::ClapPlugin,
};

const BLOCK: usize = 4;

fn slice(channel: &mut [f32; BLOCK]) -> &mut [f32] {
    channel
}

/// Main output = main input + sidechain; the "Direct" output = main input / 2.
#[derive(Default)]
struct Sidechained {
    config: Option<ProcessConfig>,
}
impl Processor<f32> for Sidechained {
    fn prepare(&mut self, config: ProcessConfig) -> Result<(), PrepareError> {
        self.config = Some(config);
        Ok(())
    }
    fn process(&mut self, block: &mut ProcessBlock<'_, '_, '_, f32>) -> ProcessStatus {
        if let Err(error) = self.config.unwrap().validate_block(block) {
            block.clear_outputs();
            return ProcessStatus::Fault(error);
        }
        let frames = block.audio.frames();
        let auxiliary = block.auxiliary.as_deref_mut().unwrap();
        let side: Vec<f32> = auxiliary[0].input(0).unwrap().to_vec();
        for channel in 0..2 {
            let input: Vec<f32> = block.audio.input(channel).unwrap().to_vec();
            for frame in 0..frames {
                block.audio.output_mut(channel).unwrap()[frame] = input[frame] + side[frame];
                auxiliary[1].output_mut(channel).unwrap()[frame] = input[frame] / 2.0;
            }
        }
        ProcessStatus::Continue
    }
    fn reset(&mut self) {}
    fn deactivate(&mut self) {
        self.config = None;
    }
}

struct SidechainPlugin;
impl PluginExport for SidechainPlugin {
    type Processor = Sidechained;
    const PRODUCT: Product<'static> = Product::new(
        "Sidechain Test",
        "RAVET",
        "1.0.0",
        "org.ravet.sidechain-test",
        Category::Effect,
        StateFormat {
            tag: *b"RVTSDC01",
            version: 1,
        },
    )
    .with_auxiliary_buses(&[
        AuxiliaryBus::sidechain("Sidechain", 1),
        AuxiliaryBus::output("Direct", 2),
    ]);
    fn create() -> Sidechained {
        Sidechained::default()
    }
}

#[test]
fn auxiliary_buses_are_clap_audio_ports() {
    let entry = PluginEntry::load_from_clack::<SinglePluginEntry<ClapPlugin<SidechainPlugin>>>(
        c"sidechain.clap",
    )
    .unwrap();
    let mut instance = ravet_clap_smoke::instance(&entry, c"org.ravet.sidechain-test").unwrap();
    let handle = instance.plugin_handle();
    let ports = handle.get_extension::<PluginAudioPorts>().unwrap();
    let mut listed = Vec::new();
    for input in [true, false] {
        for index in 0..ports.count(&handle, input) {
            let mut buffer = AudioPortInfoBuffer::new();
            let port = ports.get(&handle, index, input, &mut buffer).unwrap();
            listed.push((
                input,
                port.id.get(),
                String::from_utf8_lossy(port.name).into_owned(),
                port.channel_count,
                port.flags.contains(AudioPortFlags::IS_MAIN),
            ));
        }
    }
    assert_eq!(
        listed,
        [
            (true, 0, "Input".to_owned(), 2, true),
            (true, 1, "Sidechain".to_owned(), 1, false),
            (false, 0, "Output".to_owned(), 2, true),
            (false, 2, "Direct".to_owned(), 2, false),
        ]
    );

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
    let mut main_in = [[1.0_f32, 2.0, 3.0, 4.0]; 2];
    let mut side_in = [[10.0_f32, 20.0, 30.0, 40.0]];
    let mut main_out = [[f32::NAN; BLOCK]; 2];
    let mut direct_out = [[f32::NAN; BLOCK]; 2];
    let mut input_ports = AudioPorts::with_capacity(3, 2);
    let mut output_ports = AudioPorts::with_capacity(4, 2);
    let inputs = input_ports.with_input_buffers([
        AudioPortBuffer {
            latency: 0,
            channels: AudioPortBufferType::f32_input_only(
                main_in.iter_mut().map(InputChannel::variable),
            ),
        },
        AudioPortBuffer {
            latency: 0,
            channels: AudioPortBufferType::f32_input_only(
                side_in.iter_mut().map(InputChannel::variable),
            ),
        },
    ]);
    let mut outputs = output_ports.with_output_buffers([
        AudioPortBuffer {
            latency: 0,
            channels: AudioPortBufferType::f32_output_only(main_out.iter_mut().map(slice)),
        },
        AudioPortBuffer {
            latency: 0,
            channels: AudioPortBufferType::f32_output_only(direct_out.iter_mut().map(slice)),
        },
    ]);
    started
        .process(
            &inputs,
            &mut outputs,
            &InputEvents::empty(),
            &mut OutputEvents::void(),
            None,
            None,
        )
        .unwrap();
    assert_eq!(main_out, [[11.0, 22.0, 33.0, 44.0]; 2]);
    assert_eq!(direct_out, [[0.5, 1.0, 1.5, 2.0]; 2]);
    let stopped = started.stop_processing();
    instance.deactivate(stopped);
}
