use ravet_signal::{LadderDesign, LadderFilter, LadderFilter32};
macro_rules! export {
    ($kind:expr,$t:ty,$f:ty,$convert:expr)=>{
        for ratio in [1.01e-5, 0.03, 0.13, 0.449] {
            for resonance in [0.5, 1.] {
                for drive in [1., 32.] {
                    let d = $convert(
                        LadderDesign::new(48000., 48000. * ratio, resonance, drive).unwrap(),
                    );
                    let g = d.gain();
                    let k = d.feedback();
                    let drive = d.input_drive();
                    let lambda = k * (((g * g) * g) * g);
                    for pattern in [0, 1, 2] {
                        let mut f = <$f>::new(d);
                        let warmup = 129;
                        for i in 0..warmup {
                            let x = warm(i, pattern) as $t;
                            f.process_sample(x);
                        }
                        let state = f.capacitor_state().map(|v| (1. - g) * v);
                        let b = ((state[0] * g + state[1]) * g + state[2]) * g + state[3];
                        for target in [0., 0.3, 0.5, 1., 2., -0.3, -1.] {
                            let x = (target as $t + k * b) / drive;
                            for x in [x.next_down(), x, x.next_up()] {
                                let c = drive * x - k * b;
                                println!(
                                    "{},{ratio:.17e},{resonance:.17e},{drive:.17e},{pattern},{warmup},{x:.17e},{c:.17e},{lambda:.17e}",
                                    $kind
                                );
                            }
                        }
                    }
                }
            }
        }
    }
}
fn warm(i: usize, pattern: usize) -> f64 {
    match pattern {
        0 => 0.3,
        1 => {
            if i.is_multiple_of(2) {
                0.4
            } else {
                -0.2
            }
        }
        _ => 0.17 * (i as f64 * 0.041).sin(),
    }
}
fn main() {
    export!(64, f64, LadderFilter, |d: LadderDesign| d);
    export!(32, f32, LadderFilter32, |d: LadderDesign| d
        .quantize_f32()
        .unwrap());
}
