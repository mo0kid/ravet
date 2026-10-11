//! Trusted in-process mock of the paired v2/parameter/processing/hosted ABI.
//!
//! Class 0 is an effect advertising inputs [main stereo, aux mono, aux stereo]
//! and outputs [main stereo, aux mono]. Class 1 is an instrument with one
//! stereo main output. DSP runs in the prepared precision:
//! effect main out[c] = main[c]*1.5 + aux_mono*0.25 + aux_stereo[c]*0.125,
//! effect aux out = main[0] - main[1]; instrument out[c][i] = (c+1)*0.1.
//! Every input parameter point is echoed to the output queue; a context with
//! TEMPO adds point id 9999 at offset 0 with value tempo/1000.
//! Opt-in modes: [`PARAMETERS`] advertises ids 1, 5 (writable), 7 (read-only)
//! and 9 (hidden); [`EVENT_BUSES`] advertises event inputs 0 (16 channels) and
//! 1 (2 channels); [`CAPTURE`] copies every processed input (frames, events,
//! payload, points, context) into [`Probe::captures`] (allocates; never used
//! under an allocation counter).
#![allow(clippy::missing_safety_doc, clippy::needless_range_loop)]
use super::{Block64, RawBus, RawInfo, RawProfile, RawProvider, RawReport, RawTiming};
use crate::vst3::instances::{
    self, Block,
    parameters::{self, Point},
    processing::{self, Context, RawEvent, RawOutputs},
};
use crate::vst3::{ClassRecord, FactoryRecord};
use std::{
    cell::Cell,
    ffi::c_void,
    sync::{
        Mutex, OnceLock,
        atomic::{AtomicBool, AtomicU32, AtomicUsize, Ordering},
    },
    thread::ThreadId,
};

pub const FAIL_START_SECOND: u32 = 1;
pub const FAIL_BEGIN_SECOND: u32 = 2;
pub const FAIL_END: u32 = 4;
pub const FAIL_PROCESS: u32 = 8;
pub const REFUSE_F64: u32 = 16;
pub const FAIL_DESTROY: u32 = 32;
pub const MALFORMED_TIMING: u32 = 64;
pub const MALFORMED_BUS: u32 = 128;
pub const PARAMETERS: u32 = 256;
pub const EVENT_BUSES: u32 = 512;
pub const CAPTURE: u32 = 1024;

/// One processed native input as received by the mock.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Capture {
    pub frames: u32,
    pub events: Vec<RawEvent>,
    pub payload: Vec<u8>,
    pub points: Vec<Point>,
    pub context: Option<Context>,
}

/// Observable counters that outlive the mock module itself.
#[derive(Default)]
pub struct Probe {
    pub mode: AtomicU32,
    pub opens: AtomicUsize,
    pub closes: AtomicUsize,
    pub creates: AtomicUsize,
    pub destroys: AtomicUsize,
    pub starts: AtomicUsize,
    pub stops: AtomicUsize,
    pub begins: AtomicUsize,
    pub ends: AtomicUsize,
    pub processes: AtomicUsize,
    pub live_leases: AtomicUsize,
    pub wrong_thread: AtomicUsize,
    pub control_during_audio: AtomicUsize,
    pub log: Mutex<Vec<String>>,
    pub captures: Mutex<Vec<Capture>>,
}
impl Probe {
    pub fn leak() -> &'static Probe {
        Box::leak(Box::default())
    }
    fn note(&self, entry: String) {
        self.log.lock().unwrap().push(entry);
    }
    pub fn entries(&self) -> Vec<String> {
        self.log.lock().unwrap().clone()
    }
    pub fn count(&self, counter: &AtomicUsize) -> usize {
        counter.load(Ordering::SeqCst)
    }
    pub fn captures(&self) -> Vec<Capture> {
        self.captures.lock().unwrap().clone()
    }
}
#[allow(clippy::too_many_arguments)]
fn capture(
    token: *mut c_void,
    frames: u32,
    points: *const Point,
    point_count: u32,
    events: *const RawEvent,
    event_count: u32,
    payload: *const u8,
    payload_bytes: u32,
    context: *const Context,
) {
    let t = unsafe { &*token.cast::<Token>() };
    let c = child(t.child.cast());
    if c.probe.mode.load(Ordering::SeqCst) & CAPTURE == 0 {
        return;
    }
    let slice = |p: *const RawEvent, n: u32| {
        if n == 0 {
            Vec::new()
        } else {
            unsafe { std::slice::from_raw_parts(p, n as usize) }.to_vec()
        }
    };
    let capture = Capture {
        frames,
        events: slice(events, event_count),
        payload: if payload_bytes == 0 {
            Vec::new()
        } else {
            unsafe { std::slice::from_raw_parts(payload, payload_bytes as usize) }.to_vec()
        },
        points: if point_count == 0 {
            Vec::new()
        } else {
            unsafe { std::slice::from_raw_parts(points, point_count as usize) }.to_vec()
        },
        context: (!context.is_null()).then(|| unsafe { *context }),
    };
    c.probe.captures.lock().unwrap().push(capture);
}
thread_local! {
    static NEXT: Cell<Option<&'static Probe>> = const { Cell::new(None) };
}
struct Module {
    probe: &'static Probe,
    children: Cell<u32>,
    /// Native CONTROL exclusion is per loaded module facade.
    live: AtomicUsize,
}
struct Child {
    probe: &'static Probe,
    module: *const Module,
    ordinal: u32,
    class: u32,
    bits: AtomicU32,
    input_active: AtomicU32,
    output_active: AtomicU32,
    running: AtomicBool,
    prepared: AtomicBool,
}
struct Token {
    child: *const Child,
    thread: OnceLock<ThreadId>,
}
fn catalogue(class: u32, output: bool) -> &'static [(u64, u32)] {
    // (arrangement, bus_type)
    match (class, output) {
        (0, false) => &[(0b11, 0), (0b1, 1), (0b11, 1)],
        (0, true) => &[(0b11, 0), (0b1, 1)],
        (_, false) => &[],
        (_, true) => &[(0b11, 0)],
    }
}
unsafe extern "C" fn open(_: *const u8, _: u32, out: *mut *mut c_void, result: *mut i32) -> i32 {
    let probe = NEXT.with(|n| n.get()).expect("probe installed");
    probe.opens.fetch_add(1, Ordering::SeqCst);
    probe.note("open".into());
    unsafe {
        *out = Box::into_raw(Box::new(Module {
            probe,
            children: Cell::new(0),
            live: AtomicUsize::new(0),
        }))
        .cast();
        *result = 0;
    }
    0
}
unsafe extern "C" fn factory(_: *const c_void, out: *mut FactoryRecord) -> i32 {
    unsafe {
        (*out).class_count = 2;
        (&mut (*out).vendor)[..5].copy_from_slice(b"Mock\0");
    }
    0
}
unsafe extern "C" fn class(_: *const c_void, index: u32, out: *mut ClassRecord) -> i32 {
    unsafe {
        (*out).id = [index as u8 + 1; 16];
        (*out).cardinality = 1;
        (&mut (*out).category)[..19].copy_from_slice(b"Audio Module Class\0");
        (&mut (*out).name)[..5].copy_from_slice(b"Mock\0");
        (&mut (*out).vendor)[..5].copy_from_slice(b"Mock\0");
    }
    0
}
unsafe extern "C" fn close(module: *mut c_void, result: *mut i32) -> i32 {
    let module = unsafe { Box::from_raw(module.cast::<Module>()) };
    module.probe.closes.fetch_add(1, Ordering::SeqCst);
    module.probe.note("close".into());
    unsafe { *result = 0 };
    0
}
unsafe extern "C" fn create(
    module: *mut c_void,
    index: u32,
    out: *mut *mut c_void,
    result: *mut i32,
) -> i32 {
    let module = unsafe { &*module.cast::<Module>() };
    let ordinal = module.children.get();
    module.children.set(ordinal + 1);
    module.probe.creates.fetch_add(1, Ordering::SeqCst);
    module.probe.note(format!("create {ordinal}"));
    unsafe {
        *out = Box::into_raw(Box::new(Child {
            probe: module.probe,
            module,
            ordinal,
            class: index,
            bits: AtomicU32::new(0),
            input_active: AtomicU32::new(0),
            output_active: AtomicU32::new(0),
            running: AtomicBool::new(false),
            prepared: AtomicBool::new(false),
        }))
        .cast();
        *result = 0;
    }
    0
}
impl Child {
    fn live(&self) -> usize {
        // SAFETY: children are destroyed before their module closes.
        unsafe { &*self.module }.live.load(Ordering::SeqCst)
    }
}
fn child<'a>(handle: *const c_void) -> &'a Child {
    unsafe { &*handle.cast::<Child>() }
}
fn control(child: &Child) {
    if child.live() != 0 {
        child
            .probe
            .control_during_audio
            .fetch_add(1, Ordering::SeqCst);
    }
}
unsafe extern "C" fn legacy_prepare(
    _: *mut c_void,
    _: *const instances::Profile,
    _: *mut i32,
) -> i32 {
    10
}
unsafe extern "C" fn start(handle: *mut c_void, result: *mut i32) -> i32 {
    let c = child(handle);
    unsafe { *result = 0 };
    // Native CONTROL refuses while any AUDIO capability of the module is live.
    if c.live() != 0 {
        c.probe.control_during_audio.fetch_add(1, Ordering::SeqCst);
        return 10;
    }
    if c.probe.mode.load(Ordering::SeqCst) & FAIL_START_SECOND != 0 && c.ordinal == 1 {
        unsafe { *result = 1 };
        return 4;
    }
    c.running.store(true, Ordering::SeqCst);
    c.probe.starts.fetch_add(1, Ordering::SeqCst);
    c.probe.note(format!("start {}", c.ordinal));
    0
}
unsafe extern "C" fn begin(handle: *mut c_void, out: *mut *mut c_void, result: *mut i32) -> i32 {
    let c = child(handle);
    unsafe { *result = 0 };
    if !c.running.load(Ordering::SeqCst) || !c.prepared.load(Ordering::SeqCst) {
        return 10;
    }
    if c.probe.mode.load(Ordering::SeqCst) & FAIL_BEGIN_SECOND != 0 && c.ordinal == 1 {
        unsafe { *result = 2 };
        return 4;
    }
    c.probe.begins.fetch_add(1, Ordering::SeqCst);
    c.probe.live_leases.fetch_add(1, Ordering::SeqCst);
    unsafe { &*c.module }.live.fetch_add(1, Ordering::SeqCst);
    c.probe.note(format!("begin {}", c.ordinal));
    unsafe {
        *out = Box::into_raw(Box::new(Token {
            child: c,
            thread: OnceLock::new(),
        }))
        .cast();
    }
    0
}
unsafe extern "C" fn legacy_process(_: *mut c_void, _: *const Block, _: *mut i32) -> i32 {
    10
}
unsafe extern "C" fn end(token: *mut c_void, result: *mut i32) -> i32 {
    let t = unsafe { &*token.cast::<Token>() };
    let c = child(t.child.cast());
    unsafe { *result = 0 };
    if c.probe.mode.load(Ordering::SeqCst) & FAIL_END != 0 {
        unsafe { *result = 3 };
        return 4;
    }
    drop(unsafe { Box::from_raw(token.cast::<Token>()) });
    c.probe.ends.fetch_add(1, Ordering::SeqCst);
    c.probe.live_leases.fetch_sub(1, Ordering::SeqCst);
    unsafe { &*c.module }.live.fetch_sub(1, Ordering::SeqCst);
    c.probe.note(format!("end {}", c.ordinal));
    0
}
unsafe extern "C" fn stop(handle: *mut c_void, result: *mut i32) -> i32 {
    let c = child(handle);
    control(c);
    c.running.store(false, Ordering::SeqCst);
    c.probe.stops.fetch_add(1, Ordering::SeqCst);
    c.probe.note(format!("stop {}", c.ordinal));
    unsafe { *result = 0 };
    0
}
unsafe extern "C" fn destroy(handle: *mut c_void, result: *mut i32) -> i32 {
    let c = child(handle);
    control(c);
    unsafe { *result = 0 };
    if c.probe.mode.load(Ordering::SeqCst) & FAIL_DESTROY != 0 {
        return 10;
    }
    let c = unsafe { Box::from_raw(handle.cast::<Child>()) };
    c.probe.destroys.fetch_add(1, Ordering::SeqCst);
    c.probe.note(format!("destroy {}", c.ordinal));
    0
}
unsafe extern "C" fn restart(_: *const c_void, flags: *mut u32) -> i32 {
    unsafe { *flags = 0 };
    0
}
/// (id, flags): 1 automatable, 5 writable, 7 read-only, 9 hidden+read-only.
const MOCK_PARAMETERS: [(u32, u32); 4] = [(1, 1), (5, 0), (7, 2), (9, 18)];
unsafe extern "C" fn p_refresh(
    handle: *mut c_void,
    count: *mut u32,
    _: *mut parameters::RawReport,
) -> i32 {
    let advertised = child(handle).probe.mode.load(Ordering::SeqCst) & PARAMETERS != 0;
    unsafe { *count = if advertised { 4 } else { 0 } };
    0
}
unsafe extern "C" fn p_info(
    handle: *const c_void,
    index: u32,
    record: *mut parameters::RawRecord,
) -> i32 {
    if child(handle).probe.mode.load(Ordering::SeqCst) & PARAMETERS == 0 || index >= 4 {
        return 10;
    }
    let (id, flags) = MOCK_PARAMETERS[index as usize];
    unsafe {
        let r = &mut *record;
        r.id = id;
        r.flags = flags;
        r.title[..6].copy_from_slice(b"Param\0");
    }
    0
}
unsafe extern "C" fn p_value(
    _: *mut c_void,
    _: u32,
    _: u32,
    _: f64,
    _: *mut f64,
    _: *mut parameters::RawReport,
) -> i32 {
    10
}
unsafe extern "C" fn p_format(
    _: *mut c_void,
    _: u32,
    _: f64,
    _: *mut u8,
    _: u32,
    _: *mut u32,
    _: *mut parameters::RawReport,
) -> i32 {
    10
}
unsafe extern "C" fn p_parse(
    _: *mut c_void,
    _: u32,
    _: *const u8,
    _: u32,
    _: *mut f64,
    _: *mut parameters::RawReport,
) -> i32 {
    10
}
unsafe extern "C" fn p_prepare(
    _: *mut c_void,
    _: *const instances::Profile,
    _: *mut parameters::RawReport,
) -> i32 {
    10
}
unsafe extern "C" fn p_process(
    _: *mut c_void,
    _: *const Block,
    _: *const Point,
    _: u32,
    _: *mut parameters::RawReport,
) -> i32 {
    10
}
unsafe extern "C" fn e_refresh(
    handle: *mut c_void,
    selection: *const processing::RawSelection,
    info: *mut processing::RawInfo,
    report: *mut processing::RawReport,
) -> i32 {
    let buses = child(handle).probe.mode.load(Ordering::SeqCst) & EVENT_BUSES != 0;
    unsafe {
        let selection = &*selection;
        let admitted = if buses { 0b11 } else { 0 };
        if selection.input_mask & !admitted != 0 || selection.output_mask != 0 {
            return 1;
        }
        (*info).input_bus_count = if buses { 2 } else { 0 };
        (*info).output_bus_count = 0;
        (*info).input_mask = selection.input_mask;
        (*report).stage = 1;
    }
    0
}
unsafe extern "C" fn e_bus(
    handle: *const c_void,
    direction: u32,
    index: u32,
    bus: *mut processing::RawBus,
) -> i32 {
    if child(handle).probe.mode.load(Ordering::SeqCst) & EVENT_BUSES == 0
        || direction != 0
        || index >= 2
    {
        return 10;
    }
    unsafe {
        let b = &mut *bus;
        b.index = index;
        b.channels = if index == 0 { 16 } else { 2 };
        b.bus_type = index;
        b.flags = u32::from(index == 0);
        b.name[..7].copy_from_slice(b"Events\0");
    }
    0
}
unsafe extern "C" fn e_assignment(_: *const c_void, _: u32, _: u32, _: u32, _: *mut u32) -> i32 {
    10
}
unsafe extern "C" fn e_prepare(
    _: *mut c_void,
    _: *const instances::Profile,
    _: *mut processing::RawReport,
) -> i32 {
    10
}
#[allow(clippy::too_many_arguments)]
unsafe extern "C" fn e_process(
    _: *mut c_void,
    _: *const Block,
    _: *const Point,
    _: u32,
    _: *const RawEvent,
    _: u32,
    _: *const u8,
    _: u32,
    _: *const Context,
    _: *mut RawOutputs,
    _: *mut processing::RawReport,
) -> i32 {
    10
}
fn control_report(report: *mut RawReport, stage: u32) {
    unsafe {
        *report = RawReport {
            stage,
            index: u32::MAX,
            ..RawReport::initial()
        }
    };
}
unsafe extern "C" fn refresh_audio(
    handle: *mut c_void,
    info: *mut RawInfo,
    report: *mut RawReport,
) -> i32 {
    let c = child(handle);
    control(c);
    c.prepared.store(false, Ordering::SeqCst);
    unsafe {
        (*info).input_buses = catalogue(c.class, false).len() as u32;
        (*info).output_buses = catalogue(c.class, true).len() as u32;
    }
    control_report(report, 105);
    0
}
unsafe extern "C" fn audio_bus(
    handle: *const c_void,
    direction: u32,
    index: u32,
    bus: *mut RawBus,
    report: *mut RawReport,
) -> i32 {
    let c = child(handle);
    let (arrangement, kind) = catalogue(c.class, direction == 1)[index as usize];
    let name: &[u8] = if kind == 0 {
        b"Main\0"
    } else {
        b"Side \xc3\xa9\0"
    };
    unsafe {
        let bus = &mut *bus;
        bus.index = index;
        bus.channels = arrangement.count_ones();
        bus.bus_type = kind;
        bus.flags = u32::from(kind == 0);
        bus.arrangement = arrangement;
        bus.name[..name.len()].copy_from_slice(name);
        if c.probe.mode.load(Ordering::SeqCst) & MALFORMED_BUS != 0 && index == 1 {
            bus.arrangement = 0b111;
        }
    }
    control_report(report, 105);
    0
}
unsafe extern "C" fn prepare(
    handle: *mut c_void,
    profile: *const RawProfile,
    report: *mut RawReport,
) -> i32 {
    let c = child(handle);
    control(c);
    let p = unsafe { &*profile };
    c.prepared.store(false, Ordering::SeqCst);
    if p.sample_bits == 64 && c.probe.mode.load(Ordering::SeqCst) & REFUSE_F64 != 0 {
        unsafe {
            *report = RawReport {
                stage: 107,
                failure: 2,
                index: u32::MAX,
                flags: 3,
                native_result: 1,
                ..RawReport::initial()
            }
        };
        return 4;
    }
    let mut inputs = 0;
    let mut outputs = 0;
    for (requests, count, mask, output) in [
        (&p.inputs, p.input_buses, &mut inputs, false),
        (&p.outputs, p.output_buses, &mut outputs, true),
    ] {
        let advertised = catalogue(c.class, output);
        if count as usize != advertised.len() {
            control_report(report, 119);
            unsafe { (*report).failure = 1 };
            return 1;
        }
        for (i, r) in requests.iter().take(count as usize).enumerate() {
            if r.arrangement != advertised[i].0 {
                unsafe {
                    *report = RawReport {
                        stage: 106,
                        failure: 6,
                        index: u32::MAX,
                        ..RawReport::initial()
                    }
                };
                return 3;
            }
            if r.active != 0 {
                *mask |= 1 << i;
            }
        }
    }
    c.bits.store(p.sample_bits, Ordering::SeqCst);
    c.input_active.store(inputs, Ordering::SeqCst);
    c.output_active.store(outputs, Ordering::SeqCst);
    c.prepared.store(true, Ordering::SeqCst);
    c.probe
        .note(format!("prepare {} bits={}", c.ordinal, p.sample_bits));
    control_report(report, 119);
    0
}
unsafe extern "C" fn timing(
    handle: *const c_void,
    timing: *mut RawTiming,
    report: *mut RawReport,
) -> i32 {
    let c = child(handle);
    unsafe {
        let t = &mut *timing;
        if c.class == 0 {
            t.latency_samples = 3;
            t.tail_kind = 1;
            t.tail_samples = 480;
        } else {
            t.latency_samples = 0;
            t.tail_kind = 2;
        }
        if c.probe.mode.load(Ordering::SeqCst) & MALFORMED_TIMING != 0 {
            t.tail_kind = 2;
            t.tail_samples = 5;
        }
    }
    control_report(report, 121);
    0
}
trait Value:
    Copy
    + From<f32>
    + std::ops::Mul<Output = Self>
    + std::ops::Add<Output = Self>
    + std::ops::Sub<Output = Self>
{
}
impl Value for f32 {}
impl Value for f64 {}
#[allow(clippy::too_many_arguments)]
fn run<S: Value>(
    token: *mut c_void,
    bits: u32,
    frames: u32,
    inputs: &[(*const S, u32)],
    outputs: &[(*mut S, u32)],
    points: *const Point,
    point_count: u32,
    context: *const Context,
    wire: *mut RawOutputs,
    report: *mut processing::RawReport,
) -> i32 {
    let t = unsafe { &*token.cast::<Token>() };
    let c = child(t.child.cast());
    let current = std::thread::current().id();
    if *t.thread.get_or_init(|| current) != current {
        c.probe.wrong_thread.fetch_add(1, Ordering::SeqCst);
        return 10;
    }
    if c.bits.load(Ordering::SeqCst) != bits {
        return 1;
    }
    let calls = c.probe.processes.fetch_add(1, Ordering::SeqCst);
    let frames = frames as usize;
    let read = |ch: usize, i: usize| unsafe { *inputs[ch].0.add(i) };
    let write = |ch: usize, i: usize, v: S| unsafe { *outputs[ch].0.add(i) = v };
    if c.probe.mode.load(Ordering::SeqCst) & FAIL_PROCESS != 0 && calls > 0 {
        for ch in 0..outputs.len() {
            for i in 0..frames {
                write(ch, i, S::from(0.));
            }
        }
        unsafe {
            *report = processing::RawReport {
                stage: 3,
                failure: 1,
                flags: 1 | 2 | 8 | 16,
                native_result: 1,
                ..processing::RawReport::initial()
            }
        };
        return 4;
    }
    let ia = c.input_active.load(Ordering::SeqCst);
    let oa = c.output_active.load(Ordering::SeqCst);
    if c.class == 0 {
        // Packed input offsets in native index order.
        let mut next = 0;
        let mut base = [usize::MAX; 3];
        for (i, &(arrangement, _)) in catalogue(0, false).iter().enumerate() {
            if ia & (1 << i) != 0 {
                base[i] = next;
                next += arrangement.count_ones() as usize;
            }
        }
        let mut out = 0;
        for i in 0..frames {
            let main = |ch: usize| {
                if base[0] == usize::MAX {
                    S::from(0.)
                } else {
                    read(base[0] + ch, i)
                }
            };
            if oa & 1 != 0 {
                for ch in 0..2 {
                    let mut v = main(ch) * S::from(1.5);
                    if base[1] != usize::MAX {
                        v = v + read(base[1], i) * S::from(0.25);
                    }
                    if base[2] != usize::MAX {
                        v = v + read(base[2] + ch, i) * S::from(0.125);
                    }
                    write(ch, i, v);
                }
                out = 2;
            }
            if oa & 2 != 0 {
                write(out, i, main(0) - main(1));
            }
        }
    } else {
        for ch in 0..outputs.len() {
            for i in 0..frames {
                write(ch, i, S::from(0.1) * S::from(ch as f32 + 1.));
            }
        }
    }
    let w = unsafe { &mut *wire };
    let mut queues = 0usize;
    let mut count = 0usize;
    let mut push = |id: u32, offset: u32, value: f64| {
        let ids = unsafe {
            std::slice::from_raw_parts_mut(w.parameter_ids, w.parameter_queue_capacity as usize)
        };
        if !ids[..queues].contains(&id) {
            ids[queues] = id;
            queues += 1;
        }
        unsafe { *w.parameter_points.add(count) = Point { id, offset, value } };
        count += 1;
    };
    if !context.is_null() && unsafe { (*context).state } & Context::TEMPO != 0 {
        push(9999, 0, unsafe { (*context).tempo } / 1000.);
    }
    for i in 0..point_count as usize {
        let p = unsafe { *points.add(i) };
        push(p.id, p.offset, p.value);
    }
    w.parameter_queue_count = queues as u32;
    w.parameter_point_count = count as u32;
    unsafe {
        *report = processing::RawReport {
            stage: 3,
            flags: 1 | 2 | 16 | 32,
            parameter_queue_count: queues as u32,
            parameter_point_count: count as u32,
            ..processing::RawReport::initial()
        }
    };
    0
}
#[allow(clippy::too_many_arguments)]
unsafe extern "C" fn process32(
    token: *mut c_void,
    block: *const Block,
    points: *const Point,
    point_count: u32,
    events: *const RawEvent,
    event_count: u32,
    payload: *const u8,
    payload_bytes: u32,
    context: *const Context,
    wire: *mut RawOutputs,
    report: *mut processing::RawReport,
) -> i32 {
    let b = unsafe { &*block };
    capture(
        token,
        b.frames,
        points,
        point_count,
        events,
        event_count,
        payload,
        payload_bytes,
        context,
    );
    let mut inputs = [(std::ptr::null(), 0); 64];
    let mut outputs = [(std::ptr::null_mut(), 0); 64];
    for i in 0..b.input_channels as usize {
        let c = unsafe { *b.inputs.add(i) };
        inputs[i] = (c.samples, c.capacity);
    }
    for i in 0..b.output_channels as usize {
        let c = unsafe { *b.outputs.add(i) };
        outputs[i] = (c.samples, c.capacity);
    }
    run::<f32>(
        token,
        32,
        b.frames,
        &inputs[..b.input_channels as usize],
        &outputs[..b.output_channels as usize],
        points,
        point_count,
        context,
        wire,
        report,
    )
}
#[allow(clippy::too_many_arguments)]
unsafe extern "C" fn process64(
    token: *mut c_void,
    block: *const Block64,
    points: *const Point,
    point_count: u32,
    events: *const RawEvent,
    event_count: u32,
    payload: *const u8,
    payload_bytes: u32,
    context: *const Context,
    wire: *mut RawOutputs,
    report: *mut processing::RawReport,
) -> i32 {
    let b = unsafe { &*block };
    capture(
        token,
        b.frames,
        points,
        point_count,
        events,
        event_count,
        payload,
        payload_bytes,
        context,
    );
    let mut inputs = [(std::ptr::null(), 0); 64];
    let mut outputs = [(std::ptr::null_mut(), 0); 64];
    for i in 0..b.input_channels as usize {
        let c = unsafe { *b.inputs.add(i) };
        inputs[i] = (c.samples, c.capacity);
    }
    for i in 0..b.output_channels as usize {
        let c = unsafe { *b.outputs.add(i) };
        outputs[i] = (c.samples, c.capacity);
    }
    run::<f64>(
        token,
        64,
        b.frames,
        &inputs[..b.input_channels as usize],
        &outputs[..b.output_channels as usize],
        points,
        point_count,
        context,
        wire,
        report,
    )
}
/// Leaked exact-paired tables for one test.
pub struct Tables {
    pub instance: &'static instances::RawProvider,
    pub parameters: &'static parameters::RawProvider,
    pub processing: &'static processing::RawProvider,
    pub hosted: &'static RawProvider,
}
pub fn tables() -> Tables {
    let instance: &'static instances::RawProvider = Box::leak(Box::new(instances::RawProvider {
        struct_size: size_of::<instances::RawProvider>() as u32,
        version: 2,
        open: Some(open),
        factory: Some(factory),
        class_info: Some(class),
        close: Some(close),
        create: Some(create),
        prepare: Some(legacy_prepare),
        start: Some(start),
        begin_audio: Some(begin),
        process: Some(legacy_process),
        end_audio: Some(end),
        stop: Some(stop),
        destroy: Some(destroy),
        restart_requests: Some(restart),
    }));
    let parameters: &'static parameters::RawProvider =
        Box::leak(Box::new(parameters::RawProvider {
            struct_size: size_of::<parameters::RawProvider>() as u32,
            version: 1,
            provider: instance,
            refresh: Some(p_refresh),
            info: Some(p_info),
            value: Some(p_value),
            format: Some(p_format),
            parse: Some(p_parse),
            prepare: Some(p_prepare),
            process: Some(p_process),
        }));
    let processing: &'static processing::RawProvider =
        Box::leak(Box::new(processing::RawProvider {
            struct_size: size_of::<processing::RawProvider>() as u32,
            version: 1,
            provider: instance,
            refresh: Some(e_refresh),
            bus_info: Some(e_bus),
            midi_assignment: Some(e_assignment),
            prepare: Some(e_prepare),
            process: Some(e_process),
        }));
    let hosted: &'static RawProvider = Box::leak(Box::new(RawProvider {
        struct_size: size_of::<RawProvider>() as u32,
        version: 1,
        instance_origin: instance,
        processing_origin: processing,
        refresh_audio: Some(refresh_audio),
        audio_bus: Some(audio_bus),
        prepare: Some(prepare),
        timing: Some(timing),
        process32: Some(process32),
        process64: Some(process64),
    }));
    Tables {
        instance,
        parameters,
        processing,
        hosted,
    }
}
/// Load a mock Library bound to `probe`.
pub fn library(tables: &Tables, probe: &'static Probe) -> instances::Library<'static> {
    NEXT.with(|n| n.set(Some(probe)));
    // SAFETY: trusted in-process mock implementing the exact v2 ABI.
    let provider = unsafe { instances::Provider::from_raw(tables.instance).unwrap() };
    unsafe { provider.load("mock.vst3").unwrap() }
}
