use ravet_native_dsp::{
    DenormalMode as Mode, ScopedDenormalMode as Scope, current_float_state as state,
};
use std::{
    hint::black_box,
    sync::{Arc, Barrier},
};
#[inline(never)]
fn mul32(a: f32, b: f32) -> f32 {
    black_box(a) * black_box(b)
}
#[inline(never)]
fn mul64(a: f64, b: f64) -> f64 {
    black_box(a) * black_box(b)
}
#[test]
fn actual_signed_operands_results_normals_and_special_classes() {
    let before = state().unwrap();
    for mode in [Mode::Preserve, Mode::FlushBoth] {
        let scope = Scope::enter(mode).unwrap();
        let during = state().unwrap();
        for sign in [1., -1.] {
            let a = mul32(f32::from_bits(1 | if sign < 0. { 1 << 31 } else { 0 }), 2.);
            let b = mul64(f64::from_bits(1 | if sign < 0. { 1 << 63 } else { 0 }), 2.);
            let c = mul32(
                f32::from_bits(f32::MIN_POSITIVE.to_bits() | if sign < 0. { 1 << 31 } else { 0 }),
                0.25,
            );
            let d = mul64(
                f64::from_bits(f64::MIN_POSITIVE.to_bits() | if sign < 0. { 1 << 63 } else { 0 }),
                0.25,
            );
            let expected_input = during.input_flush();
            let expected_result = during.result_flush();
            assert_eq!(
                a.to_bits(),
                if expected_input {
                    if sign < 0. { 1 << 31 } else { 0 }
                } else {
                    (sign as f32 * f32::from_bits(2)).to_bits()
                }
            );
            assert_eq!(
                b.to_bits(),
                if expected_input {
                    if sign < 0. { 1 << 63 } else { 0 }
                } else {
                    (sign * f64::from_bits(2)).to_bits()
                }
            );
            assert_eq!(
                c.to_bits(),
                if expected_result {
                    if sign < 0. { 1 << 31 } else { 0 }
                } else {
                    (sign as f32 * f32::from_bits(1 << 21)).to_bits()
                }
            );
            assert_eq!(
                d.to_bits(),
                if expected_result {
                    if sign < 0. { 1 << 63 } else { 0 }
                } else {
                    (sign * f64::from_bits(1 << 50)).to_bits()
                }
            );
        }
        assert_eq!(mul32(1.25, -2.), -2.5);
        assert_eq!(mul64(1.25, -2.), -2.5);
        assert!(mul32(f32::NAN, 2.).is_nan());
        assert!(mul64(f64::NAN, 2.).is_nan());
        assert_eq!(mul32(f32::INFINITY, -2.), f32::NEG_INFINITY);
        assert_eq!(mul64(f64::INFINITY, -2.), f64::NEG_INFINITY);
        println!(
            "actual {:?} mode={mode:?} prior={before:?} during={during:?}; signed f32/f64 input/result flush and special classes",
            during.backend
        );
        drop(scope);
        assert_eq!(state().unwrap(), before);
    }
}
#[test]
fn exact_nested_early_return_and_unwind_restoration_without_heap() {
    let before = state().unwrap();
    let info = allocation_counter::measure(|| {
        let outer = Scope::enter(Mode::FlushBoth).unwrap();
        let during = state().unwrap();
        {
            let inner = Scope::enter(Mode::Preserve).unwrap();
            assert_eq!(inner.prior_state(), during);
        }
        assert_eq!(state().unwrap(), during);
        drop(outer);
        assert_eq!(state().unwrap(), before);
    });
    assert_eq!(info.count_total, 0);
    assert_eq!(info.count_current, 0);
    fn early() {
        let _guard = Scope::enter(Mode::FlushBoth).unwrap();
    }
    early();
    assert_eq!(state().unwrap(), before);
    assert!(
        std::panic::catch_unwind(|| {
            let _guard = Scope::enter(Mode::FlushBoth).unwrap();
            let _ = mul64(f64::MIN_POSITIVE, 0.3);
            panic!("controlled unwind restoration");
        })
        .is_err()
    );
    assert_eq!(state().unwrap(), before);
}
#[test]
fn concurrent_scopes_are_local_to_the_creating_threads() {
    let before = state().unwrap();
    let barrier = Arc::new(Barrier::new(3));
    let workers: Vec<_> = (0..2)
        .map(|_| {
            let barrier = barrier.clone();
            std::thread::spawn(move || {
                let prior = state().unwrap();
                let guard = Scope::enter(Mode::FlushBoth).unwrap();
                barrier.wait();
                let observed = state().unwrap();
                assert!(observed.input_flush() && observed.result_flush());
                barrier.wait();
                drop(guard);
                assert_eq!(state().unwrap(), prior);
            })
        })
        .collect();
    barrier.wait();
    assert_eq!(state().unwrap(), before);
    barrier.wait();
    for worker in workers {
        worker.join().unwrap();
    }
    assert_eq!(state().unwrap(), before);
}
