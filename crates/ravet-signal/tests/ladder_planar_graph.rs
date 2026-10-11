use ravet_foundation::{
    AudioBuffer, AudioLayout, PrepareError, ProcessBlock, ProcessConfig, ProcessStatus, Processor,
    Transport,
};
use ravet_graph::{Connection, Destination, Graph, GraphNode, GraphStatus, NodeId, Source};
use ravet_signal::{
    FirConvolver, LadderBank as B, LadderBank32 as B32, LadderDesign as D, LadderMode as M,
};
use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};
const MODES: [M; 6] = [
    M::LowPass12,
    M::LowPass24,
    M::HighPass12,
    M::HighPass24,
    M::BandPass12,
    M::BandPass24,
];
const TAPS: [f64; 5] = [0.1, 0.3, -0.1, 0.6, 0.2];
fn linear_solve(mut a: [[f64; 4]; 4], mut b: [f64; 4]) -> [f64; 4] {
    for i in 0..4 {
        let p = (i..4)
            .max_by(|&j, &k| a[j][i].abs().total_cmp(&a[k][i].abs()))
            .unwrap();
        a.swap(i, p);
        b.swap(i, p);
        assert!(a[i][i].abs() > 1e-20);
        let z = a[i][i];
        for value in &mut a[i][i..] {
            *value /= z;
        }
        b[i] /= z;
        for j in 0..4 {
            if j != i {
                let z = a[j][i];
                let pivot_row = a[i];
                for (value, &pivot) in a[j][i..].iter_mut().zip(&pivot_row[i..]) {
                    *value -= z * pivot;
                }
                b[j] -= z * b[i];
            }
        }
    }
    b
}
fn nodal(state: &mut [f64; 4], x: f64, gain: f64, k: f64, d: f64, nonlinear: bool) -> [f64; 6] {
    let g = gain / (1. - gain);
    let mut y = *state;
    let mut converged = false;
    for _ in 0..80 {
        let v = d * x - k * y[3];
        let u = if nonlinear { v.tanh() } else { v };
        let derivative = if nonlinear { 1. - u * u } else { 1. };
        let residual = std::array::from_fn(|i| {
            (1. + g) * y[i] - state[i] - g * if i == 0 { u } else { y[i - 1] }
        });
        let mut jac = [[0.; 4]; 4];
        for i in 0..4 {
            jac[i][i] = 1. + g;
            if i > 0 {
                jac[i][i - 1] = -g;
            }
        }
        jac[0][3] += g * k * derivative;
        let delta = linear_solve(jac, residual);
        let before = y;
        for i in 0..4 {
            y[i] -= delta[i];
        }
        if delta.iter().map(|v| v.abs()).fold(0., f64::max)
            < 2e-15 * (1. + y.iter().map(|v| v.abs()).fold(0., f64::max))
        {
            converged = true;
            break;
        }
        if before == y {
            converged = true;
            break;
        }
    }
    assert!(
        converged,
        "independent four-dimensional Newton oracle did not converge within 80 iterations"
    );
    let u = if nonlinear {
        (d * x - k * y[3]).tanh()
    } else {
        d * x - k * y[3]
    };
    for i in 0..4 {
        state[i] = 2. * y[i] - state[i];
    }
    let [a, b, c, e] = y;
    [
        b,
        e,
        u - 2. * a + b,
        u - 4. * a + 6. * b - 4. * c + e,
        a - b,
        b - 2. * c + e,
    ]
}
macro_rules! graph {
    ($name:ident,$t:ty,$bank:ty,$design:ty,$convert:expr,$tol:expr)=>{
        #[test]
        fn $name() {
            struct Consumer {
                bank: $bank,
                profiles: [$design; 6],
                input: [Vec<$t>; 3],
                wet: [Vec<$t>; 3],
                fir: [FirConvolver; 3],
                position: usize,
                config: Option<ProcessConfig>,
                inplace: bool,
                frames: Arc<AtomicUsize>,
            }
            impl Processor<$t> for Consumer {
                fn prepare(&mut self, c: ProcessConfig) -> Result<(), PrepareError> {
                    if c.layout() != AudioLayout::new(3, 3).unwrap() {
                        return Err(PrepareError::UnsupportedLayout);
                    }
                    if c.maximum_frames() > 67 {
                        return Err(PrepareError::InvalidMaximumFrames);
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
                    let mut offset = 0;
                    while offset < frames {
                        let n = self.position;
                        let design = self.profiles[(n / 31) % 6];
                        if n != 0 && n.is_multiple_of(97) {
                            self.bank.replace_design(design);
                        } else if n.is_multiple_of(31) {
                            self.bank.set_design(design);
                        }
                        self.bank.set_bypassed((n / 23) % 3 == 1);
                        let count = [
                            frames - offset,
                            31 - n % 31,
                            17 - n % 17,
                            23 - n % 23,
                            97 - n % 97,
                        ]
                        .into_iter()
                        .min()
                        .unwrap();
                        for ch in 0..3 {
                            self.input[ch][..count]
                                .copy_from_slice(&b.audio.input(ch).unwrap()[offset..offset + count]);
                        }
                        let mode = MODES[(n / 17) % 6];
                        if self.inplace {
                            for ch in 0..3 {
                                self.wet[ch][..count].copy_from_slice(&self.input[ch][..count]);
                            }
                            let [a, c, d] = &mut self.wet;
                            let mut views = [&mut a[..count], &mut c[..count], &mut d[..count]];
                            self.bank.process_in_place(&mut views, count, mode).unwrap();
                        } else {
                            let input = self.input.each_ref().map(|v| &v[..count]);
                            let [a, c, d] = &mut self.wet;
                            let mut views = [&mut a[..count], &mut c[..count], &mut d[..count]];
                            self.bank.process(&input, &mut views, count, mode).unwrap();
                        }
                        for ch in 0..3 {
                            for i in 0..count {
                                b.audio.output_mut(ch).unwrap()[offset + i] =
                                    self.fir[ch].process_sample(self.wet[ch][i] as f64) as $t;
                            }
                        }
                        self.position += count;
                        offset += count;
                    }
                    self.frames.fetch_add(frames, Ordering::Relaxed);
                    ProcessStatus::Continue
                }
                fn reset(&mut self) {
                    self.bank.replace_design(self.profiles[0]);
                    self.bank.set_bypassed(false);
                    for f in &mut self.fir {
                        f.reset();
                    }
                    self.position = 0;
                    self.frames.store(0, Ordering::Relaxed);
                }
                fn deactivate(&mut self) {
                    self.config = None;
                }
            }
            let profiles = std::array::from_fn(|i| {
                let (fc, r, d) = [
                    (700., 0., 1.),
                    (1200., 0.4, 2.),
                    (2700., 0.75, 4.),
                    (7000., 0.9, 8.),
                    (15000., 0.99, 16.),
                    (21000., 1., 32.),
                ][i];
                $convert(D::new(48000., fc, r, d).unwrap())
            });
            let input: [Vec<$t>; 3] = std::array::from_fn(|ch| {
                (0..1501)
                    .map(|n| ((n as f64 * (0.173 + ch as f64 * 0.037)).sin() * 0.5) as $t)
                    .collect()
            });
            let mut states = [[0.; 4]; 3];
            let mut wet: [Vec<f64>; 3] = std::array::from_fn(|_| Vec::new());
            for n in 0usize..1501 {
                if n.is_multiple_of(97) {
                    states = [[0.; 4]; 3];
                }
                let design = profiles[(n / 31) % 6];
                let mode = (n / 17) % 6;
                let bypassed = (n / 23) % 3 == 1;
                for ch in 0..3 {
                    let x = input[ch][n] as f64;
                    let y = if bypassed {
                        x
                    } else {
                        nodal(
                            &mut states[ch],
                            x,
                            design.gain() as f64,
                            design.feedback() as f64,
                            design.input_drive() as f64,
                            true,
                        )[mode]
                    };
                    wet[ch].push(y);
                }
            }
            let reference: [Vec<f64>; 3] = std::array::from_fn(|ch| {
                (0..1501)
                    .map(|n| {
                        TAPS.iter()
                            .enumerate()
                            .filter(|(i, _)| *i <= n)
                            .map(|(i, t)| t * wet[ch][n - i])
                            .sum()
                    })
                    .collect()
            });
            let mut maxerror = 0f64;
            for inplace in [false, true] {
                let counter = Arc::new(AtomicUsize::new(0));
                let consumer = Consumer {
                    bank: <$bank>::new(profiles[0], 3, 67).unwrap(),
                    profiles,
                    input: std::array::from_fn(|_| vec![0.; 67]),
                    wet: std::array::from_fn(|_| vec![0.; 67]),
                    fir: std::array::from_fn(|_| FirConvolver::new(&TAPS).unwrap()),
                    position: 0,
                    config: None,
                    inplace,
                    frames: counter.clone(),
                };
                let routes: Vec<Connection<$t>> = (0..3)
                    .flat_map(|ch| {
                        [
                            Connection {
                                source: Source::Input(ch),
                                destination: Destination::Node {
                                    id: NodeId(0),
                                    channel: ch,
                                },
                                gain: 1.,
                            },
                            Connection {
                                source: Source::Node {
                                    id: NodeId(0),
                                    channel: ch,
                                },
                                destination: Destination::Output(ch),
                                gain: 1.,
                            },
                        ]
                    })
                    .collect();
                let mut graph = Graph::new(
                    AudioLayout::new(3, 3).unwrap(),
                    vec![GraphNode {
                        id: NodeId(0),
                        layout: AudioLayout::new(3, 3).unwrap(),
                        processor: Box::new(consumer),
                    }],
                    &routes,
                )
                .unwrap();
                let mut output: [Vec<$t>; 3] = std::array::from_fn(|_| vec![99.; 1504]);
                for _ in 0..2 {
                    graph
                        .prepare(
                            ProcessConfig::new(48000., 67, 0, AudioLayout::new(3, 3).unwrap()).unwrap(),
                        )
                        .unwrap();
                    let mut done = 0;
                    let mut turn = 0;
                    let info = allocation_counter::measure(|| {
                        while done < 1501 {
                            let count = [1, 67, 0, 2, 31, 5][turn % 6].min(1501 - done);
                            turn += 1;
                            let inputs = input.each_ref().map(|v| &v[done..done + count]);
                            let [a, b, c] = &mut output;
                            let mut outputs = [
                                &mut a[done..done + count],
                                &mut b[done..done + count],
                                &mut c[done..done + count],
                            ];
                            let audio = AudioBuffer::separate(&inputs, &mut outputs, count).unwrap();
                            assert_eq!(
                                graph.process(&mut ProcessBlock::new(audio, &[], Transport::default())),
                                GraphStatus::Continue
                            );
                            done += count;
                        }
                    });
                    assert_eq!(info.count_total, 0);
                    assert_eq!(info.count_current, 0);
                    assert_eq!(counter.load(Ordering::Relaxed), 1501);
                    for ch in 0..3 {
                        assert_eq!(&output[ch][1501..], &[99.; 3]);
                        for (&a, &e) in output[ch][..1501].iter().zip(&reference[ch]) {
                            let error = (a as f64 - e).abs();
                            maxerror = maxerror.max(error);
                            assert!(
                                error < $tol * (1. + e.abs()),
                                "inplace{inplace} error{error:e} expected{e:e}"
                            );
                        }
                    }
                    // Poison one channel through an actual callback; other channels
                    // remain finite. Callback reset restores every bank/FIR history.
                    graph.reset();
                    let mut a = [<$t>::NAN];
                    let mut b = [0.];
                    let mut c = [0.];
                    let mut views = [&mut a[..], &mut b[..], &mut c[..]];
                    let audio = AudioBuffer::in_place(&mut views, 1).unwrap();
                    assert_eq!(
                        graph.process(&mut ProcessBlock::new(audio, &[], Transport::default())),
                        GraphStatus::Continue
                    );
                    assert!(a[0].is_nan());
                    assert!(b[0].is_finite() && c[0].is_finite());
                    graph.reset();
                    let input = input.each_ref().map(|v| &v[..1]);
                    let mut a = [0.];
                    let mut b = [0.];
                    let mut c = [0.];
                    let mut views = [&mut a[..], &mut b[..], &mut c[..]];
                    let audio = AudioBuffer::separate(&input, &mut views, 1).unwrap();
                    assert_eq!(
                        graph.process(&mut ProcessBlock::new(audio, &[], Transport::default())),
                        GraphStatus::Continue
                    );
                    assert!(a[0].is_finite() && b[0].is_finite() && c[0].is_finite());
                    for (ch, y) in [a[0], b[0], c[0]].into_iter().enumerate() {
                        assert!((y as f64 - reference[ch][0]).abs() < $tol);
                    }
                    graph.deactivate();
                }
            }
            println!(
                "actual PLANAR/INPLACE Graph->FIR native{} independent design31/mode17/bypass23/replace97 max absolute error={maxerror:e}",
                std::mem::size_of::<$t>() * 8
            );
        }
    }
}
graph!(
    native64_actual_planar_inplace_graph_fir_retune_modes_bypass_poison_reset,
    f64,
    B,
    D,
    |d: D| d,
    3e-11
);
graph!(
    native32_actual_planar_inplace_graph_fir_retune_modes_bypass_poison_reset,
    f32,
    B32,
    ravet_signal::LadderDesign32,
    |d: D| d.quantize_f32().unwrap(),
    1e-5
);
