//! MIDI-only stereo CLAP export of the portable 16-voice sine reference.
//! Level is the sole persisted control. Notes, envelopes, MIDI controller/RPN
//! performance state and telemetry are transient. Restores atomically publish
//! level/generation; audio synchronization resets notes before the next callback.
//! No CLAP note-ID dialect, custom editor, SysEx/UMP, aux buses or f64 export.
use clack_extensions::{audio_ports::*, note_ports::*, params::*, state::*, tail::*};
use clack_plugin::{events::event_types::ParamValueEvent, prelude::*, utils::Cookie};
use ravet_clap::{MAXIMUM_EVENTS, MainBusBridge, ParameterEvents};
use ravet_foundation::{AudioLayout, EventData, ProcessConfig};
use ravet_instrument::{SineInstrument, ZoneSide};
use std::{
    cell::RefCell,
    ffi::CStr,
    fmt::Write,
    io::{Read, Write as IoWrite},
    sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering},
};
#[cfg(not(target_has_atomic = "64"))]
compile_error!("Sine CLAP requires lock-free 64-bit atomics");
pub const PLUGIN_ID: &str = "org.ravet.sine";
pub const LEVEL_ID: ClapId = ClapId::new(1);
pub struct SinePlugin;
impl Plugin for SinePlugin {
    type AudioProcessor<'a> = SineAudio<'a>;
    type Shared<'a> = SineShared;
    type MainThread<'a> = SineMain<'a>;
    fn declare_extensions(builder: &mut PluginExtensions<Self>, _: Option<&SineShared>) {
        builder
            .register::<PluginAudioPorts>()
            .register::<PluginNotePorts>()
            .register::<PluginParams>()
            .register::<PluginState>()
            .register::<PluginTail>();
    }
}
impl DefaultPluginFactory for SinePlugin {
    fn get_descriptor() -> PluginDescriptor {
        use clack_plugin::plugin::features::{INSTRUMENT, STEREO, SYNTHESIZER};
        PluginDescriptor::new(PLUGIN_ID, "RAVET Sine")
            .with_vendor("RAVET")
            .with_version(env!("CARGO_PKG_VERSION"))
            .with_description("Portable sine MIDI instrument reference")
            .with_features([INSTRUMENT, SYNTHESIZER, STEREO])
    }
    fn new_shared(_: HostSharedHandle<'_>) -> Result<SineShared, PluginError> {
        Ok(SineShared {
            state: AtomicU64::new(u64::from(0.2_f32.to_bits())),
            fault: AtomicU32::new(0),
            parameter_refresh: AtomicBool::new(false),
        })
    }
    fn new_main_thread<'a>(
        host: HostMainThreadHandle<'a>,
        shared: &'a SineShared,
    ) -> Result<SineMain<'a>, PluginError> {
        Ok(SineMain {
            host,
            shared,
            events: RefCell::new(ParameterEvents::new()),
        })
    }
}
pub struct SineShared {
    state: AtomicU64,
    fault: AtomicU32,
    parameter_refresh: AtomicBool,
}
impl PluginShared<'_> for SineShared {}
impl SineShared {
    fn snapshot(&self) -> u64 {
        self.state.load(Ordering::Acquire)
    }
    fn level(snapshot: u64) -> f32 {
        f32::from_bits(snapshot as u32)
    }
    fn restore(&self, level: f32) {
        let mut old = self.snapshot();
        loop {
            let next = ((old >> 32).wrapping_add(1) << 32) | u64::from(level.to_bits());
            match self
                .state
                .compare_exchange_weak(old, next, Ordering::AcqRel, Ordering::Acquire)
            {
                Ok(_) => break,
                Err(newer) => old = newer,
            }
        }
    }
    fn reflect(&self, snapshot: u64, level: f32) -> bool {
        self.state
            .compare_exchange(
                snapshot,
                (snapshot & 0xffff_ffff_0000_0000) | u64::from(level.to_bits()),
                Ordering::AcqRel,
                Ordering::Acquire,
            )
            .is_ok()
            && Self::level(snapshot).to_bits() != level.to_bits()
    }
    pub fn fault_code(&self) -> u32 {
        self.fault.load(Ordering::Relaxed)
    }
}
pub struct SineMain<'a> {
    host: HostMainThreadHandle<'a>,
    shared: &'a SineShared,
    events: RefCell<ParameterEvents>,
}
impl<'a> PluginMainThread<'a, SineShared> for SineMain<'a> {
    fn on_main_thread(&self) {
        if self.shared.parameter_refresh.swap(false, Ordering::AcqRel)
            && let Some(params) = self.host.get_extension::<HostParams>()
        {
            params.rescan(&self.host, ParamRescanFlags::VALUES);
        }
    }
}
impl PluginAudioPortsImpl for SineMain<'_> {
    fn count(&self, input: bool) -> u32 {
        u32::from(!input)
    }
    fn get(&self, index: u32, input: bool, writer: &mut AudioPortInfoWriter) {
        if !input && index == 0 {
            writer.set(&AudioPortInfo {
                id: ClapId::new(0),
                name: b"Stereo output",
                channel_count: 2,
                flags: AudioPortFlags::IS_MAIN,
                port_type: Some(AudioPortType::STEREO),
                in_place_pair: None,
            });
        }
    }
}
impl PluginNotePortsImpl for SineMain<'_> {
    fn count(&self, input: bool) -> u32 {
        u32::from(input)
    }
    fn get(&self, index: u32, input: bool, writer: &mut NotePortInfoWriter) {
        if input && index == 0 {
            writer.set(&NotePortInfo {
                id: ClapId::new(0),
                name: b"MIDI input",
                supported_dialects: NoteDialects::MIDI,
                preferred_dialect: Some(NoteDialect::Midi),
            });
        }
    }
}
fn next_level(events: &[ravet_foundation::Event], initial: f32) -> f32 {
    let mut level = initial;
    for event in events {
        if let EventData::ParameterValue {
            id: ravet_instrument::INSTRUMENT_LEVEL_ID,
            value,
        } = event.data
        {
            level = value.clamp(0., 1.);
        }
    }
    level
}
impl PluginMainThreadParams for SineMain<'_> {
    fn count(&self) -> u32 {
        1
    }
    fn get_info(&self, index: u32, writer: &mut ParamInfoWriter) {
        if index == 0 {
            writer.set(&ParamInfo {
                id: LEVEL_ID,
                flags: ParamInfoFlags::IS_AUTOMATABLE,
                cookie: Cookie::empty(),
                name: b"Level",
                module: b"",
                min_value: 0.,
                max_value: 1.,
                default_value: 0.2,
            });
        }
    }
    fn get_value(&self, id: ClapId) -> Option<f64> {
        (id == LEVEL_ID).then(|| f64::from(SineShared::level(self.shared.snapshot())))
    }
    fn value_to_text(
        &self,
        id: ClapId,
        value: f64,
        writer: &mut ParamDisplayWriter,
    ) -> std::fmt::Result {
        if id != LEVEL_ID || !value.is_finite() {
            return Err(std::fmt::Error);
        }
        write!(writer, "{value:.3}")
    }
    fn text_to_value(&self, id: ClapId, text: &CStr) -> Option<f64> {
        if id != LEVEL_ID {
            return None;
        }
        let value: f64 = text.to_str().ok()?.trim().parse().ok()?;
        (value.is_finite() && (0. ..=1.).contains(&value)).then_some(value)
    }
    fn flush(&self, input: &InputEvents, _: &mut OutputEvents) {
        let mut events = self.events.borrow_mut();
        match events.translate(input, 0, true) {
            Ok(events) => {
                let snapshot = self.shared.snapshot();
                self.shared
                    .reflect(snapshot, next_level(events, SineShared::level(snapshot)));
                self.shared.fault.store(0, Ordering::Relaxed);
            }
            Err(_) => self.shared.fault.store(1, Ordering::Relaxed),
        }
    }
}
/// Deterministic 16-byte level record, magic/version RVETSIN1, level LE f32,
/// four reserved zero bytes. Reject nonfinite/out-of-range values before output.
pub fn encode_state(level: f32) -> Result<[u8; 16], &'static str> {
    if !level.is_finite() || !(0. ..=1.).contains(&level) {
        return Err("invalid sine level");
    }
    let mut bytes = [0; 16];
    bytes[..8].copy_from_slice(b"RVETSIN1");
    bytes[8..12].copy_from_slice(&level.to_le_bytes());
    Ok(bytes)
}
/// Validate exactly one record before publishing any control state.
pub fn decode_state(bytes: &[u8]) -> Result<f32, &'static str> {
    if bytes.len() != 16 || &bytes[..8] != b"RVETSIN1" || bytes[12..] != [0; 4] {
        return Err("invalid sine state record");
    }
    let level = f32::from_le_bytes(bytes[8..12].try_into().expect("validated length"));
    encode_state(level)?;
    Ok(level)
}
impl PluginStateImpl for SineMain<'_> {
    fn save(&self, output: &mut clack_plugin::stream::OutputStream) -> Result<(), PluginError> {
        output.write_all(
            &encode_state(SineShared::level(self.shared.snapshot()))
                .map_err(PluginError::Message)?,
        )?;
        Ok(())
    }
    fn load(&self, input: &mut clack_plugin::stream::InputStream) -> Result<(), PluginError> {
        let mut bytes = [0; 16];
        input.read_exact(&mut bytes)?;
        let mut trailing = [0];
        if input.read(&mut trailing)? != 0 {
            return Err(PluginError::Message("trailing sine state"));
        }
        let level = decode_state(&bytes).map_err(PluginError::Message)?;
        self.shared.restore(level);
        self.host.request_process();
        if let Some(params) = self.host.get_extension::<HostParams>() {
            params.rescan(&self.host, ParamRescanFlags::VALUES);
        }
        Ok(())
    }
}
pub struct SineAudio<'a> {
    host: HostAudioProcessorHandle<'a>,
    shared: &'a SineShared,
    bridge: MainBusBridge<SineInstrument<16>>,
    generation: u32,
}
impl SineAudio<'_> {
    fn reflect(&self, snapshot: u64) {
        if self
            .shared
            .reflect(snapshot, self.bridge.processor().level())
            && !self.shared.parameter_refresh.swap(true, Ordering::AcqRel)
        {
            self.host.request_callback();
        }
    }
    fn synchronize(&mut self) -> u64 {
        let snapshot = self.shared.snapshot();
        let generation = (snapshot >> 32) as u32;
        if generation != self.generation {
            let initial = [ParamValueEvent::new(
                0,
                LEVEL_ID,
                Pckn::match_all(),
                f64::from(SineShared::level(snapshot)),
            )];
            let result = self.bridge.flush(&InputEvents::from_buffer(&initial));
            debug_assert!(result.is_ok());
            self.bridge.reset();
            self.generation = generation;
        }
        snapshot
    }
}
impl<'a> PluginAudioProcessor<'a, SineShared, SineMain<'a>> for SineAudio<'a> {
    fn activate(
        host: HostAudioProcessorHandle<'a>,
        _: &SineMain<'a>,
        shared: &'a SineShared,
        config: PluginAudioConfiguration,
    ) -> Result<Self, PluginError> {
        let config = ProcessConfig::new(
            config.sample_rate,
            config.max_frames_count as usize,
            MAXIMUM_EVENTS,
            AudioLayout::STEREO_INSTRUMENT,
        )?;
        let processor = SineInstrument::new(ZoneSide::Lower)
            .map_err(|_| PluginError::Message("invalid voice capacity"))?;
        let bridge = MainBusBridge::prepare(processor, config, Some(0))
            .map_err(|_| PluginError::Message("invalid sine activation"))?;
        let mut audio = Self {
            host,
            shared,
            bridge,
            generation: ((shared.snapshot() >> 32) as u32).wrapping_sub(1),
        };
        audio.synchronize();
        Ok(audio)
    }
    fn process(
        &mut self,
        process: Process,
        mut audio: Audio,
        events: Events,
    ) -> Result<ProcessStatus, PluginError> {
        let snapshot = self.synchronize();
        let result = self
            .bridge
            .process(&mut audio, events.input, process.transport);
        if result.is_ok() {
            self.reflect(snapshot);
        }
        self.shared
            .fault
            .store(u32::from(result.is_err()), Ordering::Relaxed);
        Ok(ProcessStatus::Continue)
    }
    fn reset(&mut self) {
        self.synchronize();
        self.bridge.reset();
    }
    fn deactivate(mut self, _: &SineMain<'a>) {
        self.bridge.deactivate();
    }
}
impl PluginAudioProcessorParams for SineAudio<'_> {
    fn flush(&mut self, input: &InputEvents, _: &mut OutputEvents) {
        let snapshot = self.synchronize();
        let result = self.bridge.flush(input);
        if result.is_ok() {
            self.reflect(snapshot);
        }
        self.shared
            .fault
            .store(u32::from(result.is_err()), Ordering::Relaxed);
    }
}
impl PluginTailImpl for SineAudio<'_> {
    fn get(&self) -> TailLength {
        TailLength::Infinite
    }
}
#[allow(unsafe_code)]
mod export {
    clack_plugin::clack_export_entry!(clack_plugin::entry::SinglePluginEntry<super::SinePlugin>);
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn a_new_restore_wins_over_stale_audio_reflection() {
        let shared = SineShared {
            state: AtomicU64::new(u64::from(0.2_f32.to_bits())),
            fault: AtomicU32::new(0),
            parameter_refresh: AtomicBool::new(false),
        };
        let initial = shared.snapshot();
        assert!(!shared.reflect(initial, 0.2));
        assert!(shared.reflect(initial, 0.3));
        for _ in 0..10_000 {
            let old = shared.snapshot();
            shared.restore(0.7);
            assert!(!shared.reflect(old, 0.1));
            assert_eq!(SineShared::level(shared.snapshot()), 0.7);
        }
    }
}
