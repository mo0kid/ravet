//! Synthetic native f32/f64 stereo gain with explicit global/scoped event modes.
use clack_extensions::{audio_ports::*, note_ports::*, params::*};
use clack_plugin::utils::Cookie;
use clack_plugin::{
    prelude::*,
    process::audio::{ChannelPair, SampleType},
};
use std::sync::atomic::{AtomicU64, Ordering};
#[cfg(test)]
thread_local! { static OUTPUT_MODE: std::cell::Cell<u8> = const { std::cell::Cell::new(0) }; }
#[cfg(test)]
#[allow(dead_code)]
pub fn set_output_mode(mode: u8) {
    OUTPUT_MODE.with(|value| value.set(mode));
}
#[cfg(test)]
fn output_mode() -> u8 {
    OUTPUT_MODE.with(|value| value.get())
}
#[cfg(not(test))]
fn output_mode() -> u8 {
    0
}
pub struct Fixture<const GESTURE: bool = false, const MODULATION: bool = false>;
pub struct Shared(AtomicU64, u8);
pub struct Main<'a>(&'a Shared);
pub struct Audio<'a> {
    shared: &'a Shared,
    gain: f64,
    modulation: f64,
    scoped_value: Option<f64>,
    scoped_modulation: Option<f64>,
    curve: Vec<f64>,
}
impl PluginShared<'_> for Shared {}
impl<const GESTURE: bool, const MODULATION: bool> Plugin for Fixture<GESTURE, MODULATION> {
    type Shared<'a> = Shared;
    type MainThread<'a> = Main<'a>;
    type AudioProcessor<'a> = Audio<'a>;
    fn declare_extensions(builder: &mut PluginExtensions<Self>, _: Option<&Shared>) {
        builder.register::<PluginAudioPorts>();
        builder.register::<PluginParams>();
        builder.register::<PluginNotePorts>();
    }
}
impl<const GESTURE: bool, const MODULATION: bool> DefaultPluginFactory
    for Fixture<GESTURE, MODULATION>
{
    fn get_descriptor() -> PluginDescriptor {
        PluginDescriptor::new(
            if MODULATION {
                "org.ravet.host-modulation-fixture"
            } else if GESTURE {
                "org.ravet.host-gesture-fixture"
            } else {
                "org.ravet.host-parameter-fixture"
            },
            "RAVET native parameter graph fixture",
        )
    }
    fn new_shared(_: HostSharedHandle<'_>) -> Result<Shared, PluginError> {
        Ok(Shared(
            AtomicU64::new(1_f64.to_bits()),
            if MODULATION {
                50
            } else if GESTURE {
                40
            } else {
                output_mode()
            },
        ))
    }
    fn new_main_thread<'a>(
        _: HostMainThreadHandle<'a>,
        shared: &'a Shared,
    ) -> Result<Main<'a>, PluginError> {
        Ok(Main(shared))
    }
}
impl<'a> PluginMainThread<'a, Shared> for Main<'a> {}
impl PluginAudioPortsImpl for Main<'_> {
    fn count(&self, _: bool) -> u32 {
        1
    }
    fn get(&self, index: u32, _: bool, writer: &mut AudioPortInfoWriter) {
        if index == 0 {
            writer.set(&AudioPortInfo {
                id: ClapId::new(0),
                name: b"Synthetic stereo",
                flags: AudioPortFlags::IS_MAIN
                    | AudioPortFlags::SUPPORTS_64BITS
                    | AudioPortFlags::REQUIRES_COMMON_SAMPLE_SIZE,
                channel_count: 2,
                port_type: Some(AudioPortType::STEREO),
                in_place_pair: Some(ClapId::new(0)),
            });
        }
    }
}
impl PluginNotePortsImpl for Main<'_> {
    fn count(&self, _: bool) -> u32 {
        2
    }
    fn get(&self, index: u32, input: bool, writer: &mut NotePortInfoWriter) {
        if index < 2 {
            writer.set(&NotePortInfo {
                id: ClapId::new(if input { 100 + index } else { 200 + index }),
                name: b"Synthetic selected note input",
                supported_dialects: if index == 0 {
                    NoteDialects::MIDI
                } else {
                    NoteDialects::CLAP
                        | NoteDialects::MIDI
                        | NoteDialects::MIDI_MPE
                        | NoteDialects::MIDI2
                },
                preferred_dialect: Some(if index == 0 {
                    NoteDialect::Midi
                } else {
                    NoteDialect::Clap
                }),
            });
        }
    }
}
fn scope_flags(mode: u8) -> ParamInfoFlags {
    let values = ParamInfoFlags::IS_AUTOMATABLE_PER_NOTE_ID
        | ParamInfoFlags::IS_AUTOMATABLE_PER_KEY
        | ParamInfoFlags::IS_AUTOMATABLE_PER_CHANNEL
        | ParamInfoFlags::IS_AUTOMATABLE_PER_PORT;
    let modulation = ParamInfoFlags::IS_MODULATABLE_PER_NOTE_ID
        | ParamInfoFlags::IS_MODULATABLE_PER_KEY
        | ParamInfoFlags::IS_MODULATABLE_PER_CHANNEL
        | ParamInfoFlags::IS_MODULATABLE_PER_PORT;
    match mode {
        80 => values | modulation | ParamInfoFlags::IS_STEPPED,
        71 => values,
        72 => modulation,
        73 => ParamInfoFlags::IS_AUTOMATABLE_PER_KEY | ParamInfoFlags::IS_MODULATABLE_PER_KEY,
        74 => {
            ParamInfoFlags::IS_AUTOMATABLE_PER_CHANNEL | ParamInfoFlags::IS_MODULATABLE_PER_CHANNEL
        }
        75 => ParamInfoFlags::IS_AUTOMATABLE_PER_PORT | ParamInfoFlags::IS_MODULATABLE_PER_PORT,
        76 => {
            ParamInfoFlags::IS_AUTOMATABLE_PER_NOTE_ID | ParamInfoFlags::IS_MODULATABLE_PER_NOTE_ID
        }
        _ => values | modulation,
    }
}
impl PluginMainThreadParams for Main<'_> {
    fn count(&self) -> u32 {
        2
    }
    fn get_info(&self, index: u32, writer: &mut ParamInfoWriter) {
        if index < 2 {
            writer.set(&ParamInfo {
                id: ClapId::new(index + 1),
                flags: if index == 0 {
                    ParamInfoFlags::IS_AUTOMATABLE
                        | if (70..=80).contains(&self.0.1) {
                            scope_flags(self.0.1)
                        } else {
                            ParamInfoFlags::IS_STEPPED
                        }
                        | if (50..=59).contains(&self.0.1) && self.0.1 != 54 {
                            ParamInfoFlags::IS_MODULATABLE
                        } else {
                            ParamInfoFlags::empty()
                        }
                } else {
                    ParamInfoFlags::IS_READONLY
                        | if self.0.1 == 11 {
                            ParamInfoFlags::IS_STEPPED
                        } else {
                            ParamInfoFlags::empty()
                        }
                },
                cookie: Cookie::empty(),
                name: b"Synthetic stepped gain",
                module: b"Fixture",
                min_value: 0.,
                max_value: if index == 1 && self.0.1 == 10 {
                    f64::MAX
                } else {
                    4.
                },
                default_value: 1.,
            });
        }
    }
    fn get_value(&self, id: ClapId) -> Option<f64> {
        matches!(id.get(), 1 | 2).then(|| f64::from_bits(self.0.0.load(Ordering::Acquire)))
    }
    fn value_to_text(&self, _: ClapId, _: f64, _: &mut ParamDisplayWriter) -> std::fmt::Result {
        Ok(())
    }
    fn text_to_value(&self, _: ClapId, _: &std::ffi::CStr) -> Option<f64> {
        None
    }
    fn flush(
        &self,
        input: &clack_plugin::events::io::InputEvents,
        _: &mut clack_plugin::events::io::OutputEvents,
    ) {
        for event in input.iter() {
            if let Some(value) =
                event.as_event::<clack_plugin::events::event_types::ParamValueEvent>()
            {
                self.0.0.store(value.value().to_bits(), Ordering::Release);
            }
        }
    }
}
impl<'a> PluginAudioProcessor<'a, Shared, Main<'a>> for Audio<'a> {
    fn activate(
        _: HostAudioProcessorHandle<'a>,
        _: &Main<'a>,
        shared: &'a Shared,
        config: PluginAudioConfiguration,
    ) -> Result<Self, PluginError> {
        Ok(Self {
            shared,
            gain: f64::from_bits(shared.0.load(Ordering::Acquire)),
            modulation: 0.,
            scoped_value: None,
            scoped_modulation: None,
            curve: vec![0.; config.max_frames_count as usize],
        })
    }
    fn process(
        &mut self,
        _: Process,
        mut audio: clack_plugin::process::Audio,
        events: clack_plugin::process::Events,
    ) -> Result<ProcessStatus, PluginError> {
        let frames = audio.frames_count() as usize;
        let mut changes = events.input.iter().peekable();
        for frame in 0..frames {
            while changes
                .peek()
                .is_some_and(|event| event.header().time() as usize == frame)
            {
                let event = changes.next().unwrap();
                use clack_plugin::events::event_types::{
                    Midi2Event, MidiEvent, NoteOffEvent, NoteOnEvent, ParamValueEvent,
                };
                if self.apply_scope(event)? {
                    let _ = events.output.try_push(event);
                } else if let Some(change) = event.as_event::<ParamValueEvent>() {
                    if change.param_id() != Some(ClapId::new(1))
                        || change.pckn() != clack_plugin::events::Pckn::match_all()
                        || !change.cookie().as_raw().is_null()
                    {
                        return Err(PluginError::Message(
                            "unexpected parameter identity/target/cookie",
                        ));
                    }
                    self.gain = change.value();
                } else if let Some(change) =
                    event.as_event::<clack_plugin::events::event_types::ParamModEvent>()
                {
                    if change.param_id() != Some(ClapId::new(1))
                        || change.pckn() != Pckn::match_all()
                        || !change.amount().is_finite()
                        || !change.cookie().as_raw().is_null()
                    {
                        return Err(PluginError::Message("unexpected synthetic modulation"));
                    }
                    self.modulation = change.amount();
                } else if let Some(change) =
                    event.as_event::<clack_plugin::events::event_types::ParamGestureBeginEvent>()
                {
                    if self.shared.1 != 44 || change.param_id() != Some(ClapId::new(1)) {
                        return Err(PluginError::Message("unexpected synthetic gesture begin"));
                    }
                } else if let Some(change) =
                    event.as_event::<clack_plugin::events::event_types::ParamGestureEndEvent>()
                {
                    if self.shared.1 != 44 || change.param_id() != Some(ClapId::new(1)) {
                        return Err(PluginError::Message("unexpected synthetic gesture end"));
                    }
                } else if let Some(change) =
                    event.as_event::<clack_plugin::events::event_types::NoteExpressionEvent>()
                {
                    if self.shared.1 != 46
                        || change.pckn() != Pckn::new(1u16, 2u16, 60u16, 123u32)
                        || change.expression_type()
                            != Some(clack_plugin::events::event_types::NoteExpressionType::Pressure)
                        || !change.value().is_finite()
                        || !(0. ..=1.).contains(&change.value())
                    {
                        return Err(PluginError::Message("unexpected synthetic note expression"));
                    }
                    self.gain = 1. + change.value();
                } else if let Some(change) = event.as_event::<NoteOnEvent>() {
                    if change.pckn()
                        != clack_plugin::events::Pckn::new(
                            1u16,
                            2u16,
                            60u16,
                            if matches!(self.shared.1, 45..=49 | 60..=62) {
                                clack_plugin::events::Match::Specific(123)
                            } else {
                                clack_plugin::events::Match::All
                            },
                        )
                        || change.velocity() != 0.5
                    {
                        return Err(PluginError::Message("unexpected synthetic native note"));
                    }
                    self.gain = 2.;
                } else if let Some(change) = event.as_event::<NoteOffEvent>() {
                    if self.shared.1 == 60 {
                        if change.pckn()
                            != Pckn::new(1u16, clack_plugin::events::Match::All, 60u16, 123u32)
                            || change.velocity() != 0.5
                        {
                            return Err(PluginError::Message("unexpected synthetic scoped off"));
                        }
                        self.gain = 1.;
                    } else {
                        if change.pckn()
                            != clack_plugin::events::Pckn::new(
                                1u16,
                                2u16,
                                60u16,
                                if matches!(self.shared.1, 45..=49 | 60..=62) {
                                    clack_plugin::events::Match::Specific(123)
                                } else {
                                    clack_plugin::events::Match::All
                                },
                            )
                            || change.velocity() != 0.5
                        {
                            return Err(PluginError::Message("unexpected synthetic native off"));
                        }
                        self.gain = 0.;
                    }
                } else if let Some(change) =
                    event.as_event::<clack_plugin::events::event_types::NoteChokeEvent>()
                {
                    if self.shared.1 != 60
                        || change.pckn()
                            != Pckn::new(
                                1u16,
                                clack_plugin::events::Match::All,
                                clack_plugin::events::Match::All,
                                clack_plugin::events::Match::All,
                            )
                    {
                        return Err(PluginError::Message("unexpected synthetic choke"));
                    }
                    self.gain = 0.;
                } else if let Some(change) = event.as_event::<MidiEvent>() {
                    if change.port_index() != 1 {
                        return Err(PluginError::Message("synthetic MIDI index mismatch"));
                    }
                    self.gain = match change.data() {
                        [0x92, 60, 64] => 2.,
                        [0x82, 60, 64] => 0.,
                        [0xC2, 7, 0] => 3.,
                        [0xF8, 0, 0] => 1.,
                        _ => return Err(PluginError::Message("unexpected synthetic MIDI bytes")),
                    };
                } else if let Some(change) = event.as_event::<Midi2Event>() {
                    if change.port_index() != 1 {
                        return Err(PluginError::Message("synthetic UMP index mismatch"));
                    }
                    self.gain = match change.data() {
                        [0x42903C00, 0x80000000, 0, 0] => 2.,
                        [0x42803C00, 0x80000000, 0, 0] => 0.,
                        _ => return Err(PluginError::Message("unexpected synthetic UMP words")),
                    };
                } else {
                    return Err(PluginError::Message("unsupported synthetic input"));
                }
                if event.as_event::<ParamValueEvent>().is_none()
                    && event
                        .as_event::<clack_plugin::events::event_types::ParamModEvent>()
                        .is_none()
                {
                    // Synthetic echo retains the native performance target/time.
                    // Both selected input/output indices are one in this fixture.
                    let _ = events.output.try_push(event);
                }
            }
            self.curve[frame] = if (70..=80).contains(&self.shared.1) {
                self.scoped_value.unwrap_or(self.gain)
                    + self.scoped_modulation.unwrap_or(self.modulation)
            } else if (50..=59).contains(&self.shared.1) {
                (self.gain + self.modulation).clamp(0., 4.)
            } else {
                self.gain
            };
        }
        macro_rules! multiply {
            ($channels:expr, $sample:ty) => {
                for pair in $channels.iter_mut() {
                    match pair {
                        ChannelPair::InputOutput(input, output) => {
                            for (frame, (output, input)) in output.iter_mut().zip(input).enumerate()
                            {
                                *output = *input * (self.curve[frame] as $sample);
                            }
                        }
                        ChannelPair::InPlace(samples) => {
                            for (frame, sample) in samples.iter_mut().enumerate() {
                                *sample *= self.curve[frame] as $sample;
                            }
                        }
                        ChannelPair::OutputOnly(samples) => samples.fill(0.),
                        ChannelPair::InputOnly(_) => (),
                    }
                }
            };
        }
        match audio
            .port_pair(0)
            .ok_or(PluginError::Message("missing fixture port"))?
            .channels()?
        {
            SampleType::F32(mut channels) => multiply!(channels, f32),
            SampleType::F64(mut channels) => multiply!(channels, f64),
            SampleType::Both(_, _) => return Err(PluginError::Message("mixed fixture precision")),
        }
        self.shared.0.store(self.gain.to_bits(), Ordering::Release);
        self.emit(events.output, frames);
        Ok(ProcessStatus::Continue)
    }
}
impl Audio<'_> {
    fn apply_scope(
        &mut self,
        event: &clack_plugin::events::UnknownEvent,
    ) -> Result<bool, PluginError> {
        if !(70..=80).contains(&self.shared.1) {
            return Ok(false);
        }
        use clack_plugin::events::event_types::{ParamModEvent, ParamValueEvent};
        if let Some(value) = event.as_event::<ParamValueEvent>() {
            if value.pckn() == Pckn::match_all() {
                return Ok(false);
            }
            if value.param_id() != Some(ClapId::new(1))
                || value.pckn().port_index != clack_plugin::events::Match::Specific(1)
                || !value.cookie().as_raw().is_null()
                || !value.value().is_finite()
            {
                return Err(PluginError::Message("unexpected scoped value"));
            }
            self.scoped_value = Some(value.value());
            return Ok(true);
        }
        if let Some(value) = event.as_event::<ParamModEvent>() {
            if value.pckn() == Pckn::match_all() {
                return Ok(false);
            }
            if value.param_id() != Some(ClapId::new(1))
                || value.pckn().port_index != clack_plugin::events::Match::Specific(1)
                || !value.cookie().as_raw().is_null()
                || !value.amount().is_finite()
            {
                return Err(PluginError::Message("unexpected scoped modulation"));
            }
            self.scoped_modulation = Some(value.amount());
            return Ok(true);
        }
        Ok(false)
    }

    fn emit(&self, output: &mut clack_plugin::events::io::OutputEvents, frames: usize) {
        use clack_plugin::events::{
            Pckn,
            event_types::{MidiEvent, ParamValueEvent},
        };
        let time = frames.saturating_sub(1) as u32;
        let mode = self.shared.1;
        if (81..=84).contains(&mode) {
            use clack_plugin::events::event_types::NoteEndEvent;
            let end = NoteEndEvent::new(u32::MAX, Pckn::new(1u16, 2u16, 60u16, 123u32));
            if mode == 81 {
                let mut raw = *end.as_raw();
                raw.velocity = f64::NAN; // Ignored completion velocity.
                let _ = output.try_push(NoteEndEvent::from_raw(&raw));
            }
            let _ = output.try_push(ParamValueEvent::new(
                time,
                ClapId::new(2),
                Pckn::match_all(),
                self.gain,
            ));
            let mut raw = *end.as_raw();
            raw.header.time = 0; // Ignored timestamp need not be sorted/in range.
            raw.velocity = f64::NAN;
            if mode != 81 {
                raw.channel = -2;
            }
            if mode == 83 {
                raw.port_index = 0;
            }
            if mode == 84 {
                raw.port_index = -1;
            }
            let _ = output.try_push(NoteEndEvent::from_raw(&raw));
            return;
        }
        if mode == 71 {
            let _ = output.try_push(clack_plugin::events::event_types::ParamModEvent::new(
                time,
                ClapId::new(1),
                Pckn::new(1u16, 2u16, 60u16, 123u32),
                0.5,
            ));
            return;
        }
        if mode == 72 {
            let _ = output.try_push(ParamValueEvent::new(
                time,
                ClapId::new(1),
                Pckn::new(1u16, 2u16, 60u16, 123u32),
                0.5,
            ));
            return;
        }
        if (77..=79).contains(&mode) {
            let native = ParamValueEvent::new(
                time,
                ClapId::new(if mode == 79 { 99 } else { 1 }),
                Pckn::new(if mode == 78 { 0u16 } else { 1u16 }, 2u16, 60u16, 123u32),
                f64::NAN,
            );
            let _ = output.try_push(native);
            return;
        }
        if matches!(mode, 61..=62) {
            let native = clack_plugin::events::event_types::NoteChokeEvent::new(
                time,
                Pckn::new(
                    1u16,
                    clack_plugin::events::Match::All,
                    clack_plugin::events::Match::All,
                    clack_plugin::events::Match::All,
                ),
            );
            let mut raw = *native.as_raw();
            raw.channel = -2;
            if mode == 62 {
                raw.port_index = 0;
            }
            let _ = output.try_push(clack_plugin::events::event_types::NoteChokeEvent::from_raw(
                &raw,
            ));
            return;
        }
        if (47..=49).contains(&mode) {
            use clack_plugin::events::event_types::{NoteExpressionEvent, NoteExpressionType};
            let native = NoteExpressionEvent::new(
                time,
                Pckn::new(if mode == 49 { 0u16 } else { 1u16 }, 2u16, 60u16, 123u32),
                NoteExpressionType::Pressure,
                f64::NAN,
            );
            if mode == 48 {
                let mut raw = *native.as_raw();
                raw.expression_id = 99;
                let _ = output.try_push(NoteExpressionEvent::from_raw(&raw));
            } else {
                let _ = output.try_push(native);
            }
            return;
        }
        if (50..=59).contains(&mode) {
            use clack_plugin::events::{Match, event_types::ParamModEvent};
            let amount = match mode {
                51 => f64::NAN,
                52 => f64::MAX,
                _ => self.modulation,
            };
            let target = if mode == 53 {
                Pckn::new(Match::Specific(1), Match::All, Match::All, Match::All)
            } else {
                Pckn::match_all()
            };
            let id = ClapId::new(match mode {
                56 => 2,
                58 => 99,
                _ => 1,
            });
            let _ = output.try_push(ParamModEvent::new(time, id, target, amount));
            let _ = output.try_push(ParamValueEvent::new(
                if mode == 55 { 0 } else { time },
                ClapId::new(2),
                Pckn::match_all(),
                self.gain,
            ));
            if mode == 57 {
                let _ = output.try_push(ParamModEvent::new(time, id, target, amount));
            }
            return;
        }
        if (40..=43).contains(&mode) {
            use clack_plugin::events::event_types::{ParamGestureBeginEvent, ParamGestureEndEvent};
            let id = ClapId::new(if mode == 42 { 2 } else { 1 });
            let _ = output.try_push(ParamGestureBeginEvent::new(0, id));
            let _ = output.try_push(ParamValueEvent::new(time, id, Pckn::match_all(), self.gain));
            let _ = output.try_push(ParamGestureEndEvent::new(
                if mode == 41 { 0 } else { time },
                id,
            ));
            if mode == 43 {
                let _ = output.try_push(ParamGestureEndEvent::new(time, id));
            }
            return;
        }
        let value = match mode {
            1 => 5.,
            7 => f64::NAN,
            8 => 0.1,
            10 => 1e300,
            11 => 0.5,
            _ => self.gain,
        };
        let time = if mode == 2 {
            frames as u32 + u32::from(frames == 0)
        } else {
            time
        };
        let event = ParamValueEvent::new(time, ClapId::new(2), Pckn::match_all(), value);
        if mode == 12 {
            let _ = output.try_push(event.with_key(clack_plugin::events::Match::Specific(60)));
            return;
        }
        let _ = output.try_push(event);
        if mode == 3 {
            let _ = output.try_push(event);
        }
        if mode == 4 {
            let _ = output.try_push(ParamValueEvent::new(
                0,
                ClapId::new(2),
                Pckn::match_all(),
                value,
            ));
        }
        if mode == 5 {
            let _ = output.try_push(ParamValueEvent::new(
                time,
                ClapId::new(999),
                Pckn::match_all(),
                value,
            ));
        }
        if mode == 6 {
            let _ = output.try_push(MidiEvent::new(time, 0, [0x90, 60, 127]));
        }
        if (30..=35).contains(&mode) {
            use clack_plugin::events::event_types::NoteOnEvent;
            let target = Pckn::new(1u16, 2u16, 60u16, clack_plugin::events::Match::All);
            match mode {
                31 => {
                    let _ = output.try_push(MidiEvent::new(time, 1, [0x92, 200, 64]));
                }
                32 => {
                    let _ = output.try_push(MidiEvent::new(time, 0, [0x92, 60, 64]));
                }
                33 => {
                    let _ = output.try_push(NoteOnEvent::new(
                        time,
                        Pckn::new(1u16, 2u16, 60u16, 7u32),
                        0.5,
                    ));
                }
                34 => {
                    let _ = output.try_push(NoteOnEvent::new(time, target, f64::NAN));
                }
                _ => {
                    let _ = output.try_push(MidiEvent::new(time, 1, [0x92, 60, 64]));
                }
            }
        }
    }
}
impl PluginAudioProcessorParams for Audio<'_> {
    fn flush(
        &mut self,
        input: &clack_plugin::events::io::InputEvents,
        output: &mut clack_plugin::events::io::OutputEvents,
    ) {
        for event in input.iter() {
            match self.apply_scope(event) {
                Ok(true) => {
                    let _ = output.try_push(event);
                    continue;
                }
                Err(_) => return,
                Ok(false) => {}
            }
            if let Some(value) =
                event.as_event::<clack_plugin::events::event_types::ParamValueEvent>()
            {
                self.gain = value.value();
            } else if let Some(value) =
                event.as_event::<clack_plugin::events::event_types::ParamModEvent>()
            {
                self.modulation = value.amount();
            } else if self.shared.1 == 44
                && (event
                    .as_event::<clack_plugin::events::event_types::ParamGestureBeginEvent>()
                    .is_some()
                    || event
                        .as_event::<clack_plugin::events::event_types::ParamGestureEndEvent>()
                        .is_some())
            {
                let _ = output.try_push(event);
            }
        }
        self.shared.0.store(self.gain.to_bits(), Ordering::Release);
        self.emit(output, 0);
    }
}
