//! The CLAP GUI extension on macOS: the product's web editor
//! ([`EditorHost`]) attached to the host's Cocoa view.
use crate::{
    ResolvedProduct,
    clap::{ClapMain, ClapShared},
    editor_host::{EditorHost, EditorTarget, adjust_size},
    export::{Edit, EditQueue, ParameterCache},
};
use clack_extensions::{gui::*, params::HostParams};
use clack_plugin::prelude::*;
use std::ptr::NonNull;

pub(crate) type GuiState<'a> = EditorHost<'a>;

/// The CLAP side of an editor: queued edits ask the host to flush them.
struct ClapTarget<'a> {
    shared: &'a ClapShared,
    host: HostMainThreadHandle<'a>,
    params: Option<HostParams>,
}

impl EditorTarget for ClapTarget<'_> {
    fn product(&self) -> &ResolvedProduct<'static> {
        self.shared.product()
    }
    fn parameters(&self) -> &ParameterCache {
        self.shared.parameters()
    }
    fn edit_queue(&self) -> &EditQueue {
        self.shared.edit_queue()
    }
    fn editor_message(&self, data: &serde_json::Value) -> Option<serde_json::Value> {
        self.shared.handle_editor_message(data)
    }
    fn queued(&self, _: &[Edit]) {
        ClapMain::request_flush(&self.host, self.params);
    }
}

fn gui_size((width, height): (u32, u32)) -> GuiSize {
    GuiSize { width, height }
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
        if !self.is_api_supported(configuration) || self.gui().borrow().is_some() {
            return Err(PluginError::Message(
                "unsupported or already created editor",
            ));
        }
        let target = ClapTarget {
            shared: self.shared(),
            host: *self.host(),
            params: self.host_params(),
        };
        let editor = EditorHost::create(target).ok_or(PluginError::Message(
            "no editor, editor creation failed or wrong thread",
        ))?;
        *self.gui().borrow_mut() = Some(editor);
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
        self.gui().borrow().as_ref().map(|state| gui_size(state.size()))
    }
    fn can_resize(&self) -> bool {
        self.shared()
            .product()
            .product()
            .editor()
            .is_some_and(|editor| editor.resizable)
    }
    fn get_resize_hints(&self) -> Option<GuiResizeHints> {
        Some(GuiResizeHints {
            can_resize_horizontally: self.can_resize(),
            can_resize_vertically: self.can_resize(),
            strategy: AspectRatioStrategy::Disregard,
        })
    }
    fn adjust_size(&self, size: GuiSize) -> Option<GuiSize> {
        let editor = self.shared().product().product().editor()?;
        Some(gui_size(adjust_size(editor, size.width, size.height)))
    }
    fn set_size(&self, size: GuiSize) -> Result<(), PluginError> {
        if self.adjust_size(size) != Some(size) {
            return Err(PluginError::Message("editor size out of range"));
        }
        let mut state = self.gui().borrow_mut();
        let state = state
            .as_mut()
            .ok_or(PluginError::Message("editor not created"))?;
        if !state.set_size(size.width, size.height) {
            return Err(PluginError::Message("editor resize failed"));
        }
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
            state.attach(parent);
        }
        Ok(())
    }
    fn set_transient(&self, _: Window) -> Result<(), PluginError> {
        Err(PluginError::Message("floating editors are not supported"))
    }
    fn show(&self) -> Result<(), PluginError> {
        let state = self.gui().borrow();
        let state = state
            .as_ref()
            .filter(|state| state.is_attached())
            .ok_or(PluginError::Message("editor has no parent"))?;
        state.set_visible(true);
        Ok(())
    }
    fn hide(&self) -> Result<(), PluginError> {
        let state = self.gui().borrow();
        let state = state
            .as_ref()
            .ok_or(PluginError::Message("editor not created"))?;
        state.set_visible(false);
        Ok(())
    }
}
