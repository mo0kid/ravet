#![cfg(target_os = "macos")]
use ravet_metal::{MetalBitmap, MetalPaint, MetalRenderer};
use ravet_surface::{
    drawing::Color,
    image::{ImageFormat, decode_image},
    layout::Rect,
};

#[test]
fn explicit_decoded_png_adoption_preserves_premultiplied_pixels_on_metal() {
    let asset = decode_image(
        ImageFormat::Png,
        include_bytes!("../../ravet-surface/tests/fixtures/image/rgba.png"),
    )
    .unwrap();
    let (width, height) = (asset.width(), asset.height());
    let expected = asset.into_premultiplied_rgba();
    let bitmap = MetalBitmap::from_premultiplied_rgba(width, height, expected.clone()).unwrap();
    let mut renderer = MetalRenderer::new(width, height, 1).unwrap();
    let output = renderer
        .render_scene(
            Color {
                red: 0,
                green: 0,
                blue: 0,
                alpha: 0,
            },
            &[MetalPaint::Image {
                source: &bitmap,
                destination: Rect::new(0., 0., f64::from(width), f64::from(height)).unwrap(),
            }],
        )
        .unwrap();
    for (i, (actual, expected)) in output.pixels.iter().zip(expected).enumerate() {
        assert!(
            (i16::from(*actual) - i16::from(expected)).abs() <= 1,
            "component{i}: {actual} vs {expected}"
        );
    }
}
