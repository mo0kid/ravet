#![allow(dead_code)]
#[path = "../../../src/color.rs"]
mod color;
#[path = "../../../src/drawing.rs"]
mod drawing;
#[path = "../../../src/layout.rs"]
mod layout;
use color::*;
fn main() {
    print!("{{\"scalars\":[");
    for c in 0..=255 {
        if c > 0 {
            print!(",");
        }
        let value = LinearRgba::from_srgba(drawing::Color::rgb(c, c, c)).components()[0];
        print!("[{c},{value:.17}]");
    }
    print!("],\"conversions\":[");
    let mut first_conversion = true;
    for value in 0..=255u8 {
        for source_profile in 0..4 {
            for target_profile in 0..4 {
                let enc = |profile| {
                    if profile & 1 == 0 {
                        ColorEncoding::Srgb
                    } else {
                        ColorEncoding::LinearSrgb
                    }
                };
                let mode = |profile| {
                    if profile & 2 == 0 {
                        AlphaMode::Straight
                    } else {
                        AlphaMode::Premultiplied
                    }
                };
                let mut pixel = [value, 255 - value, 128, value.wrapping_mul(19)];
                if mode(source_profile) == AlphaMode::Premultiplied {
                    let alpha = pixel[3];
                    for channel in &mut pixel[..3] {
                        *channel = (*channel).min(alpha);
                    }
                }
                let source =
                    RgbaView::new(1, 1, &pixel, enc(source_profile), mode(source_profile)).unwrap();
                let mut out = [0; 4];
                source
                    .convert_into(enc(target_profile), mode(target_profile), &mut out)
                    .unwrap();
                if !first_conversion {
                    print!(",");
                }
                first_conversion = false;
                print!("[{:?},{source_profile},{target_profile},{:?}]", pixel, out);
            }
        }
    }
    print!("],\"cases\":[");
    let mut seed = 0x5612a73u32;
    let mut first = true;
    for sample in 0..32 {
        let mut rgba = [[0u8; 4]; 2];
        for pixel in &mut rgba {
            for channel in pixel {
                seed = seed.wrapping_mul(1664525).wrapping_add(1013904223);
                *channel = (seed >> 24) as u8;
            }
        }
        if sample == 0 {
            rgba = [[255, 255, 255, 128], [0, 0, 0, 255]];
        }
        if sample == 1 {
            rgba = [[255, 70, 12, 0], [8, 90, 255, 0]];
        }
        for source_profile in 0..4 {
            for backdrop_profile in 0..4 {
                for target_profile in 0..4 {
                    let enc = |profile| {
                        if profile & 1 == 0 {
                            ColorEncoding::Srgb
                        } else {
                            ColorEncoding::LinearSrgb
                        }
                    };
                    let mode = |profile| {
                        if profile & 2 == 0 {
                            AlphaMode::Straight
                        } else {
                            AlphaMode::Premultiplied
                        }
                    };
                    let mut s = rgba[0];
                    let mut b = rgba[1];
                    if mode(source_profile) == AlphaMode::Premultiplied {
                        for c in &mut s[..3] {
                            *c = (*c).min(rgba[0][3]);
                        }
                    }
                    if mode(backdrop_profile) == AlphaMode::Premultiplied {
                        for c in &mut b[..3] {
                            *c = (*c).min(rgba[1][3]);
                        }
                    }
                    let source =
                        RgbaView::new(1, 1, &s, enc(source_profile), mode(source_profile)).unwrap();
                    let background =
                        RgbaView::new(1, 1, &b, enc(backdrop_profile), mode(backdrop_profile))
                            .unwrap();
                    let mut out = [0; 4];
                    composite_source_over(
                        source,
                        background,
                        enc(target_profile),
                        mode(target_profile),
                        &mut out,
                    )
                    .unwrap();
                    if !first {
                        print!(",");
                    }
                    first = false;
                    print!(
                        "[{:?},{:?},{source_profile},{backdrop_profile},{target_profile},{:?}]",
                        s, b, out
                    );
                }
            }
        }
    }
    println!("]}}");
}
