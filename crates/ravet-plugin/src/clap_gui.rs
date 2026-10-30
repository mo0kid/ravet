//! The CLAP GUI extension on macOS: the product's web editor in a RAVET web
//! view, speaking the [editor protocol](crate::editor).
use crate::{
    clap::{ClapMain, ClapShared},
    editor::{self, EditorMessage, Gestures},
    export::Edit,
};
use clack_extensions::{gui::*, params::HostParams};
use clack_plugin::prelude::*;
use ravet_webview::{Callbacks, WebView};
use std::{collections::VecDeque, ptr::NonNull};

/// Largest size, as a multiple of the product's editor size, for resizable editors.
const MAXIMUM_SCALE: u32 = 4;

pub(crate) struct GuiState<'a> {
    view: WebView<'a>,
    attached: bool,
    size: GuiSize,
}

/// Main-thread bridge between the web view and the plug-in.
struct Bridge<'a> {
    shared: &'a ClapShared,
    host: HostMainThreadHandle<'a>,
    params: Option<HostParams>,
    gestures: Gestures,
    /// Messages waiting for delivery before the next snapshot.
    outgoing: VecDeque<String>,
    /// The page has sent `ready`.
    ready: bool,
    values: Vec<f32>,
    sequence: u64,
    snapshot: Option<String>,
}

impl Bridge<'_> {
    fn queue(&self, edits: &[Edit]) -> bool {
        if !self.shared.edit_queue().push(edits) {
            return false;
        }
        ClapMain::request_flush(&self.host, self.params);
        true
    }
    fn handle(&mut self, message: EditorMessage) {
        if self.gestures.apply(&message).is_err() {
            return;
        }
        let parameters = self.shared.parameters();
        match message {
            EditorMessage::Ready => {
                self.ready = true;
                self.snapshot = None;
                let info = editor::info(self.shared.product());
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
                if self.queue(&[Edit::Value(index, value)]) {
                    parameters.set(index, value);
                }
            }
            EditorMessage::End { index } => {
                // End must reach the host; retry on the next message if full.
                if !self.queue(&[Edit::End(index)]) {
                    let _ = self.gestures.apply(&EditorMessage::Begin { index });
                }
            }
            EditorMessage::Commit { index, value } => {
                if self.queue(&[
                    Edit::Begin(index),
                    Edit::Value(index, value),
                    Edit::End(index),
                ]) {
                    parameters.set(index, value);
                }
            }
            EditorMessage::Product(data) => {
                if let Some(reply) = self.shared.handle_editor_message(&data) {
                    self.outgoing.push_back(editor::product_message(&reply));
                }
            }
        }
    }
}

impl Callbacks for Bridge<'_> {
    fn message(&mut self, json: &str) {
        if let Ok(message) = editor::parse(self.shared.product(), json) {
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
        let parameters = self.shared.parameters();
        let changed = (0..parameters.len())
            .any(|index| parameters.get(index).to_bits() != self.values[index].to_bits());
        if changed || self.snapshot.is_none() {
            parameters.read_into(&mut self.values);
            self.sequence += 1;
            self.snapshot = Some(editor::snapshot(
                self.shared.product(),
                self.sequence,
                &self.values,
                None,
                None,
            ));
        }
        self.snapshot.clone()
    }
}

impl Drop for Bridge<'_> {
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

impl<'a> ClapMain<'a> {
    fn editor_size(&self) -> Option<(GuiSize, bool)> {
        let editor = self.shared().product().product().editor()?;
        Some((
            GuiSize {
                width: editor.width,
                height: editor.height,
            },
            editor.resizable,
        ))
    }
}

impl PluginGuiImpl for ClapMain<'_> {
    fn is_api_supported(&self, configuration: GuiConfiguration) -> bool {
        configuration.api_type == GuiApiType::COCOA && !configuration.is_floating
    }
    fn get_preferred_api(&self) -> Option<GuiConfiguration<'_>> {
        Some(GuiConfiguration {
            api_type: GuiApiType::COCOA,
            is_floating: false,
        })
    }
    fn create(&self, configuration: GuiConfiguration) -> Result<(), PluginError> {
        let (size, _) = self
            .editor_size()
            .ok_or(PluginError::Message("product has no editor"))?;
        if !self.is_api_supported(configuration) || self.gui().borrow().is_some() {
            return Err(PluginError::Message(
                "unsupported or already created editor",
            ));
        }
        let shared = self.shared();
        let html = editor::page(
            shared
                .product()
                .product()
                .editor()
                .map(|e| e.html)
                .unwrap_or_default(),
        );
        let bridge = Bridge {
            shared,
            host: *self.host(),
            params: self.host_params(),
            gestures: Gestures::new(shared.product()),
            outgoing: VecDeque::new(),
            ready: false,
            values: vec![f32::NAN; shared.parameters().len()],
            sequence: 0,
            snapshot: None,
        };
        let view = WebView::from_html(&html, f64::from(size.width), f64::from(size.height), bridge)
            .map_err(|_| PluginError::Message("editor creation failed or wrong thread"))?;
        view.set_visible(false);
        *self.gui().borrow_mut() = Some(GuiState {
            view,
            attached: false,
            size,
        });
        Ok(())
    }
    fn destroy(&self) {
        let editor = self.gui().borrow_mut().take();
        drop(editor);
    }
    fn set_scale(&self, scale: f64) -> Result<(), PluginError> {
        if scale.is_finite() && scale > 0.0 {
            Ok(())
        } else {
            Err(PluginError::Message("invalid editor scale"))
        }
    }
    fn get_size(&self) -> Option<GuiSize> {
        self.gui().borrow().as_ref().map(|state| state.size)
    }
    fn can_resize(&self) -> bool {
        self.editor_size().is_some_and(|(_, resizable)| resizable)
    }
    fn get_resize_hints(&self) -> Option<GuiResizeHints> {
        Some(GuiResizeHints {
            can_resize_horizontally: self.can_resize(),
            can_resize_vertically: self.can_resize(),
            strategy: AspectRatioStrategy::Disregard,
        })
    }
    fn adjust_size(&self, size: GuiSize) -> Option<GuiSize> {
        let (base, resizable) = self.editor_size()?;
        if !resizable {
            return Some(base);
        }
        Some(GuiSize {
            width: size.width.clamp(base.width, base.width * MAXIMUM_SCALE),
            height: size.height.clamp(base.height, base.height * MAXIMUM_SCALE),
        })
    }
    fn set_size(&self, size: GuiSize) -> Result<(), PluginError> {
        if self.adjust_size(size) != Some(size) {
            return Err(PluginError::Message("editor size out of range"));
        }
        let mut state = self.gui().borrow_mut();
        let state = state
            .as_mut()
            .ok_or(PluginError::Message("editor not created"))?;
        if !state
            .view
            .set_size(f64::from(size.width), f64::from(size.height))
        {
            return Err(PluginError::Message("editor resize failed"));
        }
        state.size = size;
        Ok(())
    }
    fn set_parent(&self, window: Window) -> Result<(), PluginError> {
        let parent =
            window
                .as_cocoa_nsview()
                .and_then(NonNull::new)
                .ok_or(PluginError::Message(
                    "editor requires a Cocoa NSView parent",
                ))?;
        let mut state = self.gui().borrow_mut();
        let state = state
            .as_mut()
            .ok_or(PluginError::Message("editor not created"))?;
        // SAFETY: the CLAP GUI contract requires the host to supply a live NSView
        // on the main thread and keep it alive until the editor is destroyed.
        #[allow(unsafe_code)]
        unsafe {
            state.view.attach(parent);
        }
        state.attached = true;
        Ok(())
    }
    fn set_transient(&self, _: Window) -> Result<(), PluginError> {
        Err(PluginError::Message("floating editors are not supported"))
    }
    fn show(&self) -> Result<(), PluginError> {
        let state = self.gui().borrow();
        let state = state
            .as_ref()
            .filter(|state| state.attached)
            .ok_or(PluginError::Message("editor has no parent"))?;
        state.view.set_visible(true);
        Ok(())
    }
    fn hide(&self) -> Result<(), PluginError> {
        let state = self.gui().borrow();
        let state = state
            .as_ref()
            .ok_or(PluginError::Message("editor not created"))?;
        state.view.set_visible(false);
        Ok(())
    }
}
