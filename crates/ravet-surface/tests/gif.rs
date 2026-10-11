#![cfg(feature = "gif-image")]
use ravet_surface::image::ImageError;
use ravet_surface::image::{GifBackground, decode_static_gif};

fn single(
    index: u8,
    width: u16,
    height: u16,
    left: u16,
    top: u16,
    transparent: Option<u8>,
    local: bool,
) -> Vec<u8> {
    let mut out = b"GIF89a".to_vec();
    out.extend(width.to_le_bytes());
    out.extend(height.to_le_bytes());
    out.extend([0x80, 0, 0]);
    out.extend([255, 255, 255, 0, 0, 0]);
    if let Some(t) = transparent {
        out.extend([0x21, 0xf9, 4, 1, 0, 0, t, 0]);
    }
    out.push(0x2c);
    out.extend(left.to_le_bytes());
    out.extend(top.to_le_bytes());
    out.extend([1, 0, 1, 0, if local { 128 } else { 0 }]);
    if local {
        out.extend([255, 0, 0, 0, 0, 255]);
    }
    // Independent literal GIF LZW: clear4, one index, end5, minimum code width3.
    let bits = 4u16 | (u16::from(index) << 3) | (5 << 6);
    out.extend([2, 2, bits as u8, (bits >> 8) as u8, 0, 0x3b]);
    out
}

#[test]
fn first_party_pillow_palette_transparency_and_interlace_are_exact() {
    for (bytes, expected, width, height) in [
        (
            include_bytes!("fixtures/gif/opaque.gif").as_slice(),
            include_bytes!("fixtures/gif/opaque.rgba").as_slice(),
            7,
            5,
        ),
        (
            include_bytes!("fixtures/gif/transparent.gif").as_slice(),
            include_bytes!("fixtures/gif/transparent.rgba").as_slice(),
            9,
            3,
        ),
        (
            include_bytes!("fixtures/gif/interlaced.gif").as_slice(),
            include_bytes!("fixtures/gif/interlaced.rgba").as_slice(),
            37,
            23,
        ),
    ] {
        let saved = bytes.to_vec();
        for background in [GifBackground::Transparent, GifBackground::LogicalPalette] {
            let decoded = decode_static_gif(bytes, background).unwrap();
            assert_eq!((decoded.width(), decoded.height()), (width, height));
            assert_eq!(decoded.pixels(), expected);
            assert_eq!(bytes, saved);
        }
        for at in 0..bytes.len() {
            assert!(
                decode_static_gif(&bytes[..at], GifBackground::Transparent).is_err(),
                "prefix{at}"
            );
        }
    }
}

#[test]
fn literal_local_palette_offset_and_canvas_policy_are_explicit() {
    for local in [false, true] {
        for transparent in [None, Some(1)] {
            let bytes = single(1, 3, 2, 1, 1, transparent, local);
            for background in [GifBackground::Transparent, GifBackground::LogicalPalette] {
                let decoded = decode_static_gif(&bytes, background).unwrap();
                let fill = if background == GifBackground::Transparent {
                    [0; 4]
                } else {
                    [255; 4]
                };
                let mut expected = [fill; 6];
                expected[4] = [
                    0,
                    0,
                    if local { 255 } else { 0 },
                    if transparent.is_some() { 0 } else { 255 },
                ];
                assert_eq!(decoded.pixels(), expected.as_flattened());
            }
        }
    }
    let bytes = single(1, 4096, 1, 4095, 0, None, false);
    assert_eq!(
        decode_static_gif(&bytes, GifBackground::Transparent)
            .unwrap()
            .pixels()
            .len(),
        16384
    );
    let mut local_only = single(1, 1, 1, 0, 0, None, true);
    local_only[10] = 0;
    local_only.drain(13..19);
    assert_eq!(
        decode_static_gif(&local_only, GifBackground::Transparent)
            .unwrap()
            .pixels(),
        [0, 0, 255, 255]
    );
    assert_eq!(
        decode_static_gif(&local_only, GifBackground::LogicalPalette),
        Err(ImageError::Unsupported)
    );
}

#[test]
fn complete_structure_palette_lzw_and_animation_faults_reject() {
    let valid = single(1, 1, 1, 0, 0, None, false);
    for at in 0..valid.len() {
        assert!(decode_static_gif(&valid[..at], GifBackground::Transparent).is_err());
    }
    let mut trailing = valid.clone();
    trailing.push(0);
    assert_eq!(
        decode_static_gif(&trailing, GifBackground::Transparent),
        Err(ImageError::Decode)
    );
    assert_eq!(
        decode_static_gif(
            &single(3, 1, 1, 0, 0, None, false),
            GifBackground::Transparent
        ),
        Err(ImageError::Decode)
    );
    assert_eq!(
        decode_static_gif(
            &single(0, 1, 1, 0, 0, Some(3), false),
            GifBackground::Transparent
        ),
        Err(ImageError::Decode)
    );
    assert!(
        decode_static_gif(
            &single(0, 0, 1, 0, 0, None, false),
            GifBackground::Transparent
        )
        .is_err()
    );
    assert!(
        decode_static_gif(
            &single(0, 4097, 1, 0, 0, None, false),
            GifBackground::Transparent
        )
        .is_err()
    );
    assert!(
        decode_static_gif(
            &single(0, 1, 1, 1, 0, None, false),
            GifBackground::Transparent
        )
        .is_err()
    );
    let mut second = valid[..valid.len() - 1].to_vec();
    second.extend_from_slice(&valid[19..]);
    assert_eq!(
        decode_static_gif(&second, GifBackground::Transparent),
        Err(ImageError::Unsupported)
    );
    for extension in [
        b"\x21\xff\x0bNETSCAPE2.0\x03\x01\x00\x00\x00".as_slice(),
        b"\x21\x01\x00".as_slice(),
    ] {
        let mut bytes = valid[..19].to_vec();
        bytes.extend(extension);
        bytes.extend(&valid[19..]);
        assert_eq!(
            decode_static_gif(&bytes, GifBackground::Transparent),
            Err(ImageError::Unsupported)
        );
    }
    let mut missing_palette = valid.clone();
    missing_palette[10] = 0;
    missing_palette.drain(13..19);
    assert_eq!(
        decode_static_gif(&missing_palette, GifBackground::Transparent),
        Err(ImageError::Decode)
    );
    let mut missing_end = valid.clone();
    missing_end[31] = 4;
    missing_end[32] = 0;
    assert!(decode_static_gif(&missing_end, GifBackground::Transparent).is_err());
    let mut extra_pixel = valid.clone();
    extra_pixel[31] = 0x4c;
    extra_pixel[32] = 0x0a;
    assert_eq!(
        decode_static_gif(&extra_pixel, GifBackground::Transparent),
        Err(ImageError::Decode)
    );
    for min in [0, 1, 9, 255] {
        let mut bad = valid.clone();
        bad[29] = min;
        assert!(decode_static_gif(&bad, GifBackground::Transparent).is_err());
    }
}

#[test]
fn structural_record_budget_is_inclusive_and_bounded() {
    let valid = single(0, 1, 1, 0, 0, None, false);
    for (count, accepted) in [(4094, true), (4095, false)] {
        let mut bytes = valid[..19].to_vec();
        for _ in 0..count {
            bytes.extend([0x21, 0xfe, 0]);
        }
        bytes.extend(&valid[19..]);
        let result = decode_static_gif(&bytes, GifBackground::Transparent);
        if accepted {
            assert!(result.is_ok(), "{result:?}");
        } else {
            assert_eq!(result, Err(ImageError::Capacity));
        }
    }
}

#[test]
fn ignored_metadata_block_partition_and_control_flags_are_checked() {
    let valid = single(1, 1, 1, 0, 0, None, false);
    let expected = decode_static_gif(&valid, GifBackground::Transparent).unwrap();
    let mut old = valid.clone();
    old[3..6].copy_from_slice(b"87a");
    assert_eq!(
        decode_static_gif(&old, GifBackground::Transparent).unwrap(),
        expected
    );
    for extension in [
        b"\x21\xfe\x03abc\x00".as_slice(),
        b"\x21\xff\x0bRAVETDATA01\x03abc\x00".as_slice(),
    ] {
        for at in [19, valid.len() - 1] {
            let mut bytes = valid[..at].to_vec();
            bytes.extend(extension);
            bytes.extend(&valid[at..]);
            assert_eq!(
                decode_static_gif(&bytes, GifBackground::Transparent).unwrap(),
                expected
            );
        }
    }
    let mut partitioned = valid[..30].to_vec();
    partitioned.extend([1, valid[31], 1, valid[32], 0, 0x3b]);
    assert_eq!(
        decode_static_gif(&partitioned, GifBackground::Transparent).unwrap(),
        expected
    );
    for flags in [2, 16, 32, 255] {
        let mut bytes = single(1, 1, 1, 0, 0, Some(1), false);
        bytes[22] = flags;
        assert!(decode_static_gif(&bytes, GifBackground::Transparent).is_err());
    }
    let mut background = valid.clone();
    background[11] = 2;
    assert_eq!(
        decode_static_gif(&background, GifBackground::Transparent),
        Err(ImageError::Decode)
    );
    let mut packed = valid.clone();
    packed[28] = 0x18;
    assert_eq!(
        decode_static_gif(&packed, GifBackground::Transparent),
        Err(ImageError::Decode)
    );
    assert_eq!(
        decode_static_gif(&vec![0; 16 * 1024 * 1024 + 1], GifBackground::Transparent),
        Err(ImageError::Capacity)
    );
}
