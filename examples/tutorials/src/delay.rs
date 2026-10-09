//! Prepared stereo teaching delay built from RAVET delay lines and smoothers.
use ravet_foundation::{
    AudioLayout, EventData, EventError, PrepareError, ProcessBlock, ProcessConfig, ProcessError,
    ProcessStatus, Processor, Tail,
};
use ravet_signal::{DelayLine, LinearSmoother, smoothing_samples};

/// Physical controls: time (ms), feedback, wet mix, bypass and ping-pong.
pub const DEFAULT_VALUES: [f32; 5] = [375., 0.45, 0.35, 0., 0.];
/// Inclusive physical bounds; bypass and ping-pong accept only zero or one.
pub const RANGES: [(f32, f32); 5] = [(1., 2000.), (0., 0.95), (0., 1.), (0., 1.), (0., 1.)];

/// Reject invalid controls without changing requested values or ramps.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct InvalidDelayControl;
impl std::fmt::Display for InvalidDelayControl {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("invalid delay control")
    }
}
impl std::error::Error for InvalidDelayControl {}

fn valid(index: usize, value: f32) -> bool {
    value.is_finite()
        && (RANGES[index].0..=RANGES[index].1).contains(&value)
        && (index < 3 || value == 0. || value == 1.)
}

/// Stereo f32 effect with two seconds of prepared storage per channel.
///
/// Supports mono/stereo, 8..192 kHz, up to 1024 frames and 128 input events.
/// IDs 1..5 correspond to [`DEFAULT_VALUES`]. Known parameter values are
/// preflighted before any history changes; other events are ignored. Time glides
/// over 20 ms (with a pitch change); feedback, mix and ping-pong routing ramp over
/// 5 ms. Bypass is immediate dry output while the delay continues internally.
/// Reset clears history and snaps to requested controls without allocating.
/// Preparation allocates two f64 delay lines; processing allocates nothing.
/// Nonfinite input samples become zero. There is no limiter or tempo sync.
pub struct DelayProcessor {
    config: Option<ProcessConfig>,
    lines: Option<[DelayLine; 2]>,
    values: [f32; 5],
    smooth: [LinearSmoother; 4],
}
impl Default for DelayProcessor {
    fn default() -> Self {
        Self {
            config: None,
            lines: None,
            values: DEFAULT_VALUES,
            smooth: [0, 1, 2, 4].map(|i| LinearSmoother::new(DEFAULT_VALUES[i], 0).unwrap()),
        }
    }
}
impl DelayProcessor {
    /// Set the complete physical patch transactionally, including while inactive.
    /// Prepared changes begin ramps on the next processed sample.
    pub fn set_controls(&mut self, values: [f32; 5]) -> Result<(), InvalidDelayControl> {
        if values.iter().enumerate().any(|(i, v)| !valid(i, *v)) {
            return Err(InvalidDelayControl);
        }
        self.values = values;
        for (smoother, index) in self.smooth.iter_mut().zip([0, 1, 2, 4]) {
            smoother
                .set_target(values[index])
                .expect("validated finite value");
        }
        Ok(())
    }
    /// Latest requested physical controls, independently of active ramps.
    pub fn controls(&self) -> [f32; 5] {
        self.values
    }
    fn apply(&mut self, data: EventData) {
        if let EventData::ParameterValue { id, value } = data
            && (1..=5).contains(&id.0)
        {
            let mut values = self.values;
            values[id.0 as usize - 1] = value;
            self.set_controls(values).expect("preflighted control");
        }
    }
}
impl Processor for DelayProcessor {
    fn prepare(&mut self, config: ProcessConfig) -> Result<(), PrepareError> {
        self.config = None;
        if !(8000.0..=192000.).contains(&config.sample_rate()) {
            return Err(PrepareError::InvalidSampleRate);
        }
        if config.maximum_frames() > 1024 || config.maximum_events() > 128 {
            return Err(PrepareError::InvalidMaximumFrames);
        }
        if ![AudioLayout::MONO, AudioLayout::STEREO].contains(&config.layout())
            || !config.auxiliary_buses().is_empty()
            || config.maximum_output_events() != 0
        {
            return Err(PrepareError::UnsupportedLayout);
        }
        let length = (config.sample_rate() * 2.).ceil() as usize + 1;
        self.lines = Some(std::array::from_fn(|_| DelayLine::new(length).unwrap()));
        self.smooth = [0, 1, 2, 4].map(|i| {
            LinearSmoother::new(
                self.values[i],
                smoothing_samples(if i == 0 { 20. } else { 5. }, config.sample_rate()).unwrap(),
            )
            .unwrap()
        });
        self.config = Some(config);
        Ok(())
    }
    fn process(&mut self, block: &mut ProcessBlock<'_, '_, '_>) -> ProcessStatus {
        let config = match self
            .config
            .ok_or(ProcessError::NotPrepared)
            .and_then(|c| c.validate_block(block).map(|()| c))
        {
            Ok(config) => config,
            Err(error) => {
                block.clear_outputs();
                return ProcessStatus::Fault(error);
            }
        };
        for (index, event) in block.events.iter().enumerate() {
            if let EventData::ParameterValue { id, value } = event.data
                && (1..=5).contains(&id.0)
                && !valid(id.0 as usize - 1, value)
            {
                block.clear_outputs();
                return ProcessStatus::Fault(ProcessError::Events(EventError::InvalidValue {
                    index,
                }));
            }
        }
        if block.audio.frames() == 0 {
            for event in block.events {
                self.apply(event.data);
            }
        }
        let mut next = 0;
        for frame in 0..block.audio.frames() {
            while next < block.events.len() && block.events[next].frame_offset == frame {
                self.apply(block.events[next].data);
                next += 1;
            }
            let mono = block.audio.input_channels() == 1;
            let dry: [f64; 2] = std::array::from_fn(|c| {
                let sample = block.audio.input(if mono { 0 } else { c }).unwrap()[frame];
                if sample.is_finite() {
                    f64::from(sample)
                } else {
                    0.
                }
            });
            let delay = f64::from(self.smooth[0].next_value()) * config.sample_rate() / 1000.;
            let feedback = f64::from(self.smooth[1].next_value());
            let mix = f64::from(self.smooth[2].next_value());
            let cross = f64::from(self.smooth[3].next_value());
            let lines = self.lines.as_mut().expect("prepared delay lines");
            let wet = lines.each_ref().map(|line| line.read(delay));
            for c in 0..2 {
                lines[c].write(dry[c] + feedback * ((1. - cross) * wet[c] + cross * wet[1 - c]));
                lines[c].advance();
            }
            for c in 0..block.audio.output_channels() {
                block.audio.output_mut(c).unwrap()[frame] = if self.values[3] == 1. {
                    dry[c] as f32
                } else {
                    ((1. - mix) * dry[c] + mix * wet[c]) as f32
                };
            }
        }
        ProcessStatus::Continue
    }
    fn reset(&mut self) {
        if let Some(lines) = &mut self.lines {
            for line in lines {
                line.reset();
            }
        }
        for smoother in &mut self.smooth {
            smoother.reset();
        }
    }
    fn deactivate(&mut self) {
        self.reset();
        self.config = None;
    }
    fn tail(&self) -> Tail {
        Tail::Infinite
    }
}
