//! Main-thread ownership of a hosted VST3 `IPlugView` editor (macOS `NSView`).
//!
//! `RavetVst3EditorProviderV1` is an additive companion paired with the exact
//! instance ABI-v2 table. [`Provider::from_raw`] checks size, version, every
//! entry and a non-null origin; [`Instance::open_editor`] additionally checks
//! pointer identity with the instance's own v2 table before any native call.
//! Existing v1/v2, state, parameter, processing and hosted-audio layouts and
//! result domains are unchanged.
//!
//! The contracts mirror the CLAP [`PluginEditor`](crate::editor::PluginEditor):
//!
//! - [`Editor`] exclusively borrows its [`Instance`], which borrows its
//!   `Library`. While the guard is alive the borrow checker prevents retiring
//!   the instance or the library, and the native provider independently refuses
//!   child destruction (`LIFECYCLE`) and module close (`CHILDREN`) while an
//!   editor lives. A view released through [`Editor::close`] or drop is
//!   therefore released before the component/controller retire and before the
//!   module closes.
//! - **Accepted limitation.** Because the guard holds `&mut Instance`, the
//!   scoped Rust calls that also need `&mut Instance` (`with_audio`, `prepare`,
//!   `start`, `stop` and state/parameter refresh) require the editor to be closed
//!   first. An editor and running audio on the same instance are shown only
//!   through the raw native tables (the native editor entries do not take the
//!   live-core CONTROL admission). A shared-borrow guard for Graph-bank or
//!   long-running children is future work (decision 0271).
//! - Leak behaviour: [`std::mem::forget`] on the guard ends the borrow without
//!   destroying the native editor. The native child then refuses destruction,
//!   so dropping the `Instance` leaks the native child (its drop ignores the
//!   refusal), and dropping the `Library` leaks the module (`CHILDREN`). This is
//!   a deliberate memory-safe leak, never a use-after-free. If a drop-time
//!   destroy is itself refused (thread or reentry, which the `!Send` guard and
//!   the absence of Rust callbacks make unreachable in practice), the native
//!   editor is kept alive deterministically and [`refused_drops`] increments.
//! - Creation calls `createView("editor")` on the platform main thread, requires
//!   `NSView` support, installs a host `IPlugFrame` and admits only an initial
//!   size of 1..=16384 per dimension. A missing editor is
//!   [`Error::Unavailable`]; an unsupported platform is [`Error::Unsupported`].
//! - [`Editor::attach`] is `unsafe`: the caller's `NSView` must stay alive until
//!   a successful [`Editor::detach`] or until the guard is closed or dropped.
//!   [`Editor::attached`] follows the native owner's authoritative state, which
//!   is reported with every owner reply, refusals included.
//! - [`Editor::resize`] calls `canResize`, then `checkSizeConstraint` (only
//!   `kResultTrue`, which equals `kResultOk`, accepts the adjusted rectangle;
//!   `kResultFalse`, `kNotImplemented` or any other result is a returned
//!   refusal with no unconstrained fallback), then `onSize`. The recorded size
//!   is the `getSize` re-queried after `onSize` when that is valid.
//! - A plug-in `IPlugFrame::resizeView` is handled natively on the main thread.
//!   During host `attached` or `setContentScaleFactor` the request is
//!   acknowledged and honoured later: it is applied after the call while the
//!   view is attached, otherwise at the next successful attach. A superseded
//!   request, a request refused by the plug-in's own `onSize`, or one lost to a
//!   fault or teardown is counted in [`State::deferred_discarded`]. During any
//!   other host call it is refused as reentry. When idle and attached, `onSize`
//!   is called in the same call stack. Idle requests to a detached view are
//!   refused. Applied plug-in sizes go to a one-slot mailbox read by
//!   [`Editor::take_resize_request`]; the consumer resizes its own container.
//!   Detach, teardown and a successful host resize clear the mailbox. No callback
//!   ever enters Rust, and no view, frame or controller pointer is exposed.
//! - [`Editor::close`] and drop run the ordered teardown: `removed` if attached,
//!   `setFrame(nullptr)`, then release. Thread and reentry refusals happen
//!   before teardown starts and return the guard; once teardown starts it
//!   always consumes the native editor.
//!
//! Window creation, event-loop pumping, keyboard and focus forwarding, and
//! accessibility are consumer responsibilities and are not provided here.
//! Editor edit gestures reach the existing component handler, which does not
//! adopt them. Evidence is local macOS arm64 only.
//!
//! `Provider` and `Editor` are not `Send`. The first snippet fails only on the
//! `Send` bound; the control below compiles the same harness with a `Send`
//! type. Error codes are not asserted because stable rustdoc does not check them.
//!
//! ```compile_fail
//! fn require_send<T: Send>() {}
//! require_send::<ravet_host::vst3::instances::editor::Provider<'static>>();
//! ```
//!
//! ```compile_fail
//! fn require_send<T: Send>() {}
//! require_send::<ravet_host::vst3::instances::editor::Editor<'static, 'static, 'static>>();
//! ```
//!
//! ```
//! fn require_send<T: Send>() {}
//! require_send::<ravet_host::vst3::instances::editor::Size>();
//! ```
//!
//! An instance cannot be retired while its editor is open. The first snippet
//! differs from the compiling control only in the position of `drop(editor)`:
//!
//! ```compile_fail
//! use ravet_host::vst3::instances::{Library, editor::{Platform, Provider}};
//! fn retire_while_open(library: &Library<'static>, provider: Provider<'static>) {
//!     let mut instance = library.create(0).unwrap();
//!     let editor = instance.open_editor(provider, Platform::NsView).unwrap();
//!     let _ = instance.retire();
//!     drop(editor);
//! }
//! ```
//!
//! ```no_run
//! use ravet_host::vst3::instances::{Library, editor::{Platform, Provider}};
//! fn retire_after_close(library: &Library<'static>, provider: Provider<'static>) {
//!     let mut instance = library.create(0).unwrap();
//!     let editor = instance.open_editor(provider, Platform::NsView).unwrap();
//!     drop(editor);
//!     let _ = instance.retire();
//! }
//! ```
use super::{Instance, RawProvider as InstanceProvider};
use std::{
    ffi::c_void,
    marker::PhantomData,
    ptr::NonNull,
    rc::Rc,
    sync::atomic::{AtomicU64, Ordering},
};

static REFUSED_DROPS: AtomicU64 = AtomicU64::new(0);
/// Process-wide count of guards whose drop-time native destroy was refused.
/// Each such native editor (and therefore its child and module) is
/// deliberately kept alive rather than released out of order.
pub fn refused_drops() -> u64 {
    REFUSED_DROPS.load(Ordering::Relaxed)
}

/// Smallest admitted editor extent per dimension.
pub const MINIMUM_EXTENT: u32 = 1;
/// Largest admitted editor extent per dimension (the CLAP editor bound).
pub const MAXIMUM_EXTENT: u32 = 16384;
const STATUS_OK: i32 = 0;
const STATUS_THREAD: i32 = 5;
const STATUS_LIFECYCLE: i32 = 10;
const STATUS_NATIVE: i32 = 4;
const STATUS_METADATA: i32 = 3;
const STATUS_RETIRED_NATIVE: i32 = 7;
const STATUS_UNAVAILABLE: i32 = 16;
const STATUS_UNSUPPORTED: i32 = 17;
const STATUS_REENTRY: i32 = 18;
const STATUS_EMPTY: i32 = 19;
const FLAG_RETURNED: u32 = 1;
const FLAG_THROWN: u32 = 2;
const FLAG_ATTACHED: u32 = 4;
const FLAG_DEFERRED: u32 = 8;
const FLAG_STATE: u32 = 16;

/// `RavetVst3EditorSize` (16 bytes).
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct RawSize {
    pub struct_size: u32,
    pub width: u32,
    pub height: u32,
    pub reserved: u32,
}
impl RawSize {
    fn initial() -> Self {
        Self {
            struct_size: size_of::<Self>() as u32,
            ..Self::default()
        }
    }
    fn of(size: Size) -> Self {
        Self {
            width: size.width,
            height: size.height,
            ..Self::initial()
        }
    }
}
/// `RavetVst3EditorReport` (24 bytes).
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct RawReport {
    pub struct_size: u32,
    pub stage: u32,
    pub failure: u32,
    pub flags: u32,
    pub native_result: i32,
    pub reserved: u32,
}
impl RawReport {
    fn initial() -> Self {
        Self {
            struct_size: size_of::<Self>() as u32,
            ..Self::default()
        }
    }
}
/// `RavetVst3EditorState` (40 bytes).
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct RawState {
    pub struct_size: u32,
    pub flags: u32,
    pub width: u32,
    pub height: u32,
    pub frame_calls: u32,
    pub frame_applied: u32,
    pub frame_refused: u32,
    pub frame_reentry_refused: u32,
    pub deferred_discarded: u32,
    pub size_mismatches: u32,
}
type Create =
    unsafe extern "C" fn(*mut c_void, u32, *mut *mut c_void, *mut RawSize, *mut RawReport) -> i32;
type Attach = unsafe extern "C" fn(*mut c_void, *mut c_void, *mut RawSize, *mut RawReport) -> i32;
type GetSize = unsafe extern "C" fn(*mut c_void, *mut RawSize, *mut RawReport) -> i32;
type Resize =
    unsafe extern "C" fn(*mut c_void, *const RawSize, *mut RawSize, *mut RawReport) -> i32;
type Scale = unsafe extern "C" fn(*mut c_void, f32, *mut RawSize, *mut RawReport) -> i32;
type Detach = unsafe extern "C" fn(*mut c_void, *mut RawReport) -> i32;
type GetState = unsafe extern "C" fn(*const c_void, *mut RawState) -> i32;
type TakeRequest = unsafe extern "C" fn(*mut c_void, *mut RawSize) -> i32;
/// `RavetVst3EditorProviderV1` (88 bytes on 64-bit targets). Nullable entries
/// are rejected before any call.
#[repr(C)]
pub struct RawProvider {
    pub struct_size: u32,
    pub version: u32,
    pub instance_origin: *const InstanceProvider,
    pub create: Option<Create>,
    pub attach: Option<Attach>,
    pub size: Option<GetSize>,
    pub resize: Option<Resize>,
    pub set_scale: Option<Scale>,
    pub detach: Option<Detach>,
    pub state: Option<GetState>,
    pub take_request: Option<TakeRequest>,
    pub destroy: Option<Detach>,
}
/// Verified editor companion table, restricted to the main thread.
#[derive(Clone, Copy)]
pub struct Provider<'a> {
    raw: &'a RawProvider,
    no_send_sync: PhantomData<Rc<()>>,
}
impl<'a> Provider<'a> {
    /// # Safety
    /// The table and its exact origin are trusted compliant native code that
    /// stays loaded for `'a` (and until process exit once a core can be pinned
    /// or a plug-in can retain the host frame). Entries contain foreign
    /// exceptions, check the platform main thread, and implement the documented
    /// ordering. A table borrow does not keep an unloaded DLL alive.
    pub unsafe fn from_raw(raw: &'a RawProvider) -> Result<Self, Error> {
        if raw.struct_size as usize != size_of::<RawProvider>()
            || raw.version != 1
            || raw.instance_origin.is_null()
            || raw.create.is_none()
            || raw.attach.is_none()
            || raw.size.is_none()
            || raw.resize.is_none()
            || raw.set_scale.is_none()
            || raw.detach.is_none()
            || raw.state.is_none()
            || raw.take_request.is_none()
            || raw.destroy.is_none()
        {
            return Err(Error::Abi);
        }
        Ok(Self {
            raw,
            no_send_sync: PhantomData,
        })
    }
}
/// Native editor platform. Only macOS `NSView` is defined.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Platform {
    NsView,
}
impl Platform {
    fn raw(self) -> u32 {
        match self {
            Self::NsView => 1,
        }
    }
}
/// Bounded editor extent in logical points (1..=16384 per dimension).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Size {
    width: u32,
    height: u32,
}
impl Size {
    /// `None` outside 1..=16384 per dimension.
    pub fn new(width: u32, height: u32) -> Option<Self> {
        let valid = (MINIMUM_EXTENT..=MAXIMUM_EXTENT).contains(&width)
            && (MINIMUM_EXTENT..=MAXIMUM_EXTENT).contains(&height);
        valid.then_some(Self { width, height })
    }
    pub fn width(self) -> u32 {
        self.width
    }
    pub fn height(self) -> u32 {
        self.height
    }
    fn admit(raw: RawSize) -> Result<Self, Error> {
        if raw.struct_size as usize != size_of::<RawSize>() || raw.reserved != 0 {
            return Err(Error::Abi);
        }
        Self::new(raw.width, raw.height).ok_or(Error::Abi)
    }
}
/// Classified native failure (`RavetVst3EditorReport::failure`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Failure {
    None,
    Local,
    Returned,
    Thrown,
    Capacity,
    Allocation,
    Malformed,
    Unavailable,
    Unsupported,
    Reentry,
}
/// Checked native report. A foreign `tresult` is present only when the SDK call
/// actually returned it; a throw never invents one.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Report {
    raw: RawReport,
    failure: Failure,
}
impl Report {
    fn admit(raw: RawReport) -> Result<Self, Error> {
        let failure = match raw.failure {
            0 => Failure::None,
            1 => Failure::Local,
            2 => Failure::Returned,
            3 => Failure::Thrown,
            4 => Failure::Capacity,
            5 => Failure::Allocation,
            6 => Failure::Malformed,
            7 => Failure::Unavailable,
            8 => Failure::Unsupported,
            9 => Failure::Reentry,
            _ => return Err(Error::Abi),
        };
        let returned = raw.flags & FLAG_RETURNED != 0;
        let thrown = raw.flags & FLAG_THROWN != 0;
        if raw.struct_size as usize != size_of::<RawReport>()
            || raw.reserved != 0
            || !(200..=213).contains(&raw.stage)
            || raw.flags
                & !(FLAG_RETURNED | FLAG_THROWN | FLAG_ATTACHED | FLAG_DEFERRED | FLAG_STATE)
                != 0
            || (raw.flags & FLAG_STATE == 0 && raw.flags & (FLAG_ATTACHED | FLAG_DEFERRED) != 0)
            || returned != (failure == Failure::Returned)
            || thrown != (failure == Failure::Thrown)
            || (!returned && raw.native_result != 0)
        {
            return Err(Error::Abi);
        }
        Ok(Self { raw, failure })
    }
    /// 200 admission, 201 createView, 202 isPlatformTypeSupported, 203 setFrame,
    /// 204 getSize, 205 attached, 206 canResize, 207 checkSizeConstraint,
    /// 208 onSize, 209 setContentScaleFactor, 210 removed, 211 setFrame(nullptr),
    /// 212 release, 213 queryInterface.
    pub fn stage(&self) -> u32 {
        self.raw.stage
    }
    pub fn failure(&self) -> Failure {
        self.failure
    }
    /// The SDK `tresult`, only when the failing call returned one.
    pub fn native_result(&self) -> Option<i32> {
        (self.failure == Failure::Returned).then_some(self.raw.native_result)
    }
    /// The live owner's authoritative attached state after the call, refusals
    /// included (a conservative attached state follows a throwing `attached`).
    /// `None` for provider-level replies that read no owner state.
    pub fn attached(&self) -> Option<bool> {
        (self.raw.flags & FLAG_STATE != 0).then_some(self.raw.flags & FLAG_ATTACHED != 0)
    }
    /// A plug-in resize requested during `attached`/scale was applied afterwards.
    pub fn deferred_resize_applied(&self) -> bool {
        self.raw.flags & FLAG_DEFERRED != 0
    }
}
/// Editor companion error.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Error {
    /// Malformed table, foreign origin, malformed record or report.
    Abi,
    /// The instance is poisoned or state-faulted.
    Poisoned,
    /// Local size or content-scale admission failed; no native call was made.
    Size,
    /// Called off the provider's main thread.
    Thread,
    /// The controller supplied no editor (or the child has no controller).
    Unavailable(Report),
    /// Platform, resizing or content scale is not supported by the view.
    Unsupported(Report),
    /// Refused because an editor or instance entry is already running.
    Reentry,
    /// Refused in the current state (already attached, not attached, faulted).
    Lifecycle(Option<Report>),
    /// Returned SDK refusal, throw or malformed SDK size.
    Native(Report),
    /// Any other native status (invalid argument, allocation, capacity).
    Status { status: i32, report: Option<Report> },
}
impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "VST3 editor error: {self:?}")
    }
}
impl std::error::Error for Error {}
fn classify(status: i32, raw: Option<RawReport>) -> Result<(), Error> {
    let report = match raw {
        Some(raw) if status != STATUS_THREAD && status != STATUS_REENTRY && raw.stage != 0 => {
            Some(Report::admit(raw)?)
        }
        _ => None,
    };
    if status == STATUS_OK {
        return match report {
            Some(report) if report.failure != Failure::None => Err(Error::Abi),
            _ => Ok(()),
        };
    }
    if report.is_some_and(|report| report.failure == Failure::None) {
        return Err(Error::Abi);
    }
    Err(match (status, report) {
        (STATUS_THREAD, _) => Error::Thread,
        (STATUS_REENTRY, _) => Error::Reentry,
        (STATUS_LIFECYCLE, report) => Error::Lifecycle(report),
        (STATUS_UNAVAILABLE, Some(report)) => Error::Unavailable(report),
        (STATUS_UNSUPPORTED, Some(report)) => Error::Unsupported(report),
        (STATUS_NATIVE | STATUS_METADATA, Some(report)) => Error::Native(report),
        (STATUS_UNAVAILABLE | STATUS_UNSUPPORTED | STATUS_NATIVE | STATUS_METADATA, None) => {
            Error::Abi
        }
        (status, report) => Error::Status { status, report },
    })
}
/// Copied editor flags and saturating frame counters.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct State {
    raw: RawState,
}
impl State {
    pub fn attached(&self) -> bool {
        self.raw.flags & 1 != 0
    }
    /// Result of the last `canResize` query.
    pub fn resizable(&self) -> bool {
        self.raw.flags & 2 != 0
    }
    pub fn content_scale_supported(&self) -> bool {
        self.raw.flags & 4 != 0
    }
    pub fn resize_request_pending(&self) -> bool {
        self.raw.flags & 8 != 0
    }
    pub fn faulted(&self) -> bool {
        self.raw.flags & 16 != 0
    }
    /// Last host-known applied size.
    pub fn size(&self) -> Option<Size> {
        Size::new(self.raw.width, self.raw.height)
    }
    pub fn frame_calls(&self) -> u32 {
        self.raw.frame_calls
    }
    pub fn frame_applied(&self) -> u32 {
        self.raw.frame_applied
    }
    pub fn frame_refused(&self) -> u32 {
        self.raw.frame_refused
    }
    pub fn frame_reentry_refused(&self) -> u32 {
        self.raw.frame_reentry_refused
    }
    /// An acknowledged deferred request waits for the next successful attach.
    pub fn deferred_request_pending(&self) -> bool {
        self.raw.flags & 32 != 0
    }
    /// Acknowledged deferred requests that were superseded, refused by the
    /// plug-in's own `onSize`, or lost to a fault or teardown.
    pub fn deferred_discarded(&self) -> u32 {
        self.raw.deferred_discarded
    }
    /// `onSize` calls whose re-queried `getSize` differed or failed.
    pub fn size_mismatches(&self) -> u32 {
        self.raw.size_mismatches
    }
}
/// Consuming close outcome; both variants prove the view was released.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Closed {
    Complete,
    /// Teardown completed but a foreign call (`removed`, `setFrame(nullptr)` or
    /// release) refused or threw; the first failure is reported.
    FailedNative(Report),
    /// The native owner was consumed but returned a malformed report.
    Unverified,
}
/// A refused close keeps the editor for explicit retry on the main thread.
pub struct CloseFailure<'i, 'm, 'a> {
    pub owner: Editor<'i, 'm, 'a>,
    pub error: Error,
}
/// Main-thread guard owning one hosted VST3 editor view.
pub struct Editor<'i, 'm, 'a> {
    instance: &'i mut Instance<'m, 'a>,
    raw: &'a RawProvider,
    handle: Option<NonNull<c_void>>,
    attached: bool,
    size: Size,
    no_send_sync: PhantomData<Rc<()>>,
}
impl<'m, 'a> Instance<'m, 'a> {
    /// Create the controller's editor view on the main thread.
    ///
    /// The returned guard exclusively borrows this instance. Refuses a foreign
    /// table, a poisoned instance, a missing editor, an unsupported platform, a
    /// second live editor, and an unbounded initial size. A malformed record
    /// from a successful native creation is destroyed before returning
    /// [`Error::Abi`].
    pub fn open_editor(
        &mut self,
        provider: Provider<'a>,
        platform: Platform,
    ) -> Result<Editor<'_, 'm, 'a>, Error> {
        if !std::ptr::eq(provider.raw.instance_origin, self.library.provider.raw) {
            return Err(Error::Abi);
        }
        if self.poisoned || self.state_fault {
            return Err(Error::Poisoned);
        }
        let child = self.handle.ok_or(Error::Abi)?.as_ptr();
        let mut handle = std::ptr::null_mut();
        let mut initial = RawSize::initial();
        let mut report = RawReport::initial();
        // SAFETY: verified exact-origin table, live main-thread child, null
        // output handle and distinct initialized output records.
        let status = unsafe {
            provider.raw.create.unwrap()(
                child,
                platform.raw(),
                &mut handle,
                &mut initial,
                &mut report,
            )
        };
        let checked = classify(status, Some(report));
        let handle = match (checked, NonNull::new(handle)) {
            (Ok(()), Some(handle)) => handle,
            (Ok(()), None) => return Err(Error::Abi),
            (Err(error), None) => return Err(error),
            (Err(error), Some(handle)) => {
                // A handle published with a failure status is still destroyed.
                destroy(provider.raw, handle);
                return Err(error);
            }
        };
        let size = match Size::admit(initial) {
            Ok(size) => size,
            Err(error) => {
                destroy(provider.raw, handle);
                return Err(error);
            }
        };
        Ok(Editor {
            instance: self,
            raw: provider.raw,
            handle: Some(handle),
            attached: false,
            size,
            no_send_sync: PhantomData,
        })
    }
}
fn destroy(raw: &RawProvider, handle: NonNull<c_void>) -> i32 {
    let mut report = RawReport::initial();
    // SAFETY: an editor handle produced by this exact table on the main thread.
    unsafe { raw.destroy.unwrap()(handle.as_ptr(), &mut report) }
}
impl<'i, 'm, 'a> Editor<'i, 'm, 'a> {
    fn handle(&self) -> *mut c_void {
        self.handle.expect("live editor handle").as_ptr()
    }
    /// Shared access to the guarded instance (no replacement or retirement).
    pub fn instance(&self) -> &Instance<'m, 'a> {
        self.instance
    }
    /// The native owner's attached state as of the last owner reply. Refusals
    /// (a second attach, operations after a fault, reentry) report the true
    /// state and never clear it; replies without owner state leave it unchanged.
    pub fn attached(&self) -> bool {
        self.attached
    }
    /// Last size admitted by this guard.
    pub fn last_size(&self) -> Size {
        self.size
    }
    fn track(&mut self, report: &RawReport) {
        if let Ok(Some(attached)) = Report::admit(*report).map(|report| report.attached()) {
            self.attached = attached;
        }
    }
    /// Attach the view to a caller-owned `NSView` and return the size afterwards
    /// (including a plug-in resize deferred during `attached`).
    ///
    /// # Safety
    /// `parent` must be a live `NSView` owned by the caller on the main thread.
    /// It must remain alive until a successful [`detach`](Self::detach) or until
    /// this guard is closed or dropped. The plug-in may add subviews to it.
    /// A Rust pointer alone does not prove the native object's lifetime.
    pub unsafe fn attach(&mut self, parent: NonNull<c_void>) -> Result<Size, Error> {
        let mut applied = RawSize::initial();
        let mut report = RawReport::initial();
        // SAFETY: exact live editor; caller's parent obligation forwarded.
        let status = unsafe {
            self.raw.attach.unwrap()(self.handle(), parent.as_ptr(), &mut applied, &mut report)
        };
        self.track(&report);
        classify(status, Some(report))?;
        self.size = Size::admit(applied)?;
        Ok(self.size)
    }
    /// Call `removed`. A returned refusal keeps the view attached and retryable.
    pub fn detach(&mut self) -> Result<(), Error> {
        let mut report = RawReport::initial();
        // SAFETY: exact live editor on the main thread, initialized report.
        let status = unsafe { self.raw.detach.unwrap()(self.handle(), &mut report) };
        self.track(&report);
        classify(status, Some(report))
    }
    /// Query `getSize`.
    pub fn size(&mut self) -> Result<Size, Error> {
        let mut output = RawSize::initial();
        let mut report = RawReport::initial();
        // SAFETY: exact live editor, distinct initialized records.
        let status = unsafe { self.raw.size.unwrap()(self.handle(), &mut output, &mut report) };
        self.track(&report);
        classify(status, Some(report))?;
        self.size = Size::admit(output)?;
        Ok(self.size)
    }
    /// Host-initiated resize: `canResize`, `checkSizeConstraint`, then `onSize`
    /// with the bounded adjusted size, which is returned.
    pub fn resize(&mut self, proposed: Size) -> Result<Size, Error> {
        let input = RawSize::of(proposed);
        let mut applied = RawSize::initial();
        let mut report = RawReport::initial();
        // SAFETY: exact live editor, distinct initialized records.
        let status =
            unsafe { self.raw.resize.unwrap()(self.handle(), &input, &mut applied, &mut report) };
        self.track(&report);
        classify(status, Some(report))?;
        self.size = Size::admit(applied)?;
        Ok(self.size)
    }
    /// Set a content scale factor in 0.25..=8.0 through the optional
    /// `IPlugViewContentScaleSupport`. Returns the size afterwards. macOS views
    /// normally report [`Error::Unsupported`]: Cocoa supplies backing scale.
    pub fn set_content_scale(&mut self, factor: f32) -> Result<Size, Error> {
        if !factor.is_finite() || !(0.25..=8.0).contains(&factor) {
            return Err(Error::Size);
        }
        let mut applied = RawSize::initial();
        let mut report = RawReport::initial();
        // SAFETY: exact live editor, bounded factor, distinct records.
        let status = unsafe {
            self.raw.set_scale.unwrap()(self.handle(), factor, &mut applied, &mut report)
        };
        self.track(&report);
        classify(status, Some(report))?;
        self.size = Size::admit(applied)?;
        Ok(self.size)
    }
    /// Consume the latest size applied for a plug-in `resizeView` request, if
    /// any. The consumer resizes its own container to it.
    pub fn take_resize_request(&mut self) -> Result<Option<Size>, Error> {
        let mut output = RawSize::initial();
        // SAFETY: exact live editor, initialized output record.
        match unsafe { self.raw.take_request.unwrap()(self.handle(), &mut output) } {
            STATUS_EMPTY => Ok(None),
            STATUS_OK => {
                let size = Size::admit(output)?;
                self.size = size;
                Ok(Some(size))
            }
            status => {
                classify(status, None)?;
                Err(Error::Abi)
            }
        }
    }
    /// Copied flags and frame counters; no foreign call.
    pub fn state(&self) -> Result<State, Error> {
        let mut raw = RawState {
            struct_size: size_of::<RawState>() as u32,
            ..RawState::default()
        };
        // SAFETY: exact live editor, initialized output record.
        let status = unsafe { self.raw.state.unwrap()(self.handle(), &mut raw) };
        classify(status, None)?;
        if raw.struct_size as usize != size_of::<RawState>() || raw.flags & !63 != 0 {
            return Err(Error::Abi);
        }
        Ok(State { raw })
    }
    /// Ordered consuming teardown: `removed` if attached, `setFrame(nullptr)`,
    /// then release. A thread or reentry refusal returns the guard for retry.
    pub fn close(mut self) -> Result<Closed, Box<CloseFailure<'i, 'm, 'a>>> {
        let handle = self.handle.take().expect("live editor handle");
        let mut report = RawReport::initial();
        // SAFETY: exact live editor on the main thread, initialized report.
        let status = unsafe { self.raw.destroy.unwrap()(handle.as_ptr(), &mut report) };
        match status {
            STATUS_OK => Ok(match Report::admit(report) {
                Ok(report) if report.failure == Failure::None => Closed::Complete,
                _ => Closed::Unverified,
            }),
            STATUS_RETIRED_NATIVE => Ok(match Report::admit(report) {
                Ok(report) if matches!(report.failure, Failure::Returned | Failure::Thrown) => {
                    Closed::FailedNative(report)
                }
                _ => Closed::Unverified,
            }),
            status => {
                self.handle = Some(handle);
                let error = classify(status, Some(report)).err().unwrap_or(Error::Abi);
                Err(Box::new(CloseFailure { owner: self, error }))
            }
        }
    }
}
impl Drop for Editor<'_, '_, '_> {
    fn drop(&mut self) {
        if let Some(handle) = self.handle.take() {
            // OK and RETIRED_NATIVE consume. Any other status means the native
            // editor was refused before teardown started: it is kept alive (the
            // child and module then also refuse retirement) and recorded rather
            // than released out of order. Consuming statuses are never retried.
            let status = destroy(self.raw, handle);
            if status != STATUS_OK && status != STATUS_RETIRED_NATIVE {
                REFUSED_DROPS.fetch_add(1, Ordering::Relaxed);
            }
        }
    }
}

#[cfg(test)]
mod tests;
