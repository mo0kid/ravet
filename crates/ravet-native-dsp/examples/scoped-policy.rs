//! Headless numerical observation. No host/device/editor or automatic default policy.
use ravet_native_dsp::{DenormalMode, ScopedDenormalMode, current_float_state};
#[inline(never)]
fn product(a: f64, b: f64) -> f64 {
    std::hint::black_box(a) * std::hint::black_box(b)
}
fn main() {
    let mode = match std::env::args().nth(1).as_deref() {
        Some("preserve") => DenormalMode::Preserve,
        Some("flush-both") => DenormalMode::FlushBoth,
        _ => {
            eprintln!("usage: scoped-policy preserve|flush-both");
            std::process::exit(2);
        }
    };
    let before = current_float_state().expect("supported OS native register profile");
    let (during, result) = {
        let _guard =
            ScopedDenormalMode::enter(mode).expect("actual environment admits selected mode");
        (
            current_float_state().unwrap(),
            product(f64::from_bits(1), 2.),
        )
    };
    let after = current_float_state().unwrap();
    assert_eq!(before, after);
    println!(
        "before={before:?}\nduring={during:?}\nresult_bits={:016x}\nafter={after:?}",
        result.to_bits()
    );
}
