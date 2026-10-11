use ravet_signal::{
    FirstOrderTptDesign as D, Phaser as P, Phaser32 as P32, PhaserConfig as C, PhaserControls as K,
    PhaserError as E,
};
fn controls() -> K {
    K {
        rate_hz: 1.3,
        depth: 0.5,
        center_frequency_hz: 1000.,
        feedback: 0.31,
        wet_mix: 0.5,
        phase_offset_cycles: 0.25,
    }
}
fn config(channels: usize, rate: f64) -> C {
    C {
        channels,
        sample_rate: rate,
        maximum_frames: 257,
    }
}
fn close(a: f64, b: f64, e: f64) {
    assert!(
        (a - b).abs() <= e,
        "{a} != {b}, error {} > {e}",
        (a - b).abs()
    );
}
// Independent trapezoidal nodal solve, distinct from production G*(x-s) then
// sum/difference. g derives actual native G, without using filter execution.
fn stage(s: &mut f64, x: f64, gain: f64) -> f64 {
    let g = gain / (1. - gain);
    let low = (g * x + *s) / (1. + g);
    *s = ((1. - g) * *s + 2. * g * x) / (1. + g);
    2. * low - x
}
#[test]
fn whole_floating_sweep_endpoints_and_exact_admitted_native_coefficients() {
    for rate in [8000., 44100., 48000., 96000., 192000., 768000.] {
        for depth in [0., 0.25, 0.5, 1.] {
            let factor = (4_f64 * depth).exp2();
            for center in [rate * 1.01e-6 * factor, rate * 0.03, rate * 0.489 / factor] {
                let k = K {
                    center_frequency_hz: center,
                    depth,
                    rate_hz: 100.,
                    ..controls()
                };
                let p = P::new(config(2, rate), k);
                let q = P32::new(config(2, rate), k);
                assert_eq!(p.is_ok(), q.is_ok());
                if p.is_err() {
                    continue;
                }
                let mut p = p.unwrap();
                let mut q = q.unwrap();
                let [low, high] = p.sweep_endpoints_hz();
                assert!(low >= rate * 1e-6 && high <= rate * 0.49);
                assert_eq!(p.controls(), k);
                assert_eq!(q.controls(), k);
                for i in 0..=1024 {
                    let phase = if i == 1024 {
                        f64::from_bits(1_f64.to_bits() - 1)
                    } else {
                        i as f64 / 1024.
                    };
                    p.reset_to_phase(phase).unwrap();
                    q.reset_to_phase(phase).unwrap();
                    for ch in 0..2 {
                        let fc = p.next_cutoff_hz(ch).unwrap();
                        assert_eq!(fc, q.next_cutoff_hz(ch).unwrap());
                        // A floating envelope observation: no hidden cutoff projection.
                        assert!(fc >= low && fc <= high, "fc{fc} outside[{low},{high}]");
                        let expected = center
                            * 2_f64.powf(
                                4. * depth
                                    * (std::f64::consts::TAU * (phase + ch as f64 * 0.25).fract())
                                        .sin(),
                            );
                        close(fc, expected, high * 3e-15);
                        let admitted = D::new(rate, fc).unwrap();
                        assert_eq!(
                            p.next_gain(ch).unwrap().to_bits(),
                            admitted.gain().to_bits()
                        );
                        assert_eq!(
                            q.next_gain(ch).unwrap().to_bits(),
                            admitted.quantize_f32().unwrap().gain().to_bits()
                        );
                    }
                }
            }
            // Actual floating extrema on both original normalized/Hz boundaries.
            for boundary_center in [rate * 1e-6 * factor, rate * 0.49 / factor] {
                let k = K {
                    center_frequency_hz: boundary_center,
                    depth,
                    ..controls()
                };
                let low = boundary_center * (-4. * depth).exp2();
                let high = boundary_center * (4. * depth).exp2();
                let admitted = low >= rate * 1e-6
                    && high <= rate * 0.49
                    && (1e-6..=0.49).contains(&(low / rate))
                    && (1e-6..=0.49).contains(&(high / rate));
                assert_eq!(P::new(config(1, rate), k).is_ok(), admitted);
                assert_eq!(P32::new(config(1, rate), k).is_ok(), admitted);
            }
        }
    }
}
macro_rules! dynamic {
    ($name:ident,$t:ty,$p:ty,$tol:expr) => {
        #[test]
        fn $name() {
            for channels in 1..=64 {
                let c = config(channels, 48000.);
                let k = controls();
                let mut p = <$p>::new(c, k).unwrap();
                let mut states = vec![[0.; 6]; channels];
                let mut feedback = vec![0.; channels];
                let mut phase = 0.;
                let mut input = vec![0. as $t; channels];
                let mut out = vec![99. as $t; channels];
                for i in 0..1024 {
                    let current = if i < 512 {
                        k
                    } else {
                        K {
                            rate_hz: 0.,
                            depth: 1.,
                            center_frequency_hz: 1000.,
                            feedback: -0.47,
                            wet_mix: 0.73,
                            phase_offset_cycles: 0.37,
                        }
                    };
                    if i == 512 {
                        let before = p.phase_cycles();
                        p.set_controls(current).unwrap();
                        assert_eq!(p.phase_cycles(), before);
                    }
                    for (ch, x) in input.iter_mut().enumerate() {
                        *x = ((i as f64 * (0.17 + ch as f64 * 0.009)).sin() * 0.31) as $t;
                    }
                    for ch in 0..channels {
                        let fc = p.next_cutoff_hz(ch).unwrap();
                        let gain = p.next_gain(ch).unwrap() as f64;
                        let expected = current.center_frequency_hz
                            * 2_f64.powf(
                                4. * current.depth
                                    * (std::f64::consts::TAU
                                        * (phase
                                            + (ch as f64 * current.phase_offset_cycles).fract())
                                        .fract())
                                    .sin(),
                            );
                        close(fc, expected, 1e-11);
                        let beta = current.feedback as $t as f64;
                        let mix = current.wet_mix as $t as f64;
                        let mut wet = input[ch] as f64 + beta * feedback[ch];
                        for state in &mut states[ch] {
                            wet = stage(state, wet, gain);
                        }
                        feedback[ch] = wet;
                        let expected = (1. - mix) * input[ch] as f64 + mix * wet;
                        // Independent native coefficient circuit observation; reference
                        // arithmetic remains f64, execution error qualified separately.
                        out[ch] = expected as $t;
                    }
                    let expected = out.clone();
                    p.process_frame(&input, &mut out).unwrap();
                    for ch in 0..channels {
                        close(out[ch] as f64, expected[ch] as f64, $tol);
                    }
                    phase += current.rate_hz / c.sample_rate;
                    if phase >= 1. {
                        phase -= 1.;
                    }
                    assert_eq!(p.phase_cycles(), phase);
                }
                p.reset();
                assert_eq!(p.phase_cycles(), 0.);
                let mut fresh = <$p>::new(
                    c,
                    K {
                        rate_hz: 0.,
                        depth: 1.,
                        center_frequency_hz: 1000.,
                        feedback: -0.47,
                        wet_mix: 0.73,
                        phase_offset_cycles: 0.37,
                    },
                )
                .unwrap();
                let mut a = vec![0.; channels];
                let mut b = vec![0.; channels];
                for _ in 0..17 {
                    p.process_frame(&input, &mut a).unwrap();
                    fresh.process_frame(&input, &mut b).unwrap();
                    assert_eq!(a, b);
                }
            }
        }
    };
}
dynamic!(
    native64_all_channels_time_varying_nodal_feedback_and_phase,
    f64,
    P,
    4e-12
);
dynamic!(
    native32_all_channels_actual_coefficient_nodal_feedback_and_phase,
    f32,
    P32,
    3e-5
);
macro_rules! lifecycle {
    ($name:ident,$t:ty,$p:ty) => {
        #[test]
        fn $name() {
            let c = config(3, 48000.);
            let k = controls();
            let initial = <$p>::new(c, k).unwrap();
            let input: [Vec<$t>; 3] = std::array::from_fn(|ch| {
                (0..257)
                    .map(|i| ((i as f64 * (0.173 + ch as f64 * 0.031)).sin() * 0.7) as $t)
                    .collect()
            });
            let mut scalar = initial.clone();
            let mut expected = vec![[0. as $t; 3]; 257];
            for i in 0..257 {
                scalar
                    .process_frame(&[input[0][i], input[1][i], input[2][i]], &mut expected[i])
                    .unwrap();
            }
            for inplace in [false, true] {
                for split in [false, true] {
                    let mut p = initial.clone();
                    let mut start = 0;
                    let mut turn = 0;
                    while start < 257 {
                        let count = if split {
                            [1, 17, 0, 7, 31, 2][turn % 6].min(257 - start)
                        } else {
                            257
                        };
                        let mut y = vec![vec![99. as $t; count + 3]; 3];
                        if inplace {
                            for ch in 0..3 {
                                y[ch][..count].copy_from_slice(&input[ch][start..start + count]);
                            }
                        }
                        let mut out: Vec<_> = y.iter_mut().map(Vec::as_mut_slice).collect();
                        let inputs = [
                            &input[0][start..start + count],
                            &input[1][start..start + count],
                            &input[2][start..start + count],
                        ];
                        let measured = allocation_counter::measure(|| {
                            if inplace {
                                p.process_planar_in_place(&mut out, count).unwrap();
                            } else {
                                p.process_planar(&inputs, &mut out, count).unwrap();
                            }
                        });
                        assert_eq!(measured.count_total, 0);
                        assert_eq!(measured.count_current, 0);
                        for ch in 0..3 {
                            for i in 0..count {
                                assert_eq!(y[ch][i], expected[start + i][ch]);
                            }
                            assert!(y[ch][count..].iter().all(|&v| v == 99.));
                        }
                        start += count;
                        turn += 1;
                    }
                    assert_eq!(p.phase_cycles(), scalar.phase_cycles());
                }
            }
            let mut p = initial.clone();
            let mut tmp = [0.; 3];
            for _ in 0..11 {
                p.process_frame(&[0.2, 0.7, -0.3], &mut tmp).unwrap();
            }
            let mut saved = p.clone();
            let mut y = [[99. as $t; 259]; 3];
            {
                let [a, b, d] = &mut y;
                assert_eq!(
                    p.process_planar(&[&input[0], &input[1], &input[2][..2]], &mut [a, b, d], 3),
                    Err(E::BufferTooShort)
                );
            }
            {
                let [a, b, d] = &mut y;
                assert_eq!(
                    p.process_planar(
                        &[&input[0], &input[1], &input[2]],
                        &mut [a, b, &mut d[..2]],
                        3
                    ),
                    Err(E::BufferTooShort)
                );
            }
            {
                let [a, b, d] = &mut y;
                assert_eq!(
                    p.process_planar_in_place(&mut [a, b, &mut d[..2]], 3),
                    Err(E::BufferTooShort)
                );
            }
            {
                let [a, b, d] = &mut y;
                assert_eq!(
                    p.process_planar(&[&[0.; 258], &[0.; 258], &[0.; 258]], &mut [a, b, d], 258),
                    Err(E::TooManyFrames)
                );
            }
            {
                let [a, b, d] = &mut y;
                assert_eq!(
                    p.process_planar_in_place(&mut [a, b, d], 258),
                    Err(E::TooManyFrames)
                );
            }
            {
                let [a, b, _] = &mut y;
                assert_eq!(
                    p.process_planar(&[&input[0], &input[1], &input[2]], &mut [a, b], 1),
                    Err(E::ChannelCount)
                );
            }
            {
                let [a, b, d] = &mut y;
                assert_eq!(
                    p.process_planar(&[&input[0], &input[1]], &mut [a, b, d], 1),
                    Err(E::ChannelCount)
                );
            }
            {
                let [a, b, _] = &mut y;
                assert_eq!(
                    p.process_planar_in_place(&mut [a, b], 1),
                    Err(E::ChannelCount)
                );
            }
            assert_eq!(
                p.process_frame(&[0.; 2], &mut [99.; 3]),
                Err(E::ChannelCount)
            );
            for phase in [-1., 1., f64::NAN, f64::INFINITY] {
                assert_eq!(p.reset_to_phase(phase), Err(E::InvalidPhase));
            }
            let mut invalid = Vec::new();
            for value in [-1., 1.00001, f64::NAN, f64::INFINITY] {
                let mut c = k;
                c.depth = value;
                invalid.push(c);
                c = k;
                c.wet_mix = value;
                invalid.push(c);
                c = k;
                c.phase_offset_cycles = value;
                invalid.push(c);
            }
            for value in [-1., 1e-300, 0.00099, 100.001, f64::NAN, f64::INFINITY] {
                let mut c = k;
                c.rate_hz = value;
                invalid.push(c);
            }
            for value in [
                -1.,
                0.,
                f64::from_bits(1),
                48000.,
                f64::MAX,
                f64::NAN,
                f64::INFINITY,
            ] {
                let mut c = k;
                c.center_frequency_hz = value;
                invalid.push(c);
            }
            for value in [-1., 1., -0.990001, 0.990001, f64::NAN, f64::INFINITY] {
                let mut c = k;
                c.feedback = value;
                invalid.push(c);
            }
            for k in invalid {
                assert_eq!(p.set_controls(k), Err(E::InvalidControls));
            }
            assert!(y.iter().all(|ch| ch.iter().all(|&v| v == 99.)));
            assert_eq!(p.phase_cycles(), saved.phase_cycles());
            assert_eq!(p.controls(), saved.controls());
            let mut actual = [0.; 3];
            let mut expected = [0.; 3];
            for _ in 0..71 {
                p.process_frame(&[0.3, -0.2, 0.7], &mut actual).unwrap();
                saved
                    .process_frame(&[0.3, -0.2, 0.7], &mut expected)
                    .unwrap();
                assert_eq!(actual, expected);
            }
            let phase = p.phase_cycles();
            p.process_planar(&[&[], &[], &[]], &mut [&mut [], &mut [], &mut []], 0)
                .unwrap();
            p.process_planar_in_place(&mut [&mut [], &mut [], &mut []], 0)
                .unwrap();
            assert_eq!(p.phase_cycles(), phase);
            let measured = allocation_counter::measure(|| {
                for _ in 0..1000 {
                    let mut p = <$p>::new(c, k).unwrap();
                    p.set_controls(k).unwrap();
                    p.reset_to_phase(0.25).unwrap();
                    p.process_frame(&[0.3, -0.71, 0.17], &mut [0.; 3]).unwrap();
                    p.reset();
                    assert!(
                        p.set_controls(K {
                            depth: f64::NAN,
                            ..k
                        })
                        .is_err()
                    );
                }
            });
            assert_eq!(measured.count_total, 0);
            assert_eq!(measured.count_current, 0);
        }
    };
}
lifecycle!(
    native64_partition_late_preflight_and_retained_history_heap_proof,
    f64,
    P
);
lifecycle!(
    native32_partition_late_preflight_and_retained_history_heap_proof,
    f32,
    P32
);
fn wet_state(state: &mut [f64; 7], input: f64, gain: f64, beta: f64) -> f64 {
    let mut v = input + beta * state[6];
    for s in &mut state[..6] {
        v = stage(s, v, gain);
    }
    state[6] = v;
    v
}
fn multiply(a: [[f64; 7]; 7], b: [[f64; 7]; 7]) -> [[f64; 7]; 7] {
    std::array::from_fn(|i| std::array::from_fn(|j| (0..7).map(|k| a[i][k] * b[k][j]).sum()))
}
fn tail_bound(state: [f64; 7], gain: f64, beta: f64) -> f64 {
    let mut matrix = [[0.; 7]; 7];
    let mut c = [0.; 7];
    for column in 0..7 {
        let mut basis = [0.; 7];
        basis[column] = 1.;
        c[column] = wet_state(&mut basis, 0., gain, beta);
        for row in 0..7 {
            matrix[row][column] = basis[row];
        }
    }
    let mut power = matrix;
    let mut block = 1;
    loop {
        let norm = power
            .iter()
            .map(|row| row.iter().map(|v| v.abs()).sum::<f64>())
            .fold(0_f64, f64::max);
        if norm < 0.5 {
            let mut row = c;
            let mut sum = 0.;
            for _ in 0..block {
                sum += row.iter().map(|v| v.abs()).sum::<f64>();
                row = std::array::from_fn(|j| (0..7).map(|k| row[k] * matrix[k][j]).sum());
            }
            return sum * state.iter().map(|v| v.abs()).fold(0_f64, f64::max) / (1. - norm);
        }
        assert!(
            block < 131072,
            "independent fixture block does not contract"
        );
        power = multiply(power, power);
        block *= 2;
    }
}
fn rational(gain: f64, beta: f64, mix: f64, f: f64) -> [f64; 2] {
    fn mul(a: [f64; 2], b: [f64; 2]) -> [f64; 2] {
        [a[0] * b[0] - a[1] * b[1], a[0] * b[1] + a[1] * b[0]]
    }
    fn div(a: [f64; 2], b: [f64; 2]) -> [f64; 2] {
        let d = b[0] * b[0] + b[1] * b[1];
        [
            (a[0] * b[0] + a[1] * b[1]) / d,
            (a[1] * b[0] - a[0] * b[1]) / d,
        ]
    }
    let z = [
        (std::f64::consts::TAU * f).cos(),
        -(std::f64::consts::TAU * f).sin(),
    ];
    let p = 1. - 2. * gain;
    let a = div([z[0] - p, z[1]], [1. - p * z[0], -p * z[1]]);
    let six = (0..6).fold([1., 0.], |acc, _| mul(acc, a));
    let delay = mul(z, six);
    let wet = div(six, [1. - beta * delay[0], -beta * delay[1]]);
    [(1. - mix) + mix * wet[0], mix * wet[1]]
}
macro_rules! fixed{
    ($name:ident,$t:ty,$p:ty,$time_tol:expr,$spectrum_tol:expr)=>{
        #[test]fn $name(){
            let mut max_time=0_f64;let mut max_spectrum=0_f64;
            for ratio in [0.07,0.13,0.25]{for beta in [-0.99,0.,0.99]{
                let k=K{rate_hz:0.,depth:0.,center_frequency_hz:48000.*ratio,feedback:beta,wet_mix:1.,phase_offset_cycles:0.};let mut p=<$p>::new(config(1,48000.),k).unwrap();
                let gain=p.next_gain(0).unwrap()as f64;let beta=p.native_feedback()as f64;let mut state=[0.;7];let mut actual=vec![0.;131072];
                for (i,y)in actual.iter_mut().enumerate(){let input=if i==0{1.}else{0.};let mut out=[0. as $t];p.process_frame(&[input],&mut out).unwrap();*y=out[0]as f64;
                    let expected=wet_state(&mut state,input as f64,gain,beta);max_time=max_time.max((*y-expected).abs());close(*y,expected,$time_tol);
                }
                let tail=tail_bound(state,gain,beta);assert!(tail<1e-12,"tail{tail} exceeds qualified fixture");
                for f in [0.,0.031,0.07,0.173,0.25,0.49]{
                    let expected=rational(gain,beta,1.,f);let re:f64=actual.iter().enumerate().map(|(i,&y)|y*(std::f64::consts::TAU*f*i as f64).cos()).sum();let im:f64=actual.iter().enumerate().map(|(i,&y)|-y*(std::f64::consts::TAU*f*i as f64).sin()).sum();
                    let error=(re-expected[0]).hypot(im-expected[1]);max_spectrum=max_spectrum.max(error);assert!(error<=tail+$spectrum_tol*(1.+expected[0].hypot(expected[1])),"error{error},expected{expected:?},tail{tail},beta{beta},ratio{ratio}");
                }
                close(rational(gain,beta,1.,0.)[0],1./(1.-beta),1e-10);close(rational(gain,beta,1.,0.5)[0],1./(1.+beta),1e-10);
            }}
            println!("{} max native time error{max_time:e}; measured rational spectrum error{max_spectrum:e}",stringify!($name));
        }
    }
}
fixed!(
    native64_static_signed_feedback_independent_state_rational_and_block_tail,
    f64,
    P,
    2e-12,
    5e-11
);
fixed!(
    native32_static_signed_feedback_actual_parameters_rational_and_block_tail,
    f32,
    P32,
    2e-6,
    4e-6
);
#[test]
fn independent_three_notches_move_with_actual_center_and_unmixed_allpass() {
    for ratio in [0.001, 0.01, 0.07, 0.13, 0.25, 0.49] {
        let k = K {
            rate_hz: 0.,
            depth: 0.,
            center_frequency_hz: 48000. * ratio,
            feedback: 0.,
            wet_mix: 0.5,
            phase_offset_cycles: 0.,
        };
        let p = P::new(config(1, 48000.), k).unwrap();
        let q = P32::new(config(1, 48000.), k).unwrap();
        for gain in [p.next_gain(0).unwrap(), q.next_gain(0).unwrap() as f64] {
            let g = gain / (1. - gain);
            let mut last = 0.;
            for angle in [
                std::f64::consts::PI / 12.,
                std::f64::consts::PI / 4.,
                5. * std::f64::consts::PI / 12.,
            ] {
                let f = (g * angle.tan()).atan() / std::f64::consts::PI;
                assert!(last < f && f < 0.5);
                last = f;
                let y = rational(gain, 0., 0.5, f);
                assert!(y[0].hypot(y[1]) < 1e-13);
                let adjacent = rational(gain, 0., 0.5, (f + 0.00001).min(0.5));
                assert!(adjacent[0].hypot(adjacent[1]) > 1e-7);
            }
            for i in 0..=1024 {
                let y = rational(gain, 0., 1., i as f64 / 2048.);
                close(y[0].hypot(y[1]), 1., 3e-10);
            }
        }
    }
    // Actual impulse-derived response qualifies cancellation separately from
    // evaluating its own closed expression at analytically chosen zeros.
    let k = K {
        rate_hz: 0.,
        depth: 0.,
        center_frequency_hz: 3360.,
        feedback: 0.,
        wet_mix: 0.5,
        phase_offset_cycles: 0.,
    };
    let mut p = P::new(config(1, 48000.), k).unwrap();
    let gain = p.next_gain(0).unwrap();
    let g = gain / (1. - gain);
    let frequencies = [
        std::f64::consts::PI / 12.,
        std::f64::consts::PI / 4.,
        5. * std::f64::consts::PI / 12.,
    ]
    .map(|angle| (g * angle.tan()).atan() / std::f64::consts::PI);
    let mut response = vec![0.; 4096];
    for (i, y) in response.iter_mut().enumerate() {
        p.process_frame(&[if i == 0 { 1. } else { 0. }], std::slice::from_mut(y))
            .unwrap();
    }
    for f in frequencies {
        let re: f64 = response
            .iter()
            .enumerate()
            .map(|(i, &v)| v * (std::f64::consts::TAU * f * i as f64).cos())
            .sum();
        let im: f64 = response
            .iter()
            .enumerate()
            .map(|(i, &v)| -v * (std::f64::consts::TAU * f * i as f64).sin())
            .sum();
        assert!(re.hypot(im) < 2e-14);
    }
    let mut q = P32::new(config(1, 48000.), k).unwrap();
    let gain = q.next_gain(0).unwrap() as f64;
    let g = gain / (1. - gain);
    let frequencies = [
        std::f64::consts::PI / 12.,
        std::f64::consts::PI / 4.,
        5. * std::f64::consts::PI / 12.,
    ]
    .map(|angle| (g * angle.tan()).atan() / std::f64::consts::PI);
    let mut response = vec![0_f32; 4096];
    for (i, y) in response.iter_mut().enumerate() {
        q.process_frame(&[if i == 0 { 1. } else { 0. }], std::slice::from_mut(y))
            .unwrap();
    }
    for f in frequencies {
        let re: f64 = response
            .iter()
            .enumerate()
            .map(|(i, &v)| f64::from(v) * (std::f64::consts::TAU * f * i as f64).cos())
            .sum();
        let im: f64 = response
            .iter()
            .enumerate()
            .map(|(i, &v)| -f64::from(v) * (std::f64::consts::TAU * f * i as f64).sin())
            .sum();
        assert!(re.hypot(im) < 2e-6); // finite native32 cancellation observation
    }
}
macro_rules! endpoints {
    ($name:ident,$t:ty,$p:ty) => {
        #[test]
        fn $name() {
            let k = K {
                rate_hz: 100.,
                depth: 1.,
                center_frequency_hz: 1000.,
                feedback: 0.31,
                wet_mix: 0.,
                phase_offset_cycles: 0.25,
            };
            let c = config(2, 8000.);
            // A full sweep reaching above Nyquist is rejected before owner creation.
            assert!(matches!(<$p>::new(c, k), Err(E::InvalidControls)));
            let c = config(2, 48000.);
            let mut dry = <$p>::new(c, k).unwrap();
            let mut wet = <$p>::new(c, K { wet_mix: 1., ..k }).unwrap();
            let mut a = [0.; 2];
            let mut b = [0.; 2];
            for i in 0..1024 {
                let x = [
                    (i as f64 * 0.173).sin() as $t,
                    (i as f64 * 0.271).cos() as $t,
                ];
                dry.process_frame(&x, &mut a).unwrap();
                wet.process_frame(&x, &mut b).unwrap();
                assert_eq!(a, x);
                assert_eq!(dry.phase_cycles(), wet.phase_cycles());
            }
            dry.set_controls(K { wet_mix: 1., ..k }).unwrap();
            dry.process_frame(&[0.13, -0.31], &mut a).unwrap();
            wet.process_frame(&[0.13, -0.31], &mut b).unwrap();
            assert_eq!(a, b);
            let before = dry.phase_cycles();
            dry.set_controls(K {
                rate_hz: 0.,
                wet_mix: 1.,
                ..k
            })
            .unwrap();
            for _ in 0..17 {
                dry.process_frame(&[0.1, -0.2], &mut a).unwrap();
                assert_eq!(dry.phase_cycles(), before);
            }
            for offset in [0., 0.25, 1.] {
                dry.set_controls(K {
                    phase_offset_cycles: offset,
                    ..k
                })
                .unwrap();
                dry.reset_to_phase(0.25).unwrap();
                assert_eq!(dry.phase_cycles(), 0.25);
                if offset == 0. || offset == 1. {
                    assert_eq!(dry.next_gain(0), dry.next_gain(1));
                }
            }
            dry.reset();
            dry.process_frame(&[<$t>::NAN, 0.1], &mut a).unwrap();
            assert!(a[0].is_nan());
            assert!(a[1].is_finite());
            dry.set_controls(K {
                wet_mix: 0.,
                feedback: 0.,
                ..k
            })
            .unwrap();
            dry.process_frame(&[0.7, -0.3], &mut a).unwrap();
            assert_eq!(a, [0.7, -0.3]);
            dry.set_controls(K {
                wet_mix: 1.,
                feedback: 0.,
                ..k
            })
            .unwrap();
            dry.process_frame(&[0.7, -0.3], &mut a).unwrap();
            assert!(a[0].is_nan());
            dry.reset();
            dry.process_frame(&[0.7, -0.3], &mut a).unwrap();
            assert!(a.iter().all(|v| v.is_finite()));
            dry.set_controls(K {
                feedback: 0.99,
                depth: 0.,
                wet_mix: 1.,
                ..k
            })
            .unwrap();
            dry.reset();
            for _ in 0..1000 {
                dry.process_frame(&[<$t>::MAX, -<$t>::MAX], &mut a).unwrap();
            }
            assert!(a.iter().all(|v| !v.is_finite()));
            for rate in [
                0.,
                1.,
                7999.99,
                768000.01,
                f64::from_bits(1),
                f64::NAN,
                f64::INFINITY,
            ] {
                assert!(matches!(
                    <$p>::new(config(2, rate), k),
                    Err(E::InvalidSampleRate)
                ));
            }
            for ch in [0, 65, usize::MAX] {
                assert!(matches!(
                    <$p>::new(config(ch, 48000.), k),
                    Err(E::InvalidChannelCount)
                ));
            }
            for max in [0, 65537, usize::MAX] {
                assert!(matches!(
                    <$p>::new(
                        C {
                            maximum_frames: max,
                            ..config(2, 48000.)
                        },
                        k
                    ),
                    Err(E::InvalidMaximumFrames)
                ));
            }
            assert!(dry.next_gain(2).is_err());
            assert!(dry.next_cutoff_hz(2).is_err());
            dry.set_controls(K { wet_mix: 0., ..k }).unwrap();
            dry.reset_to_phase(0.999).unwrap();
            dry.process_frame(&[0.; 2], &mut a).unwrap();
            assert!(dry.phase_cycles() < 0.01);
        }
    };
}
endpoints!(
    native64_dry_history_phase_offsets_nonfinite_and_config_endpoints,
    f64,
    P
);
endpoints!(
    native32_dry_history_native_poison_and_config_endpoints,
    f32,
    P32
);
macro_rules! graph {
    ($name:ident,$t:ty,$p:ty,$tol:expr) => {
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
            struct Consumer {
                profiles: Vec<K>,
                phaser: $p,
                fir: [FirConvolver; 3],
                config: Option<ProcessConfig>,
                position: usize,
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
                    for i in 0..b.audio.frames() {
                        if self.position.is_multiple_of(29) {
                            self.phaser
                                .set_controls(
                                    self.profiles[(self.position / 29) % self.profiles.len()],
                                )
                                .unwrap();
                        }
                        let input = [
                            b.audio.input(0).unwrap()[i],
                            b.audio.input(1).unwrap()[i],
                            b.audio.input(2).unwrap()[i],
                        ];
                        let mut y = [0.; 3];
                        self.phaser.process_frame(&input, &mut y).unwrap();
                        for (ch, &v) in y.iter().enumerate() {
                            b.audio.output_mut(ch).unwrap()[i] =
                                self.fir[ch].process_sample(v as f64) as $t;
                        }
                        self.position += 1;
                    }
                    self.frames.fetch_add(b.audio.frames(), Ordering::Relaxed);
                    ProcessStatus::Continue
                }
                fn reset(&mut self) {
                    self.phaser.set_controls(self.profiles[0]).unwrap();
                    self.phaser.reset();
                    for f in &mut self.fir {
                        f.reset();
                    }
                    self.position = 0;
                    self.frames.store(0, Ordering::Relaxed);
                }
                fn deactivate(&mut self) {
                    if self.config.take().is_some() {
                        self.deactivations.fetch_add(1, Ordering::Relaxed);
                    }
                }
            }
            let profiles = vec![
                controls(),
                K {
                    rate_hz: 0.,
                    depth: 1.,
                    center_frequency_hz: 700.,
                    feedback: -0.47,
                    wet_mix: 0.73,
                    phase_offset_cycles: 0.37,
                },
                K {
                    rate_hz: 100.,
                    depth: 0.2,
                    center_frequency_hz: 9000.,
                    feedback: 0.93,
                    wet_mix: 1.,
                    phase_offset_cycles: 0.,
                },
                K {
                    rate_hz: 0.001,
                    depth: 0.,
                    center_frequency_hz: 7000.,
                    feedback: 0.,
                    wet_mix: 0.,
                    phase_offset_cycles: 1.,
                },
                K {
                    rate_hz: 37.,
                    depth: 1.,
                    center_frequency_hz: 900.,
                    feedback: -0.99,
                    wet_mix: 0.91,
                    phase_offset_cycles: 0.11,
                },
            ];
            let input: [Vec<$t>; 3] = std::array::from_fn(|ch| {
                (0..1501)
                    .map(|i| {
                        ((i as f64 * (0.173 + ch as f64 * 0.037)).sin() * (0.7 - ch as f64 * 0.13)
                            + ((i * 13 % 23) as f64 - 11.) / 97.) as $t
                    })
                    .collect()
            });
            let impulses: [&[f64]; 3] = [&[0.5, -0.25, 0.125], &[0.1, 0.7], &[-0.13, 0.3, 0.9]];
            let mut states = [[0.; 7]; 3];
            let mut phase = 0.;
            let mut selected: [Vec<f64>; 3] = [vec![], vec![], vec![]];
            for i in 0..1501 {
                let k = profiles[(i / 29) % profiles.len()];
                for ch in 0..3 {
                    let phase_ch = (phase + (ch as f64 * k.phase_offset_cycles).fract()).fract();
                    let fc = k.center_frequency_hz
                        * 2_f64.powf(4. * k.depth * (std::f64::consts::TAU * phase_ch).sin());
                    let gain = D::new(48000., fc).unwrap().gain() as $t as f64;
                    let beta = k.feedback as $t as f64;
                    let mix = k.wet_mix as $t as f64;
                    let wet = wet_state(&mut states[ch], input[ch][i] as f64, gain, beta);
                    let y = if mix == 0. {
                        input[ch][i] as f64
                    } else if mix == 1. {
                        wet
                    } else {
                        (1. - mix) * input[ch][i] as f64 + mix * wet
                    };
                    selected[ch].push(y as $t as f64);
                }
                phase += k.rate_hz / 48000.;
                if phase >= 1. {
                    phase -= 1.;
                }
            }
            let expected: [Vec<f64>; 3] = std::array::from_fn(|ch| {
                (0..1501)
                    .map(|i| {
                        impulses[ch]
                            .iter()
                            .enumerate()
                            .filter(|(n, _)| *n <= i)
                            .map(|(n, &h)| h * selected[ch][i - n])
                            .sum()
                    })
                    .collect()
            });
            let frames = Arc::new(AtomicUsize::new(0));
            let deactivations = Arc::new(AtomicUsize::new(0));
            let layout = AudioLayout::new(3, 3).unwrap();
            let node = GraphNode {
                id: NodeId(24),
                layout,
                processor: Box::new(Consumer {
                    phaser: <$p>::new(
                        C {
                            maximum_frames: 67,
                            ..config(3, 48000.)
                        },
                        profiles[0],
                    )
                    .unwrap(),
                    profiles,
                    fir: [
                        FirConvolver::new(impulses[0]).unwrap(),
                        FirConvolver::new(impulses[1]).unwrap(),
                        FirConvolver::new(impulses[2]).unwrap(),
                    ],
                    config: None,
                    position: 0,
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
                                id: NodeId(24),
                                channel,
                            },
                            gain: 1.,
                        },
                        Connection {
                            source: Source::Node {
                                id: NodeId(24),
                                channel,
                            },
                            destination: Destination::Output(channel),
                            gain: 1.,
                        },
                    ]
                })
                .collect();
            let mut graph = Graph::new(layout, vec![node], &routes).unwrap();
            for cycle in 0..2 {
                graph
                    .prepare(ProcessConfig::new(48000., 67, 0, layout).unwrap())
                    .unwrap();
                for repeat in 0..2 {
                    let mut bad = [[99. as $t; 69]; 3];
                    let [a, b, c] = &mut bad;
                    let mut out = [a.as_mut_slice(), b.as_mut_slice(), c.as_mut_slice()];
                    let sources = [&input[0][..68], &input[1][..68], &input[2][..68]];
                    let audio = AudioBuffer::separate(&sources, &mut out, 68).unwrap();
                    assert!(matches!(
                        graph.process(&mut ProcessBlock::new(audio, &[], Transport::default())),
                        GraphStatus::Fault { node: None, .. }
                    ));
                    assert_eq!(frames.load(Ordering::Relaxed), 0);
                    assert!(
                        bad.iter()
                            .all(|ch| ch[..68].iter().all(|&v| v == 0.) && ch[68] == 99.)
                    );
                    let mut start = 0;
                    let mut turn = 0;
                    while start < 1501 {
                        let count = [1, 67, 0, 2, 31, 5][turn % 6].min(1501 - start);
                        let mut y = [[99. as $t; 69]; 3];
                        let measured = allocation_counter::measure(|| {
                            let inputs = [
                                &input[0][start..start + count],
                                &input[1][start..start + count],
                                &input[2][start..start + count],
                            ];
                            let [a, b, c] = &mut y;
                            let mut out = [a.as_mut_slice(), b.as_mut_slice(), c.as_mut_slice()];
                            let audio = AudioBuffer::separate(&inputs, &mut out, count).unwrap();
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
                                close(y[ch][i] as f64, expected[ch][start + i], $tol);
                            }
                            assert!(y[ch][count..].iter().all(|&v| v == 99.));
                        }
                        start += count;
                        turn += 1;
                        assert_eq!(frames.load(Ordering::Relaxed), start);
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
    actual_native64_full_controls_multichannel_graph_fir_consumer,
    f64,
    P,
    1e-11
);
graph!(
    actual_native32_full_controls_multichannel_graph_with_explicit_f64_fir_bridge,
    f32,
    P32,
    4e-5
);
#[test]
fn independent_native64_direct_polynomial_six_stage_feedback_time() {
    for ratio in [0.03, 0.07, 0.13, 0.25, 0.49] {
        for beta in [-0.99, 0., 0.99] {
            let k = K {
                rate_hz: 0.,
                depth: 0.,
                center_frequency_hz: 48000. * ratio,
                feedback: beta,
                wet_mix: 1.,
                phase_offset_cycles: 0.,
            };
            let mut phaser = P::new(config(1, 48000.), k).unwrap();
            let gain = phaser.next_gain(0).unwrap();
            let pole = 1. - 2. * gain;
            let mut previous_x = [0.; 6];
            let mut previous_y = [0.; 6];
            let mut previous_wet = 0.;
            for i in 0..8192 {
                let input = (i as f64 * 0.173).sin() * 0.2 + ((i * 17 % 29) as f64 - 14.) / 97.;
                let mut y = input + beta * previous_wet;
                for ch in 0..6 {
                    let next = -pole * y + previous_x[ch] + pole * previous_y[ch];
                    previous_x[ch] = y;
                    previous_y[ch] = next;
                    y = next;
                }
                previous_wet = y;
                let mut actual = [0.];
                phaser.process_frame(&[input], &mut actual).unwrap();
                close(actual[0], y, 2e-10);
            }
        }
    }
}
#[test]
fn finite_million_frame_low_rate_phase_error_and_fast_wrap_observations() {
    for (rate, lfo, frames) in [(768000., 0.001, 1_000_000_usize), (8000., 100., 10001)] {
        let k = K {
            rate_hz: lfo,
            depth: 0.,
            center_frequency_hz: rate * 0.01,
            feedback: 0.,
            wet_mix: 1.,
            phase_offset_cycles: 0.,
        };
        let mut phaser = P::new(config(3, rate), k).unwrap();
        let step = lfo / rate;
        let mut max_error = 0_f64;
        for i in 1..=frames {
            let mut output = [99.; 3];
            phaser.process_frame(&[0.; 3], &mut output).unwrap();
            assert_eq!(output, [0.; 3]);
            let expected = (i as f64 * step).fract();
            let diff = (phaser.phase_cycles() - expected).abs();
            let circular = diff.min((1. - diff).abs());
            max_error = max_error.max(circular);
            let gamma = i as f64 * f64::EPSILON / (1. - i as f64 * f64::EPSILON);
            assert!(circular <= gamma * (i as f64 * step + 1.) + f64::EPSILON);
            assert!((0.0..1.).contains(&phaser.phase_cycles()));
        }
        println!(
            "phase finite rate{rate} lfo{lfo} frames{frames}, max circular ideal error{max_error:e}"
        );
    }
}
