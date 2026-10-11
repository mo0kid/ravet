//! Owned opaque stopped VST3 component/controller state.
//!
//! Capture uses explicit bounded capacities, copies SDK streams and publishes a
//! complete snapshot only after both getters succeed within their supported
//! profile. Restore runs component, controller-component and optional controller
//! state in order. A late refusal can follow an already committed component;
//! there is no rollback. Failed foreign restore poisons further processing while
//! stopped retirement remains available. Successful restore requires preparation.
//! State bytes authenticate neither plug-in schema/version nor trusted native code.
//!
//! ```compile_fail
//! use ravet_host::vst3::instances::state::Provider;
//! fn requires_send<T: Send>() {}
//! requires_send::<Provider<'static>>();
//! ```
//! ```compile_fail
//! use ravet_host::vst3::instances::{Instance, state::{Provider, CaptureCapacity}};
//! fn state_during_audio(instance: &mut Instance<'_, '_>, provider: Provider<'_>) {
//!     instance.with_audio(|_| instance.capture_state(provider,
//!         CaptureCapacity::new(1024, 1024).unwrap())).unwrap();
//! }
use super::{Instance, RawProvider as InstanceProvider};
use std::{ffi::c_void, marker::PhantomData, rc::Rc};
const MAXIMUM_BYTES: usize = 8 * 1024 * 1024;
const MUTATING: u32 = 1;
const THROWN: u32 = 2;
const POISONED: u32 = 4;
const PREPARE_REQUIRED: u32 = 8;
const ATTEMPTED: u32 = 16;
const RETURNED: u32 = 32;

#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Record {
    pub struct_size: u32,
    pub controller_mode: u32,
    pub component_size: u32,
    pub controller_size: u32,
    pub class_id: [u8; 16],
}
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RawReport {
    pub struct_size: u32,
    pub stage: u32,
    pub stream_failure: u32,
    pub flags: u32,
    pub native_result: i32,
    pub controller_capture_result: i32,
}
type Capture = unsafe extern "C" fn(
    *mut c_void,
    *mut Record,
    *mut u8,
    u32,
    *mut u8,
    u32,
    *mut RawReport,
) -> i32;
type Restore =
    unsafe extern "C" fn(*mut c_void, *const Record, *const u8, *const u8, *mut RawReport) -> i32;
/// Exact-table companion to an independently validated instance-provider v2.
#[repr(C)]
pub struct RawProvider {
    pub struct_size: u32,
    pub version: u32,
    pub provider: *const InstanceProvider,
    pub capture: Option<Capture>,
    pub restore: Option<Restore>,
}
/// Main-owned copied table borrow. It retains no independently loaded DLL.
#[derive(Clone, Copy)]
pub struct Provider<'a> {
    raw: &'a RawProvider,
    owner: PhantomData<Rc<()>>,
}
impl<'a> Provider<'a> {
    /// # Safety
    /// The exact companion and pointed instance table honor the C ownership,
    /// thread, full-preflight, exception, stream-retention and poisoning contracts.
    /// Their executable/context code stays loaded through every owner and until
    /// process exit after pinning. Native code and readable records are trusted.
    pub unsafe fn from_raw(raw: &'a RawProvider) -> Result<Self, Error> {
        if raw.struct_size as usize != size_of::<RawProvider>()
            || raw.version != 1
            || raw.provider.is_null()
            || raw.capture.is_none()
            || raw.restore.is_none()
        {
            return Err(Error::Abi);
        }
        Ok(Self {
            raw,
            owner: PhantomData,
        })
    }
}
/// Explicit fixed stream budgets, each 1..8 MiB. No growing sizing retry occurs.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CaptureCapacity {
    component: u32,
    controller: u32,
}
impl CaptureCapacity {
    pub fn new(component: usize, controller: usize) -> Result<Self, Error> {
        if !(1..=MAXIMUM_BYTES).contains(&component) || !(1..=MAXIMUM_BYTES).contains(&controller) {
            return Err(Error::Shape);
        }
        Ok(Self {
            component: component as u32,
            controller: controller as u32,
        })
    }
}
/// Controller presence and serialization support are independent. Bytes may be
/// empty and still represent a successfully captured supported controller.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ControllerState {
    Absent,
    Unsupported,
    Bytes(Vec<u8>),
}
/// Copied opaque bytes and class UID; no SDK handle, stream or provider borrow.
/// Snapshots can move between threads, but restoring requires a main-owned child.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Snapshot {
    class_id: [u8; 16],
    component: Vec<u8>,
    controller: ControllerState,
}
impl Snapshot {
    /// Admit caller-owned bytes without interpreting a plug-in's schema/version.
    /// Identity/presence and bounded lengths are checked again before restoration.
    pub fn new(
        class_id: [u8; 16],
        component: Vec<u8>,
        controller: ControllerState,
    ) -> Result<Self, Error> {
        if class_id == [0; 16]
            || component.len() > MAXIMUM_BYTES
            || matches!(&controller, ControllerState::Bytes(bytes) if bytes.len() > MAXIMUM_BYTES)
        {
            return Err(Error::Shape);
        }
        Ok(Self {
            class_id,
            component,
            controller,
        })
    }
    pub fn class_id(&self) -> [u8; 16] {
        self.class_id
    }
    pub fn component(&self) -> &[u8] {
        &self.component
    }
    pub fn controller(&self) -> &ControllerState {
        &self.controller
    }
    fn record(&self) -> Record {
        let (controller_mode, controller_size) = match &self.controller {
            ControllerState::Absent => (0, 0),
            ControllerState::Unsupported => (1, 0),
            ControllerState::Bytes(bytes) => (2, bytes.len() as u32),
        };
        Record {
            struct_size: size_of::<Record>() as u32,
            controller_mode,
            component_size: self.component.len() as u32,
            controller_size,
            class_id: self.class_id,
        }
    }
}
/// Exact SDK operation which first failed; None is a fully completed transaction.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Stage {
    Complete,
    ComponentCapture,
    ControllerCapture,
    ComponentRestore,
    ControllerComponentRestore,
    ControllerRestore,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StreamFailure {
    Protocol,
    Admission,
    SourceWrite,
    Capacity,
}
/// Original returned SDK status stays distinct from local stream/exception faults.
/// A caught foreign exception has no original returned SDK status.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Report {
    raw: RawReport,
}
impl Report {
    fn initial() -> RawReport {
        RawReport {
            struct_size: size_of::<RawReport>() as u32,
            stage: 0,
            stream_failure: 0,
            flags: 0,
            native_result: 0,
            controller_capture_result: 0,
        }
    }
    fn admitted(raw: RawReport) -> Result<Self, Error> {
        if raw.struct_size as usize != size_of::<RawReport>()
            || raw.stage > 5
            || raw.stream_failure > 4
            || raw.flags & !63 != 0
            || (raw.flags & RETURNED != 0 && raw.flags & THROWN != 0)
            || (raw.flags & (RETURNED | THROWN) != 0 && raw.flags & ATTEMPTED == 0)
        {
            return Err(Error::Abi);
        }
        Ok(Self { raw })
    }
    pub fn stage(self) -> Stage {
        match self.raw.stage {
            0 => Stage::Complete,
            1 => Stage::ComponentCapture,
            2 => Stage::ControllerCapture,
            3 => Stage::ComponentRestore,
            4 => Stage::ControllerComponentRestore,
            _ => Stage::ControllerRestore,
        }
    }
    pub fn stream_failure(self) -> Option<StreamFailure> {
        match self.raw.stream_failure {
            0 => None,
            1 => Some(StreamFailure::Protocol),
            2 => Some(StreamFailure::Admission),
            3 => Some(StreamFailure::SourceWrite),
            _ => Some(StreamFailure::Capacity),
        }
    }
    pub fn returned_status(self) -> Option<i32> {
        (self.raw.flags & RETURNED != 0).then_some(self.raw.native_result)
    }
    pub fn attempted(self) -> bool {
        self.raw.flags & ATTEMPTED != 0
    }
    pub fn threw(self) -> bool {
        self.raw.flags & THROWN != 0
    }
    pub fn poisoned(self) -> bool {
        self.raw.flags & POISONED != 0
    }
    pub fn prepare_required(self) -> bool {
        self.raw.flags & PREPARE_REQUIRED != 0
    }
    /// Meaningful on successful capture, alongside Snapshot::controller().
    pub fn controller_capture_status(self) -> i32 {
        self.raw.controller_capture_result
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Error {
    Abi,
    Shape,
    Allocation,
    Lifecycle,
    Poisoned,
    Provider { status: i32, report: Report },
}
impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "VST3 state error: {self:?}")
    }
}
impl std::error::Error for Error {}
/// Complete captured snapshot and its original controller support result.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Captured {
    pub snapshot: Snapshot,
    pub report: Report,
}
fn zeroed(capacity: u32) -> Result<Vec<u8>, Error> {
    let mut bytes = Vec::new();
    bytes
        .try_reserve_exact(capacity as usize)
        .map_err(|_| Error::Allocation)?;
    bytes.resize(capacity as usize, 0);
    Ok(bytes)
}
impl Instance<'_, '_> {
    fn state_admission(&self, provider: Provider<'_>) -> Result<(), Error> {
        if !std::ptr::eq(provider.raw.provider, self.library.provider.raw) {
            return Err(Error::Abi);
        }
        if self.poisoned || self.state_fault {
            return Err(Error::Poisoned);
        }
        if self.running {
            return Err(Error::Lifecycle);
        }
        Ok(())
    }
    /// Capture stopped component and optional controller state, allocating bounded
    /// owned output on CONTROL. Failure publishes no partially captured snapshot.
    /// Foreign getters may mutate their own history; there is no getter rollback.
    pub fn capture_state(
        &mut self,
        provider: Provider<'_>,
        capacity: CaptureCapacity,
    ) -> Result<Captured, Error> {
        self.state_admission(provider)?;
        let mut component = zeroed(capacity.component)?;
        let mut controller = zeroed(capacity.controller)?;
        let mut record = Record {
            struct_size: size_of::<Record>() as u32,
            controller_mode: 0,
            component_size: 0,
            controller_size: 0,
            class_id: [0; 16],
        };
        let mut raw_report = Report::initial();
        // SAFETY: exact paired provider and exclusive stopped child; disjoint
        // owned byte arrays/metadata live through the synchronous bounded call.
        let status = unsafe {
            provider.raw.capture.unwrap()(
                self.handle.unwrap().as_ptr(),
                &mut record,
                component.as_mut_ptr(),
                capacity.component,
                controller.as_mut_ptr(),
                capacity.controller,
                &mut raw_report,
            )
        };
        let report = Report::admitted(raw_report)?;
        if matches!(
            report.stage(),
            Stage::ComponentRestore | Stage::ControllerComponentRestore | Stage::ControllerRestore
        ) || raw_report.flags & (MUTATING | POISONED | PREPARE_REQUIRED) != 0
        {
            return Err(Error::Abi);
        }
        if status != 0 {
            return Err(Error::Provider { status, report });
        }
        if record.struct_size as usize != size_of::<Record>()
            || record.class_id != self.class_id
            || record.controller_mode > 2
            || record.component_size > capacity.component
            || record.controller_size > capacity.controller
            || (record.controller_mode != 2 && record.controller_size != 0)
            || report.stage() != Stage::Complete
            || report.stream_failure().is_some()
            || report.returned_status() != Some(0)
            || raw_report.flags & (MUTATING | POISONED | PREPARE_REQUIRED) != 0
        {
            return Err(Error::Abi);
        }
        component.truncate(record.component_size as usize);
        controller.truncate(record.controller_size as usize);
        let controller = match record.controller_mode {
            0 => ControllerState::Absent,
            1 => ControllerState::Unsupported,
            _ => ControllerState::Bytes(controller),
        };
        Ok(Captured {
            snapshot: Snapshot::new(record.class_id, component, controller)?,
            report,
        })
    }
    /// Restore an opaque same-class snapshot. SDK/controller/stream failure after
    /// mutation poisons further state/preparation/audio, but joined stop/retire
    /// remains safe. A new child is recovery. Success invalidates the old profile
    /// and requires explicit preparation; restart requests remain uncleared.
    pub fn restore_state(
        &mut self,
        provider: Provider<'_>,
        snapshot: &Snapshot,
    ) -> Result<Report, Error> {
        self.state_admission(provider)?;
        if snapshot.class_id != self.class_id {
            return Err(Error::Shape);
        }
        let record = snapshot.record();
        let mut raw_report = Report::initial();
        let controller = match &snapshot.controller {
            ControllerState::Bytes(bytes) => bytes.as_ptr(),
            _ => std::ptr::null(),
        };
        // SAFETY: exact paired provider, exclusive stopped owner and immutable
        // bounded owned snapshot sources; native creates independent SDK copies.
        let status = unsafe {
            provider.raw.restore.unwrap()(
                self.handle.unwrap().as_ptr(),
                &record,
                snapshot.component.as_ptr(),
                controller,
                &mut raw_report,
            )
        };
        // A malformed post-call report cannot prove mutation did not happen;
        // retain a stopped, safely retirable child but refuse further processing.
        let report = match Report::admitted(raw_report) {
            Ok(report) => report,
            Err(error) => {
                self.state_fault = true;
                self.profile = None;
                self.processing_provider = None;
                self.processing_metadata = None;
                self.hosted = Default::default();
                return Err(error);
            }
        };
        // A failed preflight has no foreign attempt/stage/stream fault. Once a
        // restore operation was attempted, the paired provider must report both
        // mutation and poison. Missing or contradictory facts cannot establish
        // that the previous prepared profile is safe to process.
        if status != 0 {
            let preflight = raw_report.flags == 0
                && report.stage() == Stage::Complete
                && report.stream_failure().is_none();
            let failure_flags = MUTATING | POISONED | ATTEMPTED;
            let foreign_failure = matches!(
                report.stage(),
                Stage::ComponentRestore
                    | Stage::ControllerComponentRestore
                    | Stage::ControllerRestore
            ) && (raw_report.flags == failure_flags | RETURNED
                || raw_report.flags == failure_flags | THROWN)
                && (report.threw()
                    || report.returned_status() != Some(0)
                    || report.stream_failure().is_some());
            if !preflight && !foreign_failure {
                self.state_fault = true;
                self.profile = None;
                self.processing_provider = None;
                self.processing_metadata = None;
                self.hosted = Default::default();
                return Err(Error::Abi);
            }
        }
        if report.poisoned() || report.raw.flags & MUTATING != 0 {
            self.profile = None;
            self.processing_provider = None;
            self.processing_metadata = None;
            self.hosted = Default::default();
            if status != 0 {
                self.state_fault = true;
            }
        }
        if status != 0 {
            return Err(Error::Provider { status, report });
        }
        if report.stage() != Stage::Complete
            || report.stream_failure().is_some()
            || report.returned_status() != Some(0)
            || report.poisoned()
            || !report.prepare_required()
            || report.raw.flags & MUTATING == 0
        {
            self.state_fault = true;
            self.profile = None;
            self.processing_provider = None;
            self.processing_metadata = None;
            self.hosted = Default::default();
            return Err(Error::Abi);
        }
        self.profile = None;
        self.processing_provider = None;
        self.processing_metadata = None;
        self.hosted = Default::default();
        Ok(report)
    }
}

#[cfg(test)]
#[path = "state_tests.rs"]
mod tests;
