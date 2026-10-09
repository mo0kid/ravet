//! CLAP export of the portable gain processor with versioned session state.
//!
//! Requested gain, bypass and restore generation share one atomic word; processor
//! mutation stays exclusive to the audio thread. Main-thread flush occurs only
//! while inactive, as required by CLAP. Faults yield silence and a diagnostic
//! code in shared state, with no callback logging or error-string allocation.

use clack_extensions::{audio_ports::*, audio_ports_config::*, params::*, state::*};
use clack_plugin::events::event_types::ParamValueEvent;
use clack_plugin::prelude::*;
use ravet_clap::{
    BridgeError, ClapParameterOptions, ClapParameterRegistration, EffectBridge, MAXIMUM_EVENTS,
    ParameterEvents,
};
use ravet_foundation::{AudioLayout, EventData, ProcessConfig};
use ravet_gain::state::{read_parameters, write_parameters};
use ravet_gain::{
    BYPASS_ID, GAIN_ID, GainMeterMailbox, GainMeters, GainProcessor, MAXIMUM_GAIN, parameter_schema,
};
use std::{
    cell::{Cell, RefCell},
    ffi::CStr,
    fmt::Write,
    sync::atomic::{AtomicU32, Ordering},
};

#[cfg(not(target_has_atomic = "64"))]
compile_error!("RAVET Gain CLAP requires lock-free 64-bit atomics");
mod editor;
#[cfg(all(target_os = "macos", feature = "native-editor"))]
#[allow(unsafe_code)]
mod gui;
mod shared_gain;
pub use editor::EditError;
use shared_gain::SharedGain;

pub const PLUGIN_ID: &str = "org.ravet.gain";
pub const CLAP_GAIN_ID: ClapId = ClapId::new(GAIN_ID.0);
pub const CLAP_BYPASS_ID: ClapId = ClapId::new(BYPASS_ID.0);

static PARAMETER_OPTIONS: [ClapParameterOptions<'static>; 2] = [
    ClapParameterOptions {
        module: "",
        bypass: false,
        requires_process: true,
        global_modulation: false,
        scoped_automation: ravet_clap::ClapParameterScopes::NONE,
        scoped_modulation: ravet_clap::ClapParameterScopes::NONE,
    },
    ClapParameterOptions {
        module: "",
        bypass: true,
        requires_process: true,
        global_modulation: false,
        scoped_automation: ravet_clap::ClapParameterScopes::NONE,
        scoped_modulation: ravet_clap::ClapParameterScopes::NONE,
    },
];

pub struct GainPlugin;
impl Plugin for GainPlugin {
    type AudioProcessor<'a> = GainAudio<'a>;
    type Shared<'a> = GainShared;
    type MainThread<'a> = GainMain<'a>;
    fn declare_extensions(builder: &mut PluginExtensions<Self>, _shared: Option<&GainShared>) {
        builder
            .register::<PluginAudioPorts>()
            .register::<PluginAudioPortsConfig>()
            .register::<PluginAudioPortsConfigInfo>()
            .register::<PluginParams>()
            .register::<PluginState>();
        #[cfg(all(target_os = "macos", feature = "native-editor"))]
        builder.register::<clack_extensions::gui::PluginGui>();
    }
}
impl DefaultPluginFactory for GainPlugin {
    fn get_descriptor() -> PluginDescriptor {
        use clack_plugin::plugin::features::{AUDIO_EFFECT, MONO, STEREO, UTILITY};
        PluginDescriptor::new(PLUGIN_ID, "RAVET Gain")
            .with_vendor("RAVET")
            .with_version(env!("CARGO_PKG_VERSION"))
            .with_description("Portable gain with sample-timed automation and 5 ms smoothing")
            .with_features([AUDIO_EFFECT, UTILITY, MONO, STEREO])
    }
    fn new_shared(_host: HostSharedHandle<'_>) -> Result<GainShared, PluginError> {
        Ok(GainShared {
            gain: SharedGain::new(),
            meters: GainMeterMailbox::default(),
            edits: editor::Edits::default(),
            fault: AtomicU32::new(0),
        })
    }
    fn new_main_thread<'a>(
        host: HostMainThreadHandle<'a>,
        shared: &'a GainShared,
    ) -> Result<GainMain<'a>, PluginError> {
        Ok(GainMain {
            parameters: ClapParameterRegistration::new(parameter_schema(), &PARAMETER_OPTIONS)
                .map_err(|_| PluginError::Message("invalid gain parameter registration"))?,
            host_params: host.get_extension(),
            host,
            shared,
            channels: Cell::new(2),
            events: RefCell::new(ParameterEvents::new()),
            #[cfg(all(target_os = "macos", feature = "native-editor"))]
            gui: RefCell::new(None),
        })
    }
}

pub struct GainShared {
    gain: SharedGain,
    meters: GainMeterMailbox,
    edits: editor::Edits,
    fault: AtomicU32,
}
impl PluginShared<'_> for GainShared {}
impl GainShared {
    /// Latest coalesced block telemetry, or None during overlapping publication.
    /// This editor-side API is not a standard CLAP meter extension.
    pub fn meters(&self) -> Option<GainMeters> {
        self.meters.try_read()
    }
    fn parameters(&self) -> ravet_foundation::ParameterState<'static, 2> {
        let snapshot = self.gain.snapshot();
        parameter_schema()
            .state([
                SharedGain::gain(snapshot),
                f32::from(SharedGain::bypassed(snapshot)),
            ])
            .expect("shared parameters are valid")
    }
    /// Zero after a successful block/flush, one after a rejected operation.
    /// This is a control-side diagnostic, not a standard CLAP extension.
    pub fn fault_code(&self) -> u32 {
        self.fault.load(Ordering::Relaxed)
    }
}

pub struct GainMain<'a> {
    parameters: ClapParameterRegistration<'static, 2>,
    host: HostMainThreadHandle<'a>,
    host_params: Option<HostParams>,
    shared: &'a GainShared,
    channels: Cell<u32>,
    events: RefCell<ParameterEvents>,
    #[cfg(all(target_os = "macos", feature = "native-editor"))]
    gui: RefCell<Option<gui::GuiState<'a>>>,
}
impl<'a> PluginMainThread<'a, GainShared> for GainMain<'a> {}

fn queue_edit(
    shared: &GainShared,
    id: ravet_foundation::ParameterId,
    value: f32,
) -> Result<(), EditError> {
    let schema = parameter_schema();
    let index = schema.index(id).ok_or(EditError::UnknownParameter)?;
    let value = schema.descriptors()[index]
        .range
        .canonicalize(value)
        .ok_or(EditError::InvalidValue)?;
    shared.edits.push(
        ClapId::new(id.0),
        value,
        SharedGain::generation(shared.gain.snapshot()),
    )
}

impl ravet_surface::ParameterControl for GainMain<'_> {
    type Error = EditError;
    fn commit(&self, id: ravet_foundation::ParameterId, physical: f32) -> Result<(), EditError> {
        self.edit_parameter(id, physical)
    }
    fn begin(&self, id: ravet_foundation::ParameterId) -> Result<(), EditError> {
        self.begin_parameter_gesture(id)
    }
    fn update(&self, id: ravet_foundation::ParameterId, physical: f32) -> Result<(), EditError> {
        self.update_parameter_gesture(id, physical)
    }
    fn end(&self, _: ravet_foundation::ParameterId) -> Result<(), EditError> {
        self.end_parameter_gesture()
    }
}
impl GainMain<'_> {
    /// Queue one complete begin/value/end editor gesture (capacity 32 edits).
    /// Finite physical values clamp/snap using the product schema. Applied only
    /// when CLAP accepts the value output event, with normal gain smoothing.
    /// Restores discard older queued values; queue overflow rejects the whole edit.
    pub fn edit_parameter(
        &self,
        id: ravet_foundation::ParameterId,
        value: f32,
    ) -> Result<(), EditError> {
        queue_edit(self.shared, id, value)?;
        self.request_edit_flush();
        Ok(())
    }
    /// Begin one continuous main-thread gesture. Reserves queue space for end.
    /// Complete edits and other begins are rejected until this gesture ends.
    pub fn begin_parameter_gesture(
        &self,
        id: ravet_foundation::ParameterId,
    ) -> Result<(), EditError> {
        if parameter_schema().index(id).is_none() {
            return Err(EditError::UnknownParameter);
        }
        self.shared.edits.begin(
            ClapId::new(id.0),
            SharedGain::generation(self.shared.gain.snapshot()),
        )?;
        self.request_edit_flush();
        Ok(())
    }
    /// Queue a live value for the open gesture; finite values clamp/snap.
    /// Overflow rejects this update but leaves reserved end capacity available.
    pub fn update_parameter_gesture(
        &self,
        id: ravet_foundation::ParameterId,
        value: f32,
    ) -> Result<(), EditError> {
        let schema = parameter_schema();
        let index = schema.index(id).ok_or(EditError::UnknownParameter)?;
        let value = schema.descriptors()[index]
            .range
            .canonicalize(value)
            .ok_or(EditError::InvalidValue)?;
        self.shared.edits.update(ClapId::new(id.0), value)?;
        self.request_edit_flush();
        Ok(())
    }
    /// End the open gesture, even when preceding value updates filled the queue.
    /// After restore, remaining updates from that gesture are discarded.
    pub fn end_parameter_gesture(&self) -> Result<(), EditError> {
        self.shared.edits.end()?;
        self.request_edit_flush();
        Ok(())
    }
    fn request_edit_flush(&self) {
        if let Some(params) = self.host_params {
            params.request_flush(&self.host);
        } else {
            self.host.request_process();
        }
    }
    /// Coherent requested gain/bypass for an editor; audio telemetry is separate.
    pub fn parameter_state(&self) -> ravet_foundation::ParameterState<'static, 2> {
        self.shared.parameters()
    }
    /// One coalesced audio-meter read; retain the previous display on None.
    pub fn meters(&self) -> Option<GainMeters> {
        self.shared.meters()
    }
}

impl PluginStateImpl for GainMain<'_> {
    fn save(&self, output: &mut clack_plugin::stream::OutputStream) -> Result<(), PluginError> {
        write_parameters(&self.shared.parameters(), output)?;
        Ok(())
    }
    fn load(&self, input: &mut clack_plugin::stream::InputStream) -> Result<(), PluginError> {
        let state = read_parameters(input)?;
        self.shared
            .gain
            .restore(state.values()[0], state.values()[1] == 1.0);
        if let Some(params) = self.host_params {
            params.rescan(&self.host, ParamRescanFlags::VALUES);
        }
        self.host.request_process();
        Ok(())
    }
}

fn port_info(channels: u32, is_input: bool) -> AudioPortInfo<'static> {
    AudioPortInfo {
        id: ClapId::new(0),
        name: if is_input {
            b"Main Input"
        } else {
            b"Main Output"
        },
        channel_count: channels,
        flags: AudioPortFlags::IS_MAIN,
        port_type: Some(if channels == 1 {
            AudioPortType::MONO
        } else {
            AudioPortType::STEREO
        }),
        in_place_pair: Some(ClapId::new(0)),
    }
}
impl PluginAudioPortsImpl for GainMain<'_> {
    fn count(&self, _is_input: bool) -> u32 {
        1
    }
    fn get(&self, index: u32, is_input: bool, writer: &mut AudioPortInfoWriter) {
        if index == 0 {
            writer.set(&port_info(self.channels.get(), is_input));
        }
    }
}
impl PluginAudioPortsConfigImpl for GainMain<'_> {
    fn count(&self) -> u32 {
        2
    }
    fn get(&self, index: u32, writer: &mut AudioPortConfigWriter) {
        if index > 1 {
            return;
        }
        let channels = index + 1;
        let port = MainPortInfo {
            channel_count: channels,
            port_type: Some(if channels == 1 {
                AudioPortType::MONO
            } else {
                AudioPortType::STEREO
            }),
        };
        writer.write(&AudioPortsConfiguration {
            id: ClapId::new(channels),
            name: if channels == 1 { b"Mono" } else { b"Stereo" },
            input_port_count: 1,
            output_port_count: 1,
            main_input: Some(port),
            main_output: Some(port),
        });
    }
    fn select(&self, id: ClapId) -> Result<(), PluginError> {
        if !(1..=2).contains(&id.get()) {
            return Err(PluginError::Message(
                "unsupported gain channel configuration",
            ));
        }
        self.channels.set(id.get());
        Ok(())
    }
}
impl PluginAudioPortsConfigInfoImpl for GainMain<'_> {
    fn current_config(&self) -> Option<ClapId> {
        Some(ClapId::new(self.channels.get()))
    }
    fn get(&self, config: ClapId, index: u32, is_input: bool, writer: &mut AudioPortInfoWriter) {
        if index == 0 && (1..=2).contains(&config.get()) {
            writer.set(&port_info(config.get(), is_input));
        }
    }
}
impl PluginMainThreadParams for GainMain<'_> {
    fn count(&self) -> u32 {
        self.parameters.count()
    }
    fn get_info(&self, index: u32, writer: &mut ParamInfoWriter) {
        if let Some(info) = self.parameters.info(index) {
            writer.set(&info);
        }
    }
    fn get_value(&self, id: ClapId) -> Option<f64> {
        let snapshot = self.shared.gain.snapshot();
        match id {
            CLAP_GAIN_ID => Some(SharedGain::gain(snapshot) as f64),
            CLAP_BYPASS_ID => Some(f64::from(SharedGain::bypassed(snapshot))),
            _ => None,
        }
    }
    fn value_to_text(
        &self,
        id: ClapId,
        value: f64,
        writer: &mut ParamDisplayWriter,
    ) -> std::fmt::Result {
        if !value.is_finite() {
            return Err(std::fmt::Error);
        }
        if id == CLAP_BYPASS_ID {
            return writer.write_str(if value >= 0.5 { "On" } else { "Off" });
        }
        if id != CLAP_GAIN_ID {
            return Err(std::fmt::Error);
        }
        write!(writer, "{value:.3} x")
    }
    fn text_to_value(&self, id: ClapId, text: &CStr) -> Option<f64> {
        let text = text.to_str().ok()?.trim();
        if id == CLAP_BYPASS_ID {
            return if text.eq_ignore_ascii_case("on") || text == "1" {
                Some(1.0)
            } else if text.eq_ignore_ascii_case("off") || text == "0" {
                Some(0.0)
            } else {
                None
            };
        }
        if id != CLAP_GAIN_ID {
            return None;
        }
        let value: f64 = text.strip_suffix('x').unwrap_or(text).trim().parse().ok()?;
        (value.is_finite() && (0.0..=MAXIMUM_GAIN as f64).contains(&value)).then_some(value)
    }
    fn flush(&self, input: &InputEvents, output: &mut OutputEvents) {
        let mut scratch = self.events.borrow_mut();
        match scratch.translate(input, 0, true) {
            Ok(events) => {
                let snapshot = self.shared.gain.snapshot();
                let mut gain = SharedGain::gain(snapshot);
                let mut bypass = SharedGain::bypassed(snapshot);
                let mut changed = false;
                for event in events {
                    match event.data {
                        EventData::ParameterValue { id: GAIN_ID, value } => {
                            gain = value.clamp(0.0, MAXIMUM_GAIN);
                            changed = true;
                        }
                        EventData::ParameterValue {
                            id: BYPASS_ID,
                            value,
                        } => {
                            bypass = value >= 0.5;
                            changed = true;
                        }
                        _ => (),
                    }
                }
                if changed {
                    self.shared.gain.restore(gain, bypass);
                }
                self.shared.fault.store(0, Ordering::Relaxed);
            }
            Err(_) => self.shared.fault.store(1, Ordering::Relaxed),
        }
        let snapshot = self.shared.gain.snapshot();
        let mut gain = SharedGain::gain(snapshot);
        let mut bypass = SharedGain::bypassed(snapshot);
        let pending = self.shared.edits.drain(
            output,
            SharedGain::generation(snapshot),
            || SharedGain::generation(self.shared.gain.snapshot()),
            |id, value| {
                if id == CLAP_GAIN_ID {
                    gain = value;
                } else {
                    bypass = value == 1.0;
                }
            },
        );
        self.shared.gain.reflect(snapshot, gain, bypass);
        if pending {
            self.request_edit_flush();
        }
    }
}

pub struct GainAudio<'a> {
    host: HostAudioProcessorHandle<'a>,
    host_params: Option<HostParams>,
    shared: &'a GainShared,
    bridge: EffectBridge<GainProcessor>,
    generation: u32,
}
impl GainAudio<'_> {
    fn editor_events(&mut self, snapshot: u64, output: &mut OutputEvents<'_>) -> u64 {
        let shared = self.shared;
        let bridge = &mut self.bridge;
        let pending = shared.edits.drain(
            output,
            SharedGain::generation(snapshot),
            || SharedGain::generation(shared.gain.snapshot()),
            |id, value| {
                let edit = [ParamValueEvent::new(0, id, Pckn::match_all(), value as f64)];
                let result = bridge.flush(&InputEvents::from_buffer(&edit));
                debug_assert!(result.is_ok());
            },
        );
        if pending {
            if let Some(params) = self.host_params {
                params.request_flush(&self.host);
            } else {
                self.host.request_process();
            }
        }
        shared.gain.reflect(
            snapshot,
            bridge.processor().requested_gain(),
            bridge.processor().bypassed(),
        )
    }

    fn synchronize(&mut self) -> u64 {
        let snapshot = self.shared.gain.snapshot();
        let generation = SharedGain::generation(snapshot);
        if generation != self.generation {
            let initial = [
                ParamValueEvent::new(
                    0,
                    CLAP_GAIN_ID,
                    Pckn::match_all(),
                    SharedGain::gain(snapshot) as f64,
                ),
                ParamValueEvent::new(
                    0,
                    CLAP_BYPASS_ID,
                    Pckn::match_all(),
                    f64::from(SharedGain::bypassed(snapshot)),
                ),
            ];
            // The prepared bridge always accepts these two validated global values.
            let result = self.bridge.flush(&InputEvents::from_buffer(&initial));
            debug_assert!(result.is_ok());
            self.bridge.reset();
            self.generation = generation;
        }
        snapshot
    }
    fn reflect(&self, snapshot: u64, result: Result<(), BridgeError>) {
        if result.is_ok() {
            self.shared.gain.reflect(
                snapshot,
                self.bridge.processor().requested_gain(),
                self.bridge.processor().bypassed(),
            );
        }
        self.shared
            .fault
            .store(u32::from(result.is_err()), Ordering::Relaxed);
    }
}
impl<'a> PluginAudioProcessor<'a, GainShared, GainMain<'a>> for GainAudio<'a> {
    fn activate(
        host: HostAudioProcessorHandle<'a>,
        main: &GainMain<'a>,
        shared: &'a GainShared,
        config: PluginAudioConfiguration,
    ) -> Result<Self, PluginError> {
        let layout = if main.channels.get() == 1 {
            AudioLayout::MONO
        } else {
            AudioLayout::STEREO
        };
        let config = ProcessConfig::new(
            config.sample_rate,
            config.max_frames_count as usize,
            MAXIMUM_EVENTS,
            layout,
        )?;
        let mut bridge = EffectBridge::prepare(GainProcessor::default(), config).map_err(|_| {
            PluginError::Message(
                "gain activation exceeds bridge capacities or has invalid configuration",
            )
        })?;
        let snapshot = shared.gain.snapshot();
        let initial = [
            ParamValueEvent::new(
                0,
                CLAP_GAIN_ID,
                Pckn::match_all(),
                SharedGain::gain(snapshot) as f64,
            ),
            ParamValueEvent::new(
                0,
                CLAP_BYPASS_ID,
                Pckn::match_all(),
                f64::from(SharedGain::bypassed(snapshot)),
            ),
        ];
        bridge
            .flush(&InputEvents::from_buffer(&initial))
            .map_err(|_| PluginError::Message("failed to initialize gain"))?;
        bridge.reset();
        shared.fault.store(0, Ordering::Relaxed);
        let _ = shared.meters.try_publish(bridge.processor().meters());
        Ok(Self {
            host_params: host.get_extension(),
            host,
            shared,
            bridge,
            generation: SharedGain::generation(snapshot),
        })
    }
    fn process(
        &mut self,
        process: Process,
        mut audio: Audio,
        events: Events,
    ) -> Result<ProcessStatus, PluginError> {
        let snapshot = self.synchronize();
        let snapshot = self.editor_events(snapshot, events.output);
        let result = self
            .bridge
            .process_with_transport(&mut audio, events.input, process.transport)
            .map(|_| ());
        self.reflect(snapshot, result);
        if result.is_ok() {
            let _ = self
                .shared
                .meters
                .try_publish(self.bridge.processor().meters());
        }
        // Clack logs PluginError at the ABI boundary. Keep rejected blocks
        // silent and publish a diagnostic rather than perform callback I/O.
        Ok(ProcessStatus::Continue)
    }
    fn reset(&mut self) {
        self.synchronize();
        self.bridge.reset();
        let _ = self
            .shared
            .meters
            .try_publish(self.bridge.processor().meters());
    }
    fn deactivate(mut self, _main: &GainMain<'a>) {
        self.bridge.deactivate();
    }
}
impl PluginAudioProcessorParams for GainAudio<'_> {
    fn flush(&mut self, input: &InputEvents, output: &mut OutputEvents) {
        let snapshot = self.synchronize();
        let snapshot = self.editor_events(snapshot, output);
        let result = self.bridge.flush(input);
        self.reflect(snapshot, result);
    }
}

// This module owns the single exported ABI symbol. Clack's macro contains
// no_mangle and its foreign entry callbacks; no raw pointers enter DSP code.
#[allow(unsafe_code)]
mod export {
    clack_plugin::clack_export_entry!(clack_plugin::entry::SinglePluginEntry<super::GainPlugin>);
}

#[cfg(test)]
#[allow(unsafe_code)]
mod control_tests;
