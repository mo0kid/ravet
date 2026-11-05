//! Automation and bypass policies, applied once for every export format.
//!
//! A product chooses them with [`Product::with_automation`] and
//! [`Product::with_bypass`]. Every format adapter (CLAP and the native table
//! behind VST3, AUv2, AAX and LV2) wraps the product's processor in a
//! [`PolicyProcessor`], so the policies behave identically in every format.
//!
//! The dry signal used by [`BypassPolicy::Hard`], [`BypassPolicy::Crossfade`]
//! and [`BypassPolicy::Frozen`] is the block's input, **not delayed by the
//! processor's latency**. A product that reports latency and needs a
//! latency-compensated bypass must keep [`BypassPolicy::Processor`] and
//! implement bypass itself.
//!
//! [`Product::with_automation`]: crate::Product::with_automation
//! [`Product::with_bypass`]: crate::Product::with_bypass
use crate::ResolvedProduct;
use ravet_foundation::{
    AudioBuffer, Event, EventData, OutputEvents, ParameterId, PrepareError, ProcessBlock,
    ProcessConfig, ProcessError, ProcessStatus, Processor, Tail,
};

/// Longest accepted [`BypassPolicy::Crossfade`] time in milliseconds.
pub const MAXIMUM_CROSSFADE_MS: f32 = 1000.0;
/// Most channels a layout can have; bounds the zero-frame block used by
/// [`BypassPolicy::Frozen`].
const MAXIMUM_CHANNELS: usize = 64;

/// When parameter changes take effect within a block.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum AutomationPolicy {
    /// Parameter events keep the host's frame offsets.
    #[default]
    SampleAccurate,
    /// Every parameter event (values, modulation and gestures, global or
    /// scoped) moves to offset 0, ahead of the block's other events, in its
    /// original order. For processors that read parameters once per block.
    PerBlock,
}

/// How the host bypass parameter ([`Parameter::bypass`]) is applied.
///
/// Every policy except [`BypassPolicy::Processor`] consumes the bypass
/// parameter: the processor never receives events for it, and
/// [`Product::resolve`] requires the product to declare one. The dry signal is
/// not latency-compensated (see the [module documentation](self)).
///
/// [`Parameter::bypass`]: crate::Parameter::bypass
/// [`Product::resolve`]: crate::Product::resolve
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub enum BypassPolicy {
    /// The processor receives the bypass parameter like any other and
    /// implements bypass itself. The default.
    #[default]
    Processor,
    /// The processor keeps running; the output crossfades linearly between the
    /// processed signal and the dry input over the given time (greater than 0,
    /// at most [`MAXIMUM_CROSSFADE_MS`]). Changes start at the event's offset.
    Crossfade { milliseconds: f32 },
    /// The processor keeps running; the output switches between the processed
    /// signal and the dry input at the event's offset, without a ramp.
    Hard,
    /// While bypassed the processor is not run, so its history is frozen, and
    /// the input is copied to the output without scratch storage
    /// ([`AudioBuffer::pass_through`]). Parameter events still reach the
    /// processor through a zero-frame block; notes and other events are
    /// discarded. The switch happens on block boundaries, using the last bypass
    /// value of the block. Not available with auxiliary buses.
    Frozen,
}

/// Wraps a processor in a product's [`AutomationPolicy`] and [`BypassPolicy`].
///
/// Preparation allocates event scratch, generated-event scratch, and for
/// [`BypassPolicy::Hard`] and [`BypassPolicy::Crossfade`] one dry copy per
/// output channel plus a gain ramp. Processing and reset never allocate. With
/// the default policies the wrapper forwards blocks unchanged.
pub struct PolicyProcessor<P> {
    processor: P,
    automation: AutomationPolicy,
    bypass: BypassPolicy,
    bypass_id: Option<ParameterId>,
    config: Option<ProcessConfig>,
    /// Filtered events for the processor.
    events: Vec<Event>,
    /// Bypass changes of the current block: (frame, bypassed).
    switches: Vec<(usize, bool)>,
    /// Generated events of the inner block, copied to the caller's output.
    generated: Vec<Event>,
    /// Dry input copy per output channel (Hard and Crossfade only).
    dry: Vec<Vec<f32>>,
    /// Wet gain per frame of the current block (Hard and Crossfade only).
    gains: Vec<f32>,
    /// The bypass state requested by the latest bypass event.
    bypassed: bool,
    /// Current wet gain: 1 processed, 0 dry.
    wet: f32,
    /// Wet gain change per sample: 1 for Hard, 1 / ramp length for Crossfade.
    step: f32,
}

impl<P> PolicyProcessor<P> {
    /// Wrap `processor` in the policies of `product`.
    pub fn new(processor: P, product: &ResolvedProduct<'_>) -> Self {
        let bypass_id = product
            .bypass_index()
            .map(|index| product.parameter_ids()[index]);
        let bypass = match bypass_id {
            Some(_) => product.product().bypass_policy(),
            None => BypassPolicy::Processor,
        };
        Self::with_policies(processor, product.product().automation(), bypass, bypass_id)
    }
    /// Wrap `processor` with explicit policies. `bypass_id` is the bypass
    /// parameter; without one, the bypass policy falls back to
    /// [`BypassPolicy::Processor`].
    pub fn with_policies(
        processor: P,
        automation: AutomationPolicy,
        bypass: BypassPolicy,
        bypass_id: Option<ParameterId>,
    ) -> Self {
        Self {
            processor,
            automation,
            bypass: if bypass_id.is_some() {
                bypass
            } else {
                BypassPolicy::Processor
            },
            bypass_id,
            config: None,
            events: Vec::new(),
            switches: Vec::new(),
            generated: Vec::new(),
            dry: Vec::new(),
            gains: Vec::new(),
            bypassed: false,
            wet: 1.0,
            step: 1.0,
        }
    }
    pub fn processor(&self) -> &P {
        &self.processor
    }
    pub fn processor_mut(&mut self) -> &mut P {
        &mut self.processor
    }
    /// Whether the latest bypass event requested bypass. Always false for
    /// [`BypassPolicy::Processor`].
    pub fn is_bypassed(&self) -> bool {
        self.bypassed
    }
    fn consumes(&self, data: &EventData) -> Option<Option<bool>> {
        let id = self
            .bypass_id
            .filter(|_| self.bypass != BypassPolicy::Processor)?;
        match *data {
            EventData::ParameterValue { id: target, value } if target == id => {
                Some(Some(value >= 0.5))
            }
            EventData::ParameterModulation { id: target, .. }
            | EventData::ParameterGesture { id: target, .. }
            | EventData::ParameterValueScoped { id: target, .. }
            | EventData::ParameterModulationScoped { id: target, .. }
                if target == id =>
            {
                Some(None)
            }
            _ => None,
        }
    }
    /// Fill `events` and `switches` from the block's events; returns their counts.
    fn filter(&mut self, input: &[Event]) -> (usize, usize) {
        let per_block = self.automation == AutomationPolicy::PerBlock;
        let (mut count, mut switches) = (0, 0);
        for event in input {
            match self.consumes(&event.data) {
                Some(Some(bypassed)) => {
                    let frame = if per_block { 0 } else { event.frame_offset };
                    self.switches[switches] = (frame, bypassed);
                    switches += 1;
                }
                Some(None) => {}
                None if per_block && is_parameter(&event.data) => {
                    self.events[count] = Event {
                        frame_offset: 0,
                        data: event.data,
                    };
                    count += 1;
                }
                None if !per_block => {
                    self.events[count] = *event;
                    count += 1;
                }
                None => {}
            }
        }
        if per_block {
            for event in input {
                if self.consumes(&event.data).is_none() && !is_parameter(&event.data) {
                    self.events[count] = *event;
                    count += 1;
                }
            }
        }
        (count, switches)
    }
}

fn is_parameter(data: &EventData) -> bool {
    matches!(
        data,
        EventData::ParameterValue { .. }
            | EventData::ParameterModulation { .. }
            | EventData::ParameterGesture { .. }
            | EventData::ParameterValueScoped { .. }
            | EventData::ParameterModulationScoped { .. }
    )
}

/// Run `processor` on the caller's audio with `events`, collecting generated
/// events in `generated` and copying them to the caller's output.
fn run_inner<P: Processor<f32>>(
    processor: &mut P,
    block: &mut ProcessBlock<'_, '_, '_, f32>,
    events: &[Event],
    generated: &mut [Event],
) -> ProcessStatus {
    let frames = block.audio.frames();
    let audio = std::mem::replace(
        &mut block.audio,
        AudioBuffer::separate(&[], &mut [], 0).expect("empty buffer"),
    );
    let mut inner = ProcessBlock {
        audio,
        auxiliary: block.auxiliary.take(),
        output_events: block
            .output_events
            .is_some()
            .then(|| OutputEvents::new(generated, frames)),
        events,
        transport: block.transport,
    };
    let status = processor.process(&mut inner);
    block.audio = inner.audio;
    block.auxiliary = inner.auxiliary;
    copy_generated(block, inner.output_events.as_ref());
    status
}

fn copy_generated(block: &mut ProcessBlock<'_, '_, '_, f32>, generated: Option<&OutputEvents>) {
    if let (Some(output), Some(generated)) = (block.output_events.as_mut(), generated) {
        for event in generated.as_slice() {
            // Same capacity and frames; zero-frame events sit at offset 0.
            let _ = output.push(*event);
        }
    }
}

impl<P: Processor<f32>> Processor<f32> for PolicyProcessor<P> {
    fn prepare(&mut self, config: ProcessConfig) -> Result<(), PrepareError> {
        self.config = None;
        let layout = config.layout();
        if self.bypass == BypassPolicy::Frozen
            && (!config.auxiliary_buses().is_empty()
                || layout.input_channels() > MAXIMUM_CHANNELS
                || layout.output_channels() > MAXIMUM_CHANNELS)
        {
            return Err(PrepareError::UnsupportedLayout);
        }
        let empty = Event {
            frame_offset: 0,
            data: EventData::ParameterValue {
                id: ParameterId(0),
                value: 0.0,
            },
        };
        self.events = vec![empty; config.maximum_events()];
        self.switches = vec![(0, false); config.maximum_events()];
        self.generated = vec![empty; config.maximum_output_events()];
        let mixes = matches!(
            self.bypass,
            BypassPolicy::Hard | BypassPolicy::Crossfade { .. }
        );
        self.dry = if mixes {
            (0..layout.output_channels())
                .map(|_| vec![0.0; config.maximum_frames()])
                .collect()
        } else {
            Vec::new()
        };
        self.gains = if mixes {
            vec![1.0; config.maximum_frames()]
        } else {
            Vec::new()
        };
        self.step = match self.bypass {
            BypassPolicy::Crossfade { milliseconds } => {
                let frames = f64::from(milliseconds) * config.sample_rate() / 1000.0;
                (1.0 / frames.max(1.0)) as f32
            }
            _ => 1.0,
        };
        self.bypassed = false;
        self.wet = 1.0;
        self.processor.prepare(config)?;
        self.config = Some(config);
        Ok(())
    }
    fn start_audio(&mut self) -> Result<(), ProcessError> {
        self.processor.start_audio()
    }
    fn stop_audio(&mut self) {
        self.processor.stop_audio();
    }
    fn process(&mut self, block: &mut ProcessBlock<'_, '_, '_, f32>) -> ProcessStatus {
        let Some(config) = self.config else {
            block.clear_outputs();
            return ProcessStatus::Fault(ProcessError::NotPrepared);
        };
        if self.bypass == BypassPolicy::Processor
            && self.automation == AutomationPolicy::SampleAccurate
        {
            return self.processor.process(block);
        }
        if let Err(error) = config.validate_block(block) {
            block.clear_outputs();
            return ProcessStatus::Fault(error);
        }
        let (count, switches) = self.filter(block.events);
        match self.bypass {
            BypassPolicy::Processor => run_inner(
                &mut self.processor,
                block,
                &self.events[..count],
                &mut self.generated,
            ),
            BypassPolicy::Frozen => self.process_frozen(block, count, switches),
            BypassPolicy::Hard | BypassPolicy::Crossfade { .. } => {
                self.process_mixed(block, count, switches)
            }
        }
    }
    fn reset(&mut self) {
        let _ = self.try_reset();
    }
    fn try_reset(&mut self) -> Result<(), ProcessError> {
        self.wet = if self.bypassed { 0.0 } else { 1.0 };
        self.processor.try_reset()
    }
    fn deactivate(&mut self) {
        self.config = None;
        self.processor.deactivate();
    }
    fn latency_samples(&self) -> u32 {
        self.processor.latency_samples()
    }
    fn tail(&self) -> Tail {
        self.processor.tail()
    }
}

impl<P: Processor<f32>> PolicyProcessor<P> {
    fn process_frozen(
        &mut self,
        block: &mut ProcessBlock<'_, '_, '_, f32>,
        count: usize,
        switches: usize,
    ) -> ProcessStatus {
        if let Some(&(_, bypassed)) = self.switches[..switches].last() {
            self.bypassed = bypassed;
        }
        if !self.bypassed {
            return run_inner(
                &mut self.processor,
                block,
                &self.events[..count],
                &mut self.generated,
            );
        }
        block.audio.pass_through();
        // Keep only parameter events, at offset 0, for a zero-frame block.
        let mut parameters = 0;
        for index in 0..count {
            let event = self.events[index];
            if is_parameter(&event.data) {
                self.events[parameters] = Event {
                    frame_offset: 0,
                    data: event.data,
                };
                parameters += 1;
            }
        }
        if parameters == 0 {
            return ProcessStatus::Continue;
        }
        let layout = self.config.expect("prepared").layout();
        let inputs: [&[f32]; MAXIMUM_CHANNELS] = [&[]; MAXIMUM_CHANNELS];
        let mut outputs: [&mut [f32]; MAXIMUM_CHANNELS] = std::array::from_fn(|_| &mut [][..]);
        let audio = AudioBuffer::separate(
            &inputs[..layout.input_channels()],
            &mut outputs[..layout.output_channels()],
            0,
        )
        .expect("empty channels");
        let mut inner = ProcessBlock {
            audio,
            auxiliary: None,
            output_events: block
                .output_events
                .is_some()
                .then(|| OutputEvents::new(&mut self.generated, 0)),
            events: &self.events[..parameters],
            transport: block.transport,
        };
        let status = self.processor.process(&mut inner);
        copy_generated(block, inner.output_events.as_ref());
        match status {
            ProcessStatus::Fault(_) => {
                block.clear_outputs();
                status
            }
            _ => ProcessStatus::Continue,
        }
    }

    fn process_mixed(
        &mut self,
        block: &mut ProcessBlock<'_, '_, '_, f32>,
        count: usize,
        switches: usize,
    ) -> ProcessStatus {
        let frames = block.audio.frames();
        let mix = self.bypassed || self.wet != 1.0 || switches > 0;
        if mix && frames > 0 {
            let inputs = block.audio.input_channels();
            for (channel, dry) in self.dry.iter_mut().enumerate() {
                match inputs.checked_sub(1) {
                    Some(last) => dry[..frames]
                        .copy_from_slice(block.audio.input(channel.min(last)).expect("input")),
                    None => dry[..frames].fill(0.0),
                }
            }
        }
        let status = run_inner(
            &mut self.processor,
            block,
            &self.events[..count],
            &mut self.generated,
        );
        if matches!(status, ProcessStatus::Fault(_)) {
            block.clear_outputs();
            return status;
        }
        if !mix {
            return status;
        }
        let mut next = 0;
        for (frame, gain) in self.gains[..frames].iter_mut().enumerate() {
            while next < switches && self.switches[next].0 <= frame {
                self.bypassed = self.switches[next].1;
                next += 1;
            }
            let target = if self.bypassed { 0.0 } else { 1.0 };
            self.wet = if self.wet < target {
                (self.wet + self.step).min(target)
            } else {
                (self.wet - self.step).max(target)
            };
            *gain = self.wet;
        }
        // Zero-frame blocks: later blocks ramp towards the latest request.
        for &(_, bypassed) in &self.switches[next..switches] {
            self.bypassed = bypassed;
        }
        for (channel, dry) in self.dry.iter().enumerate() {
            let output = block.audio.output_mut(channel).expect("output");
            for ((sample, dry), gain) in output.iter_mut().zip(&dry[..frames]).zip(&self.gains) {
                *sample = *sample * gain + dry * (1.0 - gain);
            }
        }
        ProcessStatus::Continue
    }
}

/// The processor every export runs: the product's processor inside its
/// policies, inside denormal flushing ([`DenormalMode::FlushBoth`]: denormal
/// inputs and results are flushed to zero for the duration of each block, and
/// the previous floating-point mode is restored afterwards).
///
/// [`DenormalMode::FlushBoth`]: ravet_native_dsp::DenormalMode::FlushBoth
#[cfg(any(feature = "clap", feature = "native"))]
pub(crate) type Wrapped<P> = ravet_native_dsp::DenormalProcessor<PolicyProcessor<P>>;

/// Wrap a product processor for export; `None` when the platform cannot
/// control denormal handling.
#[cfg(any(feature = "clap", feature = "native"))]
pub(crate) fn wrap<P: Processor<f32>>(
    processor: P,
    product: &ResolvedProduct<'_>,
) -> Option<Wrapped<P>> {
    ravet_native_dsp::DenormalProcessor::new(
        PolicyProcessor::new(processor, product),
        ravet_native_dsp::DenormalMode::FlushBoth,
    )
    .ok()
}

#[cfg(test)]
mod tests;
