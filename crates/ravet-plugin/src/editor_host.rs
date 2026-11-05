//! The product's web editor in a RAVET web view (macOS), shared by the CLAP GUI
//! extension and the native table's `editor_*` functions.
//!
//! The view speaks the [editor protocol](crate::editor): it sends `info` and
//! snapshots of the parameter cache, and turns the page's edits into
//! [`Edit`]s on the export's [`EditQueue`], writing values to the cache.
use crate::{
    Editor, ResolvedProduct,
    editor::{self, EditorMessage, Gestures},
    export::{Edit, EditQueue, ParameterCache},
};
use ravet_webview::{Callbacks, WebView};
use std::{collections::VecDeque, ffi::c_void, ptr::NonNull};

/// Largest size, as a multiple of the product's editor size, for resizable editors.
pub(crate) const MAXIMUM_SCALE: u32 = 4;

/// What an editor needs from its export.
pub(crate) trait EditorTarget {
    fn product(&self) -> &ResolvedProduct<'static>;
    fn parameters(&self) -> &ParameterCache;
    fn edit_queue(&self) -> &EditQueue;
    fn editor_message(&self, data: &serde_json::Value) -> Option<serde_json::Value>;
    /// `edits` were queued and their values written to the cache.
    fn queued(&self, edits: &[Edit]);
}

/// The size an editor accepts for a requested size: the product's size when
/// not resizable, otherwise clamped to one to [`MAXIMUM_SCALE`] times it.
pub(crate) fn adjust_size(editor: Editor<'_>, width: u32, height: u32) -> (u32, u32) {
    if !editor.resizable {
        return (editor.width, editor.height);
    }
    (
        width.clamp(editor.width, editor.width * MAXIMUM_SCALE),
        height.clamp(editor.height, editor.height * MAXIMUM_SCALE),
    )
}

/// An open editor: its web view, whether it has a parent, and its size.
pub(crate) struct EditorHost<'a> {
    view: WebView<'a>,
    attached: bool,
    size: (u32, u32),
}

impl<'a> EditorHost<'a> {
    /// Create the product's editor, hidden and unattached, at its declared
    /// size. `None` when the product has no editor, off the main thread, or
    /// when the web view cannot be created.
    pub(crate) fn create(target: impl EditorTarget + 'a) -> Option<Self> {
        let editor = target.product().product().editor()?;
        let html = editor::page(editor.html);
        let bridge = Bridge::new(target);
        let view = WebView::from_html(
            &html,
            f64::from(editor.width),
            f64::from(editor.height),
            bridge,
        )
        .ok()?;
        view.set_visible(false);
        Some(Self {
            view,
            attached: false,
            size: (editor.width, editor.height),
        })
    }
    /// Attach to a host view.
    ///
    /// # Safety
    /// `parent` must be a live `NSView` on the main thread that stays alive
    /// until this editor is dropped.
    #[allow(unsafe_code)]
    pub(crate) unsafe fn attach(&mut self, parent: NonNull<c_void>) {
        // SAFETY: forwarded from the caller's contract.
        unsafe { self.view.attach(parent) };
        self.attached = true;
    }
    #[cfg_attr(not(feature = "clap"), allow(dead_code))]
    pub(crate) fn is_attached(&self) -> bool {
        self.attached
    }
    pub(crate) fn set_visible(&self, visible: bool) {
        self.view.set_visible(visible);
    }
    pub(crate) fn size(&self) -> (u32, u32) {
        self.size
    }
    /// Resize to an accepted size (see [`adjust_size`]).
    pub(crate) fn set_size(&mut self, width: u32, height: u32) -> bool {
        if !self.view.set_size(f64::from(width), f64::from(height)) {
            return false;
        }
        self.size = (width, height);
        true
    }
}

/// Main-thread bridge between the web view and the export.
struct Bridge<T: EditorTarget> {
    target: T,
    gestures: Gestures,
    /// Messages waiting for delivery before the next snapshot.
    outgoing: VecDeque<String>,
    /// The page has sent `ready`.
    ready: bool,
    values: Vec<f32>,
    sequence: u64,
    snapshot: Option<String>,
}

/// The editor-protocol side of an editor without a web view, for tests.
#[cfg(all(test, feature = "native"))]
pub(crate) fn bridge(target: impl EditorTarget) -> impl Callbacks {
    Bridge::new(target)
}

impl<T: EditorTarget> Bridge<T> {
    fn new(target: T) -> Self {
        let parameters = target.parameters().len();
        let gestures = Gestures::new(target.product());
        Self {
            target,
            gestures,
            outgoing: VecDeque::new(),
            ready: false,
            values: vec![f32::NAN; parameters],
            sequence: 0,
            snapshot: None,
        }
    }
    /// Queue edits for the host, writing their values to the cache. All or nothing.
    fn queue(&self, edits: &[Edit]) -> bool {
        if !self.target.edit_queue().push(edits) {
            return false;
        }
        for edit in edits {
            if let Edit::Value(index, value) = *edit {
                self.target.parameters().set(index, value);
            }
        }
        self.target.queued(edits);
        true
    }
    fn handle(&mut self, message: EditorMessage) {
        if self.gestures.apply(&message).is_err() {
            return;
        }
        match message {
            EditorMessage::Ready => {
                self.ready = true;
                self.snapshot = None;
                let info = editor::info(self.target.product());
                if info.len() <= editor::MAXIMUM_INCOMING_BYTES {
                    self.outgoing.push_back(info);
                }
            }
            EditorMessage::Begin { index } => {
                if !self.queue(&[Edit::Begin(index)]) {
                    // Keep gestures balanced with what the host will see.
                    let _ = self.gestures.apply(&EditorMessage::End { index });
                }
            }
            EditorMessage::Set { index, value } => {
                self.queue(&[Edit::Value(index, value)]);
            }
            EditorMessage::End { index } => {
                // End must reach the host; retry on the next message if full.
                if !self.queue(&[Edit::End(index)]) {
                    let _ = self.gestures.apply(&EditorMessage::Begin { index });
                }
            }
            EditorMessage::Commit { index, value } => {
                self.queue(&[
                    Edit::Begin(index),
                    Edit::Value(index, value),
                    Edit::End(index),
                ]);
            }
            EditorMessage::Product(data) => {
                if let Some(reply) = self.target.editor_message(&data) {
                    self.outgoing.push_back(editor::product_message(&reply));
                }
            }
        }
    }
}

impl<T: EditorTarget> Callbacks for Bridge<T> {
    fn message(&mut self, json: &str) {
        if let Ok(message) = editor::parse(self.target.product(), json) {
            self.handle(message);
        }
    }
    fn snapshot(&mut self) -> Option<String> {
        if !self.ready {
            return None;
        }
        if let Some(message) = self.outgoing.pop_front() {
            return Some(message);
        }
        let parameters = self.target.parameters();
        let changed = (0..parameters.len())
            .any(|index| parameters.get(index).to_bits() != self.values[index].to_bits());
        if changed || self.snapshot.is_none() {
            parameters.read_into(&mut self.values);
            self.sequence += 1;
            self.snapshot = Some(editor::snapshot(
                self.target.product(),
                self.sequence,
                &self.values,
                None,
                None,
            ));
        }
        self.snapshot.clone()
    }
}

impl<T: EditorTarget> Drop for Bridge<T> {
    fn drop(&mut self) {
        // Close gestures the page left open, so the host sees every end.
        let ends: Vec<Edit> = self
            .gestures
            .close_all()
            .into_iter()
            .map(Edit::End)
            .collect();
        if !ends.is_empty() {
            self.queue(&ends);
        }
    }
}
