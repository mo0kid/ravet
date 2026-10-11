//! Provisional CLAP bridges for portable effects and instruments.
//! `MainBusBridge` supports one 0..64-input/1..64-output native f32/f64 bus
//! with optional short MIDI input and bounded generated-event delivery; `EffectBridge` preserves the narrower gain path.
//!
//! Clack owns foreign pointers and ABI callbacks. This crate only uses its safe
//! views. Host samples pass through scratch storage allocated at activation,
//! supporting separate, in-place, and mixed channel pairs with the same portable
//! processor. Global parameter-value events and block-start transport are
//! translated by the original `EffectBridge`. The separate main-bus adapter
//! adds instruments/MIDI/native precision and opt-in bounded output events;
//! explicit CLAP note ports add note identities and standard f64 expressions;
//! `MultiBusBridge` adds ordered auxiliary buses with compact directional port mapping.
//! Global modulation and endpoint-scoped parameter input require independent opt-ins. Invalid input rejects a block before changing processor state.

mod main_bus;
mod multi_bus;
mod notes;
mod output;
mod parameters;
pub use parameters::{
    ClapParameterError, ClapParameterOptions, ClapParameterRegistration, ClapParameterScopes,
};
mod transport;
pub use main_bus::{BridgeReport, ClapSample, MainBusBridge};
pub use multi_bus::MultiBusBridge;
pub use output::{OutputDelivery, OutputRejection};
pub use transport::translate_transport;

use clack_plugin::events::{
    Event as ClapEvent,
    event_types::{
        Midi2Event, MidiEvent, ParamGestureBeginEvent, ParamGestureEndEvent, ParamModEvent,
        ParamValueEvent,
    },
    io::InputEvents,
};
use clack_plugin::process::{
    Audio, ConstantMask,
    audio::{ChannelPair, SampleType},
};
use ravet_foundation::{
    AudioBuffer, Event, EventData, ParameterId, PrepareError, ProcessBlock, ProcessConfig,
    ProcessError, ProcessStatus, Processor, Transport,
};

/// Bound both traversal of host events and the preallocated translation buffer.
pub const MAXIMUM_EVENTS: usize = 1024;
/// Bound scratch storage for this prototype (512 KiB of stereo f32 samples).
pub const MAXIMUM_FRAMES: usize = 65_536;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BridgeError {
    Prepare(PrepareError),
    TooManyFrames,
    /// Combined prepared audio/event storage exceeds the adapter byte limit.
    ScratchCapacity,
    LayoutMismatch,
    UnsupportedSampleType,
    InvalidAudio,
    TooManyEvents,
    InvalidEvent,
    InvalidTransport,
    Processor(ProcessError),
}

/// Preallocated event translator, also usable for inactive parameter flushes.
pub struct ParameterEvents {
    events: Vec<Event>,
    capacity: usize,
    global_modulation: bool,
    gesture_input: bool,
    clap_note_port: Option<u16>,
    scoped_parameter_port: Option<u16>,
}
impl ParameterEvents {
    pub fn new() -> Self {
        Self {
            events: Vec::with_capacity(MAXIMUM_EVENTS),
            capacity: MAXIMUM_EVENTS,
            global_modulation: false,
            gesture_input: false,
            clap_note_port: None,
            scoped_parameter_port: None,
        }
    }
    pub fn with_capacity(capacity: usize) -> Result<Self, BridgeError> {
        if capacity > MAXIMUM_EVENTS {
            return Err(BridgeError::TooManyEvents);
        }
        Ok(Self {
            events: Vec::with_capacity(capacity),
            capacity,
            global_modulation: false,
            gesture_input: false,
            clap_note_port: None,
            scoped_parameter_port: None,
        })
    }
    /// Select one native CLAP note/expression input port (0..256); None disables.
    /// On/Off with optional ID, scoped Off/choke and standard expressions
    /// retain timing/scope. Unknown kinds filter;
    /// malformed selected data rejects. Flush ignores performance events, and
    /// selected performance in zero-frame processing rejects. No voice policy or
    /// native metadata is synthesized; exporters advertise processor support.
    pub fn with_clap_note_input_port(mut self, port: Option<u16>) -> Result<Self, BridgeError> {
        notes::validate_port(port)?;
        self.clap_note_port = port;
        Ok(self)
    }
    /// Select endpoint-scoped parameter value/modulation translation on one note
    /// input port (0..256), independently of note-event translation. None keeps
    /// scoped filtering. Exact port/target/f64 values retain process timing; flush
    /// normalizes offsets to zero. Metadata/range/voice policy stay exporter-owned.
    pub fn with_scoped_parameter_input_port(
        mut self,
        port: Option<u16>,
    ) -> Result<Self, BridgeError> {
        notes::validate_port(port)?;
        self.scoped_parameter_port = port;
        Ok(self)
    }
    /// Select raw parameter begin/end translation. Default false ignores them.
    /// No pairing, ownership, scalar edits or recording policy is inferred.
    /// Process offsets preserve order; flush offsets normalize to zero.
    pub fn with_parameter_gesture_input(mut self, enabled: bool) -> Self {
        self.gesture_input = enabled;
        self
    }
    /// Select global physical-unit modulation translation. Default false retains
    /// ignored modulation behavior. Scoped events remain ignored; no metadata flags,
    /// base-value changes, DSP policy or persistence are synthesized. Finite native
    /// amounts must narrow to finite f32. Flush offsets normalize to zero.
    pub fn with_global_parameter_modulation(mut self, enabled: bool) -> Self {
        self.global_modulation = enabled;
        self
    }
    /// Flush timestamps carry no sample meaning and are normalized to zero.
    /// Process timestamps are checked across all events, including ignored kinds.
    /// Scoped parameters require explicit selection. Selected native notes/expressions are
    /// translated during process only; flush retains parameter-only behavior.
    pub fn translate(
        &mut self,
        input: &InputEvents<'_>,
        frames: usize,
        flush: bool,
    ) -> Result<&[Event], BridgeError> {
        self.translate_inner(input, frames, flush, None, false)
    }
    /// Translate global parameters and validated short MIDI on one explicit
    /// port. Process timestamps validate across all events; other-port MIDI,
    /// CLAP notes/expressions retain default filtering unless explicitly selected.
    /// SysEx, UMP and scoped/disabled modulation are ignored. CLAP's
    /// three-byte MIDI padding is ignored beyond the status-defined length.
    /// Flush accepts only parameters, never notes/controllers.
    pub fn translate_midi(
        &mut self,
        input: &InputEvents<'_>,
        frames: usize,
        port: u16,
    ) -> Result<&[Event], BridgeError> {
        self.translate_inner(input, frames, false, Some(port), false)
    }
    /// Translates parameters, opt-in global modulation, MIDI short messages and complete UMP packets on
    /// one explicit port. UMP word count follows the message-type nibble; unused
    /// CLAP words are ignored. No semantic validation or protocol negotiation.
    /// Input count/timing validation includes ignored events and other ports.
    pub fn translate_midi_ump(
        &mut self,
        input: &InputEvents<'_>,
        frames: usize,
        port: u16,
    ) -> Result<&[Event], BridgeError> {
        self.translate_inner(input, frames, false, Some(port), true)
    }
    fn translate_inner(
        &mut self,
        input: &InputEvents<'_>,
        frames: usize,
        flush: bool,
        midi_port: Option<u16>,
        ump: bool,
    ) -> Result<&[Event], BridgeError> {
        self.events.clear();
        let count = input.len();
        if count as usize > self.capacity {
            return Err(BridgeError::TooManyEvents);
        }
        let mut previous = 0;
        for index in 0..count {
            let event = input.get(index).ok_or(BridgeError::InvalidEvent)?;
            let offset = event.header().time() as usize;
            if !flush
                && ((frames == 0 && offset != 0)
                    || (frames > 0 && offset >= frames)
                    || offset < previous)
            {
                return Err(BridgeError::InvalidEvent);
            }
            previous = offset;
            if event.header().as_raw().space_id == 0
                && event.header().type_id() == MidiEvent::TYPE_ID
                && let Some(port) = midi_port
            {
                let midi = event
                    .as_event::<MidiEvent>()
                    .ok_or(BridgeError::InvalidEvent)?;
                if midi.port_index() != port {
                    continue;
                }
                let bytes = midi.data();
                let length = match bytes[0] {
                    0x80..=0xbf | 0xe0..=0xef | 0xf2 => 3,
                    0xc0..=0xdf | 0xf1 | 0xf3 => 2,
                    0xf6..=0xf8 | 0xfa..=0xfc | 0xfe..=0xff => 1,
                    _ => return Err(BridgeError::InvalidEvent),
                };
                let message = ravet_foundation::MidiMessage::from_bytes(&bytes[..length])
                    .map_err(|_| BridgeError::InvalidEvent)?;
                self.events.push(Event {
                    frame_offset: offset,
                    data: EventData::Midi(message),
                });
            }
            if ump
                && event.header().as_raw().space_id == 0
                && event.header().type_id() == Midi2Event::TYPE_ID
                && let Some(port) = midi_port
            {
                let midi = event
                    .as_event::<Midi2Event>()
                    .ok_or(BridgeError::InvalidEvent)?;
                if midi.port_index() != port {
                    continue;
                }
                let words = midi.data();
                let packet = ravet_foundation::UmpPacket::from_words(
                    &words[..ravet_foundation::UmpPacket::word_count(words[0])],
                )
                .map_err(|_| BridgeError::InvalidEvent)?;
                self.events.push(Event {
                    frame_offset: offset,
                    data: EventData::Ump(packet),
                });
            }
            if !flush
                && let Some(port) = self.clap_note_port
                && let Some(data) = notes::decode(port, event)?
            {
                if frames == 0 {
                    return Err(BridgeError::InvalidEvent);
                }
                self.events.push(Event {
                    frame_offset: offset,
                    data,
                });
            }
            if self.gesture_input && event.header().as_raw().space_id == 0 {
                let gesture = if event.header().type_id() == ParamGestureBeginEvent::TYPE_ID {
                    Some((
                        event
                            .as_event::<ParamGestureBeginEvent>()
                            .ok_or(BridgeError::InvalidEvent)?
                            .param_id(),
                        ravet_foundation::ParameterGesturePhase::Begin,
                    ))
                } else if event.header().type_id() == ParamGestureEndEvent::TYPE_ID {
                    Some((
                        event
                            .as_event::<ParamGestureEndEvent>()
                            .ok_or(BridgeError::InvalidEvent)?
                            .param_id(),
                        ravet_foundation::ParameterGesturePhase::End,
                    ))
                } else {
                    None
                };
                if let Some((id, phase)) = gesture {
                    self.events.push(Event {
                        frame_offset: if flush { 0 } else { offset },
                        data: EventData::ParameterGesture {
                            id: ParameterId(id.ok_or(BridgeError::InvalidEvent)?.get()),
                            phase,
                        },
                    });
                }
            }
            if (self.global_modulation || self.scoped_parameter_port.is_some())
                && event.header().as_raw().space_id == 0
                && event.header().type_id() == ParamModEvent::TYPE_ID
            {
                let modulation = event
                    .as_event::<ParamModEvent>()
                    .ok_or(BridgeError::InvalidEvent)?;
                let raw = modulation.as_raw();
                if (raw.port_index, raw.channel, raw.key, raw.note_id) != (-1, -1, -1, -1) {
                    if self
                        .scoped_parameter_port
                        .is_some_and(|port| raw.port_index == port as i16)
                    {
                        let target = notes::target(raw.channel, raw.key, raw.note_id)?;
                        let id = modulation.param_id().ok_or(BridgeError::InvalidEvent)?;
                        if !raw.amount.is_finite() {
                            return Err(BridgeError::InvalidEvent);
                        }
                        self.events.push(Event {
                            frame_offset: if flush { 0 } else { offset },
                            data: EventData::ParameterModulationScoped {
                                id: ParameterId(id.get()),
                                target,
                                amount: raw.amount,
                            },
                        });
                    }
                    continue;
                }
                if !self.global_modulation {
                    continue;
                }
                let id = modulation.param_id().ok_or(BridgeError::InvalidEvent)?;
                let amount = modulation.amount();
                let amount32 = amount as f32;
                if !amount.is_finite() || !amount32.is_finite() {
                    return Err(BridgeError::InvalidEvent);
                }
                self.events.push(Event {
                    frame_offset: if flush { 0 } else { offset },
                    data: EventData::ParameterModulation {
                        id: ParameterId(id.get()),
                        amount: amount32,
                    },
                });
            }
            if event.header().as_raw().space_id == 0
                && event.header().type_id() == ParamValueEvent::TYPE_ID
            {
                let parameter = event
                    .as_event::<ParamValueEvent>()
                    .ok_or(BridgeError::InvalidEvent)?;
                let raw = parameter.as_raw();
                if (raw.port_index, raw.channel, raw.key, raw.note_id) != (-1, -1, -1, -1) {
                    if self
                        .scoped_parameter_port
                        .is_some_and(|port| raw.port_index == port as i16)
                    {
                        let target = notes::target(raw.channel, raw.key, raw.note_id)?;
                        let id = parameter.param_id().ok_or(BridgeError::InvalidEvent)?;
                        if !raw.value.is_finite() {
                            return Err(BridgeError::InvalidEvent);
                        }
                        self.events.push(Event {
                            frame_offset: if flush { 0 } else { offset },
                            data: EventData::ParameterValueScoped {
                                id: ParameterId(id.get()),
                                target,
                                value: raw.value,
                            },
                        });
                    }
                    continue;
                }
                let id = parameter.param_id().ok_or(BridgeError::InvalidEvent)?;
                let value = parameter.value();
                let value32 = value as f32;
                if !value.is_finite() || !value32.is_finite() {
                    return Err(BridgeError::InvalidEvent);
                }
                self.events.push(Event {
                    frame_offset: if flush { 0 } else { offset },
                    data: EventData::ParameterValue {
                        id: ParameterId(id.get()),
                        value: value32,
                    },
                });
            }
        }
        Ok(&self.events)
    }
}
impl Default for ParameterEvents {
    fn default() -> Self {
        Self::new()
    }
}

/// A prepared portable effect plus bounded audio/event scratch storage.
pub struct EffectBridge<P> {
    processor: P,
    config: ProcessConfig,
    scratch: [Vec<f32>; 2],
    events: ParameterEvents,
    previous_transport: Option<(Transport, usize)>,
    reset_error: Option<ProcessError>,
}
impl<P: Processor> EffectBridge<P> {
    pub fn prepare(mut processor: P, config: ProcessConfig) -> Result<Self, BridgeError> {
        if config.maximum_frames() > MAXIMUM_FRAMES {
            return Err(BridgeError::TooManyFrames);
        }
        if config.layout().input_channels() != config.layout().output_channels()
            || !(1..=2).contains(&config.layout().output_channels())
            || !config.auxiliary_buses().is_empty()
            || config.maximum_output_events() != 0
        {
            return Err(BridgeError::LayoutMismatch);
        }
        if config.maximum_events() > MAXIMUM_EVENTS {
            return Err(BridgeError::TooManyEvents);
        }
        processor.prepare(config).map_err(BridgeError::Prepare)?;
        Ok(Self {
            processor,
            config,
            scratch: std::array::from_fn(|_| vec![0.0; config.maximum_frames()]),
            events: ParameterEvents::with_capacity(config.maximum_events())?,
            previous_transport: None,
            reset_error: None,
        })
    }
    /// Select native CLAP note/expression input for a processor that explicitly
    /// supports it. Port must be below 256; None restores default filtering.
    /// This effect adapter has no generated-event output. Metadata stays exporter-owned.
    pub fn with_clap_note_input_port(mut self, port: Option<u16>) -> Result<Self, BridgeError> {
        self.events = self.events.with_clap_note_input_port(port)?;
        Ok(self)
    }
    /// Select scoped parameter values/modulation on a note input endpoint below
    /// 256. No metadata, voice precedence, range or saved-state policy is inferred.
    pub fn with_scoped_parameter_input_port(
        mut self,
        port: Option<u16>,
    ) -> Result<Self, BridgeError> {
        self.events = self.events.with_scoped_parameter_input_port(port)?;
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
                self.previous_transport = None;
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
    }

    /// Apply parameter changes without advancing audio time.
    pub fn flush(&mut self, input: &InputEvents<'_>) -> Result<(), BridgeError> {
        self.events.translate(input, 0, true)?;
        self.run_processor(0, Transport::default()).map(|_| ())
    }
    pub fn process(
        &mut self,
        audio: &mut Audio<'_>,
        input: &InputEvents<'_>,
    ) -> Result<ProcessStatus, BridgeError> {
        self.process_with_transport(audio, input, None)
    }
    /// Forward validated block-start CLAP transport. Mid-block transport events
    /// remain ignored; processors receive the context at sample zero only.
    /// Consecutive known sample positions detect seeks/loops with one-sample
    /// conversion tolerance. Missing context/reset breaks continuity tracking.
    /// Rejected blocks silence outputs and do not advance tracking or DSP state.
    pub fn process_with_transport(
        &mut self,
        audio: &mut Audio<'_>,
        input: &InputEvents<'_>,
        transport: Option<&clack_plugin::events::event_types::TransportEvent>,
    ) -> Result<ProcessStatus, BridgeError> {
        let result = (|| {
            let mut context = translate_transport(transport, self.config.sample_rate())?;
            if let (Some((previous, frames)), Some(position)) =
                (self.previous_transport, context.sample_position)
                && let Some(start) = previous.sample_position
            {
                let advance = if previous.playing == Some(true) {
                    frames as i64
                } else {
                    0
                };
                context.discontinuity = start
                    .checked_add(advance)
                    .is_none_or(|expected| position.abs_diff(expected) > 1);
            }
            let status = self.process_inner(audio, input, context)?;
            self.previous_transport = transport.map(|_| (context, audio.frames_count() as usize));
            Ok(status)
        })();
        if result.is_err() {
            silence(audio);
        }
        result
    }
    fn process_inner(
        &mut self,
        audio: &mut Audio<'_>,
        input: &InputEvents<'_>,
        transport: Transport,
    ) -> Result<ProcessStatus, BridgeError> {
        let frames = audio.frames_count() as usize;
        if frames > self.config.maximum_frames() {
            return Err(BridgeError::TooManyFrames);
        }
        if audio.input_port_count() != 1 || audio.output_port_count() != 1 {
            return Err(BridgeError::LayoutMismatch);
        }
        let count = self.config.layout().output_channels();
        // Validate the whole event stream before reading samples or applying it.
        self.events.translate(input, frames, false)?;
        {
            let mut pair = audio.port_pair(0).ok_or(BridgeError::LayoutMismatch)?;
            let mut channels = pair
                .channels()
                .map_err(|_| BridgeError::InvalidAudio)?
                .into_f32()
                .ok_or(BridgeError::UnsupportedSampleType)?;
            if channels.input_channel_count() != count || channels.output_channel_count() != count {
                return Err(BridgeError::LayoutMismatch);
            }
            for (index, channel) in channels.iter_mut().enumerate() {
                let source = match channel {
                    ChannelPair::InputOutput(input, _) | ChannelPair::InputOnly(input) => input,
                    ChannelPair::InPlace(samples) => samples,
                    ChannelPair::OutputOnly(_) => return Err(BridgeError::LayoutMismatch),
                };
                self.scratch[index][..frames].copy_from_slice(source);
            }
        }
        let status = self.run_processor(frames, transport)?;
        let mut output = audio.output_port(0).ok_or(BridgeError::LayoutMismatch)?;
        let mut channels = output
            .channels()
            .map_err(|_| BridgeError::InvalidAudio)?
            .into_f32()
            .ok_or(BridgeError::UnsupportedSampleType)?;
        for (index, channel) in channels.iter_mut().enumerate() {
            channel.copy_from_slice(&self.scratch[index][..frames]);
        }
        output.set_constant_mask(ConstantMask::from_bits(0));
        Ok(status)
    }
    fn run_processor(
        &mut self,
        frames: usize,
        transport: Transport,
    ) -> Result<ProcessStatus, BridgeError> {
        if let Some(error) = self.reset_error {
            return Err(BridgeError::Processor(error));
        }
        let [left, right] = &mut self.scratch;
        let mut channels = [&mut left[..frames], &mut right[..frames]];
        let audio = AudioBuffer::in_place(
            &mut channels[..self.config.layout().output_channels()],
            frames,
        )
        .map_err(|_| BridgeError::InvalidAudio)?;
        let mut block = ProcessBlock::new(audio, &self.events.events, transport);
        match self.processor.process(&mut block) {
            ProcessStatus::Fault(error) => Err(BridgeError::Processor(error)),
            status => Ok(status),
        }
    }
}

/// Clear all supplied output samples, including unsupported f64 buffers, when
/// the bridge rejects a block. Clack still requires valid host-owned storage.
pub fn silence(audio: &mut Audio<'_>) {
    for mut port in audio.output_ports() {
        if let Ok(channels) = port.channels() {
            match channels {
                SampleType::F32(mut channels) => {
                    for channel in channels.iter_mut() {
                        channel.fill(0.0);
                    }
                }
                SampleType::F64(mut channels) => {
                    for channel in channels.iter_mut() {
                        channel.fill(0.0);
                    }
                }
                SampleType::Both(mut f32_channels, mut f64_channels) => {
                    for channel in f32_channels.iter_mut() {
                        channel.fill(0.0);
                    }
                    for channel in f64_channels.iter_mut() {
                        channel.fill(0.0);
                    }
                }
            }
        }
        port.set_constant_mask(ConstantMask::FULLY_CONSTANT);
    }
}
