//! Hosted native f32/f64 main/aux bus profiles for a paired VST3 instance.
//!
//! `RavetVst3HostedAudioProviderV1` is an additive companion paired with the
//! exact instance ABI-v2 table and rich processing-v1 table. Acquisition checks
//! version, size, required entries and both origin pointers. Existing v1/v2,
//! state, parameter and processing layouts and result domains are unchanged.
//!
//! Every advertised audio bus receives an explicit request in native index
//! order; no `defaultActive` inference or fallback precision is applied. Packed
//! channels follow increasing native bus index separately in each direction and
//! include only active buses. Inactive buses keep their native index, channel
//! count and pointer array inside the native provider. Samples stay in the
//! selected precision end to end: no converter narrows f64 to f32.
//!
//! CONTROL methods remain MAIN-only and stopped. The scoped AUDIO capability
//! carries a copied precision/channel snapshot and the exact static table
//! borrow; it is still `Send`, not `Sync`, and cannot escape its scope. The
//! owned Graph bank in [`graph`](crate::vst3::instances::hosted_audio::graph) retains its Library on MAIN instead.
//! Allocation-free checks cover Rust code only; foreign SDK heap use, crash
//! isolation, DAW compatibility and real-time deadlines are not established.
//!
//! The hosted CONTROL companion cannot cross threads:
//!
//! ```compile_fail,E0277
//! use ravet_host::vst3::instances::hosted_audio::Provider;
//! fn require_send<T: Send>() {}
//! require_send::<Provider<'static>>();
//! ```
use super::{Audio, Block, InputChannel, Instance, OutputChannel, Profile, parameters, processing};
use processing::{Input, Output, OutputStorage, RawOutputs};
use ravet_foundation::Tail;
use std::{ffi::c_void, marker::PhantomData, rc::Rc};

pub mod graph;

/// Maximum advertised audio buses per direction in this bounded profile.
pub const MAXIMUM_BUSES: usize = 16;
/// Maximum active packed channels per direction.
pub const MAXIMUM_CHANNELS: usize = 64;
const STAGE_BUSES: u32 = 105;
const STAGE_PROFILE: u32 = 119;
const STAGE_LATENCY: u32 = 121;

#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct RawInfo {
    pub struct_size: u32,
    pub input_buses: u32,
    pub output_buses: u32,
    pub reserved: u32,
}
#[repr(C)]
#[derive(Clone, Copy)]
pub struct RawBus {
    pub struct_size: u32,
    pub index: u32,
    pub channels: u32,
    pub bus_type: u32,
    pub flags: u32,
    pub reserved: u32,
    pub arrangement: u64,
    pub name: [u8; 384],
}
impl RawBus {
    fn initial() -> Self {
        Self {
            struct_size: size_of::<Self>() as u32,
            index: 0,
            channels: 0,
            bus_type: 0,
            flags: 0,
            reserved: 0,
            arrangement: 0,
            name: [0; 384],
        }
    }
}
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct RawBusRequest {
    pub arrangement: u64,
    pub active: u32,
    pub reserved: u32,
}
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RawProfile {
    pub struct_size: u32,
    pub maximum_frames: u32,
    pub sample_bits: u32,
    pub input_buses: u32,
    pub output_buses: u32,
    pub reserved: u32,
    pub sample_rate: f64,
    pub inputs: [RawBusRequest; 16],
    pub outputs: [RawBusRequest; 16],
}
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct RawTiming {
    pub struct_size: u32,
    pub latency_samples: u32,
    pub tail_kind: u32,
    pub reserved: u32,
    pub tail_samples: u64,
}
/// Companion CONTROL diagnostic. Stages 100..=120 are private operation stages;
/// 121 and 122 are the latency/tail getters of this companion only.
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
impl RawReport {
    pub(crate) fn initial() -> Self {
        Self {
            struct_size: size_of::<Self>() as u32,
            stage: 0,
            failure: 0,
            index: 0,
            flags: 0,
            native_result: 0,
        }
    }
}
#[repr(C)]
#[derive(Clone, Copy)]
pub struct InputChannel64 {
    samples: *const f64,
    capacity: u32,
    reserved: u32,
}
#[repr(C)]
#[derive(Clone, Copy)]
pub struct OutputChannel64 {
    samples: *mut f64,
    capacity: u32,
    reserved: u32,
}
#[repr(C)]
pub struct Block64 {
    struct_size: u32,
    frames: u32,
    input_channels: u32,
    output_channels: u32,
    inputs: *const InputChannel64,
    outputs: *const OutputChannel64,
}
type Process<B> = unsafe extern "C" fn(
    *mut c_void,
    *const B,
    *const parameters::Point,
    u32,
    *const processing::RawEvent,
    u32,
    *const u8,
    u32,
    *const processing::Context,
    *mut RawOutputs,
    *mut processing::RawReport,
) -> i32;
type Refresh = unsafe extern "C" fn(*mut c_void, *mut RawInfo, *mut RawReport) -> i32;
type BusInfo = unsafe extern "C" fn(*const c_void, u32, u32, *mut RawBus, *mut RawReport) -> i32;
type Prepare = unsafe extern "C" fn(*mut c_void, *const RawProfile, *mut RawReport) -> i32;
type Timing32 = unsafe extern "C" fn(*const c_void, *mut RawTiming, *mut RawReport) -> i32;
/// Exact C layout of `RavetVst3HostedAudioProviderV1`.
#[repr(C)]
pub struct RawProvider {
    pub struct_size: u32,
    pub version: u32,
    pub instance_origin: *const super::RawProvider,
    pub processing_origin: *const processing::RawProvider,
    pub refresh_audio: Option<Refresh>,
    pub audio_bus: Option<BusInfo>,
    pub prepare: Option<Prepare>,
    pub timing: Option<Timing32>,
    pub process32: Option<Process<Block>>,
    pub process64: Option<Process<Block64>>,
}
/// MAIN/stopped borrow of a verified hosted companion table.
#[derive(Clone, Copy)]
pub struct Provider<'a> {
    pub(crate) raw: &'a RawProvider,
    no_send_sync: PhantomData<Rc<()>>,
}
impl<'a> Provider<'a> {
    /// # Safety
    /// The readable table and both origin tables are trusted compliant native
    /// code that stays loaded for `'a` (and until process exit after a pin).
    /// Callbacks contain exceptions, preflight every whole borrowed range and
    /// bound every count. A table borrow does not retain an unloaded DLL.
    pub unsafe fn from_raw(raw: &'a RawProvider) -> Result<Self, Error> {
        if raw.struct_size as usize != size_of::<RawProvider>()
            || raw.version != 1
            || raw.instance_origin.is_null()
            || raw.processing_origin.is_null()
            || raw.refresh_audio.is_none()
            || raw.audio_bus.is_none()
            || raw.prepare.is_none()
            || raw.timing.is_none()
            || raw.process32.is_none()
            || raw.process64.is_none()
        {
            return Err(Error::Abi);
        }
        Ok(Self {
            raw,
            no_send_sync: PhantomData,
        })
    }
}
/// Checked companion CONTROL report. Getter stages never claim a returned tresult.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ControlReport {
    raw: RawReport,
}
impl ControlReport {
    fn admit(raw: RawReport) -> Result<Self, Error> {
        let attempted = raw.flags & 1 != 0;
        let returned = raw.flags & 2 != 0;
        let threw = raw.flags & 4 != 0;
        if raw.struct_size as usize != size_of::<RawReport>()
            || raw.index != u32::MAX
            || raw.flags & !15 != 0
            || !(100..=122).contains(&raw.stage)
            || raw.failure > 6
            || (returned && threw)
            || attempted != (returned || threw)
            || returned != (raw.failure == 2)
            || threw != (raw.failure == 3)
            || (returned && raw.stage >= 121)
            || (!returned && raw.native_result != 0)
        {
            return Err(Error::Abi);
        }
        Ok(Self { raw })
    }
    pub fn stage(&self) -> u32 {
        self.raw.stage
    }
    /// 0 none, 1 local invalid, 2 returned foreign refusal, 3 foreign throw,
    /// 4 capacity, 5 allocation, 6 malformed native metadata.
    pub fn failure(&self) -> u32 {
        self.raw.failure
    }
    pub fn attempted(&self) -> bool {
        self.raw.flags & 1 != 0
    }
    pub fn returned_status(&self) -> Option<i32> {
        (self.raw.flags & 2 != 0).then_some(self.raw.native_result)
    }
    pub fn threw(&self) -> bool {
        self.raw.flags & 4 != 0
    }
    /// A previously prepared native readiness was invalidated by this call.
    pub fn invalidated(&self) -> bool {
        self.raw.flags & 8 != 0
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Error {
    /// Malformed table, record or report; no unverified output is exposed.
    Abi,
    /// Local shape/profile/request refusal before any native call.
    Shape,
    /// Requested precision differs from the typed API or prepared profile.
    Precision,
    Capacity,
    Allocation,
    /// Required stopped catalogue/processing/prepared state is absent.
    Lifecycle,
    Poisoned,
    /// Native refusal. `report` is `None` when the provider refused before
    /// writing its diagnostic (local pointer/thread/range admission).
    Native {
        status: i32,
        report: Option<ControlReport>,
    },
    Processing(processing::Error),
}
impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "VST3 hosted audio error: {self:?}")
    }
}
impl std::error::Error for Error {}
fn control_accepted(status: i32, raw: RawReport, stage: u32) -> Result<ControlReport, Error> {
    if status != 0 && raw == RawReport::initial() {
        return Err(Error::Native {
            status,
            report: None,
        });
    }
    let report = ControlReport::admit(raw)?;
    if (status == 0) != (report.failure() == 0) {
        return Err(Error::Abi);
    }
    if status != 0 {
        return Err(Error::Native {
            status,
            report: Some(report),
        });
    }
    if raw.stage != stage || raw.flags & 7 != 0 {
        return Err(Error::Abi);
    }
    Ok(report)
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Direction {
    Input,
    Output,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Precision {
    F32,
    F64,
}
impl Precision {
    fn bits(self) -> u32 {
        match self {
            Self::F32 => 32,
            Self::F64 => 64,
        }
    }
}
/// Copied advertised audio bus. Arrangement is the native speaker bit mask.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AudioBus {
    pub index: u32,
    pub direction: Direction,
    pub channels: u32,
    pub auxiliary: bool,
    pub default_active: bool,
    pub arrangement: u64,
    pub name: String,
}
/// Immutable copied catalogue of every advertised audio bus in native order.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AudioCatalogue {
    inputs: Vec<AudioBus>,
    outputs: Vec<AudioBus>,
}
impl AudioCatalogue {
    pub fn inputs(&self) -> &[AudioBus] {
        &self.inputs
    }
    pub fn outputs(&self) -> &[AudioBus] {
        &self.outputs
    }
    pub fn buses(&self, direction: Direction) -> &[AudioBus] {
        match direction {
            Direction::Input => &self.inputs,
            Direction::Output => &self.outputs,
        }
    }
}
/// Explicit request for one advertised native bus index.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BusRequest {
    arrangement: u64,
    active: bool,
}
impl BusRequest {
    pub fn active(arrangement: u64) -> Self {
        Self {
            arrangement,
            active: true,
        }
    }
    pub fn inactive(arrangement: u64) -> Self {
        Self {
            arrangement,
            active: false,
        }
    }
    pub fn arrangement(self) -> u64 {
        self.arrangement
    }
    pub fn is_active(self) -> bool {
        self.active
    }
    pub fn channels(self) -> u32 {
        self.arrangement.count_ones()
    }
}
/// One advertised bus in a packed map. Inactive buses have no channel range.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct PackedBus {
    pub index: u32,
    pub arrangement: u64,
    pub channels: u32,
    pub active: bool,
    /// First packed channel; equals the running total for an inactive bus.
    pub first_channel: u32,
}
impl PackedBus {
    pub fn channel_range(&self) -> Option<std::ops::Range<usize>> {
        self.active
            .then(|| self.first_channel as usize..(self.first_channel + self.channels) as usize)
    }
}
/// Deterministic native-index packing of active channels per direction. Graph
/// nodes expose exactly these packed channels; sidechain/aux buses therefore
/// route through ordinary node channels without changing Graph's main-only
/// outer configuration.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PackedAudioMap {
    inputs: [PackedBus; 16],
    outputs: [PackedBus; 16],
    input_buses: u32,
    output_buses: u32,
    input_channels: u32,
    output_channels: u32,
}
impl PackedAudioMap {
    pub fn input_channels(&self) -> usize {
        self.input_channels as usize
    }
    pub fn output_channels(&self) -> usize {
        self.output_channels as usize
    }
    /// Every advertised bus, including inactive ones, in native index order.
    pub fn buses(&self, direction: Direction) -> &[PackedBus] {
        match direction {
            Direction::Input => &self.inputs[..self.input_buses as usize],
            Direction::Output => &self.outputs[..self.output_buses as usize],
        }
    }
    fn build(inputs: &[BusRequest], outputs: &[BusRequest]) -> Result<Self, Error> {
        if inputs.len() > MAXIMUM_BUSES || outputs.len() > MAXIMUM_BUSES {
            return Err(Error::Shape);
        }
        let mut map = Self {
            inputs: [PackedBus::default(); 16],
            outputs: [PackedBus::default(); 16],
            input_buses: inputs.len() as u32,
            output_buses: outputs.len() as u32,
            input_channels: 0,
            output_channels: 0,
        };
        for (requests, buses, total) in [
            (inputs, &mut map.inputs, &mut map.input_channels),
            (outputs, &mut map.outputs, &mut map.output_channels),
        ] {
            for (i, request) in requests.iter().enumerate() {
                let channels = request.channels();
                if request.active && channels == 0 {
                    return Err(Error::Shape);
                }
                buses[i] = PackedBus {
                    index: i as u32,
                    arrangement: request.arrangement,
                    channels,
                    active: request.active,
                    first_channel: *total,
                };
                if request.active {
                    if channels > MAXIMUM_CHANNELS as u32 - *total {
                        return Err(Error::Capacity);
                    }
                    *total += channels;
                }
            }
        }
        if map.input_channels == 0 && map.output_channels == 0 {
            return Err(Error::Shape);
        }
        Ok(map)
    }
}
/// Checked complete hosted profile: precision, rate, maximum block and an
/// explicit request for every advertised bus in native index order.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct HostedAudioProfile {
    raw: RawProfile,
    precision: Precision,
    map: PackedAudioMap,
}
impl HostedAudioProfile {
    /// Rate must be finite, positive and at most 768000; frames 1..=65536.
    /// At most 16 requests and 64 active packed channels per direction, with at
    /// least one active channel overall. Requests are not checked against a
    /// catalogue until preparation.
    pub fn new(
        precision: Precision,
        sample_rate: f64,
        maximum_frames: u32,
        inputs: &[BusRequest],
        outputs: &[BusRequest],
    ) -> Result<Self, Error> {
        if !sample_rate.is_finite()
            || sample_rate <= 0.
            || sample_rate > 768000.
            || !(1..=65536).contains(&maximum_frames)
        {
            return Err(Error::Shape);
        }
        let map = PackedAudioMap::build(inputs, outputs)?;
        let mut raw = RawProfile {
            struct_size: size_of::<RawProfile>() as u32,
            maximum_frames,
            sample_bits: precision.bits(),
            input_buses: inputs.len() as u32,
            output_buses: outputs.len() as u32,
            reserved: 0,
            sample_rate,
            inputs: [RawBusRequest::default(); 16],
            outputs: [RawBusRequest::default(); 16],
        };
        for (requests, raw) in [(inputs, &mut raw.inputs), (outputs, &mut raw.outputs)] {
            for (i, r) in requests.iter().enumerate() {
                raw[i] = RawBusRequest {
                    arrangement: r.arrangement,
                    active: u32::from(r.active),
                    reserved: 0,
                };
            }
        }
        Ok(Self {
            raw,
            precision,
            map,
        })
    }
    pub fn precision(&self) -> Precision {
        self.precision
    }
    pub fn sample_rate(&self) -> f64 {
        self.raw.sample_rate
    }
    pub fn maximum_frames(&self) -> u32 {
        self.raw.maximum_frames
    }
    pub fn map(&self) -> &PackedAudioMap {
        &self.map
    }
}
/// Prepared latency and tail copied from the actual native getters.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Timing {
    pub latency_samples: u32,
    pub tail: Tail,
}
impl Timing {
    fn admit(raw: RawTiming) -> Result<Self, Error> {
        if raw.struct_size as usize != size_of::<RawTiming>() || raw.reserved != 0 {
            return Err(Error::Abi);
        }
        let tail = match (raw.tail_kind, raw.tail_samples) {
            (0, 0) => Tail::None,
            (1, n) if n > 0 => Tail::Samples(n),
            (2, 0) => Tail::Infinite,
            _ => return Err(Error::Abi),
        };
        Ok(Self {
            latency_samples: raw.latency_samples,
            tail,
        })
    }
}
#[derive(Clone, Copy)]
pub(super) struct Prepared<'a> {
    provider: &'a RawProvider,
    profile: HostedAudioProfile,
    timing: Timing,
}
/// Instance-private hosted CONTROL state. Cleared by every topology change.
#[derive(Default)]
pub(super) struct State<'a> {
    catalogue: Option<(&'a RawProvider, AudioCatalogue)>,
    prepared: Option<Prepared<'a>>,
}
impl<'a> State<'a> {
    pub(super) fn audio(&self) -> Option<AudioState<'a>> {
        self.prepared.map(|p| AudioState {
            provider: p.provider,
            precision: p.profile.precision,
            maximum_frames: p.profile.raw.maximum_frames,
            sample_rate: p.profile.raw.sample_rate,
            input_channels: p.profile.map.input_channels,
            output_channels: p.profile.map.output_channels,
        })
    }
}
/// Copied AUDIO snapshot: exact table borrow plus precision and packed counts.
#[derive(Clone, Copy)]
pub(super) struct AudioState<'a> {
    provider: &'a RawProvider,
    precision: Precision,
    maximum_frames: u32,
    sample_rate: f64,
    input_channels: u32,
    output_channels: u32,
}
fn validated_bus(raw: &RawBus, index: u32, direction: Direction) -> Result<AudioBus, Error> {
    if raw.struct_size as usize != size_of::<RawBus>()
        || raw.index != index
        || raw.channels > MAXIMUM_CHANNELS as u32
        || raw.bus_type > 1
        || raw.flags & !1 != 0
        || raw.reserved != 0
        || raw.arrangement.count_ones() != raw.channels
    {
        return Err(Error::Abi);
    }
    let end = raw.name.iter().position(|&c| c == 0).ok_or(Error::Abi)?;
    if end == 0 {
        return Err(Error::Abi);
    }
    let text = std::str::from_utf8(&raw.name[..end]).map_err(|_| Error::Abi)?;
    let mut name = String::new();
    name.try_reserve_exact(text.len())
        .map_err(|_| Error::Allocation)?;
    name.push_str(text);
    Ok(AudioBus {
        index,
        direction,
        channels: raw.channels,
        auxiliary: raw.bus_type == 1,
        default_active: raw.flags & 1 != 0,
        arrangement: raw.arrangement,
        name,
    })
}
impl<'a> Instance<'_, 'a> {
    fn hosted_admission(&self, provider: Provider<'_>) -> Result<(), Error> {
        if !std::ptr::eq(provider.raw.instance_origin, self.library.provider.raw) {
            return Err(Error::Abi);
        }
        if self.poisoned || self.state_fault {
            return Err(Error::Poisoned);
        }
        if self.running {
            return Err(Error::Lifecycle);
        }
        if !self
            .processing_metadata
            .is_some_and(|(raw, _)| std::ptr::eq(raw, provider.raw.processing_origin))
        {
            return Err(Error::Lifecycle);
        }
        Ok(())
    }
    /// Stopped copied audio-bus catalogue. Requires a prior exact-paired
    /// `refresh_processing`. Native refresh invalidates prior readiness before
    /// inspection, so every prepared profile is cleared first; stopped retry must
    /// reprepare. Publication follows complete Rust admission of every record.
    pub fn refresh_hosted_audio(
        &mut self,
        provider: Provider<'a>,
    ) -> Result<AudioCatalogue, Error> {
        self.hosted_admission(provider)?;
        self.hosted = State::default();
        self.profile = None;
        self.parameter_provider = None;
        self.processing_provider = None;
        let handle = self.handle.unwrap().as_ptr();
        let mut info = RawInfo {
            struct_size: size_of::<RawInfo>() as u32,
            ..RawInfo::default()
        };
        let mut report = RawReport::initial();
        // SAFETY: verified exact-paired table, stopped exclusive MAIN child and
        // distinct initialized output/diagnostic records.
        let status = unsafe { provider.raw.refresh_audio.unwrap()(handle, &mut info, &mut report) };
        control_accepted(status, report, STAGE_BUSES)?;
        if info.struct_size as usize != size_of::<RawInfo>()
            || info.reserved != 0
            || info.input_buses > MAXIMUM_BUSES as u32
            || info.output_buses > MAXIMUM_BUSES as u32
        {
            return Err(Error::Abi);
        }
        let mut catalogue = AudioCatalogue {
            inputs: Vec::new(),
            outputs: Vec::new(),
        };
        for (direction, count) in [
            (Direction::Input, info.input_buses),
            (Direction::Output, info.output_buses),
        ] {
            let list = match direction {
                Direction::Input => &mut catalogue.inputs,
                Direction::Output => &mut catalogue.outputs,
            };
            list.try_reserve_exact(count as usize)
                .map_err(|_| Error::Allocation)?;
            for i in 0..count {
                let mut bus = RawBus::initial();
                let mut report = RawReport::initial();
                // SAFETY: same stopped child, bounded copied index, owned records.
                let status = unsafe {
                    provider.raw.audio_bus.unwrap()(
                        handle,
                        u32::from(direction == Direction::Output),
                        i,
                        &mut bus,
                        &mut report,
                    )
                };
                control_accepted(status, report, STAGE_BUSES)?;
                list.push(validated_bus(&bus, i, direction)?);
            }
            if list.iter().filter(|b| !b.auxiliary).count() > 1 {
                return Err(Error::Abi);
            }
        }
        self.hosted.catalogue = Some((provider.raw, catalogue.clone()));
        Ok(catalogue)
    }
    /// Prepare an exact f32 hosted profile; see [`Self::prepare_hosted64`].
    pub fn prepare_hosted32(
        &mut self,
        provider: Provider<'a>,
        profile: &HostedAudioProfile,
    ) -> Result<Timing, Error> {
        if profile.precision != Precision::F32 {
            return Err(Error::Precision);
        }
        self.prepare_hosted(provider, profile)
    }
    /// Prepare an exact native f64 profile against the refreshed catalogue.
    ///
    /// Local preflight failure preserves the prior valid profile. Once the
    /// native call begins, Rust readiness is cleared first; failure retains the
    /// original stage/result. Success publishes only after the native provider
    /// has reinspected arrangements and activated the requested buses, followed
    /// by the latency/tail getters. Native `canProcessSampleSize` refusal never
    /// selects a fallback precision.
    pub fn prepare_hosted64(
        &mut self,
        provider: Provider<'a>,
        profile: &HostedAudioProfile,
    ) -> Result<Timing, Error> {
        if profile.precision != Precision::F64 {
            return Err(Error::Precision);
        }
        self.prepare_hosted(provider, profile)
    }
    fn prepare_hosted(
        &mut self,
        provider: Provider<'a>,
        profile: &HostedAudioProfile,
    ) -> Result<Timing, Error> {
        self.hosted_admission(provider)?;
        let catalogue = match &self.hosted.catalogue {
            Some((raw, catalogue)) if std::ptr::eq(*raw, provider.raw) => catalogue,
            _ => return Err(Error::Lifecycle),
        };
        if catalogue.inputs.len() != profile.raw.input_buses as usize
            || catalogue.outputs.len() != profile.raw.output_buses as usize
        {
            return Err(Error::Shape);
        }
        self.hosted.prepared = None;
        self.profile = None;
        self.parameter_provider = None;
        self.processing_provider = None;
        let handle = self.handle.unwrap().as_ptr();
        let mut report = RawReport::initial();
        // SAFETY: exact-paired stopped child, validated immutable profile and a
        // distinct diagnostic; native owns negotiation and scratch on CONTROL.
        let status = unsafe { provider.raw.prepare.unwrap()(handle, &profile.raw, &mut report) };
        control_accepted(status, report, STAGE_PROFILE)?;
        let mut timing = RawTiming {
            struct_size: size_of::<RawTiming>() as u32,
            ..RawTiming::default()
        };
        let mut report = RawReport::initial();
        // SAFETY: prepared stopped child, owned output/diagnostic records.
        let status = unsafe { provider.raw.timing.unwrap()(handle, &mut timing, &mut report) };
        control_accepted(status, report, STAGE_LATENCY)?;
        let timing = Timing::admit(timing)?;
        self.hosted.prepared = Some(Prepared {
            provider: provider.raw,
            profile: *profile,
            timing,
        });
        self.profile = Some(Profile::new(
            profile.raw.sample_rate,
            profile.raw.maximum_frames,
            profile.map.input_channels,
            profile.map.output_channels,
        ));
        Ok(timing)
    }
    /// Currently prepared hosted profile, if any.
    pub fn hosted_profile(&self) -> Option<&HostedAudioProfile> {
        self.hosted.prepared.as_ref().map(|p| &p.profile)
    }
    /// Cached prepared latency/tail; no native call.
    pub fn hosted_timing(&self) -> Option<Timing> {
        self.hosted.prepared.map(|p| p.timing)
    }
}
mod sealed {
    use super::*;
    /// Raw borrowed planar channel for one native call.
    #[derive(Clone, Copy)]
    pub struct InChannel<S> {
        pub samples: *const S,
        pub capacity: u32,
    }
    #[derive(Clone, Copy)]
    pub struct OutChannel<S> {
        pub samples: *mut S,
        pub capacity: u32,
    }
    pub trait Sealed: Sized {
        /// # Safety
        /// Exact prepared table/handle on its sole AUDIO worker. Channel slices
        /// are initialized for `capacity`, outputs distinct and writable, and all
        /// counts were checked against the prepared packed profile.
        #[allow(clippy::too_many_arguments)]
        unsafe fn invoke(
            provider: &RawProvider,
            handle: *mut c_void,
            inputs: &[InChannel<Self>],
            outputs: &[OutChannel<Self>],
            frames: u32,
            input: &Input<'_>,
            wire: &mut RawOutputs,
            report: &mut processing::RawReport,
        ) -> i32;
    }
    impl Sealed for f32 {
        unsafe fn invoke(
            provider: &RawProvider,
            handle: *mut c_void,
            inputs: &[InChannel<f32>],
            outputs: &[OutChannel<f32>],
            frames: u32,
            input: &Input<'_>,
            wire: &mut RawOutputs,
            report: &mut processing::RawReport,
        ) -> i32 {
            let mut native_inputs = [InputChannel {
                samples: std::ptr::null(),
                capacity: 0,
                reserved: 0,
            }; MAXIMUM_CHANNELS];
            let mut native_outputs = [OutputChannel {
                samples: std::ptr::null_mut(),
                capacity: 0,
                reserved: 0,
            }; MAXIMUM_CHANNELS];
            for (d, s) in native_inputs.iter_mut().zip(inputs) {
                *d = InputChannel {
                    samples: s.samples,
                    capacity: s.capacity,
                    reserved: 0,
                };
            }
            for (d, s) in native_outputs.iter_mut().zip(outputs) {
                *d = OutputChannel {
                    samples: s.samples,
                    capacity: s.capacity,
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
                    native_inputs.as_ptr()
                },
                outputs: if outputs.is_empty() {
                    std::ptr::null()
                } else {
                    native_outputs.as_ptr()
                },
            };
            // SAFETY: forwarded caller contract; fixed stack metadata outlives call.
            unsafe {
                provider.process32.unwrap()(
                    handle,
                    &block,
                    input.points.as_ptr(),
                    input.points.len() as u32,
                    input.events.records.as_ptr(),
                    input.events.count as u32,
                    input.events.bytes().as_ptr(),
                    input.events.used as u32,
                    input.context.map_or(std::ptr::null(), |c| c),
                    wire,
                    report,
                )
            }
        }
    }
    impl Sealed for f64 {
        unsafe fn invoke(
            provider: &RawProvider,
            handle: *mut c_void,
            inputs: &[InChannel<f64>],
            outputs: &[OutChannel<f64>],
            frames: u32,
            input: &Input<'_>,
            wire: &mut RawOutputs,
            report: &mut processing::RawReport,
        ) -> i32 {
            let mut native_inputs = [InputChannel64 {
                samples: std::ptr::null(),
                capacity: 0,
                reserved: 0,
            }; MAXIMUM_CHANNELS];
            let mut native_outputs = [OutputChannel64 {
                samples: std::ptr::null_mut(),
                capacity: 0,
                reserved: 0,
            }; MAXIMUM_CHANNELS];
            for (d, s) in native_inputs.iter_mut().zip(inputs) {
                *d = InputChannel64 {
                    samples: s.samples,
                    capacity: s.capacity,
                    reserved: 0,
                };
            }
            for (d, s) in native_outputs.iter_mut().zip(outputs) {
                *d = OutputChannel64 {
                    samples: s.samples,
                    capacity: s.capacity,
                    reserved: 0,
                };
            }
            let block = Block64 {
                struct_size: size_of::<Block64>() as u32,
                frames,
                input_channels: inputs.len() as u32,
                output_channels: outputs.len() as u32,
                inputs: if inputs.is_empty() {
                    std::ptr::null()
                } else {
                    native_inputs.as_ptr()
                },
                outputs: if outputs.is_empty() {
                    std::ptr::null()
                } else {
                    native_outputs.as_ptr()
                },
            };
            // SAFETY: forwarded caller contract; fixed stack metadata outlives call.
            unsafe {
                provider.process64.unwrap()(
                    handle,
                    &block,
                    input.points.as_ptr(),
                    input.points.len() as u32,
                    input.events.records.as_ptr(),
                    input.events.count as u32,
                    input.events.bytes().as_ptr(),
                    input.events.used as u32,
                    input.context.map_or(std::ptr::null(), |c| c),
                    wire,
                    report,
                )
            }
        }
    }
}
use sealed::{InChannel, OutChannel};
/// Sealed native hosted sample precision (`f32` or `f64`).
pub trait HostedSample: sealed::Sealed + Copy + Default + Send + 'static {
    const PRECISION: Precision;
}
impl HostedSample for f32 {
    const PRECISION: Precision = Precision::F32;
}
impl HostedSample for f64 {
    const PRECISION: Precision = Precision::F64;
}
/// Shared AUDIO admission and native dispatch for the scoped capability and
/// the owned Graph node. All checks precede the native call; a malformed reply
/// or attempted native failure is reported so the caller retires capability.
#[allow(clippy::too_many_arguments)]
fn dispatch<'o, S: HostedSample>(
    state: &AudioState<'_>,
    handle: *mut c_void,
    buses: &processing::EventBuses,
    inputs: &[InChannel<S>],
    outputs: &[OutChannel<S>],
    frames: u32,
    input: &Input<'_>,
    storage: &'o mut OutputStorage,
) -> (Result<Output<'o>, Error>, bool) {
    if S::PRECISION != state.precision {
        return (Err(Error::Precision), false);
    }
    if inputs.len() != state.input_channels as usize
        || outputs.len() != state.output_channels as usize
        || frames > state.maximum_frames
        || inputs.iter().any(|c| c.capacity < frames)
        || outputs.iter().any(|c| c.capacity < frames)
        || !parameters::points_admitted(input.points, frames)
        || !input.events.admitted(frames, &buses.input)
        || input
            .context
            .is_some_and(|c| !c.admitted(state.sample_rate))
    {
        return (Err(Error::Shape), false);
    }
    let mut wire = storage.wire();
    let mut raw = processing::RawReport::initial();
    // SAFETY: exact prepared table on the sole AUDIO worker; caller supplies
    // initialized channel capacities and distinct writable outputs.
    let status = unsafe {
        S::invoke(
            state.provider,
            handle,
            inputs,
            outputs,
            frames,
            input,
            &mut wire,
            &mut raw,
        )
    };
    let result = processing::accepted(status, raw)
        .and_then(|report| {
            if report.stage() != 3
                || report.returned_status() != Some(0)
                || !report.history_may_have_advanced()
            {
                return Err(processing::Error::Abi);
            }
            storage.view(&wire, frames, &buses.output, report)
        })
        .map_err(Error::Processing);
    let retire = matches!(result, Err(Error::Processing(processing::Error::Abi)))
        || (result.is_err() && raw.flags & 1 != 0);
    (result, retire)
}
impl Audio<'_, '_> {
    /// Packed `(inputs, outputs)` of a hosted capability, if any.
    pub fn hosted_channels(&self) -> Option<(usize, usize)> {
        self.hosted
            .map(|h| (h.input_channels as usize, h.output_channels as usize))
    }
    /// Process one hosted block in the prepared precision with complete rich
    /// input and one verified rich output batch.
    ///
    /// Slices are the packed active channels of [`PackedAudioMap`]. Precision,
    /// counts, frames, points, events and context are admitted before native
    /// calls; invalid blocks preserve outputs. Native DSP faults silence valid
    /// output prefixes and retire this capability's hosted processing.
    ///
    /// ```compile_fail,E0499
    /// use ravet_host::vst3::instances::{Audio, processing::{Input, InputEvents, OutputStorage}};
    /// fn overwrite(audio: &mut Audio<'_, '_>, events: &InputEvents, storage: &mut OutputStorage) {
    ///     let a = audio.process_hosted::<f64>(&[], &mut [], 0, Input {points: &[], events, context: None}, storage).unwrap();
    ///     let b = audio.process_hosted::<f64>(&[], &mut [], 0, Input {points: &[], events, context: None}, storage).unwrap();
    ///     drop(a); drop(b);
    /// }
    /// ```
    pub fn process_hosted<'o, S: HostedSample>(
        &mut self,
        inputs: &[&[S]],
        outputs: &mut [&mut [S]],
        frames: u32,
        input: Input<'_>,
        storage: &'o mut OutputStorage,
    ) -> Result<Output<'o>, Error> {
        let state = self.hosted.ok_or(Error::Lifecycle)?;
        if inputs.len() > MAXIMUM_CHANNELS || outputs.len() > MAXIMUM_CHANNELS {
            return Err(Error::Shape);
        }
        let mut native_inputs = [InChannel {
            samples: std::ptr::null(),
            capacity: 0,
        }; MAXIMUM_CHANNELS];
        let mut native_outputs = [OutChannel {
            samples: std::ptr::null_mut(),
            capacity: 0,
        }; MAXIMUM_CHANNELS];
        for (d, s) in native_inputs.iter_mut().zip(inputs) {
            *d = InChannel {
                samples: s.as_ptr(),
                capacity: u32::try_from(s.len()).map_err(|_| Error::Shape)?,
            };
        }
        for (d, s) in native_outputs.iter_mut().zip(outputs.iter_mut()) {
            *d = OutChannel {
                samples: s.as_mut_ptr(),
                capacity: u32::try_from(s.len()).map_err(|_| Error::Shape)?,
            };
        }
        let (result, retire) = dispatch(
            &state,
            self.handle.as_ptr(),
            &self.event_buses,
            &native_inputs[..inputs.len()],
            &native_outputs[..outputs.len()],
            frames,
            &input,
            storage,
        );
        if retire {
            self.hosted = None;
        }
        result
    }
}

#[cfg(test)]
mod mock;
#[cfg(test)]
mod tests;
