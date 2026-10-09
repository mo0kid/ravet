use ravet_device::PreparedOutput;
use ravet_foundation::{
    PrepareError, ProcessBlock, ProcessConfig, ProcessError, ProcessStatus, Processor,
};

struct Ramp {
    config: Option<ProcessConfig>,
    fault_at: Option<i64>,
}
impl Processor for Ramp {
    fn prepare(&mut self, config: ProcessConfig) -> Result<(), PrepareError> {
        self.config = Some(config);
        Ok(())
    }
    fn process(&mut self, block: &mut ProcessBlock<'_, '_, '_>) -> ProcessStatus {
        self.config.unwrap().validate_block(block).unwrap();
        let position = block.transport.sample_position.unwrap();
        if self.fault_at == Some(position) {
            return ProcessStatus::Fault(ProcessError::NotPrepared);
        }
        for channel in 0..block.audio.output_channels() {
            for (frame, output) in block
                .audio
                .output_mut(channel)
                .unwrap()
                .iter_mut()
                .enumerate()
            {
                *output = (position + frame as i64) as f32 + channel as f32 * 100.0;
            }
        }
        ProcessStatus::Continue
    }
    fn reset(&mut self) {}
    fn deactivate(&mut self) {
        self.config = None;
    }
}
fn ramp(fault_at: Option<i64>) -> Ramp {
    Ramp {
        config: None,
        fault_at,
    }
}

#[test]
fn variable_callbacks_are_partitioned_and_interleaved_without_losing_position() {
    let mut renderer = PreparedOutput::new(ramp(None), 48000, 2, 3).unwrap();
    let mut output = [-1.0; 16];
    renderer.render(&mut output);
    assert_eq!(
        output,
        [
            0., 100., 1., 101., 2., 102., 3., 103., 4., 104., 5., 105., 6., 106., 7., 107.
        ]
    );
    let mut next = [-1.; 2];
    renderer.render(&mut next);
    assert_eq!(next, [8., 108.]);
    let stats = renderer.statistics();
    assert_eq!(stats.callbacks, 2);
    assert_eq!(stats.frames, 9);
    assert_eq!(stats.processor_faults, 0);
}
#[test]
fn malformed_partial_frame_and_faulting_subblock_are_silenced() {
    let mut renderer = PreparedOutput::new(ramp(Some(3)), 48000, 2, 3).unwrap();
    let mut malformed = [9.; 3];
    renderer.render(&mut malformed);
    assert_eq!(malformed, [0.; 3]);
    let mut output = [-1.; 16];
    renderer.render(&mut output);
    assert_eq!(
        output,
        [
            0., 100., 1., 101., 2., 102., 0., 0., 0., 0., 0., 0., 6., 106., 7., 107.
        ]
    );
    assert_eq!(renderer.statistics().processor_faults, 2);
    assert_eq!(renderer.statistics().frames, 8);
}
#[test]
fn callback_adapter_performs_no_allocation_or_deallocation() {
    let mut renderer = PreparedOutput::new(ramp(None), 48000, 8, 64).unwrap();
    let mut output = [0.; 1032];
    let counts = allocation_counter::measure(|| {
        for _ in 0..1000 {
            renderer.render(&mut output);
        }
    });
    assert_eq!(counts.count_total, 0);
    assert_eq!(counts.count_current, 0);
    assert_eq!(counts.bytes_current, 0);
    assert_eq!(renderer.statistics().frames, 129_000);
}
#[test]
fn invalid_preparation_limits_are_rejected() {
    assert!(matches!(
        PreparedOutput::new(ramp(None), 0, 2, 64),
        Err(PrepareError::InvalidSampleRate)
    ));
    for channels in [0, 65] {
        assert!(matches!(
            PreparedOutput::new(ramp(None), 48000, channels, 64),
            Err(PrepareError::UnsupportedLayout)
        ));
    }
    for frames in [0, 65_537] {
        assert!(matches!(
            PreparedOutput::new(ramp(None), 48000, 2, frames),
            Err(PrepareError::InvalidMaximumFrames)
        ));
    }
}
