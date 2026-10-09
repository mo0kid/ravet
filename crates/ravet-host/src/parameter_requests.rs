//! Deferred host parameter extension callbacks, with explicit bounded clear loss.
use crate::HostRequests;
use clack_extensions::params::{
    HostParams, HostParamsImplMainThread, HostParamsImplShared, ParamClearFlags, ParamRescanFlags,
};
use clack_host::{
    host::{HostExtensions, HostHandlers, MainThreadHandler, SharedHandler},
    utils::ClapId,
};
use std::{
    cell::RefCell,
    rc::Rc,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
};

/// One exact stable-ID reference invalidation. No parameter value edit is implied.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ParameterClear {
    pub id: ClapId,
    pub flags: ParamClearFlags,
}
/// Main-thread changes, bounded to 64 distinct IDs between drains. Repeated IDs
/// merge flags in first-observed order. Overflow retains the prefix and marks a
/// host fault: the consumer must reconcile its references or reject the instance,
/// rather than assume this partial snapshot is complete or clear unrelated IDs.
#[derive(Clone, Copy, Debug)]
pub struct PendingParameterChanges {
    pub rescan: ParamRescanFlags,
    pub overflow: bool,
    clears: [Option<ParameterClear>; 64],
    len: usize,
}
impl Default for PendingParameterChanges {
    fn default() -> Self {
        Self {
            rescan: ParamRescanFlags::empty(),
            overflow: false,
            clears: [None; 64],
            len: 0,
        }
    }
}
impl PendingParameterChanges {
    pub fn clears(&self) -> impl ExactSizeIterator<Item = ParameterClear> + '_ {
        self.clears[..self.len].iter().map(|item| item.unwrap())
    }
}
/// Main-thread-only shared ownership; clone during preparation. Callback changes
/// never execute plugin code, allocate, clear consumer data or restart processing.
/// Drain outside callbacks; rescan ALL requires deactivation before inspection.
#[derive(Clone, Default)]
pub struct ParameterMainRequests(Rc<RefCell<PendingParameterChanges>>);
impl ParameterMainRequests {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn take(&self) -> PendingParameterChanges {
        self.0.replace(PendingParameterChanges::default())
    }
}
impl MainThreadHandler<'_> for ParameterMainRequests {}
impl HostParamsImplMainThread for ParameterMainRequests {
    fn rescan(&self, flags: ParamRescanFlags) {
        self.0.borrow_mut().rescan |= flags;
    }
    fn clear(&self, id: ClapId, flags: ParamClearFlags) {
        if flags.is_empty() {
            return;
        }
        let mut pending = self.0.borrow_mut();
        let len = pending.len;
        for clear in pending.clears[..len].iter_mut().flatten() {
            if clear.id == id {
                clear.flags |= flags;
                return;
            }
        }
        if len == 64 {
            pending.overflow = true;
            return;
        }
        pending.clears[len] = Some(ParameterClear { id, flags });
        pending.len += 1;
    }
}
/// Thread-safe coalesced flush request plus the existing core lifecycle mailbox.
/// A drain racing with publication leaves that publication for this or the next
/// tick. A consumed flush must schedule process/flush under the consumer's active
/// state policy; this wrapper never invokes concurrent process and flush calls.
#[derive(Clone, Default)]
pub struct ParameterSharedRequests {
    core: HostRequests,
    flush: Arc<AtomicBool>,
}
impl ParameterSharedRequests {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn core(&self) -> &HostRequests {
        &self.core
    }
    pub fn take_flush(&self) -> bool {
        self.flush.swap(false, Ordering::AcqRel)
    }
    pub fn defer_flush(&self) {
        self.flush.store(true, Ordering::Release);
    }
}
impl HostParamsImplShared for ParameterSharedRequests {
    fn request_flush(&self) {
        self.defer_flush();
    }
}
impl SharedHandler<'_> for ParameterSharedRequests {
    fn request_restart(&self) {
        self.core.request_restart();
    }
    fn request_process(&self) {
        self.core.request_process();
    }
    fn request_callback(&self) {
        self.core.request_callback();
    }
}
/// Explicit parameter-aware host handlers. Prepare one shared and one main-thread
/// mailbox per instance and pass clones into the respective instance factories.
/// Consumers must service changes/flush/core requests; no automatic rescan,
/// automation storage, native event loop or lifecycle scheduler is installed.
pub struct ParameterHost;
impl HostHandlers for ParameterHost {
    type Shared<'a> = ParameterSharedRequests;
    type MainThread<'a> = ParameterMainRequests;
    type AudioProcessor<'a> = ();
    fn declare_extensions(builder: &mut HostExtensions<Self>, _: &Self::Shared<'_>) {
        builder.register::<HostParams>();
    }
}
