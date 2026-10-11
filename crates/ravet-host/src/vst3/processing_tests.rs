use super::*;
use std::{
    cell::{Cell, RefCell},
    ptr::NonNull,
};
fn header(offset: u32) -> Header {
    Header {
        bus: 1,
        offset,
        ppq_position: -0.125,
        flags: 0xc001,
    }
}
fn note() -> Note {
    Note {
        channel: 1,
        pitch: 69,
        note_id: 401,
        tuning: -3.25,
        velocity: 0.75,
        length: 100,
    }
}
#[test]
fn exact_c_abi_and_report_provenance() {
    assert_eq!(size_of::<RawEvent>(), 112);
    assert_eq!(size_of::<Context>(), 120);
    assert_eq!(size_of::<RawOutputs>(), 72);
    assert_eq!(size_of::<RawReport>(), 56);
    assert_eq!(size_of::<RawProvider>(), 56);
    assert_eq!(size_of::<RawSelection>(), 16);
    assert_eq!(size_of::<RawInfo>(), 32);
    assert_eq!(size_of::<RawBus>(), 536);
    assert_eq!(std::mem::offset_of!(RawEvent, integer_value), 64);
    assert_eq!(std::mem::offset_of!(RawReport, native_result), 32);
    let initial = RawReport::initial();
    for raw in [
        RawReport {
            flags: 2,
            ..initial
        },
        RawReport {
            flags: 4,
            ..initial
        },
        RawReport {
            flags: 7,
            ..initial
        },
        RawReport {
            flags: 16,
            ..initial
        },
        RawReport {
            flags: 1,
            ..initial
        },
        RawReport {
            native_result: 4242,
            ..initial
        },
        RawReport {
            reserved: 1,
            ..initial
        },
        RawReport {
            reserved2: 1,
            ..initial
        },
        RawReport {
            flags: 64,
            ..initial
        },
        RawReport {
            stage: 5,
            ..initial
        },
        RawReport {
            failure: 5,
            ..initial
        },
        RawReport {
            event_count: 2049,
            ..initial
        },
    ] {
        assert_eq!(Report::admit(raw), Err(Error::Abi));
    }
    let raw = RawReport {
        stage: 3,
        flags: 25 | 4,
        failure: 2,
        ..initial
    };
    let report = Report::admit(raw).unwrap();
    assert!(report.threw() && report.invalidated() && report.history_may_have_advanced());
    assert_eq!(report.returned_status(), None);
    assert_eq!(report.index(), None);
    let report = Report::admit(RawReport {
        stage: 3,
        flags: 27,
        native_result: 4242,
        failure: 1,
        ..initial
    })
    .unwrap();
    assert_eq!(report.returned_status(), Some(4242));
}
#[test]
fn all_nine_input_kinds_copy_unicode_binary_and_reject_atomically() {
    let mut storage = InputEvents::new(16, 128).unwrap();
    let mut binary = [0xff, 0, 0x80, 0xf0, 0xf7];
    let text = String::from("A😀");
    let events = [
        Kind::NoteOn(note()),
        Kind::NoteOff(Note {
            note_id: -1,
            ..note()
        }),
        Kind::SysEx(&binary),
        Kind::PolyPressure {
            channel: 1,
            pitch: 71,
            note_id: 401,
            pressure: 0.5,
        },
        Kind::ExpressionValue {
            expression_id: 100007,
            note_id: 401,
            value: 0.625,
        },
        Kind::ExpressionText {
            expression_id: 100008,
            note_id: 401,
            text: &text,
        },
        Kind::Chord {
            root: 69,
            bass: 57,
            mask: 65535,
            text: &text,
        },
        Kind::Scale {
            root: 60,
            mask: 0xa5a5,
            text: &text,
        },
        Kind::ExpressionInteger {
            expression_id: 100009,
            note_id: 401,
            value: u64::MAX,
        },
    ];
    for (i, kind) in events.into_iter().enumerate() {
        storage
            .push(Event {
                header: header(i as u32),
                kind,
            })
            .unwrap();
    }
    assert_eq!(storage.len(), 9);
    assert_eq!(storage.records[8].integer_value, u64::MAX);
    binary.fill(0);
    drop(text);
    assert_eq!(&storage.bytes()[..5], &[0xff, 0, 0x80, 0xf0, 0xf7]);
    assert_eq!(&storage.bytes()[6..12], &[0x41, 0, 0x3d, 0xd8, 0, 0xde]);
    let mut buses = [0; 16];
    buses[1] = 2;
    assert!(storage.admitted(16, &buses));
    assert!(!storage.admitted(8, &buses));
    let before = storage.records.clone();
    let bytes = storage.bytes().to_vec();
    let used = storage.used;
    for kind in [
        Kind::NoteOn(Note {
            channel: 16,
            ..note()
        }),
        Kind::NoteOn(Note {
            note_id: -2,
            ..note()
        }),
        Kind::NoteOff(Note {
            velocity: f32::NAN,
            ..note()
        }),
        Kind::ExpressionValue {
            expression_id: 0,
            note_id: -1,
            value: 0.5,
        },
        Kind::ExpressionInteger {
            expression_id: u32::MAX,
            note_id: 1,
            value: 0,
        },
        Kind::ExpressionText {
            expression_id: 1,
            note_id: 1,
            text: "a\0b",
        },
    ] {
        assert_eq!(
            storage.push(Event {
                header: header(9),
                kind
            }),
            Err(Error::Shape)
        );
        assert_eq!(storage.records, before);
        assert_eq!(storage.bytes(), bytes);
        assert_eq!(storage.used, used);
    }
    let huge = "a".repeat(4097);
    assert_eq!(
        storage.push(Event {
            header: header(9),
            kind: Kind::Scale {
                root: 0,
                mask: 0,
                text: &huge
            }
        }),
        Err(Error::Shape)
    );
    assert_eq!(
        storage.push(Event {
            header: header(0),
            kind: Kind::NoteOn(note())
        }),
        Err(Error::Shape)
    );
    assert_eq!(storage.len(), 9);
    storage.clear();
    assert!(storage.is_empty() && storage.payload_bytes() == 0);
    let mut tiny = InputEvents::new(2, 5).unwrap();
    tiny.push(Event {
        header: header(0),
        kind: Kind::SysEx(&[1, 2, 3]),
    })
    .unwrap();
    let bytes = tiny.bytes().to_vec();
    assert_eq!(
        tiny.push(Event {
            header: header(0),
            kind: Kind::Scale {
                root: 0,
                mask: 0,
                text: "A"
            }
        }),
        Err(Error::Capacity)
    );
    assert_eq!(tiny.len(), 1);
    assert_eq!(tiny.bytes(), bytes);
}
#[test]
fn complete_context_independent_presence_and_exact_integer_domains() {
    let mut context = Context::new(48000., -9007199254740993);
    context.continuous_time_samples = 9007199254740993;
    context.state = Context::PLAYING | Context::CONTINUOUS;
    context.tempo = f64::NAN;
    assert!(context.admitted(48000.));
    context.state |= Context::TEMPO;
    assert!(!context.admitted(48000.));
    context.tempo = 123.25;
    context.state |= Context::CLOCK | Context::SMPTE;
    context.frames_per_second = 123;
    context.samples_to_next_clock = -777;
    context.smpte_offset_subframes = -999;
    assert!(context.admitted(48000.) && !context.admitted(44100.));
    context.frame_rate_flags = 4;
    assert!(!context.admitted(48000.));
    context.frame_rate_flags = 3;
    context.state |= Context::CHORD;
    context.chord_key = 255;
    context.chord_root = 255;
    context.chord_mask = 0xfff;
    assert!(context.admitted(48000.));
    context.chord_mask = 0x1000;
    assert!(!context.admitted(48000.));
    context.chord_mask = 0;
    context.state |= Context::CYCLE;
    context.cycle_start_music = 2.;
    context.cycle_end_music = 1.;
    assert!(!context.admitted(48000.));
    context.cycle_end_music = 3.;
    context.state |= Context::TIME_SIGNATURE;
    context.time_signature_numerator = 7;
    context.time_signature_denominator = 8;
    assert!(context.admitted(48000.));
}
#[test]
fn copied_midi_mapping_preserves_bus_channel_value_and_rejects_conflicts() {
    let catalogue = Catalogue {
        inputs: vec![],
        outputs: vec![],
        mapping: true,
        context_present: false,
        context_requirements: 0,
        assignments: vec![
            Mapping {
                bus: 3,
                channel: 4,
                controller: 7,
                id: 11,
            },
            Mapping {
                bus: 3,
                channel: 4,
                controller: 128,
                id: 12,
            },
            Mapping {
                bus: 3,
                channel: 4,
                controller: 129,
                id: 13,
            },
        ],
    };
    assert_eq!(
        catalogue.midi_point(3, 5, &[0xb4, 7, 64]).unwrap(),
        Point {
            id: 11,
            offset: 5,
            value: 64. / 127.
        }
    );
    assert_eq!(catalogue.midi_point(3, 6, &[0xd4, 127]).unwrap().value, 1.);
    assert_eq!(
        catalogue.midi_point(3, 7, &[0xe4, 0, 64]).unwrap().value,
        8192. / 16383.
    );
    assert_eq!(
        catalogue.midi_point(2, 5, &[0xb4, 7, 64]),
        Err(Error::Unassigned)
    );
    assert_eq!(
        catalogue.midi_point(3, 5, &[0xb5, 7, 64]),
        Err(Error::Unassigned)
    );
    for bytes in [
        &[0x94, 69, 64][..],
        &[0xc4, 1],
        &[0xf0, 1, 2],
        &[0xb4, 7],
        &[0xd4, 1, 2],
        &[0xb4, 7, 128],
    ] {
        assert_eq!(catalogue.midi_point(3, 0, bytes), Err(Error::Shape));
    }
    let mut points = InputPoints::new(128).unwrap();
    points.push_midi(&catalogue, 3, 5, &[0xb4, 7, 64]).unwrap();
    assert_eq!(
        points.push(Point {
            id: 11,
            offset: 5,
            value: 0.75
        }),
        Err(Error::Conflict)
    );
    assert_eq!(points.points().len(), 1);
    points
        .push(Point {
            id: 12,
            offset: 5,
            value: 0.75,
        })
        .unwrap();
    assert_eq!(
        points.push(Point {
            id: 13,
            offset: 4,
            value: 0.75
        }),
        Err(Error::Shape)
    );
    points.clear();
    for id in 0..64 {
        points
            .push(Point {
                id,
                offset: 0,
                value: 0.5,
            })
            .unwrap();
    }
    assert_eq!(
        points.push(Point {
            id: 64,
            offset: 0,
            value: 0.5
        }),
        Err(Error::Capacity)
    );
    assert_eq!(points.points().len(), 64);
}
/// Fixed-size by-value copy of everything `process` received. It is plain stack
/// data so recording it never allocates, including inside measured callbacks.
const CAPTURED_EVENTS: usize = 16;
const CAPTURED_BYTES: usize = 128;
const CAPTURED_POINTS: usize = 8;
/// Channel descriptor as (sample address, capacity, reserved).
type Descriptor = (usize, u32, u32);
#[derive(Clone, Copy)]
struct Capture {
    /// struct_size, frames, input_channels, output_channels.
    block: [u32; 4],
    inputs_ptr: usize,
    inputs: [Descriptor; 2],
    outputs_ptr: usize,
    outputs: [Descriptor; 2],
    points_ptr: usize,
    point_count: u32,
    points: [Point; CAPTURED_POINTS],
    events_ptr: usize,
    event_count: u32,
    events: [RawEvent; CAPTURED_EVENTS],
    payload_ptr: usize,
    payload_bytes: u32,
    payload: [u8; CAPTURED_BYTES],
    context_ptr: usize,
    context: Option<Context>,
    /// struct_size, reserved, event_capacity, event_count, payload_capacity,
    /// payload_used, queue_capacity, queue_count, point_capacity, point_count.
    wire: [u32; 10],
    report: RawReport,
}
impl Capture {
    fn new() -> Self {
        Self {
            block: [0; 4],
            inputs_ptr: 0,
            inputs: [(0, 0, 0); 2],
            outputs_ptr: 0,
            outputs: [(0, 0, 0); 2],
            points_ptr: 0,
            point_count: 0,
            points: [Point {
                id: 0,
                offset: 0,
                value: 0.,
            }; CAPTURED_POINTS],
            events_ptr: 0,
            event_count: 0,
            events: [RawEvent::default(); CAPTURED_EVENTS],
            payload_ptr: 0,
            payload_bytes: 0,
            payload: [0; CAPTURED_BYTES],
            context_ptr: 0,
            context: None,
            wire: [0; 10],
            report: RawReport::initial(),
        }
    }
}
impl Default for Capture {
    fn default() -> Self {
        Self::new()
    }
}
/// Scripted output writer. It may change the wire and the report it is given.
type Reply = fn(&mut RawOutputs, &mut RawReport);
#[derive(Default)]
struct State {
    calls: Cell<u32>,
    mode: Cell<u32>,
    capture: RefCell<Capture>,
    /// Scripted reply: base writer, then optional single defect, then `status`.
    base: Cell<Option<Reply>>,
    defect: Cell<Option<Reply>>,
    /// Replaces the whole report and skips the base writer when set.
    report: Cell<Option<RawReport>>,
    status: Cell<i32>,
}
/// # Safety
/// Each pointer is exactly what the safe wrapper passed: a readable range for its
/// count (dangling is acceptable only for an empty range) or null where Rust
/// passes null. All of them are live for the duration of the native call.
#[allow(clippy::too_many_arguments)]
unsafe fn record_arguments(
    state: &State,
    block: &Block,
    points: *const Point,
    point_count: u32,
    events: *const RawEvent,
    event_count: u32,
    payload: *const u8,
    payload_bytes: u32,
    context: *const Context,
    outputs: &RawOutputs,
    report: &RawReport,
) {
    let mut c = Capture::new();
    c.block = [
        block.struct_size,
        block.frames,
        block.input_channels,
        block.output_channels,
    ];
    c.inputs_ptr = block.inputs as usize;
    if !block.inputs.is_null() {
        for i in 0..(block.input_channels as usize).min(2) {
            // SAFETY: the descriptor array has input_channels initialized entries.
            let d = unsafe { &*block.inputs.add(i) };
            c.inputs[i] = (d.samples as usize, d.capacity, d.reserved);
        }
    }
    c.outputs_ptr = block.outputs as usize;
    if !block.outputs.is_null() {
        for i in 0..(block.output_channels as usize).min(2) {
            // SAFETY: the descriptor array has output_channels initialized entries.
            let d = unsafe { &*block.outputs.add(i) };
            c.outputs[i] = (d.samples as usize, d.capacity, d.reserved);
        }
    }
    c.points_ptr = points as usize;
    c.point_count = point_count;
    let n = (point_count as usize).min(CAPTURED_POINTS);
    if n > 0 {
        // SAFETY: the caller passed `point_count` initialized points.
        c.points[..n].copy_from_slice(unsafe { std::slice::from_raw_parts(points, n) });
    }
    c.events_ptr = events as usize;
    c.event_count = event_count;
    let n = (event_count as usize).min(CAPTURED_EVENTS);
    if n > 0 {
        // SAFETY: the caller passed `event_count` initialized records.
        c.events[..n].copy_from_slice(unsafe { std::slice::from_raw_parts(events, n) });
    }
    c.payload_ptr = payload as usize;
    c.payload_bytes = payload_bytes;
    let n = (payload_bytes as usize).min(CAPTURED_BYTES);
    if n > 0 {
        // SAFETY: the caller passed `payload_bytes` initialized bytes.
        c.payload[..n].copy_from_slice(unsafe { std::slice::from_raw_parts(payload, n) });
    }
    c.context_ptr = context as usize;
    if !context.is_null() {
        // SAFETY: non-null context points at one initialized record.
        c.context = Some(unsafe { *context });
    }
    c.wire = [
        outputs.struct_size,
        outputs.reserved,
        outputs.event_capacity,
        outputs.event_count,
        outputs.payload_capacity,
        outputs.payload_used,
        outputs.parameter_queue_capacity,
        outputs.parameter_queue_count,
        outputs.parameter_point_capacity,
        outputs.parameter_point_count,
    ];
    c.report = *report;
    *state.capture.borrow_mut() = c;
}
unsafe extern "C" fn mock_process(
    handle: *mut c_void,
    block: *const Block,
    points: *const Point,
    point_count: u32,
    events: *const RawEvent,
    event_count: u32,
    payload: *const u8,
    payload_bytes: u32,
    context: *const Context,
    outputs: *mut RawOutputs,
    report: *mut RawReport,
) -> i32 {
    // SAFETY: each test owns initialized state/block/outputs/report for this call;
    // callbacks deliberately vary wire replies without invalid memory accesses.
    let (state, block, output) = unsafe { (&*handle.cast::<State>(), &*block, &mut *outputs) };
    state.calls.set(state.calls.get() + 1);
    // SAFETY: arguments are forwarded unchanged from this callback's own call.
    unsafe {
        record_arguments(
            state,
            block,
            points,
            point_count,
            events,
            event_count,
            payload,
            payload_bytes,
            context,
            output,
            &*report,
        );
    }
    if state.mode.get() == 12 {
        // Valid local native refusal before DSP or output mutation.
        unsafe {
            *report = RawReport::initial();
        }
        return 2;
    }
    if let Some(raw) = state.report.get() {
        // SAFETY: caller-owned initialized report record.
        unsafe {
            *report = raw;
        }
        return state.status.get();
    }
    if let Some(base) = state.base.get() {
        let mut r = RawReport {
            stage: 3,
            flags: 51,
            ..RawReport::initial()
        };
        base(output, &mut r);
        if let Some(defect) = state.defect.get() {
            defect(output, &mut r);
        }
        // SAFETY: caller-owned initialized report record.
        unsafe {
            *report = r;
        }
        return state.status.get();
    }
    let mut r = RawReport {
        stage: 3,
        flags: 51,
        ..RawReport::initial()
    };
    if state.mode.get() == 1 || state.mode.get() == 8 {
        r.flags = 27;
        r.failure = if state.mode.get() == 8 { 3 } else { 1 };
        r.native_result = if state.mode.get() == 8 { 0 } else { 4242 };
        r.input_parameter_failure = u32::from(state.mode.get() == 8);
        // SAFETY: caller preflights exact writable channel prefixes.
        for ch in 0..block.output_channels as usize {
            let c = unsafe { &*block.outputs.add(ch) };
            unsafe { std::slice::from_raw_parts_mut(c.samples, block.frames as usize) }.fill(0.);
        }
        unsafe {
            *report = r;
        }
        return 4;
    }
    let event = RawEvent {
        struct_size: size_of::<RawEvent>() as u32,
        kind: 0,
        bus: 0,
        offset: if block.frames == 0 {
            0
        } else {
            block.frames - 1
        },
        channel: 1,
        pitch: 69,
        note_id: -777,
        velocity: 0.5,
        ..RawEvent::default()
    };
    if output.event_capacity > 0 {
        unsafe {
            *output.events = event;
        }
        output.event_count = 1;
    }
    if state.mode.get() == 2 {
        output.event_count = output.event_capacity + 1;
    }
    if state.mode.get() == 3 && output.payload_capacity >= 4 {
        unsafe {
            (*output.events).kind = 5;
            (*output.events).note_id = -1;
            (*output.events).payload_bytes = 2;
            std::ptr::copy_nonoverlapping([0u8, 0xd8, 0, 0].as_ptr(), output.payload, 4);
        }
        output.payload_used = 4;
    }
    if state.mode.get() == 5 {
        output.events = std::ptr::null_mut();
    }
    if state.mode.get() == 6 {
        r.flags = 19;
    }
    if state.mode.get() == 9 {
        r.native_result = 4242;
    }
    if state.mode.get() == 10 {
        r.flags = 0;
    }
    if state.mode.get() == 11 {
        r.flags = 64;
    }
    r.event_count = output.event_count;
    r.payload_used = output.payload_used;
    r.parameter_queue_count = output.parameter_queue_count;
    r.parameter_point_count = output.parameter_point_count;
    unsafe {
        *report = r;
    }
    0
}
unsafe extern "C" fn mock_refresh(
    _: *mut c_void,
    _: *const RawSelection,
    _: *mut RawInfo,
    _: *mut RawReport,
) -> i32 {
    10
}
unsafe extern "C" fn mock_bus(_: *const c_void, _: u32, _: u32, _: *mut RawBus) -> i32 {
    10
}
unsafe extern "C" fn mock_assignment(_: *const c_void, _: u32, _: u32, _: u32, _: *mut u32) -> i32 {
    10
}
unsafe extern "C" fn mock_prepare(_: *mut c_void, _: *const Profile, _: *mut RawReport) -> i32 {
    10
}
fn instance_raw() -> super::super::RawProvider {
    super::super::RawProvider {
        struct_size: size_of::<super::super::RawProvider>() as u32,
        version: 2,
        open: None,
        factory: None,
        class_info: None,
        close: None,
        create: None,
        prepare: None,
        start: None,
        begin_audio: None,
        process: None,
        end_audio: None,
        stop: None,
        destroy: None,
        restart_requests: None,
    }
}
fn processing_raw(native: &super::super::RawProvider) -> RawProvider {
    RawProvider {
        struct_size: size_of::<RawProvider>() as u32,
        version: 1,
        provider: native,
        refresh: Some(mock_refresh),
        bus_info: Some(mock_bus),
        midi_assignment: Some(mock_assignment),
        prepare: Some(mock_prepare),
        process: Some(mock_process),
    }
}
fn audio<'a>(
    native: &'a super::super::RawProvider,
    processing: &'a RawProvider,
    state: &State,
) -> Audio<'a, 'a> {
    audio_with_profile(native, processing, state, Profile::new(48000., 16, 0, 2))
}
fn audio_with_profile<'a>(
    native: &'a super::super::RawProvider,
    processing: &'a RawProvider,
    state: &State,
    profile: Profile,
) -> Audio<'a, 'a> {
    let mut buses = EventBuses::default();
    buses.input[1] = 2;
    buses.output[0] = 2;
    Audio {
        raw: native,
        handle: NonNull::from(state).cast(),
        profile,
        parameters: None,
        processing: Some(processing),
        event_buses: buses,
        hosted: Default::default(),
        scope: PhantomData,
        no_sync: PhantomData,
    }
}
#[test]
fn nullable_or_wrong_version_companion_rejects_before_call() {
    let native = instance_raw();
    let mut raw = processing_raw(&native);
    // SAFETY: readable local tables; constructor only checks complete structure.
    assert!(unsafe { Provider::from_raw(&raw) }.is_ok());
    raw.process = None;
    assert!(matches!(
        unsafe { Provider::from_raw(&raw) },
        Err(Error::Abi)
    ));
    raw.process = Some(mock_process);
    raw.version = 2;
    assert!(matches!(
        unsafe { Provider::from_raw(&raw) },
        Err(Error::Abi)
    ));
    raw.version = 1;
    raw.provider = std::ptr::null();
    assert!(matches!(
        unsafe { Provider::from_raw(&raw) },
        Err(Error::Abi)
    ));
}
#[test]
fn safe_processing_preflight_zero_and_signed_output_with_tails() {
    let native = instance_raw();
    let raw = processing_raw(&native);
    let state = State::default();
    let mut audio = audio(&native, &raw, &state);
    let mut input = InputEvents::new(2, 16).unwrap();
    let mut storage = OutputStorage::new(2, 16, 2, 4).unwrap();
    let mut left = [9.; 20];
    let mut right = [8.; 20];
    {
        let output = audio
            .process_events(
                &[],
                &mut [&mut left, &mut right],
                0,
                Input {
                    points: &[],
                    events: &input,
                    context: None,
                },
                &mut storage,
            )
            .unwrap();
        let e = output.events().next().unwrap();
        assert_eq!(e.record().note_id, -777);
        assert_eq!(e.record().offset, 0);
        assert!(output.report().published());
    }
    input
        .push(Event {
            header: header(8),
            kind: Kind::NoteOn(note()),
        })
        .unwrap();
    assert!(matches!(
        audio.process_events(
            &[],
            &mut [&mut left, &mut right],
            8,
            Input {
                points: &[],
                events: &input,
                context: None
            },
            &mut storage
        ),
        Err(Error::Shape)
    ));
    let context = Context::new(44100., 0);
    assert!(matches!(
        audio.process_events(
            &[],
            &mut [&mut left, &mut right],
            16,
            Input {
                points: &[],
                events: &input,
                context: Some(&context)
            },
            &mut storage
        ),
        Err(Error::Shape)
    ));
    let invalid = Point {
        id: u32::MAX,
        offset: 0,
        value: 0.5,
    };
    assert!(matches!(
        audio.process_events(
            &[],
            &mut [&mut left, &mut right],
            16,
            Input {
                points: &[invalid],
                events: &input,
                context: None
            },
            &mut storage
        ),
        Err(Error::Shape)
    ));
    assert_eq!(state.calls.get(), 1);
    assert_eq!(left, [9.; 20]);
    assert_eq!(right, [8.; 20]);
    let output = audio
        .process_events(
            &[],
            &mut [&mut left, &mut right],
            16,
            Input {
                points: &[],
                events: &input,
                context: None,
            },
            &mut storage,
        )
        .unwrap();
    assert_eq!(output.events().next().unwrap().record().offset, 15);
    assert_eq!(left[16], 9.);
    assert_eq!(right[16], 8.);
}
#[test]
fn native_failure_and_malformed_success_never_expose_unverified_views() {
    for mode in [1, 2, 3, 5, 6, 8, 9, 10, 11] {
        let native = instance_raw();
        let raw = processing_raw(&native);
        let state = State::default();
        state.mode.set(mode);
        let mut audio = audio(&native, &raw, &state);
        let input = InputEvents::new(0, 0).unwrap();
        let mut storage = OutputStorage::new(2, 16, 2, 4).unwrap();
        let mut left = [9.; 20];
        let mut right = [8.; 20];
        let result = audio.process_events(
            &[],
            &mut [&mut left, &mut right],
            16,
            Input {
                points: &[],
                events: &input,
                context: None,
            },
            &mut storage,
        );
        match result {
            Err(Error::Provider { status: 4, report }) if mode == 1 || mode == 8 => {
                assert!(report.invalidated() && report.history_may_have_advanced());
                assert_eq!(
                    report.returned_status(),
                    Some(if mode == 1 { 4242 } else { 0 })
                );
                assert_eq!(report.input_parameter_failure(), u32::from(mode == 8));
                assert_eq!(left[..16], [0.; 16]);
                assert_eq!(left[16], 9.);
            }
            Err(Error::Abi) if mode != 1 && mode != 8 => (),
            _ => panic!("unexpected mode{mode}"),
        }
        assert!(matches!(
            audio.process_events(
                &[],
                &mut [&mut left, &mut right],
                0,
                Input {
                    points: &[],
                    events: &input,
                    context: None
                },
                &mut storage
            ),
            Err(Error::Lifecycle)
        ));
        assert_eq!(state.calls.get(), 1);
    }
}
#[test]
fn ten_thousand_calls_and_prepared_payload_conversion_allocate_nothing() {
    let native = instance_raw();
    let raw = processing_raw(&native);
    let state = State::default();
    let mut audio = audio(&native, &raw, &state);
    let mut input = InputEvents::new(2, 32).unwrap();
    let mut storage = OutputStorage::new(2, 32, 2, 4).unwrap();
    let mut left = [9.; 20];
    let mut right = [8.; 20];
    let measured = allocation_counter::measure(|| {
        for i in 0..10000 {
            input.clear();
            input
                .push(Event {
                    header: header(0),
                    kind: Kind::ExpressionText {
                        expression_id: 1,
                        note_id: 1,
                        text: "A😀",
                    },
                })
                .unwrap();
            let output = audio
                .process_events(
                    &[],
                    &mut [&mut left, &mut right],
                    i % 17,
                    Input {
                        points: &[],
                        events: &input,
                        context: None,
                    },
                    &mut storage,
                )
                .unwrap();
            assert_eq!(output.events().count(), 1);
            assert!(output.report().published());
        }
    });
    assert_eq!(measured.count_total, 0);
    assert_eq!(measured.count_current, 0);
    assert_eq!(state.calls.get(), 10000);
    assert_eq!(left[16], 9.);
}

#[test]
fn valid_native_pre_dispatch_refusal_preserves_prepared_capability_for_retry() {
    let native = instance_raw();
    let raw = processing_raw(&native);
    let state = State::default();
    state.mode.set(12);
    let mut audio = audio(&native, &raw, &state);
    let input = InputEvents::new(0, 0).unwrap();
    let mut storage = OutputStorage::new(2, 16, 2, 4).unwrap();
    let mut left = [9.; 20];
    let mut right = [8.; 20];
    match audio.process_events(
        &[],
        &mut [&mut left, &mut right],
        16,
        Input {
            points: &[],
            events: &input,
            context: None,
        },
        &mut storage,
    ) {
        Err(Error::Provider { status: 2, report }) => {
            assert_eq!(report.returned_status(), None);
            assert!(
                !report.threw() && !report.invalidated() && !report.history_may_have_advanced()
            );
        }
        _ => panic!("pre-dispatch refusal lost"),
    }
    assert!(audio.processing.is_some());
    assert_eq!(left, [9.; 20]);
    assert_eq!(right, [8.; 20]);
    state.mode.set(0);
    let output = audio
        .process_events(
            &[],
            &mut [&mut left, &mut right],
            16,
            Input {
                points: &[],
                events: &input,
                context: None,
            },
            &mut storage,
        )
        .unwrap();
    assert!(output.report().published());
    assert_eq!(state.calls.get(), 2);
}

#[derive(Default)]
struct ControlState {
    mode: Cell<u32>,
    refreshes: Cell<u32>,
    bus_reads: Cell<u32>,
    assignments: Cell<u32>,
    prepares: Cell<u32>,
    releases: Cell<u32>,
}
unsafe extern "C" fn control_release(handle: *mut c_void, result: *mut i32) -> i32 {
    // SAFETY: CONTROL fixtures borrow an initialized stack state for the entire
    // mock owner lifetime. There is no foreign allocation to release.
    let state = unsafe { &*handle.cast::<ControlState>() };
    state.releases.set(state.releases.get() + 1);
    unsafe {
        *result = 0;
    }
    0
}
unsafe extern "C" fn control_refresh(
    handle: *mut c_void,
    selection: *const RawSelection,
    info: *mut RawInfo,
    report: *mut RawReport,
) -> i32 {
    // SAFETY: safe wrapper supplies initialized, disjoint records and the fixture
    // handle identifies a live ControlState. Reply mutations stay within them.
    let (state, selection, info) =
        unsafe { (&*handle.cast::<ControlState>(), &*selection, &mut *info) };
    state.refreshes.set(state.refreshes.get() + 1);
    if state.mode.get() == 1 {
        return 10;
    }
    info.input_bus_count = 2;
    info.output_bus_count = 2;
    info.input_mask = selection.input_mask;
    info.output_mask = selection.output_mask;
    info.flags = 3;
    info.context_requirements = 1025;
    if state.mode.get() == 4 {
        info.flags = 1;
    }
    // SAFETY: successful native CONTROL refresh writes the exact stage to its
    // initialized caller-owned report, independently of copied metadata.
    unsafe {
        (*report).stage = 1;
        if state.mode.get() == 7 {
            (*report).stage = 3;
            (*report).flags = 51;
        }
    }
    0
}
unsafe extern "C" fn control_bus(
    handle: *const c_void,
    output: u32,
    index: u32,
    bus: *mut RawBus,
) -> i32 {
    // SAFETY: wrapper supplies one initialized record. The fixture exercises a
    // malformed late record without writing outside that record.
    let (state, bus) = unsafe { (&*handle.cast::<ControlState>(), &mut *bus) };
    state.bus_reads.set(state.bus_reads.get() + 1);
    bus.index = index;
    bus.channels = index + 1;
    bus.bus_type = u32::from(index == 1);
    bus.flags = 1;
    bus.name[..6].copy_from_slice("A😀\0".as_bytes());
    if state.mode.get() == 2 && output == 1 && index == 1 {
        bus.name[0] = 0xff;
    }
    0
}
unsafe extern "C" fn control_assignment(
    handle: *const c_void,
    bus: u32,
    channel: u32,
    controller: u32,
    id: *mut u32,
) -> i32 {
    // SAFETY: initialized scalar output and borrowed fixture state.
    let state = unsafe { &*handle.cast::<ControlState>() };
    state.assignments.set(state.assignments.get() + 1);
    assert_eq!(bus, 1); // Only the explicitly selected input bus is inspected.
    assert!(channel < 2);
    if state.mode.get() == 3 && channel == 1 && controller == 129 {
        return 10;
    }
    unsafe {
        *id = if channel == 1 && matches!(controller, 7 | 128 | 129) {
            401 + controller
        } else {
            u32::MAX
        };
    }
    0
}
unsafe extern "C" fn control_prepare(
    handle: *mut c_void,
    _: *const Profile,
    report: *mut RawReport,
) -> i32 {
    // SAFETY: wrapper supplies exact profile/report and live fixture state.
    let state = unsafe { &*handle.cast::<ControlState>() };
    state.prepares.set(state.prepares.get() + 1);
    if state.mode.get() == 6 {
        unsafe {
            *report = RawReport {
                stage: 108,
                flags: 3,
                failure: 1,
                native_result: 4242,
                ..RawReport::initial()
            };
        }
        return 4;
    }
    // SAFETY: caller owns the live initialized CONTROL report; no AUDIO facts.
    unsafe {
        (*report).stage = 2;
        if state.mode.get() == 8 {
            (*report).stage = 3;
            (*report).flags = 51;
        }
    }
    0
}
fn control_raw(native: &super::super::RawProvider) -> RawProvider {
    RawProvider {
        refresh: Some(control_refresh),
        bus_info: Some(control_bus),
        midi_assignment: Some(control_assignment),
        prepare: Some(control_prepare),
        ..processing_raw(native)
    }
}
fn control_library<'a>(
    native: &'a super::super::RawProvider,
    state: &ControlState,
) -> super::super::Library<'a> {
    // Other constructor/lifecycle contracts are covered by instance_tests. This
    // CONTROL fixture uses a borrowed state handle with balanced no-free releases.
    super::super::Library {
        provider: super::super::Provider {
            raw: native,
            owner: PhantomData,
        },
        handle: Some(NonNull::from(state).cast()),
        classes: Vec::new(),
        factory: None,
    }
}
fn control_child<'m, 'a>(
    library: &'m super::super::Library<'a>,
    state: &ControlState,
) -> Instance<'m, 'a> {
    Instance {
        library: crate::vst3::instances::Parent::Borrowed(library),
        handle: Some(NonNull::from(state).cast()),
        profile: None,
        running: false,
        poisoned: false,
        state_fault: false,
        parameter_provider: None,
        processing_provider: None,
        processing_metadata: None,
        hosted: Default::default(),
        class_id: [1; 16],
    }
}
#[test]
fn stopped_refresh_copies_selection_and_optional_interfaces_before_atomic_metadata_publication() {
    let mut native = instance_raw();
    native.close = Some(control_release);
    native.destroy = Some(control_release);
    let raw = control_raw(&native);
    // SAFETY: complete authored companion callbacks and exact live paired table.
    let provider = unsafe { Provider::from_raw(&raw).unwrap() };
    let state = ControlState::default();
    {
        let library = control_library(&native, &state);
        {
            let mut child = control_child(&library, &state);
            let catalogue = child.refresh_processing(provider, 2, 1).unwrap();
            assert_eq!(state.refreshes.get(), 1);
            assert_eq!(state.bus_reads.get(), 4);
            assert_eq!(state.assignments.get(), 260);
            assert_eq!(catalogue.inputs()[0].name, "A😀");
            assert!(!catalogue.inputs()[0].selected);
            assert!(catalogue.inputs()[1].selected && catalogue.inputs()[1].auxiliary);
            assert!(catalogue.outputs()[0].selected);
            assert!(!catalogue.outputs()[1].selected);
            assert_eq!(catalogue.context_requirements(), Some(1025));
            assert!(catalogue.has_midi_mapping());
            let point = catalogue.midi_point(1, 9, &[0xb1, 7, 64]).unwrap();
            assert_eq!((point.id, point.offset, point.value), (408, 9, 64. / 127.));
            let buses = child.processing_metadata.unwrap().1;
            assert_eq!(
                buses.input,
                [0, 2, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0]
            );
            assert_eq!(buses.output[0], 1);
            for mode in [1, 2, 3, 4] {
                state.mode.set(mode);
                assert!(child.refresh_processing(provider, 2, 1).is_err());
                assert!(child.processing_metadata.is_none());
                assert!(child.processing_provider.is_none());
                // An earlier owned catalogue never borrows the native/current
                // metadata, including after a late UTF8/mapping failure.
                assert_eq!(catalogue.inputs()[0].name, "A😀");
                assert_eq!(catalogue.midi_point(1, 9, &[0xb1, 7, 64]).unwrap(), point);
            }
            state.mode.set(0);
            child.refresh_processing(provider, 2, 1).unwrap();
            assert!(child.processing_metadata.is_some());
        }
        assert_eq!(state.releases.get(), 1);
    }
    assert_eq!(state.releases.get(), 2);
}
#[test]
fn processing_control_requires_exact_companion_stopped_owner_and_preserves_original_prepare_refusal()
 {
    let mut native = instance_raw();
    native.close = Some(control_release);
    native.destroy = Some(control_release);
    let raw = control_raw(&native);
    let other_raw = control_raw(&native);
    let wrong_native = instance_raw();
    let wrong_raw = control_raw(&wrong_native);
    // SAFETY: readable authored fixtures, exact local identities and lifetimes.
    let provider = unsafe { Provider::from_raw(&raw).unwrap() };
    let other = unsafe { Provider::from_raw(&other_raw).unwrap() };
    let wrong = unsafe { Provider::from_raw(&wrong_raw).unwrap() };
    let state = ControlState::default();
    let library = control_library(&native, &state);
    let mut child = control_child(&library, &state);
    let profile = Profile::new(48000., 16, 0, 2);
    assert_eq!(
        child.prepare_processing(provider, profile),
        Err(Error::Lifecycle)
    );
    assert_eq!(state.prepares.get(), 0);
    assert_eq!(child.refresh_processing(wrong, 2, 1), Err(Error::Abi));
    assert_eq!(state.refreshes.get(), 0);
    child.refresh_processing(provider, 2, 1).unwrap();
    assert_eq!(
        child.prepare_processing(other, profile),
        Err(Error::Lifecycle)
    );
    assert_eq!(
        child.prepare_processing(provider, Profile::new(f64::NAN, 16, 0, 2)),
        Err(Error::Shape)
    );
    assert_eq!(state.prepares.get(), 0);
    assert!(child.processing_metadata.is_some());
    child.running = true;
    assert_eq!(
        child.prepare_processing(provider, profile),
        Err(Error::Lifecycle)
    );
    assert_eq!(
        child.refresh_processing(provider, 2, 1),
        Err(Error::Lifecycle)
    );
    child.running = false;
    child.state_fault = true;
    assert_eq!(
        child.prepare_processing(provider, profile),
        Err(Error::Poisoned)
    );
    child.state_fault = false;
    child.poisoned = true;
    assert_eq!(
        child.refresh_processing(provider, 2, 1),
        Err(Error::Poisoned)
    );
    child.poisoned = false;
    assert_eq!(state.refreshes.get(), 1);
    child.prepare_processing(provider, profile).unwrap();
    assert!(std::ptr::eq(child.processing_provider.unwrap(), &raw));
    assert_eq!(child.profile.unwrap().maximum_frames, 16);
    state.mode.set(6);
    match child.prepare_processing(provider, profile) {
        Err(Error::Provider { status: 4, report }) => {
            assert_eq!(report.stage(), 108);
            assert_eq!(report.returned_status(), Some(4242));
            assert!(!report.threw() && !report.history_may_have_advanced());
        }
        _ => panic!("original refusal not preserved"),
    }
    assert!(child.profile.is_none() && child.processing_provider.is_none());
    assert!(child.processing_metadata.is_some());
    state.mode.set(0);
    child.prepare_processing(provider, profile).unwrap();
    assert_eq!(state.prepares.get(), 3);
}

// ---------------------------------------------------------------------------
// ABI layout, marshalling and output-admission discrimination tests.
// ---------------------------------------------------------------------------

fn unit_bytes(units: &[u16]) -> Vec<u8> {
    units.iter().flat_map(|u| u.to_ne_bytes()).collect()
}
fn put_units(buf: &mut [u8], start: usize, units: &[u16]) {
    for (i, unit) in units.iter().enumerate() {
        buf[start + 2 * i..start + 2 * i + 2].copy_from_slice(&unit.to_ne_bytes());
    }
}
fn pt(id: u32, offset: u32, value: f64) -> Point {
    Point { id, offset, value }
}

macro_rules! assert_offsets {
    ($ty:ty { $($field:ident: $offset:expr),+ $(,)? }) => {
        $(assert_eq!(
            std::mem::offset_of!($ty, $field),
            $offset,
            concat!(stringify!($ty), "::", stringify!($field))
        );)+
    };
}
// Literal numbers below were computed from the declaration order and natural C
// alignment of the provided RavetVst3*.h structs (8-byte pointers and doubles),
// not from the Rust field order. A Rust-side swap of same-typed neighbours moves
// an offset or a name and therefore fails here.
#[cfg(target_pointer_width = "64")]
#[test]
fn every_wire_field_offset_matches_the_literal_c_header_layout() {
    // RavetVst3EventRecord (`type` is Rust `kind`).
    assert_offsets!(RawEvent {
        struct_size: 0,
        kind: 4,
        flags: 6,
        bus: 8,
        offset: 12,
        ppq_position: 16,
        channel: 24,
        pitch: 28,
        note_id: 32,
        note_length: 36,
        tuning: 40,
        velocity: 44,
        expression_id: 48,
        data_type: 52,
        value: 56,
        integer_value: 64,
        root: 72,
        bass: 76,
        mask: 80,
        controller: 84,
        midi_value: 88,
        midi_value2: 92,
        payload_offset: 96,
        payload_bytes: 100,
        reserved: 104,
    });
    // RavetVst3ContextRecord.
    assert_offsets!(Context {
        struct_size: 0,
        state: 4,
        sample_rate: 8,
        project_time_samples: 16,
        system_time: 24,
        continuous_time_samples: 32,
        project_time_music: 40,
        bar_position_music: 48,
        cycle_start_music: 56,
        cycle_end_music: 64,
        tempo: 72,
        time_signature_numerator: 80,
        time_signature_denominator: 84,
        chord_key: 88,
        chord_root: 92,
        chord_mask: 96,
        smpte_offset_subframes: 100,
        frames_per_second: 104,
        frame_rate_flags: 108,
        samples_to_next_clock: 112,
        reserved: 116,
    });
    // RavetVst3ProcessingOutputs.
    assert_offsets!(RawOutputs {
        struct_size: 0,
        reserved: 4,
        events: 8,
        event_capacity: 16,
        event_count: 20,
        payload: 24,
        payload_capacity: 32,
        payload_used: 36,
        parameter_ids: 40,
        parameter_queue_capacity: 48,
        parameter_queue_count: 52,
        parameter_points: 56,
        parameter_point_capacity: 64,
        parameter_point_count: 68,
    });
    // RavetVst3ProcessingReport.
    assert_offsets!(RawReport {
        struct_size: 0,
        stage: 4,
        failure: 8,
        index: 12,
        flags: 16,
        input_parameter_failure: 20,
        output_event_failure: 24,
        reserved: 28,
        native_result: 32,
        event_count: 36,
        payload_used: 40,
        parameter_queue_count: 44,
        parameter_point_count: 48,
        reserved2: 52,
    });
    // RavetVst3ProcessingSelection and RavetVst3ProcessingInfo (the C names are
    // selected_input_mask / selected_output_mask).
    assert_offsets!(RawSelection {
        struct_size: 0,
        input_mask: 4,
        output_mask: 8,
        reserved: 12,
    });
    assert_offsets!(RawInfo {
        struct_size: 0,
        input_bus_count: 4,
        output_bus_count: 8,
        input_mask: 12,
        output_mask: 16,
        flags: 20,
        context_requirements: 24,
        reserved: 28,
    });
    // RavetVst3EventBusRecord.
    assert_offsets!(RawBus {
        struct_size: 0,
        index: 4,
        channels: 8,
        bus_type: 12,
        flags: 16,
        reserved: 20,
        name: 24,
    });
    // RavetVst3ProcessingProviderV1.
    assert_offsets!(RawProvider {
        struct_size: 0,
        version: 4,
        provider: 8,
        refresh: 16,
        bus_info: 24,
        midi_assignment: 32,
        prepare: 40,
        process: 48,
    });
    // RavetVst3AudioProfile, RavetVst3AudioBlock and channel descriptors, and
    // RavetVst3ParameterPoint.
    assert_eq!(size_of::<Profile>(), 24);
    assert_offsets!(Profile {
        struct_size: 0,
        maximum_frames: 4,
        input_channels: 8,
        output_channels: 12,
        sample_rate: 16,
    });
    assert_eq!(size_of::<Block>(), 32);
    assert_offsets!(Block {
        struct_size: 0,
        frames: 4,
        input_channels: 8,
        output_channels: 12,
        inputs: 16,
        outputs: 24,
    });
    assert_eq!(size_of::<InputChannel>(), 16);
    assert_offsets!(InputChannel {
        samples: 0,
        capacity: 8,
        reserved: 12,
    });
    assert_eq!(size_of::<OutputChannel>(), 16);
    assert_offsets!(OutputChannel {
        samples: 0,
        capacity: 8,
        reserved: 12,
    });
    assert_eq!(size_of::<Point>(), 16);
    assert_offsets!(Point {
        id: 0,
        offset: 4,
        value: 8,
    });
}

// ---- F1: every argument Rust passes to native `process` ---------------------

/// Nine kinds, two opaque SysEx blobs of odd length and four text events, pushed
/// into a payload that was previously dirty so alignment pad bytes must be
/// written explicitly rather than inherited.
fn rich_input() -> InputEvents {
    let mut input = InputEvents::new(16, 128).unwrap();
    input
        .push(Event {
            header: header(0),
            kind: Kind::SysEx(&[0xaa; 128]),
        })
        .unwrap();
    input.clear();
    let kinds = [
        Kind::NoteOn(note()),
        Kind::NoteOff(Note {
            note_id: -1,
            ..note()
        }),
        Kind::SysEx(&[0xff, 0, 0x80, 0xf0, 0xf7]),
        Kind::ExpressionText {
            expression_id: 100008,
            note_id: 401,
            text: "A😀",
        },
        Kind::PolyPressure {
            channel: 1,
            pitch: 71,
            note_id: 401,
            pressure: 0.5,
        },
        Kind::SysEx(&[0x80, 0xff, 0x00]),
        Kind::Chord {
            root: 69,
            bass: 57,
            mask: 65535,
            text: "😀B",
        },
        Kind::Scale {
            root: 60,
            mask: 0xa5a5,
            text: "é",
        },
        Kind::ExpressionValue {
            expression_id: 100007,
            note_id: 401,
            value: 0.625,
        },
        Kind::ExpressionInteger {
            expression_id: 100009,
            note_id: 401,
            value: 0x8000_0000_0000_0001,
        },
    ];
    for (i, kind) in kinds.into_iter().enumerate() {
        input
            .push(Event {
                header: header(i as u32),
                kind,
            })
            .unwrap();
    }
    input
}
fn input_record(kind: u16, offset: u32) -> RawEvent {
    RawEvent {
        struct_size: 112,
        kind,
        flags: 0xc001,
        bus: 1,
        offset,
        ppq_position: -0.125,
        ..RawEvent::default()
    }
}
/// Independently written wire records for `rich_input`.
fn rich_input_records() -> [RawEvent; 10] {
    [
        RawEvent {
            channel: 1,
            pitch: 69,
            note_id: 401,
            note_length: 100,
            tuning: -3.25,
            velocity: 0.75,
            ..input_record(0, 0)
        },
        RawEvent {
            channel: 1,
            pitch: 69,
            note_id: -1,
            note_length: 100,
            tuning: -3.25,
            velocity: 0.75,
            ..input_record(1, 1)
        },
        RawEvent {
            data_type: 0,
            payload_offset: 0,
            payload_bytes: 5,
            ..input_record(2, 2)
        },
        RawEvent {
            expression_id: 100008,
            note_id: 401,
            payload_offset: 6,
            payload_bytes: 6,
            ..input_record(5, 3)
        },
        RawEvent {
            channel: 1,
            pitch: 71,
            note_id: 401,
            velocity: 0.5,
            payload_offset: 12,
            ..input_record(3, 4)
        },
        RawEvent {
            data_type: 0,
            payload_offset: 12,
            payload_bytes: 3,
            ..input_record(2, 5)
        },
        RawEvent {
            root: 69,
            bass: 57,
            mask: 65535,
            payload_offset: 16,
            payload_bytes: 6,
            ..input_record(6, 6)
        },
        RawEvent {
            root: 60,
            mask: 0xa5a5,
            payload_offset: 22,
            payload_bytes: 2,
            ..input_record(7, 7)
        },
        RawEvent {
            expression_id: 100007,
            note_id: 401,
            value: 0.625,
            payload_offset: 24,
            ..input_record(4, 8)
        },
        RawEvent {
            expression_id: 100009,
            note_id: 401,
            integer_value: 0x8000_0000_0000_0001,
            payload_offset: 24,
            ..input_record(8, 9)
        },
    ]
}
/// Used payload bytes of `rich_input`: odd SysEx lengths (5 and 15 used) force
/// zero pad bytes at 5 and 15 before the aligned UTF-16 text at 6 and 16.
fn rich_input_payload() -> [u8; 24] {
    let mut b = [0; 24];
    b[..5].copy_from_slice(&[0xff, 0, 0x80, 0xf0, 0xf7]);
    put_units(&mut b, 6, &[0x0041, 0xd83d, 0xde00]);
    b[12..15].copy_from_slice(&[0x80, 0xff, 0x00]);
    put_units(&mut b, 16, &[0xd83d, 0xde00, 0x0042]);
    put_units(&mut b, 22, &[0x00e9]);
    b
}
fn input_points() -> [Point; 4] {
    [
        pt(7, 0, 0.125),
        pt(9, 3, 0.5),
        pt(7, 4, 0.875),
        pt(9, 15, 1.),
    ]
}
/// Every presence bit set and every field distinct, with signed 64-bit values
/// that an f64 round trip could not preserve.
fn full_context() -> Context {
    let mut c = Context::new(48000., -9007199254740993);
    c.state = Context::PLAYING
        | Context::CYCLE_ACTIVE
        | Context::RECORDING
        | Context::SYSTEM_TIME
        | Context::MUSIC
        | Context::TEMPO
        | Context::BAR
        | Context::CYCLE
        | Context::TIME_SIGNATURE
        | Context::SMPTE
        | Context::CLOCK
        | Context::CONTINUOUS
        | Context::CHORD;
    c.system_time = -9223372036854775807;
    c.continuous_time_samples = 9007199254740993;
    c.project_time_music = -1234.5;
    c.bar_position_music = 0.375;
    c.cycle_start_music = 1.5;
    c.cycle_end_music = 2.5;
    c.tempo = 133.25;
    c.time_signature_numerator = 7;
    c.time_signature_denominator = 8;
    c.chord_key = 255;
    c.chord_root = 254;
    c.chord_mask = 0xabc;
    c.smpte_offset_subframes = -999;
    c.frames_per_second = 30;
    c.frame_rate_flags = 3;
    c.samples_to_next_clock = -777;
    c
}
#[test]
fn native_process_receives_every_argument_by_value() {
    let context = full_context();
    for v in [
        context.project_time_samples,
        context.continuous_time_samples,
        context.system_time,
    ] {
        assert_ne!(v as f64 as i64, v, "fixture must exceed f64 exact range");
    }
    assert_eq!(context.project_time_samples, -9007199254740993);
    for (inputs, outputs) in [(0u32, 1u32), (0, 2), (1, 1), (2, 2)] {
        let native = instance_raw();
        let raw = processing_raw(&native);
        let state = State::default();
        let mut audio = audio_with_profile(
            &native,
            &raw,
            &state,
            Profile::new(48000., 16, inputs, outputs),
        );
        let input = rich_input();
        let points = input_points();
        let mut storage = OutputStorage::new(16, 64, 4, 16).unwrap();
        let in_a = [0.25f32; 24];
        let in_b = [0.5f32; 31];
        let mut left = [9.; 20];
        let mut right = [8.; 22];
        let (n_in, n_out) = (inputs as usize, outputs as usize);
        let left_ptr = left.as_ptr() as usize;
        let right_ptr = right.as_ptr() as usize;
        audio
            .process_events(
                &[&in_a[..], &in_b[..]][..n_in],
                &mut [&mut left[..], &mut right[..]][..n_out],
                16,
                Input {
                    points: &points,
                    events: &input,
                    context: Some(&context),
                },
                &mut storage,
            )
            .unwrap();
        let c = *state.capture.borrow();
        let label = format!("{inputs} in / {outputs} out");
        assert_eq!(c.block, [32, 16, inputs, outputs], "{label}");
        let mut want_in = [(0usize, 0u32, 0u32); 2];
        let all_in = [
            (in_a.as_ptr() as usize, 24, 0),
            (in_b.as_ptr() as usize, 31, 0),
        ];
        want_in[..n_in].copy_from_slice(&all_in[..n_in]);
        assert_eq!(c.inputs, want_in, "{label}");
        assert_eq!(c.inputs_ptr == 0, inputs == 0, "{label}");
        let mut want_out = [(0usize, 0u32, 0u32); 2];
        let all_out = [(left_ptr, 20, 0), (right_ptr, 22, 0)];
        want_out[..n_out].copy_from_slice(&all_out[..n_out]);
        assert_eq!(c.outputs, want_out, "{label}");
        assert_ne!(c.outputs_ptr, 0, "{label}");
        assert_eq!(c.point_count, 4, "{label}");
        assert_eq!(c.points_ptr, points.as_ptr() as usize, "{label}");
        assert_eq!(c.points[..4], input_points(), "{label}");
        assert_eq!(c.event_count, 10, "{label}");
        assert_eq!(c.events_ptr, input.records.as_ptr() as usize, "{label}");
        assert_eq!(c.events[..10], rich_input_records(), "{label}");
        assert_eq!(c.payload_bytes, 24, "{label}");
        assert_eq!(c.payload_ptr, input.bytes().as_ptr() as usize, "{label}");
        assert_eq!(c.payload[..24], rich_input_payload(), "{label}");
        assert_eq!(c.payload[5], 0, "{label}: pad before aligned text");
        assert_eq!(c.payload[15], 0, "{label}: pad before aligned text");
        assert_eq!(
            c.context_ptr,
            std::ptr::from_ref(&context) as usize,
            "{label}"
        );
        assert_eq!(c.context, Some(context), "{label}");
        let got = c.context.unwrap();
        assert_eq!(got.project_time_samples, -9007199254740993);
        assert_eq!(got.continuous_time_samples, 9007199254740993);
        assert_eq!(got.system_time, -9223372036854775807);
        assert_eq!(c.wire, [72, 0, 16, 0, 64, 0, 4, 0, 16, 0], "{label}");
        assert_eq!(
            c.report,
            RawReport {
                struct_size: 56,
                stage: 0,
                failure: 0,
                index: u32::MAX,
                flags: 0,
                input_parameter_failure: 0,
                output_event_failure: 0,
                reserved: 0,
                native_result: 0,
                event_count: 0,
                payload_used: 0,
                parameter_queue_count: 0,
                parameter_point_count: 0,
                reserved2: 0,
            },
            "{label}"
        );
        // Second call: nothing optional supplied, so every optional argument is
        // empty/null rather than a stale copy of the first call.
        let empty = InputEvents::new(0, 0).unwrap();
        let no_points: &[Point] = &[];
        audio
            .process_events(
                &[&in_a[..], &in_b[..]][..n_in],
                &mut [&mut left[..], &mut right[..]][..n_out],
                16,
                Input {
                    points: no_points,
                    events: &empty,
                    context: None,
                },
                &mut storage,
            )
            .unwrap();
        let c = *state.capture.borrow();
        assert_eq!(c.context_ptr, 0, "{label}");
        assert_eq!(c.context, None, "{label}");
        assert_eq!(c.point_count, 0, "{label}");
        assert_eq!(c.points_ptr, no_points.as_ptr() as usize, "{label}");
        assert_eq!(c.event_count, 0, "{label}");
        assert_eq!(c.events_ptr, empty.records.as_ptr() as usize, "{label}");
        assert_eq!(c.payload_bytes, 0, "{label}");
        assert_eq!(c.payload_ptr, empty.bytes().as_ptr() as usize, "{label}");
        assert_eq!(state.calls.get(), 2);
    }
}
#[test]
fn ten_thousand_calls_with_context_points_and_capture_allocate_nothing() {
    let native = instance_raw();
    let raw = processing_raw(&native);
    let state = State::default();
    let mut audio = audio(&native, &raw, &state);
    let mut input = InputEvents::new(2, 32).unwrap();
    let mut storage = OutputStorage::new(2, 32, 2, 4).unwrap();
    let mut left = [9.; 20];
    let mut right = [8.; 20];
    // Fixture setup allocates; the measured loop below must not.
    let context = full_context();
    let points = [pt(7, 0, 0.5)];
    let measured = allocation_counter::measure(|| {
        for i in 0..10000 {
            input.clear();
            input
                .push(Event {
                    header: header(0),
                    kind: Kind::ExpressionText {
                        expression_id: 1,
                        note_id: 1,
                        text: "A😀",
                    },
                })
                .unwrap();
            let output = audio
                .process_events(
                    &[],
                    &mut [&mut left, &mut right],
                    i % 17,
                    Input {
                        points: &points,
                        events: &input,
                        context: Some(&context),
                    },
                    &mut storage,
                )
                .unwrap();
            assert_eq!(output.events().count(), 1);
            assert!(output.report().published());
        }
    });
    assert_eq!(measured.count_total, 0);
    assert_eq!(measured.count_current, 0);
    assert_eq!(state.calls.get(), 10000);
    let c = *state.capture.borrow();
    assert_eq!(c.block[1], 9999 % 17);
    assert_eq!(c.context, Some(context));
    assert_eq!(c.points[..1], points);
    assert_eq!(c.event_count, 1);
    assert_eq!(c.payload_bytes, 6);
    assert_eq!(c.payload[..6], unit_bytes(&[0x0041, 0xd83d, 0xde00])[..]);
    assert_eq!(left[16], 9.);
}

// ---- F2/F7: scripted replies through the real `process_events` -------------

fn leave(_: &mut RawOutputs, _: &mut RawReport) {}
fn sync(o: &RawOutputs, r: &mut RawReport) {
    r.event_count = o.event_count;
    r.payload_used = o.payload_used;
    r.parameter_queue_count = o.parameter_queue_count;
    r.parameter_point_count = o.parameter_point_count;
}
fn out_events(o: &mut RawOutputs) -> &mut [RawEvent] {
    // SAFETY: scripts run before any defect edits the wire; pointers and
    // capacities come from the live OutputStorage the safe wrapper just built.
    unsafe { std::slice::from_raw_parts_mut(o.events, o.event_capacity as usize) }
}
fn out_payload(o: &mut RawOutputs) -> &mut [u8] {
    // SAFETY: as `out_events`; the byte capacity is the storage's initialized size.
    unsafe { std::slice::from_raw_parts_mut(o.payload, o.payload_capacity as usize) }
}
fn out_ids(o: &mut RawOutputs) -> &mut [u32] {
    // SAFETY: as `out_events`.
    unsafe { std::slice::from_raw_parts_mut(o.parameter_ids, o.parameter_queue_capacity as usize) }
}
fn out_points(o: &mut RawOutputs) -> &mut [Point] {
    // SAFETY: as `out_events`.
    unsafe {
        std::slice::from_raw_parts_mut(o.parameter_points, o.parameter_point_capacity as usize)
    }
}
fn ev(o: &mut RawOutputs, i: usize) -> &mut RawEvent {
    &mut out_events(o)[i]
}
fn output_record(kind: u16, offset: u32) -> RawEvent {
    RawEvent {
        struct_size: 112,
        kind,
        flags: if offset.is_multiple_of(2) { 0xc001 } else { 1 },
        bus: 0,
        offset,
        ppq_position: f64::from(offset) * 0.25,
        ..RawEvent::default()
    }
}
/// All ten supported output kinds. Text and SysEx payloads are laid out so the
/// Chord's UTF-16 follows an odd-length (5 byte) SysEx.
fn rich_output_events() -> [RawEvent; 10] {
    [
        RawEvent {
            channel: 1,
            pitch: 60,
            note_id: i32::MIN,
            note_length: 5,
            tuning: -1.5,
            velocity: 1.,
            ..output_record(0, 0)
        },
        RawEvent {
            channel: 0,
            pitch: 127,
            note_id: i32::MAX,
            tuning: 0.25,
            velocity: 0.,
            ..output_record(1, 1)
        },
        RawEvent {
            data_type: 0,
            payload_offset: 0,
            payload_bytes: 5,
            ..output_record(2, 2)
        },
        RawEvent {
            root: 69,
            bass: 57,
            mask: 65535,
            payload_offset: 6,
            payload_bytes: 6,
            ..output_record(6, 3)
        },
        RawEvent {
            expression_id: 8,
            note_id: -42,
            payload_offset: 14,
            payload_bytes: 4,
            ..output_record(5, 3)
        },
        RawEvent {
            root: 60,
            mask: 0xa5a5,
            payload_offset: 20,
            payload_bytes: 0,
            ..output_record(7, 5)
        },
        RawEvent {
            channel: 1,
            pitch: 0,
            note_id: -5,
            velocity: 0.25,
            ..output_record(3, 6)
        },
        RawEvent {
            expression_id: 7,
            note_id: -9,
            value: 0.125,
            ..output_record(4, 7)
        },
        RawEvent {
            expression_id: 9,
            note_id: -77,
            integer_value: 0x8000_0000_0000_0001,
            ..output_record(8, 15)
        },
        RawEvent {
            channel: 1,
            controller: 255,
            midi_value: 127,
            midi_value2: 0,
            ..output_record(65535, 15)
        },
    ]
}
const RICH_IDS: [u32; 3] = [41, 7, 2_000_000_000];
/// Queue 7 stays empty; 41 and 2_000_000_000 interleave and are not globally
/// sorted. Offsets only increase strictly within one ID.
fn rich_points() -> [Point; 6] {
    [
        pt(41, 3, 0.),
        pt(2_000_000_000, 1, 1.),
        pt(41, 8, 0.5),
        pt(2_000_000_000, 2, 0.25),
        pt(41, 15, 1.),
        pt(2_000_000_000, 15, 0.75),
    ]
}
fn rich_reply(o: &mut RawOutputs, r: &mut RawReport) {
    out_events(o)[..10].copy_from_slice(&rich_output_events());
    let payload = &mut out_payload(o)[..22];
    payload.fill(0);
    payload[..5].copy_from_slice(&[0xf0, 0x01, 0xff, 0x00, 0xf7]);
    put_units(payload, 6, &[0xd83d, 0xde00, 0x0042]);
    put_units(payload, 14, &[0x005a, 0x007a]);
    out_ids(o)[..3].copy_from_slice(&RICH_IDS);
    out_points(o)[..6].copy_from_slice(&rich_points());
    o.event_count = 10;
    o.payload_used = 22;
    o.parameter_queue_count = 3;
    o.parameter_point_count = 6;
    sync(o, r);
}
/// Two queues, the second empty, with no events or payload at all.
fn two_queue_reply(o: &mut RawOutputs, r: &mut RawReport) {
    out_ids(o)[..2].copy_from_slice(&[5, 6]);
    out_points(o)[0] = pt(5, 9, 0.5);
    o.event_count = 0;
    o.payload_used = 0;
    o.parameter_queue_count = 2;
    o.parameter_point_count = 1;
    sync(o, r);
}
/// Valid for a zero-frame block: every offset is exactly zero.
fn zero_frame_reply(o: &mut RawOutputs, r: &mut RawReport) {
    out_events(o)[0] = RawEvent {
        channel: 1,
        pitch: 60,
        note_id: -1,
        velocity: 0.5,
        ..output_record(0, 0)
    };
    out_ids(o)[0] = 3;
    out_points(o)[0] = pt(3, 0, 0.5);
    o.event_count = 1;
    o.payload_used = 0;
    o.parameter_queue_count = 1;
    o.parameter_point_count = 1;
    sync(o, r);
}
#[derive(Clone, Copy)]
struct Script {
    base: Reply,
    defect: Option<Reply>,
    report: Option<RawReport>,
    status: i32,
    frames: u32,
    /// Events, payload bytes, queues and points of the OutputStorage.
    shape: (usize, usize, usize, usize),
}
impl Script {
    fn new(base: Reply) -> Self {
        Self {
            base,
            defect: None,
            report: None,
            status: 0,
            frames: 16,
            shape: (16, 64, 4, 16),
        }
    }
}
struct Outcome {
    retained: bool,
    calls: u32,
    second_lifecycle: bool,
    second_published: bool,
}
/// Runs one scripted first call and hands its result to `check`, then reports
/// whether the local capability survived and what an immediate retry does. The
/// retry uses the default successful mock, so a retained capability must publish.
fn scripted(s: Script, check: impl FnOnce(Result<Output<'_>, Error>)) -> Outcome {
    let native = instance_raw();
    let raw = processing_raw(&native);
    let state = State::default();
    state.base.set(Some(s.base));
    state.defect.set(s.defect);
    state.report.set(s.report);
    state.status.set(s.status);
    let mut audio = audio(&native, &raw, &state);
    let input = InputEvents::new(0, 0).unwrap();
    let mut storage = OutputStorage::new(s.shape.0, s.shape.1, s.shape.2, s.shape.3).unwrap();
    let mut left = [9.; 20];
    let mut right = [8.; 20];
    check(audio.process_events(
        &[],
        &mut [&mut left, &mut right],
        s.frames,
        Input {
            points: &[],
            events: &input,
            context: None,
        },
        &mut storage,
    ));
    let retained = audio.processing.is_some();
    state.base.set(None);
    state.defect.set(None);
    state.report.set(None);
    state.status.set(0);
    let second = audio.process_events(
        &[],
        &mut [&mut left, &mut right],
        s.frames,
        Input {
            points: &[],
            events: &input,
            context: None,
        },
        &mut storage,
    );
    Outcome {
        retained,
        calls: state.calls.get(),
        second_lifecycle: matches!(second, Err(Error::Lifecycle)),
        second_published: matches!(&second, Ok(o) if o.report().published()),
    }
}
#[test]
fn accepted_output_covers_ten_kinds_counted_text_opaque_bytes_and_queues() {
    let outcome = scripted(Script::new(rich_reply), |result| {
        let output = result.expect("rich reply is admitted");
        assert!(output.report().published());
        assert_eq!(output.events().len(), 10);
        let events: Vec<_> = output.events().collect();
        let kinds: Vec<u16> = events.iter().map(|e| e.record().kind).collect();
        assert_eq!(kinds, [0, 1, 2, 6, 5, 7, 3, 4, 8, 65535]);
        for (i, (e, want)) in events.iter().zip(rich_output_events()).enumerate() {
            assert_eq!(*e.record(), want, "event {i}");
        }
        // Signed identities and 64-bit integers are carried untouched.
        assert_eq!(events[0].record().note_id, i32::MIN);
        assert_eq!(events[1].record().note_id, i32::MAX);
        assert_eq!(events[8].record().integer_value, 0x8000_0000_0000_0001);
        assert_eq!(events[9].record().controller, 255);
        assert_eq!(events[9].record().midi_value, 127);
        // Opaque counted bytes keep every value, including embedded zeros.
        assert_eq!(events[2].payload(), [0xf0, 0x01, 0xff, 0x00, 0xf7]);
        assert!(events[2].text().is_none());
        // Counted text follows the odd-length SysEx: byte 5 is padding.
        assert_eq!(events[3].payload(), unit_bytes(&[0xd83d, 0xde00, 0x0042]));
        assert_eq!(
            events[3].text().map(|t| t.collect::<String>()).as_deref(),
            Some("😀B")
        );
        assert_eq!(events[4].payload(), unit_bytes(&[0x005a, 0x007a]));
        assert_eq!(
            events[4].text().map(|t| t.collect::<String>()).as_deref(),
            Some("Zz")
        );
        // Zero-length text still decodes as present-but-empty.
        assert!(events[5].payload().is_empty());
        assert_eq!(
            events[5].text().map(|t| t.collect::<String>()).as_deref(),
            Some("")
        );
        for i in [0, 1, 6, 7, 8, 9] {
            assert!(events[i].payload().is_empty(), "event {i}");
            assert!(events[i].text().is_none(), "event {i}");
        }
        assert_eq!(output.parameter_ids(), RICH_IDS);
        assert_eq!(output.parameter_points(), rich_points());
    });
    assert!(outcome.retained && outcome.second_published);
    assert_eq!(outcome.calls, 2);
}
#[test]
fn accepted_output_with_two_queues_one_empty_and_no_events() {
    let script = Script {
        shape: (0, 0, 2, 1),
        ..Script::new(two_queue_reply)
    };
    let outcome = scripted(script, |result| {
        let output = result.expect("two-queue reply is admitted");
        assert_eq!(output.events().len(), 0);
        assert_eq!(output.parameter_ids(), [5, 6]);
        assert_eq!(output.parameter_points(), [pt(5, 9, 0.5)]);
    });
    assert!(outcome.retained && outcome.second_published);
}
struct Defect {
    name: &'static str,
    apply: Reply,
}
fn assert_abi_and_retired(name: &str, script: Script) {
    let outcome = scripted(script, |result| {
        assert_eq!(result.err(), Some(Error::Abi), "{name}");
    });
    assert!(!outcome.retained, "{name}: capability kept");
    assert_eq!(outcome.calls, 1, "{name}");
    assert!(outcome.second_lifecycle, "{name}: retry reached native");
}
#[test]
fn single_defect_output_replies_are_refused_and_retire_the_capability() {
    // Each row changes exactly one thing in the otherwise valid rich reply.
    let defects = [
        Defect {
            name: "wire struct_size",
            apply: |o, _| o.struct_size = 64,
        },
        Defect {
            name: "wire reserved",
            apply: |o, _| o.reserved = 1,
        },
        Defect {
            name: "events pointer moved",
            apply: |o, _| o.events = o.events.wrapping_add(1),
        },
        Defect {
            name: "events pointer null",
            apply: |o, _| o.events = std::ptr::null_mut(),
        },
        Defect {
            name: "payload pointer moved",
            apply: |o, _| o.payload = o.payload.wrapping_add(2),
        },
        Defect {
            name: "parameter_ids pointer moved",
            apply: |o, _| o.parameter_ids = o.parameter_ids.wrapping_add(1),
        },
        Defect {
            name: "parameter_points pointer moved",
            apply: |o, _| o.parameter_points = o.parameter_points.wrapping_add(1),
        },
        Defect {
            name: "event capacity changed",
            apply: |o, _| o.event_capacity -= 1,
        },
        Defect {
            name: "payload capacity changed",
            apply: |o, _| o.payload_capacity -= 2,
        },
        Defect {
            name: "queue capacity changed",
            apply: |o, _| o.parameter_queue_capacity -= 1,
        },
        Defect {
            name: "point capacity changed",
            apply: |o, _| o.parameter_point_capacity -= 1,
        },
        Defect {
            name: "event count above capacity",
            apply: |o, r| {
                o.event_count = 17;
                sync(o, r);
            },
        },
        Defect {
            name: "payload used above capacity",
            apply: |o, r| {
                o.payload_used = 66;
                sync(o, r);
            },
        },
        Defect {
            name: "queue count above capacity",
            apply: |o, r| {
                o.parameter_queue_count = 5;
                sync(o, r);
            },
        },
        Defect {
            name: "point count above capacity",
            apply: |o, r| {
                o.parameter_point_count = 17;
                sync(o, r);
            },
        },
        Defect {
            name: "report not published",
            apply: |_, r| r.flags = 19,
        },
        Defect {
            name: "report event count differs",
            apply: |_, r| r.event_count = 9,
        },
        Defect {
            name: "report payload count differs",
            apply: |_, r| r.payload_used = 20,
        },
        Defect {
            name: "report queue count differs",
            apply: |_, r| r.parameter_queue_count = 2,
        },
        Defect {
            name: "report point count differs",
            apply: |_, r| r.parameter_point_count = 5,
        },
        Defect {
            name: "event struct_size",
            apply: |o, _| ev(o, 0).struct_size = 96,
        },
        Defect {
            name: "event reserved",
            apply: |o, _| ev(o, 0).reserved = 1,
        },
        Defect {
            name: "event bus beyond table",
            apply: |o, _| ev(o, 0).bus = 16,
        },
        Defect {
            name: "event bus unselected",
            apply: |o, _| ev(o, 0).bus = 1,
        },
        Defect {
            name: "event unknown flag",
            apply: |o, _| ev(o, 0).flags = 0x0002,
        },
        Defect {
            name: "event ppq NaN",
            apply: |o, _| ev(o, 0).ppq_position = f64::NAN,
        },
        Defect {
            name: "event ppq infinite",
            apply: |o, _| ev(o, 0).ppq_position = f64::INFINITY,
        },
        Defect {
            name: "event offset at frames",
            apply: |o, _| ev(o, 9).offset = 16,
        },
        Defect {
            name: "event offsets decrease",
            apply: |o, _| ev(o, 5).offset = 2,
        },
        Defect {
            name: "event payload_offset past used",
            apply: |o, _| ev(o, 2).payload_offset = 23,
        },
        Defect {
            name: "event payload_bytes past used",
            apply: |o, _| ev(o, 2).payload_bytes = 23,
        },
        Defect {
            name: "note channel equals bus channels",
            apply: |o, _| ev(o, 0).channel = 2,
        },
        Defect {
            name: "note channel negative",
            apply: |o, _| ev(o, 0).channel = -1,
        },
        Defect {
            name: "note pitch 128",
            apply: |o, _| ev(o, 0).pitch = 128,
        },
        Defect {
            name: "note pitch negative",
            apply: |o, _| ev(o, 0).pitch = -1,
        },
        Defect {
            name: "note length negative",
            apply: |o, _| ev(o, 0).note_length = -1,
        },
        Defect {
            name: "note tuning NaN",
            apply: |o, _| ev(o, 0).tuning = f32::NAN,
        },
        Defect {
            name: "note velocity above one",
            apply: |o, _| ev(o, 0).velocity = 1.5,
        },
        Defect {
            name: "note velocity negative",
            apply: |o, _| ev(o, 0).velocity = -0.5,
        },
        Defect {
            name: "note velocity NaN",
            apply: |o, _| ev(o, 0).velocity = f32::NAN,
        },
        Defect {
            name: "note velocity infinite",
            apply: |o, _| ev(o, 0).velocity = f32::INFINITY,
        },
        Defect {
            name: "note carries payload",
            apply: |o, _| ev(o, 0).payload_bytes = 2,
        },
        Defect {
            name: "note-off velocity above one",
            apply: |o, _| ev(o, 1).velocity = 2.,
        },
        Defect {
            name: "note-off tuning infinite",
            apply: |o, _| ev(o, 1).tuning = f32::INFINITY,
        },
        Defect {
            name: "data subtype not zero",
            apply: |o, _| ev(o, 2).data_type = 1,
        },
        Defect {
            name: "pressure above one",
            apply: |o, _| ev(o, 6).velocity = 1.5,
        },
        Defect {
            name: "expression value above one",
            apply: |o, _| ev(o, 7).value = 1.5,
        },
        Defect {
            name: "expression value NaN",
            apply: |o, _| ev(o, 7).value = f64::NAN,
        },
        Defect {
            name: "expression value negative",
            apply: |o, _| ev(o, 7).value = -0.25,
        },
        Defect {
            name: "expression id MAX",
            apply: |o, _| ev(o, 7).expression_id = u32::MAX,
        },
        Defect {
            name: "expression carries payload",
            apply: |o, _| ev(o, 7).payload_bytes = 1,
        },
        Defect {
            name: "text expression id MAX",
            apply: |o, _| ev(o, 4).expression_id = u32::MAX,
        },
        Defect {
            name: "integer expression id MAX",
            apply: |o, _| ev(o, 8).expression_id = u32::MAX,
        },
        Defect {
            name: "integer expression carries payload",
            apply: |o, _| ev(o, 8).payload_bytes = 1,
        },
        Defect {
            name: "chord root 128",
            apply: |o, _| ev(o, 3).root = 128,
        },
        Defect {
            name: "chord root negative",
            apply: |o, _| ev(o, 3).root = -1,
        },
        Defect {
            name: "chord bass 128",
            apply: |o, _| ev(o, 3).bass = 128,
        },
        Defect {
            name: "chord mask above 16 bits",
            apply: |o, _| ev(o, 3).mask = 65536,
        },
        Defect {
            name: "scale root 128",
            apply: |o, _| ev(o, 5).root = 128,
        },
        Defect {
            name: "scale mask above 16 bits",
            apply: |o, _| ev(o, 5).mask = 65536,
        },
        Defect {
            name: "unknown kind",
            apply: |o, _| ev(o, 0).kind = 9,
        },
        Defect {
            name: "legacy controller 256",
            apply: |o, _| ev(o, 9).controller = 256,
        },
        Defect {
            name: "legacy value 128",
            apply: |o, _| ev(o, 9).midi_value = 128,
        },
        Defect {
            name: "legacy value2 128",
            apply: |o, _| ev(o, 9).midi_value2 = 128,
        },
        Defect {
            name: "legacy channel beyond bus",
            apply: |o, _| ev(o, 9).channel = 2,
        },
        Defect {
            name: "legacy carries payload",
            apply: |o, _| ev(o, 9).payload_bytes = 1,
        },
        Defect {
            name: "text payload_offset odd",
            // Empty text whose terminator bytes are still zero, so only the
            // alignment rule can reject it.
            apply: |o, _| ev(o, 5).payload_offset = 19,
        },
        Defect {
            name: "text terminator first byte nonzero",
            apply: |o, _| out_payload(o)[18] = 1,
        },
        Defect {
            name: "text terminator second byte nonzero",
            apply: |o, _| out_payload(o)[19] = 1,
        },
        Defect {
            name: "text terminator beyond payload_used",
            // The empty Scale text ends exactly at 20; its terminator needs 22.
            apply: |o, r| {
                o.payload_used = 20;
                sync(o, r);
            },
        },
        Defect {
            name: "text unpaired high surrogate",
            apply: |o, _| put_units(out_payload(o), 14, &[0xd800]),
        },
        Defect {
            name: "text lone low surrogate",
            apply: |o, _| put_units(out_payload(o), 14, &[0xdc00]),
        },
        Defect {
            name: "text embedded NUL",
            apply: |o, _| put_units(out_payload(o), 14, &[0x0000]),
        },
        Defect {
            name: "text odd byte count",
            apply: |o, _| ev(o, 4).payload_bytes = 3,
        },
        Defect {
            name: "chord surrogate pair broken",
            apply: |o, _| put_units(out_payload(o), 8, &[0x0041]),
        },
        Defect {
            name: "queue id duplicated",
            apply: |o, _| out_ids(o)[1] = 41,
        },
        Defect {
            name: "queue id u32::MAX",
            apply: |o, _| out_ids(o)[1] = u32::MAX,
        },
        Defect {
            name: "point id absent from queues",
            apply: |o, _| out_points(o)[1].id = 12345,
        },
        Defect {
            name: "point value above one",
            apply: |o, _| out_points(o)[2].value = 1.5,
        },
        Defect {
            name: "point value negative",
            apply: |o, _| out_points(o)[2].value = -0.5,
        },
        Defect {
            name: "point value NaN",
            apply: |o, _| out_points(o)[2].value = f64::NAN,
        },
        Defect {
            name: "point offset at frames",
            apply: |o, _| out_points(o)[4].offset = 16,
        },
        Defect {
            name: "point offset repeats within one id",
            apply: |o, _| out_points(o)[2].offset = 3,
        },
        Defect {
            name: "point offset decreases within one id",
            apply: |o, _| out_points(o)[4].offset = 7,
        },
    ];
    // The undamaged reply is admitted, so every failure below is its one defect.
    let control = scripted(Script::new(rich_reply), |result| {
        assert!(result.is_ok());
    });
    assert!(control.retained);
    for defect in &defects {
        assert_abi_and_retired(
            defect.name,
            Script {
                defect: Some(defect.apply),
                ..Script::new(rich_reply)
            },
        );
    }
}
#[test]
fn zero_frame_output_offsets_must_be_exactly_zero() {
    let zero = Script {
        frames: 0,
        ..Script::new(zero_frame_reply)
    };
    let control = scripted(zero, |result| {
        let output = result.expect("zero-frame reply is admitted");
        assert_eq!(output.events().next().unwrap().record().offset, 0);
        assert_eq!(output.parameter_points(), [pt(3, 0, 0.5)]);
    });
    assert!(control.retained && control.second_published);
    let defects = [
        Defect {
            name: "zero-frame event offset 1",
            apply: |o, _| ev(o, 0).offset = 1,
        },
        Defect {
            name: "zero-frame point offset 1",
            apply: |o, _| out_points(o)[0].offset = 1,
        },
    ];
    for defect in &defects {
        assert_abi_and_retired(
            defect.name,
            Script {
                defect: Some(defect.apply),
                ..zero
            },
        );
    }
}

struct Refusal {
    name: &'static str,
    status: i32,
    raw: RawReport,
}
#[test]
fn malformed_nonzero_error_reports_retire_while_valid_pre_dispatch_refusals_retry() {
    let initial = RawReport::initial();
    let shape = (2, 16, 2, 4);
    // Report::admit rejects each of these before the status is considered, so a
    // nonzero status cannot hide a corrupt report from the capability policy.
    let malformed = [
        Refusal {
            name: "unknown flag bit",
            status: 2,
            raw: RawReport {
                flags: 64,
                ..initial
            },
        },
        Refusal {
            name: "reserved",
            status: 1,
            raw: RawReport {
                reserved: 1,
                ..initial
            },
        },
        Refusal {
            name: "reserved2",
            status: 10,
            raw: RawReport {
                reserved2: 1,
                ..initial
            },
        },
        Refusal {
            name: "stage out of range",
            status: 2,
            raw: RawReport {
                stage: 99,
                ..initial
            },
        },
        Refusal {
            name: "failure out of range",
            status: 4,
            raw: RawReport {
                failure: 9,
                ..initial
            },
        },
        Refusal {
            name: "struct_size",
            status: 2,
            raw: RawReport {
                struct_size: 55,
                ..initial
            },
        },
        Refusal {
            name: "negative status with bad flag",
            status: -1,
            raw: RawReport {
                flags: 64,
                ..initial
            },
        },
        Refusal {
            name: "threw without attempted",
            status: 2,
            raw: RawReport {
                flags: 4,
                ..initial
            },
        },
        Refusal {
            name: "returned without attempted",
            status: 2,
            raw: RawReport {
                flags: 2,
                ..initial
            },
        },
        Refusal {
            name: "attempted without outcome",
            status: 2,
            raw: RawReport {
                flags: 1,
                ..initial
            },
        },
        Refusal {
            name: "returned and threw",
            status: 4,
            raw: RawReport {
                flags: 7,
                ..initial
            },
        },
        Refusal {
            name: "history without attempted",
            status: 2,
            raw: RawReport {
                flags: 16,
                ..initial
            },
        },
        Refusal {
            name: "published without returned",
            status: 2,
            raw: RawReport {
                flags: 32,
                ..initial
            },
        },
        Refusal {
            name: "native result without returned",
            status: 2,
            raw: RawReport {
                native_result: 9,
                ..initial
            },
        },
        Refusal {
            name: "event count",
            status: 2,
            raw: RawReport {
                event_count: 2049,
                ..initial
            },
        },
        Refusal {
            name: "payload count",
            status: 2,
            raw: RawReport {
                payload_used: 65537,
                ..initial
            },
        },
        Refusal {
            name: "queue count",
            status: 2,
            raw: RawReport {
                parameter_queue_count: 65,
                ..initial
            },
        },
        Refusal {
            name: "point count",
            status: 2,
            raw: RawReport {
                parameter_point_count: 2049,
                ..initial
            },
        },
    ];
    for row in &malformed {
        let script = Script {
            report: Some(row.raw),
            status: row.status,
            shape,
            ..Script::new(leave)
        };
        assert_abi_and_retired(row.name, script);
    }
    // Structurally valid refusals that did not attempt native DSP stay retryable.
    let retryable = [
        Refusal {
            name: "invalid",
            status: 1,
            raw: initial,
        },
        Refusal {
            name: "capacity",
            status: 2,
            raw: initial,
        },
        Refusal {
            name: "lifecycle",
            status: 10,
            raw: initial,
        },
        Refusal {
            name: "thread with provenance",
            status: 5,
            raw: RawReport {
                stage: 3,
                failure: 4,
                index: 3,
                ..initial
            },
        },
        Refusal {
            name: "local stage code",
            status: 3,
            raw: RawReport {
                stage: 112,
                ..initial
            },
        },
    ];
    for row in &retryable {
        let (status, raw) = (row.status, row.raw);
        let script = Script {
            report: Some(raw),
            status,
            shape,
            ..Script::new(leave)
        };
        let outcome = scripted(script, |result| match result {
            Err(e) => assert_eq!(
                e,
                Error::Provider {
                    status,
                    report: Report::admit(raw).unwrap()
                }
            ),
            Ok(_) => panic!("{}: refusal lost", row.name),
        });
        assert!(outcome.retained, "{}: capability retired", row.name);
        assert_eq!(outcome.calls, 2, "{}", row.name);
        assert!(outcome.second_published, "{}: retry failed", row.name);
    }
    // Contrast: a valid refusal after native DSP was attempted retires.
    for (name, raw) in [
        (
            "attempted native failure",
            RawReport {
                stage: 3,
                flags: 27,
                failure: 1,
                native_result: 4242,
                ..initial
            },
        ),
        (
            "attempted thrown failure",
            RawReport {
                stage: 3,
                flags: 29,
                failure: 2,
                ..initial
            },
        ),
    ] {
        let script = Script {
            report: Some(raw),
            status: 4,
            shape,
            ..Script::new(leave)
        };
        let outcome = scripted(script, |result| match result {
            Err(e) => assert_eq!(
                e,
                Error::Provider {
                    status: 4,
                    report: Report::admit(raw).unwrap()
                }
            ),
            Ok(_) => panic!("{name}: failure lost"),
        });
        assert!(!outcome.retained, "{name}");
        assert_eq!(outcome.calls, 1, "{name}");
        assert!(outcome.second_lifecycle, "{name}");
    }
}

#[test]
fn malformed_control_success_cannot_publish_catalogue_or_prepared_capability() {
    let mut native = instance_raw();
    native.close = Some(control_release);
    native.destroy = Some(control_release);
    let raw = control_raw(&native);
    // SAFETY: exact complete authored native table with live borrowed state.
    let provider = unsafe { Provider::from_raw(&raw).unwrap() };
    let state = ControlState::default();
    {
        let library = control_library(&native, &state);
        {
            let mut child = control_child(&library, &state);
            let owned = child.refresh_processing(provider, 2, 1).unwrap();
            let reads = state.bus_reads.get();
            state.mode.set(7);
            assert_eq!(child.refresh_processing(provider, 2, 1), Err(Error::Abi));
            assert_eq!(state.bus_reads.get(), reads);
            assert!(child.processing_metadata.is_none());
            assert!(child.processing_provider.is_none());
            assert_eq!(owned.inputs()[0].name, "A😀");
            state.mode.set(0);
            child.refresh_processing(provider, 2, 1).unwrap();
            let profile = Profile::new(48000., 16, 0, 2);
            child.prepare_processing(provider, profile).unwrap();
            state.mode.set(8);
            assert_eq!(child.prepare_processing(provider, profile), Err(Error::Abi));
            assert!(child.profile.is_none());
            assert!(child.processing_provider.is_none());
            assert!(child.parameter_provider.is_none());
            state.mode.set(0);
            child.prepare_processing(provider, profile).unwrap();
            assert!(child.profile.is_some());
            assert!(child.processing_provider.is_some());
        }
        assert_eq!(state.releases.get(), 1);
    }
    assert_eq!(state.releases.get(), 2);
}
