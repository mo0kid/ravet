// Minimal sample-timed gain effect / monophonic sine instrument.
// Replace this processor with your own. No format wrapper code is needed.
use ravet_foundation::{EventData, NotePhase, ParameterId, PrepareError,
    ProcessBlock, ProcessConfig, ProcessError, ProcessStatus, Processor};
const INSTRUMENT: bool = @INSTRUMENT@;
pub struct MyProcessor {
    config: Option<ProcessConfig>,
    gain: f32,
    bypass: bool,
    phase: f64,
    frequency: f64,
    velocity: f32,
    key: u8,
}
impl Default for MyProcessor {
    fn default() -> Self {
        Self { config: None, gain: 0.5, bypass: false, phase: 0.0,
            frequency: 440.0, velocity: 0.0, key: 69 }
    }
}
impl MyProcessor {
    fn event(&mut self, data: EventData) {
        match data {
            EventData::ParameterValue { id: ParameterId(1), value } => self.gain = value.clamp(0.0, 1.0),
            EventData::ParameterValue { id: ParameterId(2), value } => self.bypass = value >= 0.5,
            EventData::NoteOn { key, velocity, .. } if INSTRUMENT => {
                self.key = key; self.velocity = velocity;
                self.frequency = 440.0 * 2.0_f64.powf((f64::from(key) - 69.0) / 12.0);
            }
            EventData::NoteOff { key, .. } if INSTRUMENT && key == self.key => self.velocity = 0.0,
            EventData::NoteWithId { phase, key, velocity, channel, .. } if INSTRUMENT => {
                self.event(match phase {
                    NotePhase::On => EventData::NoteOn { key, velocity, channel },
                    NotePhase::Off => EventData::NoteOff { key, velocity, channel },
                });
            }
            EventData::NoteOffScoped { target, .. } | EventData::NoteChoke { target }
                if INSTRUMENT && target.key.is_none_or(|key| key == self.key) => self.velocity = 0.0,
            EventData::Midi(message) if INSTRUMENT => {
                let bytes = message.as_bytes();
                if bytes.len() == 3 {
                    let channel = bytes[0] & 15;
                    let key = bytes[1];
                    let velocity = f32::from(bytes[2]) / 127.0;
                    if bytes[0] & 0xf0 == 0x90 && bytes[2] != 0 {
                        self.event(EventData::NoteOn { key, velocity, channel });
                    } else if matches!(bytes[0] & 0xf0, 0x80 | 0x90) {
                        self.event(EventData::NoteOff { key, velocity, channel });
                    }
                }
            }
            _ => {}
        }
    }
}
impl Processor for MyProcessor {
    fn prepare(&mut self, config: ProcessConfig) -> Result<(), PrepareError> {
        self.config = None;
        if config.layout() != ravet_foundation::AudioLayout::@LAYOUT@ || !config.auxiliary_buses().is_empty() {
            return Err(PrepareError::UnsupportedLayout);
        }
        self.config = Some(config);
        self.reset();
        Ok(())
    }
    fn process(&mut self, block: &mut ProcessBlock<'_, '_, '_>) -> ProcessStatus {
        let validation = self.config.ok_or(ProcessError::NotPrepared).and_then(|c| c.validate_block(block));
        if let Err(error) = validation { block.clear_outputs(); return ProcessStatus::Fault(error); }
        let rate = self.config.unwrap().sample_rate();
        let mut next = 0;
        if block.audio.frames() == 0 { for event in block.events { self.event(event.data); } }
        for frame in 0..block.audio.frames() {
            while let Some(event) = block.events.get(next) {
                if event.frame_offset != frame { break; }
                self.event(event.data); next += 1;
            }
            let tone = self.phase.sin() as f32 * self.velocity * self.gain;
            self.phase = (self.phase + std::f64::consts::TAU * self.frequency / rate) % std::f64::consts::TAU;
            for channel in 0..block.audio.output_channels() {
                let input = block.audio.input(channel).map_or(0.0, |samples| samples[frame]);
                block.audio.output_mut(channel).unwrap()[frame] = if INSTRUMENT {
                    if self.bypass { 0.0 } else { tone }
                } else if self.bypass { input } else { input * self.gain };
            }
        }
        ProcessStatus::Continue
    }
    fn reset(&mut self) { self.phase = 0.0; self.velocity = 0.0; }
    fn deactivate(&mut self) { self.config = None; self.reset(); }
}
