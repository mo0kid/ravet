//! Format-independent parts of an export: the product's processor trait and the
//! shared parameter cache that every format adapter reads and writes.
use crate::{
    Product, ResolvedProduct,
    state::{self, SavedState, StateError, StateHooks},
};
use ravet_foundation::Processor;
use std::sync::atomic::{AtomicU32, AtomicU64, AtomicUsize, Ordering};

/// A plug-in RAVET can export: a product description and the processor behind it.
///
/// Implement it on a marker type, so any existing processor can be exported
/// without a wrapper:
///
/// ```ignore
/// pub struct MyPlugin;
/// impl ravet_plugin::PluginExport for MyPlugin {
///     type Processor = MyProcessor;
///     const PRODUCT: Product<'static> = /* … */;
///     fn create() -> MyProcessor { MyProcessor::default() }
/// }
/// ravet_plugin::export_plugin!(MyPlugin);
/// ```
///
/// The processor receives parameter changes as `EventData::ParameterValue`
/// events carrying the product's host IDs and physical values, already clamped
/// and snapped to each parameter's range.
pub trait PluginExport: 'static {
    type Processor: Processor<f32> + Send + 'static;
    /// The product description. It is validated when a host creates the plug-in.
    const PRODUCT: Product<'static>;
    /// Create an unprepared processor. Called on the main thread.
    fn create() -> Self::Processor;
    /// Update state saved by an earlier product state version.
    fn migrate_state(_saved: &mut SavedState) -> Result<(), StateError> {
        Ok(())
    }
    /// Convert state without the RAVET envelope, for example from an earlier framework.
    fn import_state(_bytes: &[u8]) -> Option<SavedState> {
        None
    }
    /// Handle a product-channel message from the editor on the main thread.
    /// A returned value is sent back to the editor as a `product` message.
    fn editor_message(_data: &serde_json::Value) -> Option<serde_json::Value> {
        None
    }
}

/// Largest state accepted from a host: the envelope's parameter and extra limits.
pub const MAXIMUM_STATE_BYTES: u64 =
    28 + 8 * state::MAXIMUM_SAVED_PARAMETERS as u64 + 4 + state::MAXIMUM_EXTRA_BYTES as u64;

/// A product's state hooks as function pointers, so format adapters can keep
/// them without being generic over the product.
#[derive(Clone, Copy)]
pub struct FnHooks {
    migrate: fn(&mut SavedState) -> Result<(), StateError>,
    import: fn(&[u8]) -> Option<SavedState>,
}
impl FnHooks {
    /// The hooks of export `P`.
    pub fn of<P: PluginExport>() -> Self {
        Self {
            migrate: P::migrate_state,
            import: P::import_state,
        }
    }
}
impl StateHooks for FnHooks {
    fn migrate(&self, saved: &mut SavedState) -> Result<(), StateError> {
        (self.migrate)(saved)
    }
    fn import(&self, bytes: &[u8]) -> Option<SavedState> {
        (self.import)(bytes)
    }
}

/// Current physical parameter values, shared by the main thread, the audio
/// thread and the editor. Lock-free and allocation-free after construction.
///
/// The main thread writes values when the host sets them, when state loads and
/// when the editor edits them, so saved state is always current. The audio thread
/// writes values it receives as automation. A restore increments the generation,
/// which tells the audio thread to resend every value to the processor.
pub struct ParameterCache {
    values: Box<[AtomicU32]>,
    generation: AtomicU64,
}

impl ParameterCache {
    /// A cache holding the product's defaults.
    pub fn new(product: &ResolvedProduct<'_>) -> Self {
        Self {
            values: product
                .defaults()
                .into_iter()
                .map(|value| AtomicU32::new(value.to_bits()))
                .collect(),
            generation: AtomicU64::new(0),
        }
    }
    pub fn len(&self) -> usize {
        self.values.len()
    }
    pub fn is_empty(&self) -> bool {
        self.values.is_empty()
    }
    /// The value at a declaration index.
    pub fn get(&self, index: usize) -> f32 {
        f32::from_bits(self.values[index].load(Ordering::Acquire))
    }
    /// Store one canonical value at a declaration index.
    pub fn set(&self, index: usize, value: f32) {
        self.values[index].store(value.to_bits(), Ordering::Release);
    }
    /// Copy every value into `output`, in declaration order.
    pub fn read_into(&self, output: &mut [f32]) {
        for (slot, value) in output.iter_mut().zip(self.values.iter()) {
            *slot = f32::from_bits(value.load(Ordering::Acquire));
        }
    }
    /// Replace every value and increment the generation, so the audio thread
    /// resends all values to the processor.
    pub fn restore(&self, values: &[f32]) {
        for (slot, value) in self.values.iter().zip(values) {
            slot.store(value.to_bits(), Ordering::Release);
        }
        self.generation.fetch_add(1, Ordering::AcqRel);
    }
    pub fn generation(&self) -> u64 {
        self.generation.load(Ordering::Acquire)
    }
}

/// One editor action on its way to the host and processor.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Edit {
    Begin(usize),
    /// A canonical physical value.
    Value(usize, f32),
    End(usize),
}

impl Edit {
    pub fn index(self) -> usize {
        match self {
            Edit::Begin(index) | Edit::Value(index, _) | Edit::End(index) => index,
        }
    }
    fn encode(self) -> u64 {
        let (kind, index, value) = match self {
            Edit::Begin(index) => (1, index, 0),
            Edit::Value(index, value) => (2, index, value.to_bits()),
            Edit::End(index) => (3, index, 0),
        };
        kind | ((index as u64) << 8) | (u64::from(value) << 32)
    }
    fn decode(word: u64) -> Option<Self> {
        let index = ((word >> 8) & 0xff_ffff) as usize;
        match word & 0xff {
            1 => Some(Edit::Begin(index)),
            2 => Some(Edit::Value(index, f32::from_bits((word >> 32) as u32))),
            3 => Some(Edit::End(index)),
            _ => None,
        }
    }
}

/// Bounded single-producer, single-consumer queue of editor edits.
///
/// The main thread pushes; the thread that delivers parameter events to the host
/// (the audio thread, or the main thread while inactive) pops. Lock-free and
/// allocation-free after construction.
pub struct EditQueue {
    slots: Box<[AtomicU64]>,
    /// Next slot to read; written only by the consumer.
    head: AtomicUsize,
    /// Next slot to write; written only by the producer.
    tail: AtomicUsize,
}

impl EditQueue {
    /// Capacity in edits.
    pub const CAPACITY: usize = 256;

    pub fn new() -> Self {
        Self {
            slots: (0..Self::CAPACITY).map(|_| AtomicU64::new(0)).collect(),
            head: AtomicUsize::new(0),
            tail: AtomicUsize::new(0),
        }
    }
    /// Push every edit, or none if there is not enough room. Producer only.
    pub fn push(&self, edits: &[Edit]) -> bool {
        let tail = self.tail.load(Ordering::Relaxed);
        let head = self.head.load(Ordering::Acquire);
        if Self::CAPACITY - tail.wrapping_sub(head) < edits.len() {
            return false;
        }
        for (offset, edit) in edits.iter().enumerate() {
            self.slots[tail.wrapping_add(offset) % Self::CAPACITY]
                .store(edit.encode(), Ordering::Relaxed);
        }
        self.tail
            .store(tail.wrapping_add(edits.len()), Ordering::Release);
        true
    }
    /// The oldest edit, without removing it. Consumer only.
    pub fn peek(&self) -> Option<Edit> {
        let head = self.head.load(Ordering::Relaxed);
        if head == self.tail.load(Ordering::Acquire) {
            return None;
        }
        Edit::decode(self.slots[head % Self::CAPACITY].load(Ordering::Relaxed))
    }
    /// Remove the oldest edit after it has been delivered. Consumer only.
    pub fn pop(&self) {
        let head = self.head.load(Ordering::Relaxed);
        if head != self.tail.load(Ordering::Acquire) {
            self.head.store(head.wrapping_add(1), Ordering::Release);
        }
    }
    pub fn is_empty(&self) -> bool {
        self.head.load(Ordering::Acquire) == self.tail.load(Ordering::Acquire)
    }
}

impl Default for EditQueue {
    fn default() -> Self {
        Self::new()
    }
}
