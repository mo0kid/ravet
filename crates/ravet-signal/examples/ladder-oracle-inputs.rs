use ravet_signal::LadderDesign;
fn main() {
    for ratio in [1.01e-5, 0.03, 0.13, 0.449] {
        for r in [0., 0.5, 1.] {
            for drive in [1., 32.] {
                let d = LadderDesign::new(48000., 48000. * ratio, r, drive).unwrap();
                let q = d.quantize_f32().unwrap();
                for (kind, g, k, drive) in [
                    (64, d.gain(), d.feedback(), d.input_drive()),
                    (
                        32,
                        q.gain() as f64,
                        q.feedback() as f64,
                        q.input_drive() as f64,
                    ),
                ] {
                    let lambda = if kind == 64 {
                        k * (((g * g) * g) * g)
                    } else {
                        ((k as f32) * ((((g as f32) * (g as f32)) * (g as f32)) * (g as f32)))
                            as f64
                    };
                    let exponents: &[f64] = if kind == 64 {
                        &[
                            f64::from_bits(1),
                            f64::MIN_POSITIVE,
                            1e-300,
                            1e-100,
                            1e-15,
                            1e-3,
                            0.1,
                            3.,
                            f64::MAX,
                        ]
                    } else {
                        &[
                            f32::from_bits(1) as f64,
                            f32::MIN_POSITIVE as f64,
                            1e-30,
                            1e-15,
                            1e-3,
                            0.1,
                            3.,
                            f32::MAX as f64,
                        ]
                    };
                    for &x in exponents {
                        for sign in [-1., 1.] {
                            let x = if kind == 64 {
                                sign * x
                            } else {
                                (sign * x) as f32 as f64
                            };
                            let c = if kind == 64 {
                                drive * x
                            } else {
                                ((drive as f32) * (x as f32)) as f64
                            };
                            println!(
                                "{kind},{ratio:.17e},{r:.17e},{drive:.17e},{x:.17e},{c:.17e},{lambda:.17e}"
                            );
                        }
                    }
                }
            }
        }
    }
}
