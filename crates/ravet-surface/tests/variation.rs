#![cfg(feature = "text")]
use ravet_surface::{drawing::PathCommand, text::*};
const FONT: &[u8] = include_bytes!("fixtures/variation/RavetVariation.ttf");
fn options() -> TextOptions<'static> {
    TextOptions {
        size: 100.0,
        direction: TextDirection::LeftToRight,
        script: Some(*b"Latn"),
        language: None,
        features: &[],
    }
}
fn setting(tag: [u8; 4], value: f32) -> TextVariation {
    TextVariation { tag, value }
}
fn right(font: &TextFont) -> f64 {
    font.outline_glyph(2)
        .unwrap()
        .into_iter()
        .filter_map(|p| match p {
            PathCommand::MoveTo(x, _) | PathCommand::LineTo(x, _) => Some(x),
            _ => None,
        })
        .fold(f64::NEG_INFINITY, f64::max)
}
#[test]
fn independent_analytic_gvar_advance_outline_and_full_normalized_coordinates() {
    let base = TextFont::from_bytes(FONT, 0).unwrap();
    let axes = base.variation_axes().unwrap();
    assert_eq!(
        axes.iter()
            .map(|a| (a.tag, a.minimum, a.default, a.maximum))
            .collect::<Vec<_>>(),
        vec![
            (*b"wght", 100.0, 400.0, 900.0),
            (*b"wdth", 50.0, 100.0, 200.0)
        ]
    );
    let profiles: [(&[TextVariation], f64, f64); 6] = [
        (&[], 50.0, 480.0),
        (&[setting(*b"wght", 900.0)], 60.0, 560.0),
        (&[setting(*b"wght", 100.0)], 45.0, 440.0),
        (&[setting(*b"wdth", 200.0)], 70.0, 680.0),
        (&[setting(*b"wdth", 50.0)], 40.0, 380.0),
        (
            &[setting(*b"wght", 650.0), setting(*b"wdth", 150.0)],
            65.0,
            620.0,
        ),
    ];
    for (settings, advance, ink) in profiles {
        let font = base.with_variations(settings).unwrap();
        assert_eq!(font.shape("a", options()).unwrap().advance().0, advance);
        assert_eq!(right(&font), ink);
        for coordinate in font.variation_coordinates().unwrap() {
            let expected = match coordinate.tag {
                t if t == *b"wght" => {
                    if coordinate.user < 400.0 {
                        (coordinate.user - 400.0) / 300.0
                    } else {
                        (coordinate.user - 400.0) / 500.0
                    }
                }
                _ => {
                    if coordinate.user < 100.0 {
                        (coordinate.user - 100.0) / 50.0
                    } else {
                        (coordinate.user - 100.0) / 100.0
                    }
                }
            };
            assert_eq!(coordinate.normalized, expected);
        }
    }
    assert_eq!(base.shape("a", options()).unwrap().advance().0, 50.0);
    assert_eq!(right(&base), 480.0);
}
#[test]
fn immutable_instance_lifetime_and_reset_omitted_axes() {
    let original = TextFont::from_bytes(FONT, 0).unwrap();
    let heavy = original
        .with_variations(&[setting(*b"wght", 900.0), setting(*b"wdth", 200.0)])
        .unwrap();
    let heavy_clone = heavy.clone();
    drop(original);
    drop(heavy);
    assert_eq!(heavy_clone.shape("a", options()).unwrap().advance().0, 80.0);
    let reset = heavy_clone.with_variations(&[]).unwrap();
    assert_eq!(reset.shape("a", options()).unwrap().advance().0, 50.0);
    let wide = heavy_clone
        .with_variations(&[setting(*b"wdth", 200.0)])
        .unwrap();
    assert_eq!(wide.shape("a", options()).unwrap().advance().0, 70.0);
}
#[test]
fn strict_user_settings_never_clamp_ignore_duplicate_or_mutate() {
    let font = TextFont::from_bytes(FONT, 0)
        .unwrap()
        .with_variations(&[setting(*b"wght", 900.0)])
        .unwrap();
    for settings in [
        vec![setting(*b"oops", 1.0)],
        vec![setting(*b"wght", 99.0)],
        vec![setting(*b"wght", 901.0)],
        vec![setting(*b"wght", f32::NAN)],
        vec![setting(*b"wght", f32::INFINITY)],
        vec![setting(*b"wght", 400.0), setting(*b"wght", 900.0)],
        vec![setting(*b"w gh", 400.0)],
    ] {
        assert!(matches!(
            font.with_variations(&settings),
            Err(TextError::InvalidInput)
        ));
        assert_eq!(font.shape("a", options()).unwrap().advance().0, 60.0);
    }
    assert!(matches!(
        font.with_variations(&vec![setting(*b"wght", 400.0); 33]),
        Err(TextError::Capacity)
    ));
    let static_font = TextFont::from_bytes(
        include_bytes!("fixtures/fallback/RavetFallbackPrimary.ttf"),
        0,
    )
    .unwrap();
    assert!(static_font.variation_axes().unwrap().is_empty());
    assert!(static_font.with_variations(&[]).is_ok());
    assert!(matches!(
        static_font.with_variations(&[setting(*b"wght", 400.0)]),
        Err(TextError::InvalidInput)
    ));
}
#[test]
fn malformed_axis_bounds_duplicate_tags_and_capacity_reject_before_instancing() {
    fn fvar_offset(data: &[u8]) -> usize {
        let n = u16::from_be_bytes(data[4..6].try_into().unwrap()) as usize;
        (0..n)
            .find_map(|i| {
                let p = 12 + i * 16;
                (&data[p..p + 4] == b"fvar")
                    .then(|| u32::from_be_bytes(data[p + 8..p + 12].try_into().unwrap()) as usize)
            })
            .unwrap()
    }
    let start = fvar_offset(FONT);
    let axes = start + u16::from_be_bytes(FONT[start + 4..start + 6].try_into().unwrap()) as usize;
    for kind in 0..8 {
        let mut data = FONT.to_vec();
        match kind {
            0 => data[axes + 12..axes + 16].copy_from_slice(&0i32.to_be_bytes()),
            1 => data[axes + 20..axes + 24].copy_from_slice(b"wght"),
            2 => data[axes + 16..axes + 18].copy_from_slice(&2u16.to_be_bytes()),
            3 => data[start..start + 2].copy_from_slice(&2u16.to_be_bytes()),
            4 => data[start + 2..start + 4].copy_from_slice(&1u16.to_be_bytes()),
            5 => data[start + 6..start + 8].copy_from_slice(&0u16.to_be_bytes()),
            6 => data[start + 10..start + 12].copy_from_slice(&21u16.to_be_bytes()),
            _ => data[start + 4..start + 6].copy_from_slice(&0u16.to_be_bytes()),
        }
        let font = TextFont::from_bytes(&data, 0).unwrap();
        assert!(matches!(
            font.variation_axes(),
            Err(TextError::UnsupportedFont)
        ));
        assert!(matches!(
            font.with_variations(&[]),
            Err(TextError::UnsupportedFont)
        ));
    }
    let mut data = FONT.to_vec();
    data[start + 8..start + 10].copy_from_slice(&33u16.to_be_bytes());
    let font = TextFont::from_bytes(&data, 0).unwrap();
    assert_eq!(font.variation_axes(), Err(TextError::Capacity));
}
#[test]
fn actual_backend_fixed_rounding_is_visible_separately_from_user_input() {
    let font = TextFont::from_bytes(FONT, 0)
        .unwrap()
        .with_variations(&[setting(*b"wght", 400.001)])
        .unwrap();
    let coordinates = font.variation_coordinates().unwrap();
    assert_eq!(coordinates[0].user, 400.001);
    assert_eq!(coordinates[0].normalized, 0.0);
    assert_eq!(font.shape("a", options()).unwrap().advance().0, 50.0);
    assert_eq!(right(&font), 480.0);
}
#[test]
fn exact_varied_face_survives_fallback_and_svg_cpu_consumption() {
    use ravet_surface::{
        drawing::{Color, SvgDrawing},
        fallback::TextFontStack,
    };
    let font = TextFont::from_bytes(FONT, 0)
        .unwrap()
        .with_variations(&[setting(*b"wght", 650.0), setting(*b"wdth", 150.0)])
        .unwrap();
    let text = TextFontStack::new(&[&font])
        .unwrap()
        .shape("a", options())
        .unwrap();
    assert_eq!(text.advance().0, 65.0);
    assert_eq!(
        text.runs()[0].font().variation_coordinates().unwrap()[0].normalized,
        0.5
    );
    let mut svg = SvgDrawing::new(128.0, 100.0, "Varied a").unwrap();
    svg.fallback_text(&text, (0.0, 80.0), Color::rgb(255, 0, 0))
        .unwrap();
    let output = svg.finish();
    assert!(output.contains("62"));
    #[cfg(feature = "raster")]
    {
        use ravet_surface::raster::{RasterCanvas, RasterText};
        let prepared = RasterText::from_fallback(&text, (0.0, 80.0)).unwrap();
        let mut canvas = RasterCanvas::new(128, 100, 1).unwrap();
        canvas
            .draw_text(&prepared, Color::rgb(255, 0, 0), false)
            .unwrap();
        let covered = canvas
            .pixels()
            .as_chunks::<4>()
            .0
            .iter()
            .filter(|p| p[3] > 0)
            .count();
        assert_eq!(covered, 61 * 70);
        assert_eq!(
            &canvas.pixels()[4 * (10 * 128 + 1)..][..4],
            &[255, 0, 0, 255]
        );
        assert_eq!(&canvas.pixels()[4 * (10 * 128 + 62)..][..4], &[0, 0, 0, 0]);
    }
}
