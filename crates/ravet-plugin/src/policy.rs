//! Automation and bypass policies, applied once for every export format.
//!
//! A product chooses them with [`Product::with_automation`] and
//! [`Product::with_bypass`]. Every format adapter (CLAP and the native table
//! behind VST3, AUv2, AAX and LV2) wraps the product's processor in a
//! [`PolicyProcessor`], so the policies behave identically in every format.
//!
//! The dry signal used by [`BypassPolicy::Hard`], [`BypassPolicy::Crossfade`]
//! and [`BypassPolicy::Frozen`] is the block's input **delayed by the
//! processor's reported latency** ([`Processor::latency_samples`], read at the
//! start of every block), so it stays aligned with the processed signal and
//! with the host's delay compensation. The delay line is prepared for the
//! larger of the latency reported after `prepare` and
//! [`Product::with_maximum_latency`], at most [`MAXIMUM_BYPASS_LATENCY`]
//! samples; a larger reported latency is compensated only up to that
//! capacity until the next `prepare` (the export adapters request a host
//! restart when latency changes). A change of latency while active moves the
//! dry signal abruptly. A reset clears the delayed history.
//!
//! [`Product::with_automation`]: crate::Product::with_automation
//! [`Product::with_bypass`]: crate::Product::with_bypass
//! [`Product::with_maximum_latency`]: crate::Product::with_maximum_latency
use crate::ResolvedProduct;
use ravet_foundation::{
    AudioBuffer, Event, EventData, MAXIMUM_AUXILIARY_BUSES, OutputEvents, ParameterId,
    PrepareError, ProcessBlock, ProcessConfig, ProcessError, ProcessStatus, Processor, Tail,
};

/// Longest accepted [`BypassPolicy::Crossfade`] time in milliseconds.
pub const MAXIMUM_CROSSFADE_MS: f32 = 1000.0;
/// Most samples of latency the bypass dry delay compensates (65,536, about
/// 1.4 s at 48 kHz).
pub const MAXIMUM_BYPASS_LATENCY: u32 = 1 << 16;
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
/// delayed by the processor's latency (see the [module documentation](self)).
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
    /// Auxiliary outputs fade to silence.
    Crossfade { milliseconds: f32 },
    /// The processor keeps running; the output switches between the processed
    /// signal and the dry input at the event's offset, without a ramp.
    /// Auxiliary outputs are silent while bypassed.
    Hard,
    /// While bypassed the processor is not run, so its history is frozen, and
    /// the input, delayed by the processor's latency, is copied to the output.
    /// Without latency (and without a declared maximum) this needs no scratch
    /// storage ([`AudioBuffer::pass_through`]). Parameter events still reach the
    /// processor through a zero-frame block; notes and other events are
    /// discarded. The switch happens on block boundaries, using the last bypass
    /// value of the block. Auxiliary outputs are silent while bypassed.
    Frozen,
}

/// Wraps a processor in a product's [`AutomationPolicy`] and [`BypassPolicy`].
///
/// Preparation allocates event scratch, generated-event scratch, for
/// [`BypassPolicy::Hard`] and [`BypassPolicy::Crossfade`] one dry copy per
/// output channel plus a gain ramp, and for every policy except
/// [`BypassPolicy::Processor`] a dry delay line per output channel when the
/// processor has (or may have) latency; [`BypassPolicy::Frozen`] then also
/// keeps a dry copy per channel. Processing and reset never allocate. With
/// the default policies the wrapper forwards blocks unchanged.
pub struct PolicyProcessor<P> {
    processor: P,
    automation: AutomationPolicy,
    bypass: BypassPolicy,
    bypass_id: Option<ParameterId>,
    /// Declared maximum latency; sizes `delay` together with the prepared latency.
    maximum_latency: u32,
    config: Option<ProcessConfig>,
    /// Filtered events for the processor.
    events: Vec<Event>,
    /// Bypass changes of the current block: (frame, bypassed).
    switches: Vec<(usize, bool)>,
    /// Generated events of the inner block, copied to the caller's output.
    generated: Vec<Event>,
    /// Dry input copy per output channel (Hard and Crossfade; Frozen with a
    /// delay line).
    dry: Vec<Vec<f32>>,
    /// Latency-compensating history of the dry input per output channel.
    delay: DryDelay,
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
            .with_maximum_latency(product.product().maximum_latency())
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
            maximum_latency: 0,
            config: None,
            events: Vec::new(),
            switches: Vec::new(),
            generated: Vec::new(),
            dry: Vec::new(),
            delay: DryDelay::default(),
            gains: Vec::new(),
            bypassed: false,
            wet: 1.0,
            step: 1.0,
        }
    }
    /// Size the dry delay for at least `samples` of latency (clamped to
    /// [`MAXIMUM_BYPASS_LATENCY`]) from the next `prepare`; see
    /// [`Product::with_maximum_latency`](crate::Product::with_maximum_latency).
    pub fn with_maximum_latency(mut self, samples: u32) -> Self {
        self.maximum_latency = samples;
        self
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
    // A buffer without channels or frames always validates; matching keeps the
    // audio path free of panics.
    let Ok(empty) = AudioBuffer::separate(&[], &mut [], 0) else {
        block.clear_outputs();
        return ProcessStatus::Fault(ProcessError::LayoutMismatch);
    };
    let audio = std::mem::replace(&mut block.audio, empty);
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
            && (layout.input_channels() > MAXIMUM_CHANNELS
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
        self.processor.prepare(config)?;
        let capacity = match self.bypass {
            BypassPolicy::Processor => 0,
            _ => self
                .processor
                .latency_samples()
                .max(self.maximum_latency)
                .min(MAXIMUM_BYPASS_LATENCY) as usize,
        };
        self.delay = DryDelay::new(layout.output_channels(), capacity);
        let mixes = match self.bypass {
            BypassPolicy::Hard | BypassPolicy::Crossfade { .. } => true,
            BypassPolicy::Frozen => capacity > 0,
            BypassPolicy::Processor => false,
        };
        self.dry = if mixes {
            (0..layout.output_channels())
                .map(|_| vec![0.0; config.maximum_frames()])
                .collect()
        } else {
            Vec::new()
        };
        self.gains = if mixes && self.bypass != BypassPolicy::Frozen {
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
            BypassPolicy::Frozen => self.process_frozen(config, block, count, switches),
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
        self.delay.clear();
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
        config: ProcessConfig,
        block: &mut ProcessBlock<'_, '_, '_, f32>,
        count: usize,
        switches: usize,
    ) -> ProcessStatus {
        if let Some(&(_, bypassed)) = self.switches[..switches].last() {
            self.bypassed = bypassed;
        }
        let frames = block.audio.frames();
        if !self.bypassed {
            // Keep the delayed history current so that bypass continues it.
            self.delay.write(&block.audio);
            return run_inner(
                &mut self.processor,
                block,
                &self.events[..count],
                &mut self.generated,
            );
        }
        if let Some(buses) = block.auxiliary.as_deref_mut() {
            for bus in buses {
                bus.clear_outputs();
            }
        }
        if self.delay.is_empty() {
            block.audio.pass_through();
        } else {
            let latency = self.processor.latency_samples();
            self.delay.read(&block.audio, latency, &mut self.dry);
            for (channel, dry) in self.dry.iter().enumerate() {
                // One dry buffer per validated output channel, so this never skips.
                if let Some(output) = block.audio.output_mut(channel) {
                    output.copy_from_slice(&dry[..frames]);
                }
            }
        }
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
        let layout = config.layout();
        let inputs: [&[f32]; MAXIMUM_CHANNELS] = [&[]; MAXIMUM_CHANNELS];
        let mut outputs: [&mut [f32]; MAXIMUM_CHANNELS] = std::array::from_fn(|_| &mut [][..]);
        // `prepare` rejects Frozen layouts over MAXIMUM_CHANNELS, and zero-frame
        // channels always validate; matching keeps the audio path free of panics.
        let (Some(inputs), Some(outputs)) = (
            inputs.get(..layout.input_channels()),
            outputs.get_mut(..layout.output_channels()),
        ) else {
            block.clear_outputs();
            return ProcessStatus::Fault(ProcessError::LayoutMismatch);
        };
        let Ok(audio) = AudioBuffer::separate(inputs, outputs, 0) else {
            block.clear_outputs();
            return ProcessStatus::Fault(ProcessError::LayoutMismatch);
        };
        // Zero-frame auxiliary buses with the prepared layouts.
        const AUXILIARY_CHANNELS: usize = MAXIMUM_AUXILIARY_BUSES * MAXIMUM_CHANNELS;
        let auxiliary_layouts = config.auxiliary_buses();
        let auxiliary_inputs: [&[f32]; AUXILIARY_CHANNELS] = [&[]; AUXILIARY_CHANNELS];
        let mut auxiliary_outputs: [&mut [f32]; AUXILIARY_CHANNELS] =
            std::array::from_fn(|_| &mut [][..]);
        let mut remaining_inputs = &auxiliary_inputs[..];
        let mut remaining_outputs = &mut auxiliary_outputs[..];
        let mut auxiliary: [AudioBuffer<'_, '_, f32>; MAXIMUM_AUXILIARY_BUSES] =
            std::array::from_fn(|index| {
                let (ins, outs) = auxiliary_layouts
                    .get(index)
                    .map_or((0, 0), |b| (b.input_channels(), b.output_channels()));
                // Each layout has at most MAXIMUM_CHANNELS per direction, so the
                // arrays always hold every prepared bus.
                let (input, rest) = remaining_inputs.split_at(ins.min(remaining_inputs.len()));
                remaining_inputs = rest;
                let outs = outs.min(remaining_outputs.len());
                let (output, rest) = std::mem::take(&mut remaining_outputs).split_at_mut(outs);
                remaining_outputs = rest;
                // Zero-frame channels always validate.
                AudioBuffer::separate(input, output, 0).unwrap_or_else(|_| empty_buffer())
            });
        let mut inner = ProcessBlock {
            audio,
            auxiliary: (!auxiliary_layouts.is_empty())
                .then(|| &mut auxiliary[..auxiliary_layouts.len()]),
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
            // `validate_block` matched the channels to the prepared layout and
            // bounded `frames` by the prepared length of each `dry` buffer.
            let latency = self.processor.latency_samples();
            self.delay.read(&block.audio, latency, &mut self.dry);
        } else {
            self.delay.write(&block.audio);
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
            // One dry buffer per validated output channel, so this never skips.
            let Some(output) = block.audio.output_mut(channel) else {
                continue;
            };
            for ((sample, dry), gain) in output.iter_mut().zip(&dry[..frames]).zip(&self.gains) {
                *sample = *sample * gain + dry * (1.0 - gain);
            }
        }
        // Auxiliary outputs have no dry signal: they fade to silence.
        if let Some(buses) = block.auxiliary.as_deref_mut() {
            for bus in buses {
                for channel in 0..bus.output_channels() {
                    if let Some(output) = bus.output_mut(channel) {
                        for (sample, gain) in output.iter_mut().zip(&self.gains) {
                            *sample *= gain;
                        }
                    }
                }
            }
        }
        ProcessStatus::Continue
    }
}

/// A buffer without channels or frames.
fn empty_buffer<'c, 's>() -> AudioBuffer<'c, 's, f32> {
    match AudioBuffer::separate(&[], &mut [], 0) {
        Ok(buffer) => buffer,
        // A buffer without channels or frames always validates.
        Err(_) => unreachable!("an empty buffer is valid"),
    }
}

/// Input channel feeding dry output `channel`: the same input, the last input
/// when there are fewer inputs, or `None` (silence) without inputs.
fn dry_input<'b>(audio: &'b AudioBuffer<'_, '_, f32>, channel: usize) -> Option<&'b [f32]> {
    audio
        .input_channels()
        .checked_sub(1)
        .and_then(|last| audio.input(channel.min(last)))
}

/// One ring of past dry input per output channel, all sharing a write
/// position. Prepared with a fixed capacity; never allocates afterwards.
#[derive(Default)]
struct DryDelay {
    lines: Vec<Vec<f32>>,
    /// Next write index into every line.
    position: usize,
}

impl DryDelay {
    fn new(channels: usize, capacity: usize) -> Self {
        Self {
            lines: if capacity == 0 {
                Vec::new()
            } else {
                (0..channels).map(|_| vec![0.0; capacity]).collect()
            },
            position: 0,
        }
    }
    fn capacity(&self) -> usize {
        self.lines.first().map_or(0, Vec::len)
    }
    fn is_empty(&self) -> bool {
        self.capacity() == 0
    }
    fn clear(&mut self) {
        for line in &mut self.lines {
            line.fill(0.0);
        }
        self.position = 0;
    }
    /// Record the block's dry input without producing output.
    fn write(&mut self, audio: &AudioBuffer<'_, '_, f32>) {
        let capacity = self.capacity();
        let frames = audio.frames();
        if capacity == 0 || frames == 0 {
            return;
        }
        // Only the newest `capacity` frames can still be read.
        let skip = frames.saturating_sub(capacity);
        let start = (self.position + skip) % capacity;
        let kept = frames - skip;
        let first = (capacity - start).min(kept);
        for (channel, line) in self.lines.iter_mut().enumerate() {
            match dry_input(audio, channel) {
                Some(input) => {
                    let input = &input[skip..];
                    line[start..start + first].copy_from_slice(&input[..first]);
                    line[..kept - first].copy_from_slice(&input[first..]);
                }
                None => {
                    line[start..start + first].fill(0.0);
                    line[..kept - first].fill(0.0);
                }
            }
        }
        self.position = (self.position + frames) % capacity;
    }
    /// Fill `dry[channel][..frames]` with the block's dry input delayed by
    /// `latency` samples (at most the capacity), recording the input.
    fn read(&mut self, audio: &AudioBuffer<'_, '_, f32>, latency: u32, dry: &mut [Vec<f32>]) {
        let frames = audio.frames();
        let capacity = self.capacity();
        let delay = (latency as usize).min(capacity);
        for (channel, dry) in dry.iter_mut().enumerate() {
            let input = dry_input(audio, channel);
            let dry = &mut dry[..frames];
            let Some(line) = self.lines.get_mut(channel).filter(|_| delay > 0) else {
                match input {
                    Some(input) => dry.copy_from_slice(input),
                    None => dry.fill(0.0),
                }
                continue;
            };
            let mut write = self.position;
            // `delay` is 1..=capacity, so the read index trails the write index
            // by `delay` and equals it (oldest sample) when `delay == capacity`.
            let mut read = (write + capacity - delay) % capacity;
            for (frame, out) in dry.iter_mut().enumerate() {
                *out = line[read];
                line[write] = input.map_or(0.0, |input| input[frame]);
                write = if write + 1 == capacity { 0 } else { write + 1 };
                read = if read + 1 == capacity { 0 } else { read + 1 };
            }
        }
        if delay == 0 {
            // Nothing was recorded above; keep the history current.
            self.write(audio);
        } else {
            self.position = (self.position + frames) % capacity;
        }
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
