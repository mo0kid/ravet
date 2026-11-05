use super::*;
use crate::{Category, Parameter, Product, ProductError, StateFormat};
use ravet_foundation::{AudioLayout, ParameterGesturePhase, Transport};

const GAIN: ParameterId = ParameterId(1);
const BYPASS: ParameterId = ParameterId(2);
const RATE: f64 = 8000.0;

/// Doubles its input, counts processed frames and records received events.
struct Probe {
    config: Option<ProcessConfig>,
    frames: usize,
    seen: Vec<(usize, usize, EventData)>,
    resets: usize,
}
impl Probe {
    fn new() -> Self {
        Self {
            config: None,
            frames: 0,
            seen: Vec::with_capacity(4096),
            resets: 0,
        }
    }
}
impl Processor<f32> for Probe {
    fn prepare(&mut self, config: ProcessConfig) -> Result<(), PrepareError> {
        self.config = Some(config);
        Ok(())
    }
    fn process(&mut self, block: &mut ProcessBlock<'_, '_, '_, f32>) -> ProcessStatus {
        let config = self.config.expect("prepared");
        if let Err(error) = config.validate_block(block) {
            block.clear_outputs();
            return ProcessStatus::Fault(error);
        }
        let frames = block.audio.frames();
        for event in block.events {
            if self.seen.len() < self.seen.capacity() {
                self.seen.push((frames, event.frame_offset, event.data));
            }
        }
        for channel in 0..block.audio.output_channels() {
            for frame in 0..frames {
                let input = block.audio.input(channel).map_or(0.0, |c| c[frame]);
                block.audio.output_mut(channel).unwrap()[frame] = 2.0 * input;
            }
        }
        self.frames += frames;
        ProcessStatus::Continue
    }
    fn reset(&mut self) {
        self.resets += 1;
    }
    fn deactivate(&mut self) {
        self.config = None;
    }
}

fn value(frame_offset: usize, id: ParameterId, value: f32) -> Event {
    Event {
        frame_offset,
        data: EventData::ParameterValue { id, value },
    }
}
fn note(frame_offset: usize) -> Event {
    Event {
        frame_offset,
        data: EventData::NoteOn {
            channel: 0,
            key: 60,
            velocity: 1.0,
        },
    }
}

fn prepared(automation: AutomationPolicy, bypass: BypassPolicy) -> PolicyProcessor<Probe> {
    let mut processor =
        PolicyProcessor::with_policies(Probe::new(), automation, bypass, Some(BYPASS));
    processor
        .prepare(ProcessConfig::new(RATE, 512, 64, AudioLayout::STEREO).unwrap())
        .unwrap();
    processor
}

fn run(
    processor: &mut PolicyProcessor<Probe>,
    input: &[f32],
    events: &[Event],
) -> ([Vec<f32>; 2], ProcessStatus) {
    let frames = input.len();
    let mut output = [vec![9.0; frames], vec![9.0; frames]];
    let inputs = [input, input];
    let mut outputs = output.each_mut().map(|c| &mut c[..]);
    let audio = AudioBuffer::separate(&inputs, &mut outputs, frames).unwrap();
    let status = processor.process(&mut ProcessBlock::new(audio, events, Transport::default()));
    (output, status)
}

fn seen(processor: &PolicyProcessor<Probe>) -> Vec<(usize, usize, EventData)> {
    processor.processor().seen.clone()
}

#[test]
fn default_policies_forward_everything() {
    let mut processor = prepared(AutomationPolicy::SampleAccurate, BypassPolicy::Processor);
    let events = [value(1, GAIN, 0.5), value(3, BYPASS, 1.0), note(5)];
    let (output, status) = run(&mut processor, &[1.0; 8], &events);
    assert_eq!(status, ProcessStatus::Continue);
    assert_eq!(output[0], [2.0; 8]);
    let offsets: Vec<_> = seen(&processor).iter().map(|e| e.1).collect();
    assert_eq!(offsets, [1, 3, 5]);
    assert!(!processor.is_bypassed());
}

#[test]
fn per_block_moves_parameter_events_to_the_start() {
    let mut processor = prepared(AutomationPolicy::PerBlock, BypassPolicy::Processor);
    let gesture = Event {
        frame_offset: 6,
        data: EventData::ParameterGesture {
            id: GAIN,
            phase: ParameterGesturePhase::End,
        },
    };
    let events = [
        note(0),
        value(2, GAIN, 0.5),
        note(4),
        gesture,
        value(7, BYPASS, 1.0),
    ];
    run(&mut processor, &[0.0; 8], &events);
    let seen: Vec<_> = seen(&processor).iter().map(|e| (e.1, e.2)).collect();
    assert_eq!(
        seen,
        [
            (0, events[1].data),
            (0, events[3].data),
            (0, events[4].data),
            (0, events[0].data),
            (4, events[2].data),
        ]
    );
}

#[test]
fn hard_bypass_switches_at_the_event_offset() {
    let mut processor = prepared(AutomationPolicy::SampleAccurate, BypassPolicy::Hard);
    let input: Vec<f32> = (1..=8).map(|i| i as f32).collect();
    let (output, status) = run(&mut processor, &input, &[value(4, BYPASS, 1.0)]);
    assert_eq!(status, ProcessStatus::Continue);
    assert_eq!(output[1], [2.0, 4.0, 6.0, 8.0, 5.0, 6.0, 7.0, 8.0]);
    assert!(processor.is_bypassed());
    assert_eq!(
        processor.processor().frames,
        8,
        "the processor keeps running"
    );
    assert!(seen(&processor).is_empty(), "bypass is consumed");
    let (output, _) = run(&mut processor, &input, &[value(2, BYPASS, 0.0)]);
    assert_eq!(output[0], [1.0, 2.0, 6.0, 8.0, 10.0, 12.0, 14.0, 16.0]);
}

#[test]
fn per_block_applies_bypass_at_the_start() {
    let mut processor = prepared(AutomationPolicy::PerBlock, BypassPolicy::Hard);
    let (output, _) = run(&mut processor, &[1.0; 8], &[value(6, BYPASS, 1.0)]);
    assert_eq!(output[0], [1.0; 8]);
}

#[test]
fn crossfade_ramps_linearly_and_reset_completes_it() {
    // 1 ms at 8 kHz is 8 samples: the wet gain falls by 1/8 per sample.
    let mut processor = prepared(
        AutomationPolicy::SampleAccurate,
        BypassPolicy::Crossfade { milliseconds: 1.0 },
    );
    let (output, _) = run(&mut processor, &[1.0; 12], &[value(2, BYPASS, 1.0)]);
    let expected = [
        2.0, 2.0, 1.875, 1.75, 1.625, 1.5, 1.375, 1.25, 1.125, 1.0, 1.0, 1.0,
    ];
    for (sample, expected) in output[0].iter().zip(expected) {
        assert!((sample - expected).abs() < 1e-6, "{:?}", output[0]);
    }
    // Leaving bypass ramps back up, then reset jumps to the target.
    let (output, _) = run(&mut processor, &[1.0; 4], &[value(0, BYPASS, 0.0)]);
    assert!((output[0][0] - 1.125).abs() < 1e-6);
    processor.reset();
    let (output, _) = run(&mut processor, &[1.0; 4], &[]);
    assert_eq!(output[0], [2.0; 4]);
}

#[test]
fn frozen_bypass_stops_the_processor_but_delivers_parameters() {
    let mut processor = prepared(AutomationPolicy::SampleAccurate, BypassPolicy::Frozen);
    run(&mut processor, &[1.0; 8], &[]);
    assert_eq!(processor.processor().frames, 8);
    let input: Vec<f32> = (0..8).map(|i| i as f32).collect();
    let events = [note(1), value(3, GAIN, 0.25), value(5, BYPASS, 1.0)];
    let (output, status) = run(&mut processor, &input, &events);
    assert_eq!(status, ProcessStatus::Continue);
    assert_eq!(output[0], input, "the whole block passes through");
    assert_eq!(output[1], input);
    assert_eq!(processor.processor().frames, 8, "history is frozen");
    // Only the parameter reached the processor, in a zero-frame block.
    assert_eq!(seen(&processor), [(0, 0, events[1].data)]);
    // Unbypassing resumes processing for the whole block.
    let (output, _) = run(&mut processor, &input, &[value(7, BYPASS, 0.0)]);
    assert_eq!(output[0][1], 2.0);
    assert_eq!(processor.processor().frames, 16);
}

#[test]
fn frozen_bypass_rejects_auxiliary_buses() {
    let mut processor = PolicyProcessor::with_policies(
        Probe::new(),
        AutomationPolicy::SampleAccurate,
        BypassPolicy::Frozen,
        Some(BYPASS),
    );
    let config = ProcessConfig::new(RATE, 64, 8, AudioLayout::STEREO)
        .unwrap()
        .with_auxiliary_buses(&[AudioLayout::STEREO])
        .unwrap();
    assert_eq!(
        processor.prepare(config),
        Err(PrepareError::UnsupportedLayout)
    );
}

#[test]
fn invalid_blocks_fault_and_silence() {
    let mut processor = prepared(AutomationPolicy::PerBlock, BypassPolicy::Hard);
    let (output, status) = run(&mut processor, &[1.0; 4], &[value(9, GAIN, 0.5)]);
    assert!(matches!(status, ProcessStatus::Fault(_)));
    assert_eq!(output[0], [0.0; 4]);
}

#[test]
fn product_validation_requires_a_bypass_parameter() {
    const WITHOUT: &[Parameter] = &[Parameter::linear("gain", "Gain", 0.0, 1.0, 1.0)];
    const WITH: &[Parameter] = &[
        Parameter::linear("gain", "Gain", 0.0, 1.0, 1.0),
        Parameter::bypass("bypass", "Bypass"),
    ];
    let product = Product::new(
        "Policy",
        "RAVET",
        "1.0.0",
        "org.ravet.policy",
        Category::Effect,
        StateFormat {
            tag: *b"RVTPOL01",
            version: 1,
        },
    );
    for policy in [
        BypassPolicy::Hard,
        BypassPolicy::Frozen,
        BypassPolicy::Crossfade { milliseconds: 5.0 },
    ] {
        assert_eq!(
            product
                .with_parameters(WITHOUT)
                .with_bypass(policy)
                .resolve()
                .unwrap_err(),
            ProductError::BypassPolicyWithoutParameter
        );
        let resolved = product
            .with_parameters(WITH)
            .with_bypass(policy)
            .with_automation(AutomationPolicy::PerBlock)
            .resolve()
            .unwrap();
        let wrapped = PolicyProcessor::new(Probe::new(), &resolved);
        assert_eq!(wrapped.bypass, policy);
        assert_eq!(wrapped.automation, AutomationPolicy::PerBlock);
    }
    assert!(product.with_parameters(WITHOUT).resolve().is_ok());
    for milliseconds in [0.0, -1.0, f32::NAN, 1000.5] {
        assert_eq!(
            product
                .with_parameters(WITH)
                .with_bypass(BypassPolicy::Crossfade { milliseconds })
                .resolve()
                .unwrap_err(),
            ProductError::InvalidBypassPolicy
        );
    }
}

#[test]
fn processing_allocates_nothing() {
    let policies = [
        (AutomationPolicy::SampleAccurate, BypassPolicy::Processor),
        (AutomationPolicy::PerBlock, BypassPolicy::Processor),
        (AutomationPolicy::PerBlock, BypassPolicy::Hard),
        (
            AutomationPolicy::SampleAccurate,
            BypassPolicy::Crossfade { milliseconds: 3.0 },
        ),
        (AutomationPolicy::SampleAccurate, BypassPolicy::Frozen),
    ];
    for (automation, bypass) in policies {
        let mut processor = prepared(automation, bypass);
        let input = [0.25f32; 256];
        let inputs = [&input[..], &input[..]];
        let mut output = [[0.0f32; 256]; 2];
        let measured = allocation_counter::measure(|| {
            for i in 0..200 {
                let events = [
                    note(0),
                    value(10, GAIN, 0.5),
                    value(100, BYPASS, (i % 2) as f32),
                    note(200),
                ];
                let mut outputs = output.each_mut().map(|c| &mut c[..]);
                let audio = AudioBuffer::separate(&inputs, &mut outputs, 256).unwrap();
                let status =
                    processor.process(&mut ProcessBlock::new(audio, &events, Transport::default()));
                assert_eq!(status, ProcessStatus::Continue);
            }
            processor.reset();
        });
        assert_eq!(measured.count_total, 0, "{automation:?} {bypass:?}");
    }
}
