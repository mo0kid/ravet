use ravet_surface::drawing::{Color, DrawingError, PathCommand, SvgDrawing};
fn drawing() -> SvgDrawing {
    SvgDrawing::new(100., 100., "Path").unwrap()
}
#[test]
fn typed_paths_serialize_curves_contours_and_reject_atomically() {
    let color = Color::rgb(10, 20, 30);
    let mut svg = drawing();
    svg.path(
        &[
            PathCommand::MoveTo(1., 2.),
            PathCommand::LineTo(3., 4.),
            PathCommand::QuadraticTo {
                control: (5., 6.),
                end: (7., 8.),
            },
            PathCommand::CubicTo {
                first: (9., 10.),
                second: (11., 12.),
                end: (13., 14.),
            },
            PathCommand::Close,
            PathCommand::MoveTo(15., 16.),
            PathCommand::LineTo(17., 18.),
        ],
        color,
    )
    .unwrap();
    assert_eq!(svg.command_count(), 1);
    let output = svg.finish();
    assert!(output.contains("M1 2 L3 4 Q5 6 7 8 C9 10 11 12 13 14 Z M15 16 L17 18"));
    assert!(!output.contains("<script"));
    let mut rejected = drawing();
    for path in [
        vec![],
        vec![PathCommand::LineTo(0., 0.)],
        vec![PathCommand::Close],
        vec![
            PathCommand::MoveTo(0., 0.),
            PathCommand::Close,
            PathCommand::LineTo(0., 0.),
        ],
        vec![PathCommand::MoveTo(f64::NAN, 0.)],
        vec![
            PathCommand::MoveTo(0., 0.),
            PathCommand::QuadraticTo {
                control: (0., f64::INFINITY),
                end: (1., 1.),
            },
        ],
    ] {
        assert_eq!(rejected.path(&path, color), Err(DrawingError::InvalidInput));
        assert_eq!(rejected.command_count(), 0);
    }
    assert_eq!(
        rejected.path(&vec![PathCommand::MoveTo(0., 0.); 4097], color),
        Err(DrawingError::Capacity)
    );
    assert_eq!(rejected.finish(), drawing().finish());
}
#[cfg(feature = "text")]
#[test]
fn local_glyphs_outline_to_paths_and_whole_run_capacity_rolls_back() {
    use ravet_surface::{
        layout::Rect,
        text::{TextDirection, TextError, TextFont, TextOptions},
    };
    let font = TextFont::from_bytes(include_bytes!("fixtures/text/NotoSansArabic.ttf"), 0).unwrap();
    let options = TextOptions {
        size: 32.,
        direction: TextDirection::LeftToRight,
        script: Some(*b"Latn"),
        language: Some("en"),
        features: &[],
    };
    let run = font.shape("A", options).unwrap();
    let glyph = run.glyphs()[0].id;
    let outline = font.outline_glyph(glyph).unwrap();
    assert!(!outline.is_empty());
    assert!(matches!(outline[0], PathCommand::MoveTo(..)));
    assert!(matches!(
        font.outline_glyph(u32::MAX),
        Err(TextError::InvalidInput)
    ));
    let space = font.shape(" ", options).unwrap();
    assert!(font.outline_glyph(space.glyphs()[0].id).unwrap().is_empty());
    let mut svg = drawing();
    svg.shaped_text(&font, "AV", options, (5., 50.), Color::rgb(0, 0, 0))
        .unwrap();
    assert_eq!(svg.command_count(), 2);
    let output = svg.finish();
    assert!(output.contains("<path"));
    assert!(
        !output.contains("<text") && !output.contains("font-family") && !output.contains("href=")
    );
    fn full() -> SvgDrawing {
        let mut svg = drawing();
        for _ in 0..4094 {
            svg.rectangle(Rect::new(0., 0., 1., 1.).unwrap(), 0., Color::rgb(0, 0, 0))
                .unwrap();
        }
        svg
    }
    let mut svg = full();
    assert_eq!(
        svg.shaped_text(&font, "AVA", options, (5., 50.), Color::rgb(0, 0, 0)),
        Err(TextError::Capacity)
    );
    assert_eq!(svg.command_count(), 4094);
    assert_eq!(svg.finish(), full().finish());
    let mut empty = drawing();
    empty
        .shaped_text(&font, " ", options, (0., 0.), Color::rgb(0, 0, 0))
        .unwrap();
    assert_eq!(empty.command_count(), 0);
    assert_eq!(
        empty.shaped_text(&font, "A", options, (f64::NAN, 0.), Color::rgb(0, 0, 0)),
        Err(TextError::InvalidInput)
    );
}
