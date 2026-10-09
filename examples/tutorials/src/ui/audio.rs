//! Bounded example-level command handoff; device adapters remain unchanged.
use crate::delay::DelayProcessor;
use crate::synth::{SubtractiveSynth, SynthPatch};
use ravet_foundation::{
    AudioBuffer, AudioLayout, Event, EventData, MidiMessage, PrepareError, ProcessBlock,
    ProcessConfig, ProcessError, ProcessStatus, Processor, SnapshotMailbox, Transport,
};
use ravet_signal::PeakRmsMeter;
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};

pub const MAX_FRAMES: usize = 512;
pub const QUEUE: usize = 16;
const MIDI_BUDGET: usize = 64;
pub struct Routing {
    pub input: u64,
    pub output: u64,
}
impl Default for Routing {
    fn default() -> Self {
        Self {
            input: 3,
            output: 3,
        }
    }
}
struct MidiSource {
    receiver: ravet_midi_device::MidiReceiver,
    dropped: u64,
}
pub struct Shared {
    pub telemetry: SnapshotMailbox<80>,
    pub panic: AtomicBool,
}
impl Shared {
    pub fn new() -> Arc<Self> {
        Arc::new(Self {
            telemetry: SnapshotMailbox::new([0; 80]),
            panic: AtomicBool::new(false),
        })
    }
}
// Fixed storage deliberately avoids heap-owned patch destruction in the callback.
#[allow(clippy::large_enum_variant)]
pub enum Command {
    Patch(SynthPatch),
    Delay([f32; 5]),
    Demo(bool),
    Midi(MidiMessage),
}
enum Engine {
    Synth(Box<SubtractiveSynth>),
    Effect(Box<DelayProcessor>),
}
pub struct Audio {
    engine: Engine,
    commands: rtrb::Consumer<Command>,
    midi: Vec<MidiSource>,
    routing: Routing,
    midi_cursor: usize,
    shared: Arc<Shared>,
    scratch: [Vec<f32>; 2],
    config: Option<ProcessConfig>,
    demo: bool,
    phase: [f64; 2],
    frames: u64,
    dropped: u64,
}
impl Audio {
    pub fn new(
        synth: bool,
        demo: bool,
        commands: rtrb::Consumer<Command>,
        midi: Vec<ravet_midi_device::MidiReceiver>,
        shared: Arc<Shared>,
        routing: Routing,
    ) -> Self {
        Self {
            engine: if synth {
                Engine::Synth(Box::default())
            } else {
                Engine::Effect(Box::default())
            },
            commands,
            midi: midi
                .into_iter()
                .map(|receiver| MidiSource {
                    receiver,
                    dropped: 0,
                })
                .collect(),
            routing,
            midi_cursor: 0,
            shared,
            scratch: [Vec::new(), Vec::new()],
            config: None,
            demo,
            phase: [0.; 2],
            frames: 0,
            dropped: 0,
        }
    }
    fn process_inner(&mut self, frames: usize, events: &[Event]) -> ProcessStatus {
        let [left, right] = &mut self.scratch;
        let mut views = [&mut left[..frames], &mut right[..frames]];
        let audio = match self.engine {
            Engine::Synth(_) => AudioBuffer::separate(&[], &mut views, frames),
            Engine::Effect(_) => AudioBuffer::in_place(&mut views, frames),
        }
        .expect("prepared stereo scratch");
        let mut block = ProcessBlock::new(audio, events, Transport::default());
        match &mut self.engine {
            Engine::Synth(s) => s.process(&mut block),
            Engine::Effect(g) => g.process(&mut block),
        }
    }
    fn flush_midi(&mut self, message: MidiMessage) {
        self.process_inner(
            0,
            &[Event {
                frame_offset: 0,
                data: EventData::Midi(message),
            }],
        );
    }
    fn reset_engine(&mut self) {
        match &mut self.engine {
            Engine::Synth(s) => s.reset(),
            Engine::Effect(g) => g.reset(),
        }
    }
}
impl Processor for Audio {
    fn prepare(&mut self, config: ProcessConfig) -> Result<(), PrepareError> {
        self.config = None;
        if config.maximum_frames() > MAX_FRAMES
            || !config.auxiliary_buses().is_empty()
            || config.maximum_output_events() != 0
        {
            return Err(PrepareError::InvalidMaximumFrames);
        }
        let channels = config.layout().output_channels();
        let valid = if channels == 64 {
            u64::MAX
        } else {
            (1u64 << channels) - 1
        };
        if self.routing.output == 0 || self.routing.output & !valid != 0 || self.midi.len() > 16 {
            return Err(PrepareError::UnsupportedLayout);
        }
        if config.layout().input_channels() > 0 {
            let channels = config.layout().input_channels();
            let valid = if channels == 64 {
                u64::MAX
            } else {
                (1u64 << channels) - 1
            };
            if self.routing.input == 0 || self.routing.input & !valid != 0 {
                return Err(PrepareError::UnsupportedLayout);
            }
        }
        let layout = match self.engine {
            Engine::Synth(_) => AudioLayout::STEREO_INSTRUMENT,
            Engine::Effect(_) => AudioLayout::STEREO,
        };
        let inner = ProcessConfig::new(config.sample_rate(), config.maximum_frames(), 2, layout)?;
        match &mut self.engine {
            Engine::Synth(s) => s.prepare(inner)?,
            Engine::Effect(g) => g.prepare(inner)?,
        };
        self.scratch = [
            vec![0.; config.maximum_frames()],
            vec![0.; config.maximum_frames()],
        ];
        self.config = Some(config);
        Ok(())
    }
    fn process(&mut self, block: &mut ProcessBlock<'_, '_, '_>) -> ProcessStatus {
        let config = match self
            .config
            .ok_or(ProcessError::NotPrepared)
            .and_then(|c| c.validate_block(block).map(|()| c))
        {
            Ok(c) => c,
            Err(e) => {
                block.clear_outputs();
                return ProcessStatus::Fault(e);
            }
        };
        if self.shared.panic.swap(false, Ordering::AcqRel) {
            self.reset_engine();
            for _ in 0..QUEUE {
                if self.commands.pop().is_err() {
                    break;
                }
            }
            for source in &mut self.midi {
                let midi = &mut source.receiver;
                for _ in 0..256 {
                    if midi.pop().is_none() {
                        break;
                    }
                }
            }
        }
        for _ in 0..QUEUE {
            let Ok(command) = self.commands.pop() else {
                break;
            };
            match command {
                Command::Patch(p) => {
                    if let Engine::Synth(s) = &mut self.engine {
                        s.set_patch(&p).expect("matching prepared rate");
                    }
                }
                Command::Delay(values) => {
                    if let Engine::Effect(delay) = &mut self.engine {
                        delay
                            .set_controls(values)
                            .expect("validated delay controls");
                    }
                }
                Command::Demo(enabled) => self.demo = enabled,
                Command::Midi(m) => self.flush_midi(m),
            }
        }
        let mut lost = false;
        for source in &mut self.midi {
            let drops = source.receiver.statistics().dropped_full;
            if drops != source.dropped {
                self.dropped = self
                    .dropped
                    .wrapping_add(drops.wrapping_sub(source.dropped));
                source.dropped = drops;
                lost = true;
                for _ in 0..256 {
                    if source.receiver.pop().is_none() {
                        break;
                    }
                }
            }
        }
        if lost {
            self.reset_engine();
        }
        let mut empty = 0;
        for _ in 0..MIDI_BUDGET {
            if self.midi.is_empty() {
                break;
            }
            let index = self.midi_cursor % self.midi.len();
            self.midi_cursor = (index + 1) % self.midi.len();
            let packet = self.midi[index].receiver.pop();
            if let Some(packet) = packet {
                empty = 0;
                if let Ok(message) = MidiMessage::from_bytes(packet.as_bytes()) {
                    self.flush_midi(message);
                }
            } else {
                empty += 1;
                if empty >= self.midi.len() {
                    break;
                }
            }
        }
        let frames = block.audio.frames();
        let mut meters = [PeakRmsMeter::default(); 4];
        if matches!(self.engine, Engine::Effect(_)) {
            for frame in 0..frames {
                for channel in 0..2 {
                    let sample = if self.demo {
                        // Staggered, tapered bursts leave space for audible echoes.
                        let beat = ((self.frames + frame as u64) as f64 / config.sample_rate()
                            - if channel == 0 { 0. } else { 0.3 })
                        .rem_euclid(0.9);
                        let envelope = if beat < 0.18 {
                            (std::f64::consts::PI * beat / 0.18).sin().powi(2)
                        } else {
                            0.
                        };
                        let sample =
                            (0.2 * envelope * (self.phase[channel] * std::f64::consts::TAU).sin())
                                as f32;
                        self.phase[channel] = (self.phase[channel]
                            + [220., 330.][channel] / config.sample_rate())
                        .fract();
                        sample
                    } else {
                        let mut sum = 0.;
                        let mut count = 0;
                        for first in (0..block.audio.input_channels()).step_by(2) {
                            if self.routing.input & (1u64 << first) == 0 {
                                continue;
                            }
                            let index = (first + channel).min(block.audio.input_channels() - 1);
                            sum += block.audio.input(index).expect("selected input")[frame];
                            count += 1;
                        }
                        if count == 0 { 0. } else { sum / count as f32 }
                    };
                    self.scratch[channel][frame] = sample;
                    meters[channel].push(sample);
                }
            }
        }
        let status = self.process_inner(frames, &[]);
        if status != ProcessStatus::Continue {
            block.clear_outputs();
            return status;
        }
        block.clear_outputs();
        for frame in 0..frames {
            for channel in 0..2 {
                meters[2 + channel].push(self.scratch[channel][frame]);
            }
            for first in (0..block.audio.output_channels()).step_by(2) {
                if self.routing.output & (1u64 << first) == 0 {
                    continue;
                }
                if first + 1 == block.audio.output_channels() {
                    block.audio.output_mut(first).expect("selected mono")[frame] =
                        (self.scratch[0][frame] + self.scratch[1][frame]) * 0.5;
                } else {
                    for channel in 0..2 {
                        block
                            .audio
                            .output_mut(first + channel)
                            .expect("selected stereo")[frame] = self.scratch[channel][frame];
                    }
                }
            }
        }
        self.frames = self.frames.wrapping_add(frames as u64);
        let mut words = [0; 80];
        words[0] = self.frames as u32;
        words[1] = (self.frames >> 32) as u32;
        for (i, meter) in meters.iter().enumerate() {
            let r = meter.reading();
            words[2 + i * 2] = r.peak.to_bits();
            words[3 + i * 2] = r.rms.to_bits();
        }
        words[10] = self.dropped as u32;
        if let Engine::Synth(s) = &self.engine {
            let envelopes = s.envelope_levels();
            for (i, envelope) in envelopes.into_iter().enumerate() {
                let voice = s.voices()[i];
                let Some(note) = voice.note else {
                    continue;
                };
                let start = 12 + i * 8;
                let phase = match voice.phase {
                    ravet_instrument::VoicePhase::Idle => 0,
                    ravet_instrument::VoicePhase::Held => 1,
                    ravet_instrument::VoicePhase::Sustained => 2,
                    ravet_instrument::VoicePhase::Released => 3,
                };
                words[start] = u32::from(note.channel) | u32::from(note.key) << 8 | phase << 16;
                for (offset, value) in [
                    voice.velocity,
                    voice.expression.member_bend,
                    voice.expression.master_bend,
                    voice.expression.pressure,
                    voice.expression.timbre,
                    envelope as f32,
                ]
                .into_iter()
                .enumerate()
                {
                    words[start + 1 + offset] = value.to_bits();
                }
            }
        }
        let _ = self.shared.telemetry.try_publish(words);
        ProcessStatus::Continue
    }
    fn reset(&mut self) {
        self.reset_engine();
        self.phase = [0.; 2];
    }
    fn deactivate(&mut self) {
        match &mut self.engine {
            Engine::Synth(s) => s.deactivate(),
            Engine::Effect(g) => g.deactivate(),
        };
        self.config = None;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ravet_midi_device::{CaptureStatus, PreparedMidiInput};
    #[test]
    fn demo_repeats_for_multiple_cycles_and_can_switch_to_live_input() {
        let (mut commands, consumer) = rtrb::RingBuffer::new(QUEUE);
        let mut audio = Audio::new(
            false,
            true,
            consumer,
            Vec::new(),
            Shared::new(),
            Routing::default(),
        );
        audio
            .prepare(ProcessConfig::new(48000., 64, 0, AudioLayout::new(4, 6).unwrap()).unwrap())
            .unwrap();
        commands
            .push(Command::Delay([375., 0.45, 0.35, 1., 0.]))
            .ok()
            .unwrap();
        let input = [[0.; 64]; 4];
        let mut output = [[0.; 64]; 6];
        let mut energy = [[0f64; 2]; 3];
        for block in 0..2025 {
            assert_eq!(
                render(&mut audio, &input, &mut output),
                ProcessStatus::Continue
            );
            let cycle = block / 675; // exactly 0.9 seconds per cycle at 48 kHz
            for (channel, samples) in output[..2].iter().enumerate() {
                energy[cycle][channel] +=
                    samples.iter().map(|s| f64::from(*s).powi(2)).sum::<f64>();
            }
        }
        for cycle in energy {
            for channel in cycle {
                assert!(channel > 10.);
            }
        }
        commands.push(Command::Demo(false)).ok().unwrap();
        render(&mut audio, &input, &mut output);
        assert!(output.iter().flatten().all(|s| *s == 0.));
        let input = [[0.1; 64], [-0.2; 64], [0.; 64], [0.; 64]];
        render(&mut audio, &input, &mut output);
        assert_eq!(output[0], input[0]);
        assert_eq!(output[1], input[1]);
    }
    fn render(
        audio: &mut Audio,
        input: &[[f32; 64]; 4],
        output: &mut [[f32; 64]; 6],
    ) -> ProcessStatus {
        let inputs = input.each_ref().map(|c| &c[..]);
        let mut outputs = output.each_mut().map(|c| &mut c[..]);
        let buffer = AudioBuffer::separate(&inputs, &mut outputs, 64).unwrap();
        audio.process(&mut ProcessBlock::new(buffer, &[], Transport::default()))
    }
    #[test]
    fn selected_input_pairs_mix_and_unselected_outputs_remain_silent_without_heap_actions() {
        let (mut commands, consumer) = rtrb::RingBuffer::new(QUEUE);
        let shared = Shared::new();
        let mut audio = Audio::new(
            false,
            false,
            consumer,
            Vec::new(),
            shared,
            Routing {
                input: 15,
                output: 60,
            },
        );
        audio
            .prepare(ProcessConfig::new(48000., 64, 0, AudioLayout::new(4, 6).unwrap()).unwrap())
            .unwrap();
        let input = [[1.; 64], [2.; 64], [3.; 64], [4.; 64]];
        let mut output = [[99.; 64]; 6];
        commands
            .push(Command::Delay([375., 0.45, 0.35, 1., 0.]))
            .ok()
            .unwrap();
        let measured = allocation_counter::measure(|| {
            for _ in 0..1000 {
                assert_eq!(
                    render(&mut audio, &input, &mut output),
                    ProcessStatus::Continue
                );
            }
            audio.reset();
        });
        assert_eq!(measured.count_total, 0);
        assert_eq!(measured.count_current, 0);
        assert_eq!(output[0], [0.; 64]);
        assert_eq!(output[1], [0.; 64]);
        for channel in [2, 4] {
            assert_eq!(output[channel], [2.; 64]);
            assert_eq!(output[channel + 1], [3.; 64]);
        }
    }
    #[test]
    fn multiple_midi_sources_and_dropped_note_off_clear_voices() {
        let (mut first, receiver_a) = PreparedMidiInput::new(2).unwrap();
        let (mut second, receiver_b) = PreparedMidiInput::new(2).unwrap();
        let (mut commands, consumer) = rtrb::RingBuffer::new(QUEUE);
        let shared = Shared::new();
        let mut audio = Audio::new(
            true,
            false,
            consumer,
            vec![receiver_a, receiver_b],
            shared.clone(),
            Routing::default(),
        );
        audio
            .prepare(ProcessConfig::new(48000., 64, 0, AudioLayout::new(4, 6).unwrap()).unwrap())
            .unwrap();
        assert_eq!(first.capture(0, &[0x90, 60, 100]), CaptureStatus::Accepted);
        assert_eq!(second.capture(0, &[0x91, 64, 100]), CaptureStatus::Accepted);
        let mut output = [[0.; 64]; 6];
        let input = [[0.; 64]; 4];
        assert_eq!(
            render(&mut audio, &input, &mut output),
            ProcessStatus::Continue
        );
        let Engine::Synth(synth) = &audio.engine else {
            panic!("synth")
        };
        assert_eq!(
            synth.voices().iter().filter(|v| v.note.is_some()).count(),
            2
        );
        first.capture(1, &[0xb0, 1, 0]);
        first.capture(2, &[0xb0, 1, 0]);
        assert_eq!(first.capture(3, &[0x80, 60, 0]), CaptureStatus::Full);
        render(&mut audio, &input, &mut output);
        assert_eq!(audio.dropped, 1);
        assert!(output.iter().flatten().all(|s| *s == 0.));
        commands
            .push(Command::Midi(
                MidiMessage::from_bytes(&[0x90, 60, 100]).unwrap(),
            ))
            .ok()
            .unwrap();
        shared.panic.store(true, Ordering::Release);
        render(&mut audio, &input, &mut output);
        assert!(output.iter().flatten().all(|s| *s == 0.));
    }
    #[test]
    fn routing_is_checked_before_processing() {
        let (_, consumer) = rtrb::RingBuffer::new(QUEUE);
        let mut audio = Audio::new(
            true,
            false,
            consumer,
            Vec::new(),
            Shared::new(),
            Routing {
                input: 3,
                output: 1 << 9,
            },
        );
        assert_eq!(
            audio.prepare(
                ProcessConfig::new(48000., 64, 0, AudioLayout::STEREO_INSTRUMENT).unwrap()
            ),
            Err(PrepareError::UnsupportedLayout)
        );
    }
}
