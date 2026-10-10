#![cfg(feature = "image")]
use ravet_surface::image::*;
fn png(bytes: &[u8]) -> ImageAsset {
    decode_image(ImageFormat::Png, bytes).unwrap()
}
fn crc(bytes: &[u8]) -> u32 {
    let mut c = !0u32;
    for b in bytes {
        c ^= u32::from(*b);
        for _ in 0..8 {
            c = (c >> 1) ^ if c & 1 != 0 { 0xedb88320 } else { 0 };
        }
    }
    !c
}
fn chunk(kind: &[u8; 4], body: &[u8]) -> Vec<u8> {
    let mut v = (body.len() as u32).to_be_bytes().to_vec();
    v.extend(kind);
    v.extend(body);
    v.extend(crc(&v[4..]).to_be_bytes());
    v
}
#[test]
fn independent_png_raw_rows_palette_transparency_depth_and_adam7() {
    let rgba = png(include_bytes!("fixtures/image/rgba.png"));
    assert_eq!(rgba.pixels(), include_bytes!("fixtures/image/rgba.raw"));
    assert_eq!((rgba.width(), rgba.height()), (13, 9));
    assert_eq!(
        png(include_bytes!("fixtures/image/gray16.png")).pixels(),
        [0, 0, 0, 255, 128, 128, 128, 255, 255, 255, 255, 255]
    );
    assert_eq!(
        png(include_bytes!("fixtures/image/palette.png")).pixels(),
        [
            255, 0, 0, 255, 0, 255, 0, 128, 0, 0, 255, 0, 100, 50, 10, 200
        ]
    );
    assert_eq!(
        png(include_bytes!("fixtures/image/gray-alpha.png")).pixels(),
        [33, 33, 33, 128, 200, 200, 200, 0]
    );
    assert_eq!(
        png(include_bytes!("fixtures/image/gray1.png"))
            .pixels()
            .as_chunks::<4>()
            .0
            .iter()
            .map(|p| p[0])
            .collect::<Vec<_>>(),
        [255, 0, 255, 0, 0, 255, 255, 0]
    );
    assert_eq!(
        png(include_bytes!("fixtures/image/rgb-trns.png")).pixels(),
        [1, 2, 3, 0, 4, 5, 6, 255]
    );
    assert_eq!(
        png(include_bytes!("fixtures/image/adam7.png")).pixels(),
        include_bytes!("fixtures/image/adam7.raw")
    );
}
#[test]
fn independent_jpeg_baseline_progressive_rgb_and_gray_agree_with_pillow() {
    for (data, oracle) in [
        (
            include_bytes!("fixtures/image/rgb-False.jpg").as_slice(),
            include_bytes!("fixtures/image/rgb-False.jpg.raw").as_slice(),
        ),
        (
            include_bytes!("fixtures/image/rgb-True.jpg").as_slice(),
            include_bytes!("fixtures/image/rgb-True.jpg.raw").as_slice(),
        ),
        (
            include_bytes!("fixtures/image/l-False.jpg").as_slice(),
            include_bytes!("fixtures/image/l-False.jpg.raw").as_slice(),
        ),
        (
            include_bytes!("fixtures/image/l-True.jpg").as_slice(),
            include_bytes!("fixtures/image/l-True.jpg.raw").as_slice(),
        ),
        (
            include_bytes!("fixtures/image/rgb-subsampling1.jpg").as_slice(),
            include_bytes!("fixtures/image/rgb-subsampling1.jpg.raw").as_slice(),
        ),
        (
            include_bytes!("fixtures/image/rgb-subsampling2.jpg").as_slice(),
            include_bytes!("fixtures/image/rgb-subsampling2.jpg.raw").as_slice(),
        ),
        (
            include_bytes!("fixtures/image/rgb-orientation6.jpg").as_slice(),
            include_bytes!("fixtures/image/rgb-orientation6.jpg.raw").as_slice(),
        ),
    ] {
        let a = decode_image(ImageFormat::Jpeg, data).unwrap();
        assert_eq!((a.width(), a.height()), (37, 23));
        assert_eq!(a.pixels().len(), oracle.len());
        let max = a
            .pixels()
            .iter()
            .zip(oracle)
            .map(|(a, b)| (i16::from(*a) - i16::from(*b)).abs())
            .max()
            .unwrap();
        eprintln!("JPEG independent component difference {max}");
        assert!(max <= 4, "JPEG backend component difference {max}");
        assert!(a.pixels().as_chunks::<4>().0.iter().all(|p| p[3] == 255));
    }
}
#[test]
fn png_owned_encoding_roundtrip_budget_and_numeric_premultiplication() {
    let a = png(include_bytes!("fixtures/image/rgba.png"));
    let encoded = encode_png(&a, 65536).unwrap();
    assert_eq!(png(&encoded), a);
    for budget in [0, 1, 8, 32, encoded.len() - 1, 16 * 1024 * 1024 + 1] {
        assert_eq!(encode_png(&a, budget), Err(ImageError::Capacity));
    }
    let copy = a.clone();
    let pma = a.into_premultiplied_rgba();
    for (p, s) in pma
        .as_chunks::<4>()
        .0
        .iter()
        .zip(copy.pixels().as_chunks::<4>().0)
    {
        assert_eq!(p[3], s[3]);
        for i in 0..3 {
            assert_eq!(
                p[i],
                ((u16::from(s[i]) * u16::from(s[3]) + 127) / 255) as u8
            );
            assert!(p[i] <= p[3]);
        }
    }
    assert_eq!(
        png(include_bytes!("fixtures/image/rgba.png")).into_straight_rgba(),
        copy.pixels()
    );
}
#[test]
fn dimensions_shape_source_and_complete_stream_rejection_are_explicit() {
    assert_eq!(
        ImageAsset::from_straight_rgba(0, 1, vec![]),
        Err(ImageError::InvalidInput)
    );
    assert_eq!(
        ImageAsset::from_straight_rgba(4097, 1, vec![]),
        Err(ImageError::Capacity)
    );
    assert_eq!(
        ImageAsset::from_straight_rgba(1, 1, vec![0; 3]),
        Err(ImageError::InvalidInput)
    );
    for format in [ImageFormat::Png, ImageFormat::Jpeg] {
        assert_eq!(decode_image(format, &[]), Err(ImageError::InvalidInput));
        assert_eq!(
            decode_image(format, &vec![0; 16 * 1024 * 1024 + 1]),
            Err(ImageError::Capacity)
        );
    }
    for (format, fixture) in [
        (
            ImageFormat::Png,
            include_bytes!("fixtures/image/rgba.png").as_slice(),
        ),
        (
            ImageFormat::Jpeg,
            include_bytes!("fixtures/image/rgb-True.jpg").as_slice(),
        ),
    ] {
        for at in 1..fixture.len() {
            assert!(
                decode_image(format, &fixture[..at]).is_err(),
                "accepted truncated {format:?} at{at}"
            );
        }
        let mut tail = fixture.to_vec();
        tail.push(0);
        assert_eq!(decode_image(format, &tail), Err(ImageError::Decode));
    }
    assert_eq!(
        decode_image(ImageFormat::Jpeg, include_bytes!("fixtures/image/cmyk.jpg")),
        Err(ImageError::Unsupported)
    );
    let base = include_bytes!("fixtures/image/rgba.png");
    let mut big = base.to_vec();
    big[16..20].copy_from_slice(&4097u32.to_be_bytes());
    let h = crc(&big[12..29]);
    big[29..33].copy_from_slice(&h.to_be_bytes());
    assert_eq!(
        decode_image(ImageFormat::Png, &big),
        Err(ImageError::Capacity)
    );
}
#[test]
fn png_crc_adler_animation_order_and_marker_budget_faults_reject() {
    let base = include_bytes!("fixtures/image/rgba.png");
    let mut corrupt = base.to_vec();
    corrupt[29] ^= 1;
    assert_eq!(
        decode_image(ImageFormat::Png, &corrupt),
        Err(ImageError::Decode)
    );
    let mut bad = base.to_vec();
    let mut at = 8;
    while &bad[at + 4..at + 8] != b"IDAT" {
        at += 12 + u32::from_be_bytes(bad[at..at + 4].try_into().unwrap()) as usize;
    }
    let n = u32::from_be_bytes(bad[at..at + 4].try_into().unwrap()) as usize;
    bad[at + 8 + n - 1] ^= 1;
    let h = crc(&bad[at + 4..at + 8 + n]);
    bad[at + 8 + n..at + 12 + n].copy_from_slice(&h.to_be_bytes());
    assert_eq!(
        decode_image(ImageFormat::Png, &bad),
        Err(ImageError::Decode)
    );
    let mut gamma = base[..33].to_vec();
    gamma.extend(chunk(b"gAMA", &50000u32.to_be_bytes()));
    gamma.extend(&base[33..]);
    assert_eq!(png(&gamma), png(base));
    let mut boundary = base[..33].to_vec();
    for _ in 0..8189 {
        boundary.extend(chunk(b"raVe", &[]));
    }
    boundary.extend(&base[33..]);
    assert_eq!(png(&boundary), png(base));
    let mut anim = base[..33].to_vec();
    anim.extend(chunk(b"acTL", &[0, 0, 0, 1, 0, 0, 0, 0]));
    anim.extend(&base[33..]);
    assert_eq!(
        decode_image(ImageFormat::Png, &anim),
        Err(ImageError::Unsupported)
    );
    let mut many = base[..33].to_vec();
    for _ in 0..8192 {
        many.extend(chunk(b"raVe", &[]));
    }
    many.extend(&base[33..]);
    assert_eq!(
        decode_image(ImageFormat::Png, &many),
        Err(ImageError::Capacity)
    );
    let mut dup = base[..33].to_vec();
    dup.extend(&base[8..33]);
    dup.extend(&base[33..]);
    assert_eq!(
        decode_image(ImageFormat::Png, &dup),
        Err(ImageError::Decode)
    );
    // Split IDAT around a valid ignored ancillary chunk: nonconsecutive IDAT forbidden.
    let id = &base[at + 8..at + 8 + n];
    let mut split = base[..at].to_vec();
    split.extend(chunk(b"IDAT", &id[..n / 2]));
    split.extend(chunk(b"raVe", &[]));
    split.extend(chunk(b"IDAT", &id[n / 2..]));
    split.extend(&base[at + 12 + n..]);
    assert_eq!(
        decode_image(ImageFormat::Png, &split),
        Err(ImageError::Decode)
    );
}
#[test]
fn jpeg_scan_restart_and_header_shape_budgets_reject_before_backend() {
    let jpeg = include_bytes!("fixtures/image/rgb-False.jpg");
    let mut at = 2;
    let mut sof = 0;
    let mut sos = 0;
    while at < jpeg.len() {
        let marker = jpeg[at + 1];
        let size = u16::from_be_bytes(jpeg[at + 2..at + 4].try_into().unwrap()) as usize;
        if marker == 0xc0 {
            sof = at;
        }
        if marker == 0xda {
            sos = at;
            break;
        }
        at += 2 + size;
    }
    assert!(sof > 0 && sos > 0);
    let mut oversize = jpeg.to_vec();
    oversize[sof + 7..sof + 9].copy_from_slice(&4097u16.to_be_bytes());
    assert_eq!(
        decode_image(ImageFormat::Jpeg, &oversize),
        Err(ImageError::Capacity)
    );
    let mut bad = jpeg.to_vec();
    bad[sof + 4] = 12;
    assert_eq!(
        decode_image(ImageFormat::Jpeg, &bad),
        Err(ImageError::Unsupported)
    );
    let scanlen = u16::from_be_bytes(jpeg[sos + 2..sos + 4].try_into().unwrap()) as usize + 2;
    let mut many = jpeg[..sos].to_vec();
    for _ in 0..33 {
        many.extend(&jpeg[sos..sos + scanlen]);
        many.push(1);
    }
    many.extend([255, 217]);
    assert_eq!(
        decode_image(ImageFormat::Jpeg, &many),
        Err(ImageError::Capacity)
    );
    let mut restart = jpeg[..sos + scanlen].to_vec();
    for _ in 0..8192 {
        restart.extend([255, 208]);
    }
    restart.extend([255, 217]);
    assert_eq!(
        decode_image(ImageFormat::Jpeg, &restart),
        Err(ImageError::Capacity)
    );
}
