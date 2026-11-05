//! A monophonic retriggered click with short-MIDI echo and optional 32-sample delay.
use ravet_foundation::{
    AudioLayout, EventData, EventError, NotePhase, ParameterId, PrepareError, ProcessBlock,
    ProcessConfig, ProcessError, ProcessStatus, Processor, Tail,
};

/// Respond to MIDI CC 1 on every channel (0/1, default 1).
pub const CC_RESPONSE: ParameterId = ParameterId(1);
/// Add 32 audio samples and report that latency (0/1, default 1).
pub const LOOKAHEAD: ParameterId = ParameterId(2);
/// Length of each one-shot, independent of rate and note-off.
pub const CLICK_SAMPLES: usize = 64;
/// Audio delay when lookahead is enabled; MIDI echo is undelayed.
pub const LATENCY: usize = 32;

/// Every positive note-on retriggers a 64-sample alternating ±velocity/4 click.
/// CC 1 scales the current click by value/127 when CC response is enabled;
/// its initial/reset value is 127. Note-off does not truncate the one-shot.
/// Short MIDI and portable note events are echoed at their input offsets.
/// Overlapping notes retrigger one voice; this is an evidence fixture, not a
/// polyphonic synthesizer. Lookahead changes clear delayed audio; hosts must
/// renegotiate PDC after a live latency change. Reset retains controls, clears
/// voice/delay and restores CC 1 to 127. Process/reset allocate nothing.
pub struct EvidenceSynth {
    config: Option<ProcessConfig>,
    cc_response: bool,
    lookahead: bool,
    cc: f32,
    remaining: usize,
    velocity: f32,
    delay: [f32; LATENCY],
    cursor: usize,
}
impl Default for EvidenceSynth {
    fn default() -> Self {
        Self {
            config: None,
            cc_response: true,
            lookahead: true,
            cc: 1.0,
            remaining: 0,
            velocity: 0.0,
            delay: [0.0; LATENCY],
            cursor: 0,
        }
    }
}
impl EvidenceSynth {
    fn apply(&mut self, data: EventData) {
        let velocity = match data {
            EventData::ParameterValue {
                id: CC_RESPONSE,
                value,
            } => {
                self.cc_response = value != 0.0;
                None
            }
            EventData::ParameterValue {
                id: LOOKAHEAD,
                value,
            } => {
                if self.lookahead != (value != 0.0) {
                    self.lookahead = value != 0.0;
                    self.delay.fill(0.0);
                    self.cursor = 0;
                }
                None
            }
            EventData::NoteOn { velocity, .. }
            | EventData::NoteWithId {
                phase: NotePhase::On,
                velocity,
                ..
            } => Some(velocity),
            EventData::Midi(message) => {
                let bytes = message.as_bytes();
                match bytes[0] & 0xf0 {
                    0x90 if bytes[2] != 0 => Some(f32::from(bytes[2]) / 127.0),
                    0xb0 if bytes[1] == 1 => {
                        self.cc = f32::from(bytes[2]) / 127.0;
                        None
                    }
                    _ => None,
                }
            }
            _ => None,
        };
        if let Some(velocity) = velocity.filter(|v| *v > 0.0) {
            self.remaining = CLICK_SAMPLES;
            self.velocity = velocity;
        }
    }
}
impl Processor for EvidenceSynth {
    fn prepare(&mut self, config: ProcessConfig) -> Result<(), PrepareError> {
        self.config = None;
        if config.layout() != AudioLayout::STEREO_INSTRUMENT || !config.auxiliary_buses().is_empty()
        {
            return Err(PrepareError::UnsupportedLayout);
        }
        self.reset();
        self.config = Some(config);
        Ok(())
    }
    fn process(&mut self, block: &mut ProcessBlock<'_, '_, '_>) -> ProcessStatus {
        let result = self
            .config
            .ok_or(ProcessError::NotPrepared)
            .and_then(|c| c.validate_block(block));
        if let Err(error) = result {
            block.clear_outputs();
            return ProcessStatus::Fault(error);
        }
        for (index, event) in block.events.iter().enumerate() {
            if let EventData::ParameterValue { id, value } = event.data
                && [CC_RESPONSE, LOOKAHEAD].contains(&id)
                && value != 0.0
                && value != 1.0
            {
                block.clear_outputs();
                return ProcessStatus::Fault(ProcessError::Events(EventError::InvalidValue {
                    index,
                }));
            }
        }
        // Echo at most one event per input. Reject insufficient capacity before
        // any DSP/control mutation; optional absent output simply disables echo.
        if let Some(output) = &mut block.output_events {
            let count = block
                .events
                .iter()
                .filter(|e| {
                    matches!(
                        e.data,
                        EventData::Midi(_)
                            | EventData::NoteOn { .. }
                            | EventData::NoteOff { .. }
                            | EventData::NoteWithId { .. }
                    )
                })
                .count();
            if output.capacity() - output.len() < count {
                block.clear_outputs();
                return ProcessStatus::Fault(ProcessError::OutputEventCapacity);
            }
            for event in block.events.iter().filter(|e| {
                matches!(
                    e.data,
                    EventData::Midi(_)
                        | EventData::NoteOn { .. }
                        | EventData::NoteOff { .. }
                        | EventData::NoteWithId { .. }
                )
            }) {
                if output.push(*event).is_err() {
                    unreachable!("preflighted output");
                }
            }
        }
        let mut next = 0;
        for frame in 0..block.audio.frames() {
            while next < block.events.len() && block.events[next].frame_offset == frame {
                self.apply(block.events[next].data);
                next += 1;
            }
            let sample = if self.remaining == 0 {
                0.0
            } else {
                self.remaining -= 1;
                let sign = if self.remaining % 2 == 1 { 1.0 } else { -1.0 };
                sign * self.velocity * 0.25 * if self.cc_response { self.cc } else { 1.0 }
            };
            let sample = if self.lookahead {
                let delayed = self.delay[self.cursor];
                self.delay[self.cursor] = sample;
                self.cursor = (self.cursor + 1) % LATENCY;
                delayed
            } else {
                sample
            };
            for channel in 0..2 {
                block.audio.output_mut(channel).unwrap()[frame] = sample;
            }
        }
        for event in &block.events[next..] {
            self.apply(event.data);
        }
        ProcessStatus::Continue
    }
    fn reset(&mut self) {
        self.cc = 1.0;
        self.remaining = 0;
        self.delay.fill(0.0);
        self.cursor = 0;
    }
    fn deactivate(&mut self) {
        self.reset();
        self.config = None;
    }
    fn latency_samples(&self) -> u32 {
        if self.lookahead { LATENCY as u32 } else { 0 }
    }
    fn tail(&self) -> Tail {
        Tail::Samples((CLICK_SAMPLES + self.latency_samples() as usize) as u64)
    }
}
