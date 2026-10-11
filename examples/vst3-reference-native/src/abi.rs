//! C ABI for `include/RavetVst3Reference.h`. Handles are exclusively owned by
//! the serialized lifecycle/processing caller; pointer validity follows the
//! header. Unwinding panics are contained and reported as status 2.
#![allow(unsafe_code)]
use crate::{
    EventProbe, KeyedGain, MAXIMUM_PROBE_RECORDS, PARAMETER_CODE, ProbeEvent, ProbeHeader,
    ProbeRecord, ReferenceSample,
};
use ravet_foundation::{
    AudioBuffer, AudioLayout, ProcessBlock, ProcessConfig, ProcessStatus, Processor, Transport,
};
use std::{
    panic::{AssertUnwindSafe, catch_unwind},
    slice,
};

const OK: i32 = 0;
const REJECTED: i32 = 1;
const PANICKED: i32 = 2;

struct Engine<S> {
    processor: KeyedGain,
    key_channels: usize,
    maximum_frames: usize,
    scratch: Vec<S>,
}
enum Precision {
    Unprepared,
    Single(Engine<f32>),
    Double(Engine<f64>),
}
/// Opaque native keyed-gain handle.
pub struct RavetKeyedGain {
    precision: Precision,
}

/// True when two channels of `frames` samples starting at `a` and `b` overlap.
/// Overlapping output channels are rejected rather than turned into aliasing
/// mutable slices.
fn overlaps<S>(a: *const S, b: *const S, frames: usize) -> bool {
    let bytes = frames.saturating_mul(size_of::<S>());
    let (a, b) = (a as usize, b as usize);
    frames > 0 && a.abs_diff(b) < bytes
}

/// Zero `frames` samples at each non-null channel, one channel at a time (no
/// two live mutable slices), for `frames` in 1..=65536.
///
/// # Safety
/// Each non-null channel holds `frames` writable samples.
unsafe fn silence_channels<S: ReferenceSample>(channels: &[*mut S], frames: usize) {
    if frames == 0 || frames > 65536 {
        return;
    }
    for &channel in channels {
        if !channel.is_null() {
            // SAFETY: caller contract; this slice ends before the next starts.
            unsafe { slice::from_raw_parts_mut(channel, frames) }.fill(S::default());
        }
    }
}

fn prepare_engine<S: ReferenceSample>(
    rate: f64,
    key_channels: usize,
    maximum_frames: usize,
) -> Option<Engine<S>> {
    let mut config = ProcessConfig::new(rate, maximum_frames, 0, AudioLayout::STEREO).ok()?;
    if key_channels > 0 {
        config = config
            .with_auxiliary_buses(&[AudioLayout::new(key_channels, 0).ok()?])
            .ok()?;
    }
    let mut processor = KeyedGain::new();
    Processor::<S>::prepare(&mut processor, config).ok()?;
    let mut scratch = Vec::new();
    scratch.try_reserve_exact(maximum_frames * 4).ok()?;
    scratch.resize(maximum_frames * 4, S::default());
    Some(Engine {
        processor,
        key_channels,
        maximum_frames,
        scratch,
    })
}

/// # Safety
/// Pointer arrays must hold 2 main, `key_channels` key and 2 distinct output
/// channels of at least `frames` samples each.
unsafe fn process_engine<S: ReferenceSample>(
    engine: &mut Engine<S>,
    main: *const *const S,
    key: *const *const S,
    outputs: *const *mut S,
    frames: usize,
) -> i32 {
    if outputs.is_null() {
        return REJECTED;
    }
    // SAFETY: header contract: two output channel pointers.
    let out = unsafe { slice::from_raw_parts(outputs, 2) };
    // SAFETY: header contract: each non-null output holds `frames` samples.
    let silence = |out: &[*mut S]| unsafe { silence_channels(out, frames) };
    if frames > engine.maximum_frames
        || out.iter().any(|p| frames > 0 && p.is_null())
        || overlaps(out[0], out[1], frames)
        || (frames > 0 && main.is_null())
        || (frames > 0 && engine.key_channels > 0 && key.is_null())
    {
        silence(out);
        return REJECTED;
    }
    if frames == 0 {
        return OK;
    }
    let stride = engine.maximum_frames;
    // SAFETY: header contract for main/key channel pointer arrays.
    let main = unsafe { slice::from_raw_parts(main, 2) };
    let key = if engine.key_channels > 0 {
        // SAFETY: as above.
        unsafe { slice::from_raw_parts(key, engine.key_channels) }
    } else {
        &[]
    };
    if main.iter().chain(key).any(|p| p.is_null()) {
        silence(out);
        return REJECTED;
    }
    // Copy every input before writing outputs so inputs may alias outputs.
    for (index, &source) in main.iter().chain(key).enumerate() {
        // SAFETY: each input holds `frames` readable samples.
        let source = unsafe { slice::from_raw_parts(source, frames) };
        engine.scratch[index * stride..index * stride + frames].copy_from_slice(source);
    }
    let (main_scratch, key_scratch) = engine.scratch.split_at(2 * stride);
    let ins = [
        &main_scratch[..frames],
        &main_scratch[stride..stride + frames],
    ];
    let keys = [
        &key_scratch[..frames],
        &key_scratch[stride..stride + frames],
    ];
    // SAFETY: two distinct writable output channels of `frames` samples.
    let (left, right) = unsafe {
        (
            slice::from_raw_parts_mut(out[0], frames),
            slice::from_raw_parts_mut(out[1], frames),
        )
    };
    let mut outs = [left, right];
    let Ok(audio) = AudioBuffer::separate(&ins, &mut outs, frames) else {
        return REJECTED;
    };
    let mut no_outputs: [&mut [S]; 0] = [];
    let Ok(key_audio) =
        AudioBuffer::separate(&keys[..engine.key_channels], &mut no_outputs, frames)
    else {
        return REJECTED;
    };
    let mut auxiliary = [key_audio];
    let mut block = ProcessBlock::new(audio, &[], Transport::default());
    if engine.key_channels > 0 {
        block.auxiliary = Some(&mut auxiliary);
    }
    match engine.processor.process(&mut block) {
        ProcessStatus::Continue => OK,
        _ => REJECTED,
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn ravet_keyed_gain_create() -> *mut RavetKeyedGain {
    catch_unwind(|| {
        Box::into_raw(Box::new(RavetKeyedGain {
            precision: Precision::Unprepared,
        }))
    })
    .unwrap_or(std::ptr::null_mut())
}
/// # Safety
/// `handle` is null or an undestroyed handle from `ravet_keyed_gain_create`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn ravet_keyed_gain_destroy(handle: *mut RavetKeyedGain) {
    if !handle.is_null() {
        // SAFETY: exclusive ownership transferred back from create.
        let _ = catch_unwind(AssertUnwindSafe(|| drop(unsafe { Box::from_raw(handle) })));
    }
}
/// # Safety
/// `handle` is a live, exclusively owned handle.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn ravet_keyed_gain_prepare(
    handle: *mut RavetKeyedGain,
    sample_bits: u32,
    rate: f64,
    key_channels: u32,
    maximum_frames: u32,
) -> i32 {
    // SAFETY: header contract.
    let Some(handle) = (unsafe { handle.as_mut() }) else {
        return REJECTED;
    };
    catch_unwind(AssertUnwindSafe(|| {
        handle.precision = Precision::Unprepared;
        if key_channels > 2 || !(1..=65536).contains(&maximum_frames) {
            return REJECTED;
        }
        let (keys, frames) = (key_channels as usize, maximum_frames as usize);
        handle.precision = match sample_bits {
            32 => prepare_engine::<f32>(rate, keys, frames).map(Precision::Single),
            64 => prepare_engine::<f64>(rate, keys, frames).map(Precision::Double),
            _ => None,
        }
        .unwrap_or(Precision::Unprepared);
        if matches!(handle.precision, Precision::Unprepared) {
            REJECTED
        } else {
            OK
        }
    }))
    .unwrap_or(PANICKED)
}
macro_rules! keyed_process {
    ($name:ident, $sample:ty, $variant:ident) => {
        /// # Safety
        /// See `RavetVst3Reference.h`: live handle; 2 main, prepared key and 2
        /// distinct output channels of at least `frames` samples.
        #[unsafe(no_mangle)]
        pub unsafe extern "C" fn $name(
            handle: *mut RavetKeyedGain,
            main: *const *const $sample,
            key: *const *const $sample,
            outputs: *const *mut $sample,
            frames: u32,
        ) -> i32 {
            // SAFETY: header contract.
            let Some(handle) = (unsafe { handle.as_mut() }) else {
                return REJECTED;
            };
            catch_unwind(AssertUnwindSafe(|| match &mut handle.precision {
                // SAFETY: forwarded header contract.
                Precision::$variant(engine) => unsafe {
                    process_engine(engine, main, key, outputs, frames as usize)
                },
                // Unprepared or prepared for the other precision: silence
                // the valid outputs and reject.
                _ => {
                    if !outputs.is_null() {
                        // SAFETY: header contract: `outputs` holds two channel
                        // pointers, each non-null one with `frames` samples.
                        unsafe {
                            silence_channels(slice::from_raw_parts(outputs, 2), frames as usize)
                        };
                    }
                    REJECTED
                }
            }))
            .unwrap_or(PANICKED)
        }
    };
}
keyed_process!(ravet_keyed_gain_process32, f32, Single);
keyed_process!(ravet_keyed_gain_process64, f64, Double);

/// C record; field meaning per code is documented in the header.
#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct RavetEventProbeRecord {
    pub offset: u32,
    pub code: u16,
    pub flags: u16,
    pub bus: i32,
    pub a: i32,
    pub b: i32,
    pub c: i32,
    pub d: i32,
    pub f0: f32,
    pub f1: f32,
    pub reserved: u32,
    pub ppq_position: f64,
    pub value: f64,
    pub integer: u64,
    pub payload: *const std::ffi::c_void,
    pub payload_count: u32,
    pub reserved2: u32,
}
const _: () = assert!(size_of::<RavetEventProbeRecord>() == 80);

/// Opaque native event-probe handle with prepared record storage.
pub struct RavetEventProbe {
    probe: EventProbe,
    records: Vec<ProbeRecord<'static>>,
}

/// # Safety
/// Payload pointers must reference `payload_count` bytes (data) or UTF-16
/// units (text) that stay valid for the call.
unsafe fn convert(raw: &RavetEventProbeRecord) -> Option<ProbeRecord<'static>> {
    let bytes = || -> Option<&'static [u8]> {
        match (raw.payload.is_null(), raw.payload_count) {
            (_, 0) => Some(&[]),
            (true, _) => None,
            // SAFETY: header contract; the borrow ends with the call.
            _ => Some(unsafe {
                slice::from_raw_parts(raw.payload.cast(), raw.payload_count as usize)
            }),
        }
    };
    let text = || -> Option<&'static [u16]> {
        match (raw.payload.is_null(), raw.payload_count) {
            (_, 0) => Some(&[]),
            (true, _) => None,
            _ if !(raw.payload as usize).is_multiple_of(2) => None,
            // SAFETY: header contract; aligned UTF-16 units valid for the call.
            _ => Some(unsafe {
                slice::from_raw_parts(raw.payload.cast(), raw.payload_count as usize)
            }),
        }
    };
    let mask = |v: i32| u16::try_from(v).ok();
    let event = match raw.code {
        0 => ProbeEvent::NoteOn {
            channel: raw.a,
            pitch: raw.b,
            note_id: raw.c,
            length: raw.d,
            tuning: raw.f0,
            velocity: raw.f1,
        },
        1 => ProbeEvent::NoteOff {
            channel: raw.a,
            pitch: raw.b,
            note_id: raw.c,
            tuning: raw.f0,
            velocity: raw.f1,
        },
        2 => ProbeEvent::Data {
            data_type: raw.a as u32,
            bytes: bytes()?,
        },
        3 => ProbeEvent::PolyPressure {
            channel: raw.a,
            pitch: raw.b,
            note_id: raw.c,
            pressure: raw.f1,
        },
        4 => ProbeEvent::ExpressionValue {
            type_id: raw.a as u32,
            note_id: raw.c,
            value: raw.value,
        },
        5 => ProbeEvent::ExpressionText {
            type_id: raw.a as u32,
            note_id: raw.c,
            text: text()?,
        },
        6 => ProbeEvent::Chord {
            root: raw.a,
            bass: raw.b,
            mask: mask(raw.d)?,
            text: text()?,
        },
        7 => ProbeEvent::Scale {
            root: raw.a,
            mask: mask(raw.d)?,
            text: text()?,
        },
        8 => ProbeEvent::ExpressionInteger {
            type_id: raw.a as u32,
            note_id: raw.c,
            value: raw.integer,
        },
        PARAMETER_CODE => ProbeEvent::Parameter {
            id: raw.a as u32,
            value: raw.value,
        },
        _ => return None,
    };
    Some(ProbeRecord {
        header: ProbeHeader {
            offset: raw.offset,
            bus: raw.bus,
            flags: raw.flags,
            ppq_position: raw.ppq_position,
        },
        event,
    })
}

#[unsafe(no_mangle)]
pub extern "C" fn ravet_event_probe_create() -> *mut RavetEventProbe {
    catch_unwind(|| {
        Box::into_raw(Box::new(RavetEventProbe {
            probe: EventProbe::new(),
            records: Vec::new(),
        }))
    })
    .unwrap_or(std::ptr::null_mut())
}
/// # Safety
/// `handle` is null or an undestroyed handle from `ravet_event_probe_create`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn ravet_event_probe_destroy(handle: *mut RavetEventProbe) {
    if !handle.is_null() {
        // SAFETY: exclusive ownership transferred back from create.
        let _ = catch_unwind(AssertUnwindSafe(|| drop(unsafe { Box::from_raw(handle) })));
    }
}
/// # Safety
/// `handle` is a live, exclusively owned handle.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn ravet_event_probe_prepare(
    handle: *mut RavetEventProbe,
    maximum_frames: u32,
) -> i32 {
    // SAFETY: header contract.
    let Some(handle) = (unsafe { handle.as_mut() }) else {
        return REJECTED;
    };
    catch_unwind(AssertUnwindSafe(|| {
        if handle.probe.prepare(maximum_frames as usize).is_err() {
            return REJECTED;
        }
        handle.records.clear();
        if handle
            .records
            .try_reserve_exact(MAXIMUM_PROBE_RECORDS)
            .is_err()
        {
            return REJECTED;
        }
        OK
    }))
    .unwrap_or(PANICKED)
}
/// # Safety
/// `handle` is a live, exclusively owned handle.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn ravet_event_probe_reset(handle: *mut RavetEventProbe) {
    // SAFETY: header contract.
    if let Some(handle) = unsafe { handle.as_mut() } {
        handle.probe.reset();
    }
}

/// # Safety
/// See the header: `records` holds `count` records whose payloads are valid for
/// the call; `left`/`right` are distinct and hold `frames` samples.
unsafe fn probe_process<S: ReferenceSample>(
    handle: *mut RavetEventProbe,
    records: *const RavetEventProbeRecord,
    count: u32,
    left: *mut S,
    right: *mut S,
    frames: u32,
    digest: *mut u32,
) -> i32 {
    // SAFETY: header contract.
    let Some(handle) = (unsafe { handle.as_mut() }) else {
        return REJECTED;
    };
    let frames = frames as usize;
    // Validate the block size against the prepared maximum and the channel
    // pointers before any output slice is formed; overlapping channels are
    // rejected instead of becoming aliasing mutable slices.
    if frames > 65536 || overlaps(left, right, frames) {
        return REJECTED;
    }
    let prepared = handle.probe.maximum_frames().is_some_and(|m| frames <= m);
    if !prepared || (frames > 0 && (left.is_null() || right.is_null())) {
        // SAFETY: header contract: each non-null output holds `frames` samples.
        unsafe { silence_channels(&[left, right], frames) };
        return REJECTED;
    }
    let (left, right): (&mut [S], &mut [S]) = if frames == 0 {
        (&mut [], &mut [])
    } else {
        // SAFETY: header contract: distinct outputs of `frames` samples.
        unsafe {
            (
                slice::from_raw_parts_mut(left, frames),
                slice::from_raw_parts_mut(right, frames),
            )
        }
    };
    catch_unwind(AssertUnwindSafe(|| {
        let reject = |left: &mut [S], right: &mut [S]| {
            left.fill(S::default());
            right.fill(S::default());
            REJECTED
        };
        let count = count as usize;
        if count > MAXIMUM_PROBE_RECORDS || (count > 0 && records.is_null()) {
            return reject(left, right);
        }
        let raw = if count == 0 {
            &[][..]
        } else {
            // SAFETY: header contract.
            unsafe { slice::from_raw_parts(records, count) }
        };
        handle.records.clear();
        for record in raw {
            // SAFETY: forwarded payload contract.
            match unsafe { convert(record) } {
                Some(converted) if handle.records.len() < handle.records.capacity() => {
                    handle.records.push(converted)
                }
                _ => {
                    handle.records.clear();
                    return reject(left, right);
                }
            }
        }
        let result = handle.probe.render(&handle.records, left, right, frames);
        handle.records.clear();
        match result {
            Ok(value) => {
                // SAFETY: optional caller-owned result.
                if let Some(digest) = unsafe { digest.as_mut() } {
                    *digest = value;
                }
                OK
            }
            Err(_) => REJECTED,
        }
    }))
    .unwrap_or(PANICKED)
}
/// # Safety
/// See `RavetVst3Reference.h`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn ravet_event_probe_process32(
    handle: *mut RavetEventProbe,
    records: *const RavetEventProbeRecord,
    count: u32,
    left: *mut f32,
    right: *mut f32,
    frames: u32,
    digest: *mut u32,
) -> i32 {
    // SAFETY: forwarded header contract.
    unsafe { probe_process(handle, records, count, left, right, frames, digest) }
}
/// # Safety
/// See `RavetVst3Reference.h`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn ravet_event_probe_process64(
    handle: *mut RavetEventProbe,
    records: *const RavetEventProbeRecord,
    count: u32,
    left: *mut f64,
    right: *mut f64,
    frames: u32,
    digest: *mut u32,
) -> i32 {
    // SAFETY: forwarded header contract.
    unsafe { probe_process(handle, records, count, left, right, frames, digest) }
}

#[cfg(test)]
mod tests;
