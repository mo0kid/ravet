use ravet_foundation::*;

fn config(layout: AudioLayout) -> ProcessConfig {
    ProcessConfig::new(48_000.0, 8, 4, layout).unwrap()
}

#[test]
fn preparation_rejects_invalid_rates_capacities_and_layouts() {
    for rate in [0.0, -1.0, f64::NAN, f64::INFINITY] {
        assert_eq!(
            ProcessConfig::new(rate, 8, 4, AudioLayout::MONO),
            Err(PrepareError::InvalidSampleRate)
        );
    }
    assert_eq!(
        ProcessConfig::new(48_000.0, 0, 4, AudioLayout::MONO),
        Err(PrepareError::InvalidMaximumFrames)
    );
    assert_eq!(
        AudioLayout::new(65, 2),
        Err(PrepareError::UnsupportedLayout)
    );
    assert_eq!(AudioLayout::new(0, 0), Err(PrepareError::UnsupportedLayout));
    assert_eq!(
        AudioLayout::new(0, 65),
        Err(PrepareError::UnsupportedLayout)
    );
    assert_eq!(AudioLayout::new(0, 2), Ok(AudioLayout::STEREO_INSTRUMENT));
    assert!(ProcessConfig::new(48_000.0, 8, 0, AudioLayout::MONO).is_ok());
}

#[test]
fn short_buffers_fail_before_processing() {
    let input = [1.0; 2];
    let inputs = [&input[..]];
    let mut output = [9.0; 3];
    let mut outputs = [&mut output[..]];
    assert_eq!(
        AudioBuffer::separate(&inputs, &mut outputs, 3).unwrap_err(),
        BufferError::InputTooShort {
            channel: 0,
            available: 2,
            required: 3
        }
    );
    assert_eq!(
        AudioBuffer::in_place(&mut outputs, 4).unwrap_err(),
        BufferError::OutputTooShort {
            channel: 0,
            available: 3,
            required: 4
        }
    );
    assert_eq!(output, [9.0; 3]);
}

#[test]
fn active_prefix_access_and_silence_preserve_backing_tails() {
    let mut left = [1.0; 4];
    let mut right = [2.0; 6];
    let mut channels = [&mut left[..], &mut right[..]];
    let mut audio = AudioBuffer::in_place(&mut channels, 2).unwrap();
    assert_eq!(audio.input_channels(), 2);
    assert_eq!(audio.output_channels(), 2);
    assert_eq!(audio.input(1), Some(&[2.0, 2.0][..]));
    assert!(audio.input(2).is_none());
    assert!(audio.output_mut(2).is_none());
    audio.clear_outputs();
    assert_eq!(left, [0.0, 0.0, 1.0, 1.0]);
    assert_eq!(right, [0.0, 0.0, 2.0, 2.0, 2.0, 2.0]);
}

#[test]
fn event_order_bounds_payloads_and_capacity_are_explicit() {
    let parameter = EventData::ParameterValue {
        id: ParameterId(7),
        value: 1.0,
    };
    let cases = [
        (
            vec![Event {
                frame_offset: 3,
                data: parameter,
            }],
            ProcessError::Events(EventError::OffsetOutOfRange {
                index: 0,
                offset: 3,
                frames: 3,
            }),
        ),
        (
            vec![
                Event {
                    frame_offset: 2,
                    data: parameter,
                },
                Event {
                    frame_offset: 1,
                    data: parameter,
                },
            ],
            ProcessError::Events(EventError::Unordered { index: 1 }),
        ),
        (
            vec![Event {
                frame_offset: 0,
                data: EventData::ParameterValue {
                    id: ParameterId(1),
                    value: f32::NAN,
                },
            }],
            ProcessError::Events(EventError::InvalidValue { index: 0 }),
        ),
        (
            vec![Event {
                frame_offset: 0,
                data: EventData::NoteOn {
                    channel: 16,
                    key: 60,
                    velocity: 0.5,
                },
            }],
            ProcessError::Events(EventError::InvalidValue { index: 0 }),
        ),
        (
            vec![Event {
                frame_offset: 0,
                data: EventData::NoteOff {
                    channel: 0,
                    key: 128,
                    velocity: 0.5,
                },
            }],
            ProcessError::Events(EventError::InvalidValue { index: 0 }),
        ),
        (
            vec![Event {
                frame_offset: 0,
                data: EventData::NoteOn {
                    channel: 0,
                    key: 60,
                    velocity: -0.1,
                },
            }],
            ProcessError::Events(EventError::InvalidValue { index: 0 }),
        ),
        (
            vec![
                Event {
                    frame_offset: 99,
                    data: parameter
                };
                5
            ],
            ProcessError::Events(EventError::TooMany {
                count: 5,
                maximum: 4,
            }),
        ),
    ];
    for (events, error) in cases {
        let mut data = [1.0; 3];
        let mut channels = [&mut data[..]];
        let audio = AudioBuffer::in_place(&mut channels, 3).unwrap();
        let block = ProcessBlock::new(audio, &events, Transport::default());
        assert_eq!(config(AudioLayout::MONO).validate_block(&block), Err(error));
    }
    let valid = [
        Event {
            frame_offset: 0,
            data: EventData::NoteOn {
                channel: 15,
                key: 127,
                velocity: 1.0,
            },
        },
        Event {
            frame_offset: 0,
            data: parameter,
        },
        Event {
            frame_offset: 2,
            data: EventData::NoteOff {
                channel: 0,
                key: 0,
                velocity: 0.0,
            },
        },
    ];
    let mut data = [1.0; 3];
    let mut channels = [&mut data[..]];
    let audio = AudioBuffer::in_place(&mut channels, 3).unwrap();
    assert_eq!(
        config(AudioLayout::MONO).validate_block(&ProcessBlock::new(
            audio,
            &valid,
            Transport::default()
        )),
        Ok(())
    );
}

#[test]
fn zero_frames_allow_only_offset_zero() {
    for (offset, valid) in [(0, true), (1, false)] {
        let mut data = [5.0; 3];
        let mut channels = [&mut data[..]];
        let audio = AudioBuffer::in_place(&mut channels, 0).unwrap();
        let events = [Event {
            frame_offset: offset,
            data: EventData::ParameterValue {
                id: ParameterId(1),
                value: 0.5,
            },
        }];
        let mut block = ProcessBlock::new(audio, &events, Transport::default());
        assert_eq!(
            config(AudioLayout::MONO).validate_block(&block).is_ok(),
            valid
        );
        block.audio.clear_outputs();
        assert_eq!(data, [5.0; 3]);
    }
}

/// A tiny instrument proves the processor boundary requires no audio inputs
/// and can consume note events on the same sample as its first output.
struct TestInstrument {
    config: Option<ProcessConfig>,
    level: f32,
}
impl Processor for TestInstrument {
    fn prepare(&mut self, config: ProcessConfig) -> Result<(), PrepareError> {
        self.config = None;
        if config.layout() != AudioLayout::MONO_INSTRUMENT {
            return Err(PrepareError::UnsupportedLayout);
        }
        self.config = Some(config);
        Ok(())
    }
    fn process(&mut self, block: &mut ProcessBlock<'_, '_, '_>) -> ProcessStatus {
        let result = self
            .config
            .ok_or(ProcessError::NotPrepared)
            .and_then(|config| config.validate_block(block));
        if let Err(error) = result {
            block.audio.clear_outputs();
            return ProcessStatus::Fault(error);
        }
        let mut cursor = 0;
        for frame in 0..block.audio.frames().max(1) {
            while let Some(event) = block.events.get(cursor) {
                if event.frame_offset != frame {
                    break;
                }
                match event.data {
                    EventData::NoteOn { velocity, .. } => self.level = velocity,
                    EventData::NoteOff { .. } => self.level = 0.0,
                    _ => {}
                }
                cursor += 1;
            }
            if let Some(sample) = block.audio.output_mut(0).and_then(|out| out.get_mut(frame)) {
                *sample = self.level;
            }
        }
        ProcessStatus::Continue
    }
    fn reset(&mut self) {
        self.level = 0.0;
    }
    fn deactivate(&mut self) {
        self.config = None;
    }
}

#[test]
fn no_input_instrument_obeys_note_timing_and_initializes_outputs() {
    let mut instrument = TestInstrument {
        config: None,
        level: 0.0,
    };
    instrument
        .prepare(config(AudioLayout::MONO_INSTRUMENT))
        .unwrap();
    let events = [
        Event {
            frame_offset: 1,
            data: EventData::NoteOn {
                channel: 0,
                key: 60,
                velocity: 0.75,
            },
        },
        Event {
            frame_offset: 3,
            data: EventData::NoteOff {
                channel: 0,
                key: 60,
                velocity: 0.0,
            },
        },
    ];
    let mut output = [f32::NAN; 5];
    let mut channels = [&mut output[..]];
    let audio = AudioBuffer::separate(&[], &mut channels, 5).unwrap();
    let mut block = ProcessBlock::new(audio, &events, Transport::default());
    assert_eq!(instrument.process(&mut block), ProcessStatus::Continue);
    assert_eq!(output, [0.0, 0.75, 0.75, 0.0, 0.0]);
}

#[test]
fn global_modulation_is_finite_independent_of_base_and_valid_for_zero_frame_flush() {
    let config = config(AudioLayout::MONO);
    for frames in [0, 4] {
        for amount in [-1000., 0., 0.25, f32::NAN, f32::INFINITY] {
            let input = [0.; 4];
            let inputs = [input.as_slice()];
            let mut output = [0.; 4];
            let mut outputs = [output.as_mut_slice()];
            let audio = AudioBuffer::separate(&inputs, &mut outputs, frames).unwrap();
            let events = [Event {
                frame_offset: 0,
                data: EventData::ParameterModulation {
                    id: ParameterId(7),
                    amount,
                },
            }];
            let block = ProcessBlock::new(audio, &events, Transport::default());
            if amount.is_finite() {
                assert_eq!(config.validate_block(&block), Ok(()));
            } else {
                assert_eq!(
                    config.validate_block(&block),
                    Err(ProcessError::Events(EventError::InvalidValue { index: 0 }))
                );
            }
            let mut storage = [Event {
                frame_offset: 0,
                data: EventData::ParameterValue {
                    id: ParameterId(7),
                    value: 2.,
                },
            }; 2];
            let mut generated = OutputEvents::new(&mut storage, frames);
            generated.push(storage_value()).unwrap();
            assert_eq!(generated.push(events[0]).is_ok(), amount.is_finite());
            assert_eq!(generated.as_slice()[0], storage_value());
        }
    }
    fn storage_value() -> Event {
        Event {
            frame_offset: 0,
            data: EventData::ParameterValue {
                id: ParameterId(7),
                value: 2.,
            },
        }
    }
}
