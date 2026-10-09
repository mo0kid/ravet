use clack_extensions::gui::{GuiConfiguration, GuiError, GuiSize, PluginGui, Window};
use clack_host::prelude::{HostHandlers, PluginInstance, PluginMainThreadHandle};

#[derive(Clone, Copy, Debug)]
pub enum EditorError {
    Unavailable,
    Unsupported,
    ParentRequired,
    Size,
    Gui(GuiError),
}
impl From<GuiError> for EditorError {
    fn from(error: GuiError) -> Self {
        Self::Gui(error)
    }
}
impl std::fmt::Display for EditorError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "editor error: {self:?}")
    }
}
impl std::error::Error for EditorError {}
fn bounded(size: GuiSize) -> bool {
    (1..=16384).contains(&size.width) && (1..=16384).contains(&size.height)
}
/// Main-thread ownership guard for a trusted CLAP plugin's native GUI.
/// Exclusive instance borrowing keeps the plugin/library alive until GUI destroy.
/// Creation negotiates the explicit API/floating mode. Embedded show requires a
/// successfully attached caller-owned parent; parent pointer lifetime remains an
/// explicit unsafe native contract. Drop hides a shown GUI and always destroys it.
/// Native parent creation, event-loop pumping, host GUI requests, focus/scale,
/// application-close and consumer callback scheduling are not automatic.
pub struct PluginEditor<'a, H: HostHandlers> {
    instance: &'a mut PluginInstance<H>,
    gui: PluginGui,
    floating: bool,
    attached: bool,
    shown: bool,
}
impl<'a, H: HostHandlers> PluginEditor<'a, H> {
    /// Called on the plugin/OS main thread. Failure before successful create
    /// supplies no guard; successful create is paired with destroy even when
    /// default embedded size is rejected (outside 1..16384 pixels per dimension).
    pub fn create(
        instance: &'a mut PluginInstance<H>,
        configuration: GuiConfiguration<'_>,
    ) -> Result<Self, EditorError> {
        let handle = instance.plugin_handle();
        let gui = handle
            .get_extension::<PluginGui>()
            .ok_or(EditorError::Unavailable)?;
        if !gui.is_api_supported(&handle, configuration) {
            return Err(EditorError::Unsupported);
        }
        gui.create(&handle, configuration)?;
        if !configuration.is_floating && gui.get_size(&handle).is_none_or(|size| !bounded(size)) {
            gui.destroy(&handle);
            return Err(EditorError::Size);
        }
        Ok(Self {
            instance,
            gui,
            floating: configuration.is_floating,
            attached: false,
            shown: false,
        })
    }
    /// A temporary main-thread handle permits explicit parameter/state operations
    /// without moving/replacing the borrowed instance or leaking the guard lifetime.
    pub fn plugin_handle(&mut self) -> PluginMainThreadHandle<'_> {
        self.instance.plugin_handle()
    }
    pub fn size(&mut self) -> Option<GuiSize> {
        self.gui.get_size(&self.instance.plugin_handle())
    }
    /// Attach an embedded GUI. Only a successful native call marks it attached.
    ///
    /// # Safety
    /// The supplied handle must reference a compatible live native parent on its
    /// OS UI thread, and the parent must remain alive until this guard is dropped.
    /// Rust handle lifetime alone does not validate the underlying native object.
    #[allow(unsafe_code)]
    pub unsafe fn set_parent(&mut self, parent: Window<'_, '_>) -> Result<(), EditorError> {
        if self.floating {
            return Err(EditorError::Unsupported);
        }
        // SAFETY: forwarded caller's native parent/thread/lifetime obligation.
        unsafe { self.gui.set_parent(&self.instance.plugin_handle(), parent) }?;
        self.attached = true;
        Ok(())
    }
    /// Set a floating GUI's transient native parent.
    ///
    /// # Safety
    /// Parent and OS UI thread must meet the same live-through-drop requirements
    /// as set_parent, including matching the plugin's negotiated native API.
    #[allow(unsafe_code)]
    pub unsafe fn set_transient(&mut self, parent: Window<'_, '_>) -> Result<(), EditorError> {
        if !self.floating {
            return Err(EditorError::Unsupported);
        }
        // SAFETY: forwarded caller's native parent/thread/lifetime obligation.
        unsafe {
            self.gui
                .set_transient(&self.instance.plugin_handle(), parent)
        }?;
        Ok(())
    }
    /// Negotiate/apply bounded embedded size. Non-resizable/floating GUIs reject;
    /// returned size is the plugin-adjusted size only after successful set_size.
    pub fn resize(&mut self, proposed: GuiSize) -> Result<GuiSize, EditorError> {
        if !bounded(proposed) {
            return Err(EditorError::Size);
        }
        let handle = self.instance.plugin_handle();
        if self.floating || !self.gui.can_resize(&handle) {
            return Err(EditorError::Unsupported);
        }
        let adjusted = self
            .gui
            .adjust_size(&handle, proposed)
            .filter(|size| bounded(*size))
            .ok_or(EditorError::Size)?;
        self.gui.set_size(&handle, adjusted)?;
        Ok(adjusted)
    }
    /// Idempotent show/hide track only successful native transitions.
    pub fn show(&mut self) -> Result<(), EditorError> {
        if !self.floating && !self.attached {
            return Err(EditorError::ParentRequired);
        }
        if !self.shown {
            self.gui.show(&self.instance.plugin_handle())?;
            self.shown = true;
        }
        Ok(())
    }
    pub fn hide(&mut self) -> Result<(), EditorError> {
        if self.shown {
            self.gui.hide(&self.instance.plugin_handle())?;
            self.shown = false;
        }
        Ok(())
    }
    pub fn shown(&self) -> bool {
        self.shown
    }
    /// Explicitly service a plugin main-thread callback when its host requested
    /// one. Consumers coalesce/schedule those requests through their own handlers.
    pub fn main_thread_callback(&mut self) {
        self.instance.call_on_main_thread_callback();
    }
    /// Service at most one coalesced callback from this instance's host mailbox,
    /// preserving restart/process requests. The consumer schedules main ticks;
    /// no polling loop is created and a callback re-request waits for another tick.
    pub fn service_callback(&mut self, requests: &crate::HostRequests) -> bool {
        requests.service_callback(self.instance)
    }
}
impl<H: HostHandlers> Drop for PluginEditor<'_, H> {
    fn drop(&mut self) {
        if self.shown {
            let _ = self.gui.hide(&self.instance.plugin_handle());
        }
        self.gui.destroy(&self.instance.plugin_handle());
    }
}
