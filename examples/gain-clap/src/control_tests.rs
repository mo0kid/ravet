//! Local host pointers stay valid for the entire callback fixture.
use super::*;
use clap_sys::host::clap_host;
use editor::tests::Sink;
use std::{ptr::NonNull, sync::atomic::AtomicUsize};

unsafe extern "C" fn request(host: *const clap_host) {
    // SAFETY: raw_host receives this live AtomicUsize and the fixture owns it
    // until every main/audio handle and callback has been dropped.
    let counter = unsafe { &*((*host).host_data.cast::<AtomicUsize>()) };
    counter.fetch_add(1, Ordering::Relaxed);
}
fn raw_host(counter: &AtomicUsize) -> clap_host {
    clap_host {
        clap_version: clap_sys::version::CLAP_VERSION,
        host_data: (counter as *const AtomicUsize).cast_mut().cast(),
        name: c"Control test".as_ptr(),
        vendor: c"RAVET".as_ptr(),
        url: c"".as_ptr(),
        version: c"1".as_ptr(),
        get_extension: None,
        request_restart: None,
        request_process: Some(request),
        request_callback: None,
    }
}
fn shared() -> GainShared {
    GainShared {
        gain: SharedGain::new(),
        meters: GainMeterMailbox::default(),
        edits: editor::Edits::default(),
        fault: AtomicU32::new(0),
    }
}

#[test]
fn main_control_edits_reject_bad_input_and_flush_to_complete_host_gestures() {
    let counter = AtomicUsize::new(0);
    let mut host = raw_host(&counter);
    let shared = shared();
    // SAFETY: fixture host/handlers are live on this test main thread.
    let main = GainMain {
        parameters: ClapParameterRegistration::new(parameter_schema(), &PARAMETER_OPTIONS).unwrap(),
        host: unsafe { HostMainThreadHandle::from_raw(NonNull::from(&mut host)) },
        host_params: None,
        shared: &shared,
        channels: Cell::new(2),
        events: RefCell::new(ParameterEvents::new()),
        #[cfg(all(target_os = "macos", feature = "native-editor"))]
        gui: RefCell::new(None),
    };
    assert_eq!(
        main.edit_parameter(GAIN_ID, f32::NAN),
        Err(EditError::InvalidValue)
    );
    assert_eq!(
        main.edit_parameter(ravet_foundation::ParameterId(99), 1.0),
        Err(EditError::UnknownParameter)
    );
    assert_eq!(counter.load(Ordering::Relaxed), 0);
    main.edit_parameter(GAIN_ID, 0.5).unwrap();
    main.edit_parameter(BYPASS_ID, 0.75).unwrap();
    assert_eq!(counter.load(Ordering::Relaxed), 2);
    assert_eq!(main.parameter_state().values(), &[1.0, 0.0]);
    let mut sink = Sink::new(128);
    main.flush(
        &InputEvents::empty(),
        &mut OutputEvents::from_buffer(&mut sink),
    );
    assert_eq!(main.parameter_state().values(), &[0.5, 1.0]);
    assert_eq!(sink.length, 6);
    assert_eq!(sink.events[1], (1, CLAP_GAIN_ID, 0.5));
    assert_eq!(sink.events[4], (1, CLAP_BYPASS_ID, 1.0));
    assert_eq!(
        main.begin_parameter_gesture(ravet_foundation::ParameterId(99)),
        Err(EditError::UnknownParameter)
    );
    main.begin_parameter_gesture(GAIN_ID).unwrap();
    assert_eq!(
        main.edit_parameter(BYPASS_ID, 0.0),
        Err(EditError::GestureActive)
    );
    assert_eq!(
        main.update_parameter_gesture(BYPASS_ID, 0.0),
        Err(EditError::NoGesture)
    );
    assert_eq!(
        main.update_parameter_gesture(GAIN_ID, f32::INFINITY),
        Err(EditError::InvalidValue)
    );
    main.update_parameter_gesture(GAIN_ID, 2.0).unwrap();
    let mut live = Sink::new(128);
    main.flush(
        &InputEvents::empty(),
        &mut OutputEvents::from_buffer(&mut live),
    );
    assert_eq!(live.length, 2);
    assert_eq!(main.parameter_state().values(), &[2.0, 1.0]);
    main.update_parameter_gesture(GAIN_ID, 9.0).unwrap(); // canonical clamp
    main.end_parameter_gesture().unwrap();
    main.flush(
        &InputEvents::empty(),
        &mut OutputEvents::from_buffer(&mut live),
    );
    assert_eq!(live.length, 4);
    assert_eq!(live.events[2], (1, CLAP_GAIN_ID, 4.0));
    assert_eq!(live.events[3], (2, CLAP_GAIN_ID, 0.0));
    assert_eq!(counter.load(Ordering::Relaxed), 6);
}

#[test]
fn audio_control_flush_retains_backpressure_smooths_and_has_no_heap_activity() {
    let counter = AtomicUsize::new(0);
    let mut host = raw_host(&counter);
    let shared = shared();
    let bridge = EffectBridge::prepare(
        GainProcessor::default(),
        ProcessConfig::new(1000.0, 8, MAXIMUM_EVENTS, AudioLayout::STEREO).unwrap(),
    )
    .unwrap();
    // SAFETY: this fixture serializes audio/control calls and keeps host live.
    let mut audio = GainAudio {
        host: unsafe { HostAudioProcessorHandle::from_raw(NonNull::from(&mut host)) },
        host_params: None,
        shared: &shared,
        bridge,
        generation: 0,
    };
    let mut sink = Sink::new(0);
    shared.edits.push(CLAP_GAIN_ID, 0.0, 0).unwrap();
    audio.flush(
        &InputEvents::empty(),
        &mut OutputEvents::from_buffer(&mut sink),
    );
    assert_eq!(shared.parameters().values(), &[1.0, 0.0]);
    assert_eq!(counter.load(Ordering::Relaxed), 1);
    sink.limit = 128;
    audio.flush(
        &InputEvents::empty(),
        &mut OutputEvents::from_buffer(&mut sink),
    );
    assert_eq!(shared.parameters().values(), &[0.0, 0.0]);
    assert_eq!(audio.bridge.processor().effective_gain(), 1.0); // normal smoothing
    let info = allocation_counter::measure(|| {
        for _ in 0..1000 {
            shared.edits.push(CLAP_GAIN_ID, 0.5, 0).unwrap();
            shared.edits.push(CLAP_BYPASS_ID, 1.0, 0).unwrap();
            audio.flush(&InputEvents::empty(), &mut OutputEvents::void());
            assert_eq!(shared.parameters().values(), &[0.5, 1.0]);
            shared.edits.begin(CLAP_GAIN_ID, 0).unwrap();
            shared.edits.update(CLAP_GAIN_ID, 0.25).unwrap();
            audio.flush(&InputEvents::empty(), &mut OutputEvents::void());
            assert_eq!(shared.parameters().values(), &[0.25, 1.0]);
            shared.edits.update(CLAP_GAIN_ID, 0.75).unwrap();
            shared.edits.end().unwrap();
            audio.flush(&InputEvents::empty(), &mut OutputEvents::void());
            assert_eq!(shared.parameters().values(), &[0.75, 1.0]);
            audio.reset();
        }
    });
    assert_eq!(info.count_total, 0);
    assert_eq!(info.count_current, 0);
    assert_eq!(info.bytes_current, 0);
    assert_eq!(shared.meters().unwrap(), GainMeters::default());
}

fn with_audio(samples: &mut [[f32; 8]; 2], operation: impl FnOnce(Audio<'_>)) {
    use clap_sys::audio_buffer::clap_audio_buffer;
    let mut input_pointers = [samples[0].as_mut_ptr(), samples[1].as_mut_ptr()];
    let mut output_pointers = input_pointers;
    let descriptor = clap_audio_buffer {
        data32: input_pointers.as_mut_ptr(),
        data64: std::ptr::null_mut(),
        channel_count: 2,
        latency: 0,
        constant_mask: 0,
    };
    let inputs = [descriptor];
    let mut outputs = [clap_audio_buffer {
        data32: output_pointers.as_mut_ptr(),
        ..descriptor
    }];
    // SAFETY: pointer arrays/descriptors and two disjoint channels outlive the
    // operation. Only matching input/output channel aliasing is permitted.
    operation(unsafe { Audio::from_raw_buffers(&inputs, &mut outputs, 8) });
}

#[test]
fn process_publishes_meters_and_editor_edits_survive_independent_host_faults() {
    let counter = AtomicUsize::new(0);
    let mut host = raw_host(&counter);
    let shared = shared();
    let bridge = EffectBridge::prepare(
        GainProcessor::default(),
        ProcessConfig::new(1000.0, 8, MAXIMUM_EVENTS, AudioLayout::STEREO).unwrap(),
    )
    .unwrap();
    // SAFETY: fixture host outlives all serialized callbacks on this thread.
    let mut audio = GainAudio {
        host: unsafe { HostAudioProcessorHandle::from_raw(NonNull::from(&mut host)) },
        host_params: None,
        shared: &shared,
        bridge,
        generation: 0,
    };
    shared.gain.restore(0.5, false);
    let mut samples = [[0.5; 8], [-1.0; 8]];
    with_audio(&mut samples, |buffers| {
        audio
            .process(
                Process {
                    transport: None,
                    steady_time: None,
                },
                buffers,
                Events {
                    input: &InputEvents::empty(),
                    output: &mut OutputEvents::void(),
                },
            )
            .unwrap();
    });
    assert_eq!(samples, [[0.25; 8], [-0.5; 8]]);
    let meters = shared.meters().unwrap();
    assert_eq!(meters.input[0].peak, 0.5);
    assert_eq!(meters.output[1].rms, 0.5);
    shared.edits.push(CLAP_GAIN_ID, 3.0, 1).unwrap();
    let invalid = [ParamValueEvent::new(
        0,
        CLAP_GAIN_ID,
        Pckn::match_all(),
        f64::NAN,
    )];
    with_audio(&mut samples, |buffers| {
        audio
            .process(
                Process {
                    transport: None,
                    steady_time: None,
                },
                buffers,
                Events {
                    input: &InputEvents::from_buffer(&invalid),
                    output: &mut OutputEvents::void(),
                },
            )
            .unwrap();
    });
    assert_eq!(samples, [[0.0; 8]; 2]);
    assert_eq!(shared.parameters().values(), &[3.0, 0.0]);
    assert_eq!(shared.meters().unwrap(), meters);
    assert_eq!(shared.fault_code(), 1);
    shared.gain.restore(0.25, true);
    let info = allocation_counter::measure(|| {
        for _ in 0..1000 {
            samples = [[0.5; 8], [-1.0; 8]];
            with_audio(&mut samples, |buffers| {
                audio
                    .process(
                        Process {
                            transport: None,
                            steady_time: None,
                        },
                        buffers,
                        Events {
                            input: &InputEvents::empty(),
                            output: &mut OutputEvents::void(),
                        },
                    )
                    .unwrap();
            });
            assert_eq!(samples, [[0.5; 8], [-1.0; 8]]);
            assert_eq!(shared.meters().unwrap().output[1].rms, 1.0);
        }
    });
    assert_eq!(info.count_total, 0);
    assert_eq!(info.count_current, 0);
    assert_eq!(info.bytes_current, 0);
}
