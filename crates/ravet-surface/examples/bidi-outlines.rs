//! Controlled Arabic/Latin one-line composition from visual runs/local outlines.
use ravet_surface::{
    bidi::BidiLine,
    drawing::{SvgDrawing, Theme},
    layout::Rect,
    text::{MAXIMUM_FONT_BYTES, TextFont, TextOptions},
};
use std::io::Read;
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let path = std::env::args()
        .nth(1)
        .ok_or("usage: bidi-outlines local-font-path")?;
    let mut bytes = Vec::new();
    std::fs::File::open(path)?
        .take((MAXIMUM_FONT_BYTES + 1) as u64)
        .read_to_end(&mut bytes)?;
    let font = TextFont::from_bytes(&bytes, 0)?;
    let mut svg =
        SvgDrawing::new(640., 180., "Local-font mixed Arabic, Latin and numbers").unwrap();
    svg.rectangle(
        Rect::new(0., 0., 640., 180.).unwrap(),
        0.,
        Theme::LIGHT.background,
    )
    .unwrap();
    for (text, y) in [("Gain 12 — سلام 34", 68.), ("سلام 34 — Gain 12", 144.)] {
        let line = BidiLine::new(text, None)?;
        let mut x = 32.;
        for (index, run) in line.visual_runs().iter().enumerate() {
            let text = line.run_text(index).ok_or("missing visual run")?;
            let options = TextOptions {
                size: 36.,
                direction: run.direction,
                script: None,
                language: None,
                features: &[],
            };
            let shaped = font.shape(text, options)?;
            if shaped.missing_glyphs() != 0 {
                return Err("fixture font lacks run coverage".into());
            }
            svg.shaped_text(&font, text, options, (x, y), Theme::LIGHT.foreground)?;
            x += shaped.advance().0;
        }
    }
    println!("{}", svg.finish());
    Ok(())
}
