//! Optional caller-linked macOS AUv3 discovery, asynchronous ownership and scoped audio.
//!
//! Loading explicitly executes trusted native code. Apple completion remains
//! asynchronous; callers service the platform main queue and poll without blocking.
//! Load options are requests: [`InstanceInfo::observed_in_process`](crate::au::InstanceInfo::observed_in_process) records the
//! actual rendering mode. Prepared main-bus f32 rendering uses the public cached
//! Audio Unit render block; this foundation does not certify IPC isolation,
//! third-party compatibility, devices or real-time deadlines.
//!
//! CONTROL owners stay on platform main. One non-cloneable, `Send` but not `Sync`
//! capability is lent to a joined exclusive scope. Retirement occurs on main even
//! on unwind or forgotten capabilities. Caller-linked provider code is process-lifetime.
//!
//! ```compile_fail
//! fn send<T: Send>() {} send::<ravet_host::au::Catalogue<'static>>();
//! ```
//! ```compile_fail
//! fn send<T: Send>() {} send::<ravet_host::au::Pending<'static>>();
//! ```
//! ```compile_fail
//! fn send<T: Send>() {} send::<ravet_host::au::Instance<'static>>();
//! ```
//! ```compile_fail
//! fn sync<T: Sync>() {} sync::<ravet_host::au::Audio<'static, 'static>>();
//! ```
//! ```compile_fail
//! fn clone<T: Clone>() {} clone::<ravet_host::au::Audio<'static, 'static>>();
//! ```
//! ```compile_fail
//! fn escape(i: &mut ravet_host::au::Instance<'_>) {
//!     i.with_audio(|audio| std::thread::spawn(move || drop(audio))).unwrap();
//! }
//! ```
//! ```compile_fail
//! fn escape_outer(i: &mut ravet_host::au::Instance<'_>) {
//!     std::thread::scope(|s| { i.with_audio(|a| s.spawn(move || drop(a))).unwrap(); });
//! }
//! ```
//! ```compile_fail
//! fn control(i: &mut ravet_host::au::Instance<'_>) {
//!     i.with_audio(|_audio| i.stop()).unwrap();
//! }
//! ```
#![allow(unsafe_code)]
// Exact bounded foreign diagnostics stay inline so failure on AUDIO allocates
// no Box/String. The fixed report is intentionally larger than typical errors.
#![allow(clippy::result_large_err)]
use std::{cell::Cell, ffi::c_void, marker::PhantomData, ptr::NonNull, rc::Rc};
/// Exact host/native result domains. Numeric zero without `returned` is not an
/// actual native success. `consumed` describes ownership retirement, not DSP.
#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct Diagnostic {
    pub byte_size: u32,
    pub attempted: u32,
    pub returned: u32,
    pub exception: u32,
    pub has_os_status: u32,
    pub has_error: u32,
    pub domain_length: u32,
    pub consumed: u32,
    pub os_status: i32,
    reserved2: u32,
    pub error_code: i64,
    domain: [u8; 256],
}
impl Default for Diagnostic {
    fn default() -> Self {
        Self {
            byte_size: size_of::<Self>() as u32,
            attempted: 0,
            returned: 0,
            exception: 0,
            has_os_status: 0,
            has_error: 0,
            domain_length: 0,
            consumed: 0,
            os_status: 0,
            reserved2: 0,
            error_code: 0,
            domain: [0; 256],
        }
    }
}
impl Diagnostic {
    /// Original UTF8 NSError domain, without allocating on AUDIO. NSError's
    /// signed 64-bit code is separate from a returned signed 32-bit OSStatus.
    pub fn error_domain(&self) -> Option<&str> {
        (self.has_error == 1)
            .then(|| {
                self.domain
                    .get(..self.domain_length as usize)
                    .and_then(|s| std::str::from_utf8(s).ok())
            })
            .flatten()
    }
    fn valid(&self) -> bool {
        self.byte_size == size_of::<Self>() as u32
            && self.returned <= self.attempted
            && self.exception <= 1
            && self.has_os_status <= 1
            && self.has_error <= 1
            && self.consumed <= 1
            && self.reserved2 == 0
            && self.domain_length <= 256
            && (self.has_os_status == 0 || self.returned > 0)
            && (self.has_error == 0 || self.returned > 0)
            && std::str::from_utf8(&self.domain[..self.domain_length as usize]).is_ok()
    }
}
#[derive(Clone, Copy, Debug)]
#[allow(clippy::large_enum_variant)] // bounded inline report avoids AUDIO allocation
pub enum Error {
    Shape,
    Metadata,
    Retired,
    Poisoned,
    Host { status: i32, diagnostic: Diagnostic },
}
impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "AU host error: {self:?}")
    }
}
impl std::error::Error for Error {}
fn report(status: i32, d: Diagnostic) -> Result<Diagnostic, Error> {
    if !d.valid() || !(0..=9).contains(&status) || (status == 0 && d.exception != 0) {
        return Err(Error::Metadata);
    }
    if status == 0 {
        Ok(d)
    } else {
        Err(Error::Host {
            status,
            diagnostic: d,
        })
    }
}
#[repr(C)]
#[derive(Clone, Copy)]
pub struct Filter {
    byte_size: u32,
    pub component_type: u32,
    pub subtype: u32,
    pub manufacturer: u32,
    pub maximum_components: u32,
    reserved: u32,
}
impl Filter {
    /// Explicit platform registry filter. Type is required; subtype/manufacturer
    /// may be zero wildcards. Bounds reject rather than silently truncate.
    pub fn new(
        component_type: u32,
        subtype: u32,
        manufacturer: u32,
        maximum_components: u32,
    ) -> Self {
        Self {
            byte_size: size_of::<Self>() as u32,
            component_type,
            subtype,
            manufacturer,
            maximum_components,
            reserved: 0,
        }
    }
}
#[repr(C)]
#[derive(Clone, Copy)]
pub struct ComponentRecord {
    byte_size: u32,
    component_type: u32,
    subtype: u32,
    manufacturer: u32,
    flags: u32,
    version: u32,
    name_length: u32,
    reserved: u32,
    name: [u8; 4096],
}
impl Default for ComponentRecord {
    fn default() -> Self {
        Self {
            byte_size: size_of::<Self>() as u32,
            component_type: 0,
            subtype: 0,
            manufacturer: 0,
            flags: 0,
            version: 0,
            name_length: 0,
            reserved: 0,
            name: [0; 4096],
        }
    }
}
#[derive(Clone, Debug)]
pub struct Component {
    pub component_type: u32,
    pub subtype: u32,
    pub manufacturer: u32,
    pub flags: u32,
    pub version: u32,
    pub name: String,
}
#[repr(u32)]
#[derive(Clone, Copy, Debug)]
pub enum LoadMode {
    Automatic = 0,
    RequireInProcess = 1,
    RequireOutOfProcess = 2,
}
#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
pub struct InstanceInfo {
    pub byte_size: u32,
    pub component_type: u32,
    pub subtype: u32,
    pub manufacturer: u32,
    pub requested_mode: u32,
    pub observed_in_process: u32,
    reserved: [u32; 2],
}
#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct Profile {
    byte_size: u32,
    pub maximum_frames: u32,
    pub input_channels: u32,
    pub output_channels: u32,
    pub sample_rate: f64,
}
impl Profile {
    pub fn new(
        sample_rate: f64,
        maximum_frames: u32,
        input_channels: u32,
        output_channels: u32,
    ) -> Self {
        Self {
            byte_size: size_of::<Self>() as u32,
            maximum_frames,
            input_channels,
            output_channels,
            sample_rate,
        }
    }
    fn valid(self) -> bool {
        (1..=65536).contains(&self.maximum_frames)
            && (1..=2).contains(&self.output_channels)
            && (self.input_channels == 0 || self.input_channels == self.output_channels)
            && self.sample_rate.is_finite()
            && self.sample_rate > 0.
            && self.sample_rate <= 768000.
    }
}
#[repr(C)]
#[derive(Clone, Copy)]
pub struct Input {
    samples: *const f32,
    capacity: u32,
    reserved: u32,
}
#[repr(C)]
#[derive(Clone, Copy)]
pub struct Output {
    samples: *mut f32,
    capacity: u32,
    reserved: u32,
}
#[repr(C)]
pub struct Block {
    byte_size: u32,
    frames: u32,
    input_channels: u32,
    output_channels: u32,
    inputs: *const Input,
    outputs: *const Output,
    sample_position: i64,
}
type Retire = unsafe extern "C" fn(*mut c_void, *mut Diagnostic) -> i32;
/// Authored v1 native ABI. A verified process-lifetime macOS provider supplies
/// all functions; no Objective-C interface or arbitrary callback crosses it.
#[repr(C)]
pub struct RawProvider {
    pub byte_size: u32,
    pub version: u32,
    pub catalogue: Option<
        unsafe extern "C" fn(*const Filter, *mut *mut c_void, *mut u32, *mut Diagnostic) -> i32,
    >,
    pub component: Option<
        unsafe extern "C" fn(*const c_void, u32, *mut ComponentRecord, *mut Diagnostic) -> i32,
    >,
    pub close_catalogue: Option<Retire>,
    pub create_async: Option<
        unsafe extern "C" fn(*mut c_void, u32, u32, *mut *mut c_void, *mut Diagnostic) -> i32,
    >,
    pub poll: Option<
        unsafe extern "C" fn(
            *mut c_void,
            *mut *mut c_void,
            *mut InstanceInfo,
            *mut Diagnostic,
        ) -> i32,
    >,
    pub cancel: Option<Retire>,
    pub prepare: Option<unsafe extern "C" fn(*mut c_void, *const Profile, *mut Diagnostic) -> i32>,
    pub begin_audio:
        Option<unsafe extern "C" fn(*mut c_void, *mut *mut c_void, *mut Diagnostic) -> i32>,
    pub process: Option<unsafe extern "C" fn(*mut c_void, *const Block, *mut Diagnostic) -> i32>,
    pub end_audio: Option<Retire>,
    pub stop: Option<Retire>,
    pub destroy: Option<Retire>,
}
#[derive(Clone, Copy)]
pub struct Provider<'a> {
    raw: &'a RawProvider,
    main: PhantomData<Rc<()>>,
}
impl<'a> Provider<'a> {
    /// Accept a caller-linked trusted provider, without loading any component.
    ///
    /// # Safety
    /// Table/functions and executable image must remain immutable/live for process
    /// lifetime, including retained asynchronous requests after owner drop. The
    /// genuine provider must enforce macOS main-thread CONTROL, bounded foreign
    /// ownership, exact reports, no ABI unwind, complete pointer preflight and
    /// synchronous exclusive joined AUDIO. Forged/malformed reports are diagnosed;
    /// safe Rust cannot prove arbitrary native code or reclaim unproved ownership.
    pub unsafe fn from_raw(raw: &'a RawProvider) -> Result<Self, Error> {
        if raw.byte_size != size_of::<RawProvider>() as u32
            || raw.version != 1
            || raw.catalogue.is_none()
            || raw.component.is_none()
            || raw.close_catalogue.is_none()
            || raw.create_async.is_none()
            || raw.poll.is_none()
            || raw.cancel.is_none()
            || raw.prepare.is_none()
            || raw.begin_audio.is_none()
            || raw.process.is_none()
            || raw.end_audio.is_none()
            || raw.stop.is_none()
            || raw.destroy.is_none()
        {
            return Err(Error::Metadata);
        }
        Ok(Self {
            raw,
            main: PhantomData,
        })
    }
    pub fn catalogue(self, filter: Filter) -> Result<Catalogue<'a>, Error> {
        if filter.component_type == 0 || !(1..=256).contains(&filter.maximum_components) {
            return Err(Error::Shape);
        }
        let (mut handle, mut count, mut d) = (std::ptr::null_mut(), 0, Diagnostic::default());
        // SAFETY: verified table, valid stack records/outputs; genuine Main check.
        report(
            unsafe { self.raw.catalogue.unwrap()(&filter, &mut handle, &mut count, &mut d) },
            d,
        )?;
        let handle = NonNull::new(handle).ok_or(Error::Metadata)?;
        let mut owner = Catalogue {
            raw: self.raw,
            handle: Some(handle),
            components: Vec::new(),
            poisoned: false,
            main: PhantomData,
        };
        if count > filter.maximum_components {
            return Err(Error::Metadata);
        }
        for index in 0..count {
            let (mut c, mut d) = (ComponentRecord::default(), Diagnostic::default());
            // SAFETY: genuine live handle and whole component output.
            report(
                unsafe { self.raw.component.unwrap()(handle.as_ptr(), index, &mut c, &mut d) },
                d,
            )?;
            if c.byte_size != size_of::<ComponentRecord>() as u32
                || c.name_length > 4096
                || c.reserved != 0
                || c.component_type != filter.component_type
                || (filter.subtype != 0 && c.subtype != filter.subtype)
                || (filter.manufacturer != 0 && c.manufacturer != filter.manufacturer)
            {
                return Err(Error::Metadata);
            }
            let name = std::str::from_utf8(&c.name[..c.name_length as usize])
                .map_err(|_| Error::Metadata)?
                .to_owned();
            if owner.components.iter().any(|p| {
                p.component_type == c.component_type
                    && p.subtype == c.subtype
                    && p.manufacturer == c.manufacturer
            }) {
                return Err(Error::Metadata);
            }
            owner.components.push(Component {
                component_type: c.component_type,
                subtype: c.subtype,
                manufacturer: c.manufacturer,
                flags: c.flags,
                version: c.version,
                name,
            });
        }
        Ok(owner)
    }
}
pub struct Catalogue<'a> {
    raw: &'a RawProvider,
    handle: Option<NonNull<c_void>>,
    components: Vec<Component>,
    poisoned: bool,
    main: PhantomData<Rc<()>>,
}
impl<'a> Catalogue<'a> {
    pub fn components(&self) -> &[Component] {
        &self.components
    }
    /// Start selected trusted native execution. Request lifetime is independent
    /// of this catalogue; cancellation does not cancel Apple's underlying work.
    pub fn create(&self, index: usize, mode: LoadMode) -> Result<Pending<'a>, Error> {
        let selected = self.components.get(index).ok_or(Error::Shape)?;
        let handle = self.handle.ok_or(Error::Retired)?;
        let (mut pending, mut d) = (std::ptr::null_mut(), Diagnostic::default());
        // SAFETY: verified table, live selected catalogue, bounded index/output.
        report(
            unsafe {
                self.raw.create_async.unwrap()(
                    handle.as_ptr(),
                    index as u32,
                    mode as u32,
                    &mut pending,
                    &mut d,
                )
            },
            d,
        )?;
        Ok(Pending {
            raw: self.raw,
            handle: Some(NonNull::new(pending).ok_or(Error::Metadata)?),
            identity: [
                selected.component_type,
                selected.subtype,
                selected.manufacturer,
            ],
            mode: mode as u32,
            poisoned: false,
            main: PhantomData,
        })
    }
    pub fn retire(mut self) -> Result<(), Error> {
        self.close()
    }
    fn close(&mut self) -> Result<(), Error> {
        if self.poisoned {
            return Err(Error::Poisoned);
        }
        let h = self.handle.ok_or(Error::Retired)?;
        let mut d = Diagnostic::default();
        // SAFETY: genuine CONTROL handle; provider explicitly reports consumption.
        let status = unsafe { self.raw.close_catalogue.unwrap()(h.as_ptr(), &mut d) };
        if !d.valid() {
            self.poisoned = true;
            return Err(Error::Metadata);
        }
        if d.consumed == 1 {
            self.handle = None;
        } else if status == 0 {
            self.poisoned = true;
            return Err(Error::Metadata);
        }
        report(status, d).map(|_| ())
    }
}
impl Drop for Catalogue<'_> {
    fn drop(&mut self) {
        if self.handle.is_some() && !self.poisoned {
            let _ = self.close();
        }
    }
}
pub struct Pending<'a> {
    raw: &'a RawProvider,
    handle: Option<NonNull<c_void>>,
    identity: [u32; 3],
    mode: u32,
    poisoned: bool,
    main: PhantomData<Rc<()>>,
}
impl<'a> Pending<'a> {
    /// Nonblocking completion poll. `None` leaves the request live. Main queue
    /// service belongs to the application; this method never pumps a run loop.
    pub fn poll(&mut self) -> Result<Option<Instance<'a>>, Error> {
        if self.poisoned {
            return Err(Error::Poisoned);
        }
        let handle = self.handle.ok_or(Error::Retired)?;
        let (mut instance, mut info, mut d) = (
            std::ptr::null_mut(),
            InstanceInfo::default(),
            Diagnostic::default(),
        );
        // SAFETY: live request and whole stack outputs; exact consumed admission.
        let status =
            unsafe { self.raw.poll.unwrap()(handle.as_ptr(), &mut instance, &mut info, &mut d) };
        if !d.valid() {
            self.poisoned = true;
            return Err(Error::Metadata);
        }
        if status == 1 && d.consumed == 0 && instance.is_null() {
            return Ok(None);
        }
        if d.consumed == 1 {
            self.handle = None;
        } else if status == 0 || !instance.is_null() {
            self.poisoned = true;
            return Err(Error::Metadata);
        }
        report(status, d)?;
        if d.consumed != 1
            || info.byte_size != size_of::<InstanceInfo>() as u32
            || info.reserved != [0, 0]
            || info.requested_mode != self.mode
            || info.observed_in_process > 1
            || [info.component_type, info.subtype, info.manufacturer] != self.identity
            || (self.mode == 1 && info.observed_in_process != 1)
            || (self.mode == 2 && info.observed_in_process != 0)
        {
            self.poisoned = true;
            return Err(Error::Metadata);
        }
        Ok(Some(Instance {
            raw: self.raw,
            handle: Some(NonNull::new(instance).ok_or(Error::Metadata)?),
            info,
            profile: None,
            poisoned: false,
            main: PhantomData,
        }))
    }
    pub fn cancel(mut self) -> Result<(), Error> {
        self.cancel_inner()
    }
    fn cancel_inner(&mut self) -> Result<(), Error> {
        if self.poisoned {
            return Err(Error::Poisoned);
        }
        let h = self.handle.ok_or(Error::Retired)?;
        let mut d = Diagnostic::default();
        // SAFETY: genuine live request, balanced facade cancellation on MAIN.
        let status = unsafe { self.raw.cancel.unwrap()(h.as_ptr(), &mut d) };
        if !d.valid() {
            self.poisoned = true;
            return Err(Error::Metadata);
        }
        if d.consumed == 1 {
            self.handle = None;
        } else if status == 0 {
            self.poisoned = true;
            return Err(Error::Metadata);
        }
        report(status, d).map(|_| ())
    }
}
impl Drop for Pending<'_> {
    fn drop(&mut self) {
        if self.handle.is_some() && !self.poisoned {
            let _ = self.cancel_inner();
        }
    }
}
pub struct Instance<'a> {
    raw: &'a RawProvider,
    handle: Option<NonNull<c_void>>,
    info: InstanceInfo,
    profile: Option<Profile>,
    poisoned: bool,
    main: PhantomData<Rc<()>>,
}
impl<'a> Instance<'a> {
    pub fn info(&self) -> InstanceInfo {
        self.info
    }
    pub fn prepare(&mut self, profile: Profile) -> Result<Diagnostic, Error> {
        if self.poisoned {
            return Err(Error::Poisoned);
        }
        if !profile.valid() {
            return Err(Error::Shape);
        }
        let h = self.handle.ok_or(Error::Retired)?;
        let mut d = Diagnostic::default();
        // SAFETY: exclusive stopped owner and validated immutable profile.
        let result = report(
            unsafe { self.raw.prepare.unwrap()(h.as_ptr(), &profile, &mut d) },
            d,
        );
        if result.is_ok() {
            self.profile = Some(profile)
        }
        result
    }
    pub fn stop(&mut self) -> Result<Diagnostic, Error> {
        if self.poisoned {
            return Err(Error::Poisoned);
        }
        let h = self.handle.ok_or(Error::Retired)?;
        let mut d = Diagnostic::default();
        // SAFETY: exclusive MAIN owner outside a joined AUDIO scope.
        let result = report(unsafe { self.raw.stop.unwrap()(h.as_ptr(), &mut d) }, d);
        self.profile = None;
        result
    }
    /// Lend sole AUDIO ownership to a joined scope. Return/unwind/forget all end
    /// native AUDIO on MAIN. Failed end retains the entire native owner and marks
    /// this facade poisoned; it never guesses that foreign retirement succeeded.
    pub fn with_audio<R>(
        &mut self,
        run: impl for<'s> FnOnce(Audio<'s, 'a>) -> R,
    ) -> Result<R, Error> {
        if self.poisoned {
            return Err(Error::Poisoned);
        }
        let h = self.handle.ok_or(Error::Retired)?;
        let profile = self.profile.ok_or(Error::Shape)?;
        let (mut token, mut d) = (std::ptr::null_mut(), Diagnostic::default());
        // SAFETY: stopped prepared MAIN owner, valid token output.
        report(
            unsafe { self.raw.begin_audio.unwrap()(h.as_ptr(), &mut token, &mut d) },
            d,
        )?;
        let token = NonNull::new(token).ok_or_else(|| {
            self.poisoned = true;
            Error::Metadata
        })?;
        let guard = AudioGuard {
            instance: self,
            token: Some(token),
        };
        let result = run(Audio {
            raw: guard.instance.raw,
            token,
            profile,
            scope: PhantomData,
            no_sync: PhantomData,
        });
        guard.finish()?;
        Ok(result)
    }
    pub fn retire(mut self) -> Result<(), Error> {
        self.destroy()
    }
    fn destroy(&mut self) -> Result<(), Error> {
        if self.poisoned {
            return Err(Error::Poisoned);
        }
        let h = self.handle.ok_or(Error::Retired)?;
        let mut d = Diagnostic::default();
        // SAFETY: no safe AUDIO token survives; exact MAIN destruction admission.
        let status = unsafe { self.raw.destroy.unwrap()(h.as_ptr(), &mut d) };
        if !d.valid() {
            self.poisoned = true;
            return Err(Error::Metadata);
        }
        if d.consumed == 1 {
            self.handle = None;
        } else if status == 0 {
            self.poisoned = true;
            return Err(Error::Metadata);
        }
        report(status, d).map(|_| ())
    }
}
impl Drop for Instance<'_> {
    fn drop(&mut self) {
        if self.handle.is_some() && !self.poisoned {
            let _ = self.destroy();
        }
    }
}
struct AudioGuard<'s, 'a> {
    instance: &'s mut Instance<'a>,
    token: Option<NonNull<c_void>>,
}
impl AudioGuard<'_, '_> {
    fn end(&mut self) -> Result<(), Error> {
        let token = self.token.take().unwrap();
        let mut d = Diagnostic::default();
        // SAFETY: HRTB scope and every safe worker ended before MAIN retirement.
        let result = report(
            unsafe { self.instance.raw.end_audio.unwrap()(token.as_ptr(), &mut d) },
            d,
        );
        if result.is_err() {
            self.instance.poisoned = true
        }
        result.map(|_| ())
    }
    fn finish(mut self) -> Result<(), Error> {
        self.end()
    }
}
impl Drop for AudioGuard<'_, '_> {
    fn drop(&mut self) {
        if self.token.is_some() {
            let _ = self.end();
        }
    }
}
pub struct Audio<'s, 'a> {
    raw: &'a RawProvider,
    token: NonNull<c_void>,
    profile: Profile,
    scope: PhantomData<&'s mut ()>,
    no_sync: PhantomData<Cell<()>>,
}
// SAFETY: trusted provider supports sole joined AUDIO handoff. HRTB capability
// cannot escape; it holds no foreign retirement owner and is deliberately !Sync.
unsafe impl Send for Audio<'_, '_> {}
impl Audio<'_, '_> {
    /// Render exact main planar slices. Valid zero frames are allowed. Full
    /// shape admission precedes native history; valid backing tails stay intact.
    /// Position is exact i64 at this ABI, projected to AU's f64 sample time with
    /// ordinary rounding. This profile supplies whole-block same-rate input pulls,
    /// not subframe/SRC scheduling. Failure reports preserve native code domains.
    pub fn process(
        &mut self,
        inputs: &[&[f32]],
        outputs: &mut [&mut [f32]],
        frames: u32,
        sample_position: i64,
    ) -> Result<Diagnostic, Error> {
        if inputs.len() != self.profile.input_channels as usize
            || outputs.len() != self.profile.output_channels as usize
            || frames > self.profile.maximum_frames
        {
            return Err(Error::Shape);
        }
        let mut ins = [Input {
            samples: std::ptr::null(),
            capacity: 0,
            reserved: 0,
        }; 2];
        let mut outs = [Output {
            samples: std::ptr::null_mut(),
            capacity: 0,
            reserved: 0,
        }; 2];
        for (i, s) in inputs.iter().enumerate() {
            if s.len() < frames as usize {
                return Err(Error::Shape);
            }
            ins[i] = Input {
                samples: s.as_ptr(),
                capacity: u32::try_from(s.len()).map_err(|_| Error::Shape)?,
                reserved: 0,
            };
        }
        for (i, s) in outputs.iter_mut().enumerate() {
            if s.len() < frames as usize {
                return Err(Error::Shape);
            }
            outs[i] = Output {
                samples: s.as_mut_ptr(),
                capacity: u32::try_from(s.len()).map_err(|_| Error::Shape)?,
                reserved: 0,
            };
        }
        let b = Block {
            byte_size: size_of::<Block>() as u32,
            frames,
            input_channels: inputs.len() as u32,
            output_channels: outputs.len() as u32,
            inputs: if inputs.is_empty() {
                std::ptr::null()
            } else {
                ins.as_ptr()
            },
            outputs: outs.as_ptr(),
            sample_position,
        };
        let mut d = Diagnostic::default();
        // SAFETY: unique scoped token, whole stack records and disjoint safe
        // mutable slices. Native owner preflights/captures inputs before rendering.
        report(
            unsafe { self.raw.process.unwrap()(self.token.as_ptr(), &b, &mut d) },
            d,
        )
    }
}

#[cfg(test)]
#[path = "au_tests.rs"]
mod tests;
