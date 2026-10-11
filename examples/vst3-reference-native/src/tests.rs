use super::*;
use ravet_foundation::{AudioBuffer, Transport};

fn keyed<S: ReferenceSample>(
    keys: usize,
    main: [&[S]; 2],
    key: &[&[S]],
    frames: usize,
) -> (ProcessStatus, [Vec<S>; 2]) {
    let mut gain = KeyedGain::new();
    let mut config = ProcessConfig::new(48000., 64, 0, AudioLayout::STEREO).unwrap();
    if keys > 0 {
        config = config
            .with_auxiliary_buses(&[AudioLayout::new(keys, 0).unwrap()])
            .unwrap();
    }
    Processor::<S>::prepare(&mut gain, config).unwrap();
    let mut out = [vec![S::default(); frames], vec![S::default(); frames]];
    let [l, r] = &mut out;
    let mut outs = [&mut l[..], &mut r[..]];
    let audio = AudioBuffer::separate(&main, &mut outs, frames).unwrap();
    let mut none: [&mut [S]; 0] = [];
    let mut aux = [AudioBuffer::separate(key, &mut none, frames).unwrap()];
    let mut block = ProcessBlock::new(audio, &[], Transport::default());
    if keys > 0 {
        block.auxiliary = Some(&mut aux);
    }
    let status = gain.process(&mut block);
    (status, out)
}

#[test]
fn keyed_gain_matches_closed_form_for_every_key_layout() {
    let l: Vec<f64> = (0..33).map(|i| 0.1 + f64::from(i) * 1e-9).collect();
    let r: Vec<f64> = (0..33).map(|i| -0.3 + f64::from(i) * 0.01).collect();
    let k0: Vec<f64> = (0..33).map(|i| 0.5 - f64::from(i) * 0.03).collect();
    let k1: Vec<f64> = (0..33).map(|i| f64::from(i) * 0.07 - 1.).collect();
    let (status, out) = keyed(2, [&l, &r], &[&k0, &k1], 33);
    assert_eq!(status, ProcessStatus::Continue);
    for i in 0..33 {
        let key = (k0[i] + k1[i]) * 0.5;
        assert_eq!(out[0][i], l[i] * key);
        assert_eq!(out[1][i], r[i] * key);
    }
    let (_, out) = keyed(1, [&l, &r], &[&k0], 33);
    assert_eq!(out[0][7], l[7] * k0[7]);
    // Sub-f32 precision survives the f64 path.
    assert_ne!(out[0][7], f64::from((l[7] * k0[7]) as f32));
    let (_, out) = keyed(0, [&l, &r], &[], 33);
    assert_eq!(out, [l.clone(), r.clone()]);
    let lf: Vec<f32> = l.iter().map(|v| *v as f32).collect();
    let kf: Vec<f32> = k0.iter().map(|v| *v as f32).collect();
    let (_, out) = keyed(1, [&lf, &lf], &[&kf], 33);
    assert_eq!(out[1][32], lf[32] * kf[32]);
    let (status, out) = keyed::<f32>(1, [&[], &[]], &[&[]], 0);
    assert_eq!((status, out[0].len()), (ProcessStatus::Continue, 0));
}

#[test]
fn keyed_gain_refuses_other_layouts() {
    let mut gain = KeyedGain::new();
    let base = ProcessConfig::new(48000., 64, 0, AudioLayout::STEREO).unwrap();
    for config in [
        ProcessConfig::new(48000., 64, 0, AudioLayout::MONO).unwrap(),
        base.with_auxiliary_buses(&[AudioLayout::new(3, 0).unwrap()])
            .unwrap(),
        base.with_auxiliary_buses(&[AudioLayout::new(1, 1).unwrap()])
            .unwrap(),
        base.with_auxiliary_buses(&[AudioLayout::new(1, 0).unwrap(); 2])
            .unwrap(),
        base.with_output_events(1),
    ] {
        assert_eq!(
            Processor::<f32>::prepare(&mut gain, config),
            Err(PrepareError::UnsupportedLayout)
        );
    }
}

/// Independent canonical serializer (allocating) for the hash cross-check.
fn serialize(record: &ProbeRecord<'_>) -> Vec<u8> {
    let mut v = Vec::new();
    v.extend(record.event.code().to_le_bytes());
    v.extend(record.header.flags.to_le_bytes());
    v.extend(record.header.bus.to_le_bytes());
    v.extend(record.header.ppq_position.to_bits().to_le_bytes());
    let text = |v: &mut Vec<u8>, t: &[u16]| {
        v.extend((t.len() as u32).to_le_bytes());
        t.iter().for_each(|u| v.extend(u.to_le_bytes()));
    };
    match record.event {
        ProbeEvent::NoteOn {
            channel,
            pitch,
            note_id,
            length,
            tuning,
            velocity,
        } => {
            [channel, pitch, note_id, length]
                .iter()
                .for_each(|x| v.extend(x.to_le_bytes()));
            v.extend(tuning.to_bits().to_le_bytes());
            v.extend(velocity.to_bits().to_le_bytes());
        }
        ProbeEvent::Data { data_type, bytes } => {
            v.extend(data_type.to_le_bytes());
            v.extend((bytes.len() as u32).to_le_bytes());
            v.extend(bytes);
        }
        ProbeEvent::Chord {
            root,
            bass,
            mask,
            text: t,
        } => {
            v.extend(root.to_le_bytes());
            v.extend(bass.to_le_bytes());
            v.extend(u32::from(mask).to_le_bytes());
            text(&mut v, t);
        }
        ProbeEvent::Parameter { id, value } => {
            v.extend(id.to_le_bytes());
            v.extend(value.to_bits().to_le_bytes());
        }
        _ => unreachable!("cross-check covers four representative kinds"),
    }
    v
}
fn fnv(bytes: &[u8]) -> u32 {
    let h = bytes.iter().fold(0x811c_9dc5u32, |h, b| {
        (h ^ u32::from(*b)).wrapping_mul(16_777_619)
    });
    (h ^ (h >> 20)) & 0xf_ffff
}

#[test]
fn hash_matches_independent_serialization_and_known_fnv_vector() {
    // Standard FNV-1a vector: "a" -> 0xe40c292c.
    let h = 0xe40c_292cu32;
    assert_eq!(fnv(b"a"), (h ^ (h >> 20)) & 0xf_ffff);
    let text: Vec<u16> = "Cmaj7 \u{1d11e}".encode_utf16().collect();
    let header = ProbeHeader {
        offset: 3,
        bus: 0,
        flags: 0xc001,
        ppq_position: -0.125,
    };
    let records = [
        ProbeRecord {
            header,
            event: ProbeEvent::NoteOn {
                channel: 15,
                pitch: 127,
                note_id: -1,
                length: 9,
                tuning: -3.25,
                velocity: 0.75,
            },
        },
        ProbeRecord {
            header,
            event: ProbeEvent::Data {
                data_type: 0,
                bytes: &[0xf0, 0x7d, 0x00, 0xf7],
            },
        },
        ProbeRecord {
            header,
            event: ProbeEvent::Chord {
                root: 60,
                bass: 48,
                mask: 0xffff,
                text: &text,
            },
        },
        ProbeRecord {
            header: ProbeHeader {
                offset: 0,
                bus: 0,
                flags: 0,
                ppq_position: 0.,
            },
            event: ProbeEvent::Parameter { id: 1, value: 0.5 },
        },
    ];
    for record in &records {
        assert_eq!(record_hash(record), fnv(&serialize(record)));
    }
    assert_ne!(record_hash(&records[0]), record_hash(&records[1]));
}

#[test]
fn probe_renders_codes_hashes_and_digest_without_allocation() {
    let mut probe = EventProbe::new();
    probe.prepare(64).unwrap();
    let header = |offset| ProbeHeader {
        offset,
        bus: 0,
        flags: 0,
        ppq_position: 0.,
    };
    let records = [
        ProbeRecord {
            header: header(2),
            event: ProbeEvent::ExpressionInteger {
                type_id: 7,
                note_id: 3,
                value: u64::MAX,
            },
        },
        ProbeRecord {
            header: header(2),
            event: ProbeEvent::Parameter { id: 1, value: 0.25 },
        },
        ProbeRecord {
            header: header(9),
            event: ProbeEvent::Scale {
                root: 2,
                mask: 0xad5,
                text: &[0x41],
            },
        },
    ];
    let (mut left, mut right) = ([9f32; 16], [9f32; 16]);
    let measured = allocation_counter::measure(|| {
        let digest = probe.render(&records, &mut left, &mut right, 12).unwrap();
        assert_eq!(digest, probe.digest());
    });
    assert_eq!(measured.count_total, 0);
    let h: Vec<u32> = records.iter().map(record_hash).collect();
    assert_eq!(left[2], 9. + 17.);
    assert_eq!(right[2], h[0] as f32 / 1048576. + h[1] as f32 / 1048576.);
    assert_eq!((left[9], right[9]), (8., h[2] as f32 / 1048576.));
    assert_eq!(left[..12].iter().filter(|v| **v != 0.).count(), 2);
    assert_eq!(left[12..], [9.; 4]);
    let expected = h.iter().fold(0, |d, x| digest_step(d, *x));
    assert_eq!(probe.digest(), expected);
    // Zero-frame blocks still fold records; offsets must be zero.
    let zero = [ProbeRecord {
        header: header(0),
        ..records[0]
    }];
    let digest = probe.render::<f64>(&zero, &mut [], &mut [], 0).unwrap();
    assert_eq!(digest, digest_step(expected, h[0]));
    // Rejection silences, keeps the digest.
    assert_eq!(
        probe.render(&records, &mut left, &mut right, 9),
        Err(ProbeError::OffsetOutOfRange { index: 2 })
    );
    assert_eq!(left[..9], [0.; 9]);
    assert_eq!(probe.digest(), digest);
    probe.reset();
    assert_eq!(probe.digest(), 0);
    assert_eq!(
        probe.render::<f32>(&[], &mut left, &mut right, 65),
        Err(ProbeError::TooManyFrames)
    );
}
