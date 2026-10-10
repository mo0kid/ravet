//! Synthetic wrapped local-font paragraphs with explicit opposite base directions.
use ravet_surface::{
    drawing::{SvgDrawing, Theme},
    layout::Rect,
    paragraph::{ParagraphOptions, TextAlignment},
    text::{MAXIMUM_FONT_BYTES, TextDirection, TextFont, TextOptions},
};
use std::io::Read;
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let path = std::env::args()
        .nth(1)
        .ok_or("usage: paragraph-outlines local-font-path")?;
    let mut bytes = Vec::new();
    std::fs::File::open(path)?
        .take((MAXIMUM_FONT_BYTES + 1) as u64)
        .read_to_end(&mut bytes)?;
    let font = TextFont::from_bytes(&bytes, 0)?;
    let mut svg = SvgDrawing::new(
        720.,
        420.,
        "Wrapped local-font paragraphs with explicit base directions",
    )
    .unwrap();
    svg.rectangle(
        Rect::new(0., 0., 720., 420.).unwrap(),
        0.,
        Theme::LIGHT.background,
    )
    .unwrap();
    let text = "Gain 12 — سلام 34 — Gain 56 — العَرَبِيَّة.\nLocal text keeps its original UTF8 clusters across wrapping.";
    for (direction, x, label) in [
        (TextDirection::LeftToRight, 32., "LTR base"),
        (TextDirection::RightToLeft, 368., "RTL base"),
    ] {
        let options = TextOptions {
            size: 24.,
            direction,
            script: None,
            language: None,
            features: &[],
        };
        svg.shaped_text(
            &font,
            label,
            TextOptions {
                direction: TextDirection::LeftToRight,
                ..options
            },
            (x, 48.),
            Theme::LIGHT.foreground,
        )?;
        let layout = svg.paragraph_text(
            &font,
            text,
            options,
            ParagraphOptions {
                width: 300.,
                line_height: 42.,
                alignment: TextAlignment::Left,
            },
            (x, 92.),
            Theme::LIGHT.foreground,
        )?;
        if layout.lines().iter().any(|line| line.overflows()) {
            return Err("synthetic fixture unexpectedly overflows".into());
        }
        eprintln!(
            "{direction:?}: {} lines, {} glyphs",
            layout.lines().len(),
            layout.glyph_count()
        );
    }
    println!("{}", svg.finish());
    Ok(())
}
