use ravet_surface::{drawing::PathCommand, text::*};
fn main() -> Result<(), Box<dyn std::error::Error>> {
    for file in std::env::args().skip(1) {
        let base = TextFont::from_bytes(&std::fs::read(&file)?, 0)?;
        let users: &[f32] = if base.variation_axes()?.is_empty() {
            &[0.]
        } else {
            &[100., 400., 650., 900.]
        };
        for user in users {
            let font = if users.len() == 1 {
                base.clone()
            } else {
                base.with_variations(&[TextVariation {
                    tag: *b"wght",
                    value: *user,
                }])?
            };
            let shaped = font.shape(
                "A",
                TextOptions {
                    size: 1000.,
                    direction: TextDirection::LeftToRight,
                    script: Some(*b"Latn"),
                    language: None,
                    features: &[],
                },
            )?;
            if shaped.glyphs().len() != 1 || shaped.missing_glyphs() != 0 {
                return Err("unexpected shaped glyph".into());
            }
            let id = shaped.glyphs()[0].id;
            let normalized = font
                .variation_coordinates()?
                .first()
                .map_or(0., |c| c.normalized * 16384.);
            println!(
                "glyph\tRUST\t{file}\t{user}\t{id}\t{:.17}\t{normalized}",
                shaped.advance().0 * f64::from(font.units_per_em()) / 1000.
            );
            for cmd in font.outline_glyph(id)? {
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
    Ok(())
}
