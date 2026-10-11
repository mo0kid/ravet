//! Rust hosted-audio Graph banks over the actual RAVET reference VST3 modules
//! (ADR 0274). Built as a static library and linked with the caller-linked
//! native provider by `ravet-vst3-reference-bank-tests` (native CMake), whose C
//! `main` calls [`ravet_vst3_reference_bank_run`] on the platform main thread.
//!
//! Each scenario builds an owned bank from a freshly loaded module, places its
//! nodes in a canonical [`Graph`], processes variable and zero-frame blocks on
//! one AUDIO worker with every `Graph::process` call measured by
//! `allocation-counter` (Rust heap only), compares the output against
//! independent closed forms, returns the nodes and reclaims the bank on MAIN.
#![allow(unsafe_code)]
use allocation_counter::measure;
use ravet_foundation::{AudioBuffer, AudioLayout, ProcessBlock, ProcessConfig, Tail, Transport};
use ravet_graph::{
    Connection, Destination, Graph, GraphNode, GraphSample, GraphStatus, NodeId, Source,
};
use ravet_host::vst3::instances::{
    self, Library,
    hosted_audio::{
        self, BusRequest, HostedSample,
        graph::{
            LaneOptions, Vst3GraphBankBuilder, Vst3GraphNodeOptions, reclaim_vst3_graph_banks,
        },
    },
    parameters::{self, Point},
    processing::{self, Event, Header, Kind, Note},
};
use std::{
    ffi::{CStr, c_char},
    panic::{AssertUnwindSafe, catch_unwind},
    sync::mpsc,
};

const STEREO: u64 = 0b11;
const MONO: u64 = 1 << 19;
const FRAMES: u32 = 64;
const BLOCKS: u32 = 600;

#[derive(Clone, Copy)]
struct Tables {
    instance: &'static instances::RawProvider,
    parameters: &'static parameters::RawProvider,
    processing: &'static processing::RawProvider,
    hosted: &'static hosted_audio::RawProvider,
}

trait Precision: HostedSample + GraphSample + PartialEq + std::fmt::Debug {
    const BITS: u32;
    fn from_f64(value: f64) -> Self;
    fn to_f64(self) -> f64;
    fn half() -> Self;
    fn one() -> Self;
    fn add(self, other: Self) -> Self;
}
impl Precision for f32 {
    const BITS: u32 = 32;
    fn from_f64(value: f64) -> Self {
        value as f32
    }
    fn to_f64(self) -> f64 {
        f64::from(self)
    }
    fn half() -> Self {
        0.5
    }
    fn one() -> Self {
        1.
    }
    fn add(self, other: Self) -> Self {
        self + other
    }
}
impl Precision for f64 {
    const BITS: u32 = 64;
    fn from_f64(value: f64) -> Self {
        value
    }
    fn to_f64(self) -> f64 {
        self
    }
    fn half() -> Self {
        0.5
    }
    fn one() -> Self {
        1.
    }
    fn add(self, other: Self) -> Self {
        self + other
    }
}

/// Load a module on MAIN and return its audio class index with a bank builder.
fn builder<S: Precision>(tables: Tables, path: &str) -> (u32, Vst3GraphBankBuilder<S>) {
    // SAFETY: the caller-linked native provider tables are process-static and
    // exactly paired; the module is the trusted RAVET reference built locally.
    let library: Library<'static> = unsafe {
        instances::Provider::from_raw(tables.instance)
            .expect("instance provider")
            .load(path)
            .expect("load reference module")
    };
    let class = library
        .classes()
        .iter()
        .position(|c| c.category.starts_with(b"Audio Module Class\0"))
        .expect("audio class") as u32;
    // SAFETY: as above.
    let (p, q, h) = unsafe {
        (
            parameters::Provider::from_raw(tables.parameters).expect("parameters"),
            processing::Provider::from_raw(tables.processing).expect("processing"),
            hosted_audio::Provider::from_raw(tables.hosted).expect("hosted"),
        )
    };
    let builder = Vst3GraphBankBuilder::new(library, p, q, h, 48000., FRAMES).expect("builder");
    (class, builder)
}

fn main_sample(call: u32, channel: u32, frame: u32) -> f64 {
    f64::from((frame * 41 + call * 13 + channel * 7) % 97) / 48. - 1. + 1e-10 * f64::from(frame + 1)
}
fn key_sample(call: u32, channel: u32, frame: u32) -> f64 {
    f64::from((frame * 17 + call * 5 + channel * 31) % 83) / 41. - 1.
}

/// Three children: stereo key active, mono key active, stereo key inactive.
fn sidechain<S: Precision>(tables: Tables, path: &str) {
    let (class, mut builder) = builder::<S>(tables, path);
    let child = |arrangement: u64, active: bool| Vst3GraphNodeOptions {
        class_index: class,
        inputs: vec![
            BusRequest::active(STEREO),
            if active {
                BusRequest::active(arrangement)
            } else {
                BusRequest::inactive(arrangement)
            },
        ],
        outputs: vec![BusRequest::active(STEREO)],
        event_inputs: 0,
        event_outputs: 0,
        lanes: None,
    };
    builder.node(child(STEREO, true)).unwrap();
    builder.node(child(MONO, true)).unwrap();
    builder.node(child(STEREO, false)).unwrap();
    let (control, nodes) = builder.build().expect("bank build");
    let layouts: Vec<_> = nodes.iter().map(|n| n.layout()).collect();
    assert_eq!(
        layouts,
        [
            AudioLayout::new(4, 2).unwrap(),
            AudioLayout::new(3, 2).unwrap(),
            AudioLayout::STEREO
        ]
    );
    for c in 0..3 {
        let timing = control.timing(c).unwrap();
        assert_eq!((timing.latency_samples, timing.tail), (0, Tail::None));
    }
    assert_eq!(control.profile(1).unwrap().map().input_channels(), 3);
    let ids = [NodeId(1), NodeId(2), NodeId(3)];
    let mut routes = Vec::new();
    let mut route = |source, id: NodeId, channel| {
        routes.push(Connection {
            source,
            destination: Destination::Node { id, channel },
            gain: S::one(),
        })
    };
    for (n, id) in ids.iter().enumerate() {
        route(Source::Input(0), *id, 0);
        route(Source::Input(1), *id, 1);
        if n == 0 {
            route(Source::Input(2), *id, 2);
            route(Source::Input(3), *id, 3);
        } else if n == 1 {
            route(Source::Input(2), *id, 2);
        }
    }
    for (n, id) in ids.iter().enumerate() {
        for channel in 0..2 {
            routes.push(Connection {
                source: Source::Node { id: *id, channel },
                destination: Destination::Output(2 * n + channel),
                gain: S::one(),
            });
        }
    }
    let outer = AudioLayout::new(4, 6).unwrap();
    let graph_nodes = nodes
        .into_iter()
        .zip(ids)
        .map(|(node, id)| GraphNode {
            id,
            layout: node.layout(),
            processor: Box::new(node) as Box<dyn ravet_foundation::Processor<S> + Send>,
        })
        .collect();
    let mut graph = Graph::new(outer, graph_nodes, &routes).expect("graph");
    graph
        .prepare(ProcessConfig::new(48000., FRAMES as usize, 0, outer).unwrap())
        .unwrap();
    let (compared, zero, wide) = std::thread::spawn(move || {
        let (mut compared, mut zero, mut wide) = (0usize, 0usize, 0usize);
        let mut ins = vec![vec![S::default(); FRAMES as usize]; 4];
        let mut outs = vec![vec![S::default(); FRAMES as usize + 3]; 6];
        for call in 0..BLOCKS {
            let frames = ((call * 7) % (FRAMES + 1)) as usize;
            zero += usize::from(frames == 0);
            for f in 0..FRAMES {
                for ch in 0..2 {
                    ins[ch as usize][f as usize] = S::from_f64(main_sample(call, ch, f));
                    ins[2 + ch as usize][f as usize] = S::from_f64(key_sample(call, ch, f));
                }
            }
            outs.iter_mut().for_each(|o| o.fill(S::from_f64(9.)));
            let views: Vec<&[S]> = ins.iter().map(|v| &v[..]).collect();
            let mut out_views: Vec<&mut [S]> = outs.iter_mut().map(|v| &mut v[..]).collect();
            let audio = AudioBuffer::separate(&views, &mut out_views, frames).unwrap();
            let mut block = ProcessBlock::new(audio, &[], Transport::default());
            let mut status = GraphStatus::Continue;
            let counted = measure(|| status = graph.process(&mut block));
            assert_eq!(status, GraphStatus::Continue);
            assert_eq!((counted.count_total, counted.count_current), (0, 0));
            for f in 0..frames {
                let stereo_key = ins[2][f].add(ins[3][f]) * S::half();
                for ch in 0..2 {
                    // Independent closed forms per child.
                    assert_eq!(outs[ch][f], ins[ch][f] * stereo_key);
                    assert_eq!(outs[2 + ch][f], ins[ch][f] * ins[2][f]);
                    assert_eq!(outs[4 + ch][f], ins[ch][f] * S::one());
                    compared += 3;
                }
                wide += usize::from(outs[0][f].to_f64() != f64::from(outs[0][f].to_f64() as f32));
            }
            for o in &outs {
                assert!(o[frames..].iter().all(|v| *v == S::from_f64(9.)));
            }
        }
        drop(graph); // Nodes return their bundles; no native teardown on AUDIO.
        (compared, zero, wide)
    })
    .join()
    .expect("sidechain worker");
    if S::BITS == 64 {
        assert!(wide > 0, "f64 keeps sub-f32 detail");
    }
    assert_eq!(control.poll_returns().unwrap().returned, 3);
    assert_eq!(control.retire().unwrap().released, 1);
    println!(
        "PASS bank sidechain {}bit: 3 children (stereo/mono active, stereo inactive), {BLOCKS} graph blocks ({zero} zero-frame, frames 0..64), {compared} samples exact, 0 Rust heap operations in measured Graph::process",
        S::BITS
    );
}

/// Independent canonical record hash (see the reference crate's docs).
struct Fnv(u32);
impl Fnv {
    fn put(&mut self, bytes: &[u8]) {
        for b in bytes {
            self.0 = (self.0 ^ u32::from(*b)).wrapping_mul(16_777_619);
        }
    }
    fn text(&mut self, text: &str) {
        let units: Vec<u16> = text.encode_utf16().collect();
        self.put(&(units.len() as u32).to_le_bytes());
        units.iter().for_each(|u| self.put(&u.to_le_bytes()));
    }
    fn fold(&self) -> u32 {
        (self.0 ^ (self.0 >> 20)) & 0xf_ffff
    }
}
fn code(kind: &Kind<'_>) -> u16 {
    match kind {
        Kind::NoteOn(_) => 0,
        Kind::NoteOff(_) => 1,
        Kind::SysEx(_) => 2,
        Kind::PolyPressure { .. } => 3,
        Kind::ExpressionValue { .. } => 4,
        Kind::ExpressionText { .. } => 5,
        Kind::Chord { .. } => 6,
        Kind::Scale { .. } => 7,
        Kind::ExpressionInteger { .. } => 8,
    }
}
fn event_hash(event: &Event<'_>) -> u32 {
    let mut h = Fnv(0x811c_9dc5);
    h.put(&code(&event.kind).to_le_bytes());
    h.put(&event.header.flags.to_le_bytes());
    h.put(&(event.header.bus as i32).to_le_bytes());
    h.put(&event.header.ppq_position.to_bits().to_le_bytes());
    let i = |h: &mut Fnv, v: i32| h.put(&v.to_le_bytes());
    match event.kind {
        Kind::NoteOn(n) => {
            for v in [
                i32::from(n.channel),
                i32::from(n.pitch),
                n.note_id,
                n.length,
            ] {
                i(&mut h, v);
            }
            h.put(&n.tuning.to_bits().to_le_bytes());
            h.put(&n.velocity.to_bits().to_le_bytes());
        }
        Kind::NoteOff(n) => {
            for v in [i32::from(n.channel), i32::from(n.pitch), n.note_id] {
                i(&mut h, v);
            }
            h.put(&n.tuning.to_bits().to_le_bytes());
            h.put(&n.velocity.to_bits().to_le_bytes());
        }
        Kind::SysEx(bytes) => {
            h.put(&0u32.to_le_bytes());
            h.put(&(bytes.len() as u32).to_le_bytes());
            h.put(bytes);
        }
        Kind::PolyPressure {
            channel,
            pitch,
            note_id,
            pressure,
        } => {
            for v in [i32::from(channel), i32::from(pitch), note_id] {
                i(&mut h, v);
            }
            h.put(&pressure.to_bits().to_le_bytes());
        }
        Kind::ExpressionValue {
            expression_id,
            note_id,
            value,
        } => {
            h.put(&expression_id.to_le_bytes());
            i(&mut h, note_id);
            h.put(&value.to_bits().to_le_bytes());
        }
        Kind::ExpressionText {
            expression_id,
            note_id,
            text,
        } => {
            h.put(&expression_id.to_le_bytes());
            i(&mut h, note_id);
            h.text(text);
        }
        Kind::Chord {
            root,
            bass,
            mask,
            text,
        } => {
            i(&mut h, i32::from(root));
            i(&mut h, i32::from(bass));
            h.put(&u32::from(mask).to_le_bytes());
            h.text(text);
        }
        Kind::Scale { root, mask, text } => {
            i(&mut h, i32::from(root));
            h.put(&u32::from(mask).to_le_bytes());
            h.text(text);
        }
        Kind::ExpressionInteger {
            expression_id,
            note_id,
            value,
        } => {
            h.put(&expression_id.to_le_bytes());
            i(&mut h, note_id);
            h.put(&value.to_le_bytes());
        }
    }
    h.fold()
}
fn point_hash(point: &Point) -> u32 {
    let mut h = Fnv(0x811c_9dc5);
    h.put(&16u16.to_le_bytes());
    h.put(&0u16.to_le_bytes());
    h.put(&0i32.to_le_bytes());
    h.put(&0f64.to_bits().to_le_bytes());
    h.put(&point.id.to_le_bytes());
    h.put(&point.value.to_bits().to_le_bytes());
    h.fold()
}
fn digest_step(digest: u32, hash: u32) -> u32 {
    (digest.wrapping_mul(0x0100_0193) ^ hash) & 0xf_ffff
}

const TEXTS: [&str; 5] = ["", "A", "Cmaj7", "\u{1d11e} x", "\u{e9}t\u{e9}"];
const SYSEX: [u8; 5] = [0xf0, 0x7d, 0x11, 0x22, 0xf7];

/// One event of each of the nine kinds plus up to two Marker points.
fn batch(call: u32, frames: u32) -> (Vec<Event<'static>>, Vec<Point>) {
    let header = |i: u32| Header {
        bus: 0,
        offset: if frames == 0 { 0 } else { i * frames / 9 },
        ppq_position: f64::from(call) * 0.25 - f64::from(i) * 0.125,
        flags: if (call + i).is_multiple_of(3) {
            0xc001
        } else {
            (call % 2) as u16
        },
    };
    let kinds = [
        Kind::NoteOn(Note {
            channel: (call % 16) as u8,
            pitch: (call % 128) as u8,
            note_id: if call.is_multiple_of(5) {
                -1
            } else {
                call as i32
            },
            tuning: -3.25 + (call % 7) as f32,
            velocity: (call % 9) as f32 / 8.,
            length: 9,
        }),
        Kind::NoteOff(Note {
            channel: 15,
            pitch: ((call * 3) % 128) as u8,
            note_id: call as i32,
            tuning: 0.5,
            velocity: 0.25,
            length: 0,
        }),
        Kind::SysEx(&SYSEX[..2 + (call % 4) as usize]),
        Kind::PolyPressure {
            channel: 2,
            pitch: 64,
            note_id: -1,
            pressure: (call % 5) as f32 / 4.,
        },
        Kind::ExpressionValue {
            expression_id: call % 8,
            note_id: call as i32,
            value: f64::from(call % 101) / 100.,
        },
        Kind::ExpressionText {
            expression_id: 100_008,
            note_id: call as i32,
            text: TEXTS[(call % 5) as usize],
        },
        Kind::Chord {
            root: (call % 128) as u8,
            bass: 57,
            mask: 0xffff - (call % 3) as u16,
            text: TEXTS[((call + 1) % 5) as usize],
        },
        Kind::Scale {
            root: 60,
            mask: 0xa5a5 ^ (call & 0xff) as u16,
            text: TEXTS[((call + 2) % 5) as usize],
        },
        Kind::ExpressionInteger {
            expression_id: 100_009,
            note_id: 0,
            value: u64::MAX - u64::from(call),
        },
    ];
    let events = kinds
        .into_iter()
        .enumerate()
        .map(|(i, kind)| Event {
            header: header(i as u32),
            kind,
        })
        .collect();
    let mut points = vec![Point {
        id: 1,
        offset: 0,
        value: f64::from(call % 17) / 16.,
    }];
    if frames > 1 {
        points.push(Point {
            id: 1,
            offset: frames - 1,
            value: 0.75,
        });
    }
    (events, points)
}

enum Work {
    Block(u32),
    Stop,
}

fn probe<S: Precision>(tables: Tables, path: &str) {
    let (class, mut builder) = builder::<S>(tables, path);
    builder
        .node(Vst3GraphNodeOptions {
            class_index: class,
            inputs: vec![],
            outputs: vec![BusRequest::active(STEREO)],
            event_inputs: 1,
            event_outputs: 1,
            lanes: Some(LaneOptions::maximum(2)),
        })
        .unwrap();
    let (control, mut nodes) = builder.build().expect("bank build");
    let node = nodes.pop().unwrap();
    assert_eq!(node.layout(), AudioLayout::STEREO_INSTRUMENT);
    let id = NodeId(7);
    let routes = [0, 1].map(|channel| Connection {
        source: Source::Node { id, channel },
        destination: Destination::Output(channel),
        gain: S::one(),
    });
    let mut graph = Graph::new(
        AudioLayout::STEREO_INSTRUMENT,
        vec![GraphNode {
            id,
            layout: node.layout(),
            processor: Box::new(node) as Box<dyn ravet_foundation::Processor<S> + Send>,
        }],
        &routes,
    )
    .expect("graph");
    graph
        .prepare(
            ProcessConfig::new(48000., FRAMES as usize, 0, AudioLayout::STEREO_INSTRUMENT).unwrap(),
        )
        .unwrap();
    let (work, inbox) = mpsc::channel::<Work>();
    let (reply, results) = mpsc::channel::<(GraphStatus, u64, Vec<Vec<S>>)>();
    let worker = std::thread::spawn(move || {
        let mut outs = vec![vec![S::default(); FRAMES as usize + 3]; 2];
        while let Ok(Work::Block(frames)) = inbox.recv() {
            outs.iter_mut().for_each(|o| o.fill(S::from_f64(9.)));
            let mut views: Vec<&mut [S]> = outs.iter_mut().map(|v| &mut v[..]).collect();
            let audio = AudioBuffer::separate(&[], &mut views, frames as usize).unwrap();
            let mut block = ProcessBlock::new(audio, &[], Transport::default());
            let mut status = GraphStatus::Continue;
            let counted = measure(|| status = graph.process(&mut block));
            let heap = counted.count_total + counted.count_current.unsigned_abs();
            reply.send((status, heap, outs.clone())).unwrap();
        }
        drop(graph);
    });
    let (mut digest, mut zero, mut events, mut points, mut impulses) = (0u32, 0, 0, 0, 0);
    for call in 0..BLOCKS {
        let frames = (call * 11) % (FRAMES + 1);
        zero += usize::from(frames == 0);
        let (input, input_points) = batch(call, frames);
        let sequence = control
            .submit(0, frames, |lane| {
                for event in &input {
                    lane.events.push(*event)?;
                }
                for point in &input_points {
                    lane.points.push(*point)?;
                }
                Ok(())
            })
            .expect("submit");
        assert_eq!(sequence, u64::from(call));
        work.send(Work::Block(frames)).unwrap();
        let (status, heap, outs) = results.recv().unwrap();
        assert_eq!(status, GraphStatus::Continue);
        assert_eq!(heap, 0, "Rust heap operations in measured Graph::process");
        // Independent expected audio and digest.
        let mut left = vec![0f64; frames as usize];
        let mut right = vec![0f64; frames as usize];
        let mut last = 0;
        for event in &input {
            let h = event_hash(event);
            digest = digest_step(digest, h);
            last = last.max(event.header.offset);
            if frames > 0 {
                left[event.header.offset as usize] += f64::from(code(&event.kind) + 1);
                right[event.header.offset as usize] += f64::from(h) / 1_048_576.;
            }
        }
        for point in &input_points {
            let h = point_hash(point);
            digest = digest_step(digest, h);
            if frames > 0 {
                left[point.offset as usize] += 17.;
                right[point.offset as usize] += f64::from(h) / 1_048_576.;
            }
        }
        for f in 0..frames as usize {
            // Sums of at most 11 20-bit fractions are exact in f32 and f64.
            assert_eq!(outs[0][f], S::from_f64(left[f]));
            assert_eq!(outs[1][f], S::from_f64(right[f]));
            impulses += usize::from(left[f] != 0.);
        }
        assert!(
            outs.iter()
                .all(|o| o[frames as usize..].iter().all(|v| *v == S::from_f64(9.)))
        );
        let checked = control
            .receive(0, |lane| {
                assert_eq!((lane.sequence, lane.frames), (u64::from(call), frames));
                let output = lane.output.expect("published rich output");
                let echoed: Vec<_> = output.events().collect();
                assert_eq!(echoed.len(), 10);
                for (event, out) in input.iter().zip(&echoed) {
                    let r = out.record();
                    assert_eq!(r.kind, code(&event.kind));
                    assert_eq!(
                        (r.bus, r.offset, r.flags),
                        (0, event.header.offset, event.header.flags)
                    );
                    assert_eq!(r.ppq_position, event.header.ppq_position);
                    match event.kind {
                        Kind::SysEx(bytes) => assert_eq!(out.payload(), bytes),
                        Kind::ExpressionText { text, .. }
                        | Kind::Chord { text, .. }
                        | Kind::Scale { text, .. } => {
                            assert_eq!(out.text().unwrap().collect::<String>(), text)
                        }
                        _ => assert!(out.payload().is_empty()),
                    }
                    match event.kind {
                        Kind::NoteOn(n) => assert_eq!(
                            (
                                r.channel,
                                r.pitch,
                                r.note_id,
                                r.note_length,
                                r.tuning,
                                r.velocity
                            ),
                            (
                                i32::from(n.channel),
                                i32::from(n.pitch),
                                n.note_id,
                                n.length,
                                n.tuning,
                                n.velocity
                            )
                        ),
                        Kind::NoteOff(n) => assert_eq!(
                            (r.channel, r.pitch, r.note_id, r.tuning, r.velocity),
                            (
                                i32::from(n.channel),
                                i32::from(n.pitch),
                                n.note_id,
                                n.tuning,
                                n.velocity
                            )
                        ),
                        Kind::PolyPressure {
                            channel,
                            pitch,
                            note_id,
                            pressure,
                        } => assert_eq!(
                            (r.channel, r.pitch, r.note_id, r.velocity),
                            (i32::from(channel), i32::from(pitch), note_id, pressure)
                        ),
                        Kind::ExpressionValue {
                            expression_id,
                            note_id,
                            value,
                        } => {
                            assert_eq!(
                                (r.expression_id, r.note_id, r.value),
                                (expression_id, note_id, value)
                            )
                        }
                        Kind::ExpressionText {
                            expression_id,
                            note_id,
                            ..
                        } => {
                            assert_eq!((r.expression_id, r.note_id), (expression_id, note_id))
                        }
                        Kind::Chord {
                            root, bass, mask, ..
                        } => assert_eq!(
                            (r.root, r.bass, r.mask),
                            (i32::from(root), i32::from(bass), u32::from(mask))
                        ),
                        Kind::Scale { root, mask, .. } => {
                            assert_eq!((r.root, r.mask), (i32::from(root), u32::from(mask)))
                        }
                        Kind::ExpressionInteger {
                            expression_id,
                            note_id,
                            value,
                        } => assert_eq!(
                            (r.expression_id, r.note_id, r.integer_value),
                            (expression_id, note_id, value)
                        ),
                        Kind::SysEx(_) => assert_eq!(r.data_type, 0),
                    }
                }
                let cc = echoed[9].record();
                assert_eq!(
                    (
                        cc.kind,
                        cc.controller,
                        cc.channel,
                        cc.midi_value,
                        cc.midi_value2,
                        cc.offset
                    ),
                    (65535, 16, 0, 9, 0, last)
                );
                assert_eq!(output.parameter_ids(), [2]);
                let point = output.parameter_points()[0];
                assert_eq!(output.parameter_points().len(), 1);
                assert_eq!(
                    (point.id, point.offset, point.value),
                    (2, frames.saturating_sub(1), f64::from(digest) / 1_048_576.)
                );
            })
            .expect("receive");
        assert!(checked.is_some(), "completed lane batch");
        events += input.len();
        points += input_points.len();
    }
    work.send(Work::Stop).unwrap();
    worker.join().expect("probe worker");
    assert_eq!(control.poll_returns().unwrap().returned, 1);
    assert_eq!(control.retire().unwrap().released, 1);
    println!(
        "PASS bank event probe {}bit: {BLOCKS} graph blocks ({zero} zero-frame, frames 0..64), {events} events of all nine kinds + {points} Marker points through rich lanes, {impulses} nonzero impulse samples exact, echo/CC/digest parameter exact, 0 Rust heap operations in measured Graph::process",
        S::BITS
    );
}

/// Run all reference-module bank scenarios. Returns 0 on success, 1 on any
/// failure (the panic message is printed). Must run on the platform main
/// thread with exactly paired, process-static native provider tables.
///
/// # Safety
/// All table pointers are the caller-linked provider's static tables, and both
/// paths are NUL-terminated UTF-8 absolute paths of trusted reference modules.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn ravet_vst3_reference_bank_run(
    instance: *const instances::RawProvider,
    parameters: *const parameters::RawProvider,
    processing: *const processing::RawProvider,
    hosted: *const hosted_audio::RawProvider,
    sidechain_path: *const c_char,
    probe_path: *const c_char,
) -> i32 {
    let outcome = catch_unwind(AssertUnwindSafe(|| {
        // SAFETY: caller contract above.
        let (tables, sidechain_path, probe_path) = unsafe {
            (
                Tables {
                    instance: instance.as_ref().expect("instance table"),
                    parameters: parameters.as_ref().expect("parameter table"),
                    processing: processing.as_ref().expect("processing table"),
                    hosted: hosted.as_ref().expect("hosted table"),
                },
                CStr::from_ptr(sidechain_path)
                    .to_str()
                    .expect("UTF-8 path")
                    .to_owned(),
                CStr::from_ptr(probe_path)
                    .to_str()
                    .expect("UTF-8 path")
                    .to_owned(),
            )
        };
        sidechain::<f32>(tables, &sidechain_path);
        sidechain::<f64>(tables, &sidechain_path);
        probe::<f32>(tables, &probe_path);
        probe::<f64>(tables, &probe_path);
        let report = reclaim_vst3_graph_banks(None).expect("final reclaim");
        assert_eq!(report.released, 0, "every bank was already retired");
        println!("4 actual reference-module Graph bank scenarios passed");
    }));
    i32::from(outcome.is_err())
}
