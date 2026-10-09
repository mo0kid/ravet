//! Numeric local-font run fixture for independent shaper comparison.
use ravet_surface::text::{TextDirection, TextFeature, TextFont, TextOptions};
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = std::env::args().skip(1).collect();
    if !(3..=4).contains(&args.len()) {
        return Err("usage: shape-text font-path text ltr|rtl [kern=0]".into());
    }
    let direction = match args[2].as_str() {
        "ltr" => TextDirection::LeftToRight,
        "rtl" => TextDirection::RightToLeft,
        _ => return Err("direction must be ltr or rtl".into()),
    };
    use std::io::Read;
    let mut data = Vec::new();
    std::fs::File::open(&args[0])?
        .take((ravet_surface::text::MAXIMUM_FONT_BYTES + 1) as u64)
        .read_to_end(&mut data)?;
    let font = TextFont::from_bytes(&data, 0)?;
    let (script, language) = if direction == TextDirection::RightToLeft {
        (*b"Arab", "ar")
    } else {
        (*b"Latn", "en")
    };
    let overrides = [TextFeature {
        tag: *b"kern",
        enabled: false,
    }];
    let features = match args.get(3).map(String::as_str) {
        None => &[][..],
        Some("kern=0") => &overrides[..],
        _ => return Err("only kern=0 override is accepted by this fixture".into()),
    };
    let run = font.shape(
        &args[1],
        TextOptions {
            size: 24.,
            direction,
            script: Some(script),
            language: Some(language),
            features,
        },
    )?;
    println!(
        "{} {} {}",
        font.units_per_em(),
        run.advance().0,
        run.advance().1
    );
    for glyph in run.glyphs() {
        println!(
            "{} {} {} {} {} {}",
            glyph.id, glyph.cluster, glyph.x, glyph.y, glyph.x_advance, glyph.y_advance
        );
    }
    Ok(())
}
