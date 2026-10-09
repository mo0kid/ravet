#[path = "../examples/support/parameter_fixture.rs"]
mod fixture;
#[path = "../examples/support/modulation_probe.rs"]
mod probe;
use clack_host::prelude::*;
use clack_plugin::entry::SinglePluginEntry;
use ravet_foundation::{AudioLayout, ProcessConfig, Tail};
use ravet_host::{HostRequests, HostedGraphNodeBuilder, HostedGraphNodeOptions, RequestHost};
fn instance(mode: u8) -> PluginInstance<RequestHost> {
    fixture::set_output_mode(mode);
    let entry = PluginEntry::load_from_clack::<SinglePluginEntry<fixture::Fixture>>(c"").unwrap();
    PluginInstance::new(
        |_| HostRequests::new(),
        |_| (),
        &entry,
        c"org.ravet.host-parameter-fixture",
        &HostInfo::new("Synthetic modulation host", "RAVET", "", "0.1").unwrap(),
    )
    .unwrap()
}
#[test]
fn native_f32_modulation_metadata_filter_flush_faults_and_base_value_preservation() {
    for (mode, input, output, capacity) in cases() {
        probe::run32(instance(mode), mode, input, output, capacity);
    }
}
#[test]
fn native_f64_modulation_metadata_filter_flush_faults_and_base_value_preservation() {
    for (mode, input, output, capacity) in cases() {
        probe::run64(instance(mode), mode, input, output, capacity);
    }
}
fn cases() -> [(u8, bool, bool, usize); 12] {
    [
        (50, true, true, 2),
        (50, true, false, 2),
        (50, false, true, 2),
        (51, true, true, 2),
        (52, true, true, 2),
        (53, true, true, 2),
        (54, true, true, 2),
        (55, true, true, 2),
        (56, true, true, 2),
        (57, true, true, 2),
        (57, true, true, 3),
        (58, true, true, 2),
    ]
}
#[test]
fn modulation_selection_without_direction_mapping_retains_inactive_instance() {
    let options = HostedGraphNodeOptions {
        configuration: ProcessConfig::new(48000., 8, 0, AudioLayout::STEREO).unwrap(),
        declared_latency: 0,
        declared_tail: Tail::None,
    };
    for input in [false, true] {
        let builder = HostedGraphNodeBuilder::new(options);
        let builder = if input {
            builder.with_parameter_modulation_input(true)
        } else {
            builder.with_parameter_modulation_output(true)
        };
        let error = builder.prepare(instance(50), |_, _| ()).err().unwrap();
        assert!(matches!(
            error.reason,
            ravet_host::HostedGraphNodeBuildFailure::Configuration
        ));
        assert!(!error.instance.is_active());
    }
}

#[test]
fn selected_output_without_native_modulation_flag_fails_after_base_mutation() {
    use ravet_foundation::{
        AudioBuffer, Event, EventData, OutputEvents, ParameterId, ProcessBlock, ProcessError,
        ProcessStatus, Processor, Transport,
    };
    use ravet_host::HostedGraphParameterMapping;
    let configuration = ProcessConfig::new(48000., 8, 1, AudioLayout::STEREO)
        .unwrap()
        .with_output_events(2);
    let options = HostedGraphNodeOptions {
        configuration,
        declared_latency: 0,
        declared_tail: Tail::None,
    };
    let mapping = |native, graph| HostedGraphParameterMapping {
        native_id: ClapId::new(native),
        graph_id: ParameterId(graph),
    };
    let (mut control, mut node) = HostedGraphNodeBuilder::new(options)
        .with_parameter_input(vec![mapping(1, 9001)])
        .with_parameter_output(vec![mapping(1, 9101), mapping(2, 9102)])
        .with_parameter_modulation_output(true)
        .prepare(instance(54), |_, _| ())
        .unwrap();
    node.prepare(configuration).unwrap();
    let mut node = std::thread::spawn(move || {
        node.start_audio().unwrap();
        let input = [0.25; 8];
        let inputs = [input.as_slice(); 2];
        let mut output = [[99.; 8]; 2];
        let mut outputs = output.each_mut().map(|samples| samples.as_mut_slice());
        let changes = [Event {
            frame_offset: 0,
            data: EventData::ParameterValue {
                id: ParameterId(9001),
                value: 2.,
            },
        }];
        let mut storage = [changes[0]; 2];
        let audio = AudioBuffer::separate(&inputs, &mut outputs, 8).unwrap();
        let mut block = ProcessBlock::new(audio, &changes, Transport::default());
        block.output_events = Some(OutputEvents::new(&mut storage, 8));
        let allocations = allocation_counter::measure(|| {
            assert_eq!(
                node.process(&mut block),
                ProcessStatus::Fault(ProcessError::BackendFailure)
            );
        });
        assert_eq!(allocations.count_total, 0);
        assert!(block.output_events.as_ref().unwrap().is_empty());
        assert_eq!(output, [[0.; 8]; 2]);
        node.stop_audio();
        node
    })
    .join()
    .unwrap();
    node.deactivate();
    assert!(control.reclaim());
    drop(node);
    let mut instance = control.into_instance().ok().unwrap();
    assert_eq!(
        ravet_host::HostedParameters::new(&mut instance, 2)
            .unwrap()
            .value(ClapId::new(1))
            .unwrap(),
        2.
    );
}
