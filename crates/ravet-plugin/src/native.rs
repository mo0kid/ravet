//! The generic C table behind the VST3, AUv2, AAX and LV2 adapters.
//!
//! [`crate::export_plugin!`] exports `ravet_plugin_v1`, which returns the
//! [`RavetPluginEntryV1`] declared in `include/RavetPlugin.h` (feature
//! `native`). The adapters are written once against that header, so any
//! [`PluginExport`] reaches every format without product-specific C++. The
//! header is the contract; this module implements it:
//!
//! - **Static data.** Product description, per-format identities, layouts and
//!   parameter metadata are built once, on the first call of
//!   `ravet_plugin_v1`, and live as long as the library. An invalid product, or
//!   one that only implements f64 processing, has no table (`NULL`).
//! - **Parameters.** Text uses [`crate::text`], the same formatting as CLAP.
//!   Normalization uses the parameter's range (scale and steps).
//! - **Instances.** Each instance owns a [`ParameterCache`], an [`EditQueue`]
//!   for editor edits, and the product's processor wrapped in its
//!   [automation and bypass policies](crate::policy) and denormal flushing.
//!   Host-side changes (`set_value`) and editor edits mark the parameter dirty;
//!   the next `process` delivers dirty values at offset 0, ahead of the host's
//!   events. `load_state` replaces the cache; before its next block the
//!   processor receives every value in a zero-frame block, followed by a reset.
//! - **Processing.** Samples are 32-bit float. Native buses, events and
//!   transport are converted with [`ravet_native`]; host buffers are copied
//!   into prepared scratch, so aliasing hosts are safe. `process` and `reset`
//!   never allocate, lock or block. Host parameter values need not be exactly
//!   representable as `float`: they are clamped and snapped to the parameter's
//!   range, written to the cache, and events for unknown IDs or with nonfinite
//!   values are dropped.
//! - **Faults.** Every function catches panics and returns 2. After a panic,
//!   `process`, `reset` and `timing` return 2 (and `process` silences its
//!   outputs) until the next successful `prepare`, which recreates the
//!   processor; parameter, state and editor calls keep working.
//! - **Editor.** On macOS, `editor_open` attaches the product's web editor to a
//!   Cocoa `NSView` (`RAVET_PLUGIN_VIEW_COCOA`) with the same editor host as the
//!   CLAP export. Other view kinds, and every view kind on other platforms,
//!   return 1 in this release. The web view polls the cache itself (every
//!   50 ms), so `editor_idle` currently does nothing.
//!
//! The processor's zero-frame synchronization blocks discard any events the
//! processor generates in them.
use crate::{
    Category, ParameterRole, PluginExport, Precision, ResolvedProduct,
    export::{Edit, EditQueue, FnHooks, MAXIMUM_STATE_BYTES, ParameterCache},
    policy::{Wrapped, wrap},
    state,
};
use ravet_foundation::{
    Event, EventData, ParameterId, ParameterScale, ProcessConfig, ProcessStatus, Processor,
    Tail, Transport,
};
use ravet_native::{
    NativeAudioAdapter, NativeEventV1, NativePlanarBus, NativeProcessorBusV1,
    NativeProcessorConfigV1, NativeProcessorResultV1, NativeProcessorTimingV1, NativeTransportV1,
    encode_native_events, native_event_kind,
};
use std::{
    cell::UnsafeCell,
    ffi::{CStr, CString, c_char, c_void},
    marker::PhantomData,
    panic::{AssertUnwindSafe, catch_unwind},
    ptr, slice,
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering},
    },
};

/// Table version written to [`RavetPluginEntryV1::version`].
pub const VERSION: u32 = 1;
/// Most events the processor receives in one block: host events plus cached
/// changes. Cached changes that do not fit wait for the next block.
const MAXIMUM_EVENTS: usize = 1024;

/// `RavetPluginCategoryV1`.
pub mod category {
    pub const EFFECT: u32 = 0;
    pub const INSTRUMENT: u32 = 1;
    pub const MIDI_EFFECT: u32 = 2;
}
/// `RavetPluginParameterFlagsV1`.
pub mod parameter_flags {
    pub const AUTOMATABLE: u32 = 1;
    pub const BYPASS: u32 = 2;
    pub const READ_ONLY: u32 = 4;
    pub const HIDDEN: u32 = 8;
    pub const STEPPED: u32 = 16;
}
/// `RavetPluginEditKindV1`.
pub mod edit_kind {
    pub const BEGIN: u32 = 0;
    pub const VALUE: u32 = 1;
    pub const END: u32 = 2;
}
/// `RavetPluginViewKindV1`.
pub mod view_kind {
    pub const COCOA: u32 = 1;
    pub const WIN32: u32 = 2;
    pub const X11: u32 = 3;
}

/// `RavetPluginLayoutV1`.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct RavetPluginLayoutV1 {
    pub inputs: u32,
    pub outputs: u32,
    pub aax_type_id: u32,
    pub reserved: u32,
}

/// `RavetPluginProductV1`.
#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct RavetPluginProductV1 {
    pub name: *const c_char,
    pub vendor: *const c_char,
    pub version: *const c_char,
    pub plugin_id: *const c_char,
    pub url: *const c_char,
    pub category: u32,
    pub note_input: u32,
    pub note_output: u32,
    pub layout_count: u32,
    pub layouts: *const RavetPluginLayoutV1,
    pub parameter_count: u32,
    pub bypass_index: i32,
    pub clap_id: *const c_char,
    pub vst3_processor_cid: [u32; 4],
    pub vst3_controller_cid: [u32; 4],
    pub au_type: u32,
    pub au_subtype: u32,
    pub au_manufacturer: u32,
    pub aax_manufacturer: u32,
    pub aax_product: u32,
    pub lv2_uri: *const c_char,
    pub editor_width: u32,
    pub editor_height: u32,
    pub editor_resizable: u32,
    pub reserved: u32,
}

/// `RavetPluginParameterV1`.
#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct RavetPluginParameterV1 {
    pub id: u32,
    pub flags: u32,
    pub scale: u32,
    pub steps: u32,
    pub minimum: f64,
    pub maximum: f64,
    pub default_value: f64,
    pub key: *const c_char,
    pub name: *const c_char,
    pub unit: *const c_char,
}

/// `RavetPluginEditV1`.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct RavetPluginEditV1 {
    pub kind: u32,
    pub index: u32,
    pub value: f64,
}

/// `RavetPluginEntryV1`: the table returned by `ravet_plugin_v1`. See
/// `include/RavetPlugin.h` for each function's contract.
#[repr(C)]
#[derive(Clone, Copy)]
pub struct RavetPluginEntryV1 {
    pub version: u32,
    pub byte_size: u32,
    pub product: *const RavetPluginProductV1,
    pub parameter: unsafe extern "C" fn(u32, *mut RavetPluginParameterV1) -> i32,
    pub parameter_index: extern "C" fn(u32) -> i32,
    pub value_to_text: unsafe extern "C" fn(u32, f64, *mut c_char, u32) -> i32,
    pub text_to_value: unsafe extern "C" fn(u32, *const c_char, *mut f64) -> i32,
    pub normalize: unsafe extern "C" fn(u32, f64, *mut f64) -> i32,
    pub denormalize: unsafe extern "C" fn(u32, f64, *mut f64) -> i32,
    pub create: extern "C" fn() -> *mut c_void,
    pub destroy: unsafe extern "C" fn(*mut c_void),
    pub get_value: unsafe extern "C" fn(*const c_void, u32) -> f64,
    pub set_value: unsafe extern "C" fn(*mut c_void, u32, f64) -> i32,
    pub save_state: unsafe extern "C" fn(*mut c_void, *mut u8, u32, *mut u32) -> i32,
    pub load_state: unsafe extern "C" fn(*mut c_void, *const u8, u32) -> i32,
    pub prepare: unsafe extern "C" fn(*mut c_void, *const NativeProcessorConfigV1) -> i32,
    #[allow(clippy::type_complexity)]
    pub process: unsafe extern "C" fn(
        *mut c_void,
        *const NativeProcessorBusV1,
        u32,
        u32,
        *const NativeEventV1,
        u32,
        *const NativeTransportV1,
        *mut NativeEventV1,
        u32,
        *mut NativeProcessorResultV1,
    ) -> i32,
    pub reset: unsafe extern "C" fn(*mut c_void) -> i32,
    pub deactivate: unsafe extern "C" fn(*mut c_void) -> i32,
    pub timing: unsafe extern "C" fn(*mut c_void, *mut NativeProcessorTimingV1) -> i32,
    pub latency_changed: unsafe extern "C" fn(*mut c_void) -> u32,
    pub take_edits: unsafe extern "C" fn(*mut c_void, *mut RavetPluginEditV1, u32) -> u32,
    pub editor_open: unsafe extern "C" fn(*mut c_void, u32, *mut c_void, u32, u32) -> i32,
    pub editor_resize: unsafe extern "C" fn(*mut c_void, u32, u32) -> i32,
    pub editor_close: unsafe extern "C" fn(*mut c_void),
    pub editor_idle: unsafe extern "C" fn(*mut c_void),
}

/// Connects an export to its static table data. Implemented by
/// [`crate::export_plugin!`]; not intended for direct use.
#[doc(hidden)]
pub trait NativeExport: 'static {
    type Plugin: PluginExport;
    /// The export's static data, built once; `None` for an invalid product.
    fn statics() -> Option<&'static NativeStatics>;
}

/// Static data of one export: the resolved product and the C records that
/// point into it. Built once and never freed.
#[doc(hidden)]
pub struct NativeStatics {
    product: ResolvedProduct<'static>,
    /// (host ID, declaration index), sorted by ID.
    ids: Vec<(u32, u32)>,
    _strings: Vec<CString>,
    parameters: Vec<RavetPluginParameterV1>,
    _layouts: Vec<RavetPluginLayoutV1>,
    record: Box<RavetPluginProductV1>,
    entry: Box<RavetPluginEntryV1>,
}

// SAFETY: the raw pointers point into immutable heap data owned by the same
// value (C strings, layouts, the boxed product record), which is never mutated
// or freed while shared.
#[allow(unsafe_code)]
unsafe impl Send for NativeStatics {}
#[allow(unsafe_code)]
unsafe impl Sync for NativeStatics {}

fn four_cc(code: [u8; 4]) -> u32 {
    u32::from_be_bytes(code)
}

impl NativeStatics {
    /// Build the static data of export `H`. Allocates; called once.
    pub fn new<H: NativeExport>() -> Option<Self> {
        let product = H::Plugin::PRODUCT.resolve().ok()?;
        if product.product().precision() == Precision::F64 {
            return None;
        }
        let description = *product.product();
        let identities = product.identities().clone();
        let mut strings = Vec::new();
        let mut string = |text: &str| -> Option<*const c_char> {
            let owned = CString::new(text).ok()?;
            let pointer = owned.as_ptr();
            strings.push(owned);
            Some(pointer)
        };
        let parameters = description
            .parameters()
            .iter()
            .zip(product.parameter_ids())
            .map(|(parameter, id)| {
                let range = parameter.range();
                let mut flags = 0;
                if parameter.is_automatable() {
                    flags |= parameter_flags::AUTOMATABLE;
                }
                if parameter.role() == ParameterRole::Bypass {
                    flags |= parameter_flags::BYPASS;
                }
                if parameter.is_read_only() {
                    flags |= parameter_flags::READ_ONLY;
                }
                if parameter.is_hidden() {
                    flags |= parameter_flags::HIDDEN;
                }
                if range.intervals().is_some() {
                    flags |= parameter_flags::STEPPED;
                }
                Some(RavetPluginParameterV1 {
                    id: id.0,
                    flags,
                    scale: u32::from(range.scale() == ParameterScale::Logarithmic),
                    steps: range.intervals().map_or(0, u32::from),
                    minimum: f64::from(range.minimum()),
                    maximum: f64::from(range.maximum()),
                    default_value: f64::from(parameter.default()),
                    key: string(parameter.key())?,
                    name: string(parameter.name())?,
                    unit: string(parameter.unit())?,
                })
            })
            .collect::<Option<Vec<_>>>()?;
        let layouts: Vec<_> = identities
            .aax_types
            .iter()
            .map(|(layout, aax_type_id)| RavetPluginLayoutV1 {
                inputs: layout.input_channels() as u32,
                outputs: layout.output_channels() as u32,
                aax_type_id: *aax_type_id,
                reserved: 0,
            })
            .collect();
        let editor = description.editor();
        let record = Box::new(RavetPluginProductV1 {
            name: string(description.name())?,
            vendor: string(description.vendor())?,
            version: string(description.version())?,
            plugin_id: string(description.plugin_id())?,
            url: string(description.url())?,
            category: match description.category() {
                Category::Effect => category::EFFECT,
                Category::Instrument => category::INSTRUMENT,
                Category::MidiEffect => category::MIDI_EFFECT,
            },
            note_input: u32::from(description.notes().input),
            note_output: u32::from(description.notes().output),
            layout_count: layouts.len() as u32,
            layouts: layouts.as_ptr(),
            parameter_count: parameters.len() as u32,
            bypass_index: product.bypass_index().map_or(-1, |index| index as i32),
            clap_id: string(&identities.clap_id)?,
            vst3_processor_cid: identities.vst3_processor,
            vst3_controller_cid: identities.vst3_controller,
            au_type: four_cc(identities.au_type),
            au_subtype: four_cc(identities.au_subtype),
            au_manufacturer: four_cc(identities.au_manufacturer),
            aax_manufacturer: identities.aax_manufacturer,
            aax_product: identities.aax_product,
            lv2_uri: string(&identities.lv2_uri)?,
            editor_width: editor.map_or(0, |editor| editor.width),
            editor_height: editor.map_or(0, |editor| editor.height),
            editor_resizable: u32::from(editor.is_some_and(|editor| editor.resizable)),
            reserved: 0,
        });
        let mut entry = Box::new(Table::<H>::ENTRY);
        entry.product = &*record;
        let mut ids: Vec<_> = product
            .parameter_ids()
            .iter()
            .enumerate()
            .map(|(index, id)| (id.0, index as u32))
            .collect();
        ids.sort_unstable();
        Some(Self {
            product,
            ids,
            _strings: strings,
            parameters,
            _layouts: layouts,
            record,
            entry,
        })
    }
    /// The resolved product.
    pub fn product(&self) -> &ResolvedProduct<'static> {
        &self.product
    }
    /// The C product record.
    pub fn record(&self) -> &RavetPluginProductV1 {
        &self.record
    }
    /// The table.
    pub fn entry(&self) -> &RavetPluginEntryV1 {
        &self.entry
    }
    fn index(&self, id: u32) -> Option<usize> {
        self.ids
            .binary_search_by_key(&id, |(candidate, _)| *candidate)
            .ok()
            .map(|position| self.ids[position].1 as usize)
    }
}

/// The table of export `H`, or null for an invalid product. Used by
/// `ravet_plugin_v1`.
#[doc(hidden)]
pub fn entry<H: NativeExport>() -> *const RavetPluginEntryV1 {
    catch_unwind(|| H::statics().map_or(ptr::null(), |statics| &*statics.entry as *const _))
        .unwrap_or(ptr::null())
}

/// State shared by every thread of one instance (and its editor).
struct Shared {
    statics: &'static NativeStatics,
    parameters: ParameterCache,
    edits: EditQueue,
    /// One bit per parameter whose cached value the processor has not received.
    dirty: Box<[AtomicU64]>,
    /// A panic happened; processing calls fail until the next prepare.
    fault: AtomicBool,
    /// Latency of the prepared processor after its latest block.
    latency: AtomicU32,
    /// Latency last read with `timing`.
    reported: AtomicU32,
}

impl Shared {
    fn mark(&self, index: usize) {
        self.dirty[index / 64].fetch_or(1 << (index % 64), Ordering::AcqRel);
    }
    fn clear_dirty(&self) {
        for word in self.dirty.iter() {
            word.store(0, Ordering::Release);
        }
    }
    fn product(&self) -> &ResolvedProduct<'static> {
        &self.statics.product
    }
}

/// The prepared side of an instance, owned by the audio thread.
struct Prepared {
    adapter: NativeAudioAdapter<f32>,
    /// Events for the processor; always [`MAXIMUM_EVENTS`] long.
    events: Vec<Event>,
    /// Cache generation the processor last received in full.
    generation: u64,
    /// Host limits from the native configuration.
    host_events: usize,
    output_events: usize,
    layout: (u32, u32),
}

struct AudioSide<P: Processor<f32>> {
    /// `None` after a panic until the next prepare recreates it.
    processor: Option<Wrapped<P>>,
    prepared: Option<Prepared>,
}

struct Instance<P: PluginExport> {
    shared: Arc<Shared>,
    /// Audio-thread state; touched by prepare/deactivate/timing only while
    /// processing is stopped, as the header requires.
    audio: UnsafeCell<AudioSide<P::Processor>>,
    /// Main-thread editor.
    #[cfg(target_os = "macos")]
    editor: UnsafeCell<Option<crate::editor_host::EditorHost<'static>>>,
}

/// The editor side of an instance: editor values become dirty parameters.
#[cfg(target_os = "macos")]
struct NativeTarget {
    shared: Arc<Shared>,
    editor_message: fn(&serde_json::Value) -> Option<serde_json::Value>,
}

#[cfg(target_os = "macos")]
impl crate::editor_host::EditorTarget for NativeTarget {
    fn product(&self) -> &ResolvedProduct<'static> {
        self.shared.product()
    }
    fn parameters(&self) -> &ParameterCache {
        &self.shared.parameters
    }
    fn edit_queue(&self) -> &EditQueue {
        &self.shared.edits
    }
    fn editor_message(&self, data: &serde_json::Value) -> Option<serde_json::Value> {
        (self.editor_message)(data)
    }
    fn queued(&self, edits: &[Edit]) {
        for edit in edits {
            if let Edit::Value(index, _) = edit {
                self.shared.mark(*index);
            }
        }
    }
}

fn aligned<T>(pointer: *const T) -> bool {
    (pointer as usize).is_multiple_of(align_of::<T>())
}

/// Silence the declared outputs of valid buses after a rejected block.
///
/// # Safety
/// As for `process`: nonnull, aligned pointers must be valid for the declared
/// channels and frames.
#[allow(unsafe_code)]
unsafe fn silence(buses: &[NativePlanarBus<f32>], frames: usize) {
    if frames == 0 || frames > 65536 {
        return;
    }
    for bus in buses {
        if bus.output_channels > 64 || bus.outputs.is_null() || !aligned(bus.outputs) {
            continue;
        }
        for channel in 0..bus.output_channels as usize {
            // SAFETY: the caller guarantees the pointer array covers the channels.
            let output = unsafe { *bus.outputs.add(channel) };
            if !output.is_null() && aligned(output) {
                // SAFETY: the caller guarantees the channel covers `frames`.
                unsafe { slice::from_raw_parts_mut(output, frames) }.fill(0.0);
            }
        }
    }
}

/// The extern functions of export `H`.
struct Table<H>(PhantomData<fn() -> H>);

type Plugin<H> = <H as NativeExport>::Plugin;
type ProcessorOf<H> = <Plugin<H> as PluginExport>::Processor;

#[allow(unsafe_code)]
impl<H: NativeExport> Table<H> {
    const ENTRY: RavetPluginEntryV1 = RavetPluginEntryV1 {
        version: VERSION,
        byte_size: size_of::<RavetPluginEntryV1>() as u32,
        product: ptr::null(),
        parameter: Self::parameter,
        parameter_index: Self::parameter_index,
        value_to_text: Self::value_to_text,
        text_to_value: Self::text_to_value,
        normalize: Self::normalize,
        denormalize: Self::denormalize,
        create: Self::create,
        destroy: Self::destroy,
        get_value: Self::get_value,
        set_value: Self::set_value,
        save_state: Self::save_state,
        load_state: Self::load_state,
        prepare: Self::prepare,
        process: Self::process,
        reset: Self::reset,
        deactivate: Self::deactivate,
        timing: Self::timing,
        latency_changed: Self::latency_changed,
        take_edits: Self::take_edits,
        editor_open: Self::editor_open,
        editor_resize: Self::editor_resize,
        editor_close: Self::editor_close,
        editor_idle: Self::editor_idle,
    };

    /// Run a static (instance-free) function, returning 2 on panic.
    fn guard_static(operation: impl FnOnce(&'static NativeStatics) -> i32) -> i32 {
        catch_unwind(AssertUnwindSafe(|| match H::statics() {
            Some(statics) => operation(statics),
            None => 1,
        }))
        .unwrap_or(2)
    }

    /// Run an instance function; a panic faults the instance and returns `panicked`.
    fn guard<R>(
        instance: *const c_void,
        rejected: R,
        panicked: R,
        operation: impl FnOnce(&Instance<Plugin<H>>) -> R,
    ) -> R {
        // SAFETY: the header requires `instance` to come from this table's
        // `create` and not yet be destroyed.
        let Some(owner) = (unsafe { instance.cast::<Instance<Plugin<H>>>().as_ref() }) else {
            return rejected;
        };
        match catch_unwind(AssertUnwindSafe(|| operation(owner))) {
            Ok(result) => result,
            Err(_) => {
                owner.shared.fault.store(true, Ordering::Release);
                panicked
            }
        }
    }

    fn parameter_of(
        statics: &'static NativeStatics,
        index: u32,
    ) -> Option<&'static crate::Parameter<'static>> {
        statics.product.product().parameters().get(index as usize)
    }

    unsafe extern "C" fn parameter(index: u32, output: *mut RavetPluginParameterV1) -> i32 {
        Self::guard_static(|statics| {
            match statics.parameters.get(index as usize) {
                Some(record) if !output.is_null() && aligned(output) => {
                    // SAFETY: checked nonnull and aligned; the caller owns the storage.
                    unsafe { output.write(*record) };
                    0
                }
                _ => 1,
            }
        })
    }

    extern "C" fn parameter_index(id: u32) -> i32 {
        catch_unwind(|| {
            H::statics()
                .and_then(|statics| statics.index(id))
                .map_or(-1, |index| index as i32)
        })
        .unwrap_or(-1)
    }

    unsafe extern "C" fn value_to_text(
        index: u32,
        physical: f64,
        text: *mut c_char,
        capacity: u32,
    ) -> i32 {
        Self::guard_static(|statics| {
            let Some(parameter) = Self::parameter_of(statics, index) else {
                return 1;
            };
            if text.is_null() || capacity == 0 {
                return 1;
            }
            // SAFETY: the caller provides `capacity` writable bytes.
            let output = unsafe { slice::from_raw_parts_mut(text.cast::<u8>(), capacity as usize) };
            output[0] = 0;
            let Some(formatted) = crate::text::format_value(parameter, physical) else {
                return 1;
            };
            let mut length = formatted.len().min(output.len() - 1);
            while !formatted.is_char_boundary(length) {
                length -= 1;
            }
            output[..length].copy_from_slice(&formatted.as_bytes()[..length]);
            output[length] = 0;
            i32::from(length < formatted.len())
        })
    }

    unsafe extern "C" fn text_to_value(index: u32, text: *const c_char, physical: *mut f64) -> i32 {
        Self::guard_static(|statics| {
            let Some(parameter) = Self::parameter_of(statics, index) else {
                return 1;
            };
            if text.is_null() || physical.is_null() || !aligned(physical) {
                return 1;
            }
            // SAFETY: the caller passes a NUL-terminated string.
            let Ok(text) = unsafe { CStr::from_ptr(text) }.to_str() else {
                return 1;
            };
            match crate::text::parse_value(parameter, text) {
                Some(value) => {
                    // SAFETY: checked nonnull and aligned.
                    unsafe { physical.write(f64::from(value)) };
                    0
                }
                None => 1,
            }
        })
    }

    fn map(
        index: u32,
        value: f64,
        output: *mut f64,
        operation: impl FnOnce(ravet_foundation::ParameterRange, f32) -> Option<f32>,
    ) -> i32 {
        Self::guard_static(|statics| {
            let Some(parameter) = Self::parameter_of(statics, index) else {
                return 1;
            };
            if output.is_null() || !aligned(output) {
                return 1;
            }
            match operation(parameter.range(), value as f32) {
                Some(result) => {
                    // SAFETY: checked nonnull and aligned.
                    unsafe { output.write(f64::from(result)) };
                    0
                }
                None => 1,
            }
        })
    }

    unsafe extern "C" fn normalize(index: u32, physical: f64, normalized: *mut f64) -> i32 {
        Self::map(index, physical, normalized, |range, value| {
            range.normalize(value)
        })
    }

    unsafe extern "C" fn denormalize(index: u32, normalized: f64, physical: *mut f64) -> i32 {
        Self::map(index, normalized, physical, |range, value| {
            range.denormalize(value)
        })
    }

    extern "C" fn create() -> *mut c_void {
        catch_unwind(|| {
            let Some(statics) = H::statics() else {
                return ptr::null_mut();
            };
            let product = &statics.product;
            let Some(processor) = wrap(Plugin::<H>::create(), product) else {
                return ptr::null_mut();
            };
            let shared = Arc::new(Shared {
                statics,
                parameters: ParameterCache::new(product),
                edits: EditQueue::new(),
                dirty: (0..product.parameter_ids().len().div_ceil(64))
                    .map(|_| AtomicU64::new(0))
                    .collect(),
                fault: AtomicBool::new(false),
                latency: AtomicU32::new(0),
                reported: AtomicU32::new(0),
            });
            let instance = Instance::<Plugin<H>> {
                shared,
                audio: UnsafeCell::new(AudioSide {
                    processor: Some(processor),
                    prepared: None,
                }),
                #[cfg(target_os = "macos")]
                editor: UnsafeCell::new(None),
            };
            Box::into_raw(Box::new(instance)).cast()
        })
        .unwrap_or(ptr::null_mut())
    }

    unsafe extern "C" fn destroy(instance: *mut c_void) {
        if instance.is_null() {
            return;
        }
        let _ = catch_unwind(AssertUnwindSafe(|| {
            // SAFETY: created by `create` and destroyed once, on MAIN.
            let owned = unsafe { Box::from_raw(instance.cast::<Instance<Plugin<H>>>()) };
            #[cfg(target_os = "macos")]
            drop(owned.editor.into_inner());
            let mut audio = owned.audio.into_inner();
            audio.prepared = None;
            if let Some(processor) = &mut audio.processor {
                processor.deactivate();
            }
        }));
    }

    unsafe extern "C" fn get_value(instance: *const c_void, index: u32) -> f64 {
        Self::guard(instance, f64::NAN, f64::NAN, |owner| {
            let parameters = &owner.shared.parameters;
            if (index as usize) < parameters.len() {
                f64::from(parameters.get(index as usize))
            } else {
                f64::NAN
            }
        })
    }

    unsafe extern "C" fn set_value(instance: *mut c_void, index: u32, physical: f64) -> i32 {
        Self::guard(instance, 1, 2, |owner| {
            let shared = &owner.shared;
            let Some(parameter) = shared
                .product()
                .product()
                .parameters()
                .get(index as usize)
            else {
                return 1;
            };
            if !physical.is_finite() {
                return 1;
            }
            let Some(value) = parameter.range().canonicalize(physical as f32) else {
                return 1;
            };
            shared.parameters.set(index as usize, value);
            shared.mark(index as usize);
            0
        })
    }

    unsafe extern "C" fn save_state(
        instance: *mut c_void,
        data: *mut u8,
        capacity: u32,
        length: *mut u32,
    ) -> i32 {
        Self::guard(instance, 1, 2, |owner| {
            if length.is_null() || !aligned(length) {
                return 1;
            }
            let shared = &owner.shared;
            let mut values = vec![0.0; shared.parameters.len()];
            shared.parameters.read_into(&mut values);
            let Ok(bytes) = state::encode(shared.product(), &values, &[]) else {
                return 1;
            };
            let Ok(size) = u32::try_from(bytes.len()) else {
                return 1;
            };
            // SAFETY: checked nonnull and aligned.
            unsafe { length.write(size) };
            if data.is_null() {
                return 0;
            }
            if capacity < size {
                return 1;
            }
            // SAFETY: the caller provides `capacity` writable bytes.
            unsafe { ptr::copy_nonoverlapping(bytes.as_ptr(), data, bytes.len()) };
            0
        })
    }

    unsafe extern "C" fn load_state(instance: *mut c_void, data: *const u8, length: u32) -> i32 {
        Self::guard(instance, 1, 2, |owner| {
            if (data.is_null() && length > 0) || u64::from(length) > MAXIMUM_STATE_BYTES {
                return 1;
            }
            let bytes = if length == 0 {
                &[][..]
            } else {
                // SAFETY: the caller provides `length` readable bytes.
                unsafe { slice::from_raw_parts(data, length as usize) }
            };
            let shared = &owner.shared;
            let hooks = FnHooks::of::<Plugin<H>>();
            match state::restore(shared.product(), bytes, &hooks) {
                Ok(restored) => {
                    shared.parameters.restore(&restored.values);
                    0
                }
                Err(_) => 1,
            }
        })
    }

    /// Send every cached value to the processor in a zero-frame block, then
    /// reset it. Returns 0, 1 if the processor rejected it, or 2 on panic.
    fn synchronize(
        shared: &Shared,
        processor: &mut Wrapped<ProcessorOf<H>>,
        prepared: &mut Prepared,
    ) -> i32 {
        let generation = shared.parameters.generation();
        shared.clear_dirty();
        let ids = shared.product().parameter_ids();
        for (index, id) in ids.iter().enumerate() {
            prepared.events[index] = Event {
                frame_offset: 0,
                data: EventData::ParameterValue {
                    id: *id,
                    value: shared.parameters.get(index),
                },
            };
        }
        let buses = [NativePlanarBus::<f32> {
            input_channels: prepared.layout.0,
            output_channels: prepared.layout.1,
            inputs: ptr::null(),
            outputs: ptr::null(),
        }];
        // SAFETY: zero frames never dereference sample pointers.
        let status = unsafe {
            prepared.adapter.process_native(
                processor,
                &buses,
                0,
                &prepared.events[..ids.len()],
                Transport::default(),
            )
        }
        .status;
        prepared.generation = generation;
        let reset = prepared.adapter.reset(processor);
        if prepared.adapter.is_poisoned() {
            return 2;
        }
        i32::from(matches!(status, ProcessStatus::Fault(_)) || reset.is_err())
    }

    unsafe extern "C" fn prepare(
        instance: *mut c_void,
        config: *const NativeProcessorConfigV1,
    ) -> i32 {
        Self::guard(instance, 1, 2, |owner| {
            // SAFETY: the header requires processing to have stopped.
            let audio = unsafe { &mut *owner.audio.get() };
            let shared = &owner.shared;
            audio.prepared = None;
            if config.is_null() || !aligned(config) {
                return 1;
            }
            // SAFETY: checked nonnull and aligned.
            let native = unsafe { *config };
            let main = native.buses[0];
            let product = shared.product().product();
            if native.bus_count != 1
                || !product.layouts().iter().any(|layout| {
                    layout.input_channels() == main.inputs as usize
                        && layout.output_channels() == main.outputs as usize
                })
            {
                return 1;
            }
            let Some(host) = native.to_configuration() else {
                return 1;
            };
            let Ok(config) = ProcessConfig::new(
                host.sample_rate(),
                host.maximum_frames(),
                MAXIMUM_EVENTS,
                host.layout(),
            ) else {
                return 1;
            };
            let config = config.with_output_events(host.maximum_output_events());
            if shared.fault.load(Ordering::Acquire) || audio.processor.is_none() {
                audio.processor = None;
                audio.processor = wrap(Plugin::<H>::create(), shared.product());
            }
            let Some(processor) = &mut audio.processor else {
                return 1;
            };
            let Ok(adapter) = NativeAudioAdapter::prepare(processor, config) else {
                return 1;
            };
            let mut prepared = Prepared {
                adapter,
                events: vec![
                    Event {
                        frame_offset: 0,
                        data: EventData::ParameterValue {
                            id: ParameterId(0),
                            value: 0.0,
                        },
                    };
                    MAXIMUM_EVENTS
                ],
                generation: 0,
                host_events: host.maximum_events(),
                output_events: host.maximum_output_events(),
                layout: (main.inputs, main.outputs),
            };
            shared.fault.store(false, Ordering::Release);
            match Self::synchronize(shared, processor, &mut prepared) {
                0 => {}
                code => {
                    if code == 2 {
                        shared.fault.store(true, Ordering::Release);
                    }
                    processor.deactivate();
                    return code;
                }
            }
            let latency = processor.latency_samples();
            shared.latency.store(latency, Ordering::Release);
            shared.reported.store(latency, Ordering::Release);
            audio.prepared = Some(prepared);
            0
        })
    }

    unsafe extern "C" fn process(
        instance: *mut c_void,
        buses: *const NativeProcessorBusV1,
        bus_count: u32,
        frames: u32,
        input: *const NativeEventV1,
        input_count: u32,
        transport: *const NativeTransportV1,
        output: *mut NativeEventV1,
        output_capacity: u32,
        result: *mut NativeProcessorResultV1,
    ) -> i32 {
        let main = if bus_count == 1 && !buses.is_null() && aligned(buses) {
            // SAFETY: checked nonnull and aligned; the caller passes `bus_count` buses.
            let bus = unsafe { *buses };
            Some(NativePlanarBus::<f32> {
                input_channels: bus.input_channels,
                output_channels: bus.output_channels,
                inputs: bus.inputs.cast(),
                outputs: bus.outputs.cast(),
            })
        } else {
            None
        };
        let native = main.as_slice();
        let valid_result = !result.is_null() && aligned(result);
        if valid_result {
            // SAFETY: checked nonnull and aligned.
            unsafe {
                result.write(NativeProcessorResultV1 {
                    status: 2,
                    event_count: 0,
                })
            };
        }
        let code = Self::guard(instance, 1, 2, |owner| {
            let shared = &owner.shared;
            if shared.fault.load(Ordering::Acquire) {
                return 2;
            }
            // SAFETY: AUDIO owns processing; the header serializes these calls.
            let audio = unsafe { &mut *owner.audio.get() };
            let (Some(processor), Some(prepared)) = (&mut audio.processor, &mut audio.prepared)
            else {
                return 1;
            };
            let frames = frames as usize;
            let count = input_count as usize;
            if main.is_none()
                || !valid_result
                || frames > prepared.adapter.configuration().maximum_frames()
                || count > prepared.host_events
                || count > MAXIMUM_EVENTS
                || (output_capacity as usize) < prepared.output_events
                || output_capacity > 1024
                || (count > 0 && (input.is_null() || !aligned(input)))
                || (output_capacity > 0 && (output.is_null() || !aligned(output)))
            {
                return 1;
            }
            let records = if count == 0 {
                &[][..]
            } else {
                // SAFETY: checked nonnull and aligned; the caller passes `count` records.
                unsafe { slice::from_raw_parts(input, count) }
            };
            let transport = if transport.is_null() {
                Transport::default()
            } else if !aligned(transport) {
                return 1;
            } else {
                // SAFETY: checked nonnull and aligned.
                match unsafe { *transport }.to_transport() {
                    Ok(transport) => transport,
                    Err(_) => return 1,
                }
            };
            // Validate every host record before changing anything.
            let mut previous = 0;
            for record in records {
                let offset = record.offset as usize;
                if (frames == 0 && offset != 0) || (frames > 0 && offset >= frames) {
                    return 1;
                }
                if offset < previous {
                    return 1;
                }
                previous = offset;
                if record.kind != native_event_kind::PARAMETER_VALUE && record.to_event().is_err()
                {
                    return 1;
                }
            }
            if prepared.generation != shared.parameters.generation() {
                let code = Self::synchronize(shared, processor, prepared);
                if code != 0 {
                    return code;
                }
            }
            let mut length = 0;
            // Cached host-side and editor changes first, as many as fit.
            let room = MAXIMUM_EVENTS - count;
            let ids = shared.product().parameter_ids();
            'dirty: for (word_index, word) in shared.dirty.iter().enumerate() {
                let mut bits = word.load(Ordering::Acquire);
                while bits != 0 {
                    if length == room {
                        break 'dirty;
                    }
                    let bit = bits.trailing_zeros() as usize;
                    bits &= bits - 1;
                    word.fetch_and(!(1 << bit), Ordering::AcqRel);
                    let index = word_index * 64 + bit;
                    prepared.events[length] = Event {
                        frame_offset: 0,
                        data: EventData::ParameterValue {
                            id: ids[index],
                            value: shared.parameters.get(index),
                        },
                    };
                    length += 1;
                }
            }
            for record in records {
                let event = if record.kind == native_event_kind::PARAMETER_VALUE {
                    let Some(index) = shared.statics.index(record.parameter_id) else {
                        continue;
                    };
                    let parameter = &shared.product().product().parameters()[index];
                    let Some(value) = parameter.range().canonicalize(record.value as f32) else {
                        continue;
                    };
                    shared.parameters.set(index, value);
                    Event {
                        frame_offset: record.offset as usize,
                        data: EventData::ParameterValue {
                            id: ids[index],
                            value,
                        },
                    }
                } else {
                    record.to_event().expect("validated record")
                };
                prepared.events[length] = event;
                length += 1;
            }
            // SAFETY: the caller guarantees valid bus pointers and samples.
            let report = unsafe {
                prepared.adapter.process_native(
                    processor,
                    native,
                    frames,
                    &prepared.events[..length],
                    transport,
                )
            };
            let status = match report.status {
                ProcessStatus::Continue => 0,
                ProcessStatus::Silence => 1,
                ProcessStatus::Fault(_) => {
                    return if prepared.adapter.is_poisoned() { 2 } else { 1 };
                }
            };
            let destination = if output_capacity == 0 {
                &mut [][..]
            } else {
                // SAFETY: checked nonnull and aligned; the caller provides the records.
                unsafe { slice::from_raw_parts_mut(output, output_capacity as usize) }
            };
            let Ok(generated) = encode_native_events(report.events, frames, destination) else {
                return 1;
            };
            let event_count = generated.len() as u32;
            shared
                .latency
                .store(processor.latency_samples(), Ordering::Release);
            // SAFETY: checked nonnull and aligned.
            unsafe {
                result.write(NativeProcessorResultV1 {
                    status,
                    event_count,
                })
            };
            0
        });
        if code != 0 {
            if code == 2
                && let Some(owner) =
                    // SAFETY: as in `guard`.
                    unsafe { instance.cast::<Instance<Plugin<H>>>().as_ref() }
            {
                owner.shared.fault.store(true, Ordering::Release);
            }
            if valid_result {
                // SAFETY: checked nonnull and aligned.
                unsafe {
                    result.write(NativeProcessorResultV1 {
                        status: 2,
                        event_count: 0,
                    })
                };
            }
            // SAFETY: the caller guarantees valid bus pointers and samples.
            unsafe { silence(native, frames as usize) };
        }
        code
    }

    unsafe extern "C" fn reset(instance: *mut c_void) -> i32 {
        Self::guard(instance, 1, 2, |owner| {
            let shared = &owner.shared;
            if shared.fault.load(Ordering::Acquire) {
                return 2;
            }
            // SAFETY: AUDIO owns processing; the header serializes these calls.
            let audio = unsafe { &mut *owner.audio.get() };
            let (Some(processor), Some(prepared)) = (&mut audio.processor, &mut audio.prepared)
            else {
                return 1;
            };
            let code = if prepared.generation != shared.parameters.generation() {
                Self::synchronize(shared, processor, prepared)
            } else if prepared.adapter.reset(processor).is_ok() {
                0
            } else if prepared.adapter.is_poisoned() {
                2
            } else {
                1
            };
            if code == 2 {
                shared.fault.store(true, Ordering::Release);
            }
            code
        })
    }

    unsafe extern "C" fn deactivate(instance: *mut c_void) -> i32 {
        Self::guard(instance, 1, 2, |owner| {
            // SAFETY: the header requires processing to have stopped.
            let audio = unsafe { &mut *owner.audio.get() };
            audio.prepared = None;
            if let Some(processor) = &mut audio.processor {
                processor.deactivate();
            }
            0
        })
    }

    unsafe extern "C" fn timing(instance: *mut c_void, output: *mut NativeProcessorTimingV1) -> i32 {
        Self::guard(instance, 1, 2, |owner| {
            let shared = &owner.shared;
            // SAFETY: the header forbids calling timing during process.
            let audio = unsafe { &*owner.audio.get() };
            if output.is_null() || !aligned(output) || audio.prepared.is_none() {
                return 1;
            }
            if shared.fault.load(Ordering::Acquire) {
                return 2;
            }
            let Some(processor) = &audio.processor else {
                return 1;
            };
            let latency = processor.latency_samples();
            let (tail_kind, tail_samples) = match processor.tail() {
                Tail::None => (0, 0),
                Tail::Samples(samples) => (1, samples),
                Tail::Infinite => (2, 0),
            };
            // SAFETY: checked nonnull and aligned.
            unsafe {
                output.write(NativeProcessorTimingV1 {
                    latency_samples: latency,
                    tail_kind,
                    tail_samples,
                })
            };
            shared.latency.store(latency, Ordering::Release);
            shared.reported.store(latency, Ordering::Release);
            0
        })
    }

    unsafe extern "C" fn latency_changed(instance: *mut c_void) -> u32 {
        Self::guard(instance, 0, 0, |owner| {
            let shared = &owner.shared;
            u32::from(
                shared.latency.load(Ordering::Acquire) != shared.reported.load(Ordering::Acquire),
            )
        })
    }

    unsafe extern "C" fn take_edits(
        instance: *mut c_void,
        edits: *mut RavetPluginEditV1,
        capacity: u32,
    ) -> u32 {
        Self::guard(instance, 0, 0, |owner| {
            if edits.is_null() || !aligned(edits) {
                return 0;
            }
            let queue = &owner.shared.edits;
            let mut written = 0;
            while written < capacity {
                let Some(edit) = queue.peek() else {
                    break;
                };
                let record = match edit {
                    Edit::Begin(index) => RavetPluginEditV1 {
                        kind: edit_kind::BEGIN,
                        index: index as u32,
                        value: 0.0,
                    },
                    Edit::Value(index, value) => RavetPluginEditV1 {
                        kind: edit_kind::VALUE,
                        index: index as u32,
                        value: f64::from(value),
                    },
                    Edit::End(index) => RavetPluginEditV1 {
                        kind: edit_kind::END,
                        index: index as u32,
                        value: 0.0,
                    },
                };
                // SAFETY: the caller provides `capacity` records.
                unsafe { edits.add(written as usize).write(record) };
                queue.pop();
                written += 1;
            }
            written
        })
    }

    unsafe extern "C" fn editor_open(
        instance: *mut c_void,
        view_kind: u32,
        parent: *mut c_void,
        width: u32,
        height: u32,
    ) -> i32 {
        Self::guard(instance, 1, 2, |owner| {
            #[cfg(target_os = "macos")]
            {
                use crate::editor_host::{EditorHost, adjust_size};
                // SAFETY: editor functions run on MAIN only.
                let slot = unsafe { &mut *owner.editor.get() };
                let Some(parent) = ptr::NonNull::new(parent) else {
                    return 1;
                };
                let Some(editor) = owner.shared.product().product().editor() else {
                    return 1;
                };
                if view_kind != view_kind::COCOA || slot.is_some() {
                    return 1;
                }
                let target = NativeTarget {
                    shared: owner.shared.clone(),
                    editor_message: Plugin::<H>::editor_message,
                };
                let Some(mut host) = EditorHost::create(target) else {
                    return 1;
                };
                let (width, height) = adjust_size(editor, width, height);
                if host.size() != (width, height) && !host.set_size(width, height) {
                    return 1;
                }
                // SAFETY: the header requires a live NSView on MAIN that stays
                // alive until editor_close or destroy.
                unsafe { host.attach(parent) };
                host.set_visible(true);
                *slot = Some(host);
                0
            }
            #[cfg(not(target_os = "macos"))]
            {
                let _ = (owner, view_kind, parent, width, height);
                1
            }
        })
    }

    unsafe extern "C" fn editor_resize(instance: *mut c_void, width: u32, height: u32) -> i32 {
        Self::guard(instance, 1, 2, |owner| {
            #[cfg(target_os = "macos")]
            {
                // SAFETY: editor functions run on MAIN only.
                let slot = unsafe { &mut *owner.editor.get() };
                let (Some(host), Some(editor)) =
                    (slot.as_mut(), owner.shared.product().product().editor())
                else {
                    return 1;
                };
                if crate::editor_host::adjust_size(editor, width, height) != (width, height) {
                    return 1;
                }
                i32::from(!host.set_size(width, height))
            }
            #[cfg(not(target_os = "macos"))]
            {
                let _ = (owner, width, height);
                1
            }
        })
    }

    unsafe extern "C" fn editor_close(instance: *mut c_void) {
        Self::guard(instance, (), (), |owner| {
            #[cfg(target_os = "macos")]
            {
                // SAFETY: editor functions run on MAIN only.
                let editor = unsafe { &mut *owner.editor.get() }.take();
                drop(editor);
            }
            #[cfg(not(target_os = "macos"))]
            let _ = owner;
        })
    }

    unsafe extern "C" fn editor_idle(instance: *mut c_void) {
        // The macOS web view polls the cache itself.
        Self::guard(instance, (), (), |_| {})
    }
}

#[cfg(test)]
mod tests;
