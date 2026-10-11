use ravet_signal::{
    LadderBank as B, LadderBank32 as B32, LadderDesign as D, LadderError as E, LadderFilter as F,
    LadderFilter32 as F32, LadderMode as M,
};
const MODES: [M; 6] = [
    M::LowPass12,
    M::LowPass24,
    M::HighPass12,
    M::HighPass24,
    M::BandPass12,
    M::BandPass24,
];
fn close(a: f64, b: f64, e: f64) {
    assert!(
        (a - b).abs() <= e,
        "{a} != {b}; error {} > {e}",
        (a - b).abs()
    );
}
// Independent four-equation Newton/nodal solve with pivoted dense Gaussian
// elimination. No production scalar affine reduction or bisection reused.
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
#[test]
fn immutable_full_profile_original_native_boundaries_and_no_alloc() {
    let measured = allocation_counter::measure(|| {
        for rate in [8000., 44100., 48000., 96000., 192000., 768000.] {
            for ratio in [1.01e-5, 0.001, 0.03, 0.13, 0.25, 0.4499] {
                for r in [0., 0.1, 0.5, 0.9, 1.] {
                    for drive in [1., 1.5, 8., 32.] {
                        let d = D::new(rate, rate * ratio, r, drive).unwrap();
                        let q = d.quantize_f32().unwrap();
                        assert_eq!(d.sample_rate(), rate);
                        assert_eq!(d.cutoff_hz(), rate * ratio);
                        assert_eq!(d.resonance(), r);
                        assert_eq!(d.input_drive(), drive);
                        assert_eq!(q.prototype(), d);
                        assert_eq!(q.gain(), d.gain() as f32);
                        assert_eq!(q.feedback(), (4. * r) as f32);
                        assert!(q.pole().abs() < 1.);
                    }
                }
            }
        }
        for rate in [
            0.,
            7999.99,
            768000.01,
            f64::from_bits(1),
            f64::NAN,
            f64::INFINITY,
        ] {
            assert_eq!(D::new(rate, 1000., 0.5, 1.), Err(E::SampleRate));
        }
        for fc in [
            0.,
            -1.,
            0.479,
            21600.01,
            f64::from_bits(1),
            f64::NAN,
            f64::INFINITY,
        ] {
            assert_eq!(D::new(48000., fc, 0.5, 1.), Err(E::Cutoff));
        }
        for r in [-1e-30, 1.0001, f64::NAN, f64::INFINITY] {
            assert_eq!(D::new(48000., 1000., r, 1.), Err(E::Resonance));
        }
        for drive in [0., 0.9999, 32.0001, f64::NAN, f64::INFINITY] {
            assert_eq!(D::new(48000., 1000., 0.5, drive), Err(E::Drive));
        }
        assert_eq!(
            D::new(48000., 1000., f64::from_bits(1), 1.)
                .unwrap()
                .quantize_f32(),
            Err(E::NativePrecision)
        );
    });
    assert_eq!(measured.count_total, 0);
    assert_eq!(measured.count_current, 0);
}
macro_rules! dynamic {
    ($name:ident,$t:ty,$f:ty,$convert:expr,$tol:expr) => {
        #[test]
        fn $name() {
            let mut maximum_absolute = 0_f64;
            let mut maximum_normalized = 0_f64;
            for ratio in [0.005, 0.07, 0.25, 0.449] {
                for resonance in [0., 0.5, 0.99, 1.] {
                    for drive in [1., 2., 8.] {
                        let design =
                            $convert(D::new(48000., 48000. * ratio, resonance, drive).unwrap());
                        let mut f = <$f>::new(design);
                        let mut state = [0.; 4];
                        for i in 0..2048 {
                            let x = (((i as f64 * 0.317).sin() * 0.23)
                                + ((i * 7 % 13) as f64 - 6.) / 41.)
                                as $t;
                            let actual = f.process_sample(x);
                            let expected = nodal(
                                &mut state,
                                x as f64,
                                design.gain() as f64,
                                design.feedback() as f64,
                                design.input_drive() as f64,
                                true,
                            );
                            for (mode, &e) in MODES.iter().zip(&expected) {
                                let error = (actual.select(*mode) as f64 - e).abs();
                                maximum_absolute = maximum_absolute.max(error);
                                maximum_normalized = maximum_normalized.max(error / (1. + e.abs()));
                                close(
                                    actual.select(*mode) as f64,
                                    e,
                                    $tol * (1. + e.abs()),
                                );
                            }
                            let info = f.last_solve();
                            assert!(
                                info.iterations
                                    <= if std::mem::size_of::<$t>() == 8 {
                                        64
                                    } else {
                                        32
                                    }
                            );
                            assert!(info.scalar_residual.abs() <= 32. * <$t>::EPSILON);
                            assert!(info.staged_residual.abs() <= 256. * <$t>::EPSILON);
                        }
                    }
                }
            }
            println!(
                "DYNAMIC nodal native{} max absolute={maximum_absolute:e}, max normalized={maximum_normalized:e}",
                std::mem::size_of::<$t>() * 8
            );
        }
    };
}
dynamic!(
    native64_all_modal_dynamic_nodal_equations_and_bounded_residual,
    f64,
    F,
    |d: D| d,
    2e-13
);
dynamic!(
    native32_all_modal_dynamic_nodal_equations_and_bounded_residual,
    f32,
    F32,
    |d: D| d.quantize_f32().unwrap(),
    4e-6
);
macro_rules! partitions {
    ($name:ident,$t:ty,$b:ty,$convert:expr) => {
        #[test]
        fn $name() {
            let d = $convert(D::new(48000., 2400., 0.79, 2.).unwrap());
            for channels in 1..=64 {
                for mode in MODES {
                    let mut frame = <$b>::new(d, channels, 79).unwrap();
                    let mut planar = frame.clone();
                    let mut inplace = frame.clone();
                    let input: Vec<Vec<$t>> = (0..channels)
                        .map(|ch| {
                            (0..127)
                                .map(|i| {
                                    ((i as f64 * (0.21 + ch as f64 * 0.003)).sin() * 0.3) as $t
                                })
                                .collect()
                        })
                        .collect();
                    let mut expected = vec![vec![0.; 127]; channels];
                    let mut row = vec![0.; channels];
                    for i in 0..127 {
                        let x: Vec<$t> = input.iter().map(|v| v[i]).collect();
                        frame.process_frame(&x, &mut row, mode).unwrap();
                        for ch in 0..channels {
                            expected[ch][i] = row[ch];
                        }
                    }
                    let mut actual = vec![vec![89.; 130]; channels];
                    let mut changed = input.clone();
                    let mut offset = 0;
                    for n in [1, 0, 79, 2, 31, 14] {
                        let x: Vec<&[$t]> = input.iter().map(|v| &v[offset..offset + n]).collect();
                        let mut y: Vec<&mut [$t]> = actual
                            .iter_mut()
                            .map(|v| &mut v[offset..offset + n])
                            .collect();
                        planar.process(&x, &mut y, n, mode).unwrap();
                        let mut z: Vec<&mut [$t]> = changed
                            .iter_mut()
                            .map(|v| &mut v[offset..offset + n])
                            .collect();
                        inplace.process_in_place(&mut z, n, mode).unwrap();
                        offset += n;
                    }
                    for ch in 0..channels {
                        assert_eq!(&actual[ch][..127], &expected[ch]);
                        assert_eq!(&actual[ch][127..], &[89.; 3]);
                        assert_eq!(changed[ch], expected[ch]);
                        assert_eq!(frame.capacitor_state(ch), planar.capacitor_state(ch));
                    }
                    let mut old = planar.clone();
                    let prior = actual.clone();
                    let x: Vec<&[$t]> = input.iter().map(|v| v.as_slice()).collect();
                    let mut y: Vec<&mut [$t]> =
                        actual.iter_mut().map(|v| v.as_mut_slice()).collect();
                    y[channels - 1] = &mut [];
                    assert_eq!(planar.process(&x, &mut y, 1, mode), Err(E::BufferLength));
                    assert_eq!(actual, prior);
                    let mut y: Vec<&mut [$t]> =
                        actual.iter_mut().map(|v| v.as_mut_slice()).collect();
                    assert_eq!(planar.process(&x, &mut y, 80, mode), Err(E::TooManyFrames));
                    assert_eq!(planar.process_channel(channels, 0.), Err(E::ChannelCount));
                    let a = planar.process_channel(0, 0.125).unwrap();
                    let b = old.process_channel(0, 0.125).unwrap();
                    assert_eq!(a, b);
                }
            }
            let mut b = <$b>::new(d, 3, 8).unwrap();
            let mut y = [[0.; 8]; 3];
            let measured = allocation_counter::measure(|| {
                for _ in 0..64 {
                    let [a, c, e] = &mut y;
                    b.process(
                        &[&[0.1; 8], &[-0.2; 8], &[0.3; 8]],
                        &mut [a, c, e],
                        8,
                        M::LowPass24,
                    )
                    .unwrap();
                    b.set_bypassed(true);
                    b.process_frame(&[0.2; 3], &mut [0.; 3], M::HighPass24)
                        .unwrap();
                    b.set_bypassed(false);
                    b.reset();
                }
            });
            assert_eq!(measured.count_total, 0);
            assert_eq!(measured.count_current, 0);
            for ch in [0, 65, usize::MAX] {
                assert!(matches!(<$b>::new(d, ch, 8), Err(E::ChannelCount)));
            }
            for max in [0, 65537, usize::MAX] {
                assert!(matches!(<$b>::new(d, 3, max), Err(E::MaximumFrames)));
            }
        }
    };
}
partitions!(
    native64_all_channels_modes_partitions_atomic_late_faults_heap,
    f64,
    B,
    |d: D| d
);
partitions!(
    native32_all_channels_modes_partitions_atomic_late_faults_heap,
    f32,
    B32,
    |d: D| d.quantize_f32().unwrap()
);
macro_rules! state_policy {
    ($name:ident,$t:ty,$f:ty,$convert:expr) => {
        #[test]
        fn $name() {
            let d = $convert(D::new(48000., 1000., 0.9, 4.).unwrap());
            let d2 = $convert(D::new(48000., 7000., 0., 2.).unwrap());
            let mut f = <$f>::new(d);
            for i in 0..100 {
                f.process_sample((i as f64 * 0.23).sin() as $t);
            }
            let mut old = f.clone();
            let states = f.capacitor_state();
            let last = f.last_solve();
            f.set_bypassed(true);
            for mode in MODES {
                assert_eq!(f.process_sample(0.3).select(mode), 0.3);
            }
            assert_eq!(f.capacitor_state(), states);
            assert_eq!(f.last_solve(), last);
            f.set_bypassed(false);
            assert_eq!(f.process_sample(-0.1), old.process_sample(-0.1));
            let states = f.capacitor_state();
            f.set_design(d2);
            assert_eq!(f.capacitor_state(), states);
            f.replace_design(d);
            assert_eq!(f.capacitor_state(), [0.; 4]);
            assert_eq!(f.process_sample(0.7), <$f>::new(d).process_sample(0.7));
            let mut pos = <$f>::new(d);
            let mut neg = <$f>::new(d);
            for i in 0..1000 {
                let x = ((i as f64 * 0.071).sin() * 2.) as $t;
                let a = pos.process_sample(x);
                let b = neg.process_sample(-x);
                for mode in MODES {
                    assert_eq!(a.select(mode), -b.select(mode));
                }
            }
            for input in [<$t>::MAX, -<$t>::MAX, <$t>::INFINITY, <$t>::NEG_INFINITY] {
                let mut f = <$f>::new(d);
                for _ in 0..128 {
                    let y = f.process_sample(input);
                    for mode in MODES {
                        assert!(y.select(mode).is_finite());
                    }
                    assert!(f.last_solve().saturated_input.abs() <= 1.);
                }
            }
            f.process_sample(<$t>::NAN);
            assert!(f.process_sample(0.).low_pass24.is_nan());
            f.set_design(d2);
            assert!(f.process_sample(0.1).low_pass24.is_nan());
            assert_eq!(
                f.last_solve().saturated_input,
                (d2.input_drive() * 0.1).tanh()
            );
            assert_eq!(f.last_solve().scalar_residual, 0.);
            assert_eq!(f.last_solve().staged_residual, 0.);
            f.set_design(d);
            assert!(f.process_sample(0.1).low_pass24.is_nan());
            assert!(f.last_solve().saturated_input.is_nan());
            f.snap_to_zero();
            assert!(f.capacitor_state().iter().any(|v| v.is_nan()));
            f.reset();
            assert!(f.process_sample(0.1).low_pass24.is_finite());
            let mut f = <$f>::new(d2);
            let sub = <$t>::from_bits(1);
            f.process_sample(sub);
            let states = f.capacitor_state();
            assert!(states.iter().all(|v| v.is_finite()));
            f.snap_to_zero();
            assert!(f.capacitor_state().iter().all(|v| !v.is_subnormal()));
            let mut keep = <$f>::new(d);
            keep.process_sample(0.3);
            let state = keep.capacitor_state();
            keep.snap_to_zero();
            assert_eq!(keep.capacitor_state(), state);
            let mut output = [91.; 7];
            let state = keep.capacitor_state();
            assert_eq!(
                keep.process(&[0.; 6], &mut output, M::LowPass12),
                Err(E::BufferLength)
            );
            assert_eq!(output, [91.; 7]);
            assert_eq!(keep.capacitor_state(), state);
        }
    };
}
state_policy!(
    native64_retune_bypass_oddness_wide_input_infinity_nan_and_snap,
    f64,
    F,
    |d: D| d
);
state_policy!(
    native32_retune_bypass_oddness_wide_input_infinity_nan_and_snap,
    f32,
    F32,
    |d: D| d.quantize_f32().unwrap()
);
#[test]
fn exp_decimal_450_digit_roots_normal_subnormal_saturation_and_native_bounds() {
    let mut max64 = 0_f64;
    let mut max32 = 0_f64;
    let mut count = 0;
    for row in include_str!("ladder-decimal.csv").lines() {
        let row: Vec<f64> = row.split(',').map(|v| v.parse().unwrap()).collect();
        let d = D::new(48000., 48000. * row[1], row[2], row[3]).unwrap();
        if row[0] == 64. {
            let mut f = F::new(d);
            f.process_sample(row[4]);
            let actual = f.last_solve().saturated_input;
            let bound = 4. * f64::EPSILON * row[5].abs() + 2. * f64::from_bits(1);
            close(actual, row[5], bound);
            if row[5].is_normal() {
                max64 = max64.max((actual - row[5]).abs() / row[5].abs());
            }
        } else {
            let mut f = F32::new(d.quantize_f32().unwrap());
            f.process_sample(row[4] as f32);
            let actual = f.last_solve().saturated_input as f64;
            let bound = 4. * f32::EPSILON as f64 * row[5].abs() + 2. * f32::from_bits(1) as f64;
            close(actual, row[5], bound);
            if (row[5] as f32).is_normal() {
                max32 = max32.max((actual - row[5]).abs() / row[5].abs());
            }
        }
        count += 1;
    }
    println!("DECIMAL roots {count}; max relative native64={max64:e}, native32={max32:e}");
    assert_eq!(count, 816);
}
fn mul(a: &[f64], b: &[f64]) -> Vec<f64> {
    let mut c = vec![0.; a.len() + b.len() - 1];
    for (i, &x) in a.iter().enumerate() {
        for (j, &y) in b.iter().enumerate() {
            c[i + j] += x * y;
        }
    }
    c
}
fn pow(a: &[f64], n: usize) -> Vec<f64> {
    let mut p = vec![1.];
    for _ in 0..n {
        p = mul(&p, a);
    }
    p
}
fn linear_polynomials(g: f64, k: f64, d: f64) -> (Vec<f64>, [Vec<f64>; 6]) {
    let q = [1., -(1. - 2. * g)];
    let lp = [g, g];
    let hp = [1. - g, -(1. - g)];
    let mut denominator = pow(&q, 4);
    for (a, b) in denominator.iter_mut().zip(pow(&lp, 4)) {
        *a += k * b;
    }
    let mut numerators = [
        mul(&pow(&lp, 2), &pow(&q, 2)),
        pow(&lp, 4),
        mul(&pow(&hp, 2), &pow(&q, 2)),
        pow(&hp, 4),
        mul(&mul(&lp, &hp), &pow(&q, 2)),
        mul(&pow(&lp, 2), &pow(&hp, 2)),
    ];
    for n in &mut numerators {
        for v in n {
            *v *= d;
        }
    }
    (denominator, numerators)
}
macro_rules! polynomial {
    ($name:ident,$t:ty,$f:ty,$convert:expr,$tol:expr) => {
        #[test]
        fn $name() {
            for ratio in [0.07, 0.13, 0.4] {
                for r in [0., 0.5, 0.9] {
                    for drive in [1., 8.] {
                        let d = $convert(D::new(48000., ratio * 48000., r, drive).unwrap());
                        let mut f = <$f>::new(d);
                        let (den, num) = linear_polynomials(
                            d.gain() as f64,
                            d.feedback() as f64,
                            d.input_drive() as f64,
                        );
                        let mut xs = [0.; 5];
                        let mut ys = [[0.; 5]; 6];
                        for i in 0..4096 {
                            xs.rotate_right(1);
                            xs[0] = (((i as f64 * 0.317).sin()
                                + ((i * 13 % 23) as f64 - 11.) / 19.)
                                * 1e-6) as $t as f64;
                            let actual = f.process_sample(xs[0] as $t);
                            for j in 0..6 {
                                ys[j].rotate_right(1);
                                let y = (num[j].iter().zip(xs).map(|(a, b)| a * b).sum::<f64>()
                                    - den[1..]
                                        .iter()
                                        .zip(&ys[j][1..])
                                        .map(|(a, b)| a * b)
                                        .sum::<f64>())
                                    / den[0];
                                ys[j][0] = y;
                                close(actual.select(MODES[j]) as f64, y, $tol);
                            }
                        }
                    }
                }
            }
        }
    };
}
polynomial!(
    native64_independent_fourth_order_modal_polynomial_small_signal,
    f64,
    F,
    |d: D| d,
    3e-13
);
polynomial!(
    native32_independent_fourth_order_modal_polynomial_small_signal,
    f32,
    F32,
    |d: D| d.quantize_f32().unwrap(),
    7e-11
);
#[derive(Clone, Copy, Debug)]
struct Z {
    re: f64,
    im: f64,
}
impl Z {
    fn new(re: f64, im: f64) -> Self {
        Self { re, im }
    }
    fn add(self, b: Self) -> Self {
        Self::new(self.re + b.re, self.im + b.im)
    }
    fn mul(self, b: Self) -> Self {
        Self::new(
            self.re * b.re - self.im * b.im,
            self.re * b.im + self.im * b.re,
        )
    }
    fn scale(self, v: f64) -> Self {
        Self::new(self.re * v, self.im * v)
    }
    fn div(self, b: Self) -> Self {
        let m = b.re * b.re + b.im * b.im;
        Self::new(
            (self.re * b.re + self.im * b.im) / m,
            (self.im * b.re - self.re * b.im) / m,
        )
    }
    fn pow(self, n: usize) -> Self {
        let mut p = Self::new(1., 0.);
        for _ in 0..n {
            p = p.mul(self);
        }
        p
    }
    fn norm(self) -> f64 {
        self.re.hypot(self.im)
    }
}
fn response(g: f64, k: f64, d: f64, f: f64) -> [Z; 6] {
    // Independent bilinear analog prototype substitution, no response helper.
    let l = if f == 0. {
        Z::new(1., 0.)
    } else if f == 0.5 {
        Z::new(0., 0.)
    } else {
        Z::new(1., 0.).div(Z::new(
            1.,
            (std::f64::consts::PI * f).tan() / (g / (1. - g)),
        ))
    };
    let h = Z::new(1. - l.re, -l.im);
    let denominator = Z::new(1., 0.).add(l.pow(4).scale(k));
    [
        l.pow(2),
        l.pow(4),
        h.pow(2),
        h.pow(4),
        l.mul(h),
        l.pow(2).mul(h.pow(2)),
    ]
    .map(|z| z.scale(d).div(denominator))
}
macro_rules! spectral {
 ($name:ident,$t:ty,$f:ty,$convert:expr,$tol:expr)=>{
        #[test]
        fn $name() {
            let mut max = 0_f64;
            for ratio in [0.03, 0.13, 0.4] {
                for r in [0., 0.5, 0.9] {
                    for drive in [1., 8.] {
                        let d = $convert(D::new(48000., ratio * 48000., r, drive).unwrap());
                        let mut filter = <$f>::new(d);
                        let amplitude = (1e-6 / drive) as $t;
                        let frequencies = [0., 0.017, 0.03, 0.07, 0.13, 0.25, 0.49, 0.5];
                        let mut observed = [[Z::new(0., 0.); 6]; 8];
                        for i in 0..32768 {
                            let actual = filter.process_sample(if i == 0 { amplitude } else { 0. });
                            for (n, &frequency) in frequencies.iter().enumerate() {
                                let phase = -std::f64::consts::TAU * frequency * i as f64;
                                let z = Z::new(phase.cos(), phase.sin());
                                for (j, &mode) in MODES.iter().enumerate() {
                                    observed[n][j] = observed[n][j]
                                        .add(z.scale(actual.select(mode) as f64 / amplitude as f64));
                                }
                            }
                        }
                        for (n, &frequency) in frequencies.iter().enumerate() {
                            let expected = response(
                                d.gain() as f64,
                                d.feedback() as f64,
                                d.input_drive() as f64,
                                frequency,
                            );
                            for (j, &h) in expected.iter().enumerate() {
                                let error =
                                    Z::new(observed[n][j].re - h.re, observed[n][j].im - h.im).norm();
                                max = max.max(error / (1. + h.norm()));
                                assert!(
                                    error <= $tol * (1. + h.norm()),
                                    "ratio={ratio} r={r} drive={drive} f={frequency} mode={:?}: {error} vs {}",
                                    MODES[j],
                                    $tol * (1. + h.norm())
                                );
                            }
                        }
                    }
                }
            }
            println!("static small-signal spectral selected-profile maximum normalized error={max:e}");
        }
    };
}
spectral!(
    native64_all_six_modal_small_signal_bilinear_spectra,
    f64,
    F,
    |d: D| d,
    2e-9
);
spectral!(
    native32_all_six_modal_small_signal_bilinear_spectra,
    f32,
    F32,
    |d: D| d.quantize_f32().unwrap(),
    2e-4
);
macro_rules! dc {
    ($name:ident,$t:ty,$f:ty,$convert:expr,$tol:expr) => {
        #[test]
        fn $name() {
            for ratio in [0.01, 0.13, 0.44] {
                for r in [0., 0.5, 1.] {
                    for drive in [1., 8., 32.] {
                        for sign in [-1., 1.] {
                            let d = $convert(D::new(48000., ratio * 48000., r, drive).unwrap());
                            let x = (sign * 3.) as $t;
                            let mut u = 0_f64;
                            for _ in 0..60 {
                                let t = (d.input_drive() as f64 * x as f64
                                    - d.feedback() as f64 * u)
                                    .tanh();
                                u -= (u - t) / (1. + d.feedback() as f64 * (1. - t * t));
                            }
                            let mut f = <$f>::new(d);
                            let mut last = f.process_sample(x);
                            for _ in 0..16384 {
                                last = f.process_sample(x);
                            }
                            close(last.low_pass12 as f64, u, $tol);
                            close(last.low_pass24 as f64, u, $tol);
                            for mode in [M::HighPass12, M::HighPass24, M::BandPass12, M::BandPass24]
                            {
                                close(last.select(mode) as f64, 0., 8. * $tol);
                            }
                        }
                    }
                }
            }
        }
    };
}
dc!(
    native64_nonlinear_dc_equilibrium_full_resonance_drive_and_modal_nulls,
    f64,
    F,
    |d: D| d,
    3e-12
);
dc!(
    native32_nonlinear_dc_equilibrium_full_resonance_drive_and_modal_nulls,
    f32,
    F32,
    |d: D| d.quantize_f32().unwrap(),
    3e-5
);
macro_rules! graph {
    ($name:ident,$t:ty,$p:ty,$design:ty,$convert:expr,$tol:expr) => {
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
                profiles: Vec<$design>,
                ladder: $p,
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
                            self.ladder.set_design(
                                self.profiles[(self.position / 29) % self.profiles.len()],
                            );
                        }
                        let input = [
                            b.audio.input(0).unwrap()[i],
                            b.audio.input(1).unwrap()[i],
                            b.audio.input(2).unwrap()[i],
                        ];
                        let mut y = [0.; 3];
                        self.ladder
                            .process_frame(&input, &mut y, MODES[(self.position / 29) % 6])
                            .unwrap();
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
                    self.ladder.set_design(self.profiles[0]);
                    self.ladder.reset();
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
            let profiles: Vec<$design> = [
                (700., 0., 1.),
                (1200., 0.4, 2.),
                (2700., 0.75, 4.),
                (7000., 0.9, 8.),
                (15000., 0.99, 16.),
                (21000., 1., 32.),
            ]
            .into_iter()
            .map(|(fc, r, drive)| $convert(D::new(48000., fc, r, drive).unwrap()))
            .collect();
            let input: [Vec<$t>; 3] = std::array::from_fn(|ch| {
                (0..1501)
                    .map(|i| {
                        ((i as f64 * (0.173 + ch as f64 * 0.037)).sin() * (0.7 - ch as f64 * 0.13)
                            + ((i * 13 % 23) as f64 - 11.) / 97.) as $t
                    })
                    .collect()
            });
            let impulses: [&[f64]; 3] = [&[0.5, -0.25, 0.125], &[0.1, 0.7], &[-0.13, 0.3, 0.9]];
            let mut states = [[0.; 4]; 3];
            let mut selected: [Vec<f64>; 3] = [vec![], vec![], vec![]];
            for i in 0..1501 {
                let d = profiles[(i / 29) % profiles.len()];
                for ch in 0..3 {
                    let values = nodal(
                        &mut states[ch],
                        input[ch][i] as f64,
                        d.gain() as f64,
                        d.feedback() as f64,
                        d.input_drive() as f64,
                        true,
                    );
                    selected[ch].push(values[(i / 29) % 6] as $t as f64);
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
                    ladder: <$p>::new(profiles[0], 3, 67).unwrap(),
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
    actual_native64_six_mode_nonlinear_graph_fir_retune_consumer,
    f64,
    B,
    D,
    |d: D| d,
    1e-10
);
graph!(
    actual_native32_six_mode_nonlinear_graph_explicit_f64_fir_bridge,
    f32,
    B32,
    ravet_signal::LadderDesign32,
    |d: D| d.quantize_f32().unwrap(),
    4e-6
);
fn ode(y: [f64; 4], x: f64, omega: f64, k: f64, d: f64) -> [f64; 4] {
    let u = (d * x - k * y[3]).tanh();
    [
        omega * (u - y[0]),
        omega * (y[0] - y[1]),
        omega * (y[1] - y[2]),
        omega * (y[2] - y[3]),
    ]
}
fn rk_interval(
    y: &mut [f64; 4],
    before: f64,
    after: f64,
    omega: f64,
    k: f64,
    d: f64,
    steps: usize,
) {
    let h = 1. / steps as f64;
    for step in 0..steps {
        let x0 = before + (after - before) * (step as f64 * h);
        let xm = before + (after - before) * ((step as f64 + 0.5) * h);
        let x1 = before + (after - before) * ((step as f64 + 1.) * h);
        let a = ode(*y, x0, omega, k, d);
        let b = ode(
            std::array::from_fn(|i| y[i] + h * a[i] / 2.),
            xm,
            omega,
            k,
            d,
        );
        let c = ode(
            std::array::from_fn(|i| y[i] + h * b[i] / 2.),
            xm,
            omega,
            k,
            d,
        );
        let e = ode(std::array::from_fn(|i| y[i] + h * c[i]), x1, omega, k, d);
        for i in 0..4 {
            y[i] += h * (a[i] + 2. * b[i] + 2. * c[i] + e[i]) / 6.;
        }
    }
}
fn modal_taps(y: [f64; 4], x: f64, k: f64, d: f64) -> [f64; 6] {
    let u = (d * x - k * y[3]).tanh();
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
macro_rules! circuit {
 ($name:ident,$t:ty,$f:ty,$convert:expr)=>{
        #[test]
        fn $name() {
            for r in [0., 0.5, 0.99] {
                for drive in [1., 8.] {
                    let mut errors = [0_f64; 3];
                    let mut oracle_error = 0_f64;
                    for (rate_index, rate) in [48000., 96000., 192000.].into_iter().enumerate() {
                        let d = $convert(D::new(rate, 700., r, drive).unwrap());
                        let g = d.gain() as f64 / (1. - d.gain() as f64);
                        let mut f = <$f>::new(d);
                        let mut y16 = [0.; 4];
                        let mut y32 = [0.; 4];
                        let mut prior = 0.;
                        for i in 0..(rate * 0.03) as usize {
                            let time = (i + 1) as f64 / rate;
                            let x = (0.3 * (std::f64::consts::TAU * 337. * time).sin()
                                + 0.07 * (std::f64::consts::TAU * 71. * time).sin())
                                as $t;
                            let actual = f.process_sample(x);
                            rk_interval(
                                &mut y16,
                                prior,
                                x as f64,
                                2. * g,
                                d.feedback() as f64,
                                d.input_drive() as f64,
                                16,
                            );
                            rk_interval(
                                &mut y32,
                                prior,
                                x as f64,
                                2. * g,
                                d.feedback() as f64,
                                d.input_drive() as f64,
                                32,
                            );
                            let reference =
                                modal_taps(y32, x as f64, d.feedback() as f64, d.input_drive() as f64);
                            let coarse =
                                modal_taps(y16, x as f64, d.feedback() as f64, d.input_drive() as f64);
                            for j in 0..6 {
                                errors[rate_index] = errors[rate_index]
                                    .max((actual.select(MODES[j]) as f64 - reference[j]).abs());
                                oracle_error = oracle_error.max((coarse[j] - reference[j]).abs());
                            }
                            prior = x as f64;
                        }
                    }
                    println!(
                        "RK nonlinear selected r={r} drive={drive} errors={errors:?} RK16vs32={oracle_error:e} native={}",
                        std::mem::size_of::<$t>()
                    );
                    assert!(oracle_error < 2e-9);
                    assert!(errors[0] < 0.02);
                    assert!(errors[1] < errors[0] * 0.4 + 2e-6);
                    assert!(errors[2] < errors[1] * 0.4 + 2e-6);
                }
            }
        }
    };
}
circuit!(
    native64_independent_rk_nonlinear_continuous_model_second_order_observation,
    f64,
    F,
    |d: D| d
);
circuit!(
    native32_independent_rk_nonlinear_continuous_model_rounding_observation,
    f32,
    F32,
    |d: D| d.quantize_f32().unwrap()
);
fn harmonic(n: usize, drive: f64, points: usize) -> f64 {
    (0..points)
        .map(|i| {
            let t = std::f64::consts::TAU * i as f64 / points as f64;
            (drive * t.sin()).tanh() * (n as f64 * t).sin()
        })
        .sum::<f64>()
        * 2.
        / points as f64
}
macro_rules! aliases {
 ($name:ident,$t:ty,$f:ty,$convert:expr,$tol:expr)=>{
        #[test]
        fn $name() {
            // Coherent periodic source: f0=7/64, fifth harmonic35/64 aliases to29/64.
            // Independent4096/8192-point saturation quadrature; ideal Fourier lowpass
            // antialias projection retains only1st/3rd harmonics before downsampling.
            let drive = 8.;
            let fifth = harmonic(5, drive, 8192);
            assert!(fifth.abs() > 0.05);
            for n in [1, 3, 5, 7] {
                close(harmonic(n, drive, 4096), harmonic(n, drive, 8192), 2e-14);
            }
            for ratio in [0.07, 0.25] {
                let d = $convert(D::new(48000., 48000. * ratio, 0., drive).unwrap());
                let mut filter = <$f>::new(d);
                let mut actual = [[0.; 64]; 6];
                for i in 0..4160 {
                    let x = (std::f64::consts::TAU * 7. * (i % 64) as f64 / 64.).sin() as $t;
                    let y = filter.process_sample(x);
                    if i >= 4096 {
                        for j in 0..6 {
                            actual[j][i - 4096] = y.select(MODES[j]) as f64;
                        }
                    }
                }
                let mut source_alias = Z::new(0., 0.);
                let mut aa_alias = Z::new(0., 0.);
                for i in 0..64 {
                    let phase = std::f64::consts::TAU * 7. * i as f64 / 64.;
                    let z = Z::new(
                        (-std::f64::consts::TAU * 29. * i as f64 / 64.).cos(),
                        (-std::f64::consts::TAU * 29. * i as f64 / 64.).sin(),
                    );
                    let source =
                        ((drive as $t) * (phase.sin() as $t)).tanh() as f64;
                    source_alias = source_alias.add(z.scale(source * 2. / 64.));
                    let aa = harmonic(1, drive, 8192) * phase.sin()
                        + harmonic(3, drive, 8192) * (3. * phase).sin();
                    aa_alias = aa_alias.add(z.scale(aa * 2. / 64.));
                }
                assert!(source_alias.norm() > 0.05);
                assert!(aa_alias.norm() < 2e-14);
                let expected = response(d.gain() as f64, 0., 1., 29. / 64.);
                for j in 0..6 {
                    let observed = (0..64).fold(Z::new(0., 0.), |sum, i| {
                        let phase = -std::f64::consts::TAU * 29. * i as f64 / 64.;
                        sum.add(Z::new(phase.cos(), phase.sin()).scale(actual[j][i] * 2. / 64.))
                    });
                    let wanted = source_alias.mul(expected[j]);
                    close(observed.re, wanted.re, $tol);
                    close(observed.im, wanted.im, $tol);
                    println!(
                        "alias native{} ratio={ratio} mode={:?} fifth-fold magnitude={:e}, ideal antialias residual={:e}",
                        std::mem::size_of::<$t>() * 8,
                        MODES[j],
                        observed.norm(),
                        aa_alias.mul(expected[j]).norm()
                    );
                }
            }
        }
    };
}
aliases!(
    native64_all_modal_actual_fifth_alias_and_converged_antialias_projection,
    f64,
    F,
    |d: D| d,
    3e-13
);
aliases!(
    native32_all_modal_actual_fifth_alias_and_converged_antialias_projection,
    f32,
    F32,
    |d: D| d.quantize_f32().unwrap(),
    3e-6
);
macro_rules! tails {
 ($name:ident,$t:ty,$f:ty,$convert:expr)=>{
        #[test]
        fn $name() {
            for ratio in [0.07, 0.13, 0.44] {
                let mut damped = <$f>::new($convert(
                    D::new(48000., 48000. * ratio, 0.9, 1.).unwrap(),
                ));
                let mut marginal = <$f>::new($convert(
                    D::new(48000., 48000. * ratio, 1., 1.).unwrap(),
                ));
                let mut peak_damped = 0_f64;
                let mut peak_marginal = 0_f64;
                for i in 0..32768 {
                    let x = if i == 0 { 0.001 } else { 0. };
                    let a = damped.process_sample(x);
                    let b = marginal.process_sample(x);
                    assert!(
                        MODES
                            .iter()
                            .all(|&m| a.select(m).is_finite() && b.select(m).is_finite())
                    );
                    if i >= 32000 {
                        peak_damped = peak_damped.max(a.low_pass24.abs() as f64);
                        peak_marginal = peak_marginal.max(b.low_pass24.abs() as f64);
                    }
                }
                println!(
                    "tail native{} ratio={ratio} k3.6 finite-window peak={peak_damped:e}, k4 peak={peak_marginal:e}",
                    std::mem::size_of::<$t>() * 8
                );
                assert!(peak_damped < 1e-20);
                assert!(peak_marginal > 1e-6);
                assert!(peak_marginal < 0.01);
            }
        }
    };
}
tails!(
    native64_static_k_less_than4_decay_vs_marginal_selected_tail,
    f64,
    F,
    |d: D| d
);
tails!(
    native32_static_k_less_than4_decay_vs_marginal_selected_tail,
    f32,
    F32,
    |d: D| d.quantize_f32().unwrap()
);
