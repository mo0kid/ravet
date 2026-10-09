//! Local-font outline-only SVG; no system font fallback or embedded font URL.
use ravet_surface::{
    drawing::{SvgDrawing, Theme},
    layout::Rect,
    text::{MAXIMUM_FONT_BYTES, TextDirection, TextFont, TextOptions},
};
use std::io::Read;
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let path = std::env::args()
        .nth(1)
        .ok_or("usage: font-outlines local-font-path")?;
    let mut bytes = Vec::new();
    std::fs::File::open(path)?
        .take((MAXIMUM_FONT_BYTES + 1) as u64)
        .read_to_end(&mut bytes)?;
    let font = TextFont::from_bytes(&bytes, 0)?;
    let mut svg = SvgDrawing::new(
        640.,
        240.,
        "Local font outlines: Gain 12, Arabic greeting and marked Arabic text",
    )
    .unwrap();
    svg.rectangle(
        Rect::new(0., 0., 640., 240.).unwrap(),
        0.,
        Theme::LIGHT.background,
    )
    .unwrap();
    for (text, direction, script, language, baseline) in [
        (
            "Gain 12",
            TextDirection::LeftToRight,
            *b"Latn",
            "en",
            (32., 64.),
        ),
        (
            "سلام",
            TextDirection::RightToLeft,
            *b"Arab",
            "ar",
            (32., 140.),
        ),
        (
            "العَرَبِيَّة",
            TextDirection::RightToLeft,
            *b"Arab",
            "ar",
            (32., 212.),
        ),
    ] {
        svg.shaped_text(
            &font,
            text,
            TextOptions {
                size: 48.,
                direction,
                script: Some(script),
                language: Some(language),
                features: &[],
            },
            baseline,
            Theme::LIGHT.foreground,
        )?;
    }
    println!("{}", svg.finish());
    Ok(())
}
