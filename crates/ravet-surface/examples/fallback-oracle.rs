use ravet_surface::{
    fallback::TextFontStack,
    text::{TextDirection, TextFont, TextOptions},
};
use std::fmt::Write;
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let primary = TextFont::from_bytes(
        include_bytes!("../tests/fixtures/fallback/RavetFallbackPrimary.ttf"),
        0,
    )?;
    let complement = TextFont::from_bytes(
        include_bytes!("../tests/fixtures/fallback/RavetFallbackComplement.ttf"),
        0,
    )?;
    let arabic = TextFont::from_bytes(
        include_bytes!("../tests/fixtures/text/NotoSansArabic.ttf"),
        0,
    )?;
    let cases = [
        ("merged", "abcffi", *b"Latn", false, false),
        ("order", "abcffi", *b"Latn", false, true),
        ("nfc", "á", *b"Latn", false, false),
        ("nfd", "a\u{301}", *b"Latn", false, false),
        ("ignorables", "\u{200d}\u{fe0f}", *b"Latn", false, false),
        ("missing", "👩\u{200d}💻", *b"Latn", false, false),
        ("rtl", "abca", *b"Latn", true, false),
        ("arabic-context", "تبت", *b"Arab", true, false),
        ("arabic-marks", "تُبَت", *b"Arab", true, false),
        ("source-joiner", "ت\u{200d}بت", *b"Arab", true, false),
    ];
    let mut result = String::from("[");
    for (case, (name, source, script, rtl, reverse)) in cases.iter().enumerate() {
        let faces = if *script == *b"Arab" {
            vec![&primary, &arabic]
        } else if *reverse {
            vec![&complement, &primary]
        } else {
            vec![&primary, &complement]
        };
        let files = if *script == *b"Arab" {
            vec![
                "fallback/RavetFallbackPrimary.ttf",
                "text/NotoSansArabic.ttf",
            ]
        } else if *reverse {
            vec![
                "fallback/RavetFallbackComplement.ttf",
                "fallback/RavetFallbackPrimary.ttf",
            ]
        } else {
            vec![
                "fallback/RavetFallbackPrimary.ttf",
                "fallback/RavetFallbackComplement.ttf",
            ]
        };
        let stack = TextFontStack::new(&faces)?;
        let direction = if *rtl {
            TextDirection::RightToLeft
        } else {
            TextDirection::LeftToRight
        };
        let language = if *script == *b"Arab" { "ar" } else { "en" };
        let shape = stack.shape(
            source,
            TextOptions {
                size: 24.,
                direction,
                script: Some(*script),
                language: Some(language),
                features: &[],
            },
        )?;
        if case > 0 {
            result.push(',');
        }
        let hex = source
            .as_bytes()
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect::<String>();
        write!(
            result,
            "{{\"name\":\"{name}\",\"source\":\"{hex}\",\"script\":\"{}\",\"direction\":\"{}\",\"language\":\"{language}\",\"size\":24,\"advance\":[{},{}],\"runs\":[",
            std::str::from_utf8(script)?,
            if *rtl { "rtl" } else { "ltr" },
            shape.advance().0,
            shape.advance().1
        )?;
        for (index, run) in shape.runs().iter().enumerate() {
            if index > 0 {
                result.push(',');
            }
            write!(
                result,
                "{{\"font\":\"{}\",\"range\":[{},{}],\"origin\":[{},{}],\"glyphs\":[",
                files[run.font_index()],
                run.range().start,
                run.range().end,
                run.origin().0,
                run.origin().1
            )?;
            for (i, glyph) in run.shape().glyphs().iter().enumerate() {
                if i > 0 {
                    result.push(',');
                }
                write!(
                    result,
                    "{{\"id\":{},\"cluster\":{},\"x\":{},\"y\":{},\"xa\":{},\"ya\":{}}}",
                    glyph.id,
                    run.source_cluster(i).unwrap(),
                    glyph.x,
                    glyph.y,
                    glyph.x_advance,
                    glyph.y_advance
                )?;
            }
            result.push_str("]}");
        }
        result.push_str("]}");
    }
    result.push(']');
    println!("{result}");
    Ok(())
}
