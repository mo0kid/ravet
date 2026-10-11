//! A Graph-bank child editor over the actual RAVET Gain VST3 editor
//! (decision 0276). Built as a static library and linked with the
//! caller-linked native provider by `ravet-vst3-editor-bank-window-tests`
//! (native CMake). Its Objective-C++ `main` owns a hidden offscreen `NSWindow`
//! and calls [`ravet_vst3_editor_bank_run`] on the platform main thread.
//!
//! Each of three cycles loads the Gain module, builds an owned f32 bank of two
//! Gain children and processes them on one AUDIO worker:
//!
//! - Child 0 has edit gestures and the editor, attached to the window's content
//!   view while the bank's leases are live.
//! - Child 1 is the reference: it receives the same normalized value as an
//!   ordinary framework `ParameterValue` at offset 0 of the block in which
//!   child 0's edit is due.
//!
//! The host injects an edit into the Gain editor's own script-message bridge.
//! The plug-in's controller then calls `beginEdit`, `performEdit` and
//! `endEdit`. In lockstep, a block processed after the injection but before
//! `pump_edits` must be unchanged, and the first block after the pump must match
//! the reference bit for bit. A free-running phase then edits while the worker
//! processes continuously. Every AUDIO `process` call is measured with
//! `allocation-counter` (Rust heap only). Teardown returns the nodes, checks
//! that reclamation is deferred while the editor is open, closes the editor,
//! and reclaims the bank.
#![allow(unsafe_code)]
use allocation_counter::measure;
use ravet_foundation::{
    AudioBuffer, Event, EventData, ParameterGesturePhase, ParameterId, ProcessBlock, ProcessConfig,
    ProcessStatus, Processor, Transport,
};
use ravet_host::vst3::instances::{
    self,
    editor::{self, Closed, Platform, Size},
    hosted_audio::{
        self, BusRequest,
        graph::{
            Vst3GraphBankBuilder, Vst3GraphNode, Vst3GraphNodeOptions,
            edits::{self, EditKind, EditOptions},
            framework::{FrameworkTranslation, ParameterRoute, ParameterScale},
            reclaim_vst3_graph_banks,
        },
    },
    parameters, processing,
};
use std::{
    ffi::{CStr, CString, c_char, c_void},
    panic::{AssertUnwindSafe, catch_unwind},
    ptr::NonNull,
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicU64, Ordering},
        mpsc,
    },
    thread,
    time::{Duration, Instant},
};

const STEREO: u64 = 0b11;
const FRAMES: usize = 256;
const GAIN_ID: u32 = 1;
const GAIN: ParameterId = ParameterId(1);

/// Host window services supplied by the Objective-C++ test (MAIN only).
#[repr(C)]
pub struct Host {
    pub context: *mut c_void,
    /// Create the hidden offscreen window for `cycle` and return its content
    /// `NSView` (owned by the host until `close_window`).
    pub open_window: unsafe extern "C" fn(*mut c_void, u32, u32, u32) -> *mut c_void,
    /// Inject one JSON edit message into the attached Gain editor's bridge.
    pub edit: unsafe extern "C" fn(*mut c_void, *const c_char) -> i32,
    /// Run the main run loop for the given seconds.
    pub pump: unsafe extern "C" fn(*mut c_void, f64),
    /// Number of subviews of the content view.
    pub subviews: unsafe extern "C" fn(*mut c_void) -> u32,
    pub resize_window: unsafe extern "C" fn(*mut c_void, u32, u32),
    pub close_window: unsafe extern "C" fn(*mut c_void),
}
#[derive(Clone, Copy)]
struct Tables {
    instance: &'static instances::RawProvider,
    parameters: &'static parameters::RawProvider,
    processing: &'static processing::RawProvider,
    hosted: &'static hosted_audio::RawProvider,
    editor: &'static editor::RawProvider,
    edits: &'static edits::RawProvider,
}
struct HostRef<'h>(&'h Host);
impl HostRef<'_> {
    fn edit(&self, kind: &str, value: f64) {
        let json = CString::new(format!(
            r#"{{"type":"{kind}","id":{GAIN_ID},"value":{value}}}"#
        ))
        .unwrap();
        // SAFETY: host contract; NUL-terminated JSON valid for the call.
        assert_eq!(
            unsafe { (self.0.edit)(self.0.context, json.as_ptr()) },
            0,
            "bridge injection"
        );
    }
    fn pump(&self, seconds: f64) {
        // SAFETY: host contract on MAIN.
        unsafe { (self.0.pump)(self.0.context, seconds) }
    }
    fn subviews(&self) -> u32 {
        // SAFETY: host contract on MAIN.
        unsafe { (self.0.subviews)(self.0.context) }
    }
}

enum Work {
    /// Process one lockstep block; child 1 gets `reference` at offset 0.
    Block {
        reference: Option<f32>,
    },
    /// Process continuously until the stop flag is set.
    Run,
    Stop,
}
#[derive(Clone, Copy, Debug)]
struct Done {
    /// Child 0 output equals child 1 output for every sample.
    equal: bool,
    /// Largest |child 0 output - input| (unity gain gives 0).
    off_unity: f32,
    /// Child 0 output / input at the last sample of the block.
    ratio: f32,
    blocks: u64,
    allocations: u64,
}
fn input(frame: usize, channel: usize) -> f32 {
    0.25 * ((frame as f32) * 0.031 + channel as f32).sin() + 0.3
}
fn worker(
    mut nodes: Vec<Vst3GraphNode<f32>>,
    work: mpsc::Receiver<Work>,
    done: mpsc::Sender<Done>,
    stop: Arc<AtomicBool>,
    running: Arc<AtomicU64>,
) {
    for node in &mut nodes {
        node.prepare(ProcessConfig::new(48000., FRAMES, 0, node.layout()).unwrap())
            .unwrap();
        node.start_audio().unwrap();
    }
    let inputs: [Vec<f32>; 2] = [0, 1].map(|c| (0..FRAMES).map(|f| input(f, c)).collect());
    let mut outputs = [
        [vec![0f32; FRAMES], vec![0f32; FRAMES]],
        [vec![0f32; FRAMES], vec![0f32; FRAMES]],
    ];
    let mut blocks = 0u64;
    let mut allocations = 0u64;
    let mut block = |nodes: &mut Vec<Vst3GraphNode<f32>>,
                     outputs: &mut [[Vec<f32>; 2]; 2],
                     reference: Option<f32>|
     -> Done {
        let event = [Event {
            frame_offset: 0,
            data: EventData::ParameterValue {
                id: GAIN,
                value: reference.unwrap_or(0.),
            },
        }];
        let events: &[Event] = if reference.is_some() { &event } else { &[] };
        for (index, node) in nodes.iter_mut().enumerate() {
            let [l, r] = &mut outputs[index];
            let ins = [&inputs[0][..], &inputs[1][..]];
            let used = if index == 1 { events } else { &[] };
            let measured = measure(|| {
                let mut outs = [&mut l[..], &mut r[..]];
                let audio = AudioBuffer::separate(&ins, &mut outs, FRAMES).unwrap();
                let status =
                    node.process(&mut ProcessBlock::new(audio, used, Transport::default()));
                assert_eq!(
                    status,
                    ProcessStatus::Continue,
                    "child {index} block {blocks}"
                );
            });
            allocations += measured.count_total;
        }
        blocks += 1;
        let [ours, theirs] = &*outputs;
        let mut off_unity = 0f32;
        for c in 0..2 {
            for f in 0..FRAMES {
                assert!(ours[c][f].is_finite() && theirs[c][f].is_finite());
                off_unity = off_unity.max((ours[c][f] - inputs[c][f]).abs());
            }
        }
        Done {
            equal: ours == theirs,
            off_unity,
            ratio: ours[0][FRAMES - 1] / inputs[0][FRAMES - 1],
            blocks,
            allocations,
        }
    };
    while let Ok(next) = work.recv() {
        match next {
            Work::Block { reference } => done
                .send(block(&mut nodes, &mut outputs, reference))
                .unwrap(),
            Work::Run => {
                let mut last = None;
                while !stop.load(Ordering::Acquire) {
                    last = Some(block(&mut nodes, &mut outputs, None));
                    running.fetch_add(1, Ordering::Release);
                    thread::sleep(Duration::from_micros(300));
                }
                done.send(last.expect("free-running blocks")).unwrap();
            }
            Work::Stop => break,
        }
    }
    // Nodes return their bundles from this worker; MAIN reclaims.
    drop(nodes);
}

fn audio_class(library: &instances::Library<'static>) -> u32 {
    let index = library
        .classes()
        .iter()
        .position(|c| c.category.starts_with(b"Audio Module Class\0"))
        .expect("audio module class");
    index as u32
}
fn options() -> Vst3GraphNodeOptions {
    Vst3GraphNodeOptions {
        class_index: 0,
        inputs: vec![BusRequest::active(STEREO)],
        outputs: vec![BusRequest::active(STEREO)],
        event_inputs: 0,
        event_outputs: 0,
        lanes: None,
    }
}
fn route(scale: ParameterScale) -> FrameworkTranslation {
    let mut t = FrameworkTranslation::new(0, 4);
    t.parameters = vec![ParameterRoute::new(GAIN, GAIN_ID, scale)];
    t
}

#[derive(Default)]
struct Totals {
    lockstep: u64,
    free: u64,
    edits: u64,
    allocations: u64,
}

fn cycle(tables: Tables, path: &str, host: &HostRef<'_>, index: u32, totals: &mut Totals) {
    // SAFETY: the caller-linked provider tables and trusted local module.
    let library = unsafe {
        instances::Provider::from_raw(tables.instance)
            .unwrap()
            .load(path)
            .expect("load Gain")
    };
    let class = audio_class(&library);
    // SAFETY: exact paired static provider tables.
    let (parameter, processing, hosted, editors, gestures) = unsafe {
        (
            parameters::Provider::from_raw(tables.parameters).unwrap(),
            processing::Provider::from_raw(tables.processing).unwrap(),
            hosted_audio::Provider::from_raw(tables.hosted).unwrap(),
            editor::Provider::from_raw(tables.editor).unwrap(),
            edits::Provider::from_raw(tables.edits).unwrap(),
        )
    };
    let mut builder = Vst3GraphBankBuilder::<f32>::new(
        library,
        parameter,
        processing,
        hosted,
        48000.,
        FRAMES as u32,
    )
    .unwrap();
    let mut node = options();
    node.class_index = class;
    let edited = builder.node(node.clone()).unwrap();
    let reference = builder.node(node).unwrap();
    builder
        .translate(
            edited,
            route(ParameterScale::Linear {
                minimum: 0.,
                maximum: 4.,
            }),
        )
        .unwrap();
    builder
        .translate(reference, route(ParameterScale::Normalized))
        .unwrap();
    builder
        .edit_gestures(edited, gestures, EditOptions::default())
        .unwrap();
    let (control, nodes) = builder.build().expect("build Gain bank");
    let (work, jobs) = mpsc::channel();
    let (done_tx, done) = mpsc::channel();
    let stop = Arc::new(AtomicBool::new(false));
    let running = Arc::new(AtomicU64::new(0));
    let audio = {
        let (stop, running) = (Arc::clone(&stop), Arc::clone(&running));
        thread::spawn(move || worker(nodes, jobs, done_tx, stop, running))
    };
    let step = |reference: Option<f32>| -> Done {
        work.send(Work::Block { reference }).unwrap();
        done.recv().expect("lockstep block")
    };
    // Leases are live: process before the editor exists.
    let first = step(None);
    assert!(first.equal && first.off_unity < 1e-6, "{first:?}");
    // Editor on MAIN while AUDIO stays live on the worker.
    let mut editor = control
        .open_editor(edited, editors, Platform::NsView)
        .expect("Gain editor");
    let initial = editor.last_size();
    // SAFETY: host contract: a live hidden window content view.
    let parent =
        unsafe { (host.0.open_window)(host.0.context, index, initial.width(), initial.height()) };
    let parent = NonNull::new(parent).expect("content view");
    // SAFETY: the content view stays alive until close_window below, after the
    // editor is closed.
    unsafe { editor.attach(parent) }.expect("attach");
    assert!(editor.attached() && host.subviews() >= 1);
    host.pump(0.3);
    let resized = editor.resize(Size::new(640, 480).unwrap()).expect("resize");
    // SAFETY: host contract on MAIN.
    unsafe { (host.0.resize_window)(host.0.context, resized.width(), resized.height()) };
    assert!(editor.state().unwrap().attached());
    let _ = editor.take_resize_request().unwrap();
    // A second editor for the same child is refused.
    assert!(
        control
            .open_editor(edited, editors, Platform::NsView)
            .is_err()
    );

    // Lockstep: an injected but unpumped edit does not reach AUDIO.
    let before = step(None);
    assert!(before.equal && before.off_unity < 1e-6, "{before:?}");
    host.edit("edit", 2.0);
    let state = control.edit_state(edited).unwrap();
    assert_eq!((state.native_pending, state.accepted), (3, 3), "{state:?}");
    let unpumped = step(None);
    assert!(unpumped.equal && unpumped.off_unity < 1e-6, "{unpumped:?}");
    assert_eq!(control.pump_edits(edited).unwrap(), 3);
    // The first block after the pump matches the framework reference exactly.
    let applied = step(Some(0.5));
    assert!(applied.equal && applied.off_unity > 1e-4, "{applied:?}");
    let mut settled = applied;
    for _ in 0..200 {
        settled = step(None);
    }
    assert!(
        settled.equal && (settled.ratio - 2.).abs() < 1e-4,
        "{settled:?}"
    );
    let mut events = Vec::new();
    control.drain_edits(edited, |e| events.push(e)).unwrap();
    assert_eq!(events.len(), 3);
    assert_eq!(events[0].kind, EditKind::Begin { id: GAIN_ID });
    assert_eq!(
        events[1].kind,
        EditKind::Perform {
            id: GAIN_ID,
            normalized: 0.5
        }
    );
    assert_eq!(events[2].kind, EditKind::End { id: GAIN_ID });
    assert_eq!(
        events.iter().map(|e| e.framework).collect::<Vec<_>>(),
        vec![
            Some(EventData::ParameterGesture {
                id: GAIN,
                phase: ParameterGesturePhase::Begin
            }),
            Some(EventData::ParameterValue {
                id: GAIN,
                value: 2.
            }),
            Some(EventData::ParameterGesture {
                id: GAIN,
                phase: ParameterGesturePhase::End
            }),
        ]
    );
    totals.lockstep += settled.blocks;

    // Free-running: edits while the worker processes continuously.
    work.send(Work::Run).unwrap();
    let values = [0.5, 3.0, 1.25, 0.75, 1.5];
    for value in values {
        host.edit("begin", 0.);
        host.edit("value", value);
        host.edit("end", 0.);
        let started = running.load(Ordering::Acquire);
        assert_eq!(control.pump_edits(edited).unwrap(), 3);
        host.pump(0.02);
        let deadline = Instant::now() + Duration::from_secs(5);
        while running.load(Ordering::Acquire) < started + 20 {
            assert!(Instant::now() < deadline, "worker stalled");
            thread::sleep(Duration::from_millis(1));
        }
    }
    // Let the last value settle, then stop the worker.
    let target = running.load(Ordering::Acquire) + 250;
    let deadline = Instant::now() + Duration::from_secs(10);
    while running.load(Ordering::Acquire) < target {
        assert!(Instant::now() < deadline, "worker stalled");
        host.pump(0.01);
    }
    stop.store(true, Ordering::Release);
    let free = done.recv().expect("free-running result");
    assert!((free.ratio - 1.5).abs() < 1e-4, "{free:?}");
    let mut published = 0;
    control.drain_edits(edited, |_| published += 1).unwrap();
    assert_eq!(published, 15);
    let state = control.edit_state(edited).unwrap();
    assert_eq!(
        (
            state.native_pending,
            state.overflowed,
            state.invalid,
            state.unknown,
            state.off_thread
        ),
        (0, 0, 0, 0, 0),
        "{state:?}"
    );
    assert_eq!(
        (state.pumped, state.applied, state.queued),
        (18, 6, 0),
        "{state:?}"
    );
    assert_eq!(free.allocations, 0, "Rust heap operations on AUDIO");
    totals.free += free.blocks - settled.blocks;
    totals.edits += state.pumped;
    totals.allocations += free.allocations;

    // Teardown: return the nodes from the worker, then reclaim.
    work.send(Work::Stop).unwrap();
    audio.join().expect("AUDIO worker");
    assert_eq!(control.poll_returns().unwrap().returned, 2);
    let id = control.id();
    drop(control);
    // The open editor defers release: no lease ends and no child retires.
    let report = reclaim_vst3_graph_banks(Some(id)).unwrap();
    assert_eq!((report.released, report.editors), (0, 1), "{report:?}");
    assert!(editor.attached());
    match index {
        // Cycle 1 drops the guard instead of closing it.
        1 => drop(editor),
        _ => assert_eq!(editor.close().ok(), Some(Closed::Complete)),
    }
    host.pump(0.1);
    assert_eq!(host.subviews(), 0);
    let report = reclaim_vst3_graph_banks(Some(id)).unwrap();
    assert_eq!((report.released, report.editors), (1, 0), "{report:?}");
    // SAFETY: host contract; the editor no longer references the view.
    unsafe { (host.0.close_window)(host.0.context) };
    host.pump(0.05);
    println!(
        "cycle {index}: editor {}x{} -> {}x{}, lockstep blocks {}, free-running blocks {}, edits {}, AUDIO allocations {}",
        initial.width(),
        initial.height(),
        resized.width(),
        resized.height(),
        settled.blocks,
        free.blocks - settled.blocks,
        state.pumped,
        free.allocations
    );
}

/// Run three Graph-bank editor cycles over the Gain module at `gain_path`.
/// Returns 0 on success, 1 on any failure (the panic message is printed).
///
/// # Safety
/// Must run on the platform main thread. All table pointers are the
/// caller-linked provider's static, exactly paired tables; the path is a
/// NUL-terminated UTF-8 absolute path of the trusted RAVET Gain VST3 built with
/// its editor; `host` supplies valid MAIN-only window services.
#[unsafe(no_mangle)]
#[allow(clippy::too_many_arguments)]
pub unsafe extern "C" fn ravet_vst3_editor_bank_run(
    instance: *const instances::RawProvider,
    parameters: *const parameters::RawProvider,
    processing: *const processing::RawProvider,
    hosted: *const hosted_audio::RawProvider,
    editor: *const editor::RawProvider,
    gestures: *const edits::RawProvider,
    gain_path: *const c_char,
    host: *const Host,
) -> i32 {
    let outcome = catch_unwind(AssertUnwindSafe(|| {
        // SAFETY: caller contract above.
        let (tables, path, host) = unsafe {
            (
                Tables {
                    instance: instance.as_ref().expect("instance table"),
                    parameters: parameters.as_ref().expect("parameter table"),
                    processing: processing.as_ref().expect("processing table"),
                    hosted: hosted.as_ref().expect("hosted table"),
                    editor: editor.as_ref().expect("editor table"),
                    edits: gestures.as_ref().expect("edit-gesture table"),
                },
                CStr::from_ptr(gain_path)
                    .to_str()
                    .expect("UTF-8 path")
                    .to_owned(),
                HostRef(host.as_ref().expect("host services")),
            )
        };
        let mut totals = Totals::default();
        for index in 0..3 {
            cycle(tables, &path, &host, index, &mut totals);
        }
        let report = reclaim_vst3_graph_banks(None).expect("final reclaim");
        assert_eq!((report.released, report.retained), (0, 0));
        assert_eq!(editor::refused_drops(), 0);
        println!(
            "PASS Graph-bank child editor: actual RAVET Gain editor in a hidden offscreen NSWindow, 3 cycles, \
             {} lockstep and {} free-running AUDIO blocks on a worker, {} controller edits pumped, \
             edit applied exactly at the first block after pump_edits (bit-equal to the framework reference), \
             reclamation deferred while open, {} Rust heap operations on AUDIO",
            totals.lockstep, totals.free, totals.edits, totals.allocations
        );
    }));
    i32::from(outcome.is_err())
}
