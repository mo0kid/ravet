use ravet_delay_plugin::DelayPlugin;
use ravet_delay_plugin::dsp::{
    self, CONTROLS, DEFAULTS, DIVISIONS, MAXIMUM_TIME_MS, SyncedDelay, synced_time_ms,
};
use ravet_foundation::{
    AudioBuffer, AudioLayout, Event, EventData, EventError, ParameterId, ProcessBlock,
    ProcessConfig, ProcessError, ProcessStatus, Processor, Tail, Transport,
};
use ravet_plugin::{PluginExport, state};

const RATE: f64 = 8000.0;

fn parameter(frame_offset: usize, id: ParameterId, value: f32) -> Event {
    Event {
        frame_offset,
        data: EventData::ParameterValue { id, value },
    }
}

fn tempo(bpm: f64) -> Transport {
    Transport {
        tempo: Some(bpm),
        ..Transport::default()
    }
}

/// A prepared delay with the given controls (ID, value) changed from defaults.
fn prepared(changes: &[(ParameterId, f32)]) -> SyncedDelay {
    let mut delay = SyncedDelay::default();
    for (id, value) in changes {
        delay.set(*id, *value).unwrap();
    }
    delay
        .prepare(ProcessConfig::new(RATE, 4096, 1024, AudioLayout::STEREO).unwrap())
        .unwrap();
    delay
}

fn process(
    delay: &mut SyncedDelay,
    input: [&[f32]; 2],
    events: &[Event],
    transport: Transport,
) -> ([Vec<f32>; 2], ProcessStatus) {
    let frames = input[0].len();
    let mut output = [vec![99.0; frames], vec![99.0; frames]];
    let mut outputs = output.each_mut().map(|c| &mut c[..]);
    let buffer = AudioBuffer::separate(&input, &mut outputs, frames).unwrap();
    let status = delay.process(&mut ProcessBlock::new(buffer, events, transport));
    (output, status)
}

fn impulse(frames: usize) -> Vec<f32> {
    let mut samples = vec![0.0; frames];
    samples[0] = 1.0;
    samples
}

/// Frames at which the channel's magnitude exceeds `threshold`.
fn peaks(samples: &[f32], threshold: f32) -> Vec<usize> {
    (0..samples.len())
        .filter(|&i| samples[i].abs() > threshold)
        .collect()
}

#[test]
fn product_matches_the_processor() {
    let product = DelayPlugin::PRODUCT.resolve().unwrap();
    let parameters = product.product().parameters();
    assert_eq!(parameters.len(), CONTROLS);
    for (index, (parameter, id)) in parameters.iter().zip(product.parameter_ids()).enumerate() {
        assert_eq!(id.0 as usize, index + 1, "{}", parameter.key());
        assert_eq!(parameter.default(), DEFAULTS[index], "{}", parameter.key());
        assert_eq!(parameter.range().minimum(), dsp::RANGES[index].0);
        assert_eq!(parameter.range().maximum(), dsp::RANGES[index].1);
    }
    assert_eq!(product.bypass_index(), Some(dsp::BYPASS.0 as usize - 1));
    assert_eq!(&product.product().state_format().tag, b"RVTDLY01");
}

#[test]
fn saved_state_round_trips() {
    let product = DelayPlugin::PRODUCT.resolve().unwrap();
    let values = [820.0, 70.0, 50.0, 0.0, 1.0, 0.0, 9.0, 2500.0];
    let bytes = state::encode(&product, &values, &[]).unwrap();
    let restored = state::restore(&product, &bytes, &state::NoHooks).unwrap();
    assert_eq!(restored.values, values);
    let mut delay = SyncedDelay::default();
    for (index, value) in restored.values.iter().enumerate() {
        delay.set(ParameterId(index as u32 + 1), *value).unwrap();
    }
    assert_eq!(delay.controls(), values);
}

#[test]
fn free_time_places_the_first_repeat() {
    let mut delay = prepared(&[
        (dsp::SYNC, 0.0),
        (dsp::TIME, 10.0),
        (dsp::FEEDBACK, 0.0),
        (dsp::MIX, 100.0),
    ]);
    let input = impulse(200);
    let (output, status) = process(&mut delay, [&input, &input], &[], Transport::default());
    assert_eq!(status, ProcessStatus::Continue);
    for channel in &output {
        assert_eq!(peaks(channel, 1e-6), [80]);
        assert!((channel[80] - 1.0).abs() < 1e-6);
    }
}

#[test]
fn sync_follows_the_host_tempo() {
    // Dotted eighth at 120 BPM is 375 ms; eighth is 250 ms.
    assert_eq!(synced_time_ms(4, 120.0), 375.0);
    assert_eq!(synced_time_ms(6, 120.0), 250.0);
    // A whole note at 30 BPM (8 s) is clamped to the longest delay.
    assert_eq!(synced_time_ms(0, 30.0), MAXIMUM_TIME_MS);
    assert_eq!(DIVISIONS[4].0, "1/8.");

    let mut delay = prepared(&[
        (dsp::DIVISION, 6.0),
        (dsp::FEEDBACK, 0.0),
        (dsp::MIX, 100.0),
    ]);
    // Without a tempo, the millisecond time applies.
    assert_eq!(delay.target_time_ms(), DEFAULTS[0]);
    let silence = vec![0.0; 64];
    process(&mut delay, [&silence, &silence], &[], tempo(120.0));
    assert_eq!(delay.tempo(), Some(120.0));
    assert_eq!(delay.target_time_ms(), 250.0);
    // The last tempo is kept while the host reports none.
    process(&mut delay, [&silence, &silence], &[], Transport::default());
    assert_eq!(delay.target_time_ms(), 250.0);
    // Invalid tempos are ignored.
    process(&mut delay, [&silence, &silence], &[], tempo(f64::NAN));
    process(&mut delay, [&silence, &silence], &[], tempo(0.0));
    assert_eq!(delay.tempo(), Some(120.0));

    // After reset the glide is complete: the repeat lands at 250 ms.
    delay.reset();
    let input = impulse(2100);
    let (output, _) = process(&mut delay, [&input, &input], &[], tempo(120.0));
    assert_eq!(peaks(&output[0], 1e-6), [2000]);

    // Turning sync off returns to the millisecond time.
    delay.set(dsp::SYNC, 0.0).unwrap();
    assert_eq!(delay.target_time_ms(), DEFAULTS[0]);
}

#[test]
fn tempo_changes_glide_without_jumps() {
    let mut delay = prepared(&[
        (dsp::DIVISION, 6.0),
        (dsp::FEEDBACK, 60.0),
        (dsp::MIX, 50.0),
    ]);
    let tone: Vec<f32> = (0..256).map(|i| (i as f32 * 0.07).sin() * 0.5).collect();
    let mut previous = 0.0f32;
    let mut largest_step = 0.0f32;
    for block in 0..64 {
        let bpm = if block < 32 { 120.0 } else { 90.0 };
        let (output, status) = process(&mut delay, [&tone, &tone], &[], tempo(bpm));
        assert_eq!(status, ProcessStatus::Continue);
        for sample in &output[0] {
            assert!(sample.is_finite());
            largest_step = largest_step.max((sample - previous).abs());
            previous = *sample;
        }
    }
    assert!(largest_step < 0.25, "step {largest_step}");
}

#[test]
fn results_do_not_depend_on_block_size() {
    let input: Vec<f32> = (0..3000)
        .map(|i| if i % 700 == 0 { 1.0 } else { 0.0 })
        .collect();
    let events = [
        parameter(0, dsp::SYNC, 0.0),
        parameter(0, dsp::TIME, 40.0),
        parameter(900, dsp::FEEDBACK, 80.0),
        parameter(1300, dsp::PING_PONG, 1.0),
        parameter(1700, dsp::TONE, 1200.0),
        parameter(2100, dsp::TIME, 75.0),
        parameter(2500, dsp::BYPASS, 1.0),
    ];
    let render = |block: usize| {
        let mut delay = prepared(&[]);
        let mut result = [Vec::new(), Vec::new()];
        for start in (0..input.len()).step_by(block) {
            let end = (start + block).min(input.len());
            let local: Vec<_> = events
                .iter()
                .filter(|e| (start..end).contains(&e.frame_offset))
                .map(|e| Event {
                    frame_offset: e.frame_offset - start,
                    ..*e
                })
                .collect();
            let clip = &input[start..end];
            let (output, status) = process(&mut delay, [clip, clip], &local, tempo(100.0));
            assert_eq!(status, ProcessStatus::Continue);
            for c in 0..2 {
                result[c].extend_from_slice(&output[c]);
            }
        }
        result
    };
    let whole = render(3000);
    assert_eq!(render(1), whole);
    assert_eq!(render(64), whole);
    assert_eq!(render(517), whole);
}

#[test]
fn ping_pong_alternates_channels() {
    let mut delay = prepared(&[
        (dsp::SYNC, 0.0),
        (dsp::TIME, 10.0),
        (dsp::FEEDBACK, 50.0),
        (dsp::MIX, 100.0),
        (dsp::PING_PONG, 1.0),
        (dsp::TONE, 20_000.0),
    ]);
    let left = impulse(400);
    let right = vec![0.0; 400];
    let (output, _) = process(&mut delay, [&left, &right], &[], Transport::default());
    assert!(output[0][80] > 0.99);
    assert!(output[1][80].abs() < 1e-6);
    assert!(output[1][160] > 0.45);
    assert!(output[0][160].abs() < 1e-3);
    assert!(output[0][240] > 0.2);
}

#[test]
fn bypass_crossfades_to_dry() {
    let mut delay = prepared(&[(dsp::BYPASS, 1.0)]);
    let input: Vec<f32> = (0..512).map(|i| ((i as f32) * 0.1).sin()).collect();
    let (output, _) = process(&mut delay, [&input, &input], &[], Transport::default());
    assert_eq!(output[0], input);
    // Leaving bypass ramps over 5 ms (40 samples at 8 kHz) to the mixed signal.
    let events = [parameter(0, dsp::BYPASS, 0.0)];
    let (output, _) = process(&mut delay, [&input, &input], &events, Transport::default());
    // Before the first repeat the wet signal is silent, so the output settles
    // at the dry level reduced by the mix.
    let dry = 1.0 - DEFAULTS[2] / 100.0;
    assert!((output[0][1] - input[1]).abs() < 0.05);
    assert!((output[0][100] - input[100] * dry).abs() < 1e-5);
}

#[test]
fn invalid_values_fault_without_applying_anything() {
    let mut delay = prepared(&[]);
    let before = delay.controls();
    let input = vec![0.5; 32];
    for (id, value) in [
        (dsp::DIVISION, 2.5),
        (dsp::DIVISION, 12.0),
        (dsp::SYNC, 0.5),
        (dsp::TIME, 0.0),
        (dsp::FEEDBACK, f32::INFINITY),
    ] {
        let events = [parameter(0, dsp::MIX, 80.0), parameter(4, id, value)];
        let (output, status) = process(&mut delay, [&input, &input], &events, Transport::default());
        assert_eq!(
            status,
            ProcessStatus::Fault(ProcessError::Events(EventError::InvalidValue { index: 1 }))
        );
        assert!(output.iter().flatten().all(|s| *s == 0.0));
        assert_eq!(delay.controls(), before);
        assert!(delay.set(id, value).is_err());
    }
    assert!(delay.set(ParameterId(99), 1.0).is_err());
}

#[test]
fn tail_is_finite_only_without_feedback() {
    let mut delay = prepared(&[]);
    assert_eq!(delay.tail(), Tail::Infinite);
    delay.set(dsp::FEEDBACK, 0.0).unwrap();
    assert_eq!(delay.tail(), Tail::Samples(16001));
}

#[test]
fn mono_layout_and_large_blocks() {
    let mut delay = SyncedDelay::default();
    delay.set(dsp::SYNC, 0.0).unwrap();
    delay.set(dsp::TIME, 2000.0).unwrap();
    delay.set(dsp::FEEDBACK, 0.0).unwrap();
    delay.set(dsp::MIX, 100.0).unwrap();
    delay
        .prepare(ProcessConfig::new(RATE, 32768, 0, AudioLayout::MONO).unwrap())
        .unwrap();
    let input = impulse(20000);
    let mut output = vec![0.0; 20000];
    let inputs = [&input[..]];
    let mut outputs = [&mut output[..]];
    let buffer = AudioBuffer::separate(&inputs, &mut outputs, 20000).unwrap();
    let status = delay.process(&mut ProcessBlock::new(buffer, &[], Transport::default()));
    assert_eq!(status, ProcessStatus::Continue);
    assert_eq!(peaks(&output, 1e-6), [16000]);
}

#[test]
fn processing_allocates_nothing() {
    let mut delay = prepared(&[(dsp::FEEDBACK, 95.0), (dsp::PING_PONG, 1.0)]);
    let input = [[0.1f32; 256], [-0.1; 256]];
    let inputs = input.each_ref().map(|c| &c[..]);
    let mut output = [[0.0f32; 256]; 2];
    let measured = allocation_counter::measure(|| {
        for i in 0..1000 {
            let events = [
                parameter(0, dsp::DIVISION, (i % DIVISIONS.len()) as f32),
                parameter(17, dsp::SYNC, (i % 2) as f32),
                parameter(100, dsp::TIME, 1.0 + (i % 2000) as f32),
                parameter(200, dsp::TONE, 500.0 + (i % 100) as f32 * 100.0),
            ];
            let mut outputs = output.each_mut().map(|c| &mut c[..]);
            let buffer = AudioBuffer::separate(&inputs, &mut outputs, 256).unwrap();
            let transport = tempo(60.0 + (i % 120) as f64);
            assert_eq!(
                delay.process(&mut ProcessBlock::new(buffer, &events, transport)),
                ProcessStatus::Continue
            );
        }
        delay.reset();
    });
    assert_eq!(measured.count_total, 0);
    assert!(output.iter().flatten().all(|s| s.is_finite()));
}
