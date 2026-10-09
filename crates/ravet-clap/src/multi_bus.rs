//! Compact native CLAP port mapping for ordered portable buses.
use crate::{
    BridgeError, BridgeReport, ClapSample, MAXIMUM_EVENTS, MAXIMUM_FRAMES, ParameterEvents,
    silence, translate_transport,
};
use clack_plugin::{
    events::{
        event_types::TransportEvent,
        io::{InputEvents, OutputEvents},
    },
    process::Audio,
};
use ravet_foundation::{
    AudioBuffer, Event, EventData, MAXIMUM_AUXILIARY_BUSES, MAXIMUM_CHANNELS, ParameterId,
    ProcessBlock, ProcessConfig, ProcessStatus, Processor, Transport,
};
const CHANNELS: usize = (MAXIMUM_AUXILIARY_BUSES + 1) * MAXIMUM_CHANNELS;
const SCRATCH_BYTES: usize = 64 * 1024 * 1024;

/// Native f32/f64 main bus plus up to eight ordered auxiliary buses.
/// Native input and output port numbering each starts at zero, visits the main
/// bus then auxiliaries in declaration order, and skips buses with zero channels
/// in that direction. Exporters must advertise this same mapping and precision.
/// All input samples are copied before any output write, preserving cross-bus
/// host aliases. Both-precision buffers reject; there is no conversion.
///
/// Preparation bounds frames to 65,536, each input/output event capacity to
/// 1,024 and combined audio/event scratch to 64 MiB. It allocates; processing,
/// parameter-only flush and reset avoid heap operations, locks and I/O. Invalid
/// host input rejects before DSP progression and silences every output port.
/// Processor faults may follow internal progression; output event delivery has
/// the same prefix/refusal contract as [`crate::MainBusBridge`]. No exporter or
/// DAW auxiliary-bus compatibility is implied by constructing this adapter.
pub struct MultiBusBridge<P, S: ClapSample = f32> {
    processor: P,
    config: ProcessConfig,
    inputs: Vec<Vec<S>>,
    outputs: Vec<Vec<S>>,
    events: ParameterEvents,
    generated: Vec<Event>,
    midi_port: Option<u16>,
    output_midi_port: Option<u16>,
    output_clap_note_port: Option<u16>,
    scoped_parameter_output_port: Option<u16>,
    note_end_output_port: Option<u16>,
    previous: Option<(Transport, usize)>,
}
impl<P: Processor<S>, S: ClapSample> MultiBusBridge<P, S> {
    /// Prepare the complete portable configuration. MIDI ports are selected
    /// independently of audio ports; absent input maps only global parameters.
    pub fn prepare(
        mut processor: P,
        config: ProcessConfig,
        midi_port: Option<u16>,
        output_midi_port: Option<u16>,
    ) -> Result<Self, BridgeError> {
        if config.maximum_frames() > MAXIMUM_FRAMES {
            return Err(BridgeError::TooManyFrames);
        }
        if config.maximum_events() > MAXIMUM_EVENTS
            || config.maximum_output_events() > MAXIMUM_EVENTS
        {
            return Err(BridgeError::TooManyEvents);
        }
        let layouts =
            std::iter::once(config.layout()).chain(config.auxiliary_buses().iter().copied());
        let (ins, outs) = layouts.fold((0, 0), |(i, o), bus| {
            (i + bus.input_channels(), o + bus.output_channels())
        });
        let bytes = (ins + outs) * config.maximum_frames() * size_of::<S>()
            + (config.maximum_events() + config.maximum_output_events()) * size_of::<Event>();
        if bytes > SCRATCH_BYTES {
            return Err(BridgeError::ScratchCapacity);
        }
        processor.prepare(config).map_err(BridgeError::Prepare)?;
        Ok(Self {
            processor,
            config,
            inputs: (0..ins)
                .map(|_| vec![S::default(); config.maximum_frames()])
                .collect(),
            outputs: (0..outs)
                .map(|_| vec![S::default(); config.maximum_frames()])
                .collect(),
            events: ParameterEvents::with_capacity(config.maximum_events())?,
            generated: vec![
                Event {
                    frame_offset: 0,
                    data: EventData::ParameterValue {
                        id: ParameterId(0),
                        value: 0.
                    }
                };
                config.maximum_output_events()
            ],
            midi_port,
            output_midi_port,
            output_clap_note_port: None,
            scoped_parameter_output_port: None,
            note_end_output_port: None,
            previous: None,
        })
    }
    /// Select explicit native CLAP note/expression input (0..256), independent
    /// of raw MIDI/UMP selection. Requires exporter-advertised processor support;
    /// no voice behavior/metadata is inferred. None restores default filtering.
    pub fn with_clap_note_input_port(mut self, port: Option<u16>) -> Result<Self, BridgeError> {
        self.events = self.events.with_clap_note_input_port(port)?;
        Ok(self)
    }
    /// Select the originating note INPUT endpoint for generated completion
    /// notifications (0..256), independently of performance output and scoped
    /// parameters. Native timestamp/velocity carry no termination semantics.
    /// IDs must fit nonnegative i32; missing routes reject whole delivery. Flush
    /// rejects completion output. No voice tracking or metadata is inferred.
    pub fn with_note_end_output_port(mut self, port: Option<u16>) -> Result<Self, BridgeError> {
        crate::notes::validate_port(port)?;
        self.note_end_output_port = port;
        Ok(self)
    }
    /// Select native CLAP generated On/Off, identified notes and expressions
    /// (0..256). Legacy notes retain unquantized f32 velocity; IDs must fit i32.
    /// Raw MIDI/UMP still use their independently selected port. Whole delivery
    /// preflight prevents identity/scope loss. Native performance in parameter
    /// flush is unrepresentable; audio/DSP already progressed before delivery.
    pub fn with_clap_note_output_port(mut self, port: Option<u16>) -> Result<Self, BridgeError> {
        crate::notes::validate_port(port)?;
        self.output_clap_note_port = port;
        Ok(self)
    }
    /// Select scoped physical values/modulation on a note input endpoint below
    /// 256. Independent of note translation and global modulation; no metadata,
    /// parameter range/state/voice behavior is inferred. None restores filtering.
    pub fn with_scoped_parameter_input_port(
        mut self,
        port: Option<u16>,
    ) -> Result<Self, BridgeError> {
        self.events = self.events.with_scoped_parameter_input_port(port)?;
        Ok(self)
    }
    /// Select the originating note input endpoint for generated scoped physical
    /// parameter values/modulation (0..256), including zero-time parameter flush.
    /// This index is independent of note output and raw MIDI/UMP selectors. Full
    /// finite f64 values/scope are retained; native IDs must fit nonnegative i32.
    pub fn with_scoped_parameter_output_port(
        mut self,
        port: Option<u16>,
    ) -> Result<Self, BridgeError> {
        crate::notes::validate_port(port)?;
        self.scoped_parameter_output_port = port;
        Ok(self)
    }
    /// Enable raw parameter begin/end input; no automatic pairing/state edits.
    pub fn with_parameter_gesture_input(mut self, enabled: bool) -> Self {
        self.events = self.events.with_parameter_gesture_input(enabled);
        self
    }
    /// Opt in to global parameter modulation input without changing metadata or
    /// saved values. The processor must explicitly implement modulation semantics;
    /// scoped events require their separate selector. Default is disabled, including flush.
    pub fn with_global_parameter_modulation(mut self, enabled: bool) -> Self {
        self.events = self.events.with_global_parameter_modulation(enabled);
        self
    }
    pub fn processor(&self) -> &P {
        &self.processor
    }
    pub fn reset(&mut self) {
        self.processor.reset();
        self.previous = None;
    }
    pub fn deactivate(&mut self) {
        self.processor.deactivate();
        self.previous = None;
    }
    /// Apply global parameter events at zero time across all prepared buses.
    pub fn flush(
        &mut self,
        input: &InputEvents<'_>,
        output: &mut OutputEvents<'_>,
    ) -> Result<BridgeReport, BridgeError> {
        self.events.translate(input, 0, true)?;
        let (status, count) = self.run(0, Transport::default())?;
        Ok(BridgeReport {
            status,
            delivery: crate::output::deliver(
                &self.generated[..count],
                self.output_midi_port,
                self.output_clap_note_port,
                self.scoped_parameter_output_port,
                self.note_end_output_port,
                true,
                output,
            ),
        })
    }
    /// Process the whole native block; a rejected block silences all native outputs.
    pub fn process(
        &mut self,
        audio: &mut Audio<'_>,
        events: &InputEvents<'_>,
        transport: Option<&TransportEvent>,
        output: &mut OutputEvents<'_>,
    ) -> Result<BridgeReport, BridgeError> {
        let result = self.process_inner(audio, events, transport, output);
        if result.is_err() {
            silence(audio);
        }
        result
    }
    fn process_inner(
        &mut self,
        audio: &mut Audio<'_>,
        events: &InputEvents<'_>,
        transport: Option<&TransportEvent>,
        output: &mut OutputEvents<'_>,
    ) -> Result<BridgeReport, BridgeError> {
        let frames = audio.frames_count() as usize;
        if frames > self.config.maximum_frames() {
            return Err(BridgeError::TooManyFrames);
        }
        let layouts = || {
            std::iter::once(self.config.layout())
                .chain(self.config.auxiliary_buses().iter().copied())
        };
        if audio.input_port_count() != layouts().filter(|b| b.input_channels() != 0).count()
            || audio.output_port_count() != layouts().filter(|b| b.output_channels() != 0).count()
        {
            return Err(BridgeError::LayoutMismatch);
        }
        if let Some(port) = self.midi_port {
            self.events.translate_midi_ump(events, frames, port)?;
        } else {
            self.events.translate(events, frames, false)?;
        }
        let mut context = translate_transport(transport, self.config.sample_rate())?;
        if let (Some((previous, count)), Some(position)) = (self.previous, context.sample_position)
            && let Some(start) = previous.sample_position
        {
            let advance = if previous.playing == Some(true) {
                count as i64
            } else {
                0
            };
            context.discontinuity = start
                .checked_add(advance)
                .is_none_or(|expected| position.abs_diff(expected) > 1);
        }
        let mut input_offset = 0;
        let mut input_port = 0;
        let mut output_port = 0;
        for bus in layouts() {
            let count = bus.input_channels();
            if count != 0 {
                S::read_port(
                    audio,
                    input_port,
                    &mut self.inputs[input_offset..input_offset + count],
                    frames,
                )?;
                input_port += 1;
            }
            input_offset += count;
            if bus.output_channels() != 0 {
                S::validate_port(audio, output_port, bus.output_channels())?;
                output_port += 1;
            }
        }
        let (status, count) = self.run(frames, context)?;
        let mut offset = 0;
        let mut port = 0;
        for bus in std::iter::once(self.config.layout())
            .chain(self.config.auxiliary_buses().iter().copied())
        {
            let channels = bus.output_channels();
            if channels != 0 {
                S::write_port(
                    audio,
                    port,
                    &self.outputs[offset..offset + channels],
                    frames,
                )?;
                port += 1;
            }
            offset += channels;
        }
        self.previous = transport.map(|_| (context, frames));
        Ok(BridgeReport {
            status,
            delivery: crate::output::deliver(
                &self.generated[..count],
                self.output_midi_port,
                self.output_clap_note_port,
                self.scoped_parameter_output_port,
                self.note_end_output_port,
                false,
                output,
            ),
        })
    }
    fn run(
        &mut self,
        frames: usize,
        transport: Transport,
    ) -> Result<(ProcessStatus, usize), BridgeError> {
        let mut input_iter = self.inputs.iter();
        let inputs: [&[S]; CHANNELS] =
            std::array::from_fn(|_| input_iter.next().map_or(&[] as &[S], |v| &v[..frames]));
        let mut output_iter = self.outputs.iter_mut();
        let mut outputs: [&mut [S]; CHANNELS] = std::array::from_fn(|_| {
            output_iter.next().map_or(&mut [] as &mut [S], |v| {
                v[..frames].fill(S::default());
                &mut v[..frames]
            })
        });
        let layout = self.config.layout();
        let (main_outputs, mut remaining_outputs) = outputs.split_at_mut(layout.output_channels());
        let main = AudioBuffer::separate(&inputs[..layout.input_channels()], main_outputs, frames)
            .map_err(|_| BridgeError::InvalidAudio)?;
        let mut input_offset = layout.input_channels();
        let mut auxiliary: [AudioBuffer<'_, '_, S>; MAXIMUM_AUXILIARY_BUSES] =
            std::array::from_fn(|index| {
                let (ins, outs) = self
                    .config
                    .auxiliary_buses()
                    .get(index)
                    .map_or((0, 0), |b| (b.input_channels(), b.output_channels()));
                let (bus_outputs, rest) = std::mem::take(&mut remaining_outputs).split_at_mut(outs);
                remaining_outputs = rest;
                let bus = AudioBuffer::separate(
                    &inputs[input_offset..input_offset + ins],
                    bus_outputs,
                    frames,
                )
                .expect("prepared scratch dimensions");
                input_offset += ins;
                bus
            });
        let mut block = ProcessBlock::new(main, &self.events.events, transport);
        block.auxiliary = Some(&mut auxiliary[..self.config.auxiliary_buses().len()]);
        if !self.generated.is_empty() {
            block.output_events = Some(ravet_foundation::OutputEvents::new(
                &mut self.generated,
                frames,
            ));
        }
        match self.processor.process(&mut block) {
            ProcessStatus::Fault(error) => Err(BridgeError::Processor(error)),
            status => Ok((
                status,
                block
                    .output_events
                    .as_ref()
                    .map_or(0, |events| events.len()),
            )),
        }
    }
}
