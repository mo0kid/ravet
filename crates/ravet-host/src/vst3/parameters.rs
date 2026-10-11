//! Owned stopped controller metadata/control and immutable sample-timed DSP input.
//!
//! Controller edits and accepted timed points are separate operations. Metadata
//! refresh explicitly reconciles the controller, without delivering values to
//! the component. Preparation selects the companion for the existing scoped
//! AUDIO owner; processing never invokes controller methods or grows storage.
//! Native queue retention pins the exact module/context on joined CONTROL end.
//! Mutable CONTROL calls across the parent module wait until every audio scope
//! has joined. Already-prepared children may borrow separate audio scopes.
//! No DAW, arbitrary third-party, event/editor/f64/auxiliary or deadline support
//! follows from these bounded main-f32 contracts.
//!
//! ```compile_fail
//! use ravet_host::vst3::instances::parameters::Provider;
//! fn requires_send<T: Send>() {}
//! requires_send::<Provider<'static>>();
//! ```
//! ```compile_fail
//! use ravet_host::vst3::instances::{Instance, parameters::Provider};
//! fn controller_during_audio(child: &mut Instance<'_, '_>, provider: Provider<'_>) {
//!     child.with_audio(|_| child.edit_parameter(provider, 1, 0.5)).unwrap();
//! }
use super::{Audio, Block, InputChannel, Instance, OutputChannel, Profile};
use std::{ffi::c_void, marker::PhantomData, rc::Rc};

/// Copied normalized point. Global offsets are nondecreasing; each ID's offsets
/// strictly increase. VST3 per-ID queues do not specify cross-ID tie ordering.
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Point {
    pub id: u32,
    pub offset: u32,
    pub value: f64,
}
#[repr(C)]
pub struct RawRecord {
    pub struct_size: u32,
    pub id: u32,
    pub flags: u32,
    pub steps: i32,
    pub unit: i32,
    pub default_normalized: f64,
    pub title: [u8; 512],
    pub short_title: [u8; 512],
    pub units: [u8; 512],
}
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RawReport {
    pub struct_size: u32,
    pub stage: u32,
    pub failure: u32,
    pub index: u32,
    pub flags: u32,
    pub native_result: i32,
}
type Refresh = unsafe extern "C" fn(*mut c_void, *mut u32, *mut RawReport) -> i32;
type Info = unsafe extern "C" fn(*const c_void, u32, *mut RawRecord) -> i32;
type Value = unsafe extern "C" fn(*mut c_void, u32, u32, f64, *mut f64, *mut RawReport) -> i32;
type Format =
    unsafe extern "C" fn(*mut c_void, u32, f64, *mut u8, u32, *mut u32, *mut RawReport) -> i32;
type Parse =
    unsafe extern "C" fn(*mut c_void, u32, *const u8, u32, *mut f64, *mut RawReport) -> i32;
type Prepare = unsafe extern "C" fn(*mut c_void, *const Profile, *mut RawReport) -> i32;
type Process =
    unsafe extern "C" fn(*mut c_void, *const Block, *const Point, u32, *mut RawReport) -> i32;
/// Exact-table-paired independent companion; existing provider/state ABIs stay intact.
#[repr(C)]
pub struct RawProvider {
    pub struct_size: u32,
    pub version: u32,
    pub provider: *const super::RawProvider,
    pub refresh: Option<Refresh>,
    pub info: Option<Info>,
    pub value: Option<Value>,
    pub format: Option<Format>,
    pub parse: Option<Parse>,
    pub prepare: Option<Prepare>,
    pub process: Option<Process>,
}
/// MAIN/stopped table borrow. The prepared AUDIO owner carries its own exact
/// table borrow through joined processing, without moving this control provider.
#[derive(Clone, Copy)]
pub struct Provider<'a> {
    pub(super) raw: &'a RawProvider,
    no_send_sync: PhantomData<Rc<()>>,
}
impl<'a> Provider<'a> {
    /// # Safety
    /// Table and pointed v2 provider are readable, compliant trusted native code.
    /// Callbacks contain exceptions, fully preflight every borrowed range, keep
    /// retained queue pools/core/context alive and pin before CONTROL retirement.
    /// All provider code stays loaded until process exit after a pin. The table
    /// borrow does not load/retain an independently unloaded provider DLL.
    pub unsafe fn from_raw(raw: &'a RawProvider) -> Result<Self, Error> {
        if raw.struct_size as usize != size_of::<RawProvider>()
            || raw.version != 1
            || raw.provider.is_null()
            || raw.refresh.is_none()
            || raw.info.is_none()
            || raw.value.is_none()
            || raw.format.is_none()
            || raw.parse.is_none()
            || raw.prepare.is_none()
            || raw.process.is_none()
        {
            return Err(Error::Abi);
        }
        Ok(Self {
            raw,
            no_send_sync: PhantomData,
        })
    }
}
/// Owned UTF-8 controller metadata. Unit IDs/flags preserve SDK identities;
/// program selectors are parameters, not an inferred generic preset bank.
#[derive(Clone, Debug, PartialEq)]
pub struct Parameter {
    pub id: u32,
    pub flags: u32,
    pub steps: i32,
    pub unit: i32,
    pub default_normalized: f64,
    pub title: String,
    pub short_title: String,
    pub units: String,
}
impl Parameter {
    pub fn writable(&self) -> bool {
        self.flags & ((1 << 1) | (1 << 4)) == 0
    }
    pub fn is_bypass(&self) -> bool {
        self.flags & (1 << 16) != 0
    }
    pub fn is_program_selector(&self) -> bool {
        self.flags & (1 << 15) != 0
    }
}
/// Independent copied inspection data; later failed refresh cannot alter it.
#[derive(Clone, Debug, PartialEq)]
pub struct Catalogue {
    parameters: Vec<Parameter>,
}
impl Catalogue {
    pub fn parameters(&self) -> &[Parameter] {
        &self.parameters
    }
}
/// Native attempt/status/exception and immutable-queue fault are independent.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Report {
    raw: RawReport,
}
impl Report {
    fn initial() -> RawReport {
        RawReport {
            struct_size: size_of::<RawReport>() as u32,
            stage: 0,
            failure: 0,
            index: 0,
            flags: 0,
            native_result: 0,
        }
    }
    fn admit(raw: RawReport) -> Result<Self, Error> {
        let attempted = raw.flags & 1 != 0;
        let returned = raw.flags & 2 != 0;
        let threw = raw.flags & 4 != 0;
        if raw.struct_size as usize != size_of::<RawReport>()
            || raw.flags & !15 != 0
            || (returned && threw)
            || ((returned || threw) && !attempted)
            || (attempted && !(returned || threw))
            || !(raw.stage <= 9 || (100..=120).contains(&raw.stage))
            || !(raw.failure <= 3 || (16..=21).contains(&raw.failure))
        {
            return Err(Error::Abi);
        }
        Ok(Self { raw })
    }
    /// Stage0..7 describes controller inspection/control,8 AUDIO processing,
    /// and9 a caught SDK preparation exception. Stages100..120 preserve the
    /// native instance lifecycle stage (Create through Lifecycle).
    pub fn stage(&self) -> u32 {
        self.raw.stage
    }
    pub fn failure(&self) -> u32 {
        self.raw.failure
    }
    pub fn index(&self) -> u32 {
        self.raw.index
    }
    pub fn threw(&self) -> bool {
        self.raw.flags & 4 != 0
    }
    pub fn invalidated(&self) -> bool {
        self.raw.flags & 8 != 0
    }
    pub fn returned_status(&self) -> Option<i32> {
        (self.raw.flags & 2 != 0).then_some(self.raw.native_result)
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Error {
    Abi,
    Shape,
    Lifecycle,
    Poisoned,
    Allocation,
    Provider { status: i32, report: Report },
}
impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "VST3 parameter error: {self:?}")
    }
}
impl std::error::Error for Error {}
fn accepted(status: i32, raw: RawReport) -> Result<Report, Error> {
    let report = Report::admit(raw)?;
    if status != 0 {
        return Err(Error::Provider { status, report });
    }
    if report.invalidated()
        || report.threw()
        || report.failure() != 0
        || report.returned_status().is_some_and(|r| r != 0)
    {
        return Err(Error::Abi);
    }
    Ok(report)
}
fn normalized(value: f64) -> bool {
    value.is_finite() && (0. ..=1.).contains(&value)
}
fn text(bytes: &[u8], required: bool) -> Result<String, Error> {
    let end = bytes.iter().position(|&b| b == 0).ok_or(Error::Abi)?;
    if required && end == 0 {
        return Err(Error::Abi);
    }
    let text = std::str::from_utf8(&bytes[..end]).map_err(|_| Error::Abi)?;
    Ok(text.to_owned())
}
impl<'m, 'a> Instance<'m, 'a> {
    fn parameter_admission(&self, provider: Provider<'_>) -> Result<(), Error> {
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
    /// Explicit stopped metadata/current-value reconciliation. Complete metadata
    /// publishes after all native inspection and Rust admission succeed. Previous
    /// returned catalogues remain inspectable after failure. Explicit parameter
    /// preparation is required again; no component values are delivered here.
    pub fn refresh_parameters(&mut self, provider: Provider<'a>) -> Result<Catalogue, Error> {
        self.parameter_admission(provider)?;
        self.parameter_provider = None;
        self.processing_provider = None;
        self.processing_metadata = None;
        self.hosted = Default::default();
        let mut count = 0;
        let mut report = Report::initial();
        // SAFETY: paired stopped child, disjoint bounded output records.
        let status = unsafe {
            provider.raw.refresh.unwrap()(self.handle.unwrap().as_ptr(), &mut count, &mut report)
        };
        accepted(status, report)?;
        if count > 1024 {
            return Err(Error::Abi);
        }
        let mut parameters = Vec::new();
        parameters
            .try_reserve_exact(count as usize)
            .map_err(|_| Error::Allocation)?;
        let mut bypass = false;
        for i in 0..count {
            let mut r = RawRecord {
                struct_size: size_of::<RawRecord>() as u32,
                id: 0,
                flags: 0,
                steps: 0,
                unit: 0,
                default_normalized: 0.,
                title: [0; 512],
                short_title: [0; 512],
                units: [0; 512],
            };
            // SAFETY: same exclusive stopped child, owned exact output record.
            let status =
                unsafe { provider.raw.info.unwrap()(self.handle.unwrap().as_ptr(), i, &mut r) };
            if status != 0 {
                return Err(Error::Provider {
                    status,
                    report: Report::admit(Report::initial())?,
                });
            }
            if r.struct_size as usize != size_of::<RawRecord>()
                || r.id == u32::MAX
                || r.steps < 0
                || r.flags & !0x1801f != 0
                || !normalized(r.default_normalized)
                || parameters.iter().any(|p: &Parameter| p.id == r.id)
                || (r.flags & 2 != 0 && r.flags & 1 != 0)
                || (r.flags & 16 != 0 && r.flags & 2 == 0)
                || (r.flags & (1 << 16) != 0 && (bypass || r.steps != 1))
            {
                return Err(Error::Abi);
            }
            let p = Parameter {
                id: r.id,
                flags: r.flags,
                steps: r.steps,
                unit: r.unit,
                default_normalized: r.default_normalized,
                title: text(&r.title, true)?,
                short_title: text(&r.short_title, false)?,
                units: text(&r.units, false)?,
            };
            bypass |= p.is_bypass();
            parameters.push(p);
        }
        Ok(Catalogue { parameters })
    }
    fn parameter_value(
        &mut self,
        provider: Provider<'_>,
        id: u32,
        operation: u32,
        input: f64,
    ) -> Result<f64, Error> {
        self.parameter_admission(provider)?;
        if id == u32::MAX
            || (operation == 2 && !input.is_finite())
            || ((operation == 1 || operation == 3) && !normalized(input))
        {
            return Err(Error::Shape);
        }
        let mut output = 0.;
        let mut report = Report::initial();
        // SAFETY: exact paired stopped child, finite input and owned disjoint outputs.
        let status = unsafe {
            provider.raw.value.unwrap()(
                self.handle.unwrap().as_ptr(),
                id,
                operation,
                input,
                &mut output,
                &mut report,
            )
        };
        let admitted = accepted(status, report).and_then(|_| {
            if !output.is_finite() || (operation != 1 && !normalized(output)) {
                Err(Error::Abi)
            } else {
                Ok(output)
            }
        });
        // A foreign edit has already run. Even a claimed status0 with malformed
        // diagnostics/output cannot prove the previous preparation remains valid.
        // Local shape/admission rejection above never reaches this boundary.
        if operation == 3 && admitted.is_err() {
            self.parameter_provider = None;
            self.processing_provider = None;
            self.processing_metadata = None;
            self.hosted = Default::default();
        }
        admitted
    }
    pub fn read_parameter(&mut self, provider: Provider<'_>, id: u32) -> Result<f64, Error> {
        self.parameter_value(provider, id, 0, 0.)
    }
    pub fn parameter_to_plain(
        &mut self,
        provider: Provider<'_>,
        id: u32,
        normalized: f64,
    ) -> Result<f64, Error> {
        self.parameter_value(provider, id, 1, normalized)
    }
    pub fn parameter_to_normalized(
        &mut self,
        provider: Provider<'_>,
        id: u32,
        plain: f64,
    ) -> Result<f64, Error> {
        self.parameter_value(provider, id, 2, plain)
    }
    /// Change controller value only. Schedule timed points separately to deliver
    /// DSP changes. Failed foreign mutation invalidates parameter preparation;
    /// explicit refresh/reprepare or a new child is recovery, without rollback.
    pub fn edit_parameter(
        &mut self,
        provider: Provider<'_>,
        id: u32,
        normalized: f64,
    ) -> Result<(), Error> {
        self.parameter_value(provider, id, 3, normalized)
            .map(|_| ())
    }
    pub fn format_parameter(
        &mut self,
        provider: Provider<'_>,
        id: u32,
        value: f64,
    ) -> Result<String, Error> {
        self.parameter_admission(provider)?;
        if id == u32::MAX || !normalized(value) {
            return Err(Error::Shape);
        }
        let mut bytes = [0; 512];
        let mut length = 0;
        let mut report = Report::initial();
        // SAFETY: exact paired stopped child and disjoint owned bounded text outputs.
        let status = unsafe {
            provider.raw.format.unwrap()(
                self.handle.unwrap().as_ptr(),
                id,
                value,
                bytes.as_mut_ptr(),
                512,
                &mut length,
                &mut report,
            )
        };
        accepted(status, report)?;
        let value = text(&bytes, false)?;
        if value.len() != length as usize {
            return Err(Error::Abi);
        }
        Ok(value)
    }
    pub fn parse_parameter(
        &mut self,
        provider: Provider<'_>,
        id: u32,
        text: &str,
    ) -> Result<f64, Error> {
        self.parameter_admission(provider)?;
        if id == u32::MAX
            || text.len() > 384
            || text.contains('\0')
            || text.encode_utf16().count() > 127
        {
            return Err(Error::Shape);
        }
        let mut value = 0.;
        let mut report = Report::initial();
        // SAFETY: paired stopped child, bounded UTF-8 source and disjoint owned outputs.
        let status = unsafe {
            provider.raw.parse.unwrap()(
                self.handle.unwrap().as_ptr(),
                id,
                text.as_ptr(),
                text.len() as u32,
                &mut value,
                &mut report,
            )
        };
        accepted(status, report)?;
        if !normalized(value) {
            return Err(Error::Abi);
        }
        Ok(value)
    }
    /// Reserve fixed native queue storage on CONTROL before SDK preparation.
    /// Existing scoped AUDIO transport carries this exact table automatically.
    /// No successful controller edit/refresh substitutes for timed component input.
    pub fn prepare_parameters(
        &mut self,
        provider: Provider<'a>,
        profile: Profile,
    ) -> Result<(), Error> {
        self.parameter_admission(provider)?;
        if profile.struct_size as usize != size_of::<Profile>()
            || !profile.sample_rate.is_finite()
            || !(0. ..=768000.).contains(&profile.sample_rate)
            || profile.sample_rate == 0.
            || !(1..=65536).contains(&profile.maximum_frames)
            || !(1..=2).contains(&profile.output_channels)
            || (profile.input_channels != 0 && profile.input_channels != profile.output_channels)
        {
            return Err(Error::Shape);
        }
        self.parameter_provider = None;
        self.processing_provider = None;
        self.processing_metadata = None;
        self.hosted = Default::default();
        self.profile = None;
        let mut report = Report::initial();
        // SAFETY: paired exclusive stopped child, valid copied profile/diagnostic.
        let status = unsafe {
            provider.raw.prepare.unwrap()(self.handle.unwrap().as_ptr(), &profile, &mut report)
        };
        accepted(status, report)?;
        self.profile = Some(profile);
        self.parameter_provider = Some(provider.raw);
        Ok(())
    }
}
pub(super) fn points_admitted(points: &[Point], frames: u32) -> bool {
    if points.len() > 2048 {
        return false;
    }
    let mut ids = [0; 64];
    let mut last = [0; 64];
    let mut count = 0;
    for (i, p) in points.iter().enumerate() {
        if p.id == u32::MAX
            || !normalized(p.value)
            || (frames > 0 && p.offset >= frames)
            || (frames == 0 && p.offset != 0)
            || (i > 0 && p.offset < points[i - 1].offset)
        {
            return false;
        }
        if let Some(q) = ids[..count].iter().position(|&id| id == p.id) {
            if p.offset <= last[q] {
                return false;
            }
            last[q] = p.offset;
        } else {
            if count == 64 {
                return false;
            }
            ids[count] = p.id;
            last[count] = p.offset;
            count += 1;
        }
    }
    true
}
impl Audio<'_, '_> {
    /// Process normalized timed points through fixed immutable native SDK queues.
    /// Slice/point shape preflight precedes native calls and all output writes.
    /// Native additionally checks complete known writable metadata/ranges and pool
    /// capacity; late rejection preserves outputs/history. A known native failure
    /// or swallowed queue fault silences active outputs and returns separate facts.
    /// At most2048 points/64 IDs, main f32 only. No controller calls or heap work.
    pub fn process_parameters(
        &mut self,
        inputs: &[&[f32]],
        outputs: &mut [&mut [f32]],
        frames: u32,
        points: &[Point],
    ) -> Result<Report, Error> {
        let provider = self.parameters.ok_or(Error::Lifecycle)?;
        if self.hosted_mode() {
            return Err(Error::Lifecycle);
        }
        if inputs.len() != self.profile.input_channels as usize
            || outputs.len() != self.profile.output_channels as usize
            || inputs.len() > 2
            || outputs.len() > 2
            || frames > self.profile.maximum_frames
            || !points_admitted(points, frames)
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
        let mut report = Report::initial();
        // SAFETY: unique scoped audio owner, exact latched companion/table, bounded
        // immutable points/input and distinct mutable caller slices/stack records.
        let status = unsafe {
            provider.process.unwrap()(
                self.handle.as_ptr(),
                &block,
                points.as_ptr(),
                points.len() as u32,
                &mut report,
            )
        };
        let report = accepted(status, report)?;
        if report.stage() != 8 || report.returned_status() != Some(0) {
            return Err(Error::Abi);
        }
        Ok(report)
    }
}

#[cfg(test)]
#[path = "parameter_tests.rs"]
mod tests;
