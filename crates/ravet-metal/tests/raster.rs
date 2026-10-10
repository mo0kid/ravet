#![cfg(target_os = "macos")]
use ravet_metal::{MetalBitmap, MetalPaint, MetalRenderer};
use ravet_surface::{
    drawing::{Color, PathCommand as P},
    layout::Rect,
    raster::{RasterCanvas, RasterFillRule, RasterPath, RasterText, RasterTransform},
    text::{TextDirection, TextFont, TextOptions},
};
#[test]
fn explicit_cpu_path_and_local_font_bitmap_preserves_premultiplied_pixels_on_metal() {
    let mut canvas = RasterCanvas::new(128, 64, 4).unwrap();
    let curve = RasterPath::new(
        &[
            P::MoveTo(1.25, 5.5),
            P::QuadraticTo {
                control: (20., 60.),
                end: (70., 2.),
            },
            P::CubicTo {
                first: (100., 10.),
                second: (20., 45.),
                end: (2., 40.),
            },
            P::Close,
        ],
        RasterTransform::default(),
    )
    .unwrap();
    canvas
        .fill_path(
            &curve,
            Color {
                red: 220,
                green: 100,
                blue: 50,
                alpha: 140,
            },
            RasterFillRule::NonZero,
            true,
        )
        .unwrap();
    let font = TextFont::from_bytes(
        include_bytes!("../../ravet-surface/tests/fixtures/text/NotoSansArabic.ttf"),
        0,
    )
    .unwrap();
    let run = font
        .shape(
            "مرحبا",
            TextOptions {
                size: 24.,
                direction: TextDirection::RightToLeft,
                script: None,
                language: None,
                features: &[],
            },
        )
        .unwrap();
    canvas
        .draw_text(
            &RasterText::from_shaped(&font, &run, 24., (8., 50.)).unwrap(),
            Color::rgb(80, 180, 240),
            true,
        )
        .unwrap();
    let source = canvas.into_image();
    assert!(
        source
            .pixels
            .as_chunks::<4>()
            .0
            .iter()
            .any(|p| p[3] > 0 && p[3] < 255)
    );
    let expected = source.pixels.clone();
    let bitmap =
        MetalBitmap::from_premultiplied_rgba(source.width, source.height, source.pixels).unwrap();
    let mut renderer = MetalRenderer::new(128, 64, 1).unwrap();
    let output = renderer
        .render_scene(
            Color {
                red: 0,
                green: 0,
                blue: 0,
                alpha: 0,
            },
            &[MetalPaint::Image {
                destination: Rect::new(0., 0., 128., 64.).unwrap(),
                source: &bitmap,
            }],
        )
        .unwrap();
    for (i, (a, b)) in output.pixels.iter().zip(expected).enumerate() {
        assert!(
            (i16::from(*a) - i16::from(b)).abs() <= 1,
            "component{i}: {a} vs {b}"
        );
    }
}
