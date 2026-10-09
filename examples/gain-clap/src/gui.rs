//! macOS-only embedded GUI; FFI parent attachment stays isolated here.
use super::*;
use clack_extensions::gui::*;
use ravet_surface::{AttachmentError, ParameterAttachment, ParameterControl};
use ravet_webview::{Callbacks, WebView};
use serde_json::{Value, json};
use std::ptr::NonNull;

const SIZE: GuiSize = GuiSize {
    width: 480,
    height: 390,
};
const HTML: &str = include_str!("../editor/index.html");
pub(super) struct GuiState<'a> {
    view: WebView<'a>,
    attached: bool,
    size: GuiSize,
}
struct GainCallbacks<'a> {
    shared: &'a GainShared,
    host: HostMainThreadHandle<'a>,
    params: Option<HostParams>,
    meters: GainMeters,
    error: Option<&'static str>,
    attachments: [ParameterAttachment<'static>; 2],
}
fn parse_edit(
    json: &str,
) -> Result<(&'static str, ravet_foundation::ParameterId, f32), &'static str> {
    if json.len() > 65_536 {
        return Err("Editor message too large");
    }
    let value: Value = serde_json::from_str(json).map_err(|_| "Invalid editor message")?;
    let object = value.as_object().ok_or("Invalid editor message")?;
    if object.len() != 3 {
        return Err("Unknown editor message");
    }
    let kind = match value["type"].as_str() {
        Some("edit") => "edit",
        Some("begin") => "begin",
        Some("value") => "value",
        Some("end") => "end",
        _ => return Err("Unknown editor message"),
    };
    let id = u32::try_from(value["id"].as_u64().ok_or("Invalid parameter ID")?)
        .map_err(|_| "Invalid parameter ID")?;
    let value = value["value"].as_f64().ok_or("Invalid parameter value")? as f32;
    if !value.is_finite() {
        return Err("Invalid parameter value");
    }
    Ok((kind, ravet_foundation::ParameterId(id), value))
}
struct GuiControl<'a> {
    shared: &'a GainShared,
    host: HostMainThreadHandle<'a>,
    params: Option<HostParams>,
}
impl GuiControl<'_> {
    fn notify(&self, result: Result<(), EditError>) -> Result<(), EditError> {
        result?;
        if let Some(params) = self.params {
            params.request_flush(&self.host);
        } else {
            self.host.request_process();
        }
        Ok(())
    }
}
impl ParameterControl for GuiControl<'_> {
    type Error = EditError;
    fn commit(&self, id: ravet_foundation::ParameterId, physical: f32) -> Result<(), EditError> {
        self.notify(queue_edit(self.shared, id, physical))
    }
    fn begin(&self, id: ravet_foundation::ParameterId) -> Result<(), EditError> {
        self.notify(self.shared.edits.begin(
            ClapId::new(id.0),
            SharedGain::generation(self.shared.gain.snapshot()),
        ))
    }
    fn update(&self, id: ravet_foundation::ParameterId, physical: f32) -> Result<(), EditError> {
        self.notify(self.shared.edits.update(ClapId::new(id.0), physical))
    }
    fn end(&self, _: ravet_foundation::ParameterId) -> Result<(), EditError> {
        self.notify(self.shared.edits.end())
    }
}
impl Callbacks for GainCallbacks<'_> {
    fn message(&mut self, json: &str) {
        let result = (|| {
            let (kind, id, value) = parse_edit(json)?;
            let index = parameter_schema().index(id).ok_or("Unknown parameter")?;
            if kind != "edit" && id != GAIN_ID {
                return Err("Unknown gesture parameter");
            }
            let control = GuiControl {
                shared: self.shared,
                host: self.host,
                params: self.params,
            };
            let attachment = &mut self.attachments[index];
            let result = match kind {
                "edit" => attachment.commit(&control, value),
                "begin" => attachment.begin(&control),
                "value" => attachment.update(&control, value),
                _ => attachment.end(&control),
            };
            result.map_err(|error| match error {
                AttachmentError::InvalidValue => "Invalid parameter value",
                AttachmentError::GestureState => "Invalid gesture sequence",
                AttachmentError::Transport(EditError::GestureActive) => {
                    "Finish the current gesture first"
                }
                AttachmentError::Transport(EditError::NoGesture) => "No active gesture",
                AttachmentError::Transport(EditError::Full) => "Host edit queue full; try again",
                AttachmentError::Transport(EditError::UnknownParameter) => "Unknown parameter",
                AttachmentError::Transport(EditError::InvalidValue) => "Invalid parameter value",
            })?;
            Ok(kind)
        })();
        self.error = match result {
            Ok("end") => self.error,
            Ok(_) => None,
            Err(error) => Some(error),
        };
    }
    fn snapshot(&mut self) -> Option<String> {
        let state = self.shared.parameters();
        for (attachment, value) in self.attachments.iter_mut().zip(state.values()) {
            attachment.refresh(*value);
        }
        if let Some(meters) = self.shared.meters() {
            self.meters = meters;
        }
        let reading = |r: ravet_gain::GainMeters, input: bool| {
            let channels = if input { r.input } else { r.output };
            channels.map(|r| json!({"peak":r.peak,"rms":r.rms,"nonfinite":r.nonfinite}))
        };
        Some(json!({"gain":self.attachments[0].physical(),"bypassed":self.attachments[1].physical()==1.0,"input":reading(self.meters,true),"output":reading(self.meters,false),"error":self.error,"pending":self.shared.edits.pending()}).to_string())
    }
}
impl Drop for GainCallbacks<'_> {
    fn drop(&mut self) {
        let control = GuiControl {
            shared: self.shared,
            host: self.host,
            params: self.params,
        };
        for attachment in &mut self.attachments {
            if attachment.gesture_active() {
                let _ = attachment.end(&control);
            }
        }
    }
}
impl PluginGuiImpl for GainMain<'_> {
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
        if !self.is_api_supported(configuration) || self.gui.borrow().is_some() {
            return Err(PluginError::Message(
                "unsupported or already created editor",
            ));
        }
        let callbacks = GainCallbacks {
            shared: self.shared,
            host: self.host,
            params: self.host_params,
            meters: GainMeters::default(),
            error: None,
            attachments: [
                ParameterAttachment::new(parameter_schema(), GAIN_ID).unwrap(),
                ParameterAttachment::new(parameter_schema(), BYPASS_ID).unwrap(),
            ],
        };
        let html = ravet_surface::embed_web_controls(HTML)
            .ok_or(PluginError::Message("invalid local control page"))?;
        let view = WebView::from_html(&html, SIZE.width as f64, SIZE.height as f64, callbacks)
            .map_err(|_| PluginError::Message("native editor creation failed or wrong thread"))?;
        view.set_background(23.0 / 255.0, 29.0 / 255.0, 36.0 / 255.0);
        view.set_visible(false);
        *self.gui.borrow_mut() = Some(GuiState {
            view,
            attached: false,
            size: SIZE,
        });
        Ok(())
    }
    fn destroy(&self) {
        let editor = self.gui.borrow_mut().take();
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
        self.gui.borrow().as_ref().map(|state| state.size)
    }
    fn can_resize(&self) -> bool {
        true
    }
    fn get_resize_hints(&self) -> Option<GuiResizeHints> {
        Some(GuiResizeHints {
            can_resize_horizontally: true,
            can_resize_vertically: true,
            strategy: AspectRatioStrategy::Disregard,
        })
    }
    fn adjust_size(&self, size: GuiSize) -> Option<GuiSize> {
        (size.width >= SIZE.width && size.height >= SIZE.height).then_some(GuiSize {
            width: size.width.min(1200),
            height: size.height.min(900),
        })
    }
    fn set_size(&self, size: GuiSize) -> Result<(), PluginError> {
        if self.adjust_size(size) != Some(size) {
            return Err(PluginError::Message(
                "editor size outside 480x390 to 1200x900",
            ));
        }
        let mut state = self.gui.borrow_mut();
        let state = state
            .as_mut()
            .ok_or(PluginError::Message("editor not created"))?;
        if !state.view.set_size(size.width as f64, size.height as f64) {
            return Err(PluginError::Message("native resize failed"));
        }
        state.size = size;
        Ok(())
    }
    fn set_parent(&self, window: Window) -> Result<(), PluginError> {
        let parent = window
            .as_cocoa_nsview()
            .and_then(NonNull::new)
            .ok_or(PluginError::Message("editor requires Cocoa NSView parent"))?;
        let mut state = self.gui.borrow_mut();
        let state = state
            .as_mut()
            .ok_or(PluginError::Message("editor not created"))?;
        // SAFETY: CLAP host must supply a live NSView on the Cocoa main thread
        // and retain it until detach/destroy, as required by its GUI contract.
        unsafe {
            state.view.attach(parent);
        }
        state.attached = true;
        Ok(())
    }
    fn set_transient(&self, _: Window) -> Result<(), PluginError> {
        Err(PluginError::Message("floating editor unsupported"))
    }
    fn show(&self) -> Result<(), PluginError> {
        let state = self.gui.borrow();
        let state = state
            .as_ref()
            .filter(|s| s.attached)
            .ok_or(PluginError::Message("editor has no parent"))?;
        state.view.set_visible(true);
        Ok(())
    }
    fn hide(&self) -> Result<(), PluginError> {
        let state = self.gui.borrow();
        let state = state
            .as_ref()
            .ok_or(PluginError::Message("editor not created"))?;
        state.view.set_visible(false);
        Ok(())
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn editor_protocol_rejects_malformed_and_nonfinite_messages() {
        assert_eq!(
            parse_edit(r#"{"type":"edit","id":1,"value":0.5}"#),
            Ok(("edit", GAIN_ID, 0.5))
        );
        for text in [
            "null",
            "[]",
            "{",
            r#"{"type":"edit","id":-1,"value":1}"#,
            r#"{"type":"edit","id":1,"value":1e100}"#,
            r#"{"type":"edit","id":1,"value":1,"extra":1}"#,
            r#"{"type":"other","id":1,"value":1}"#,
        ] {
            assert!(parse_edit(text).is_err());
        }
    }
}
