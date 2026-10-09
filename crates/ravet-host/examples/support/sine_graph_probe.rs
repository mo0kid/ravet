use clack_host::prelude::*;
use ravet_foundation::{
    AudioBuffer, AudioLayout, Event, EventData, ProcessBlock, ProcessConfig, ProcessStatus,
    Processor, Tail, Transport,
};
use ravet_host::{
    HostedGraphNodeBuilder, HostedGraphNodeOptions, HostedGraphNoteDialect, HostedGraphNoteInput,
    RequestHost,
};

/// Synthetic offline instrument probe: no device or host session is opened.
pub fn run(instance: PluginInstance<RequestHost>) -> Result<(), Box<dyn std::error::Error>> {
    let options = HostedGraphNodeOptions {
        configuration: ProcessConfig::new(48000., 4096, 2, AudioLayout::STEREO_INSTRUMENT)?,
        declared_latency: 0,
        declared_tail: Tail::Infinite,
    };
    let (mut control, mut node) = HostedGraphNodeBuilder::new(options)
        .with_note_input(HostedGraphNoteInput {
            port_index: 0,
            dialect: HostedGraphNoteDialect::Midi,
        })
        .prepare(instance, |_, _| ())?;
    node.prepare(options.configuration)?;
    let mut node = std::thread::spawn(move || {
        node.start_audio().unwrap();
        let stream = [
            Event {
                frame_offset: 16,
                data: EventData::NoteOn {
                    channel: 0,
                    key: 69,
                    velocity: 1.,
                },
            },
            Event {
                frame_offset: 512,
                data: EventData::NoteOff {
                    channel: 0,
                    key: 69,
                    velocity: 0.,
                },
            },
        ];
        let mut output = [[0.; 4096]; 2];
        let counts = allocation_counter::measure(|| {
            for _ in 0..1000 {
                let mut outputs = output.each_mut().map(|channel| channel.as_mut_slice());
                let audio = AudioBuffer::separate(&[], &mut outputs, 4096).unwrap();
                assert_eq!(
                    node.process(&mut ProcessBlock::new(audio, &stream, Transport::default())),
                    ProcessStatus::Continue
                );
                assert_eq!(output[0], output[1]);
                assert!(output[0].iter().all(|sample| sample.is_finite()));
                assert!(output[0][..16].iter().all(|sample| *sample == 0.));
                assert!(output[0][32..512].iter().any(|sample| sample.abs() > 0.001));
                // 50ms release at 48kHz, completed before the next held note.
                assert!(output[0][2912..].iter().all(|sample| *sample == 0.));
            }
        });
        assert_eq!(counts.count_total, 0);
        assert_eq!(counts.count_current, 0);
        node.stop_audio();
        node
    })
    .join()
    .map_err(|_| "sine graph audio owner panicked")?;
    node.deactivate();
    assert!(control.reclaim());
    assert_eq!(control.statistics().processed_blocks, 1000);
    assert_eq!(control.statistics().processed_frames, 4096000);
    assert_eq!(control.statistics().backend_failures, 0);
    drop(node);
    let instance = control.into_instance().ok().ok_or("inactive main return")?;
    assert!(!instance.is_active());
    println!(
        "PASS 1000 native f32 no-input sine node blocks/4096000 frames: timed MIDI notes, stereo equality, finite sound, release silence, zero measured allocations and main retirement"
    );
    Ok(())
}
