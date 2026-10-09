use ravet_foundation::*;

fn event(offset: usize, value: f32) -> Event {
    Event {
        frame_offset: offset,
        data: EventData::ParameterValue {
            id: ParameterId(7),
            value,
        },
    }
}

struct Sidechain {
    config: Option<ProcessConfig>,
}
impl Processor<f64> for Sidechain {
    fn prepare(&mut self, config: ProcessConfig) -> Result<(), PrepareError> {
        self.config = None;
        if config.layout() != AudioLayout::new(4, 4)?
            || config.auxiliary_buses() != [AudioLayout::new(1, 1)?]
        {
            return Err(PrepareError::UnsupportedLayout);
        }
        self.config = Some(config);
        Ok(())
    }
    fn process(&mut self, block: &mut ProcessBlock<'_, '_, '_, f64>) -> ProcessStatus {
        let result = self
            .config
            .ok_or(ProcessError::NotPrepared)
            .and_then(|config| config.validate_block(block));
        if let Err(error) = result {
            block.clear_outputs();
            return ProcessStatus::Fault(error);
        }
        let sidechain = &mut block.auxiliary.as_deref_mut().unwrap()[0];
        for frame in 0..block.audio.frames() {
            let control = sidechain.input(0).unwrap()[frame];
            sidechain.output_mut(0).unwrap()[frame] = control;
            for channel in 0..4 {
                let input = block.audio.input(channel).unwrap()[frame];
                block.audio.output_mut(channel).unwrap()[frame] = input * control;
            }
        }
        for &event in block.events {
            block.output_events.as_mut().unwrap().push(event).unwrap();
        }
        ProcessStatus::Continue
    }
    fn reset(&mut self) {}
    fn deactivate(&mut self) {
        self.config = None;
    }
    fn latency_samples(&self) -> u32 {
        12
    }
    fn tail(&self) -> Tail {
        Tail::Samples(2400)
    }
}

#[test]
fn f64_multichannel_sidechain_and_output_events_are_processed_without_conversion() {
    let config = ProcessConfig::new(48000.0, 2, 2, AudioLayout::new(4, 4).unwrap())
        .unwrap()
        .with_auxiliary_buses(&[AudioLayout::new(1, 1).unwrap()])
        .unwrap()
        .with_output_events(2);
    let mut processor = Sidechain { config: None };
    processor.prepare(config).unwrap();
    let input = [1.00000000000001, 2.0, 99.0];
    let control = [0.5, 0.25, 99.0];
    let mut main = [[-1.0; 3]; 4];
    let mut control_output = [-1.0; 3];
    let mut main_channels = main.each_mut().map(|channel| &mut channel[..]);
    let mut side_channels = [&mut control_output[..]];
    let input_channels = [&input[..]; 4];
    let control_channels = [&control[..]];
    let audio = AudioBuffer::separate(&input_channels, &mut main_channels, 2).unwrap();
    let mut auxiliary = [AudioBuffer::separate(&control_channels, &mut side_channels, 2).unwrap()];
    let events = [event(0, 0.5), event(1, 0.75)];
    let mut storage = [event(0, 0.0); 2];
    let mut block = ProcessBlock::new(audio, &events, Transport::default());
    block.auxiliary = Some(&mut auxiliary);
    block.output_events = Some(OutputEvents::new(&mut storage, 2));
    assert_eq!(processor.process(&mut block), ProcessStatus::Continue);
    assert_eq!(block.output_events.as_ref().unwrap().as_slice(), &events);
    assert_eq!(processor.latency_samples(), 12);
    assert_eq!(processor.tail(), Tail::Samples(2400));
    for output in main {
        assert_eq!(output, [input[0] * 0.5, 0.5, -1.0]);
    }
    assert_eq!(control_output, [0.5, 0.25, -1.0]);
}

#[test]
fn mismatched_auxiliary_frames_reject_before_state_or_samples_change() {
    let config = ProcessConfig::new(48000.0, 2, 0, AudioLayout::MONO)
        .unwrap()
        .with_auxiliary_buses(&[AudioLayout::new(1, 0).unwrap()])
        .unwrap();
    let mut main = [3.0_f32; 3];
    let mut channels = [&mut main[..]];
    let control = [2.0_f32; 3];
    let inputs = [&control[..]];
    let mut outputs = [];
    let mut buses = [AudioBuffer::separate(&inputs, &mut outputs, 1).unwrap()];
    let mut block = ProcessBlock::new(
        AudioBuffer::in_place(&mut channels, 2).unwrap(),
        &[],
        Transport::default(),
    );
    block.auxiliary = Some(&mut buses);
    assert_eq!(
        config.validate_block(&block),
        Err(ProcessError::LayoutMismatch)
    );
    block.clear_outputs();
    assert_eq!(main, [0.0, 0.0, 3.0]);
    assert_eq!(control, [2.0; 3]);
    assert_eq!(
        config.with_auxiliary_buses(&[AudioLayout::MONO; 9]),
        Err(PrepareError::TooManyAuxiliaryBuses)
    );
}

#[test]
fn bounded_event_output_preserves_accepted_prefix_on_each_failure() {
    let mut storage = [event(0, 0.0); 2];
    let mut writer = OutputEvents::new(&mut storage, 8);
    writer.push(event(3, 1.0)).unwrap();
    assert!(matches!(
        writer.push(event(2, 0.0)),
        Err(EventError::Unordered { .. })
    ));
    assert!(matches!(
        writer.push(event(8, 0.0)),
        Err(EventError::OffsetOutOfRange { .. })
    ));
    assert!(matches!(
        writer.push(event(3, f32::NAN)),
        Err(EventError::InvalidValue { .. })
    ));
    assert_eq!(writer.as_slice(), &[event(3, 1.0)]);
    writer.push(event(3, 2.0)).unwrap();
    assert!(matches!(
        writer.push(event(4, 3.0)),
        Err(EventError::TooMany { .. })
    ));
    assert_eq!(writer.as_slice(), &[event(3, 1.0), event(3, 2.0)]);
    writer.clear();
    assert!(writer.is_empty());
    let mut writer = OutputEvents::new(&mut storage, 0);
    assert!(writer.push(event(1, 1.0)).is_err());
    writer.push(event(0, 1.0)).unwrap();
}

#[test]
fn output_storage_must_match_prepared_capacity_frames_and_empty_state() {
    let config = ProcessConfig::new(48000.0, 8, 0, AudioLayout::MONO)
        .unwrap()
        .with_output_events(1);
    let mut main = [0.0_f32; 8];
    let mut channels = [&mut main[..]];
    let mut storage = [event(0, 0.0); 1];
    let mut block = ProcessBlock::new(
        AudioBuffer::in_place(&mut channels, 8).unwrap(),
        &[],
        Transport::default(),
    );
    assert_eq!(
        config.validate_block(&block),
        Err(ProcessError::OutputEventCapacity)
    );
    block.output_events = Some(OutputEvents::new(&mut storage, 8));
    assert!(config.validate_block(&block).is_ok());
    block
        .output_events
        .as_mut()
        .unwrap()
        .push(event(0, 1.0))
        .unwrap();
    assert_eq!(
        config.validate_block(&block),
        Err(ProcessError::OutputEventCapacity)
    );
    block.clear_outputs();
    assert!(config.validate_block(&block).is_ok());
}
