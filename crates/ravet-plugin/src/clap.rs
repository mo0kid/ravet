//! Generic CLAP export of any [`PluginExport`].
//!
//! Use [`crate::export_plugin!`] rather than naming these types. Everything the
//! host sees comes from the product description: descriptor, audio and note
//! ports, parameters, saved state, latency and tail.
//!
//! Current limits: the first declared layout is used, processing is f32, note
//! output and product extra state are not exported, and there is no editor yet.
use crate::{
    Category, ParameterRole, ResolvedProduct,
    export::{Edit, EditQueue, ParameterCache, PluginExport},
    state::{self, SavedState, StateError, StateHooks},
};
use clack_extensions::{
    audio_ports::{
        AudioPortFlags, AudioPortInfo, AudioPortInfoWriter, AudioPortType, PluginAudioPorts,
        PluginAudioPortsImpl,
    },
    latency::{HostLatency, PluginLatency, PluginLatencyImpl},
    note_ports::{
        NoteDialect, NoteDialects, NotePortInfo, NotePortInfoWriter, PluginNotePorts,
        PluginNotePortsImpl,
    },
    params::{
        HostParams, ParamDisplayWriter, ParamInfo, ParamInfoFlags, ParamInfoWriter,
        ParamRescanFlags, PluginAudioProcessorParams, PluginMainThreadParams, PluginParams,
    },
    state::{PluginState, PluginStateImpl},
    tail::{PluginTail, PluginTailImpl, TailLength},
};
use clack_plugin::{
    events::{
        UnknownEvent,
        event_types::{ParamGestureBeginEvent, ParamGestureEndEvent, ParamValueEvent},
        io::EventBuffer,
    },
    prelude::*,
    utils::Cookie,
};
use ravet_clap::{MAXIMUM_EVENTS, MainBusBridge};
use ravet_foundation::{ParameterScale, ProcessConfig, Processor as _, Tail};
use ravet_native_dsp::{DenormalMode, DenormalProcessor};
use std::{
    ffi::CStr,
    fmt::Write as _,
    io::{Read, Write as _},
    marker::PhantomData,
    sync::atomic::{AtomicBool, AtomicU32, Ordering},
};

/// Largest state accepted from a host: the envelope's parameter and extra limits.
const MAXIMUM_STATE_BYTES: u64 =
    28 + 8 * state::MAXIMUM_SAVED_PARAMETERS as u64 + 4 + state::MAXIMUM_EXTRA_BYTES as u64;

/// The CLAP plug-in type for processor `P`.
pub struct ClapPlugin<P>(PhantomData<fn() -> P>);

impl<P: PluginExport> Plugin for ClapPlugin<P> {
    type AudioProcessor<'a> = ClapAudio<'a, P>;
    type Shared<'a> = ClapShared;
    type MainThread<'a> = ClapMain<'a>;
    fn declare_extensions(builder: &mut PluginExtensions<Self>, _: Option<&ClapShared>) {
        builder
            .register::<PluginAudioPorts>()
            .register::<PluginNotePorts>()
            .register::<PluginParams>()
            .register::<PluginState>()
            .register::<PluginLatency>()
            .register::<PluginTail>();
        #[cfg(target_os = "macos")]
        if P::PRODUCT.editor().is_some() {
            builder.register::<clack_extensions::gui::PluginGui>();
        }
    }
}

impl<P: PluginExport> DefaultPluginFactory for ClapPlugin<P> {
    fn get_descriptor() -> PluginDescriptor {
        use clack_plugin::plugin::features::{
            AUDIO_EFFECT, INSTRUMENT, MONO, NOTE_EFFECT, STEREO, SYNTHESIZER,
        };
        let product = P::PRODUCT;
        let id = product
            .identity_overrides()
            .clap_id
            .unwrap_or(product.plugin_id());
        let mut features = match product.category() {
            Category::Effect => vec![AUDIO_EFFECT],
            Category::Instrument => vec![INSTRUMENT, SYNTHESIZER],
            Category::MidiEffect => vec![NOTE_EFFECT],
        };
        if let Some(layout) = product.layouts().first() {
            match layout.output_channels() {
                1 => features.push(MONO),
                2 => features.push(STEREO),
                _ => {}
            }
        }
        let descriptor = PluginDescriptor::new(id, product.name())
            .with_vendor(product.vendor())
            .with_version(product.version())
            .with_features(features);
        if product.url().is_empty() {
            descriptor
        } else {
            descriptor.with_url(product.url())
        }
    }
    fn new_shared(_: HostSharedHandle<'_>) -> Result<ClapShared, PluginError> {
        let product = P::PRODUCT
            .resolve()
            .map_err(|_| PluginError::Message("invalid RAVET product description"))?;
        if product.product().notes().output {
            return Err(PluginError::Message("CLAP note output is not exported yet"));
        }
        if product.product().precision() == crate::Precision::F64 {
            return Err(PluginError::Message("CLAP export processes f32"));
        }
        let parameters = ParameterCache::new(&product);
        Ok(ClapShared {
            product,
            parameters,
            hooks: FnHooks {
                migrate: P::migrate_state,
                import: P::import_state,
            },
            edits: EditQueue::new(),
            editor_message: P::editor_message,
            latency: AtomicU32::new(0),
            restart: AtomicBool::new(false),
            fault: AtomicBool::new(false),
        })
    }
    fn new_main_thread<'a>(
        host: HostMainThreadHandle<'a>,
        shared: &'a ClapShared,
    ) -> Result<ClapMain<'a>, PluginError> {
        Ok(ClapMain {
            host,
            shared,
            params: host.get_extension::<HostParams>(),
            reported_latency: AtomicU32::new(0),
            #[cfg(target_os = "macos")]
            gui: std::cell::RefCell::new(None),
        })
    }
}

/// State shared by every thread of one plug-in instance.
pub struct ClapShared {
    product: ResolvedProduct<'static>,
    parameters: ParameterCache,
    hooks: FnHooks,
    /// Editor edits waiting to be delivered to the host and processor.
    edits: EditQueue,
    editor_message: fn(&serde_json::Value) -> Option<serde_json::Value>,
    /// Latency of the active processor.
    latency: AtomicU32,
    /// The processor's latency changed while active; the main thread requests a restart.
    restart: AtomicBool,
    /// The last processing call failed.
    fault: AtomicBool,
}
impl PluginShared<'_> for ClapShared {}

/// The product's state hooks, captured without making shared state generic.
struct FnHooks {
    migrate: fn(&mut SavedState) -> Result<(), StateError>,
    import: fn(&[u8]) -> Option<SavedState>,
}
impl StateHooks for FnHooks {
    fn migrate(&self, saved: &mut SavedState) -> Result<(), StateError> {
        (self.migrate)(saved)
    }
    fn import(&self, bytes: &[u8]) -> Option<SavedState> {
        (self.import)(bytes)
    }
}
impl ClapShared {
    pub fn product(&self) -> &ResolvedProduct<'static> {
        &self.product
    }
    pub fn parameters(&self) -> &ParameterCache {
        &self.parameters
    }
    pub(crate) fn edit_queue(&self) -> &EditQueue {
        &self.edits
    }
    pub(crate) fn handle_editor_message(
        &self,
        data: &serde_json::Value,
    ) -> Option<serde_json::Value> {
        (self.editor_message)(data)
    }
    /// Whether the last processing call failed.
    pub fn faulted(&self) -> bool {
        self.fault.load(Ordering::Relaxed)
    }
    /// Deliver queued editor edits: gesture and value events to the host, and
    /// values to the processor at the start of `events`. Stops early when the
    /// host's output queue is full; the rest are delivered next time.
    fn deliver_edits(&self, output: &mut OutputEvents, events: &mut EventBuffer) {
        while let Some(edit) = self.edits.peek() {
            let id = ClapId::new(self.product.parameter_ids()[edit.index()].0);
            let delivered = match edit {
                Edit::Begin(_) => output.try_push(ParamGestureBeginEvent::new(0, id)).is_ok(),
                Edit::End(_) => output.try_push(ParamGestureEndEvent::new(0, id)).is_ok(),
                Edit::Value(_, value) => {
                    let event = ParamValueEvent::new(0, id, Pckn::match_all(), f64::from(value));
                    let ok = output.try_push(event).is_ok();
                    if ok {
                        events.push(&event);
                    }
                    ok
                }
            };
            if !delivered {
                break;
            }
            self.edits.pop();
        }
    }
    fn index(&self, id: ClapId) -> Option<usize> {
        self.product
            .parameter_index(ravet_foundation::ParameterId(id.get()))
    }
    /// Canonical value for a global parameter event, if it targets a known
    /// parameter and the value is finite.
    fn canonical(&self, id: ClapId, value: f64) -> Option<(usize, f32)> {
        let index = self.index(id)?;
        let parameter = &self.product.product().parameters()[index];
        Some((index, parameter.range().canonicalize(value as f32)?))
    }
}

/// Main-thread side of one plug-in instance.
pub struct ClapMain<'a> {
    host: HostMainThreadHandle<'a>,
    shared: &'a ClapShared,
    params: Option<HostParams>,
    #[cfg(target_os = "macos")]
    gui: std::cell::RefCell<Option<crate::clap_gui::GuiState<'a>>>,
    /// Latency last reported to the host.
    reported_latency: AtomicU32,
}

impl<'a> PluginMainThread<'a, ClapShared> for ClapMain<'a> {
    fn on_main_thread(&self) {
        if self.shared.restart.swap(false, Ordering::AcqRel) {
            self.host.request_restart();
        }
    }
}

impl<'a> ClapMain<'a> {
    pub(crate) fn shared(&self) -> &'a ClapShared {
        self.shared
    }
    #[cfg(target_os = "macos")]
    pub(crate) fn host(&self) -> &HostMainThreadHandle<'a> {
        &self.host
    }
    #[cfg(target_os = "macos")]
    pub(crate) fn host_params(&self) -> Option<HostParams> {
        self.params
    }
    #[cfg(target_os = "macos")]
    pub(crate) fn gui(&self) -> &std::cell::RefCell<Option<crate::clap_gui::GuiState<'a>>> {
        &self.gui
    }
    /// Ask the host to deliver queued editor edits.
    pub(crate) fn request_flush(host: &HostMainThreadHandle<'_>, params: Option<HostParams>) {
        match params {
            Some(params) => params.request_flush(host),
            None => host.request_process(),
        }
    }
    fn rescan_values(&self) {
        if let Some(params) = self.host.get_extension::<HostParams>() {
            params.rescan(&self.host, ParamRescanFlags::VALUES);
        }
    }
}

fn port_type(channels: usize) -> Option<AudioPortType<'static>> {
    match channels {
        1 => Some(AudioPortType::MONO),
        2 => Some(AudioPortType::STEREO),
        _ => None,
    }
}

impl PluginAudioPortsImpl for ClapMain<'_> {
    fn count(&self, input: bool) -> u32 {
        let layout = self.shared.product.product().layouts()[0];
        let channels = if input {
            layout.input_channels()
        } else {
            layout.output_channels()
        };
        u32::from(channels > 0)
    }
    fn get(&self, index: u32, input: bool, writer: &mut AudioPortInfoWriter) {
        let layout = self.shared.product.product().layouts()[0];
        let channels = if input {
            layout.input_channels()
        } else {
            layout.output_channels()
        };
        if index != 0 || channels == 0 {
            return;
        }
        writer.set(&AudioPortInfo {
            id: ClapId::new(0),
            name: if input { b"Input" } else { b"Output" },
            channel_count: channels as u32,
            flags: AudioPortFlags::IS_MAIN,
            port_type: port_type(channels),
            in_place_pair: None,
        });
    }
}

impl PluginNotePortsImpl for ClapMain<'_> {
    fn count(&self, input: bool) -> u32 {
        u32::from(input && self.shared.product.product().notes().input)
    }
    fn get(&self, index: u32, input: bool, writer: &mut NotePortInfoWriter) {
        if input && index == 0 && self.shared.product.product().notes().input {
            writer.set(&NotePortInfo {
                id: ClapId::new(0),
                name: b"MIDI input",
                supported_dialects: NoteDialects::MIDI,
                preferred_dialect: Some(NoteDialect::Midi),
            });
        }
    }
}

/// CLAP marks a parameter stepped only when every step is an integer.
fn integer_steps(range: ravet_foundation::ParameterRange) -> bool {
    range.intervals().is_some_and(|steps| {
        range.minimum().fract() == 0.0
            && range.maximum().fract() == 0.0
            && range.maximum() - range.minimum() == f32::from(steps)
    })
}

impl PluginMainThreadParams for ClapMain<'_> {
    fn count(&self) -> u32 {
        self.shared.parameters.len() as u32
    }
    fn get_info(&self, index: u32, writer: &mut ParamInfoWriter) {
        let index = index as usize;
        let Some(parameter) = self.shared.product.product().parameters().get(index) else {
            return;
        };
        let range = parameter.range();
        let mut flags = ParamInfoFlags::empty();
        if parameter.is_automatable() {
            flags |= ParamInfoFlags::IS_AUTOMATABLE;
        }
        if integer_steps(range) {
            flags |= ParamInfoFlags::IS_STEPPED;
        }
        if parameter.role() == ParameterRole::Bypass {
            flags |= ParamInfoFlags::IS_BYPASS | ParamInfoFlags::IS_STEPPED;
        }
        if parameter.is_hidden() {
            flags |= ParamInfoFlags::IS_HIDDEN;
        }
        if parameter.is_read_only() {
            flags |= ParamInfoFlags::IS_READONLY;
        }
        writer.set(&ParamInfo {
            id: ClapId::new(self.shared.product.parameter_ids()[index].0),
            flags,
            cookie: Cookie::empty(),
            name: parameter.name().as_bytes(),
            module: b"",
            min_value: f64::from(range.minimum()),
            max_value: f64::from(range.maximum()),
            default_value: f64::from(parameter.default()),
        });
    }
    fn get_value(&self, id: ClapId) -> Option<f64> {
        Some(f64::from(
            self.shared.parameters.get(self.shared.index(id)?),
        ))
    }
    fn value_to_text(
        &self,
        id: ClapId,
        value: f64,
        writer: &mut ParamDisplayWriter,
    ) -> std::fmt::Result {
        let index = self.shared.index(id).ok_or(std::fmt::Error)?;
        let parameter = &self.shared.product.product().parameters()[index];
        let value = parameter
            .range()
            .canonicalize(value as f32)
            .ok_or(std::fmt::Error)?;
        if parameter.role() == ParameterRole::Bypass {
            return write!(writer, "{}", if value >= 0.5 { "On" } else { "Off" });
        }
        let decimals = match (parameter.range().intervals(), parameter.range().scale()) {
            (Some(_), _) if value.fract() == 0.0 => 0,
            (_, ParameterScale::Logarithmic) if value.abs() >= 100.0 => 0,
            _ => 2,
        };
        write!(writer, "{value:.decimals$}")?;
        if !parameter.unit().is_empty() {
            write!(writer, " {}", parameter.unit())?;
        }
        Ok(())
    }
    fn text_to_value(&self, id: ClapId, text: &CStr) -> Option<f64> {
        let index = self.shared.index(id)?;
        let parameter = &self.shared.product.product().parameters()[index];
        let text = text.to_str().ok()?.trim();
        let number = text.strip_suffix(parameter.unit()).unwrap_or(text).trim();
        let value = match (parameter.role(), number) {
            (ParameterRole::Bypass, "On" | "on") => 1.0,
            (ParameterRole::Bypass, "Off" | "off") => 0.0,
            _ => number.parse::<f32>().ok()?,
        };
        parameter.range().canonicalize(value).map(f64::from)
    }
    fn flush(&self, input: &InputEvents, output: &mut OutputEvents) {
        // Inactive: editor values are already cached; report them to the host.
        let mut ignored = EventBuffer::new();
        self.shared.deliver_edits(output, &mut ignored);
        // Store host values; activation sends the whole cache to the processor.
        for event in input {
            if let Some((index, value)) =
                global_value(event).and_then(|(id, value)| self.shared.canonical(id, value))
            {
                self.shared.parameters.set(index, value);
            }
        }
    }
}

/// The parameter ID and value of a global parameter-value event.
fn global_value(event: &UnknownEvent) -> Option<(ClapId, f64)> {
    if event.header().as_raw().space_id != 0 || event.header().type_id() != ParamValueEvent::TYPE_ID
    {
        return None;
    }
    let event = event.as_event::<ParamValueEvent>()?;
    let raw = event.as_raw();
    if (raw.port_index, raw.channel, raw.key, raw.note_id) != (-1, -1, -1, -1) {
        return None;
    }
    Some((event.param_id()?, event.value()))
}

impl PluginStateImpl for ClapMain<'_> {
    fn save(&self, output: &mut clack_plugin::stream::OutputStream) -> Result<(), PluginError> {
        let mut values = vec![0.0; self.shared.parameters.len()];
        self.shared.parameters.read_into(&mut values);
        let bytes = state::encode(&self.shared.product, &values, &[])
            .map_err(|_| PluginError::Message("RAVET state could not be encoded"))?;
        output.write_all(&bytes)?;
        Ok(())
    }
    fn load(&self, input: &mut clack_plugin::stream::InputStream) -> Result<(), PluginError> {
        let mut bytes = Vec::new();
        input
            .take(MAXIMUM_STATE_BYTES + 1)
            .read_to_end(&mut bytes)?;
        if bytes.len() as u64 > MAXIMUM_STATE_BYTES {
            return Err(PluginError::Message("RAVET state is too large"));
        }
        let restored = state::restore(&self.shared.product, &bytes, &self.shared.hooks)
            .map_err(|_| PluginError::Message("RAVET state was rejected"))?;
        self.shared.parameters.restore(&restored.values);
        self.host.request_process();
        self.rescan_values();
        Ok(())
    }
}

impl PluginLatencyImpl for ClapMain<'_> {
    fn get(&self) -> u32 {
        self.shared.latency.load(Ordering::Acquire)
    }
}

/// Audio-thread side of one active plug-in instance.
pub struct ClapAudio<'a, P: PluginExport> {
    host: HostAudioProcessorHandle<'a>,
    shared: &'a ClapShared,
    bridge: MainBusBridge<DenormalProcessor<P::Processor>>,
    /// Canonicalized copy of the host's input events.
    events: EventBuffer,
    generation: u64,
}

impl<P: PluginExport> ClapAudio<'_, P> {
    /// Resend every cached value to the processor after a restore or activation.
    fn synchronize(&mut self) {
        let generation = self.shared.parameters.generation();
        if generation == self.generation {
            return;
        }
        self.events.clear();
        for (index, id) in self.shared.product.parameter_ids().iter().enumerate() {
            self.events.push(&ParamValueEvent::new(
                0,
                ClapId::new(id.0),
                Pckn::match_all(),
                f64::from(self.shared.parameters.get(index)),
            ));
        }
        let result = self.bridge.flush(&self.events.as_input());
        debug_assert!(result.is_ok());
        self.bridge.reset();
        self.generation = generation;
    }
    /// Copy host events, snapping global parameter values to their ranges and
    /// recording them in the cache. Unknown parameters and nonfinite values are dropped.
    fn canonicalize(&mut self, input: &InputEvents) {
        for event in input {
            match global_value(event) {
                Some((id, value)) => {
                    if let Some((index, value)) = self.shared.canonical(id, value) {
                        self.shared.parameters.set(index, value);
                        let mut copy = *event
                            .as_event::<ParamValueEvent>()
                            .expect("parameter event");
                        copy.set_value(f64::from(value));
                        self.events.push(&copy);
                    }
                }
                None => self.events.push(event),
            }
        }
    }
    fn after_processing(&self, ok: bool) {
        self.shared.fault.store(!ok, Ordering::Relaxed);
        let latency = self.bridge.processor().latency_samples();
        if latency != self.shared.latency.load(Ordering::Relaxed)
            && !self.shared.restart.swap(true, Ordering::AcqRel)
        {
            self.host.request_callback();
        }
    }
}

impl<'a, P: PluginExport> PluginAudioProcessor<'a, ClapShared, ClapMain<'a>> for ClapAudio<'a, P> {
    fn activate(
        host: HostAudioProcessorHandle<'a>,
        main: &ClapMain<'a>,
        shared: &'a ClapShared,
        config: PluginAudioConfiguration,
    ) -> Result<Self, PluginError> {
        let product = shared.product.product();
        let config = ProcessConfig::new(
            config.sample_rate,
            config.max_frames_count as usize,
            MAXIMUM_EVENTS,
            product.layouts()[0],
        )?;
        let processor = DenormalProcessor::new(P::create(), DenormalMode::FlushBoth)
            .map_err(|_| PluginError::Message("denormal flushing is unavailable"))?;
        let note_port = product.notes().input.then_some(0);
        let bridge = MainBusBridge::prepare(processor, config, note_port)
            .map_err(|_| PluginError::Message("RAVET processor rejected activation"))?;
        let latency = bridge.processor().latency_samples();
        shared.latency.store(latency, Ordering::Release);
        shared.restart.store(false, Ordering::Release);
        if main.reported_latency.swap(latency, Ordering::AcqRel) != latency
            && let Some(host_latency) = main.host.get_extension::<HostLatency>()
        {
            host_latency.changed(&main.host);
        }
        let mut audio = Self {
            host,
            shared,
            bridge,
            events: EventBuffer::with_capacity(MAXIMUM_EVENTS),
            generation: shared.parameters.generation().wrapping_sub(1),
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
        self.synchronize();
        self.events.clear();
        self.shared.deliver_edits(events.output, &mut self.events);
        self.canonicalize(events.input);
        let result = self
            .bridge
            .process(&mut audio, &self.events.as_input(), process.transport);
        self.after_processing(result.is_ok());
        Ok(ProcessStatus::Continue)
    }
    fn reset(&mut self) {
        self.synchronize();
        self.bridge.reset();
    }
    fn deactivate(mut self, _: &ClapMain<'a>) {
        self.bridge.deactivate();
    }
}

impl<P: PluginExport> PluginAudioProcessorParams for ClapAudio<'_, P> {
    fn flush(&mut self, input: &InputEvents, output: &mut OutputEvents) {
        self.synchronize();
        self.events.clear();
        self.shared.deliver_edits(output, &mut self.events);
        self.canonicalize(input);
        let result = self.bridge.flush(&self.events.as_input());
        self.shared.fault.store(result.is_err(), Ordering::Relaxed);
    }
}

impl<P: PluginExport> PluginTailImpl for ClapAudio<'_, P> {
    fn get(&self) -> TailLength {
        match self.bridge.processor().tail() {
            Tail::None => TailLength::Finite(0),
            Tail::Samples(samples) => {
                TailLength::Finite(samples.min(u64::from(u32::MAX - 1)) as u32)
            }
            Tail::Infinite => TailLength::Infinite,
        }
    }
}
