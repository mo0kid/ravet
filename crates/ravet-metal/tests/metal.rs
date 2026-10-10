#![cfg(target_os = "macos")]
use ravet_metal::{MetalError, MetalRectangle, MetalRenderer};
use ravet_surface::{drawing::Color, layout::Rect};
fn rectangle(x: f64, y: f64, w: f64, h: f64, color: Color) -> MetalRectangle {
    MetalRectangle {
        rectangle: Rect::new(x, y, w, h).unwrap(),
        color,
    }
}
fn oracle(width: u32, height: u32, background: Color, rectangles: &[MetalRectangle]) -> Vec<u8> {
    let alpha = f64::from(background.alpha) / 255.;
    let mut pixels = vec![0; width as usize * height as usize * 4];
    for y in 0..height {
        for x in 0..width {
            let mut color = [
                f64::from(background.red) * alpha,
                f64::from(background.green) * alpha,
                f64::from(background.blue) * alpha,
                f64::from(background.alpha),
            ];
            for paint in rectangles {
                let r = paint.rectangle;
                let left = r.x().clamp(0., f64::from(width)) as f32;
                let top = r.y().clamp(0., f64::from(height)) as f32;
                let right = (r.x() + r.width()).clamp(0., f64::from(width)) as f32;
                let bottom = (r.y() + r.height()).clamp(0., f64::from(height)) as f32;
                if f64::from(x) + 0.5 >= f64::from(left)
                    && f64::from(x) + 0.5 < f64::from(right)
                    && f64::from(y) + 0.5 >= f64::from(top)
                    && f64::from(y) + 0.5 < f64::from(bottom)
                {
                    let source = paint.color;
                    let alpha = f64::from(source.alpha) / 255.;
                    for (channel, value) in
                        color
                            .iter_mut()
                            .take(3)
                            .zip([source.red, source.green, source.blue])
                    {
                        *channel = (f64::from(value) * alpha + *channel * (1. - alpha)).round();
                    }
                    color[3] = (f64::from(source.alpha) + color[3] * (1. - alpha)).round();
                }
            }
            let offset = (y * width + x) as usize * 4;
            for i in 0..4 {
                pixels[offset + i] = color[i].round() as u8;
            }
        }
    }
    pixels
}
#[test]
fn actual_gpu_rectangles_match_independent_half_open_source_over_pixel_oracle() {
    let background = Color {
        red: 25,
        green: 100,
        blue: 200,
        alpha: 173,
    };
    let rectangles = [
        rectangle(-2., 1., 7., 4., Color::rgb(255, 0, 0)),
        rectangle(
            3.,
            2.,
            8.,
            5.,
            Color {
                red: 0,
                green: 255,
                blue: 40,
                alpha: 128,
            },
        ),
        rectangle(
            8.,
            0.,
            8.,
            3.,
            Color {
                red: 0,
                green: 0,
                blue: 255,
                alpha: 64,
            },
        ),
        rectangle(100., 100., 1., 1., Color::rgb(255, 255, 255)),
        rectangle(0., 0., 0., 2., Color::rgb(255, 255, 255)),
    ];
    let expected = oracle(13, 9, background, &rectangles);
    let mut renderer = MetalRenderer::new(13, 9, 8).unwrap();
    let image = renderer.render(background, &rectangles).unwrap();
    assert_eq!((image.width, image.height), (13, 9));
    assert_eq!(image.pixels.len(), expected.len());
    for (index, (&actual, &wanted)) in image.pixels.iter().zip(&expected).enumerate() {
        assert!(
            (i16::from(actual) - i16::from(wanted)).abs() <= 2,
            "component{index}: {actual} vs {wanted}"
        );
    }
    let copy = image.pixels.clone();
    let next = renderer.render(Color::rgb(0, 0, 0), &[]).unwrap();
    assert_eq!(image.pixels, copy);
    assert!(
        next.pixels
            .as_chunks::<4>()
            .0
            .iter()
            .all(|p| *p == [0, 0, 0, 255])
    );
}
#[test]
fn bounds_work_rejection_clipped_extremes_and_empty_capacity_preserve_reuse() {
    for dimensions in [(0, 1), (1, 0), (4097, 1), (1, u32::MAX)] {
        assert!(matches!(
            MetalRenderer::new(dimensions.0, dimensions.1, 1),
            Err(MetalError::InvalidInput)
        ))
    }
    assert!(matches!(
        MetalRenderer::new(1, 1, 4097),
        Err(MetalError::Capacity)
    ));
    let red = rectangle(0., 0., 1., 1., Color::rgb(255, 0, 0));
    let mut zero = MetalRenderer::new(2, 2, 0).unwrap();
    assert!(matches!(
        zero.render(Color::rgb(0, 0, 0), &[red]),
        Err(MetalError::Capacity)
    ));
    assert_eq!(
        zero.render(Color::rgb(255, 255, 255), &[]).unwrap().pixels,
        [255; 16]
    );
    let mut large = MetalRenderer::new(4096, 4096, 4).unwrap();
    let cover = rectangle(0., 0., 4096., 4096., Color::rgb(255, 0, 0));
    assert!(matches!(
        large.render(Color::rgb(0, 0, 0), &[cover; 4]),
        Err(MetalError::Capacity)
    ));
    drop(large);
    let mut renderer = MetalRenderer::new(2, 2, 2).unwrap();
    let outside = rectangle(f64::MAX / 2., 0., f64::MAX / 4., 1., Color::rgb(255, 0, 0));
    let tiny = rectangle(
        0.,
        0.,
        f64::MIN_POSITIVE,
        f64::MIN_POSITIVE,
        Color::rgb(255, 0, 0),
    );
    assert!(
        renderer
            .render(Color::rgb(0, 0, 255), &[outside, tiny])
            .unwrap()
            .pixels
            .as_chunks::<4>()
            .0
            .iter()
            .all(|p| *p == [0, 0, 255, 255])
    );
    assert!(
        renderer
            .render(Color::rgb(0, 0, 0), &[red])
            .unwrap()
            .pixels
            .as_chunks::<4>()
            .0
            .contains(&[255, 0, 0, 255])
    );
}
#[test]
fn worker_created_gpu_context_reuses_and_tears_down_explicitly() {
    std::thread::spawn(|| {
        for _ in 0..4 {
            let mut renderer = MetalRenderer::new(8, 8, 1).unwrap();
            for color in [
                Color::rgb(255, 0, 0),
                Color::rgb(0, 255, 0),
                Color::rgb(0, 0, 255),
            ] {
                let shape = rectangle(0., 0., 8., 4., color);
                let image = renderer.render(Color::rgb(0, 0, 0), &[shape]).unwrap();
                assert_eq!(
                    &image.pixels[..4],
                    &[color.red, color.green, color.blue, 255]
                );
                assert_eq!(&image.pixels[8 * 7 * 4..8 * 7 * 4 + 4], &[0, 0, 0, 255]);
            }
        }
    })
    .join()
    .unwrap();
}
