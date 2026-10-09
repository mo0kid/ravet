use crate::abi::*;
use std::ptr;
#[test]
fn native_f64_cross_channel_aliasing_bypass_fault_and_shared_state() {
    unsafe {
        let handle = ravet_gain64_create();
        assert!(!handle.is_null());
        assert_eq!(ravet_gain64_prepare(handle, 48000., 2, 8), 0);
        assert_eq!(ravet_gain64_restore(handle, 2., 0), 0);
        let exact = 1. + f64::EPSILON;
        let mut left = [exact; 9];
        let mut right = [-exact; 9];
        let inputs = [left.as_ptr(), right.as_ptr()];
        let outputs = [right.as_mut_ptr(), left.as_mut_ptr()];
        let event = [NativeEvent {
            offset: 1,
            id: 2,
            value: 1.,
        }];
        let mut gain = -1.;
        let mut bypass = 99;
        assert_eq!(
            ravet_gain64_process(
                handle,
                inputs.as_ptr(),
                outputs.as_ptr(),
                2,
                8,
                event.as_ptr(),
                1,
                &mut gain,
                &mut bypass
            ),
            0
        );
        assert_eq!(right[0], 2. * exact);
        assert_eq!(left[0], -2. * exact);
        assert_eq!(&right[1..8], &[exact; 7]);
        assert_eq!(&left[1..8], &[-exact; 7]);
        assert_eq!((left[8], right[8]), (exact, -exact));
        assert_eq!((gain, bypass), (2., 1));
        let bad = [NativeEvent {
            offset: 8,
            id: 1,
            value: 3.,
        }];
        gain = 99.;
        bypass = 99;
        assert_eq!(
            ravet_gain64_process(
                handle,
                inputs.as_ptr(),
                outputs.as_ptr(),
                2,
                8,
                bad.as_ptr(),
                1,
                &mut gain,
                &mut bypass
            ),
            1
        );
        assert_eq!(&left[..8], &[0.; 8]);
        assert_eq!(&right[..8], &[0.; 8]);
        assert_eq!((gain, bypass), (99., 99));
        assert_eq!(
            ravet_gain64_process(
                handle,
                ptr::null(),
                ptr::null(),
                2,
                0,
                ptr::null(),
                0,
                &mut gain,
                &mut bypass
            ),
            0
        );
        assert_eq!((gain, bypass), (2., 1));
        assert_eq!(ravet_gain64_restore(handle, f32::NAN, 0), 1);
        assert_eq!(ravet_gain64_restore(handle, 1., 2), 1);
        let mut record = [0; 32];
        assert_eq!(ravet_gain_encode(0.5, 0, record.as_mut_ptr(), 32), 0);
        assert_eq!(
            ravet_gain_decode(record.as_ptr(), 32, &mut gain, &mut bypass),
            0
        );
        assert_eq!(ravet_gain64_restore(handle, gain, bypass), 0);
        left.fill(exact);
        right.fill(-exact);
        assert_eq!(
            ravet_gain64_process(
                handle,
                inputs.as_ptr(),
                outputs.as_ptr(),
                2,
                8,
                ptr::null(),
                0,
                &mut gain,
                &mut bypass
            ),
            0
        );
        assert_eq!(right[0], exact * 0.5);
        assert_eq!(left[0], -exact * 0.5);
        assert_eq!(ravet_gain64_prepare(handle, 48000., 3, 8), 1);
        assert_eq!(
            ravet_gain64_process(
                handle,
                inputs.as_ptr(),
                outputs.as_ptr(),
                2,
                8,
                ptr::null(),
                0,
                &mut gain,
                &mut bypass
            ),
            1
        );
        assert_eq!(&left[..8], &[0.; 8]);
        ravet_gain64_destroy(handle);
    }
}
#[test]
fn native_f64_extrema_and_prepared_callbacks_keep_precision_without_heap_operations() {
    unsafe {
        let handle = ravet_gain64_create();
        assert_eq!(ravet_gain64_prepare(handle, 48000., 1, 8), 0);
        let mut samples = [
            f64::MAX,
            f64::from_bits(1),
            1. + f64::EPSILON,
            0.,
            0.,
            0.,
            0.,
            0.,
        ];
        let inputs = [samples.as_ptr()];
        let outputs = [samples.as_mut_ptr()];
        let mut gain = 1.;
        let mut bypass = 0;
        assert_eq!(
            ravet_gain64_process(
                handle,
                inputs.as_ptr(),
                outputs.as_ptr(),
                1,
                8,
                ptr::null(),
                0,
                &mut gain,
                &mut bypass
            ),
            0
        );
        assert_eq!(samples[0], f64::MAX);
        assert_eq!(samples[1], f64::from_bits(1));
        assert_eq!(samples[2], 1. + f64::EPSILON);
        let counts = allocation_counter::measure(|| {
            for _ in 0..10_000 {
                samples.fill(1. + f64::EPSILON);
                assert_eq!(ravet_gain64_restore(handle, 1., 0), 0);
                assert_eq!(
                    ravet_gain64_process(
                        handle,
                        inputs.as_ptr(),
                        outputs.as_ptr(),
                        1,
                        8,
                        ptr::null(),
                        0,
                        &mut gain,
                        &mut bypass
                    ),
                    0
                );
                assert_eq!(samples[0], 1. + f64::EPSILON);
                assert_eq!(
                    ravet_gain64_process(
                        handle,
                        ptr::null(),
                        ptr::null(),
                        1,
                        0,
                        ptr::null(),
                        0,
                        &mut gain,
                        &mut bypass
                    ),
                    0
                );
                assert_eq!(
                    ravet_gain64_process(
                        handle,
                        inputs.as_ptr(),
                        outputs.as_ptr(),
                        1,
                        8,
                        ptr::null(),
                        1025,
                        &mut gain,
                        &mut bypass
                    ),
                    1
                );
                ravet_gain64_reset(handle);
            }
        });
        assert_eq!(counts.count_total, 0);
        assert_eq!(counts.count_current, 0);
        assert_eq!(counts.bytes_current, 0);
        ravet_gain64_destroy(handle);
    }
}
