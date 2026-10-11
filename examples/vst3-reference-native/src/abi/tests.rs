//! Direct tests of every exported C ABI entry point: valid input, null and
//! invalid pointers, out-of-range sizes and counts, and sequencing errors.
//! Processing and reset calls on prepared handles are measured with
//! `allocation-counter` (Rust heap only).
use super::*;
use crate::{digest_step, record_hash};
use allocation_counter::measure;
use std::ptr::{null, null_mut};

const FRAMES: u32 = 64;

fn assert_no_heap<R>(f: impl FnOnce() -> R) -> R {
    let mut result = None;
    let info = measure(|| result = Some(f()));
    assert_eq!(
        (info.count_total, info.count_current),
        (0, 0),
        "heap operation"
    );
    result.unwrap()
}

// ---------------------------------------------------------------- keyed gain

fn keyed_case<S: ReferenceSample + std::fmt::Debug>(bits: u32, sample: fn(f64) -> S) {
    // SAFETY: every pointer below references live local storage that outlives
    // the call, with at least the lengths passed.
    unsafe {
        let frames = FRAMES as usize;
        let main: [Vec<S>; 2] = std::array::from_fn(|c| {
            (0..frames + 1)
                .map(|f| sample(0.25 + c as f64 * 0.5 - f as f64 * 0.01))
                .collect()
        });
        let key: [Vec<S>; 2] = std::array::from_fn(|c| {
            (0..frames + 1)
                .map(|f| sample(0.5 - c as f64 * 0.25 + f as f64 * 0.003))
                .collect()
        });
        let main_ptrs = [main[0].as_ptr(), main[1].as_ptr()];
        let key_ptrs = [key[0].as_ptr(), key[1].as_ptr()];
        let mut out = [vec![sample(9.); frames + 1], vec![sample(9.); frames + 1]];
        let process = |h, m: *const *const S, k: *const *const S, o: *const *mut S, n: u32| {
            if bits == 32 {
                ravet_keyed_gain_process32(h, m.cast(), k.cast(), o.cast(), n)
            } else {
                ravet_keyed_gain_process64(h, m.cast(), k.cast(), o.cast(), n)
            }
        };
        // The other precision's entry point, called with correctly sized
        // 8-byte storage (`u64`) so its silencing stays in bounds for both.
        let other = |h, n: u32, out: &mut [Vec<u64>; 2]| {
            let o = [out[0].as_mut_ptr(), out[1].as_mut_ptr()];
            if bits == 32 {
                ravet_keyed_gain_process64(h, null(), null(), o.as_ptr().cast(), n)
            } else {
                ravet_keyed_gain_process32(h, null(), null(), o.as_ptr().cast(), n)
            }
        };
        let refill = |out: &mut [Vec<S>; 2]| out.iter_mut().for_each(|c| c.fill(sample(9.)));
        let silenced = |out: &[Vec<S>; 2], n: usize| {
            out.iter()
                .all(|c| c[..n].iter().all(|v| *v == S::default()))
        };

        // Null handle and destroy(null).
        ravet_keyed_gain_destroy(null_mut());
        assert_eq!(
            ravet_keyed_gain_prepare(null_mut(), bits, 48000., 0, FRAMES),
            REJECTED
        );
        let o = [out[0].as_mut_ptr(), out[1].as_mut_ptr()];
        assert_eq!(
            process(null_mut(), main_ptrs.as_ptr(), null(), o.as_ptr(), FRAMES),
            REJECTED
        );

        let h = ravet_keyed_gain_create();
        assert!(!h.is_null());
        // Sequencing: process before prepare is rejected and silences outputs.
        let o = [out[0].as_mut_ptr(), out[1].as_mut_ptr()];
        assert_eq!(
            process(h, main_ptrs.as_ptr(), null(), o.as_ptr(), FRAMES),
            REJECTED
        );
        assert!(silenced(&out, frames));
        // Invalid preparation: precision, key channels, frames, rate.
        for (b, rate, keys, max) in [
            (16, 48000., 0, FRAMES),
            (0, 48000., 0, FRAMES),
            (bits, 48000., 3, FRAMES),
            (bits, 48000., 0, 0),
            (bits, 48000., 0, 65537),
            (bits, 0., 0, FRAMES),
            (bits, -48000., 0, FRAMES),
            (bits, f64::NAN, 0, FRAMES),
            (bits, f64::INFINITY, 0, FRAMES),
        ] {
            assert_eq!(
                ravet_keyed_gain_prepare(h, b, rate, keys, max),
                REJECTED,
                "{b} {rate} {keys} {max}"
            );
        }

        for keys in 0..=2u32 {
            assert_eq!(ravet_keyed_gain_prepare(h, bits, 48000., keys, FRAMES), OK);
            let k = if keys > 0 { key_ptrs.as_ptr() } else { null() };
            // Valid block equals the closed form; the tail is untouched.
            for n in [0, 1, 17, FRAMES] {
                refill(&mut out);
                let o = [out[0].as_mut_ptr(), out[1].as_mut_ptr()];
                let status = assert_no_heap(|| process(h, main_ptrs.as_ptr(), k, o.as_ptr(), n));
                assert_eq!(status, OK);
                for f in 0..n as usize {
                    let key = match keys {
                        0 => S::ONE,
                        1 => key[0][f],
                        _ => (key[0][f] + key[1][f]) * S::HALF,
                    };
                    for c in 0..2 {
                        assert_eq!(out[c][f], main[c][f] * key);
                    }
                }
                assert!(
                    out.iter()
                        .all(|c| c[n as usize..].iter().all(|v| *v == sample(9.)))
                );
            }
            // Zero frames admit null pointers everywhere.
            assert_eq!(process(h, null(), null(), [null_mut(); 2].as_ptr(), 0), OK);
            // Prepared for this precision only: the other entry point refuses
            // and silences its (correctly typed) outputs.
            let mut wrong = [vec![u64::MAX; frames], vec![u64::MAX; frames]];
            assert_eq!(other(h, FRAMES, &mut wrong), REJECTED);
            // process64 zeroes `frames` u64-sized samples, process32 half as many words.
            let words = if bits == 32 { frames } else { frames / 2 };
            assert!(wrong.iter().all(|c| c[..words].iter().all(|v| *v == 0)));
            assert!(
                wrong
                    .iter()
                    .all(|c| c[words..].iter().all(|v| *v == u64::MAX))
            );
            // Null pointer arrays and channels are rejected; valid outputs are silenced.
            assert_eq!(process(h, main_ptrs.as_ptr(), k, null(), FRAMES), REJECTED);
            for (m, label) in [
                (null(), "main array"),
                ([main_ptrs[0], null()].as_ptr(), "main channel"),
            ] {
                refill(&mut out);
                let o = [out[0].as_mut_ptr(), out[1].as_mut_ptr()];
                assert_eq!(process(h, m, k, o.as_ptr(), FRAMES), REJECTED, "{label}");
                assert!(silenced(&out, frames), "{label}");
            }
            if keys > 0 {
                refill(&mut out);
                let o = [out[0].as_mut_ptr(), out[1].as_mut_ptr()];
                assert_eq!(
                    process(h, main_ptrs.as_ptr(), null(), o.as_ptr(), FRAMES),
                    REJECTED
                );
                assert!(silenced(&out, frames));
                let short = [key_ptrs[0], null()];
                let none: [*const S; 1] = [null()];
                let k_bad = if keys == 2 {
                    short.as_ptr()
                } else {
                    none.as_ptr()
                };
                refill(&mut out);
                let o = [out[0].as_mut_ptr(), out[1].as_mut_ptr()];
                assert_eq!(
                    process(h, main_ptrs.as_ptr(), k_bad, o.as_ptr(), FRAMES),
                    REJECTED
                );
                assert!(silenced(&out, frames));
            }
            refill(&mut out);
            let o = [out[0].as_mut_ptr(), null_mut()];
            assert_eq!(
                process(h, main_ptrs.as_ptr(), k, o.as_ptr(), FRAMES),
                REJECTED
            );
            assert!(out[0][..frames].iter().all(|v| *v == S::default()));
            // Frames above the prepared maximum.
            refill(&mut out);
            let o = [out[0].as_mut_ptr(), out[1].as_mut_ptr()];
            assert_eq!(
                process(h, main_ptrs.as_ptr(), k, o.as_ptr(), FRAMES + 1),
                REJECTED
            );
            assert!(silenced(&out, frames + 1));
            // Overlapping output channels are rejected, not aliased.
            refill(&mut out);
            let base = out[0].as_mut_ptr();
            for o in [[base, base], [base, base.add(1)]] {
                assert_eq!(process(h, main_ptrs.as_ptr(), k, o.as_ptr(), 8), REJECTED);
            }
            // In-place processing: outputs alias the main inputs.
            let mut inplace = [main[0].clone(), main[1].clone()];
            let io = [inplace[0].as_mut_ptr(), inplace[1].as_mut_ptr()];
            let ii = [io[0].cast_const(), io[1].cast_const()];
            assert_eq!(process(h, ii.as_ptr(), k, io.as_ptr(), FRAMES), OK);
            for f in 0..frames {
                let key = match keys {
                    0 => S::ONE,
                    1 => key[0][f],
                    _ => (key[0][f] + key[1][f]) * S::HALF,
                };
                assert_eq!(inplace[0][f], main[0][f] * key);
                assert_eq!(inplace[1][f], main[1][f] * key);
            }
        }
        // A failed re-prepare leaves the handle unprepared.
        assert_eq!(
            ravet_keyed_gain_prepare(h, bits, 48000., 3, FRAMES),
            REJECTED
        );
        refill(&mut out);
        let o = [out[0].as_mut_ptr(), out[1].as_mut_ptr()];
        assert_eq!(
            process(h, main_ptrs.as_ptr(), key_ptrs.as_ptr(), o.as_ptr(), FRAMES),
            REJECTED
        );
        assert!(silenced(&out, frames));
        ravet_keyed_gain_destroy(h);
    }
}

#[test]
fn keyed_gain_abi_entry_points_f32() {
    keyed_case::<f32>(32, |v| v as f32);
}

#[test]
fn keyed_gain_abi_entry_points_f64() {
    keyed_case::<f64>(64, |v| v);
}

// --------------------------------------------------------------- event probe

fn raw(code: u16, offset: u32) -> RavetEventProbeRecord {
    RavetEventProbeRecord {
        offset,
        code,
        flags: 0x8001,
        bus: 0,
        a: 3,
        b: 64,
        c: 77,
        d: 0x1234,
        f0: -0.5,
        f1: 0.75,
        reserved: 0,
        ppq_position: 1.5,
        value: 0.125,
        integer: u64::MAX - 5,
        payload: null(),
        payload_count: 0,
        reserved2: 0,
    }
}

/// The same records as typed Rust values, built independently of `convert`.
fn typed<'a>(
    records: &[RavetEventProbeRecord],
    bytes: &'a [u8],
    text: &'a [u16],
) -> Vec<ProbeRecord<'a>> {
    records
        .iter()
        .map(|r| {
            let event = match r.code {
                0 => ProbeEvent::NoteOn {
                    channel: r.a,
                    pitch: r.b,
                    note_id: r.c,
                    length: r.d,
                    tuning: r.f0,
                    velocity: r.f1,
                },
                1 => ProbeEvent::NoteOff {
                    channel: r.a,
                    pitch: r.b,
                    note_id: r.c,
                    tuning: r.f0,
                    velocity: r.f1,
                },
                2 => ProbeEvent::Data {
                    data_type: r.a as u32,
                    bytes,
                },
                3 => ProbeEvent::PolyPressure {
                    channel: r.a,
                    pitch: r.b,
                    note_id: r.c,
                    pressure: r.f1,
                },
                4 => ProbeEvent::ExpressionValue {
                    type_id: r.a as u32,
                    note_id: r.c,
                    value: r.value,
                },
                5 => ProbeEvent::ExpressionText {
                    type_id: r.a as u32,
                    note_id: r.c,
                    text,
                },
                6 => ProbeEvent::Chord {
                    root: r.a,
                    bass: r.b,
                    mask: r.d as u16,
                    text,
                },
                7 => ProbeEvent::Scale {
                    root: r.a,
                    mask: r.d as u16,
                    text,
                },
                8 => ProbeEvent::ExpressionInteger {
                    type_id: r.a as u32,
                    note_id: r.c,
                    value: r.integer,
                },
                _ => ProbeEvent::Parameter {
                    id: r.a as u32,
                    value: r.value,
                },
            };
            ProbeRecord {
                header: ProbeHeader {
                    offset: r.offset,
                    bus: r.bus,
                    flags: r.flags,
                    ppq_position: r.ppq_position,
                },
                event,
            }
        })
        .collect()
}

fn probe_case<S: ReferenceSample + std::fmt::Debug>(bits: u32, sample: fn(f64) -> S) {
    let bytes: Vec<u8> = vec![0xf0, 0x7d, 0x01, 0xf7];
    let text: Vec<u16> = "Cmaj7 \u{e9}".encode_utf16().collect();
    // All ten codes at distinct offsets.
    let mut records: Vec<RavetEventProbeRecord> =
        (0..9u16).map(|c| raw(c, u32::from(c) * 5)).collect();
    records.push(raw(PARAMETER_CODE, 50));
    for r in records.iter_mut() {
        match r.code {
            2 => (r.payload, r.payload_count) = (bytes.as_ptr().cast(), bytes.len() as u32),
            5..=7 => (r.payload, r.payload_count) = (text.as_ptr().cast(), text.len() as u32),
            _ => {}
        }
    }
    let typed = typed(&records, &bytes, &text);
    let frames = FRAMES as usize;
    // SAFETY: every pointer below references live local storage that outlives
    // the call, with at least the lengths passed.
    unsafe {
        let process = |h,
                       r: *const RavetEventProbeRecord,
                       n: u32,
                       l: *mut S,
                       rr: *mut S,
                       f: u32,
                       d: *mut u32| {
            if bits == 32 {
                ravet_event_probe_process32(h, r, n, l.cast(), rr.cast(), f, d)
            } else {
                ravet_event_probe_process64(h, r, n, l.cast(), rr.cast(), f, d)
            }
        };
        let mut left = vec![sample(9.); frames + 1];
        let mut right = vec![sample(9.); frames + 1];
        let mut digest = 0xdead_u32;
        let silenced =
            |l: &[S], r: &[S], n: usize| l[..n].iter().chain(&r[..n]).all(|v| *v == S::default());

        // Null handles.
        ravet_event_probe_destroy(null_mut());
        ravet_event_probe_reset(null_mut());
        assert_eq!(ravet_event_probe_prepare(null_mut(), FRAMES), REJECTED);
        assert_eq!(
            process(
                null_mut(),
                records.as_ptr(),
                10,
                left.as_mut_ptr(),
                right.as_mut_ptr(),
                FRAMES,
                &mut digest
            ),
            REJECTED
        );

        let h = ravet_event_probe_create();
        assert!(!h.is_null());
        // Sequencing: process before prepare is rejected, silenced, digest untouched.
        assert_eq!(
            process(
                h,
                records.as_ptr(),
                10,
                left.as_mut_ptr(),
                right.as_mut_ptr(),
                FRAMES,
                &mut digest
            ),
            REJECTED
        );
        assert!(silenced(&left, &right, frames));
        assert_eq!(digest, 0xdead);
        for max in [0, 65537] {
            assert_eq!(ravet_event_probe_prepare(h, max), REJECTED);
        }
        assert_eq!(ravet_event_probe_prepare(h, FRAMES), OK);

        // Valid block: equals the typed reference render and record hashes.
        let mut reference = EventProbe::new();
        reference.prepare(frames).unwrap();
        let mut expected_left = vec![S::default(); frames];
        let mut expected_right = vec![S::default(); frames];
        let expected = reference
            .render(&typed, &mut expected_left, &mut expected_right, frames)
            .unwrap();
        left.fill(sample(9.));
        right.fill(sample(9.));
        let status = assert_no_heap(|| {
            process(
                h,
                records.as_ptr(),
                10,
                left.as_mut_ptr(),
                right.as_mut_ptr(),
                FRAMES,
                &mut digest,
            )
        });
        assert_eq!(status, OK);
        assert_eq!(digest, expected);
        assert_eq!(&left[..frames], &expected_left[..]);
        assert_eq!(&right[..frames], &expected_right[..]);
        assert_eq!((left[frames], right[frames]), (sample(9.), sample(9.)));
        let mut check = 0;
        for (r, t) in records.iter().zip(&typed) {
            let o = r.offset as usize;
            assert_eq!(left[o], S::exact(u32::from(r.code) + 1));
            assert_eq!(right[o], S::exact(record_hash(t)) * S::HASH_SCALE);
            check = digest_step(check, record_hash(t));
        }
        assert_eq!(check, expected);

        // Rejections leave the digest unchanged and silence outputs.
        let mut bad_text = text.clone();
        bad_text.push(0);
        let odd = bad_text
            .as_ptr()
            .cast::<u8>()
            .add(1)
            .cast::<std::ffi::c_void>();
        let mut cases: Vec<(&str, Vec<RavetEventProbeRecord>, u32)> = Vec::new();
        for code in [9u16, 15, 17, u16::MAX] {
            cases.push(("unknown code", vec![raw(code, 0)], FRAMES));
        }
        let mut r = raw(2, 0);
        r.payload_count = 3;
        cases.push(("data null payload", vec![r], FRAMES));
        for code in [5u16, 6, 7] {
            let mut r = raw(code, 0);
            r.payload_count = 2;
            cases.push(("text null payload", vec![r], FRAMES));
            let mut r = raw(code, 0);
            (r.payload, r.payload_count) = (odd, 2);
            cases.push(("text odd alignment", vec![r], FRAMES));
        }
        for (code, d) in [(6u16, -1), (6, 65536), (7, -1), (7, 70000)] {
            let mut r = raw(code, 0);
            r.d = d;
            cases.push(("mask out of range", vec![r], FRAMES));
        }
        cases.push(("offset at frames", vec![raw(0, FRAMES)], FRAMES));
        cases.push(("offset past frames", vec![raw(0, u32::MAX)], FRAMES));
        cases.push(("zero-frame nonzero offset", vec![raw(0, 1)], 0));
        cases.push((
            "one bad record among good",
            {
                let mut v = records.clone();
                v.push(raw(99, 1));
                v
            },
            FRAMES,
        ));
        for (label, case, f) in &cases {
            left.fill(sample(9.));
            right.fill(sample(9.));
            let mut d = 0xbeef_u32;
            let status = process(
                h,
                case.as_ptr(),
                case.len() as u32,
                left.as_mut_ptr(),
                right.as_mut_ptr(),
                *f,
                &mut d,
            );
            assert_eq!(status, REJECTED, "{label}");
            assert_eq!(d, 0xbeef, "{label}");
            assert!(silenced(&left, &right, *f as usize), "{label}");
        }
        // Count, pointer and size refusals.
        let many = vec![raw(0, 0); MAXIMUM_PROBE_RECORDS + 1];
        for (label, recs, n, l, r, f) in [
            (
                "count above 4096",
                many.as_ptr(),
                (MAXIMUM_PROBE_RECORDS + 1) as u32,
                left.as_mut_ptr(),
                right.as_mut_ptr(),
                FRAMES,
            ),
            (
                "null records",
                null(),
                1,
                left.as_mut_ptr(),
                right.as_mut_ptr(),
                FRAMES,
            ),
            (
                "frames above maximum",
                records.as_ptr(),
                10,
                left.as_mut_ptr(),
                right.as_mut_ptr(),
                FRAMES + 1,
            ),
        ] {
            left.fill(sample(9.));
            right.fill(sample(9.));
            let mut d = 0xbeef_u32;
            assert_eq!(process(h, recs, n, l, r, f, &mut d), REJECTED, "{label}");
            assert_eq!(d, 0xbeef, "{label}");
            assert!(silenced(&left, &right, f as usize), "{label}");
        }
        left.fill(sample(9.));
        assert_eq!(
            process(
                h,
                records.as_ptr(),
                10,
                left.as_mut_ptr(),
                null_mut(),
                FRAMES,
                &mut digest
            ),
            REJECTED
        );
        assert!(left[..frames].iter().all(|v| *v == S::default()));
        assert_eq!(
            process(
                h,
                records.as_ptr(),
                10,
                null_mut(),
                null_mut(),
                FRAMES,
                &mut digest
            ),
            REJECTED
        );
        let base = left.as_mut_ptr();
        for other in [base, base.add(1)] {
            assert_eq!(
                process(h, records.as_ptr(), 10, base, other, 8, &mut digest),
                REJECTED
            );
        }
        assert_eq!(
            process(
                h,
                records.as_ptr(),
                10,
                left.as_mut_ptr(),
                right.as_mut_ptr(),
                65537,
                &mut digest
            ),
            REJECTED
        );
        assert_eq!(digest, expected, "rejections never touched the digest");

        // Continuation: the running digest folds from the last success.
        let mut next_left = vec![S::default(); frames];
        let mut next_right = vec![S::default(); frames];
        let continued = reference
            .render(&typed, &mut next_left, &mut next_right, frames)
            .unwrap();
        assert_eq!(
            process(
                h,
                records.as_ptr(),
                10,
                left.as_mut_ptr(),
                right.as_mut_ptr(),
                FRAMES,
                null_mut()
            ),
            OK,
            "null digest pointer is optional"
        );
        // Zero-frame block: null outputs; records at offset zero still fold.
        let zero: Vec<_> = records
            .iter()
            .map(|r| RavetEventProbeRecord { offset: 0, ..*r })
            .collect();
        let zero_typed: Vec<_> = typed
            .iter()
            .map(|r| ProbeRecord {
                header: ProbeHeader {
                    offset: 0,
                    ..r.header
                },
                ..*r
            })
            .collect();
        let folded = reference
            .render::<S>(&zero_typed, &mut [], &mut [], 0)
            .unwrap();
        assert_ne!(folded, continued);
        let status = assert_no_heap(|| {
            process(h, zero.as_ptr(), 10, null_mut(), null_mut(), 0, &mut digest)
        });
        assert_eq!((status, digest), (OK, folded));
        // Empty block: no records, outputs silent, digest unchanged.
        left.fill(sample(9.));
        right.fill(sample(9.));
        assert_eq!(
            process(
                h,
                null(),
                0,
                left.as_mut_ptr(),
                right.as_mut_ptr(),
                FRAMES,
                &mut digest
            ),
            OK
        );
        assert_eq!(digest, folded);
        assert!(silenced(&left, &right, frames));
        // Exactly 4096 records are admitted.
        let full = vec![raw(0, 0); MAXIMUM_PROBE_RECORDS];
        let status = assert_no_heap(|| {
            process(
                h,
                full.as_ptr(),
                full.len() as u32,
                left.as_mut_ptr(),
                right.as_mut_ptr(),
                FRAMES,
                &mut digest,
            )
        });
        assert_eq!(status, OK);
        // Reset clears the digest without allocating; re-prepare clears it too.
        assert_no_heap(|| ravet_event_probe_reset(h));
        assert_eq!(
            process(
                h,
                records.as_ptr(),
                10,
                left.as_mut_ptr(),
                right.as_mut_ptr(),
                FRAMES,
                &mut digest
            ),
            OK
        );
        assert_eq!(digest, expected);
        assert_eq!(ravet_event_probe_prepare(h, FRAMES), OK);
        assert_eq!(
            process(
                h,
                records.as_ptr(),
                10,
                left.as_mut_ptr(),
                right.as_mut_ptr(),
                FRAMES,
                &mut digest
            ),
            OK
        );
        assert_eq!(digest, expected);
        // A failed re-prepare leaves the probe unprepared.
        assert_eq!(ravet_event_probe_prepare(h, 0), REJECTED);
        assert_eq!(
            process(
                h,
                records.as_ptr(),
                10,
                left.as_mut_ptr(),
                right.as_mut_ptr(),
                FRAMES,
                &mut digest
            ),
            REJECTED
        );
        ravet_event_probe_destroy(h);
    }
}

#[test]
fn event_probe_abi_entry_points_f32() {
    probe_case::<f32>(32, |v| v as f32);
}

#[test]
fn event_probe_abi_entry_points_f64() {
    probe_case::<f64>(64, |v| v);
}
