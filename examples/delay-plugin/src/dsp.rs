//! Tempo-synced stereo delay processor, independent of any plug-in format.
//!
//! Signal path per channel: input → delay line → one-pole low-pass ("tone") →
//! feedback (straight or ping-pong) back into the delay lines. The output mixes
//! dry and wet signals, and bypass crossfades to the dry input over 5 ms.
use ravet_foundation::{
    AudioLayout, EventData, EventError, ParameterId, PrepareError, ProcessBlock, ProcessConfig,
    ProcessError, ProcessStatus, Processor, Tail,
};
use ravet_signal::{DelayLine, LinearSmoother, smoothing_samples};

/// Delay time in milliseconds, used when sync is off or the host gives no tempo.
pub const TIME: ParameterId = ParameterId(1);
/// Feedback, in percent of the delayed signal fed back.
pub const FEEDBACK: ParameterId = ParameterId(2);
/// Wet/dry mix in percent.
pub const MIX: ParameterId = ParameterId(3);
/// Dry output while the delay keeps running (0 or 1).
pub const BYPASS: ParameterId = ParameterId(4);
/// Cross-feed the repeats between channels (0 or 1).
pub const PING_PONG: ParameterId = ParameterId(5);
/// Follow the host tempo (0 or 1).
pub const SYNC: ParameterId = ParameterId(6);
/// Index into [`DIVISIONS`] used while synced.
pub const DIVISION: ParameterId = ParameterId(7);
/// Low-pass cutoff in the feedback path, in hertz.
pub const TONE: ParameterId = ParameterId(8);

/// Number of controls; IDs run from 1 to this value.
pub const CONTROLS: usize = 8;
/// Default physical values, indexed by ID - 1.
pub const DEFAULTS: [f32; CONTROLS] = [375.0, 45.0, 35.0, 0.0, 0.0, 1.0, 4.0, 6000.0];
/// Inclusive physical ranges, indexed by ID - 1. Bypass, ping-pong and sync
/// accept only 0 or 1; the division only whole indices.
pub const RANGES: [(f32, f32); CONTROLS] = [
    (MINIMUM_TIME_MS, MAXIMUM_TIME_MS),
    (0.0, 95.0),
    (0.0, 100.0),
    (0.0, 1.0),
    (0.0, 1.0),
    (0.0, 1.0),
    (0.0, (DIVISIONS.len() - 1) as f32),
    (500.0, 20_000.0),
];

/// Shortest delay time in milliseconds.
pub const MINIMUM_TIME_MS: f32 = 1.0;
/// Longest delay time in milliseconds; synced times are clamped to it.
pub const MAXIMUM_TIME_MS: f32 = 2000.0;

/// Note divisions available while synced: label and length in quarter notes.
pub const DIVISIONS: [(&str, f64); 12] = [
    ("1/1", 4.0),
    ("1/2", 2.0),
    ("1/4.", 1.5),
    ("1/4", 1.0),
    ("1/8.", 0.75),
    ("1/4T", 2.0 / 3.0),
    ("1/8", 0.5),
    ("1/16.", 0.375),
    ("1/8T", 1.0 / 3.0),
    ("1/16", 0.25),
    ("1/16T", 1.0 / 6.0),
    ("1/32", 0.125),
];

/// Glide applied when the delay time changes, giving the tape-style pitch bend.
const TIME_GLIDE_MS: f64 = 50.0;
/// Ramp applied to feedback, mix, ping-pong, tone and bypass.
const RAMP_MS: f64 = 5.0;

/// Smoothed values: time (ms), feedback, mix, ping-pong, tone and bypass.
const SMOOTHED_COUNT: usize = 6;
const TIME_SLOT: usize = 0;

/// An unknown control ID or a value outside its range.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct InvalidControl;
impl std::fmt::Display for InvalidControl {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("invalid delay control")
    }
}
impl std::error::Error for InvalidControl {}

fn valid(index: usize, value: f32) -> bool {
    let (minimum, maximum) = RANGES[index];
    value.is_finite()
        && (minimum..=maximum).contains(&value)
        && match index + 1 {
            4..=6 => value == 0.0 || value == 1.0,
            7 => value.fract() == 0.0,
            _ => true,
        }
}

/// Delay time for a note division at a tempo, clamped to the delay's range.
pub fn synced_time_ms(division: usize, tempo: f64) -> f32 {
    let beats = DIVISIONS[division.min(DIVISIONS.len() - 1)].1;
    ((beats * 60_000.0 / tempo) as f32).clamp(MINIMUM_TIME_MS, MAXIMUM_TIME_MS)
}

/// Stereo delay with tempo sync, a tone control in the feedback path and
/// ping-pong cross-feed.
///
/// Supports mono and stereo at sample rates up to 768 kHz, any block size and any number of
/// events. Parameter changes arrive as `ParameterValue` events with the IDs in
/// this module and are applied at their sample offset; unknown IDs are ignored.
/// A block with an invalid known value faults before any value is applied.
///
/// With sync on, the time follows the selected division at the host tempo read
/// at the start of each block; the last known tempo is kept while the host gives
/// none. Without any tempo since preparation, the millisecond time applies.
/// Changes of time glide over 50 ms, other controls ramp over 5 ms.
///
/// Preparation allocates two delay lines of two seconds each; processing and
/// reset allocate nothing. Nonfinite input samples become zero.
pub struct SyncedDelay {
    config: Option<ProcessConfig>,
    lines: Option<[DelayLine; 2]>,
    values: [f32; CONTROLS],
    tempo: Option<f64>,
    smooth: [LinearSmoother; SMOOTHED_COUNT],
    tone_state: [f64; 2],
}

impl Default for SyncedDelay {
    fn default() -> Self {
        let mut delay = Self {
            config: None,
            lines: None,
            values: DEFAULTS,
            tempo: None,
            smooth: [0.0; SMOOTHED_COUNT].map(|v| LinearSmoother::new(v, 0).unwrap()),
            tone_state: [0.0; 2],
        };
        delay.smooth = delay.targets().map(|v| LinearSmoother::new(v, 0).unwrap());
        delay
    }
}

impl SyncedDelay {
    /// Requested physical controls, indexed by ID - 1.
    pub fn controls(&self) -> [f32; CONTROLS] {
        self.values
    }

    /// The last host tempo received since preparation, if any.
    pub fn tempo(&self) -> Option<f64> {
        self.tempo
    }

    /// The delay time currently targeted, in milliseconds: the synced time when
    /// sync is on and a tempo is known, otherwise the time control.
    pub fn target_time_ms(&self) -> f32 {
        match (self.values[SYNC.0 as usize - 1] == 1.0, self.tempo) {
            (true, Some(tempo)) => {
                synced_time_ms(self.values[DIVISION.0 as usize - 1] as usize, tempo)
            }
            _ => self.values[TIME.0 as usize - 1],
        }
    }

    /// Set one control while inactive or between blocks; the change ramps from
    /// the next processed sample.
    ///
    /// # Errors
    ///
    /// Returns [`InvalidControl`] for an unknown ID or an invalid value, without
    /// changing anything.
    pub fn set(&mut self, id: ParameterId, value: f32) -> Result<(), InvalidControl> {
        let index = (id.0 as usize).checked_sub(1).filter(|i| *i < CONTROLS);
        match index {
            Some(index) if valid(index, value) => {
                self.values[index] = value;
                self.retarget();
                Ok(())
            }
            _ => Err(InvalidControl),
        }
    }

    fn targets(&self) -> [f32; SMOOTHED_COUNT] {
        [
            self.target_time_ms(),
            self.values[1] / 100.0,
            self.values[2] / 100.0,
            self.values[4],
            self.values[7],
            self.values[3],
        ]
    }

    fn retarget(&mut self) {
        let targets = self.targets();
        for (smoother, target) in self.smooth.iter_mut().zip(targets) {
            // Cannot fail: controls are validated finite, and the synced time
            // is clamped to the finite time range.
            smoother
                .set_target(target)
                .expect("validated finite target");
        }
    }

    fn apply(&mut self, data: EventData) {
        if let EventData::ParameterValue { id, value } = data
            && (1..=CONTROLS as u32).contains(&id.0)
        {
            // Cannot fail: `process` checks every known value before applying any.
            self.set(id, value).expect("preflighted control");
        }
    }

    fn observe_tempo(&mut self, tempo: Option<f64>) {
        if let Some(tempo) = tempo.filter(|t| t.is_finite() && *t > 0.0)
            && self.tempo != Some(tempo)
        {
            self.tempo = Some(tempo);
            self.retarget();
        }
    }
}

impl Processor for SyncedDelay {
    fn prepare(&mut self, config: ProcessConfig) -> Result<(), PrepareError> {
        self.config = None;
        if config.sample_rate() > 768_000.0 {
            return Err(PrepareError::InvalidSampleRate);
        }
        if ![AudioLayout::MONO, AudioLayout::STEREO].contains(&config.layout())
            || !config.auxiliary_buses().is_empty()
            || config.maximum_output_events() != 0
        {
            return Err(PrepareError::UnsupportedLayout);
        }
        let rate = config.sample_rate();
        let length = (rate * f64::from(MAXIMUM_TIME_MS) / 1000.0).ceil() as usize + 2;
        self.lines = Some(std::array::from_fn(|_| DelayLine::new(length).unwrap()));
        self.tempo = None;
        let targets = self.targets();
        self.smooth = std::array::from_fn(|slot| {
            let ms = if slot == TIME_SLOT {
                TIME_GLIDE_MS
            } else {
                RAMP_MS
            };
            LinearSmoother::new(targets[slot], smoothing_samples(ms, rate).unwrap()).unwrap()
        });
        self.tone_state = [0.0; 2];
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
                && (1..=CONTROLS as u32).contains(&id.0)
                && !valid(id.0 as usize - 1, value)
            {
                block.clear_outputs();
                return ProcessStatus::Fault(ProcessError::Events(EventError::InvalidValue {
                    index,
                }));
            }
        }
        self.observe_tempo(block.transport.tempo);
        let rate = config.sample_rate();
        let frames = block.audio.frames();
        let mono = block.audio.input_channels() == 1;
        // `validate_block` matched the buffer to the prepared mono or stereo
        // layout, so the channel accessors below always succeed, and a prepared
        // configuration implies prepared delay lines.
        let mut next = 0;
        for frame in 0..frames {
            while next < block.events.len() && block.events[next].frame_offset == frame {
                self.apply(block.events[next].data);
                next += 1;
            }
            let dry: [f64; 2] = std::array::from_fn(|c| {
                let sample = block.audio.input(if mono { 0 } else { c }).unwrap()[frame];
                if sample.is_finite() {
                    f64::from(sample)
                } else {
                    0.0
                }
            });
            let [time, feedback, mix, cross, tone, bypass] =
                self.smooth.each_mut().map(|s| f64::from(s.next_value()));
            // One-pole low-pass coefficient for the feedback path.
            let pole = (-std::f64::consts::TAU * tone / rate).exp();
            let delay = time * rate / 1000.0;
            let lines = self.lines.as_mut().expect("prepared delay lines");
            let wet = lines.each_ref().map(|line| line.read(delay));
            for (state, wet) in self.tone_state.iter_mut().zip(wet) {
                *state = wet + pole * (*state - wet);
            }
            let filtered = self.tone_state;
            for c in 0..2 {
                let returned = (1.0 - cross) * filtered[c] + cross * filtered[1 - c];
                lines[c].write(dry[c] + feedback * returned);
                lines[c].advance();
            }
            let out: [f64; 2] = if mono {
                let wet = 0.5 * (wet[0] + wet[1]);
                [(1.0 - mix) * dry[0] + mix * wet; 2]
            } else {
                std::array::from_fn(|c| (1.0 - mix) * dry[c] + mix * wet[c])
            };
            for c in 0..block.audio.output_channels() {
                let sample = out[c] + bypass * (dry[c] - out[c]);
                block.audio.output_mut(c).unwrap()[frame] = sample as f32;
            }
        }
        // Zero-frame blocks still deliver their values.
        for event in &block.events[next..] {
            self.apply(event.data);
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
        self.tone_state = [0.0; 2];
    }

    fn deactivate(&mut self) {
        self.reset();
        self.config = None;
    }

    fn tail(&self) -> Tail {
        match self.config {
            Some(config) if self.values[FEEDBACK.0 as usize - 1] == 0.0 => {
                let seconds = f64::from(MAXIMUM_TIME_MS) / 1000.0;
                Tail::Samples((config.sample_rate() * seconds).ceil() as u64 + 1)
            }
            _ => Tail::Infinite,
        }
    }
}
