use ravet_surface::{
    drawing::{Color, PathCommand as P},
    raster::*,
    text::{TextDirection, TextFont, TextOptions},
};
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut c = RasterCanvas::new(480, 240, 32)?;
    c.clear(Color::rgb(23, 21, 29));
    let curve = RasterPath::new(
        &[
            P::MoveTo(24., 170.),
            P::CubicTo {
                first: (120., 40.),
                second: (220., 230.),
                end: (320., 110.),
            },
            P::QuadraticTo {
                control: (380., 20.),
                end: (454., 128.),
            },
        ],
        RasterTransform::default(),
    )?;
    c.stroke_path(
        &curve,
        Color::rgb(243, 182, 93),
        RasterStroke {
            width: 5.,
            cap: RasterLineCap::Round,
            join: RasterLineJoin::Round,
            ..Default::default()
        },
        true,
    )?;
    let mut hole = vec![
        P::MoveTo(340., 154.),
        P::LineTo(454., 154.),
        P::LineTo(454., 218.),
        P::LineTo(340., 218.),
        P::Close,
    ];
    hole.extend([
        P::MoveTo(366., 173.),
        P::LineTo(428., 173.),
        P::LineTo(428., 200.),
        P::LineTo(366., 200.),
        P::Close,
    ]);
    c.fill_path(
        &RasterPath::new(&hole, RasterTransform::default())?,
        Color::rgb(179, 167, 188),
        RasterFillRule::EvenOdd,
        true,
    )?;
    let font = TextFont::from_bytes(
        include_bytes!("../tests/fixtures/text/NotoSansArabic.ttf"),
        0,
    )?;
    for (source, direction, baseline) in [
        ("RAVET CPU paths", TextDirection::LeftToRight, (24., 48.)),
        ("مرحبا", TextDirection::RightToLeft, (24., 210.)),
    ] {
        let run = font.shape(
            source,
            TextOptions {
                size: 28.,
                direction,
                script: None,
                language: None,
                features: &[],
            },
        )?;
        c.draw_text(
            &RasterText::from_shaped(&font, &run, 28., baseline)?,
            Color::rgb(242, 236, 226),
            true,
        )?;
    }
    use std::io::Write;
    std::io::stdout().lock().write_all(&c.into_image().pixels)?;
    Ok(())
}
