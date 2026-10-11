use ravet_surface::color::*;
use ravet_surface::drawing::Color;
fn view(pixels: &[u8], encoding: ColorEncoding, mode: AlphaMode) -> RgbaView<'_> {
    RgbaView::new((pixels.len() / 4) as u32, 1, pixels, encoding, mode).unwrap()
}
#[test]
fn scalar_transfer_endpoints_thresholds_and_every_srgb_byte() {
    for c in 0..=255 {
        for alpha in [0, 1, 128, 255] {
            let encoded = Color {
                red: c,
                green: 255 - c,
                blue: c / 2,
                alpha,
            };
            assert_eq!(LinearRgba::from_srgba(encoded).to_srgba(), encoded);
        }
    }
    assert_eq!(
        LinearRgba::new(0., 0., 0., 0.).unwrap().components(),
        [0.; 4]
    );
    assert_eq!(
        LinearRgba::new(1., 1., 1., 1.).unwrap().to_srgba(),
        Color::rgb(255, 255, 255)
    );
    let dark = LinearRgba::from_srgba(Color::rgb(10, 10, 10)).components()[0];
    assert!((dark - (10. / 255. / 12.92)).abs() < 1e-16);
    let brighter = LinearRgba::from_srgba(Color::rgb(11, 11, 11)).components()[0];
    assert!((brighter - ((11. / 255. + 0.055) / 1.055_f64).powf(2.4)).abs() < 1e-16);
    assert_eq!(
        LinearRgba::new(0.0031308, 0., 0., 1.)
            .unwrap()
            .to_srgba()
            .red,
        10
    );
    for bad in [
        f64::NAN,
        f64::INFINITY,
        f64::NEG_INFINITY,
        -f64::EPSILON,
        1. + f64::EPSILON,
    ] {
        for index in 0..4 {
            let mut c = [0.5; 4];
            c[index] = bad;
            assert!(LinearRgba::new(c[0], c[1], c[2], c[3]).is_none());
        }
    }
}
#[test]
fn source_over_linear_light_and_translucent_backdrop() {
    let white = LinearRgba::from_srgba(Color {
        red: 255,
        green: 255,
        blue: 255,
        alpha: 128,
    });
    assert_eq!(
        white
            .source_over(LinearRgba::from_srgba(Color::rgb(0, 0, 0)))
            .to_srgba(),
        Color::rgb(188, 188, 188)
    );
    let red = LinearRgba::new(1., 0., 0., 0.5).unwrap();
    let blue = LinearRgba::new(0., 0., 1., 0.5).unwrap();
    let mixed = red.source_over(blue).components();
    assert_eq!(mixed[3], 0.75);
    assert!((mixed[0] - 2. / 3.).abs() < 1e-15);
    assert!((mixed[2] - 1. / 3.).abs() < 1e-15);
    assert_eq!(mixed[1], 0.);
    assert_ne!(red.source_over(blue), blue.source_over(red));
    let hidden = LinearRgba::new(1., 0.5, 0.25, 0.).unwrap();
    assert_eq!(hidden.to_srgba().alpha, 0);
    assert_eq!(hidden.to_srgba().red, 255);
    assert_eq!(hidden.source_over(hidden).components(), [0.; 4]);
    assert_eq!(hidden.source_over(blue), blue);
    assert_eq!(
        LinearRgba::new(1., 1., 1., 1.)
            .unwrap()
            .source_over(blue)
            .components(),
        [1.; 4]
    );
}
#[test]
fn profile_admission_and_exact_identity_copy() {
    let bytes = [255, 100, 80, 0, 1, 2, 3, 255];
    let src = view(&bytes, ColorEncoding::Srgb, AlphaMode::Straight);
    assert_eq!(
        (src.width(), src.height(), src.encoding(), src.alpha_mode()),
        (2, 1, ColorEncoding::Srgb, AlphaMode::Straight)
    );
    let mut output = [99; 8];
    src.convert_into(src.encoding(), src.alpha_mode(), &mut output)
        .unwrap();
    assert_eq!(output, bytes);
    assert_eq!(src.pixels(), bytes);
    for (w, h) in [(0, 1), (1, 0)] {
        assert_eq!(
            RgbaView::new(w, h, &[], ColorEncoding::Srgb, AlphaMode::Straight).map(|_| ()),
            Err(ColorError::InvalidInput)
        );
    }
    for (w, h) in [(4097, 1), (1, 4097), (u32::MAX, u32::MAX)] {
        assert_eq!(
            RgbaView::new(w, h, &[], ColorEncoding::Srgb, AlphaMode::Straight).map(|_| ()),
            Err(ColorError::Capacity)
        );
    }
    assert!(RgbaView::new(1, 1, &[0; 3], ColorEncoding::Srgb, AlphaMode::Straight).is_err());
    assert!(RgbaView::new(1, 1, &[0; 5], ColorEncoding::Srgb, AlphaMode::Straight).is_err());
    let boundary = vec![0; 4096 * 4];
    assert!(RgbaView::new(4096, 1, &boundary, ColorEncoding::Srgb, AlphaMode::Straight).is_ok());
    for encoding in [ColorEncoding::Srgb, ColorEncoding::LinearSrgb] {
        assert!(
            RgbaView::new(
                2,
                1,
                &[0, 0, 0, 0, 128, 127, 126, 128],
                encoding,
                AlphaMode::Premultiplied
            )
            .is_ok()
        );
        assert!(
            RgbaView::new(
                2,
                1,
                &[0, 0, 0, 0, 128, 127, 129, 128],
                encoding,
                AlphaMode::Premultiplied
            )
            .is_err()
        );
        assert!(RgbaView::new(1, 1, &[1, 0, 0, 0], encoding, AlphaMode::Premultiplied).is_err());
    }
}
#[test]
fn encoded_premultiplication_and_hidden_rgb_policy() {
    let bytes = [255, 128, 64, 128, 200, 100, 50, 0];
    let source = view(&bytes, ColorEncoding::Srgb, AlphaMode::Straight);
    let mut encoded = [0; 8];
    source
        .convert_into(ColorEncoding::Srgb, AlphaMode::Premultiplied, &mut encoded)
        .unwrap();
    assert_eq!(encoded, [128, 64, 32, 128, 0, 0, 0, 0]);
    let mut linear = [0; 8];
    source
        .convert_into(
            ColorEncoding::LinearSrgb,
            AlphaMode::Premultiplied,
            &mut linear,
        )
        .unwrap();
    assert_eq!(linear, [128, 28, 7, 128, 0, 0, 0, 0]);
    let mut hidden = [0; 8];
    source
        .convert_into(ColorEncoding::LinearSrgb, AlphaMode::Straight, &mut hidden)
        .unwrap();
    assert_ne!(&hidden[4..7], &[0, 0, 0]);
    assert_eq!(hidden[7], 0);
    let mut restored = [0; 8];
    view(&encoded, ColorEncoding::Srgb, AlphaMode::Premultiplied)
        .convert_into(ColorEncoding::Srgb, AlphaMode::Straight, &mut restored)
        .unwrap();
    assert_eq!(restored, [255, 128, 64, 128, 0, 0, 0, 0]);
    assert_ne!(encoded, linear);
    let tie = view(
        &[170, 17, 128, 170],
        ColorEncoding::Srgb,
        AlphaMode::Premultiplied,
    );
    let mut exact = [0; 4];
    tie.convert_into(ColorEncoding::Srgb, AlphaMode::Straight, &mut exact)
        .unwrap();
    assert_eq!(exact, [255, 26, 192, 170]);
}
#[test]
fn whole_output_preflight_and_mixed_profile_composition() {
    let s = [255, 255, 255, 128];
    let b = [0, 0, 0, 255];
    let source = view(&s, ColorEncoding::Srgb, AlphaMode::Straight);
    let backdrop = view(&b, ColorEncoding::LinearSrgb, AlphaMode::Premultiplied);
    let mut output = [7; 4];
    composite_source_over(
        source,
        backdrop,
        ColorEncoding::Srgb,
        AlphaMode::Straight,
        &mut output,
    )
    .unwrap();
    assert_eq!(output, [188, 188, 188, 255]);
    for length in [0, 3, 5, 8] {
        let mut destination = vec![77; length];
        let original = destination.clone();
        assert_eq!(
            source.convert_into(
                ColorEncoding::LinearSrgb,
                AlphaMode::Premultiplied,
                &mut destination
            ),
            Err(ColorError::InvalidInput)
        );
        assert_eq!(destination, original);
        assert_eq!(
            composite_source_over(
                source,
                backdrop,
                ColorEncoding::Srgb,
                AlphaMode::Straight,
                &mut destination
            ),
            Err(ColorError::InvalidInput)
        );
        assert_eq!(destination, original);
    }
    let incompatible = view(&[0; 8], ColorEncoding::Srgb, AlphaMode::Straight);
    output = [13; 4];
    assert_eq!(
        composite_source_over(
            source,
            incompatible,
            ColorEncoding::Srgb,
            AlphaMode::Straight,
            &mut output
        ),
        Err(ColorError::InvalidInput)
    );
    assert_eq!(output, [13; 4]);
    let hidden = view(
        &[255, 200, 100, 0],
        ColorEncoding::Srgb,
        AlphaMode::Straight,
    );
    composite_source_over(
        hidden,
        hidden,
        ColorEncoding::Srgb,
        AlphaMode::Straight,
        &mut output,
    )
    .unwrap();
    assert_eq!(output, [0; 4]);
}
#[test]
fn mixed_associations_preserve_alpha_and_output_premultiplied_invariant() {
    for alpha in 0..=255 {
        let p = [alpha, alpha / 2, alpha / 3, alpha];
        let source = view(&p, ColorEncoding::Srgb, AlphaMode::Premultiplied);
        for encoding in [ColorEncoding::Srgb, ColorEncoding::LinearSrgb] {
            let mut out = [0; 4];
            source
                .convert_into(encoding, AlphaMode::Premultiplied, &mut out)
                .unwrap();
            assert_eq!(out[3], alpha);
            assert!(out[..3].iter().all(|v| *v <= alpha));
            let backdrop = view(
                &[27, 128, 210, 129],
                ColorEncoding::Srgb,
                AlphaMode::Straight,
            );
            composite_source_over(
                source,
                backdrop,
                encoding,
                AlphaMode::Premultiplied,
                &mut out,
            )
            .unwrap();
            assert!(out[..3].iter().all(|v| *v <= out[3]));
        }
    }
}
#[test]
fn prepared_views_conversion_and_compositing_allocate_nothing() {
    let foreground = [255, 180, 60, 128];
    let background = [30, 30, 30, 255];
    let foreground = view(&foreground, ColorEncoding::Srgb, AlphaMode::Straight);
    let background = view(&background, ColorEncoding::Srgb, AlphaMode::Straight);
    let mut output = [0; 4];
    let measured = allocation_counter::measure(|| {
        for _ in 0..4096 {
            foreground
                .convert_into(
                    ColorEncoding::LinearSrgb,
                    AlphaMode::Premultiplied,
                    &mut output,
                )
                .unwrap();
            composite_source_over(
                foreground,
                background,
                ColorEncoding::Srgb,
                AlphaMode::Straight,
                &mut output,
            )
            .unwrap();
            std::hint::black_box(output);
        }
    });
    assert_eq!(measured.count_total, 0);
    assert_eq!(measured.bytes_total, 0);
}
#[test]
fn inclusive_image_capacity_and_last_pixel_validation() {
    let mut bytes = vec![0; 4096 * 4096 * 4];
    assert!(RgbaView::new(4096, 4096, &bytes, ColorEncoding::Srgb, AlphaMode::Straight).is_ok());
    assert!(
        RgbaView::new(
            4096,
            4096,
            &bytes,
            ColorEncoding::Srgb,
            AlphaMode::Premultiplied
        )
        .is_ok()
    );
    let index = bytes.len() - 4;
    bytes[index] = 1;
    assert_eq!(
        RgbaView::new(
            4096,
            4096,
            &bytes,
            ColorEncoding::Srgb,
            AlphaMode::Premultiplied
        )
        .map(|_| ()),
        Err(ColorError::InvalidInput)
    );
    assert_eq!(bytes[index], 1);
}

#[test]
fn normalized_weights_preserve_color_at_subnormal_alpha() {
    let smallest = f64::from_bits(1);
    let foreground = LinearRgba::new(0.5, 0.25, 1., smallest).unwrap();
    let transparent = LinearRgba::new(0.9, 0.4, 0.1, 0.).unwrap();
    assert_eq!(
        foreground.source_over(transparent).components(),
        foreground.components()
    );
    assert_eq!(
        transparent.source_over(foreground).components(),
        foreground.components()
    );
    assert_eq!(
        foreground.source_over(foreground).components(),
        [0.5, 0.25, 1., f64::from_bits(2)]
    );
}
