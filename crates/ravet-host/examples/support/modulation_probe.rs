//! Synthetic public modulation checks on native audio owners, no device or GUI.
use clack_host::prelude::*;
use ravet_foundation::{
    AudioBuffer, AudioLayout, Event, EventData, OutputEvents, ParameterId, ProcessBlock,
    ProcessConfig, ProcessError, ProcessStatus, Processor, Tail, Transport,
};
use ravet_host::{
    HostedGraphNodeBuilder, HostedGraphNodeOptions, HostedGraphParameterMapping, RequestHost,
};
fn value(frame_offset: usize, id: u32, value: f32) -> Event {
    Event {
        frame_offset,
        data: EventData::ParameterValue {
            id: ParameterId(id),
            value,
        },
    }
}
fn modulation(frame_offset: usize, id: u32, amount: f32) -> Event {
    Event {
        frame_offset,
        data: EventData::ParameterModulation {
            id: ParameterId(id),
            amount,
        },
    }
}
fn mapping(native: u32, graph: u32) -> HostedGraphParameterMapping {
    HostedGraphParameterMapping {
        native_id: ClapId::new(native),
        graph_id: ParameterId(graph),
    }
}
macro_rules! probe {
    ($name:ident,$sample:ty,$prepare:ident,$input:expr) => {
        pub fn $name(
            instance: PluginInstance<RequestHost>,
            mode: u8,
            input_enabled: bool,
            output_enabled: bool,
            capacity: usize,
        ) {
            let configuration = ProcessConfig::new(48000., 8, 3, AudioLayout::STEREO)
                .unwrap()
                .with_output_events(capacity);
            let options = HostedGraphNodeOptions {
                configuration,
                declared_latency: 0,
                declared_tail: Tail::None,
            };
            let (mut control, mut node) = HostedGraphNodeBuilder::new(options)
                .with_parameter_input(vec![mapping(1, 9001)])
                .with_parameter_output(vec![mapping(1, 9101), mapping(2, 9102)])
                .with_parameter_modulation_input(input_enabled)
                .with_parameter_modulation_output(output_enabled)
                .$prepare(instance, |_, _| ())
                .unwrap();
            node.prepare(configuration).unwrap();
            let bad_input = !input_enabled || mode == 54;
            let bad_output = output_enabled
                && (matches!(mode, 51 | 52 | 55 | 56) || (mode == 57 && capacity == 2));
            let filtered = !output_enabled || matches!(mode, 53 | 58);
            let mut node = std::thread::spawn(move || {
                node.start_audio().unwrap();
                let input = [$input as $sample; 8];
                let inputs = [input.as_slice(); 2];
                let mut output = [[99. as $sample; 8]; 2];
                let mut storage = [value(0, 0, 0.); 3];
                let changes = [
                    value(0, 9001, 2.),
                    modulation(2, 9001, -0.5),
                    modulation(4, 9001, 0.75),
                ];
                let allocations = allocation_counter::measure(|| {
                    for iteration in 0..1000 {
                        let mut outputs = output.each_mut().map(|samples| samples.as_mut_slice());
                        let audio = AudioBuffer::separate(&inputs, &mut outputs, 8).unwrap();
                        let mut block = ProcessBlock::new(audio, &changes, Transport::default());
                        block.output_events = Some(OutputEvents::new(&mut storage[..capacity], 8));
                        let status = node.process(&mut block);
                        if bad_input || bad_output {
                            if bad_input {
                                assert!(matches!(
                                    status,
                                    ProcessStatus::Fault(ProcessError::Events(_))
                                ));
                            } else {
                                assert_eq!(
                                    status,
                                    ProcessStatus::Fault(ProcessError::BackendFailure)
                                );
                            }
                            assert!(block.output_events.as_ref().unwrap().is_empty());
                            assert_eq!(output, [[0. as $sample; 8]; 2]);
                        } else {
                            assert_eq!(status, ProcessStatus::Continue, "mode {mode}, input {input_enabled}, output {output_enabled}, capacity {capacity}");
                            let expected = if filtered {
                                &[value(7, 9102, 2.)][..]
                            } else if mode == 57 {
                                &[
                                    modulation(7, 9101, 0.75),
                                    value(7, 9102, 2.),
                                    modulation(7, 9101, 0.75),
                                ][..]
                            } else {
                                &[modulation(7, 9101, 0.75), value(7, 9102, 2.)][..]
                            };
                            assert_eq!(block.output_events.as_ref().unwrap().as_slice(), expected);
                            for samples in output {
                                for (frame, sample) in samples.into_iter().enumerate() {
                                    let gain = if frame < 2 {
                                        if iteration == 0 { 2. } else { 2.75 }
                                    } else if frame < 4 {
                                        1.5
                                    } else {
                                        2.75
                                    };
                                    assert_eq!(sample, $input as $sample * gain as $sample);
                                }
                            }
                        }
                    }

                    if mode==50 && input_enabled && output_enabled {
                        let mut outputs=output.each_mut().map(|samples|samples.as_mut_slice());
                        let audio=AudioBuffer::separate(&inputs,&mut outputs,8).unwrap();
                        let invalid=[value(0,9001,3.),modulation(1,9002,-1.)];
                        let mut block=ProcessBlock::new(audio,&invalid,Transport::default());
                        block.output_events=Some(OutputEvents::new(&mut storage[..capacity],8));
                        assert!(matches!(node.process(&mut block),ProcessStatus::Fault(ProcessError::Events(_))));
                        assert!(block.output_events.as_ref().unwrap().is_empty());
                        let mut empty=[[];2];let mut outputs=empty.each_mut().map(|samples|samples.as_mut_slice());let inputs=[&[][..];2];
                        let audio=AudioBuffer::separate(&inputs,&mut outputs,0).unwrap();
                        let mut block=ProcessBlock::new(audio,&[],Transport::default());
                        block.output_events=Some(OutputEvents::new(&mut storage[..capacity],0));
                        assert_eq!(node.process(&mut block),ProcessStatus::Continue);
                        assert_eq!(block.output_events.as_ref().unwrap().as_slice(),&[modulation(0,9101,0.75),value(0,9102,2.)]);
                    }
                    let mut empty = [[]; 2];
                    let mut outputs = empty.each_mut().map(|samples| samples.as_mut_slice());
                    let inputs = [&[][..]; 2];
                    let audio = AudioBuffer::separate(&inputs, &mut outputs, 0).unwrap();
                    let changes = [value(0, 9001, 4.), modulation(0, 9001, -1.)];
                    let mut block = ProcessBlock::new(audio, &changes, Transport::default());
                    block.output_events = Some(OutputEvents::new(&mut storage[..capacity], 0));
                    let status = node.process(&mut block);
                    if bad_input {
                        assert!(matches!(
                            status,
                            ProcessStatus::Fault(ProcessError::Events(_))
                        ));
                    } else if bad_output && mode != 55 {
                        assert_eq!(status, ProcessStatus::Fault(ProcessError::BackendFailure));
                    } else {
                        assert_eq!(status, ProcessStatus::Continue, "mode {mode}, input {input_enabled}, output {output_enabled}, capacity {capacity}");
                        let expected = if filtered {
                            &[value(0, 9102, 4.)][..]
                        } else if mode == 57 {
                            &[
                                modulation(0, 9101, -1.),
                                value(0, 9102, 4.),
                                modulation(0, 9101, -1.),
                            ][..]
                        } else {
                            &[modulation(0, 9101, -1.), value(0, 9102, 4.)][..]
                        };
                        assert_eq!(block.output_events.as_ref().unwrap().as_slice(), expected);
                    }
                });
                assert_eq!(allocations.count_total, 0);
                assert_eq!(allocations.count_current, 0);
                node.stop_audio();
                node
            })
            .join()
            .unwrap();
            node.deactivate();
            assert!(control.reclaim());
            drop(node);
            assert_eq!(
                control.statistics().parameter_flushes,
                if bad_input {0} else if mode==50 && input_enabled && output_enabled {2} else {1}
            );
            let mut instance = control.into_instance().ok().unwrap();
            assert_eq!(
                ravet_host::HostedParameters::new(&mut instance, 2)
                    .unwrap()
                    .value(ClapId::new(1))
                    .unwrap(),
                if bad_input { 1. } else { 4. }
            );
        }
    };
}
probe!(run32, f32, prepare, 0.25);
probe!(run64, f64, prepare64, 1e200);
