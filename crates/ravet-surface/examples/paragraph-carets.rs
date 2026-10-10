//! Source-aware bidi affinity and synthetic ligature selection pen geometry.
use ravet_surface::{
    caret::{CaretAffinity, CaretMap, SelectionRect, TextPosition},
    drawing::{Color, SvgDrawing, Theme},
    layout::Rect,
    paragraph::{ParagraphLayout, ParagraphOptions, TextAlignment},
    text::{MAXIMUM_FONT_BYTES, TextDirection, TextFont, TextOptions},
};
use std::io::Read;
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let path = std::env::args()
        .nth(1)
        .ok_or("usage: paragraph-carets local-font-path")?;
    let mut bytes = Vec::new();
    std::fs::File::open(path)?
        .take((MAXIMUM_FONT_BYTES + 1) as u64)
        .read_to_end(&mut bytes)?;
    let font = TextFont::from_bytes(&bytes, 0)?;
    let ligature = TextFont::from_bytes(
        include_bytes!("../tests/fixtures/caret/RavetSyntheticLigature.ttf"),
        0,
    )?;
    let mut svg = SvgDrawing::new(
        720.,
        320.,
        "Bidi affinity and synthetic ligature source geometry",
    )
    .unwrap();
    svg.rectangle(
        Rect::new(0., 0., 720., 320.).unwrap(),
        0.,
        Theme::LIGHT.background,
    )
    .map_err(|error| format!("drawing rejected: {error:?}"))?;
    let label_style = TextOptions {
        size: 24.,
        direction: TextDirection::LeftToRight,
        script: None,
        language: None,
        features: &[],
    };
    for (source, selected, baseline, size, selected_font, label) in [
        (
            "AV سلام",
            0..5,
            100.,
            40.,
            &font,
            "Mixed bidi: one byte, two affinities",
        ),
        (
            "ffi",
            1..2,
            255.,
            72.,
            &ligature,
            "Synthetic ffi: middle grapheme selected",
        ),
    ] {
        svg.shaped_text(
            &font,
            label,
            label_style,
            (32., baseline - 65.),
            Theme::LIGHT.foreground,
        )?;
        let style = TextOptions {
            size,
            ..label_style
        };
        let options = ParagraphOptions {
            width: 640.,
            line_height: 90.,
            alignment: TextAlignment::Left,
        };
        let layout = ParagraphLayout::new(selected_font, source, style, options)?;
        let map = CaretMap::new(&layout)?;
        let mut boxes = [SelectionRect::empty(); 8];
        let count = map.selection(selected, &mut boxes)?;
        for selection in &boxes[..count] {
            let bounds = selection.rectangle;
            svg.rectangle(
                Rect::new(
                    32. + bounds.x(),
                    baseline + bounds.y(),
                    bounds.width(),
                    bounds.height(),
                )
                .unwrap(),
                0.,
                Color::rgb(218, 228, 255),
            )
            .map_err(|error| format!("drawing rejected: {error:?}"))?;
        }
        svg.paragraph_text(
            selected_font,
            source,
            style,
            options,
            (32., baseline),
            Theme::LIGHT.foreground,
        )?;
        let positions: Vec<_> = if source == "ffi" {
            (0..=3)
                .map(|byte_offset| TextPosition {
                    byte_offset,
                    affinity: CaretAffinity::Downstream,
                })
                .collect()
        } else {
            [CaretAffinity::Upstream, CaretAffinity::Downstream]
                .into_iter()
                .map(|affinity| TextPosition {
                    byte_offset: 3,
                    affinity,
                })
                .collect()
        };
        for (index, position) in positions.into_iter().enumerate() {
            let caret = map.caret(position).ok_or("fixture position rejected")?;
            let color = if index % 2 == 0 {
                Color::rgb(208, 48, 64)
            } else {
                Color::rgb(24, 96, 208)
            };
            let x = 32. + caret.point.0;
            svg.polyline(&[(x, baseline - 45.), (x, baseline + 45.)], 2., color)
                .map_err(|error| format!("drawing rejected: {error:?}"))?;
            eprintln!(
                "{source:?}: byte{} {:?} X{}",
                position.byte_offset, position.affinity, caret.point.0
            );
        }
        eprintln!(
            "{source:?}: {count} selection boxes, {} glyphs",
            layout.glyph_count()
        );
    }
    println!("{}", svg.finish());
    Ok(())
}
