#![cfg(target_os = "macos")]
use ravet_metal::{MetalBitmap, MetalError, MetalPaint, MetalRectangle, MetalRenderer};
use ravet_surface::{drawing::Color, layout::Rect};
fn bounds(x: f64, y: f64, w: f64, h: f64) -> Rect {
    Rect::new(x, y, w, h).unwrap()
}
fn oracle(width: u32, height: u32, background: Color, paints: &[MetalPaint<'_>]) -> Vec<u8> {
    let mut pixels = vec![0; width as usize * height as usize * 4];
    for y in 0..height {
        for x in 0..width {
            let a = f64::from(background.alpha) / 255.;
            let mut out = [
                f64::from(background.red) * a,
                f64::from(background.green) * a,
                f64::from(background.blue) * a,
                f64::from(background.alpha),
            ];
            for paint in paints {
                let destination = match paint {
                    MetalPaint::Rectangle(p) => p.rectangle,
                    MetalPaint::Image { destination, .. } => *destination,
                };
                let left = destination.x().clamp(0., f64::from(width)) as f32;
                let top = destination.y().clamp(0., f64::from(height)) as f32;
                let right =
                    (destination.x() + destination.width()).clamp(0., f64::from(width)) as f32;
                let bottom =
                    (destination.y() + destination.height()).clamp(0., f64::from(height)) as f32;
                let px = f64::from(x) + 0.5;
                let py = f64::from(y) + 0.5;
                if px < f64::from(left)
                    || px >= f64::from(right)
                    || py < f64::from(top)
                    || py >= f64::from(bottom)
                {
                    continue;
                }
                let src = match paint {
                    MetalPaint::Rectangle(p) => {
                        let c = p.color;
                        let a = f64::from(c.alpha) / 255.;
                        [
                            f64::from(c.red) * a,
                            f64::from(c.green) * a,
                            f64::from(c.blue) * a,
                            f64::from(c.alpha),
                        ]
                    }
                    MetalPaint::Image { source, .. } => {
                        let u0 = ((f64::from(left) - destination.x()) / destination.width())
                            .clamp(0., 1.) as f32;
                        let u1 = ((f64::from(right) - destination.x()) / destination.width())
                            .clamp(0., 1.) as f32;
                        let v0 = ((f64::from(top) - destination.y()) / destination.height())
                            .clamp(0., 1.) as f32;
                        let v1 = ((f64::from(bottom) - destination.y()) / destination.height())
                            .clamp(0., 1.) as f32;
                        let u = f64::from(u0)
                            + (f64::from(u1) - f64::from(u0)) * (px - f64::from(left))
                                / (f64::from(right) - f64::from(left));
                        let v = f64::from(v0)
                            + (f64::from(v1) - f64::from(v0)) * (py - f64::from(top))
                                / (f64::from(bottom) - f64::from(top));
                        let sx = (u * f64::from(source.width()))
                            .floor()
                            .clamp(0., f64::from(source.width() - 1))
                            as usize;
                        let sy = (v * f64::from(source.height()))
                            .floor()
                            .clamp(0., f64::from(source.height() - 1))
                            as usize;
                        let i = (sy * source.width() as usize + sx) * 4;
                        std::array::from_fn(|c| f64::from(source.pixels()[i + c]))
                    }
                };
                let a = src[3] / 255.;
                for i in 0..4 {
                    out[i] = (src[i] + out[i] * (1. - a)).round();
                }
            }
            let i = (y * width + x) as usize * 4;
            for c in 0..4 {
                pixels[i + c] = out[c].round() as u8;
            }
        }
    }
    pixels
}
#[test]
fn actual_gpu_mixed_images_clip_scale_blend_in_order_and_keep_owned_frames() {
    let image = MetalBitmap::from_premultiplied_rgba(
        3,
        2,
        vec![
            255, 0, 0, 255, 0, 128, 0, 128, 0, 0, 255, 255, 10, 20, 30, 40, 255, 255, 255, 255, 0,
            0, 0, 0,
        ],
    )
    .unwrap();
    let paints = [
        MetalPaint::Image {
            destination: bounds(-2., 1., 9., 6.),
            source: &image,
        },
        MetalPaint::Rectangle(MetalRectangle {
            rectangle: bounds(3., 2., 7., 5.),
            color: Color {
                red: 255,
                green: 0,
                blue: 80,
                alpha: 64,
            },
        }),
        MetalPaint::Image {
            destination: bounds(6., -1., 6., 4.),
            source: &image,
        },
    ];
    let bg = Color {
        red: 15,
        green: 60,
        blue: 120,
        alpha: 170,
    };
    let expected = oracle(13, 9, bg, &paints);
    let mut renderer = MetalRenderer::new(13, 9, 4).unwrap();
    let result = renderer.render_scene(bg, &paints).unwrap();
    for (i, (&a, &b)) in result.pixels.iter().zip(&expected).enumerate() {
        assert!(
            (i16::from(a) - i16::from(b)).abs() <= 2,
            "component{i}: {a} vs {b}"
        );
    }
    let prior = result.pixels.clone();
    let bitmap = result.into_bitmap().unwrap();
    let copy = renderer
        .render_scene(
            Color::rgb(0, 0, 0),
            &[MetalPaint::Image {
                destination: bounds(0., 0., 13., 9.),
                source: &bitmap,
            }],
        )
        .unwrap();
    // Source-over translucent readback uses its retained alpha; black is opaque.
    for (i, p) in copy.pixels.as_chunks::<4>().0.iter().enumerate() {
        assert_eq!(&p[..3], &prior[i * 4..i * 4 + 3]);
        assert_eq!(p[3], 255);
    }
    let _next = renderer.render(Color::rgb(0, 255, 0), &[]).unwrap();
    assert_eq!(bitmap.pixels(), prior);
}
#[test]
fn immutable_bitmap_and_scene_limits_reject_without_losing_renderer_reuse() {
    for (w, h, p) in [
        (0, 1, vec![]),
        (4097, 1, vec![]),
        (1, 1, vec![0; 3]),
        (1, 1, vec![1, 0, 0, 0]),
    ] {
        assert!(matches!(
            MetalBitmap::from_premultiplied_rgba(w, h, p),
            Err(MetalError::InvalidInput)
        ));
    }
    let assets: Vec<_> = (0..17)
        .map(|_| MetalBitmap::from_premultiplied_rgba(1, 1, vec![255; 4]).unwrap())
        .collect();
    let paints: Vec<_> = assets
        .iter()
        .map(|source| MetalPaint::Image {
            destination: bounds(100., 100., 1., 1.),
            source,
        })
        .collect();
    let mut renderer = MetalRenderer::new(2, 2, 20).unwrap();
    assert!(
        renderer
            .render_scene(Color::rgb(0, 0, 0), &paints[..16])
            .is_ok()
    );
    assert!(matches!(
        renderer.render_scene(Color::rgb(0, 0, 0), &paints),
        Err(MetalError::Capacity)
    ));
    let repeats = vec![paints[0]; 20];
    assert!(renderer.render_scene(Color::rgb(0, 0, 0), &repeats).is_ok());
    assert!(matches!(
        renderer.render_scene(Color::rgb(0, 0, 0), &vec![paints[0]; 21]),
        Err(MetalError::Capacity)
    ));
    let large = MetalBitmap::from_premultiplied_rgba(4096, 4096, vec![0; 67_108_864]).unwrap();
    let budget = [
        MetalPaint::Image {
            destination: bounds(100., 100., 1., 1.),
            source: &large,
        },
        paints[0],
    ];
    assert!(matches!(
        renderer.render_scene(Color::rgb(0, 0, 0), &budget),
        Err(MetalError::Capacity)
    ));
    let good = renderer
        .render_scene(
            Color::rgb(0, 0, 255),
            &[MetalPaint::Image {
                destination: bounds(0., 0., 1., 1.),
                source: &assets[0],
            }],
        )
        .unwrap();
    assert_eq!(&good.pixels[..4], &[255; 4]);
    assert_eq!(&good.pixels[12..16], &[0, 0, 255, 255]);
}
#[test]
fn explicit_local_video_frames_adopt_and_upload_without_implicit_reader_or_color_transform() {
    use ravet_media::VideoReader;
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../ravet-media/tests/fixtures/red-blue.mp4");
    let mut reader = VideoReader::open(&path).unwrap();
    let mut renderer = MetalRenderer::new(128, 96, 1).unwrap();
    for time in [0., 1.] {
        let frame = reader.frame_at(time).unwrap();
        let asset =
            MetalBitmap::from_premultiplied_rgba(frame.info.width, frame.info.height, frame.pixels)
                .unwrap();
        let image = renderer
            .render_scene(
                Color::rgb(0, 0, 0),
                &[MetalPaint::Image {
                    destination: bounds(0., 0., 128., 96.),
                    source: &asset,
                }],
            )
            .unwrap();
        for y in 0..96usize {
            for x in 0..128usize {
                let s = ((y / 2) * 64 + x / 2) * 4;
                let d = (y * 128 + x) * 4;
                assert_eq!(&image.pixels[d..d + 4], &asset.pixels()[s..s + 4]);
            }
        }
    }
}
