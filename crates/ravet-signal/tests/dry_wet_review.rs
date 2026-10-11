use ravet_signal::{
    DelayInterpolation as I, DryWetGains as G, DryWetMixer as M, DryWetMixer32 as M32,
    DryWetMixerConfig as C, PanningLaw as L, PanningNormalization as N,
};
fn config(channels: usize) -> C {
    C {
        channels,
        maximum_frames: 32,
        fifo_capacity_frames: 37,
        maximum_latency_samples: 17,
        interpolation: I::Linear,
        ramp_frames: 0,
    }
}
#[test]
fn integer_fraction_getters_are_observations_not_executed_delay() {
    let mut c = config(1);
    c.interpolation = I::Integer;
    let mut mixer = M32::new(
        c,
        G::new(0., L::Linear, N::UnityEndpoints)
            .unwrap()
            .quantize_f32(),
    )
    .unwrap();
    mixer.set_wet_latency_samples(2.99999999).unwrap();
    assert_eq!(mixer.native_fraction(), 1.);
    assert_eq!(mixer.native_thiran_coefficient(), 0.);
    mixer.push_dry(&[&[1., 0., 0., 0., 0.]], 5).unwrap();
    let mut output = [99.; 5];
    mixer.mix_wet(&[&[0.; 5]], &mut [&mut output], 5).unwrap();
    assert_eq!(output, [0., 0., 1., 0., 0.]);
}
#[test]
fn exactly_representable_wet_mirrors_energy_and_headroom() {
    // Dyadic coordinates make the reflected coordinate exact in both directions.
    for law in [
        L::Linear,
        L::Balanced,
        L::Sine,
        L::SineThreeHalves,
        L::SineSquared,
        L::SquareRoot,
        L::ThreeQuarterPower,
    ] {
        for norm in [N::UnityEndpoints, N::UnityCenter] {
            let endpoint = G::new(0., law, norm).unwrap().gains()[0];
            for i in 0..=1024 {
                let u = i as f64 / 1024.;
                let a = G::new(u, law, norm).unwrap();
                let b = G::new(1. - u, law, norm).unwrap();
                assert_eq!(a.gains(), [b.gains()[1], b.gains()[0]]);
                assert_eq!(
                    a.quantize_f32().gains(),
                    [b.quantize_f32().gains()[1], b.quantize_f32().gains()[0]]
                );
                assert!(a.gains().iter().all(|&v| v >= 0. && v <= endpoint));
                let power = a.gains().iter().map(|v| v * v).sum::<f64>();
                if matches!(law, L::Sine | L::SquareRoot) {
                    assert!(
                        (power - endpoint * endpoint).abs()
                            <= 8. * f64::EPSILON * endpoint * endpoint
                    );
                }
            }
        }
    }
}
macro_rules! graph {
    ($name:ident,$t:ty,$m:ty,$convert:expr,$tol:expr) => {
        #[test]
        fn $name() {
            use ravet_foundation::{
                AudioBuffer, AudioLayout, PrepareError, ProcessBlock, ProcessConfig, ProcessStatus,
                Processor, Transport,
            };
            use ravet_graph::{
                Connection, Destination, Graph, GraphNode, GraphStatus, NodeId, Source,
            };
            use ravet_signal::FirConvolver;
            use std::sync::{
                Arc,
                atomic::{AtomicUsize, Ordering},
            };
            const FIR: [f64; 6] = [0.2, 0.1, 0.2, 0.2, 0.1, 0.2];
            struct Consumer {
                mixer: $m,
                wet: [FirConvolver; 3],
                config: Option<ProcessConfig>,
                frames: Arc<AtomicUsize>,
                deactivations: Arc<AtomicUsize>,
            }
            impl Processor<$t> for Consumer {
                fn prepare(&mut self, c: ProcessConfig) -> Result<(), PrepareError> {
                    self.config = None;
                    if c.layout() != AudioLayout::new(3, 3).unwrap() {
                        return Err(PrepareError::UnsupportedLayout);
                    }
                    if c.sample_rate() != 48000. {
                        return Err(PrepareError::InvalidSampleRate);
                    }
                    self.config = Some(c);
                    self.reset();
                    Ok(())
                }
                fn process(&mut self, b: &mut ProcessBlock<'_, '_, '_, $t>) -> ProcessStatus {
                    if let Err(e) = self.config.unwrap().validate_block(b) {
                        b.clear_outputs();
                        return ProcessStatus::Fault(e);
                    }
                    let frames = b.audio.frames();
                    let split = frames / 3;
                    let input = [
                        b.audio.input(0).unwrap(),
                        b.audio.input(1).unwrap(),
                        b.audio.input(2).unwrap(),
                    ];
                    self.mixer
                        .push_dry(
                            &[&input[0][..split], &input[1][..split], &input[2][..split]],
                            split,
                        )
                        .unwrap();
                    self.mixer
                        .push_dry(
                            &[
                                &input[0][split..frames],
                                &input[1][split..frames],
                                &input[2][split..frames],
                            ],
                            frames - split,
                        )
                        .unwrap();
                    let mut wet = [[0. as $t; 32]; 3];
                    let mut mixed = [[0. as $t; 32]; 3];
                    for ch in 0..3 {
                        for i in 0..frames {
                            // Explicit native32-to-f64 FIR boundary; invert wet for null measurement.
                            wet[ch][i] = (-self.wet[ch].process_sample(input[ch][i] as f64)) as $t;
                        }
                    }
                    let consume = frames / 2;
                    {
                        let [a, c, d] = &mut mixed;
                        self.mixer
                            .mix_wet(
                                &[&wet[0][..consume], &wet[1][..consume], &wet[2][..consume]],
                                &mut [&mut a[..consume], &mut c[..consume], &mut d[..consume]],
                                consume,
                            )
                            .unwrap();
                    }
                    {
                        let [a, c, d] = &mut mixed;
                        self.mixer
                            .mix_wet(
                                &[
                                    &wet[0][consume..frames],
                                    &wet[1][consume..frames],
                                    &wet[2][consume..frames],
                                ],
                                &mut [
                                    &mut a[consume..frames],
                                    &mut c[consume..frames],
                                    &mut d[consume..frames],
                                ],
                                frames - consume,
                            )
                            .unwrap();
                    }
                    for (ch, y) in mixed.iter().enumerate() {
                        b.audio.output_mut(ch).unwrap()[..frames].copy_from_slice(&y[..frames]);
                    }
                    let previous = self.frames.fetch_add(frames, Ordering::Relaxed);
                    assert_eq!(self.mixer.accepted_dry_frames(), (previous + frames) as u64);
                    assert_eq!(self.mixer.consumed_mix_frames(), (previous + frames) as u64);
                    assert_eq!(self.mixer.queued_frames(), 0);
                    ProcessStatus::Continue
                }
                fn reset(&mut self) {
                    self.mixer.reset();
                    for w in &mut self.wet {
                        w.reset();
                    }
                    self.frames.store(0, Ordering::Relaxed);
                }
                fn deactivate(&mut self) {
                    if self.config.take().is_some() {
                        self.deactivations.fetch_add(1, Ordering::Relaxed);
                    }
                }
            }
            let frames = Arc::new(AtomicUsize::new(0));
            let deactivations = Arc::new(AtomicUsize::new(0));
            let layout = AudioLayout::new(3, 3).unwrap();
            let mut mixer = <$m>::new(
                config(3),
                ($convert)(G::new(0.5, L::Linear, N::UnityEndpoints).unwrap()),
            )
            .unwrap();
            mixer.set_wet_latency_samples(2.5).unwrap();
            let node = GraphNode {
                id: NodeId(243),
                layout,
                processor: Box::new(Consumer {
                    mixer,
                    wet: std::array::from_fn(|_| FirConvolver::new(&FIR).unwrap()),
                    config: None,
                    frames: frames.clone(),
                    deactivations: deactivations.clone(),
                }),
            };
            let routes: Vec<_> = (0..3)
                .flat_map(|channel| {
                    [
                        Connection {
                            source: Source::Input(channel),
                            destination: Destination::Node {
                                id: NodeId(243),
                                channel,
                            },
                            gain: 1.,
                        },
                        Connection {
                            source: Source::Node {
                                id: NodeId(243),
                                channel,
                            },
                            destination: Destination::Output(channel),
                            gain: 1.,
                        },
                    ]
                })
                .collect();
            let mut graph = Graph::new(layout, vec![node], &routes).unwrap();
            let amplitudes = [1. as $t, 0.7 as $t, -0.3 as $t];
            for cycle in 0..2 {
                graph
                    .prepare(ProcessConfig::new(48000., 32, 0, layout).unwrap())
                    .unwrap();
                for repeat in 0..2 {
                    let mut bad = [[99. as $t; 35]; 3];
                    let [a, b, c] = &mut bad;
                    let mut destination = [a.as_mut_slice(), b.as_mut_slice(), c.as_mut_slice()];
                    let input = [&[0. as $t; 33][..]; 3];
                    let audio = AudioBuffer::separate(&input, &mut destination, 33).unwrap();
                    assert!(matches!(
                        graph.process(&mut ProcessBlock::new(audio, &[], Transport::default())),
                        GraphStatus::Fault { node: None, .. }
                    ));
                    assert_eq!(frames.load(Ordering::Relaxed), 0);
                    assert!(
                        bad.iter().all(|v| v[..33].iter().all(|&y| y == 0.)
                            && v[33..].iter().all(|&y| y == 99.))
                    );
                    let mut response: [Vec<f64>; 3] = std::array::from_fn(|_| vec![]);
                    let mut start = 0;
                    let mut turn = 0;
                    while start < 512 {
                        let count = [1, 32, 0, 7, 19, 2][turn % 6].min(512 - start);
                        let source: [[$t; 32]; 3] = std::array::from_fn(|ch| {
                            std::array::from_fn(
                                |i| if start + i == 0 { amplitudes[ch] } else { 0. },
                            )
                        });
                        let mut output = [[99. as $t; 35]; 3];
                        let measured = allocation_counter::measure(|| {
                            let input = [
                                &source[0][..count],
                                &source[1][..count],
                                &source[2][..count],
                            ];
                            let [a, b, c] = &mut output;
                            let mut destination =
                                [a.as_mut_slice(), b.as_mut_slice(), c.as_mut_slice()];
                            let audio =
                                AudioBuffer::separate(&input, &mut destination, count).unwrap();
                            assert_eq!(
                                graph.process(&mut ProcessBlock::new(
                                    audio,
                                    &[],
                                    Transport::default()
                                )),
                                GraphStatus::Continue
                            );
                        });
                        assert_eq!(measured.count_total, 0);
                        assert_eq!(measured.count_current, 0);
                        for ch in 0..3 {
                            for i in 0..count {
                                // Independent finite convolution: half-linear dry minus half symmetric FIR.
                                let n = start + i;
                                let dry = if n == 2 || n == 3 { 0.25 } else { 0. };
                                let wet = FIR.get(n).map_or(0., |v| -0.5 * v);
                                assert!(
                                    (output[ch][i] as f64 - (dry + wet) * amplitudes[ch] as f64)
                                        .abs()
                                        <= $tol
                                );
                                response[ch].push(output[ch][i] as f64);
                            }
                            assert!(output[ch][count..].iter().all(|&v| v == 99.));
                        }
                        start += count;
                        turn += 1;
                        assert_eq!(frames.load(Ordering::Relaxed), start);
                    }
                    for ch in 0..3 {
                        for f in [0., 0.001, 0.01, 0.03, 0.1, 0.25, 0.49] {
                            let w = std::f64::consts::TAU * f;
                            let coefficient = 0.5
                                * ((0.5 * w).cos()
                                    - 0.4 * (2.5 * w).cos()
                                    - 0.2 * (1.5 * w).cos()
                                    - 0.4 * (0.5 * w).cos());
                            let expected = [
                                coefficient * (2.5 * w).cos(),
                                -coefficient * (2.5 * w).sin(),
                            ]
                            .map(|v| v * amplitudes[ch] as f64);
                            let actual = [
                                response[ch]
                                    .iter()
                                    .enumerate()
                                    .map(|(i, &y)| y * (w * i as f64).cos())
                                    .sum::<f64>(),
                                response[ch]
                                    .iter()
                                    .enumerate()
                                    .map(|(i, &y)| -y * (w * i as f64).sin())
                                    .sum::<f64>(),
                            ];
                            assert!(
                                (actual[0] - expected[0]).hypot(actual[1] - expected[1])
                                    <= 6. * $tol
                            );
                            // |cos(a)-cos(b)| <= |a²-b²|/2 gives this low-frequency null bound.
                            if f <= 0.03 {
                                assert!(
                                    actual[0].hypot(actual[1])
                                        <= 0.7 * w * w * (amplitudes[ch] as f64).abs() + 6. * $tol
                                );
                            }
                        }
                    }
                    if repeat == 0 {
                        graph.reset();
                    }
                }
                graph.deactivate();
                assert_eq!(deactivations.load(Ordering::Relaxed), cycle + 1);
            }
        }
    };
}
graph!(
    native64_fractional_group_delay_graph_null_with_analytic_response,
    f64,
    M,
    |g: G| g,
    2e-16
);
graph!(
    native32_fractional_group_delay_graph_null_with_explicit_fir_bridge,
    f32,
    M32,
    |g: G| g.quantize_f32(),
    6e-8
);
