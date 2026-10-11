//! Explicit caller-linked VST3 instance ownership and scoped main-bus f32 audio.
//!
//! The v2 provider is a separate table alongside the catalogue-only v1 ABI.
//! Parent and child control owners stay on the verified platform main thread;
//! [`crate::vst3::instances::Instance::with_audio`] lends one non-cloneable, `Send` but not `Sync` audio
//! capability to an exclusive scope. All safe scoped workers join before native
//! audio teardown on main, including panic unwind and forgotten capabilities.
//!
//! Native modules and provider code remain explicitly trusted. The main f32
//! audio profile accepts separately paired [`crate::vst3::instances::parameters`]
//! and [`crate::vst3::instances::processing`]
//! companions for timed points, copied events, output batches and block context;
//! hosted editor and graph integration remain pending. Stopped state
//! transactions use the separately paired [`crate::vst3::instances::state`]
//! companion. Preparation may
//! allocate; slice processing uses fixed stack metadata. Allocation measurements
//! do not certify host compatibility or a real-time deadline.
//!
//! Ownership restrictions are compile-time contracts:
//!
//! ```compile_fail
//! use ravet_host::vst3::instances::Library;
//! fn require_send<T: Send>() {}
//! require_send::<Library<'static>>();
//! ```
//! ```compile_fail
//! use ravet_host::vst3::instances::Instance;
//! fn require_send<T: Send>() {}
//! require_send::<Instance<'static, 'static>>();
//! ```
//! ```compile_fail
//! use ravet_host::vst3::instances::Audio;
//! fn require_sync<T: Sync>() {}
//! require_sync::<Audio<'static, 'static>>();
//! ```
//! ```compile_fail
//! use ravet_host::vst3::instances::Audio;
//! fn require_clone<T: Clone>() {}
//! require_clone::<Audio<'static, 'static>>();
//! ```
//! ```compile_fail
//! use ravet_host::vst3::instances::Instance;
//! fn escape(child: &mut Instance<'_, '_>) {
//!     child.with_audio(|audio| std::thread::spawn(move || drop(audio))).unwrap();
//! }
//! ```
//! ```compile_fail
//! use ravet_host::vst3::instances::Instance;
//! fn escape_outer_scope(child: &mut Instance<'_, '_>) {
//!     std::thread::scope(|scope| {
//!         child.with_audio(|audio| scope.spawn(move || drop(audio))).unwrap();
//!     });
//! }
//! ```
//! ```compile_fail
//! use ravet_host::vst3::instances::Library;
//! fn close_parent(library: Library<'_>) {
//!     let child = library.create(0).unwrap();
//!     let _ = library.retire();
//!     drop(child);
//! }
//! ```
//! ```compile_fail
//! use ravet_host::vst3::instances::Instance;
//! fn control_during_audio(child: &mut Instance<'_, '_>) {
//!     child.with_audio(|_audio| child.stop()).unwrap();
//! }
//! ```
use super::{ClassRecord, FactoryRecord};
use std::{cell::Cell, ffi::c_void, marker::PhantomData, path::Path, ptr::NonNull, rc::Rc};
type Open = unsafe extern "C" fn(*const u8, u32, *mut *mut c_void, *mut i32) -> i32;
type Close = unsafe extern "C" fn(*mut c_void, *mut i32) -> i32;
#[repr(C)]
pub struct RawProvider {
    pub struct_size: u32,
    pub version: u32,
    pub open: Option<Open>,
    pub factory: Option<unsafe extern "C" fn(*const c_void, *mut FactoryRecord) -> i32>,
    pub class_info: Option<unsafe extern "C" fn(*const c_void, u32, *mut ClassRecord) -> i32>,
    pub close: Option<Close>,
    pub create: Option<unsafe extern "C" fn(*mut c_void, u32, *mut *mut c_void, *mut i32) -> i32>,
    pub prepare: Option<unsafe extern "C" fn(*mut c_void, *const Profile, *mut i32) -> i32>,
    pub start: Option<Close>,
    pub begin_audio: Option<unsafe extern "C" fn(*mut c_void, *mut *mut c_void, *mut i32) -> i32>,
    pub process: Option<unsafe extern "C" fn(*mut c_void, *const Block, *mut i32) -> i32>,
    pub end_audio: Option<Close>,
    pub stop: Option<Close>,
    pub destroy: Option<Close>,
    pub restart_requests: Option<unsafe extern "C" fn(*const c_void, *mut u32) -> i32>,
}
#[repr(C)]
#[derive(Clone, Copy)]
pub struct Profile {
    pub struct_size: u32,
    pub maximum_frames: u32,
    pub input_channels: u32,
    pub output_channels: u32,
    pub sample_rate: f64,
}
impl Profile {
    pub fn new(rate: f64, maximum_frames: u32, inputs: u32, outputs: u32) -> Self {
        Self {
            struct_size: size_of::<Self>() as u32,
            maximum_frames,
            input_channels: inputs,
            output_channels: outputs,
            sample_rate: rate,
        }
    }
}
#[repr(C)]
#[derive(Clone, Copy)]
pub struct InputChannel {
    samples: *const f32,
    capacity: u32,
    reserved: u32,
}
#[repr(C)]
#[derive(Clone, Copy)]
pub struct OutputChannel {
    samples: *mut f32,
    capacity: u32,
    reserved: u32,
}
#[repr(C)]
pub struct Block {
    struct_size: u32,
    frames: u32,
    input_channels: u32,
    output_channels: u32,
    inputs: *const InputChannel,
    outputs: *const OutputChannel,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Error {
    Abi,
    Path,
    Metadata,
    Shape,
    Poisoned,
    Native { status: i32, result: i32 },
}
impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "VST3 instance error: {self:?}")
    }
}
impl std::error::Error for Error {}
fn accepted(status: i32, result: i32) -> Result<(), Error> {
    if status == 0 {
        Ok(())
    } else {
        Err(Error::Native { status, result })
    }
}
#[derive(Clone, Copy)]
pub struct Provider<'a> {
    raw: &'a RawProvider,
    owner: PhantomData<Rc<()>>,
}
impl<'a> Provider<'a> {
    /// # Safety
    /// All callbacks honor exact v2 ownership/thread/range contracts, contain
    /// foreign exceptions, and remain callable on the current platform main
    /// thread. Provider executable/host context code remains loaded until process
    /// exit if pinning or exceptional retained native ownership is possible.
    /// A table borrow does not retain an independently loaded provider DLL.
    pub unsafe fn from_raw(raw: &'a RawProvider) -> Result<Self, Error> {
        if raw.struct_size as usize != size_of::<RawProvider>()
            || raw.version != 2
            || raw.open.is_none()
            || raw.factory.is_none()
            || raw.class_info.is_none()
            || raw.close.is_none()
            || raw.create.is_none()
            || raw.prepare.is_none()
            || raw.start.is_none()
            || raw.begin_audio.is_none()
            || raw.process.is_none()
            || raw.end_audio.is_none()
            || raw.stop.is_none()
            || raw.destroy.is_none()
            || raw.restart_requests.is_none()
        {
            return Err(Error::Abi);
        }
        Ok(Self {
            raw,
            owner: PhantomData,
        })
    }
    /// # Safety
    /// Explicit module and dependencies are trusted compliant native code with
    /// valid pointers/lifetimes and thread behavior; no sandbox is supplied.
    pub unsafe fn load(self, path: impl AsRef<Path>) -> Result<Library<'a>, Error> {
        let path = path.as_ref().to_str().ok_or(Error::Path)?;
        if path.is_empty() || path.len() > 4096 || path.as_bytes().contains(&0) {
            return Err(Error::Path);
        }
        let mut handle = std::ptr::null_mut();
        let mut result = 0;
        // SAFETY: verified code/table/main ownership, bounded UTF8 and exclusive outputs.
        accepted(
            unsafe {
                self.raw.open.unwrap()(path.as_ptr(), path.len() as u32, &mut handle, &mut result)
            },
            result,
        )?;
        let handle = NonNull::new(handle).ok_or(Error::Metadata)?;
        let mut library = Library {
            provider: self,
            handle: Some(handle),
            classes: Vec::new(),
            factory: None,
        };
        let mut factory = FactoryRecord {
            struct_size: size_of::<FactoryRecord>() as u32,
            flags: 0,
            class_count: 0,
            host_context_result: 0,
            vendor: [0; 64],
            url: [0; 256],
            email: [0; 128],
        };
        // SAFETY: live owned module and exclusive exact-sized record.
        accepted(
            unsafe { self.raw.factory.unwrap()(handle.as_ptr(), &mut factory) },
            0,
        )?;
        if factory.struct_size as usize != size_of::<FactoryRecord>()
            || !(1..=256).contains(&factory.class_count)
        {
            return Err(Error::Metadata);
        }
        for (bytes, required) in [
            (&factory.vendor[..], true),
            (&factory.url[..], false),
            (&factory.email[..], false),
        ] {
            text(bytes, required)?;
        }
        library.factory = Some(factory);
        let count = library.factory.as_ref().unwrap().class_count;
        for index in 0..count {
            let mut class = ClassRecord {
                struct_size: size_of::<ClassRecord>() as u32,
                id: [0; 16],
                cardinality: 0,
                class_flags: 0,
                vendor_from_factory: 0,
                category: [0; 32],
                name: [0; 192],
                vendor: [0; 192],
                version: [0; 192],
                sdk_version: [0; 192],
                subcategories: [0; 128],
            };
            // SAFETY: bounded index, live module and exclusive exact record.
            accepted(
                unsafe { self.raw.class_info.unwrap()(handle.as_ptr(), index, &mut class) },
                0,
            )?;
            if class.struct_size as usize != size_of::<ClassRecord>()
                || class.id == [0; 16]
                || class.cardinality <= 0
                || class.vendor_from_factory > 1
                || library
                    .classes
                    .iter()
                    .any(|c: &ClassRecord| c.id == class.id)
            {
                return Err(Error::Metadata);
            }
            for (bytes, required) in [
                (&class.category[..], true),
                (&class.name[..], true),
                (&class.vendor[..], true),
                (&class.version[..], false),
                (&class.sdk_version[..], false),
                (&class.subcategories[..], false),
            ] {
                text(bytes, required)?;
            }
            library.classes.push(class);
        }
        Ok(library)
    }
}
fn text(bytes: &[u8], required: bool) -> Result<&str, Error> {
    let n = bytes.iter().position(|x| *x == 0).ok_or(Error::Metadata)?;
    if required && n == 0 {
        return Err(Error::Metadata);
    }
    std::str::from_utf8(&bytes[..n]).map_err(|_| Error::Metadata)
}
pub struct Library<'a> {
    provider: Provider<'a>,
    handle: Option<NonNull<c_void>>,
    classes: Vec<ClassRecord>,
    factory: Option<FactoryRecord>,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Retirement {
    Complete { context_result: i32 },
    FailedNative { context_result: i32 },
    ProcessPinned,
}
pub struct RetireFailure<'a> {
    pub owner: Library<'a>,
    pub error: Error,
}
impl<'a> Library<'a> {
    pub fn factory(&self) -> &FactoryRecord {
        self.factory.as_ref().unwrap()
    }
    pub fn classes(&self) -> &[ClassRecord] {
        &self.classes
    }
    pub fn create(&self, index: u32) -> Result<Instance<'_, 'a>, Error> {
        let (handle, class_id) = self.create_native(index)?;
        Ok(Instance::new(Parent::Borrowed(self), handle, class_id))
    }
    /// Private bank constructor: the child retains its MAIN Library through an
    /// `Rc`, so no borrowed lifetime is erased or fabricated. Never public.
    #[cfg(feature = "vst3-hosted-audio")]
    pub(crate) fn create_retained(
        self: &Rc<Self>,
        index: u32,
    ) -> Result<Instance<'static, 'a>, Error> {
        let (handle, class_id) = self.create_native(index)?;
        Ok(Instance::new(
            Parent::Retained(Rc::clone(self)),
            handle,
            class_id,
        ))
    }
    fn create_native(&self, index: u32) -> Result<(NonNull<c_void>, [u8; 16]), Error> {
        if index as usize >= self.classes.len() {
            return Err(Error::Shape);
        }
        let mut child = std::ptr::null_mut();
        let mut result = 0;
        // SAFETY: main-owned parent, bounded class and exact exclusive output.
        accepted(
            unsafe {
                self.provider.raw.create.unwrap()(
                    self.handle.unwrap().as_ptr(),
                    index,
                    &mut child,
                    &mut result,
                )
            },
            result,
        )?;
        Ok((
            NonNull::new(child).ok_or(Error::Metadata)?,
            self.classes[index as usize].id,
        ))
    }
    pub fn retire(mut self) -> Result<Retirement, Box<RetireFailure<'a>>> {
        let handle = self.handle.take().unwrap();
        let mut result = 0;
        // SAFETY: !Send/Sync owner remains on verified main; Rust child borrows ended.
        match unsafe { self.provider.raw.close.unwrap()(handle.as_ptr(), &mut result) } {
            0 => Ok(Retirement::Complete {
                context_result: result,
            }),
            7 => Ok(Retirement::FailedNative {
                context_result: result,
            }),
            9 => Ok(Retirement::ProcessPinned),
            status => {
                self.handle = Some(handle);
                Err(Box::new(RetireFailure {
                    owner: self,
                    error: Error::Native { status, result },
                }))
            }
        }
    }
}
impl Drop for Library<'_> {
    fn drop(&mut self) {
        if let Some(handle) = self.handle.take() {
            let mut ignored = 0;
            // SAFETY: verified main/code lifetime; consumed statuses never cause retry.
            let _ = unsafe { self.provider.raw.close.unwrap()(handle.as_ptr(), &mut ignored) };
        }
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InstanceRetirement {
    Complete,
    FailedNative { result: i32 },
}
pub struct InstanceRetireFailure<'m, 'a> {
    pub owner: Instance<'m, 'a>,
    pub error: Error,
}
/// Private parent ownership. Public creation always borrows its Library; only the
/// hosted Graph bank's private constructor retains a `'static` Library on MAIN.
pub(crate) enum Parent<'m, 'a> {
    Borrowed(&'m Library<'a>),
    #[cfg(feature = "vst3-hosted-audio")]
    Retained(Rc<Library<'a>>),
}
impl<'a> std::ops::Deref for Parent<'_, 'a> {
    type Target = Library<'a>;
    fn deref(&self) -> &Library<'a> {
        match self {
            Self::Borrowed(library) => library,
            #[cfg(feature = "vst3-hosted-audio")]
            Self::Retained(library) => library,
        }
    }
}
#[cfg(feature = "vst3-hosted-audio")]
type HostedState<'a> = hosted_audio::State<'a>;
#[cfg(not(feature = "vst3-hosted-audio"))]
type HostedState<'a> = PhantomData<&'a ()>;
#[cfg(feature = "vst3-hosted-audio")]
type HostedAudio<'a> = Option<hosted_audio::AudioState<'a>>;
#[cfg(not(feature = "vst3-hosted-audio"))]
type HostedAudio<'a> = PhantomData<&'a ()>;
pub struct Instance<'m, 'a> {
    library: Parent<'m, 'a>,
    handle: Option<NonNull<c_void>>,
    profile: Option<Profile>,
    running: bool,
    poisoned: bool,
    state_fault: bool,
    parameter_provider: Option<&'a parameters::RawProvider>,
    processing_provider: Option<&'a processing::RawProvider>,
    processing_metadata: Option<(&'a processing::RawProvider, processing::EventBuses)>,
    hosted: HostedState<'a>,
    class_id: [u8; 16],
}
impl<'m, 'a> Instance<'m, 'a> {
    fn new(library: Parent<'m, 'a>, handle: NonNull<c_void>, class_id: [u8; 16]) -> Self {
        Self {
            library,
            handle: Some(handle),
            profile: None,
            running: false,
            poisoned: false,
            state_fault: false,
            parameter_provider: None,
            processing_provider: None,
            processing_metadata: None,
            hosted: HostedState::default(),
            class_id,
        }
    }
    /// Read coalesced native restart flags on the control thread.
    /// This does not clear the flags or reconfigure the instance. The caller
    /// must decide how to retire/rebuild a profile after a restart request.
    pub fn restart_requests(&self) -> Result<u32, Error> {
        if self.poisoned {
            return Err(Error::Poisoned);
        }
        let mut flags = 0;
        // SAFETY: retained main-owned child, exact exclusive output, no native
        // interface lifetime or mutation is transferred by this atomic read.
        accepted(
            unsafe {
                self.library.provider.raw.restart_requests.unwrap()(
                    self.handle.unwrap().as_ptr(),
                    &mut flags,
                )
            },
            0,
        )?;
        Ok(flags)
    }
    pub fn retire(mut self) -> Result<InstanceRetirement, Box<InstanceRetireFailure<'m, 'a>>> {
        if self.poisoned {
            return Err(Box::new(InstanceRetireFailure {
                owner: self,
                error: Error::Poisoned,
            }));
        }
        let handle = self.handle.take().unwrap();
        let mut result = 0;
        // SAFETY: exclusive CONTROL owner, library borrow/code live, no audio scope.
        match unsafe { self.library.provider.raw.destroy.unwrap()(handle.as_ptr(), &mut result) } {
            0 => Ok(InstanceRetirement::Complete),
            7 => Ok(InstanceRetirement::FailedNative { result }),
            status => {
                self.handle = Some(handle);
                Err(Box::new(InstanceRetireFailure {
                    owner: self,
                    error: Error::Native { status, result },
                }))
            }
        }
    }
    pub fn prepare(&mut self, profile: Profile) -> Result<(), Error> {
        self.hosted = HostedState::default();
        self.parameter_provider = None;
        self.processing_provider = None;
        self.processing_metadata = None;
        if self.poisoned || self.state_fault {
            return Err(Error::Poisoned);
        }
        let mut result = 0;
        // SAFETY: exclusive main child and borrowed exact profile; no audio scope.
        accepted(
            unsafe {
                self.library.provider.raw.prepare.unwrap()(
                    self.handle.unwrap().as_ptr(),
                    &profile,
                    &mut result,
                )
            },
            result,
        )?;
        self.profile = Some(profile);
        Ok(())
    }
    pub fn start(&mut self) -> Result<(), Error> {
        if self.poisoned || self.state_fault {
            return Err(Error::Poisoned);
        }
        let mut result = 0;
        // SAFETY: exclusive main child, no audio scope.
        accepted(
            unsafe {
                self.library.provider.raw.start.unwrap()(self.handle.unwrap().as_ptr(), &mut result)
            },
            result,
        )?;
        self.running = true;
        Ok(())
    }
    pub fn stop(&mut self) -> Result<(), Error> {
        if self.poisoned {
            return Err(Error::Poisoned);
        }
        let mut result = 0;
        // SAFETY: scoped audio and all safe worker borrows already joined.
        let status = unsafe {
            self.library.provider.raw.stop.unwrap()(self.handle.unwrap().as_ptr(), &mut result)
        };
        self.running = false;
        accepted(status, result)
    }
    /// Lend the sole audio capability for a non-escaping exclusive scope.
    ///
    /// Move the capability into a joined [`std::thread::scope`] worker for native
    /// processing. A plain `'static` spawned thread cannot capture it, and the
    /// instance cannot be mutated or retired until this scope ends. The native
    /// provider binds first processing to exactly one audio worker; repeatedly
    /// moving the token to different workers within a single scope is rejected.
    /// The capability itself has no native lifecycle or destructor: a control
    /// guard ends audio on main on return/unwind, even if the value is forgotten.
    ///
    /// An unexpected failed native end poisons the child and retains its native
    /// ownership rather than destroying an unproved audio owner. This is a
    /// fail-closed exceptional retention policy, not automatic recovery.
    pub fn with_audio<R>(
        &mut self,
        run: impl for<'s> FnOnce(Audio<'s, 'a>) -> R,
    ) -> Result<R, Error> {
        if self.poisoned || self.state_fault {
            return Err(Error::Poisoned);
        }
        if !self.running {
            return Err(Error::Shape);
        }
        let profile = self.profile.ok_or(Error::Shape)?;
        let raw = self.library.provider.raw;
        let mut token = std::ptr::null_mut();
        let mut result = 0;
        // SAFETY: exclusive main owner, prepared running child and output token.
        accepted(
            unsafe {
                raw.begin_audio.unwrap()(self.handle.unwrap().as_ptr(), &mut token, &mut result)
            },
            result,
        )?;
        let handle = NonNull::new(token).ok_or(Error::Metadata)?;
        let guard = AudioScope {
            instance: self,
            handle: Some(handle),
        };
        let output = run(Audio {
            raw,
            handle,
            profile,
            parameters: guard.instance.parameter_provider,
            processing: guard.instance.processing_provider,
            event_buses: guard
                .instance
                .processing_metadata
                .map_or_else(processing::EventBuses::default, |(_, buses)| buses),
            #[cfg(feature = "vst3-hosted-audio")]
            hosted: guard.instance.hosted.audio(),
            #[cfg(not(feature = "vst3-hosted-audio"))]
            hosted: PhantomData,
            scope: PhantomData,
            no_sync: PhantomData,
        });
        guard.finish()?;
        Ok(output)
    }
}
impl Drop for Instance<'_, '_> {
    fn drop(&mut self) {
        if self.poisoned {
            return;
        } // exceptional unproved AUDIO retirement retains whole native ownership
        if let Some(handle) = self.handle.take() {
            let mut ignored = 0;
            // SAFETY: library borrow/code/main remains live, all safe audio scopes ended.
            let _ = unsafe {
                self.library.provider.raw.destroy.unwrap()(handle.as_ptr(), &mut ignored)
            };
        }
    }
}
struct AudioScope<'s, 'm, 'a> {
    instance: &'s mut Instance<'m, 'a>,
    handle: Option<NonNull<c_void>>,
}
impl AudioScope<'_, '_, '_> {
    fn end(&mut self) -> Result<(), Error> {
        let handle = self.handle.take().unwrap();
        let mut result = 0;
        // SAFETY: HRTB scope ended; safe scoped workers joined on both return/unwind.
        let status = unsafe {
            self.instance.library.provider.raw.end_audio.unwrap()(handle.as_ptr(), &mut result)
        };
        if status != 0 {
            self.instance.poisoned = true
        }
        accepted(status, result)
    }
    fn finish(mut self) -> Result<(), Error> {
        self.end()
    }
}
impl Drop for AudioScope<'_, '_, '_> {
    fn drop(&mut self) {
        if self.handle.is_some() {
            let _ = self.end();
        }
    }
}
pub struct Audio<'s, 'a> {
    raw: &'a RawProvider,
    handle: NonNull<c_void>,
    profile: Profile,
    parameters: Option<&'a parameters::RawProvider>,
    processing: Option<&'a processing::RawProvider>,
    event_buses: processing::EventBuses,
    hosted: HostedAudio<'a>,
    scope: PhantomData<&'s mut ()>,
    no_sync: PhantomData<Cell<()>>,
}
// SAFETY: verified provider explicitly supports exclusive AUDIO handoff, lifetime
// scope cannot escape, sole token has no interface ownership/destructor; !Sync.
unsafe impl Send for Audio<'_, '_> {}
impl Audio<'_, '_> {
    /// Whether this capability was lent from a hosted 32/64 aux-bus profile.
    /// Legacy main-f32 entry points refuse such a capability before native calls.
    fn hosted_mode(&self) -> bool {
        #[cfg(feature = "vst3-hosted-audio")]
        {
            self.hosted.is_some()
        }
        #[cfg(not(feature = "vst3-hosted-audio"))]
        {
            false
        }
    }
    /// Process exact prepared main-bus channels into caller-owned planar slices.
    /// Shapes, active lengths and the prepared maximum are checked before native
    /// calls. Zero frames are admitted. Backing tails remain untouched; safe
    /// mutable slices are distinct, while the raw native ABI separately accepts
    /// validated input/output aliases through prepared input scratch.
    /// Known native DSP faults silence valid output prefixes and return the
    /// original provider/native status. Invalid slice shapes preserve outputs.
    pub fn process(
        &mut self,
        inputs: &[&[f32]],
        outputs: &mut [&mut [f32]],
        frames: u32,
    ) -> Result<(), Error> {
        if self.hosted_mode() {
            return Err(Error::Shape);
        }
        if inputs.len() != self.profile.input_channels as usize
            || outputs.len() != self.profile.output_channels as usize
            || inputs.len() > 2
            || outputs.len() > 2
            || frames > self.profile.maximum_frames
        {
            return Err(Error::Shape);
        }
        let mut input = [InputChannel {
            samples: std::ptr::null(),
            capacity: 0,
            reserved: 0,
        }; 2];
        let mut output = [OutputChannel {
            samples: std::ptr::null_mut(),
            capacity: 0,
            reserved: 0,
        }; 2];
        for (i, s) in inputs.iter().enumerate() {
            if s.len() < frames as usize {
                return Err(Error::Shape);
            }
            input[i] = InputChannel {
                samples: s.as_ptr(),
                capacity: u32::try_from(s.len()).map_err(|_| Error::Shape)?,
                reserved: 0,
            };
        }
        for (i, s) in outputs.iter_mut().enumerate() {
            if s.len() < frames as usize {
                return Err(Error::Shape);
            }
            output[i] = OutputChannel {
                samples: s.as_mut_ptr(),
                capacity: u32::try_from(s.len()).map_err(|_| Error::Shape)?,
                reserved: 0,
            };
        }
        let block = Block {
            struct_size: size_of::<Block>() as u32,
            frames,
            input_channels: inputs.len() as u32,
            output_channels: outputs.len() as u32,
            inputs: if inputs.is_empty() {
                std::ptr::null()
            } else {
                input.as_ptr()
            },
            outputs: output.as_ptr(),
        };
        let mut result = 0;
        // SAFETY: unique audio token, valid disjoint mutable slices and bounded
        // immutable inputs/stack metadata; native provider preflights whole block.
        accepted(
            unsafe { self.raw.process.unwrap()(self.handle.as_ptr(), &block, &mut result) },
            result,
        )
    }
}

#[cfg(test)]
#[path = "instance_tests.rs"]
mod tests;

/// Owned stopped component/controller state and its separately paired ABI.
#[path = "state.rs"]
pub mod state;

/// Owned parameter controls and bounded timed DSP input through a paired table.
pub mod parameters;

/// Exact-paired owned events, parameter output and block-start context.
pub mod processing;

/// Hosted native f32/f64 main/aux bus profiles, rich processing and the owned
/// MAIN-retired Graph bank. Default-disabled `vst3-hosted-audio` feature.
#[cfg(feature = "vst3-hosted-audio")]
pub mod hosted_audio;

// Main-thread hosted editor view ownership (macOS `NSView`); the module's own
// documentation resolves its links. Default-disabled `vst3-editor` feature.
#[cfg(feature = "vst3-editor")]
pub mod editor;
