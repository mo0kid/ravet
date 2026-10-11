//! Native-precision main-bus bridge; no raw pointers enter portable processing.
use crate::{
    BridgeError, MAXIMUM_EVENTS, MAXIMUM_FRAMES, ParameterEvents, silence, translate_transport,
};
use clack_plugin::{
    events::{
        event_types::TransportEvent,
        io::{InputEvents, OutputEvents},
    },
    process::{
        Audio, ConstantMask,
        audio::{ChannelPair, SampleType},
    },
};
use ravet_foundation::{
    AudioBuffer, Event, EventData, MAXIMUM_CHANNELS, ParameterId, ProcessBlock, ProcessConfig,
    ProcessError, ProcessStatus, Processor, Sample, Transport,
};

/// Sealed native sample dispatch for f32/f64 CLAP buffers. No conversion occurs.
/// Host buffers containing both precisions reject rather than selecting one.
pub trait ClapSample: Sample {
    #[doc(hidden)]
    fn read_port(
        audio: &Audio<'_>,
        port: usize,
        scratch: &mut [Vec<Self>],
        frames: usize,
    ) -> Result<(), BridgeError>;
    #[doc(hidden)]
    fn validate_port(
        audio: &mut Audio<'_>,
        port: usize,
        channels: usize,
    ) -> Result<(), BridgeError>;
    #[doc(hidden)]
    fn write_port(
        audio: &mut Audio<'_>,
        port: usize,
        scratch: &[Vec<Self>],
        frames: usize,
    ) -> Result<(), BridgeError>;
    #[doc(hidden)]
    fn read(
        audio: &mut Audio<'_>,
        scratch: &mut [Vec<Self>; MAXIMUM_CHANNELS],
        inputs: usize,
        outputs: usize,
        frames: usize,
    ) -> Result<(), BridgeError>;
    #[doc(hidden)]
    fn write(
        audio: &mut Audio<'_>,
        scratch: &[Vec<Self>; MAXIMUM_CHANNELS],
        frames: usize,
    ) -> Result<(), BridgeError>;
}
macro_rules! sample {
    ($ty:ty, $variant:ident) => {
        impl ClapSample for $ty {
            fn read_port(
                audio: &Audio<'_>,
                port: usize,
                scratch: &mut [Vec<Self>],
                frames: usize,
            ) -> Result<(), BridgeError> {
                let port = audio.input_port(port).ok_or(BridgeError::LayoutMismatch)?;
                let channels = match port.channels().map_err(|_| BridgeError::InvalidAudio)? {
                    SampleType::$variant(channels) => channels,
                    _ => return Err(BridgeError::UnsupportedSampleType),
                };
                if channels.channel_count() as usize != scratch.len() {
                    return Err(BridgeError::LayoutMismatch);
                }
                for (source, dest) in channels.iter().zip(scratch) {
                    dest[..frames].copy_from_slice(source);
                }
                Ok(())
            }
            fn validate_port(
                audio: &mut Audio<'_>,
                port: usize,
                count: usize,
            ) -> Result<(), BridgeError> {
                let mut port = audio.output_port(port).ok_or(BridgeError::LayoutMismatch)?;
                let channels = match port.channels().map_err(|_| BridgeError::InvalidAudio)? {
                    SampleType::$variant(channels) => channels,
                    _ => return Err(BridgeError::UnsupportedSampleType),
                };
                if channels.channel_count() as usize != count {
                    return Err(BridgeError::LayoutMismatch);
                }
                Ok(())
            }
            fn write_port(
                audio: &mut Audio<'_>,
                port: usize,
                scratch: &[Vec<Self>],
                frames: usize,
            ) -> Result<(), BridgeError> {
                let mut port = audio.output_port(port).ok_or(BridgeError::LayoutMismatch)?;
                let mut channels = match port.channels().map_err(|_| BridgeError::InvalidAudio)? {
                    SampleType::$variant(channels) => channels,
                    _ => return Err(BridgeError::UnsupportedSampleType),
                };
                for (source, dest) in scratch.iter().zip(channels.iter_mut()) {
                    dest.copy_from_slice(&source[..frames]);
                }
                port.set_constant_mask(ConstantMask::from_bits(0));
                Ok(())
            }
            fn read(
                audio: &mut Audio<'_>,
                scratch: &mut [Vec<Self>; MAXIMUM_CHANNELS],
                inputs: usize,
                outputs: usize,
                frames: usize,
            ) -> Result<(), BridgeError> {
                let mut pair = audio.port_pair(0).ok_or(BridgeError::LayoutMismatch)?;
                let mut channels = match pair.channels().map_err(|_| BridgeError::InvalidAudio)? {
                    SampleType::$variant(channels) => channels,
                    _ => return Err(BridgeError::UnsupportedSampleType),
                };
                if channels.input_channel_count() != inputs
                    || channels.output_channel_count() != outputs
                {
                    return Err(BridgeError::LayoutMismatch);
                }
                for (i, channel) in channels.iter_mut().enumerate() {
                    match channel {
                        ChannelPair::InputOutput(input, _) | ChannelPair::InputOnly(input) => {
                            scratch[i][..frames].copy_from_slice(input)
                        }
                        ChannelPair::InPlace(input) => scratch[i][..frames].copy_from_slice(input),
                        ChannelPair::OutputOnly(_) => (),
                    }
                }
                Ok(())
            }
            fn write(
                audio: &mut Audio<'_>,
                scratch: &[Vec<Self>; MAXIMUM_CHANNELS],
                frames: usize,
            ) -> Result<(), BridgeError> {
                let mut port = audio.output_port(0).ok_or(BridgeError::LayoutMismatch)?;
                let mut channels = match port.channels().map_err(|_| BridgeError::InvalidAudio)? {
                    SampleType::$variant(channels) => channels,
                    _ => return Err(BridgeError::UnsupportedSampleType),
                };
                for (i, channel) in channels.iter_mut().enumerate() {
                    channel.copy_from_slice(&scratch[i][..frames]);
                }
                port.set_constant_mask(ConstantMask::from_bits(0));
                Ok(())
            }
        }
    };
}
sample!(f32, F32);
sample!(f64, F64);
/// Successful audio progression plus separately reported generated-event delivery.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BridgeReport {
    pub status: ProcessStatus,
    pub delivery: crate::OutputDelivery,
}
/// Prepared one-main-bus effect/instrument adapter for native f32 or f64.
/// Accepts 0..64 inputs and 1..64 outputs, <=65,536 frames/1,024 events; no
/// auxiliary buses. Opt-in bounded output events use explicit host delivery.
/// Optional selected MIDI input port maps
/// validated short messages with sample timing; scoped parameters require an
/// independent input/output selector.
/// UMP words retain framing and timing without semantic conversion.
/// Native CLAP notes/standard expressions require explicit selected ports;
/// byte-pointer SysEx and note end filter. Exporters
/// must advertise only processor-supported MIDI dialects and precision/layout.
///
/// Input scratch is copied before processing to preserve legal host aliases;
/// output scratch is separate and every portable output must be initialized.
/// Up to 64 MiB scratch at maximum f64 shape. Preparation/drop allocate; process,
/// flush/reset avoid heap/locks/I/O. Invalid whole blocks silence host outputs
/// without changing DSP/transport history. Host buffer validity remains Clack's
/// caller contract. Flush applies parameters only, without advancing time.
pub struct MainBusBridge<P, S: ClapSample = f32> {
    processor: P,
    config: ProcessConfig,
    inputs: [Vec<S>; MAXIMUM_CHANNELS],
    outputs: [Vec<S>; MAXIMUM_CHANNELS],
    events: ParameterEvents,
    midi_port: Option<u16>,
    output_midi_port: Option<u16>,
    output_clap_note_port: Option<u16>,
    scoped_parameter_output_port: Option<u16>,
    note_end_output_port: Option<u16>,
    output_events: Vec<Event>,
    previous: Option<(Transport, usize)>,
    reset_error: Option<ProcessError>,
}
impl<P: Processor<S>, S: ClapSample> MainBusBridge<P, S> {
    pub fn prepare(
        processor: P,
        config: ProcessConfig,
        midi_port: Option<u16>,
    ) -> Result<Self, BridgeError> {
        if config.maximum_output_events() != 0 {
            return Err(BridgeError::LayoutMismatch);
        }
        Self::prepare_with_output(processor, config, midi_port, None)
    }
    /// Prepare at most 1024 generated events in addition to bounded audio/input
    /// scratch. Parameter output is global; MIDI/normalized notes require the
    /// selected output port. Exporters advertise that port's MIDI dialect. Notes
    /// quantize velocity to 7 bits (positive note-on rounds to at least one).
    /// Use process_with_output/flush_with_output when capacity is nonzero.
    pub fn prepare_with_output(
        mut processor: P,
        config: ProcessConfig,
        midi_port: Option<u16>,
        output_midi_port: Option<u16>,
    ) -> Result<Self, BridgeError> {
        if config.maximum_output_events() > MAXIMUM_EVENTS {
            return Err(BridgeError::TooManyEvents);
        }
        if config.maximum_frames() > MAXIMUM_FRAMES {
            return Err(BridgeError::TooManyFrames);
        }
        if config.maximum_events() > MAXIMUM_EVENTS {
            return Err(BridgeError::TooManyEvents);
        }
        if config.layout().output_channels() == 0 || !config.auxiliary_buses().is_empty() {
            return Err(BridgeError::LayoutMismatch);
        }
        processor.prepare(config).map_err(BridgeError::Prepare)?;
        Ok(Self {
            processor,
            config,
            inputs: std::array::from_fn(|i| {
                if i < config.layout().input_channels() {
                    vec![S::default(); config.maximum_frames()]
                } else {
                    Vec::new()
                }
            }),
            outputs: std::array::from_fn(|i| {
                if i < config.layout().output_channels() {
                    vec![S::default(); config.maximum_frames()]
                } else {
                    Vec::new()
                }
            }),
            events: ParameterEvents::with_capacity(config.maximum_events())?,
            midi_port,
            output_midi_port,
            output_clap_note_port: None,
            scoped_parameter_output_port: None,
            note_end_output_port: None,
            output_events: vec![
                Event {
                    frame_offset: 0,
                    data: EventData::ParameterValue {
                        id: ParameterId(0),
                        value: 0.
                    }
                };
                config.maximum_output_events()
            ],
            previous: None,
            reset_error: None,
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
        let _ = self.try_reset();
    }
    /// Preserve refused history reset and stop subsequent DSP dispatch.
    pub fn try_reset(&mut self) -> Result<(), ProcessError> {
        match self.processor.try_reset() {
            Ok(()) => {
                self.reset_error = None;
                self.previous = None;
                Ok(())
            }
            Err(error) => {
                self.reset_error.get_or_insert(error);
                Err(error)
            }
        }
    }
    /// Original refused reset, until a complete successful reset.
    pub fn reset_error(&self) -> Option<ProcessError> {
        self.reset_error
    }
    pub fn deactivate(&mut self) {
        self.processor.deactivate();
        self.previous = None;
    }
    pub fn flush(&mut self, events: &InputEvents<'_>) -> Result<(), BridgeError> {
        if self.config.maximum_output_events() != 0 {
            return Err(BridgeError::LayoutMismatch);
        }
        self.flush_with_output(events, &mut OutputEvents::void())
            .map(|_| ())
    }
    /// Parameter-only zero-time flush with generated output delivery. Queue
    /// rejection is reported after DSP progresses; it cannot roll back state.
    pub fn flush_with_output(
        &mut self,
        events: &InputEvents<'_>,
        output: &mut OutputEvents<'_>,
    ) -> Result<BridgeReport, BridgeError> {
        self.events.translate(events, 0, true)?;
        let (status, count) = self.run(0, Transport::default())?;
        let delivery = crate::output::deliver(
            &self.output_events[..count],
            self.output_midi_port,
            self.output_clap_note_port,
            self.scoped_parameter_output_port,
            self.note_end_output_port,
            true,
            output,
        );
        Ok(BridgeReport { status, delivery })
    }
    pub fn process(
        &mut self,
        audio: &mut Audio<'_>,
        events: &InputEvents<'_>,
        transport: Option<&TransportEvent>,
    ) -> Result<ProcessStatus, BridgeError> {
        if self.config.maximum_output_events() != 0 {
            silence(audio);
            return Err(BridgeError::LayoutMismatch);
        }
        self.process_with_output(audio, events, transport, &mut OutputEvents::void())
            .map(|report| report.status)
    }
    /// Process audio and bounded generated events. Invalid input/DSP faults
    /// silence audio and emit nothing. Successful audio is preserved even if the
    /// generated stream is unrepresentable or the host queue accepts only a
    /// prefix. Delivery reports discarded events; there is no implicit retry.
    pub fn process_with_output(
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
        let inputs = self.config.layout().input_channels();
        let outputs = self.config.layout().output_channels();
        if audio.input_port_count() != usize::from(inputs != 0) || audio.output_port_count() != 1 {
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
        S::read(audio, &mut self.inputs, inputs, outputs, frames)?;
        let (status, count) = self.run(frames, context)?;
        S::write(audio, &self.outputs, frames)?;
        self.previous = transport.map(|_| (context, frames));
        let delivery = crate::output::deliver(
            &self.output_events[..count],
            self.output_midi_port,
            self.output_clap_note_port,
            self.scoped_parameter_output_port,
            self.note_end_output_port,
            false,
            output,
        );
        Ok(BridgeReport { status, delivery })
    }
    fn run(
        &mut self,
        frames: usize,
        transport: Transport,
    ) -> Result<(ProcessStatus, usize), BridgeError> {
        if let Some(error) = self.reset_error {
            return Err(BridgeError::Processor(error));
        }
        let inputs = self.inputs.each_ref().map(|c| &c[..frames.min(c.len())]);
        let mut outputs = self.outputs.each_mut().map(|c| {
            let len = frames.min(c.len());
            &mut c[..len]
        });
        let audio = AudioBuffer::separate(
            &inputs[..self.config.layout().input_channels()],
            &mut outputs[..self.config.layout().output_channels()],
            frames,
        )
        .map_err(|_| BridgeError::InvalidAudio)?;
        let mut block = ProcessBlock::new(audio, &self.events.events, transport);
        if self.config.maximum_output_events() != 0 {
            block.output_events = Some(ravet_foundation::OutputEvents::new(
                &mut self.output_events,
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
                    .map_or(0, |output| output.len()),
            )),
        }
    }
}
