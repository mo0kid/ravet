//! Coalesced CLAP lifecycle requests, deferred to consumer-owned scheduling.
use clack_host::{
    host::{HostHandlers, SharedHandler},
    plugin::PluginInstance,
};
use std::sync::{
    Arc,
    atomic::{AtomicU8, Ordering},
};
const RESTART: u8 = 1;
const PROCESS: u8 = 2;
const CALLBACK: u8 = 4;

/// Coalesced requested lifecycle actions. Multiple identical requests merge.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct PendingHostRequests {
    /// Stop/deactivate and reactivate under the consumer's lifecycle policy.
    pub restart: bool,
    /// Activate/start or wake processing under the consumer's scheduling policy.
    pub process: bool,
    /// Invoke the plugin's main-thread callback outside the requesting callback.
    pub callback: bool,
}

/// Shared request mailbox; clone once during preparation, never inside callbacks.
/// Request publication and draining use a single atomic byte, no allocation,
/// lock or I/O. Requests are level-triggered, not a lossless command sequence.
#[derive(Clone, Debug, Default)]
pub struct HostRequests(Arc<AtomicU8>);
impl HostRequests {
    /// Allocates shared storage during host preparation.
    pub fn new() -> Self {
        Self::default()
    }
    /// Atomically drains all currently pending kinds. Requests racing after the
    /// exchange remain pending. Consumers must act on or explicitly defer them.
    pub fn take(&self) -> PendingHostRequests {
        let flags = self.0.swap(0, Ordering::AcqRel);
        PendingHostRequests {
            restart: flags & RESTART != 0,
            process: flags & PROCESS != 0,
            callback: flags & CALLBACK != 0,
        }
    }
    /// Publishes selected kinds, for explicit deferral or scheduling adapters.
    pub fn defer(&self, requests: PendingHostRequests) {
        let flags = (u8::from(requests.restart) * RESTART)
            | (u8::from(requests.process) * PROCESS)
            | (u8::from(requests.callback) * CALLBACK);
        self.0.fetch_or(flags, Ordering::Release);
    }
    /// Services at most one coalesced callback on the instance's main thread,
    /// preserving restart/process requests. A callback that requests another
    /// callback leaves it pending for the next consumer tick (no unbounded loop).
    /// The supplied mailbox must belong to this instance.
    pub fn service_callback<H: HostHandlers>(&self, instance: &mut PluginInstance<H>) -> bool {
        let flags = self.0.fetch_and(!CALLBACK, Ordering::AcqRel);
        if flags & CALLBACK == 0 {
            return false;
        }
        instance.call_on_main_thread_callback();
        true
    }
}
impl SharedHandler<'_> for HostRequests {
    fn request_restart(&self) {
        self.0.fetch_or(RESTART, Ordering::Release);
    }
    fn request_process(&self) {
        self.0.fetch_or(PROCESS, Ordering::Release);
    }
    fn request_callback(&self) {
        self.0.fetch_or(CALLBACK, Ordering::Release);
    }
}
/// Minimal Clack host handlers with meaningful core request handoff.
/// This does not advertise host extensions, create a window/device, wake a native
/// event loop or execute restart/process transitions automatically.
pub struct RequestHost;
impl HostHandlers for RequestHost {
    type Shared<'a> = HostRequests;
    type MainThread<'a> = ();
    type AudioProcessor<'a> = ();
}
