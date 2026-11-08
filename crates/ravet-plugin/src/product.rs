//! Product and parameter descriptions shared by every export format.
use crate::identity::{Identities, IdentityOverrides};
use crate::policy::{AutomationPolicy, BypassPolicy};
use ravet_foundation::{AudioLayout, ParameterId, ParameterRange, ParameterScale};

/// Maximum parameters per product, matching the native table limit.
pub const MAXIMUM_PARAMETERS: usize = 1024;
/// Maximum length of a parameter key, matching the LV2 symbol limit used here.
pub const MAXIMUM_KEY_BYTES: usize = 64;

/// The host-facing role of a product.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Category {
    /// Processes audio input into audio output.
    Effect,
    /// Generates audio from notes or MIDI.
    Instrument,
    /// Transforms notes or MIDI; audio is passed through or absent.
    MidiEffect,
}

/// Processing precisions the product implements.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Precision {
    F32,
    F64,
    /// Both precisions; hosts choose.
    Both,
}

/// Note and MIDI ports the product exposes.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct NotePorts {
    pub input: bool,
    pub output: bool,
}

/// How a parameter behaves for hosts.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ParameterRole {
    /// An ordinary parameter.
    Normal,
    /// The host bypass control: a toggle where 1 means bypassed.
    Bypass,
}

/// One parameter, identified permanently by its key.
///
/// The key is a stable identifier of up to 64 bytes: an ASCII letter or
/// underscore followed by letters, digits or underscores. Formats use it as the
/// LV2 symbol and the editor protocol address. The numeric host ID is either
/// supplied with [`Parameter::with_id`], to keep sessions from an earlier
/// product loading, or derived from the key by [`derive_parameter_id`]. It never
/// depends on declaration order.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Parameter<'a> {
    key: &'a str,
    id: Option<ParameterId>,
    name: &'a str,
    unit: &'a str,
    range: ParameterRange,
    default: f32,
    role: ParameterRole,
    automatable: bool,
    hidden: bool,
    read_only: bool,
}

const fn range(
    minimum: f32,
    maximum: f32,
    scale: ParameterScale,
    steps: Option<u16>,
) -> ParameterRange {
    match ParameterRange::new(minimum, maximum, scale, steps) {
        Ok(range) => range,
        Err(_) => panic!("invalid parameter range"),
    }
}

impl<'a> Parameter<'a> {
    /// A parameter over an explicit range. Validation happens in [`Product::resolve`].
    pub const fn new(key: &'a str, name: &'a str, range: ParameterRange, default: f32) -> Self {
        Self {
            key,
            id: None,
            name,
            unit: "",
            range,
            default,
            role: ParameterRole::Normal,
            automatable: true,
            hidden: false,
            read_only: false,
        }
    }
    /// A continuous linear parameter.
    ///
    /// # Panics
    /// If the range is invalid (nonfinite, or minimum not below maximum). In a
    /// `const` product description, as recommended, this is a compile error
    /// rather than a run-time panic.
    pub const fn linear(
        key: &'a str,
        name: &'a str,
        minimum: f32,
        maximum: f32,
        default: f32,
    ) -> Self {
        Self::new(
            key,
            name,
            range(minimum, maximum, ParameterScale::Linear, None),
            default,
        )
    }
    /// A continuous logarithmic parameter with strictly positive bounds.
    ///
    /// # Panics
    /// If the range is invalid; a compile error in a `const` description.
    pub const fn logarithmic(
        key: &'a str,
        name: &'a str,
        minimum: f32,
        maximum: f32,
        default: f32,
    ) -> Self {
        Self::new(
            key,
            name,
            range(minimum, maximum, ParameterScale::Logarithmic, None),
            default,
        )
    }
    /// A stepped parameter with `steps + 1` equally spaced values.
    ///
    /// # Panics
    /// If the range or step count is invalid; a compile error in a `const`
    /// description.
    pub const fn stepped(
        key: &'a str,
        name: &'a str,
        minimum: f32,
        maximum: f32,
        steps: u16,
        default: f32,
    ) -> Self {
        Self::new(
            key,
            name,
            range(minimum, maximum, ParameterScale::Linear, Some(steps)),
            default,
        )
    }
    /// An on/off parameter with values 0 and 1.
    pub const fn toggle(key: &'a str, name: &'a str, default: bool) -> Self {
        Self::stepped(key, name, 0.0, 1.0, 1, if default { 1.0 } else { 0.0 })
    }
    /// The host bypass toggle, off by default.
    pub const fn bypass(key: &'a str, name: &'a str) -> Self {
        let mut parameter = Self::toggle(key, name, false);
        parameter.role = ParameterRole::Bypass;
        parameter
    }
    /// Use an explicit permanent host ID instead of the derived one.
    pub const fn with_id(mut self, id: u32) -> Self {
        self.id = Some(ParameterId(id));
        self
    }
    /// Display unit, for example "dB".
    pub const fn with_unit(mut self, unit: &'a str) -> Self {
        self.unit = unit;
        self
    }
    /// Hosts may not record automation for this parameter.
    pub const fn not_automatable(mut self) -> Self {
        self.automatable = false;
        self
    }
    /// Hidden from host-generated parameter lists.
    pub const fn hidden(mut self) -> Self {
        self.hidden = true;
        self
    }
    /// Written by the processor only, for example a meter or status value.
    pub const fn read_only(mut self) -> Self {
        self.read_only = true;
        self.automatable = false;
        self
    }
    pub fn key(&self) -> &'a str {
        self.key
    }
    pub fn name(&self) -> &'a str {
        self.name
    }
    pub fn unit(&self) -> &'a str {
        self.unit
    }
    pub fn range(&self) -> ParameterRange {
        self.range
    }
    pub fn default(&self) -> f32 {
        self.default
    }
    pub fn role(&self) -> ParameterRole {
        self.role
    }
    pub fn is_automatable(&self) -> bool {
        self.automatable
    }
    pub fn is_hidden(&self) -> bool {
        self.hidden
    }
    pub fn is_read_only(&self) -> bool {
        self.read_only
    }
    /// The explicit ID, if one was supplied.
    pub fn explicit_id(&self) -> Option<ParameterId> {
        self.id
    }
    /// The host ID: explicit, or derived from the key.
    pub fn id(&self) -> ParameterId {
        self.id.unwrap_or_else(|| derive_parameter_id(self.key))
    }
}

/// Derive a parameter ID from its key: 32-bit FNV-1a with the top bit cleared,
/// so every format can represent it. This derivation never changes.
pub fn derive_parameter_id(key: &str) -> ParameterId {
    let mut hash: u32 = 0x811c_9dc5;
    for byte in key.bytes() {
        hash ^= u32::from(byte);
        hash = hash.wrapping_mul(0x0100_0193);
    }
    ParameterId(hash & 0x7fff_ffff)
}

/// Format and version of the saved state, see [`crate::state`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct StateFormat {
    /// Eight bytes identifying the product's state; never reused by another product.
    pub tag: [u8; 8],
    /// Current product state version, starting at 1. Increase it when the
    /// meaning of saved values or extra data changes.
    pub version: u32,
}

/// Everything formats need to describe one plug-in.
///
/// Build it with [`Product::new`] and the `with_*` methods, usually as a
/// `const`, then call [`Product::resolve`] to validate it.
#[derive(Clone, Copy, Debug)]
pub struct Product<'a> {
    name: &'a str,
    vendor: &'a str,
    version: &'a str,
    plugin_id: &'a str,
    url: &'a str,
    category: Category,
    layouts: &'a [AudioLayout],
    auxiliary_buses: &'a [AuxiliaryBus<'a>],
    notes: NotePorts,
    precision: Precision,
    parameters: &'a [Parameter<'a>],
    state: StateFormat,
    identities: IdentityOverrides<'a>,
    editor: Option<Editor<'a>>,
    editor_assets: &'a [EditorAsset<'a>],
    automation: AutomationPolicy,
    bypass: BypassPolicy,
    maximum_latency: u32,
}

/// Most bytes in an [`AuxiliaryBus`] name.
pub const MAXIMUM_BUS_NAME_BYTES: usize = 64;

/// An audio bus in addition to the main bus, such as a sidechain input or an
/// extra output of a multi-output instrument.
///
/// A bus may have inputs, outputs or both (at most 64 channels each, at least
/// one channel in total). An input-only bus is a **sidechain**: formats that
/// distinguish sidechains (LV2, AAX) mark it as one. The processor receives
/// every auxiliary bus in declaration order in [`ProcessBlock::auxiliary`],
/// prepared with the same layouts through
/// [`ProcessConfig::with_auxiliary_buses`]. A bus the host leaves disconnected
/// or inactive carries silence, and its outputs are discarded.
///
/// [`ProcessBlock::auxiliary`]: ravet_foundation::ProcessBlock::auxiliary
/// [`ProcessConfig::with_auxiliary_buses`]: ravet_foundation::ProcessConfig::with_auxiliary_buses
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AuxiliaryBus<'a> {
    /// Shown by hosts: nonblank, at most [`MAXIMUM_BUS_NAME_BYTES`], no
    /// control characters, unique among the product's auxiliary buses.
    pub name: &'a str,
    pub inputs: usize,
    pub outputs: usize,
}

impl<'a> AuxiliaryBus<'a> {
    pub const fn new(name: &'a str, inputs: usize, outputs: usize) -> Self {
        Self {
            name,
            inputs,
            outputs,
        }
    }
    /// An input-only sidechain bus.
    pub const fn sidechain(name: &'a str, channels: usize) -> Self {
        Self::new(name, channels, 0)
    }
    /// An output-only bus, for example an extra instrument output.
    pub const fn output(name: &'a str, channels: usize) -> Self {
        Self::new(name, 0, channels)
    }
    /// Whether the bus is input-only.
    pub const fn is_sidechain(&self) -> bool {
        self.inputs > 0 && self.outputs == 0
    }
    /// The bus's channel counts as a processor layout; `None` when invalid.
    pub fn layout(&self) -> Option<AudioLayout> {
        AudioLayout::new(self.inputs, self.outputs).ok()
    }
}

/// A web editor: one HTML page and its size in logical pixels.
///
/// RAVET injects its editor client into the page, which talks to the plug-in
/// through the [editor protocol](crate::editor). The page is limited to 1 MiB.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Editor<'a> {
    pub html: &'a str,
    pub width: u32,
    pub height: u32,
    /// Hosts may enlarge the editor up to four times its size in each direction.
    pub resizable: bool,
}

/// Largest accepted editor page.
pub const MAXIMUM_EDITOR_HTML_BYTES: usize = 1 << 20;

/// A file the editor page loads by relative URL, embedded in the binary, for
/// example `EditorAsset::new("img/knob.png", include_bytes!("../editor/img/knob.png"))`.
///
/// With assets, the editor page is served from a local asset origin together
/// with them, so images, fonts, scripts and styles need not be inlined into the
/// 1 MiB page or sent through 64 KiB editor messages. Requests are answered from
/// memory: no file system or network access is involved.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct EditorAsset<'a> {
    /// Relative path: ASCII letters, digits and `._-/`, at most 512 bytes, no
    /// `.`, `..` or empty segment, no leading or trailing `/`. `index.html` is
    /// reserved for the page.
    pub path: &'a str,
    /// Content type such as `image/png`; empty infers it from the extension.
    pub mime: &'a str,
    pub data: &'a [u8],
}

impl<'a> EditorAsset<'a> {
    /// An asset whose content type follows from its extension.
    pub const fn new(path: &'a str, data: &'a [u8]) -> Self {
        Self {
            path,
            mime: "",
            data,
        }
    }
    fn is_valid(&self) -> bool {
        let path = self.path;
        !path.is_empty()
            && path.len() <= 512
            && path != "index.html"
            && path
                .split('/')
                .all(|segment| !segment.is_empty() && segment != "." && segment != "..")
            && path
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'_' | b'-' | b'/'))
            && self.mime.len() <= 127
            && self.mime.bytes().all(|b| (0x20..=0x7e).contains(&b))
            && self.data.len() <= MAXIMUM_EDITOR_ASSET_BYTES
    }
}

/// Most editor assets per product.
pub const MAXIMUM_EDITOR_ASSETS: usize = 4096;
/// Largest single editor asset (64 MiB).
pub const MAXIMUM_EDITOR_ASSET_BYTES: usize = 64 << 20;
/// Largest total size of a product's editor assets (256 MiB).
pub const MAXIMUM_EDITOR_ASSETS_TOTAL_BYTES: usize = 256 << 20;

impl<'a> Product<'a> {
    /// A product with a stereo layout (stereo output only for instruments), f32
    /// processing, no notes and no parameters.
    ///
    /// `version` has three numeric components. `plugin_id` is a reverse-domain
    /// identity such as `com.example.delay`; format identities derive from it.
    pub const fn new(
        name: &'a str,
        vendor: &'a str,
        version: &'a str,
        plugin_id: &'a str,
        category: Category,
        state: StateFormat,
    ) -> Self {
        Self {
            name,
            vendor,
            version,
            plugin_id,
            url: "",
            category,
            layouts: match category {
                Category::Instrument => &[AudioLayout::STEREO_INSTRUMENT],
                _ => &[AudioLayout::STEREO],
            },
            auxiliary_buses: &[],
            notes: NotePorts {
                input: matches!(category, Category::Instrument | Category::MidiEffect),
                output: matches!(category, Category::MidiEffect),
            },
            precision: Precision::F32,
            parameters: &[],
            state,
            identities: IdentityOverrides::NONE,
            editor: None,
            editor_assets: &[],
            automation: AutomationPolicy::SampleAccurate,
            bypass: BypassPolicy::Processor,
            maximum_latency: 0,
        }
    }
    /// Supported main-bus layouts, preferred first. Hosts may choose any of them.
    pub const fn with_layouts(mut self, layouts: &'a [AudioLayout]) -> Self {
        self.layouts = layouts;
        self
    }
    /// Audio buses beyond the main bus, in the order the processor receives
    /// them (at most [`MAXIMUM_AUXILIARY_BUSES`]); see [`AuxiliaryBus`]. Every
    /// main layout uses the same auxiliary buses.
    ///
    /// [`MAXIMUM_AUXILIARY_BUSES`]: ravet_foundation::MAXIMUM_AUXILIARY_BUSES
    pub const fn with_auxiliary_buses(mut self, buses: &'a [AuxiliaryBus<'a>]) -> Self {
        self.auxiliary_buses = buses;
        self
    }
    pub const fn with_notes(mut self, notes: NotePorts) -> Self {
        self.notes = notes;
        self
    }
    pub const fn with_precision(mut self, precision: Precision) -> Self {
        self.precision = precision;
        self
    }
    pub const fn with_parameters(mut self, parameters: &'a [Parameter<'a>]) -> Self {
        self.parameters = parameters;
        self
    }
    pub const fn with_url(mut self, url: &'a str) -> Self {
        self.url = url;
        self
    }
    /// Give the product a web editor.
    pub const fn with_editor(mut self, editor: Editor<'a>) -> Self {
        self.editor = Some(editor);
        self
    }
    /// Files the editor page loads by relative URL; see [`EditorAsset`].
    /// Requires [`Product::with_editor`].
    pub const fn with_editor_assets(mut self, assets: &'a [EditorAsset<'a>]) -> Self {
        self.editor_assets = assets;
        self
    }
    /// Keep format identities from an earlier product instead of deriving them.
    pub const fn with_identities(mut self, identities: IdentityOverrides<'a>) -> Self {
        self.identities = identities;
        self
    }
    /// How parameter changes reach the processor within a block; see
    /// [`AutomationPolicy`]. The default is sample-accurate.
    pub const fn with_automation(mut self, automation: AutomationPolicy) -> Self {
        self.automation = automation;
        self
    }
    /// How the host bypass parameter is applied; see [`BypassPolicy`]. The
    /// default passes it to the processor. Other policies require a
    /// [`Parameter::bypass`] parameter.
    pub const fn with_bypass(mut self, bypass: BypassPolicy) -> Self {
        self.bypass = bypass;
        self
    }
    /// The most latency, in samples, the processor can report while active
    /// (at most [`MAXIMUM_BYPASS_LATENCY`]). Bypass policies other than
    /// [`BypassPolicy::Processor`] size their latency-compensating dry delay
    /// for the larger of this and the latency reported after `prepare`;
    /// declare it when parameters or state can raise the latency after
    /// preparation. Unused with [`BypassPolicy::Processor`]. The default is 0.
    ///
    /// [`MAXIMUM_BYPASS_LATENCY`]: crate::policy::MAXIMUM_BYPASS_LATENCY
    pub const fn with_maximum_latency(mut self, samples: u32) -> Self {
        self.maximum_latency = samples;
        self
    }
    pub fn name(&self) -> &'a str {
        self.name
    }
    pub fn vendor(&self) -> &'a str {
        self.vendor
    }
    pub fn version(&self) -> &'a str {
        self.version
    }
    pub fn plugin_id(&self) -> &'a str {
        self.plugin_id
    }
    pub fn url(&self) -> &'a str {
        self.url
    }
    pub fn category(&self) -> Category {
        self.category
    }
    pub fn layouts(&self) -> &'a [AudioLayout] {
        self.layouts
    }
    pub fn auxiliary_buses(&self) -> &'a [AuxiliaryBus<'a>] {
        self.auxiliary_buses
    }
    /// The auxiliary buses' layouts, in order, for
    /// [`ProcessConfig::with_auxiliary_buses`](ravet_foundation::ProcessConfig::with_auxiliary_buses).
    /// Invalid buses are skipped; [`Product::resolve`] rejects them.
    pub fn auxiliary_layouts(&self) -> Vec<AudioLayout> {
        self.auxiliary_buses
            .iter()
            .filter_map(AuxiliaryBus::layout)
            .collect()
    }
    pub fn notes(&self) -> NotePorts {
        self.notes
    }
    pub fn precision(&self) -> Precision {
        self.precision
    }
    pub fn parameters(&self) -> &'a [Parameter<'a>] {
        self.parameters
    }
    pub fn state_format(&self) -> StateFormat {
        self.state
    }
    pub fn identity_overrides(&self) -> IdentityOverrides<'a> {
        self.identities
    }
    pub fn editor(&self) -> Option<Editor<'a>> {
        self.editor
    }
    pub fn editor_assets(&self) -> &'a [EditorAsset<'a>] {
        self.editor_assets
    }
    pub fn automation(&self) -> AutomationPolicy {
        self.automation
    }
    pub fn bypass_policy(&self) -> BypassPolicy {
        self.bypass
    }
    /// The declared maximum latency; see [`Product::with_maximum_latency`].
    pub fn maximum_latency(&self) -> u32 {
        self.maximum_latency
    }

    /// Validate the description and resolve parameter IDs and format identities.
    /// Allocates; call on a control thread.
    pub fn resolve(&self) -> Result<ResolvedProduct<'a>, ProductError> {
        for (field, text) in [("name", self.name), ("vendor", self.vendor)] {
            if text.trim().is_empty() || text.len() > 128 || text.chars().any(char::is_control) {
                return Err(ProductError::InvalidText(field));
            }
        }
        if self.url.len() > 1024 || self.url.chars().any(char::is_control) {
            return Err(ProductError::InvalidText("url"));
        }
        if !valid_version(self.version) {
            return Err(ProductError::InvalidVersion);
        }
        if !valid_plugin_id(self.plugin_id) {
            return Err(ProductError::InvalidPluginId);
        }
        if self.state.version == 0 {
            return Err(ProductError::InvalidStateFormat);
        }
        if let Some(editor) = self.editor
            && (editor.html.is_empty()
                || editor.html.len() > MAXIMUM_EDITOR_HTML_BYTES
                || !(64..=4096).contains(&editor.width)
                || !(64..=4096).contains(&editor.height))
        {
            return Err(ProductError::InvalidEditor);
        }
        if !self.editor_assets.is_empty() && self.editor.is_none()
            || self.editor_assets.len() > MAXIMUM_EDITOR_ASSETS
        {
            return Err(ProductError::InvalidEditorAsset(0));
        }
        let mut total = 0_usize;
        for (index, asset) in self.editor_assets.iter().enumerate() {
            total = total.saturating_add(asset.data.len());
            if !asset.is_valid()
                || total > MAXIMUM_EDITOR_ASSETS_TOTAL_BYTES
                || self.editor_assets[..index]
                    .iter()
                    .any(|other| other.path == asset.path)
            {
                return Err(ProductError::InvalidEditorAsset(index));
            }
        }
        if self.layouts.is_empty() {
            return Err(ProductError::NoLayouts);
        }
        for (index, layout) in self.layouts.iter().enumerate() {
            if self.layouts[..index].contains(layout) {
                return Err(ProductError::DuplicateLayout(*layout));
            }
            if self.category == Category::Effect && layout.input_channels() == 0 {
                return Err(ProductError::EffectWithoutInput);
            }
        }
        if self.auxiliary_buses.len() > ravet_foundation::MAXIMUM_AUXILIARY_BUSES {
            return Err(ProductError::TooManyAuxiliaryBuses);
        }
        for (index, bus) in self.auxiliary_buses.iter().enumerate() {
            if bus.layout().is_none()
                || bus.name.trim().is_empty()
                || bus.name.len() > MAXIMUM_BUS_NAME_BYTES
                || bus.name.chars().any(char::is_control)
                || self.auxiliary_buses[..index]
                    .iter()
                    .any(|other| other.name == bus.name)
            {
                return Err(ProductError::InvalidAuxiliaryBus(index));
            }
        }
        if self.parameters.len() > MAXIMUM_PARAMETERS {
            return Err(ProductError::TooManyParameters);
        }
        let mut ids = Vec::with_capacity(self.parameters.len());
        let mut bypass = None;
        for (index, parameter) in self.parameters.iter().enumerate() {
            if !valid_key(parameter.key) {
                return Err(ProductError::InvalidKey(index));
            }
            if parameter.name.trim().is_empty()
                || parameter.name.len() > 128
                || parameter.name.chars().any(char::is_control)
                || parameter.unit.len() > 32
                || parameter.unit.chars().any(char::is_control)
            {
                return Err(ProductError::InvalidParameterText(index));
            }
            if parameter.range.canonicalize(parameter.default) != Some(parameter.default) {
                return Err(ProductError::InvalidDefault(index));
            }
            if self.parameters[..index]
                .iter()
                .any(|p| p.key == parameter.key)
            {
                return Err(ProductError::DuplicateKey(index));
            }
            let id = parameter.id();
            if id.0 == u32::MAX {
                return Err(ProductError::InvalidId(index));
            }
            if let Some(other) = ids.iter().position(|existing| *existing == id) {
                return Err(ProductError::DuplicateId {
                    first: other,
                    second: index,
                });
            }
            if parameter.role == ParameterRole::Bypass {
                let range = parameter.range;
                if bypass.is_some()
                    || range.minimum() != 0.0
                    || range.maximum() != 1.0
                    || range.intervals() != Some(1)
                    || parameter.read_only
                {
                    return Err(ProductError::InvalidBypass(index));
                }
                bypass = Some(index);
            }
            ids.push(id);
        }
        match self.bypass {
            BypassPolicy::Processor => {}
            _ if bypass.is_none() => return Err(ProductError::BypassPolicyWithoutParameter),
            BypassPolicy::Crossfade { milliseconds }
                if !(milliseconds.is_finite()
                    && milliseconds > 0.0
                    && milliseconds <= crate::policy::MAXIMUM_CROSSFADE_MS) =>
            {
                return Err(ProductError::InvalidBypassPolicy);
            }
            _ if self.maximum_latency > crate::policy::MAXIMUM_BYPASS_LATENCY => {
                return Err(ProductError::InvalidBypassPolicy);
            }
            _ => {}
        }
        let identities = Identities::resolve(self)?;
        Ok(ResolvedProduct {
            product: *self,
            ids,
            bypass,
            identities,
        })
    }
}

/// A validated product with resolved parameter IDs and format identities.
#[derive(Clone, Debug)]
pub struct ResolvedProduct<'a> {
    product: Product<'a>,
    ids: Vec<ParameterId>,
    bypass: Option<usize>,
    identities: Identities,
}

impl<'a> ResolvedProduct<'a> {
    pub fn product(&self) -> &Product<'a> {
        &self.product
    }
    /// Host IDs in declaration order.
    pub fn parameter_ids(&self) -> &[ParameterId] {
        &self.ids
    }
    /// Declaration index of a host ID.
    pub fn parameter_index(&self, id: ParameterId) -> Option<usize> {
        self.ids.iter().position(|candidate| *candidate == id)
    }
    /// Declaration index of a parameter key.
    pub fn parameter_index_by_key(&self, key: &str) -> Option<usize> {
        self.product.parameters.iter().position(|p| p.key == key)
    }
    /// Declaration index of the bypass parameter, if the product has one.
    pub fn bypass_index(&self) -> Option<usize> {
        self.bypass
    }
    /// Default physical values in declaration order.
    pub fn defaults(&self) -> Vec<f32> {
        self.product.parameters.iter().map(|p| p.default).collect()
    }
    pub fn identities(&self) -> &Identities {
        &self.identities
    }
}

/// Reasons a product description is rejected. Indices refer to declaration order.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ProductError {
    InvalidText(&'static str),
    InvalidVersion,
    InvalidPluginId,
    InvalidStateFormat,
    NoLayouts,
    DuplicateLayout(AudioLayout),
    EffectWithoutInput,
    TooManyParameters,
    InvalidKey(usize),
    InvalidParameterText(usize),
    InvalidDefault(usize),
    DuplicateKey(usize),
    InvalidId(usize),
    /// Two parameters share a host ID; give one an explicit ID.
    DuplicateId {
        first: usize,
        second: usize,
    },
    InvalidBypass(usize),
    InvalidIdentity(&'static str),
    /// The editor page is empty or over 1 MiB, or its size is outside 64 to 4096.
    InvalidEditor,
    /// An editor asset has an invalid path or content type, repeats a path or
    /// exceeds the size limits; or there are assets without an editor, or more
    /// than [`MAXIMUM_EDITOR_ASSETS`] (both reported at index 0).
    InvalidEditorAsset(usize),
    /// A bypass policy other than [`BypassPolicy::Processor`] needs a
    /// [`Parameter::bypass`] parameter.
    BypassPolicyWithoutParameter,
    /// More than [`MAXIMUM_AUXILIARY_BUSES`](ravet_foundation::MAXIMUM_AUXILIARY_BUSES)
    /// auxiliary buses.
    TooManyAuxiliaryBuses,
    /// An auxiliary bus has no channels or more than 64 in a direction, or an
    /// invalid or repeated name.
    InvalidAuxiliaryBus(usize),
    /// A crossfade time is not finite or outside 0 (exclusive) to 1000 ms, or
    /// a bypass policy other than [`BypassPolicy::Processor`] is combined with
    /// a declared maximum latency above
    /// [`MAXIMUM_BYPASS_LATENCY`](crate::policy::MAXIMUM_BYPASS_LATENCY).
    InvalidBypassPolicy,
}
impl core::fmt::Display for ProductError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "product error: {self:?}")
    }
}
impl std::error::Error for ProductError {}

fn valid_version(version: &str) -> bool {
    let parts: Vec<_> = version.split('.').collect();
    parts.len() == 3
        && parts.iter().all(|part| {
            !part.is_empty()
                && part.len() <= 9
                && part.bytes().all(|c| c.is_ascii_digit())
                && (part.len() == 1 || !part.starts_with('0'))
        })
}

pub(crate) fn valid_plugin_id(id: &str) -> bool {
    let parts: Vec<_> = id.split('.').collect();
    id.len() <= 255
        && parts.len() >= 2
        && parts.iter().all(|part| {
            !part.is_empty()
                && part
                    .bytes()
                    .all(|c| c.is_ascii_alphanumeric() || c == b'-' || c == b'_')
        })
}

fn valid_key(key: &str) -> bool {
    let mut bytes = key.bytes();
    key.len() <= MAXIMUM_KEY_BYTES
        && bytes
            .next()
            .is_some_and(|c| c.is_ascii_alphabetic() || c == b'_')
        && bytes.all(|c| c.is_ascii_alphanumeric() || c == b'_')
}
