use super::mock::{self, Probe};
use super::*;
use crate::vst3::instances::{
    Instance, Library,
    parameters::{self, Point},
    processing::{self, Context, InputEvents, OutputStorage},
};
use std::{mem::offset_of, sync::atomic::Ordering};

const STEREO: u64 = 0b11;
const MONO: u64 = 0b1;

struct Fixture {
    tables: mock::Tables,
    probe: &'static Probe,
    library: Library<'static>,
}
fn fixture(mode: u32) -> Fixture {
    let tables = mock::tables();
    let probe = Probe::leak();
    probe.mode.store(mode, Ordering::SeqCst);
    let library = mock::library(&tables, probe);
    Fixture {
        tables,
        probe,
        library,
    }
}
impl Fixture {
    fn hosted(&self) -> Provider<'static> {
        unsafe { Provider::from_raw(self.tables.hosted).unwrap() }
    }
    fn refreshed(&self, class: u32) -> (Instance<'_, 'static>, AudioCatalogue) {
        let mut child = self.library.create(class).unwrap();
        let parameters = unsafe { parameters::Provider::from_raw(self.tables.parameters).unwrap() };
        let processing = unsafe { processing::Provider::from_raw(self.tables.processing).unwrap() };
        child.refresh_parameters(parameters).unwrap();
        child.refresh_processing(processing, 0, 0).unwrap();
        let catalogue = child.refresh_hosted_audio(self.hosted()).unwrap();
        (child, catalogue)
    }
}
fn effect_profile(precision: Precision, aux_mono: bool, aux_stereo: bool) -> HostedAudioProfile {
    let side = |active, arrangement| {
        if active {
            BusRequest::active(arrangement)
        } else {
            BusRequest::inactive(arrangement)
        }
    };
    HostedAudioProfile::new(
        precision,
        48000.,
        64,
        &[
            BusRequest::active(STEREO),
            side(aux_mono, MONO),
            side(aux_stereo, STEREO),
        ],
        &[BusRequest::active(STEREO), BusRequest::active(MONO)],
    )
    .unwrap()
}

#[test]
fn every_hosted_wire_layout_matches_the_literal_c_header() {
    assert_eq!(size_of::<RawInfo>(), 16);
    assert_eq!(size_of::<RawBus>(), 416);
    assert_eq!(offset_of!(RawBus, arrangement), 24);
    assert_eq!(offset_of!(RawBus, name), 32);
    assert_eq!(size_of::<RawBusRequest>(), 16);
    assert_eq!(size_of::<RawProfile>(), 544);
    assert_eq!(offset_of!(RawProfile, sample_rate), 24);
    assert_eq!(offset_of!(RawProfile, inputs), 32);
    assert_eq!(offset_of!(RawProfile, outputs), 288);
    assert_eq!(size_of::<RawTiming>(), 24);
    assert_eq!(offset_of!(RawTiming, tail_samples), 16);
    assert_eq!(size_of::<RawReport>(), 24);
    assert_eq!(size_of::<InputChannel64>(), 16);
    assert_eq!(size_of::<OutputChannel64>(), 16);
    assert_eq!(size_of::<Block64>(), 32);
    assert_eq!(offset_of!(Block64, inputs), 16);
    assert_eq!(size_of::<RawProvider>(), 72);
    assert_eq!(offset_of!(RawProvider, instance_origin), 8);
    assert_eq!(offset_of!(RawProvider, processing_origin), 16);
    assert_eq!(offset_of!(RawProvider, refresh_audio), 24);
    assert_eq!(offset_of!(RawProvider, process32), 56);
    assert_eq!(offset_of!(RawProvider, process64), 64);
}

#[test]
fn companion_requires_exact_version_entries_and_origin_identity() {
    let f = fixture(0);
    let good = f.tables.hosted;
    let mut copy = RawProvider { ..*good };
    copy.version = 2;
    assert!(matches!(
        unsafe { Provider::from_raw(&copy) },
        Err(Error::Abi)
    ));
    let mut copy = RawProvider { ..*good };
    copy.processing_origin = std::ptr::null();
    assert!(matches!(
        unsafe { Provider::from_raw(&copy) },
        Err(Error::Abi)
    ));
    let mut copy = RawProvider { ..*good };
    copy.process64 = None;
    assert!(matches!(
        unsafe { Provider::from_raw(&copy) },
        Err(Error::Abi)
    ));
    // A foreign instance origin refuses before any native call.
    let other = mock::tables();
    let foreign = unsafe { Provider::from_raw(other.hosted).unwrap() };
    let (mut child, _) = f.refreshed(0);
    assert_eq!(child.refresh_hosted_audio(foreign), Err(Error::Abi));
    // Missing exact processing refresh is a lifecycle refusal.
    let mut fresh = f.library.create(0).unwrap();
    assert_eq!(
        fresh.refresh_hosted_audio(f.hosted()),
        Err(Error::Lifecycle)
    );
    let profile = effect_profile(Precision::F32, false, false);
    assert_eq!(
        fresh.prepare_hosted32(f.hosted(), &profile),
        Err(Error::Lifecycle)
    );
}

#[test]
fn catalogue_copies_every_bus_in_native_order_with_types_names_and_flags() {
    let f = fixture(0);
    let (_child, catalogue) = f.refreshed(0);
    let inputs = catalogue.inputs();
    assert_eq!(inputs.len(), 3);
    assert_eq!(
        inputs[0],
        AudioBus {
            index: 0,
            direction: Direction::Input,
            channels: 2,
            auxiliary: false,
            default_active: true,
            arrangement: STEREO,
            name: "Main".into(),
        }
    );
    assert_eq!(inputs[1].name, "Side \u{e9}");
    assert!(inputs[1].auxiliary && !inputs[1].default_active);
    assert_eq!((inputs[2].index, inputs[2].channels), (2, 2));
    assert_eq!(catalogue.outputs().len(), 2);
    assert_eq!(catalogue.buses(Direction::Output)[1].channels, 1);
    let (_instrument, catalogue) = f.refreshed(1);
    assert!(catalogue.inputs().is_empty());
    assert_eq!(catalogue.outputs().len(), 1);

    let f = fixture(mock::MALFORMED_BUS);
    let mut child = f.library.create(0).unwrap();
    let parameters = unsafe { parameters::Provider::from_raw(f.tables.parameters).unwrap() };
    let processing = unsafe { processing::Provider::from_raw(f.tables.processing).unwrap() };
    child.refresh_parameters(parameters).unwrap();
    child.refresh_processing(processing, 0, 0).unwrap();
    assert_eq!(child.refresh_hosted_audio(f.hosted()), Err(Error::Abi));
    let profile = effect_profile(Precision::F32, false, false);
    assert_eq!(
        child.prepare_hosted32(f.hosted(), &profile),
        Err(Error::Lifecycle)
    );
}

#[test]
fn profile_admission_and_deterministic_packed_map() {
    let s = BusRequest::active(STEREO);
    for (rate, frames) in [
        (f64::NAN, 16),
        (0., 16),
        (768000.5, 16),
        (48000., 0),
        (48000., 65537),
    ] {
        assert_eq!(
            HostedAudioProfile::new(Precision::F64, rate, frames, &[s], &[s]),
            Err(Error::Shape)
        );
    }
    assert!(HostedAudioProfile::new(Precision::F64, 768000., 65536, &[], &[s]).is_ok());
    assert_eq!(
        HostedAudioProfile::new(Precision::F32, 48000., 16, &[BusRequest::active(0)], &[s]),
        Err(Error::Shape)
    );
    assert_eq!(
        HostedAudioProfile::new(
            Precision::F32,
            48000.,
            16,
            &[BusRequest::inactive(STEREO)],
            &[]
        ),
        Err(Error::Shape)
    );
    assert_eq!(
        HostedAudioProfile::new(Precision::F32, 48000., 16, &[s; 17], &[s]),
        Err(Error::Shape)
    );
    let wide = BusRequest::active(0xffff);
    assert_eq!(
        HostedAudioProfile::new(Precision::F32, 48000., 16, &[wide; 5], &[s]),
        Err(Error::Capacity)
    );
    // Exactly 64 channels per direction is admitted.
    let full = HostedAudioProfile::new(Precision::F64, 48000., 16, &[wide; 4], &[wide; 4]).unwrap();
    assert_eq!(
        (full.map().input_channels(), full.map().output_channels()),
        (64, 64)
    );
    let profile = effect_profile(Precision::F64, false, true);
    let inputs = profile.map().buses(Direction::Input);
    assert_eq!(inputs.len(), 3);
    assert_eq!(inputs[0].channel_range(), Some(0..2));
    assert_eq!(inputs[1].channel_range(), None);
    assert_eq!(inputs[1].first_channel, 2);
    assert_eq!(inputs[2].channel_range(), Some(2..4));
    assert_eq!(profile.map().input_channels(), 4);
    assert_eq!(profile.map().output_channels(), 3);
}

#[test]
fn local_refusal_preserves_prior_profile_and_typed_precision_is_exact() {
    let f = fixture(0);
    let (mut child, _) = f.refreshed(0);
    let profile = effect_profile(Precision::F32, true, false);
    assert_eq!(
        child.prepare_hosted64(f.hosted(), &profile),
        Err(Error::Precision)
    );
    let timing = child.prepare_hosted32(f.hosted(), &profile).unwrap();
    assert_eq!(timing.latency_samples, 3);
    assert_eq!(timing.tail, Tail::Samples(480));
    // Every advertised bus needs a request: a missing request refuses locally.
    let short = HostedAudioProfile::new(
        Precision::F32,
        48000.,
        64,
        &[BusRequest::active(STEREO)],
        &[BusRequest::active(STEREO), BusRequest::active(MONO)],
    )
    .unwrap();
    assert_eq!(
        child.prepare_hosted32(f.hosted(), &short),
        Err(Error::Shape)
    );
    assert_eq!(child.hosted_profile(), Some(&profile));
    assert_eq!(child.hosted_timing(), Some(timing));
    // Native arrangement disagreement is an actual native refusal with its
    // original stage; readiness was cleared before the foreign call.
    let wrong = HostedAudioProfile::new(
        Precision::F32,
        48000.,
        64,
        &[
            BusRequest::active(STEREO),
            BusRequest::inactive(STEREO),
            BusRequest::inactive(STEREO),
        ],
        &[BusRequest::active(STEREO), BusRequest::active(MONO)],
    )
    .unwrap();
    let Err(Error::Native {
        status: 3,
        report: Some(report),
    }) = child.prepare_hosted32(f.hosted(), &wrong)
    else {
        panic!("expected native metadata refusal");
    };
    assert_eq!((report.stage(), report.failure()), (106, 6));
    assert!(!report.attempted() && report.returned_status().is_none());
    assert_eq!(child.hosted_profile(), None);
    assert!(child.with_audio(|_| ()).is_err());
}

#[test]
fn native_precision_refusal_keeps_original_returned_result_without_fallback() {
    let f = fixture(mock::REFUSE_F64);
    let (mut child, _) = f.refreshed(0);
    let profile = effect_profile(Precision::F64, false, false);
    let Err(Error::Native {
        status: 4,
        report: Some(report),
    }) = child.prepare_hosted64(f.hosted(), &profile)
    else {
        panic!("expected returned precision refusal");
    };
    assert_eq!(report.stage(), 107);
    assert_eq!(report.returned_status(), Some(1));
    assert!(report.attempted() && !report.threw());
    assert_eq!(child.hosted_profile(), None);
    assert!(
        !f.probe.entries().iter().any(|e| e.starts_with("prepare")),
        "no fallback precision was negotiated"
    );
}

#[test]
fn malformed_timing_never_publishes_a_profile() {
    let f = fixture(mock::MALFORMED_TIMING);
    let (mut child, _) = f.refreshed(1);
    let profile = HostedAudioProfile::new(
        Precision::F64,
        48000.,
        16,
        &[],
        &[BusRequest::active(STEREO)],
    )
    .unwrap();
    assert_eq!(
        child.prepare_hosted64(f.hosted(), &profile),
        Err(Error::Abi)
    );
    assert_eq!(child.hosted_profile(), None);
}

/// Effect outputs and echoed `(id, offset, value)` points.
type EffectRun = ([[f64; 4]; 3], Vec<(u32, u32, f64)>);
fn run_effect<S: HostedSample + Into<f64>>(
    f: &Fixture,
    precision: Precision,
    main: [[S; 4]; 2],
    aux: [S; 4],
    sides: [[S; 4]; 2],
) -> EffectRun {
    let (mut child, _) = f.refreshed(0);
    child
        .prepare_hosted(f.hosted(), &effect_profile(precision, true, true))
        .unwrap();
    child.start().unwrap();
    let events = InputEvents::new(0, 0).unwrap();
    let mut storage = OutputStorage::maximum().unwrap();
    let mut context = Context::new(48000., 0);
    context.state = Context::TEMPO;
    context.tempo = 120.;
    let points = [Point {
        id: 7,
        offset: 1,
        value: 0.25,
    }];
    let (out, echoed) = child
        .with_audio(|mut audio| {
            std::thread::scope(|scope| {
                scope
                    .spawn(move || {
                        let mut out = [[S::default(); 4]; 3];
                        assert_eq!(audio.hosted_channels(), Some((5, 3)));
                        let [o0, o1, o2] = &mut out;
                        let output = audio
                            .process_hosted::<S>(
                                &[&main[0], &main[1], &aux, &sides[0], &sides[1]],
                                &mut [o0, o1, o2],
                                4,
                                Input {
                                    points: &points,
                                    events: &events,
                                    context: Some(&context),
                                },
                                &mut storage,
                            )
                            .unwrap();
                        assert!(output.report().published());
                        assert_eq!(output.parameter_ids(), &[9999, 7]);
                        let echoed = output
                            .parameter_points()
                            .iter()
                            .map(|p| (p.id, p.offset, p.value))
                            .collect::<Vec<_>>();
                        (out, echoed)
                    })
                    .join()
                    .unwrap()
            })
        })
        .unwrap();
    child.stop().unwrap();
    (out.map(|ch| ch.map(Into::into)), echoed)
}

#[test]
fn f64_main_and_two_aux_buses_stay_f64_end_to_end() {
    let f = fixture(0);
    // 0.1 and 0.3 are not representable in f32: any narrowing changes bits.
    let main = [[0.1f64, -0.3, 1e-9, 2.], [0.7, 0.2, -1e-9, -2.]];
    let aux = [0.3f64, 0.6, 0., 1.];
    let sides = [[0.9f64, 0.1, 0.5, -0.5], [0.3, 0.3, 0.3, 0.3]];
    let (out, echoed) = run_effect(&f, Precision::F64, main, aux, sides);
    for ch in 0..2 {
        for i in 0..4 {
            let expected = main[ch][i] * 1.5 + aux[i] * 0.25 + sides[ch][i] * 0.125;
            assert_eq!(out[ch][i].to_bits(), expected.to_bits());
            let narrowed = f64::from(
                main[ch][i] as f32 * 1.5 + aux[i] as f32 * 0.25 + sides[ch][i] as f32 * 0.125,
            );
            if i < 2 {
                assert_ne!(out[ch][i], narrowed, "f64 path must not narrow");
            }
        }
    }
    for i in 0..4 {
        assert_eq!(out[2][i], main[0][i] - main[1][i]);
    }
    assert_eq!(echoed, vec![(9999, 0, 0.12), (7, 1, 0.25)]);
    assert_eq!(f.probe.count(&f.probe.wrong_thread), 0);
}

#[test]
fn f32_main_aux_processing_matches_independent_oracle() {
    let f = fixture(0);
    let main = [[0.5f32, -0.25, 1., 0.], [0.125, 0.5, -1., 0.75]];
    let aux = [1f32, 0.5, 0.25, 0.];
    let sides = [[0.5f32; 4], [-0.5f32; 4]];
    let (out, _) = run_effect(&f, Precision::F32, main, aux, sides);
    for ch in 0..2 {
        for i in 0..4 {
            let expected = main[ch][i] * 1.5 + aux[i] * 0.25 + sides[ch][i] * 0.125;
            assert_eq!(out[ch][i], f64::from(expected));
        }
    }
}

#[test]
fn inactive_aux_indices_keep_native_order_and_legacy_entries_refuse() {
    let f = fixture(0);
    let (mut child, _) = f.refreshed(0);
    let profile = effect_profile(Precision::F32, false, true);
    child.prepare_hosted32(f.hosted(), &profile).unwrap();
    child.start().unwrap();
    let events = InputEvents::new(0, 0).unwrap();
    let mut storage = OutputStorage::maximum().unwrap();
    child
        .with_audio(|mut audio| {
            let main = [[1f32; 2], [0.5; 2]];
            let sides = [[2f32; 2], [4.; 2]];
            let mut out = [[0f32; 2]; 3];
            // Legacy main-f32 entries refuse a hosted capability before natives.
            let [a, b, c] = &mut out;
            assert_eq!(
                audio.process(&[&main[0], &main[1]], &mut [&mut a[..], &mut b[..]], 2),
                Err(crate::vst3::instances::Error::Shape)
            );
            assert_eq!(
                audio
                    .process_events(
                        &[&main[0], &main[1]],
                        &mut [&mut a[..], &mut b[..]],
                        2,
                        Input {
                            points: &[],
                            events: &events,
                            context: None
                        },
                        &mut storage
                    )
                    .err(),
                Some(processing::Error::Lifecycle)
            );
            // Precision is fixed for the scope.
            let mut wide = [[0f64; 2]; 3];
            let [x, y, z] = &mut wide;
            assert_eq!(
                audio
                    .process_hosted::<f64>(
                        &[&[0.; 2], &[0.; 2], &[0.; 2], &[0.; 2]],
                        &mut [x, y, z],
                        2,
                        Input {
                            points: &[],
                            events: &events,
                            context: None
                        },
                        &mut storage
                    )
                    .err(),
                Some(Error::Precision)
            );
            // Wrong packed count (aux mono is inactive: 4 inputs, not 5).
            assert_eq!(
                audio
                    .process_hosted::<f32>(
                        &[&main[0], &main[1], &[0.; 2], &sides[0], &sides[1]],
                        &mut [&mut a[..], &mut b[..], &mut c[..]],
                        2,
                        Input {
                            points: &[],
                            events: &events,
                            context: None
                        },
                        &mut storage
                    )
                    .err(),
                Some(Error::Shape)
            );
            audio
                .process_hosted::<f32>(
                    &[&main[0], &main[1], &sides[0], &sides[1]],
                    &mut [a, b, c],
                    2,
                    Input {
                        points: &[],
                        events: &events,
                        context: None,
                    },
                    &mut storage,
                )
                .unwrap();
            assert_eq!(out[0], [1.5 + 0.25; 2]);
            assert_eq!(out[1], [0.75 + 0.5; 2]);
            assert_eq!(out[2], [0.5; 2]);
        })
        .unwrap();
    assert_eq!(f.probe.count(&f.probe.processes), 1);
}

#[test]
fn native_fault_silences_and_retires_only_the_hosted_capability() {
    let f = fixture(mock::FAIL_PROCESS);
    let (mut child, _) = f.refreshed(1);
    let profile = HostedAudioProfile::new(
        Precision::F64,
        48000.,
        8,
        &[],
        &[BusRequest::active(STEREO)],
    )
    .unwrap();
    assert_eq!(
        child.prepare_hosted64(f.hosted(), &profile).unwrap().tail,
        Tail::Infinite
    );
    child.start().unwrap();
    let events = InputEvents::new(0, 0).unwrap();
    let mut storage = OutputStorage::maximum().unwrap();
    child
        .with_audio(|mut audio| {
            let mut out = [[9f64; 8]; 2];
            let [l, r] = &mut out;
            let input = || Input {
                points: &[],
                events: &events,
                context: None,
            };
            audio
                .process_hosted::<f64>(&[], &mut [l, r], 8, input(), &mut storage)
                .unwrap();
            // Mock instrument constant is the f32 literal 0.1 widened, times 2.
            assert_eq!(out[1][7], f64::from(0.1f32) * 2.);
            let [l, r] = &mut out;
            let error = audio
                .process_hosted::<f64>(&[], &mut [l, r], 8, input(), &mut storage)
                .err()
                .unwrap();
            assert!(matches!(
                error,
                Error::Processing(processing::Error::Provider { status: 4, .. })
            ));
            assert_eq!(out, [[0.; 8]; 2]);
            let [l, r] = &mut out;
            assert_eq!(
                audio
                    .process_hosted::<f64>(&[], &mut [l, r], 8, input(), &mut storage)
                    .err(),
                Some(Error::Lifecycle)
            );
        })
        .unwrap();
    assert_eq!(f.probe.count(&f.probe.processes), 2);
}

#[test]
fn ten_thousand_hosted_f64_calls_allocate_nothing_in_rust() {
    let f = fixture(0);
    let (mut child, _) = f.refreshed(0);
    child
        .prepare_hosted64(f.hosted(), &effect_profile(Precision::F64, true, true))
        .unwrap();
    child.start().unwrap();
    let events = InputEvents::new(0, 0).unwrap();
    let mut storage = OutputStorage::maximum().unwrap();
    let points = [Point {
        id: 3,
        offset: 0,
        value: 0.5,
    }];
    child
        .with_audio(|mut audio| {
            let input = [[0.25f64; 64]; 5];
            let mut out = [[0f64; 64]; 3];
            let measured = allocation_counter::measure(|| {
                for i in 0..10000u32 {
                    let frames = i % 65;
                    let [a, b, c] = &mut out;
                    let p: &[Point] = if frames == 0 { &[] } else { &points };
                    let output = audio
                        .process_hosted::<f64>(
                            &[&input[0], &input[1], &input[2], &input[3], &input[4]],
                            &mut [a, b, c],
                            frames,
                            Input {
                                points: p,
                                events: &events,
                                context: None,
                            },
                            &mut storage,
                        )
                        .unwrap();
                    assert_eq!(output.parameter_points().len(), p.len());
                }
            });
            assert_eq!(measured.count_total, 0);
            assert_eq!(out[0][0], 0.25 * 1.5 + 0.25 * 0.25 + 0.25 * 0.125);
        })
        .unwrap();
    assert_eq!(f.probe.count(&f.probe.processes), 10000);
}
