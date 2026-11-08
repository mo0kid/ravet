use super::*;
use crate::{
    AuxiliaryBus, Category, MAXIMUM_BUS_NAME_BYTES, Parameter, Product, ProductError, StateFormat,
};
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
    /// Reported latency; the probe's output is not actually delayed.
    latency: u32,
}
impl Probe {
    fn new() -> Self {
        Self {
            config: None,
            frames: 0,
            seen: Vec::with_capacity(4096),
            resets: 0,
            latency: 0,
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
    fn latency_samples(&self) -> u32 {
        self.latency
    }
}

/// Doubles its input delayed by `LAG` samples and reports that latency.
struct Lagged {
    history: [[f32; LAG]; 2],
    position: usize,
    frames: usize,
}
const LAG: usize = 3;
impl Processor<f32> for Lagged {
    fn prepare(&mut self, _: ProcessConfig) -> Result<(), PrepareError> {
        Ok(())
    }
    fn process(&mut self, block: &mut ProcessBlock<'_, '_, '_, f32>) -> ProcessStatus {
        let frames = block.audio.frames();
        for frame in 0..frames {
            for channel in 0..2 {
                let input = block.audio.input(channel).unwrap()[frame];
                let delayed = std::mem::replace(&mut self.history[channel][self.position], input);
                block.audio.output_mut(channel).unwrap()[frame] = 2.0 * delayed;
            }
            self.position = (self.position + 1) % LAG;
        }
        self.frames += frames;
        ProcessStatus::Continue
    }
    fn reset(&mut self) {
        self.history = [[0.0; LAG]; 2];
    }
    fn deactivate(&mut self) {}
    fn latency_samples(&self) -> u32 {
        LAG as u32
    }
}
fn lagged(bypass: BypassPolicy) -> PolicyProcessor<Lagged> {
    let lagged = Lagged {
        history: [[0.0; LAG]; 2],
        position: 0,
        frames: 0,
    };
    let mut processor = PolicyProcessor::with_policies(
        lagged,
        AutomationPolicy::SampleAccurate,
        bypass,
        Some(BYPASS),
    );
    processor
        .prepare(ProcessConfig::new(RATE, 512, 64, AudioLayout::STEREO).unwrap())
        .unwrap();
    processor
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

fn run<P: Processor<f32>>(
    processor: &mut PolicyProcessor<P>,
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

fn ramp(from: usize, to: usize) -> Vec<f32> {
    (from..=to).map(|i| i as f32).collect()
}

#[test]
fn hard_bypass_delays_the_dry_signal_by_the_latency() {
    let mut processor = lagged(BypassPolicy::Hard);
    let (output, _) = run(&mut processor, &ramp(1, 8), &[]);
    assert_eq!(output[0], [0.0, 0.0, 0.0, 2.0, 4.0, 6.0, 8.0, 10.0]);
    // The dry signal continues the processed signal's timing.
    let (output, _) = run(&mut processor, &ramp(9, 16), &[value(4, BYPASS, 1.0)]);
    assert_eq!(output[0], [12.0, 14.0, 16.0, 18.0, 10.0, 11.0, 12.0, 13.0]);
    assert_eq!(output[1], output[0]);
    // A reset clears the delayed history along with the processor's.
    processor.reset();
    let (output, _) = run(&mut processor, &ramp(17, 20), &[]);
    assert_eq!(output[0], [0.0, 0.0, 0.0, 17.0]);
}

#[test]
fn crossfade_mixes_aligned_signals() {
    // The processed signal is twice the delayed input throughout the ramp.
    let mut processor = lagged(BypassPolicy::Crossfade { milliseconds: 1.0 });
    run(&mut processor, &ramp(1, 8), &[]);
    let (output, _) = run(&mut processor, &ramp(9, 20), &[value(0, BYPASS, 1.0)]);
    for (frame, sample) in output[0].iter().enumerate() {
        let dry = (frame + 6) as f32;
        let wet = ((7 - frame.min(8) as i32) as f32 / 8.0).max(0.0);
        let expected = dry * (1.0 + wet);
        assert!((sample - expected).abs() < 1e-5, "{:?}", output[0]);
    }
}

#[test]
fn frozen_bypass_delays_the_pass_through() {
    let mut processor = lagged(BypassPolicy::Frozen);
    run(&mut processor, &ramp(1, 8), &[]);
    let (output, _) = run(&mut processor, &ramp(9, 16), &[value(0, BYPASS, 1.0)]);
    assert_eq!(output[0], ramp(6, 13));
    assert_eq!(output[1], ramp(6, 13));
    assert_eq!(processor.processor().frames, 8, "history is frozen");
    // Short blocks keep the delay continuous.
    let (output, _) = run(&mut processor, &ramp(17, 18), &[]);
    assert_eq!(output[0], [14.0, 15.0]);
    let (output, _) = run(&mut processor, &ramp(19, 19), &[]);
    assert_eq!(output[0], [16.0]);
}

#[test]
fn dry_delay_is_continuous_across_block_sizes_and_switches() {
    let mut processor = prepared(AutomationPolicy::SampleAccurate, BypassPolicy::Hard);
    processor.processor_mut().latency = 5;
    processor = processor.with_maximum_latency(5);
    processor
        .prepare(ProcessConfig::new(RATE, 512, 64, AudioLayout::STEREO).unwrap())
        .unwrap();
    let mut next = 1;
    for (index, frames) in [1, 2, 7, 3, 0, 11, 4, 5, 6, 1, 9].into_iter().enumerate() {
        let bypassed = index % 3 != 1;
        let input = ramp(next, next + frames - 1);
        let events = [value(0, BYPASS, if bypassed { 1.0 } else { 0.0 })];
        let (output, _) = run(&mut processor, &input, &events);
        for (frame, sample) in output[0].iter().enumerate() {
            let expected = if bypassed {
                (next + frame).saturating_sub(5) as f32
            } else {
                2.0 * input[frame]
            };
            assert_eq!(*sample, expected, "block {index}: {:?}", output[0]);
        }
        next += frames;
    }
}

#[test]
fn declared_maximum_latency_bounds_the_dry_delay() {
    // Latency raised after preparation is compensated up to the declared maximum.
    for (maximum, expected) in [
        (0, [1.0, 2.0, 3.0, 4.0]),
        (2, [0.0, 0.0, 1.0, 2.0]),
        (8, [0.0, 0.0, 0.0, 1.0]),
    ] {
        let mut processor = PolicyProcessor::with_policies(
            Probe::new(),
            AutomationPolicy::SampleAccurate,
            BypassPolicy::Hard,
            Some(BYPASS),
        )
        .with_maximum_latency(maximum);
        processor
            .prepare(ProcessConfig::new(RATE, 512, 64, AudioLayout::STEREO).unwrap())
            .unwrap();
        processor.processor_mut().latency = 3;
        let (output, _) = run(&mut processor, &ramp(1, 4), &[value(0, BYPASS, 1.0)]);
        assert_eq!(output[0], expected, "maximum {maximum}");
    }
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

/// Main output = 2 × input; auxiliary bus 0 (1 in, 1 out) outputs 3 × its
/// input. Validates every block, so a missing auxiliary bus faults.
struct WithAuxiliary {
    config: Option<ProcessConfig>,
    blocks: Vec<usize>,
}
impl Processor<f32> for WithAuxiliary {
    fn prepare(&mut self, config: ProcessConfig) -> Result<(), PrepareError> {
        self.config = Some(config);
        Ok(())
    }
    fn process(&mut self, block: &mut ProcessBlock<'_, '_, '_, f32>) -> ProcessStatus {
        if let Err(error) = self.config.unwrap().validate_block(block) {
            block.clear_outputs();
            return ProcessStatus::Fault(error);
        }
        let frames = block.audio.frames();
        self.blocks.push(frames);
        for channel in 0..2 {
            for frame in 0..frames {
                let input = block.audio.input(channel).unwrap()[frame];
                block.audio.output_mut(channel).unwrap()[frame] = 2.0 * input;
            }
        }
        let bus = &mut block.auxiliary.as_deref_mut().unwrap()[0];
        for frame in 0..frames {
            let input = bus.input(0).unwrap()[frame];
            bus.output_mut(0).unwrap()[frame] = 3.0 * input;
        }
        ProcessStatus::Continue
    }
    fn reset(&mut self) {}
    fn deactivate(&mut self) {}
}

fn with_auxiliary(bypass: BypassPolicy) -> PolicyProcessor<WithAuxiliary> {
    let inner = WithAuxiliary {
        config: None,
        blocks: Vec::new(),
    };
    let mut processor = PolicyProcessor::with_policies(
        inner,
        AutomationPolicy::SampleAccurate,
        bypass,
        Some(BYPASS),
    );
    let config = ProcessConfig::new(RATE, 64, 8, AudioLayout::STEREO)
        .unwrap()
        .with_auxiliary_buses(&[AudioLayout::MONO])
        .unwrap();
    processor.prepare(config).unwrap();
    processor
}

/// Run one block with a 1-in/1-out auxiliary bus; returns (main, auxiliary).
fn run_auxiliary(
    processor: &mut PolicyProcessor<WithAuxiliary>,
    input: &[f32],
    side: &[f32],
    events: &[Event],
) -> (Vec<f32>, Vec<f32>, ProcessStatus) {
    let frames = input.len();
    let mut main = [vec![9.0; frames], vec![9.0; frames]];
    let mut auxiliary_output = vec![9.0; frames];
    let inputs = [input, input];
    let mut outputs = main.each_mut().map(|c| &mut c[..]);
    let side_inputs = [side];
    let mut side_outputs = [&mut auxiliary_output[..]];
    let audio = AudioBuffer::separate(&inputs, &mut outputs, frames).unwrap();
    let mut buses = [AudioBuffer::separate(&side_inputs, &mut side_outputs, frames).unwrap()];
    let mut block = ProcessBlock::new(audio, events, Transport::default());
    block.auxiliary = Some(&mut buses);
    let status = processor.process(&mut block);
    let [left, _] = main;
    (left, auxiliary_output, status)
}

#[test]
fn hard_bypass_silences_auxiliary_outputs() {
    let mut processor = with_auxiliary(BypassPolicy::Hard);
    let (main, auxiliary, status) = run_auxiliary(
        &mut processor,
        &[1.0; 4],
        &[1.0; 4],
        &[value(2, BYPASS, 1.0)],
    );
    assert_eq!(status, ProcessStatus::Continue);
    assert_eq!(main, [2.0, 2.0, 1.0, 1.0]);
    assert_eq!(auxiliary, [3.0, 3.0, 0.0, 0.0]);
}

#[test]
fn crossfade_fades_auxiliary_outputs() {
    let mut processor = with_auxiliary(BypassPolicy::Crossfade { milliseconds: 0.5 });
    // 0.5 ms at 8 kHz is 4 samples.
    let (_, auxiliary, _) = run_auxiliary(
        &mut processor,
        &[1.0; 6],
        &[1.0; 6],
        &[value(0, BYPASS, 1.0)],
    );
    assert_eq!(auxiliary, [2.25, 1.5, 0.75, 0.0, 0.0, 0.0]);
}

#[test]
fn frozen_bypass_supports_auxiliary_buses() {
    let mut processor = with_auxiliary(BypassPolicy::Frozen);
    let (main, auxiliary, _) = run_auxiliary(&mut processor, &[1.0; 4], &[1.0; 4], &[]);
    assert_eq!((main[0], auxiliary[0]), (2.0, 3.0));
    let events = [value(0, BYPASS, 1.0), value(1, GAIN, 0.5)];
    let (main, auxiliary, status) = run_auxiliary(&mut processor, &[1.0; 4], &[1.0; 4], &events);
    assert_eq!(status, ProcessStatus::Continue);
    assert_eq!(main, [1.0; 4], "the main input passes through");
    assert_eq!(auxiliary, [0.0; 4], "auxiliary outputs are silent");
    // The parameter reached the processor in a zero-frame block that carried
    // the prepared auxiliary bus (the processor validates it).
    assert_eq!(processor.processor().blocks, [4, 0]);
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
    // Auxiliary buses.
    let valid = [
        AuxiliaryBus::sidechain("Sidechain", 2),
        AuxiliaryBus::output("Direct", 2),
        AuxiliaryBus::new("Both", 1, 1),
    ];
    let resolved = product.with_auxiliary_buses(&valid).resolve().unwrap();
    assert_eq!(
        resolved.product().auxiliary_layouts(),
        [
            AudioLayout::new(2, 0).unwrap(),
            AudioLayout::new(0, 2).unwrap(),
            AudioLayout::MONO
        ]
    );
    assert!(valid[0].is_sidechain() && !valid[1].is_sidechain() && !valid[2].is_sidechain());
    let long = "x".repeat(MAXIMUM_BUS_NAME_BYTES + 1);
    for (index, buses) in [
        (0, vec![AuxiliaryBus::new("Empty", 0, 0)]),
        (0, vec![AuxiliaryBus::sidechain("Wide", 65)]),
        (0, vec![AuxiliaryBus::sidechain(" ", 1)]),
        (0, vec![AuxiliaryBus::sidechain("Tab\t", 1)]),
        (0, vec![AuxiliaryBus::sidechain(&long, 1)]),
        (
            1,
            vec![
                AuxiliaryBus::sidechain("Same", 1),
                AuxiliaryBus::output("Same", 1),
            ],
        ),
    ] {
        assert_eq!(
            product.with_auxiliary_buses(&buses).resolve().unwrap_err(),
            ProductError::InvalidAuxiliaryBus(index),
            "{buses:?}"
        );
    }
    let nine: Vec<_> = ["a", "b", "c", "d", "e", "f", "g", "h", "i"]
        .iter()
        .map(|name| AuxiliaryBus::sidechain(name, 1))
        .collect();
    assert_eq!(
        product.with_auxiliary_buses(&nine).resolve().unwrap_err(),
        ProductError::TooManyAuxiliaryBuses
    );
    assert!(product.with_auxiliary_buses(&nine[..8]).resolve().is_ok());
    let latency = product
        .with_parameters(WITH)
        .with_bypass(BypassPolicy::Hard);
    assert!(
        latency
            .with_maximum_latency(MAXIMUM_BYPASS_LATENCY)
            .resolve()
            .is_ok()
    );
    assert_eq!(
        latency
            .with_maximum_latency(MAXIMUM_BYPASS_LATENCY + 1)
            .resolve()
            .unwrap_err(),
        ProductError::InvalidBypassPolicy
    );
    let resolved = latency.with_maximum_latency(7).resolve().unwrap();
    assert_eq!(
        PolicyProcessor::new(Probe::new(), &resolved).maximum_latency,
        7
    );
    assert!(
        product
            .with_parameters(WITH)
            .with_maximum_latency(u32::MAX)
            .resolve()
            .is_ok(),
        "unused without a bypass policy"
    );
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
    for ((automation, bypass), latency) in policies
        .into_iter()
        .flat_map(|policies| [(policies, 0), (policies, 37)])
    {
        let mut processor = prepared(automation, bypass);
        processor.processor_mut().latency = latency;
        processor = processor.with_maximum_latency(latency);
        processor
            .prepare(ProcessConfig::new(RATE, 512, 64, AudioLayout::STEREO).unwrap())
            .unwrap();
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
        assert_eq!(
            measured.count_total, 0,
            "{automation:?} {bypass:?} latency {latency}"
        );
    }
}
