use clack_extensions::{audio_ports::*, tail::*};
use clack_host::prelude::*;
/// Trusted sidechain reference ABI exercise for linked and built libraries.
pub fn exercise(entry: &PluginEntry) -> Result<usize, Box<dyn std::error::Error>> {
    let factory = entry.get_plugin_factory().ok_or("missing factory")?;
    let descriptors: Vec<_> = factory.plugin_descriptors().collect();
    assert_eq!(descriptors.len(), 1);
    assert_eq!(descriptors[0].id(), Some(c"org.ravet.sidechain"));
    let mut blocks = 0;
    for sample_rate in [44100., 48000., 96000.] {
        let mut instance = ravet_clap_smoke::instance(entry, c"org.ravet.sidechain")?;
        let handle = instance.plugin_handle();
        let ports = handle
            .get_extension::<PluginAudioPorts>()
            .ok_or("missing ports")?;
        assert_eq!(ports.count(&handle, true), 2);
        assert_eq!(ports.count(&handle, false), 1);
        let mut info = AudioPortInfoBuffer::new();
        for (input, index, count) in [(true, 0, 2), (true, 1, 1), (false, 0, 2)] {
            let port = ports.get(&handle, index, input, &mut info).unwrap();
            assert_eq!(port.channel_count, count);
            assert_eq!(port.flags.contains(AudioPortFlags::IS_MAIN), index == 0);
            assert!(port.flags.contains(
                AudioPortFlags::SUPPORTS_64BITS | AudioPortFlags::REQUIRES_COMMON_SAMPLE_SIZE
            ));
            assert_eq!(
                port.in_place_pair,
                if index == 0 {
                    Some(ClapId::new(0))
                } else {
                    None
                }
            );
        }
        assert!(ports.get(&handle, 2, true, &mut info).is_none());
        let tail = handle.get_extension::<PluginTail>().ok_or("missing tail")?;
        let stopped = instance.activate(
            |_, _| (),
            PluginAudioConfiguration {
                sample_rate,
                min_frames_count: 1,
                max_frames_count: 32,
            },
        )?;
        let (stopped, count) = std::thread::scope(|scope| {
            scope
                .spawn(move || {
                    let mut started = stopped.start_processing().unwrap();
                    assert_eq!(tail.get(&started.plugin_handle()), TailLength::Finite(0));
                    let mut processed = 0;
                    macro_rules! precision {
                        ($ty:ty,$input:ident,$output:ident,$value:expr) => {{
                            let mut main = [[$value; 32], [-$value; 32]];
                            let mut detector = [0 as $ty; 32];
                            detector[1] = 0.2;
                            detector[2] = -0.3;
                            detector[3] = <$ty>::NAN;
                            detector[4] = <$ty>::INFINITY;
                            let mut rendered = [[99 as $ty; 32]; 2];
                            let mut in_ports = AudioPorts::with_capacity(3, 2);
                            let mut out_ports = AudioPorts::with_capacity(2, 1);
                            {
                                let input = in_ports.with_input_buffers([
                                    AudioPortBuffer {
                                        latency: 0,
                                        channels: AudioPortBufferType::$input(
                                            main.iter_mut()
                                                .map(InputChannel::variable)
                                                .collect::<Vec<_>>()
                                                .into_iter(),
                                        ),
                                    },
                                    AudioPortBuffer {
                                        latency: 0,
                                        channels: AudioPortBufferType::$input(
                                            vec![InputChannel::variable(&mut detector)].into_iter(),
                                        ),
                                    },
                                ]);
                                let mut output = out_ports.with_output_buffers([AudioPortBuffer {
                                    latency: 0,
                                    channels: AudioPortBufferType::$output(
                                        rendered.iter_mut().map(|c| c.as_mut_slice()),
                                    ),
                                }]);
                                let heap = allocation_counter::measure(|| {
                                    for _ in 0..1000 {
                                        started.reset();
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
                                });
                                assert_eq!(heap.count_total, 0);
                                assert_eq!(heap.count_current, 0);
                                processed += 1000;
                            }
                            assert_eq!(rendered[0][1], $value);
                            assert_eq!(rendered[1][2], -$value);
                            for channel in &rendered {
                                assert_eq!(channel[0], 0.);
                                assert!(channel[3..].iter().all(|x| *x == 0.));
                            }
                            // A missing sidechain rejects the entire callback and clears output.
                            {
                                let input = in_ports.with_input_buffers([AudioPortBuffer {
                                    latency: 0,
                                    channels: AudioPortBufferType::$input(
                                        main.iter_mut().map(InputChannel::variable),
                                    ),
                                }]);
                                let mut output = out_ports.with_output_buffers([AudioPortBuffer {
                                    latency: 0,
                                    channels: AudioPortBufferType::$output(
                                        rendered.iter_mut().map(|c| c.as_mut_slice()),
                                    ),
                                }]);
                                assert!(
                                    started
                                        .process(
                                            &input,
                                            &mut output,
                                            &InputEvents::empty(),
                                            &mut OutputEvents::void(),
                                            None,
                                            None
                                        )
                                        .is_ok()
                                );
                                processed += 1;
                            }
                            assert_eq!(rendered, [[0 as $ty; 32]; 2]);
                        }};
                    }
                    precision!(f32, f32_input_only, f32_output_only, 0.75f32);
                    precision!(f64, f64_input_only, f64_output_only, 1. + 2f64.powi(-40));
                    (started.stop_processing(), processed)
                })
                .join()
                .unwrap()
        });
        blocks += count;
        instance.deactivate(stopped);
    }
    Ok(blocks)
}
