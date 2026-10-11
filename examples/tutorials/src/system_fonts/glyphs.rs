use ravet_surface::{drawing::PathCommand, text::*};
use ravet_webview::fonts::*;
pub fn run() -> Result<(), Box<dyn std::error::Error>> {
    let catalog = SystemFontCatalog::discover()?;
    for family in ["Helvetica", "Arial", "Skia"] {
        for selection in catalog.catalog().styles(family) {
            let loaded = match catalog.load(selection) {
                Ok(f) => f,
                Err(e) => {
                    eprintln!(
                        "refused={} error={e:?}",
                        catalog.catalog().descriptor(selection)?.postscript_name()
                    );
                    continue;
                }
            };
            let base = TextFont::from_bytes(loaded.bytes(), 0)?;
            let settings: Vec<_> = loaded
                .variations()
                .iter()
                .map(|v| TextVariation {
                    tag: v.tag,
                    value: v.value,
                })
                .collect();
            let font = base.with_variations(&settings)?;
            for c in ['A', 'a', 'g', 'm', '0'] {
                let text = font.shape(
                    &c.to_string(),
                    TextOptions {
                        size: 1024.0,
                        direction: TextDirection::LeftToRight,
                        script: Some(*b"Latn"),
                        language: None,
                        features: &[],
                    },
                )?;
                if text.glyphs().len() != 1 || text.missing_glyphs() != 0 {
                    return Err("not one covered glyph".into());
                }
                let glyph = text.glyphs()[0].id;
                println!(
                    "glyph\t{}\t{}\t{}\t{}\t{:.17}",
                    loaded.descriptor().postscript_name(),
                    font.units_per_em(),
                    c as u32,
                    glyph,
                    text.advance().0 * f64::from(font.units_per_em()) / 1024.0
                );
                for cmd in font.outline_glyph(glyph)? {
                    match cmd {
                        PathCommand::MoveTo(x, y) => println!("M\t{x:.17}\t{y:.17}"),
                        PathCommand::LineTo(x, y) => println!("L\t{x:.17}\t{y:.17}"),
                        PathCommand::QuadraticTo {
                            control: (x, y),
                            end: (a, b),
                        } => println!("Q\t{x:.17}\t{y:.17}\t{a:.17}\t{b:.17}"),
                        PathCommand::CubicTo {
                            first: (x, y),
                            second: (a, b),
                            end: (c, d),
                        } => println!("C\t{x:.17}\t{y:.17}\t{a:.17}\t{b:.17}\t{c:.17}\t{d:.17}"),
                        PathCommand::Close => println!("Z"),
                    }
                }
                println!("end");
            }
        }
    }
    Ok(())
}
