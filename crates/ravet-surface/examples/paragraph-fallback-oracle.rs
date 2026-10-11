//! Export prepared exact-face paragraph runs for an independent HarfBuzz oracle.
use ravet_surface::{
    fallback::TextFontStack,
    fallback_paragraph::*,
    paragraph::{ParagraphOptions, TextAlignment},
    text::{TextDirection, TextFont, TextVariation},
};
use std::fmt::Write;
fn json(s: &str) -> String {
    let mut r = String::from("\"");
    for c in s.chars() {
        match c {
            '"' => r.push_str("\\\""),
            '\\' => r.push_str("\\\\"),
            '\n' => r.push_str("\\n"),
            '\r' => r.push_str("\\r"),
            c if c.is_control() => write!(r, "\\u{:04x}", c as u32).unwrap(),
            c => r.push(c),
        }
    }
    r.push('"');
    r
}
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
    let heavy = TextFont::from_bytes(
        include_bytes!("../tests/fixtures/variation/RavetVariation.ttf"),
        0,
    )?
    .with_variations(&[TextVariation {
        tag: *b"wght",
        value: 900.,
    }])?;
    let files = [
        "crates/ravet-surface/tests/fixtures/fallback/RavetFallbackPrimary.ttf",
        "crates/ravet-surface/tests/fixtures/fallback/RavetFallbackComplement.ttf",
        "crates/ravet-surface/tests/fixtures/text/NotoSansArabic.ttf",
        "crates/ravet-surface/tests/fixtures/variation/RavetVariation.ttf",
    ];
    let fonts = [&primary, &complement, &arabic, &heavy];
    let stack = TextFontStack::new(&fonts)?;
    let cases = [
        ("wrap", "ab ab ab", 14.5, 10., false),
        ("mixed", "a ب c", 100., 10., false),
        ("mixed-rtl", "a ب c", 100., 10., true),
        ("arabic", "تبت", 100., 10., true),
        ("marks", "a\u{301} á ffi", 100., 10., false),
        ("soft-isolate", "a\u{2067}ب ب ب\u{2069}c", 12., 10., false),
        (
            "line-isolate",
            "\u{2067}ب \u{2028}a ب\u{2069} c",
            100.,
            10.,
            false,
        ),
        (
            "paragraph-isolate",
            "\u{2067}ب \u{2029}a ب\u{2069} c",
            100.,
            10.,
            false,
        ),
        (
            "hard-lines",
            "ab\r\n\na\rb\u{2028}a\u{2029}",
            100.,
            10.,
            false,
        ),
        ("ignorable", "\u{200d}\u{fe0f}", 100., 10., false),
        ("missing", "👩\u{200d}💻", 100., 10., false),
        ("selected-heavy", "a a", 85., 100., false),
        ("arabic-zwj", "ب\u{200d}", 100., 10., false),
        ("rtl-zwj-wrap", "abc\u{200d} def", 14.5, 10., true),
        (
            "flag-full",
            "ب 🏴\u{e0067}\u{e0062}\u{e0065}\u{e006e}\u{e0067}\u{e007f} ب",
            100.,
            10.,
            false,
        ),
        (
            "flag-candidate",
            "ب 🏴\u{e0067}\u{e0062}\u{e0065}\u{e006e}\u{e0067}\u{e007f} ب",
            14.5,
            10.,
            false,
        ),
        ("prepend-latin", "ب \u{0600}a\u{301} ب", 100., 10., false),
        ("prepend-latin-rtl", "ب \u{0600}a\u{301} ب", 100., 10., true),
        ("prepend-arabic", "a \u{0600}ب\u{301} a", 100., 10., false),
        (
            "prepend-arabic-rtl",
            "a \u{0600}ب\u{301} a",
            100.,
            10.,
            true,
        ),
        ("rtl-zwj-discarded", "a abc\u{200d} def", 14.5, 10., true),
        (
            "flag-discarded",
            "ب 🏴\u{e0067}\u{e0062}\u{e0065}\u{e006e}\u{e0067}\u{e007f} ب",
            10.,
            10.,
            false,
        ),
    ];
    println!("[ ");
    for (n, (name, text, width, size, rtl)) in cases.iter().enumerate() {
        if n > 0 {
            println!(",");
        }
        let scripts = if name.starts_with("mixed") {
            vec![
                ParagraphScriptSpan {
                    range: 0..2,
                    script: *b"Latn",
                },
                ParagraphScriptSpan {
                    range: 2..4,
                    script: *b"Arab",
                },
                ParagraphScriptSpan {
                    range: 4..6,
                    script: *b"Latn",
                },
            ]
        } else if name.starts_with("flag-") {
            vec![
                ParagraphScriptSpan {
                    range: 0..3,
                    script: *b"Arab",
                },
                ParagraphScriptSpan {
                    range: 3..32,
                    script: *b"Latn",
                },
                ParagraphScriptSpan {
                    range: 32..34,
                    script: *b"Arab",
                },
            ]
        } else {
            vec![ParagraphScriptSpan {
                range: 0..text.len(),
                script: if name.starts_with("arabic") {
                    *b"Arab"
                } else {
                    *b"Latn"
                },
            }]
        };
        let heavy_stack = TextFontStack::new(&[&heavy])?;
        let use_stack = if *name == "selected-heavy" {
            &heavy_stack
        } else {
            &stack
        };
        let l = FallbackParagraphLayout::new(
            use_stack,
            text,
            FallbackParagraphTextOptions {
                size: *size,
                base_direction: if *rtl {
                    TextDirection::RightToLeft
                } else {
                    TextDirection::LeftToRight
                },
                language: None,
                features: &[],
                scripts: &scripts,
            },
            ParagraphOptions {
                width: *width,
                line_height: *size * 1.2,
                alignment: TextAlignment::Left,
            },
        )?;
        print!(
            "{{\"name\":{},\"text\":{},\"size\":{},\"lines\":[",
            json(name),
            json(text),
            size
        );
        for (i, line) in l.lines().iter().enumerate() {
            if i > 0 {
                print!(",");
            }
            print!(
                "{{\"start\":{},\"end\":{},\"width\":{},\"runs\":[",
                line.range().start,
                line.range().end,
                line.width()
            );
            for (j, r) in line.runs().iter().enumerate() {
                if j > 0 {
                    print!(",");
                }
                let index = if *name == "selected-heavy" {
                    3
                } else {
                    r.font_index()
                };
                let script = std::str::from_utf8(&r.script()).unwrap().to_string();
                print!(
                    "{{\"font\":{},\"variation\":{},\"contextStart\":{},\"contextEnd\":{},\"start\":{},\"end\":{},\"direction\":{},\"script\":{},\"upem\":{},\"x\":{},\"glyphs\":[",
                    json(files[index]),
                    json(if index == 3 { "wght=900" } else { "" }),
                    r.context_range().start,
                    r.context_range().end,
                    r.range().start,
                    r.range().end,
                    json(if r.direction() == TextDirection::RightToLeft {
                        "rtl"
                    } else {
                        "ltr"
                    }),
                    json(&script),
                    r.font().units_per_em(),
                    r.x()
                );
                for (k, g) in r.shape().glyphs().iter().enumerate() {
                    if k > 0 {
                        print!(",");
                    }
                    print!(
                        "{{\"id\":{},\"cluster\":{},\"x\":{},\"y\":{},\"xa\":{},\"ya\":{}}}",
                        g.id,
                        r.source_cluster(k).unwrap(),
                        g.x,
                        g.y,
                        g.x_advance,
                        g.y_advance
                    );
                }
                print!("]}}");
            }
            print!("]}}");
        }
        print!("]}}");
    }
    println!("]");
    Ok(())
}
