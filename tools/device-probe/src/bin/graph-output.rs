//! Native sine graph worker: silent devices and explicitly offline timed notes.
use clack_host::prelude::*;
use ravet_audio_file::audio_ring;
use ravet_device::{
    PreparedGraphOutput, PreparedOutput, RingOutput, cpal, open_output, output_device,
};
use ravet_foundation::{AudioLayout, Event, EventData, ProcessConfig, Tail, Transport};
use ravet_graph::{Connection, Destination, Graph, GraphNode, NodeId, Source};
use ravet_host::{
    ClapLibrary, HostRequests, HostedGraphNodeBuilder, HostedGraphNodeOptions,
    HostedGraphNoteDialect, HostedGraphNoteInput, RequestHost, prepare_hosted_graph_node,
};
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};
use std::time::{Duration, Instant};
#[allow(unsafe_code)]
fn load(path: &std::path::Path) -> Result<ClapLibrary, ravet_host::HostError> {
    // SAFETY: explicitly selected trusted compliant public sine bundle.
    unsafe { ClapLibrary::load(path) }
}
struct Worker {
    stop: Arc<AtomicBool>,
    handle: Option<std::thread::JoinHandle<(PreparedGraphOutput, u64)>>,
}
impl Worker {
    fn finish(&mut self) -> Result<(PreparedGraphOutput, u64), Box<dyn std::error::Error>> {
        self.stop.store(true, Ordering::Release);
        self.handle
            .take()
            .ok_or("worker already joined")?
            .join()
            .map_err(|_| "graph worker panicked".into())
    }
}
impl Drop for Worker {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Release);
        if let Some(handle) = self.handle.take()
            && let Ok((prepared, _)) = handle.join()
        {
            drop(prepared.into_graph());
        }
    }
}
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args: Vec<_> = std::env::args().skip(1).collect();
    let notes = args.first().is_some_and(|arg| arg == "--offline-notes");
    if notes {
        args.remove(0);
    }
    if notes && args.len() != 1 {
        return Err("--offline-notes accepts only an offline bundle path".into());
    }
    if args.len() != 1 && args.len() != 5 {
        return Err("usage: graph-output [--offline-notes] <trusted RAVET Sine CLAP> [<explicit device ID> <rate> <channels> <seconds1..30>]".into());
    }
    let live = args.len() == 5;
    let rate: u32 = if live { args[2].parse()? } else { 48000 };
    let channels: usize = if live { args[3].parse()? } else { 2 };
    let seconds: u64 = if live { args[4].parse()? } else { 0 };
    if !(2..=64).contains(&channels) || (live && !(1..=30).contains(&seconds)) {
        return Err("silent profile requires 2..64 channels and 1..30 live seconds".into());
    }
    let library = load(std::path::Path::new(&args[0]))?;
    let instance = PluginInstance::<RequestHost>::new(
        |_| HostRequests::new(),
        |_| (),
        library.entry(),
        c"org.ravet.sine",
        &HostInfo::new("Silent graph device probe", "RAVET", "", "0.1")?,
    )?;
    let event_capacity = if notes { 2 } else { 0 };
    let node_config = ProcessConfig::new(
        f64::from(rate),
        256,
        event_capacity,
        AudioLayout::STEREO_INSTRUMENT,
    )?;
    let options = HostedGraphNodeOptions {
        configuration: node_config,
        declared_latency: 0,
        declared_tail: Tail::Infinite,
    };
    let (mut control, node) = if notes {
        HostedGraphNodeBuilder::new(options)
            .with_note_input(HostedGraphNoteInput {
                port_index: 0,
                dialect: HostedGraphNoteDialect::Midi,
            })
            .prepare(instance, |_, _| ())?
    } else {
        prepare_hosted_graph_node(instance, options, |_, _| ())?
    };
    let layout = AudioLayout::new(0, channels)?;
    let routes: Vec<_> = (0..2)
        .map(|channel| Connection {
            source: Source::Node {
                id: NodeId(1),
                channel,
            },
            destination: Destination::Output(channel),
            gain: 1.,
        })
        .collect();
    let graph = Box::new(Graph::new(
        layout,
        vec![GraphNode {
            id: NodeId(1),
            layout: AudioLayout::STEREO_INSTRUMENT,
            processor: Box::new(node),
        }],
        &routes,
    )?);
    let config = ProcessConfig::new(f64::from(rate), 256, event_capacity, layout)?;
    let (producer, consumer) =
        audio_ring(channels, 4096).map_err(|error| format!("ring: {error:?}"))?;
    let mut prepared = PreparedGraphOutput::new(graph, producer, config)?;
    let playback = RingOutput::new(consumer, f64::from(rate))?;
    let monitor = playback.monitor();
    let stop = Arc::new(AtomicBool::new(false));
    let audio_stop = stop.clone();
    let (ready, wait) = std::sync::mpsc::sync_channel(0);
    let handle = std::thread::spawn(move || {
        let mut rendered = 0;
        {
            let mut session = prepared.start_audio().unwrap();
            let changes = [
                Event {
                    frame_offset: 16,
                    data: EventData::NoteOn {
                        channel: 0,
                        key: 69,
                        velocity: 1.,
                    },
                },
                Event {
                    frame_offset: 128,
                    data: EventData::NoteOff {
                        channel: 0,
                        key: 69,
                        velocity: 0.,
                    },
                },
            ];
            for index in 0..16 {
                let counts = allocation_counter::measure(|| {
                    let progress = session
                        .pump_events(
                            if notes && index == 0 { &changes } else { &[] },
                            Transport::default(),
                        )
                        .unwrap();
                    rendered += progress.rendered_frames as u64;
                });
                assert_eq!(counts.count_total, 0);
                assert_eq!(counts.count_current, 0);
            }
            ready.send(()).unwrap();
            while !audio_stop.load(Ordering::Acquire) {
                let mut full = false;
                let counts = allocation_counter::measure(|| {
                    let progress = session.pump(Transport::default()).unwrap();
                    rendered += progress.rendered_frames as u64;
                    full = progress.published_frames == 0;
                });
                assert_eq!(counts.count_total, 0);
                assert_eq!(counts.count_current, 0);
                if full {
                    std::thread::sleep(Duration::from_millis(1));
                }
            }
        }
        (prepared, rendered)
    });
    let mut worker = Worker {
        stop,
        handle: Some(handle),
    };
    wait.recv()?;
    if live {
        let device = output_device(Some(&args[1]))?.ok_or("explicit output device unavailable")?;
        let stream = open_output(
            &device,
            cpal::StreamConfig {
                channels: channels as u16,
                sample_rate: rate,
                buffer_size: cpal::BufferSize::Default,
            },
            256,
            playback,
        )?;
        stream.play()?;
        std::thread::sleep(Duration::from_secs(seconds));
        stream.pause()?;
        let stats = stream.statistics();
        println!("device {stats:?}; queue {:?}", monitor.statistics());
        drop(stream);
        if stats.callbacks == 0
            || stats.frames == 0
            || stats.processor_faults != 0
            || stats.backend_errors != 0
            || stats.panics != 0
        {
            return Err("device callback validation failed".into());
        }
    } else {
        let mut renderer = PreparedOutput::new(playback, rate, channels, 256)?;
        let mut output = vec![1.; channels * 256];
        let mut heard = false;
        for _ in 0..1000 {
            let deadline = Instant::now() + Duration::from_secs(2);
            let previous = monitor.statistics().source_frames;
            loop {
                let counts = allocation_counter::measure(|| renderer.render(&mut output));
                assert_eq!(counts.count_total, 0);
                assert_eq!(counts.count_current, 0);
                assert!(output.iter().all(|sample| sample.is_finite()));
                let consumed = monitor.statistics().source_frames - previous;
                if notes && consumed > 0 {
                    for (frame, samples) in output.chunks_exact(channels).enumerate() {
                        assert_eq!(samples[0], samples[1]);
                        let position = previous + frame as u64;
                        if !(16..2528).contains(&position) {
                            assert!(samples.iter().all(|sample| *sample == 0.));
                        } else if samples[0].abs() > 0.001 {
                            heard = true;
                        }
                    }
                } else {
                    assert!(output.iter().all(|sample| *sample == 0.));
                }
                if monitor.statistics().source_frames >= previous + 256 {
                    break;
                }
                if Instant::now() >= deadline {
                    return Err("worker starvation".into());
                }
                std::thread::sleep(Duration::from_millis(1));
            }
        }
        assert_eq!(heard, notes);
        assert_eq!(renderer.statistics().processor_faults, 0);
        println!(
            "offline device {:?}; queue {:?}",
            renderer.statistics(),
            monitor.statistics()
        );
        drop(renderer);
    }
    let (prepared, rendered) = worker.finish()?;
    drop(prepared.into_graph());
    assert!(control.reclaim());
    assert_eq!(control.statistics().processed_frames, rendered);
    assert_eq!(control.statistics().backend_failures, 0);
    let instance = control.into_instance().ok().ok_or("native main return")?;
    assert!(!instance.is_active());
    assert!(monitor.statistics().source_frames > 0);
    println!(
        "PASS native graph worker, {rendered} rendered frames, bounded queue, owner stop/main return (live={live}, notes={notes}); underflow is counted, no clock/deadline guarantee"
    );
    Ok(())
}
