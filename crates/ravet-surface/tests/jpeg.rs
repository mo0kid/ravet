#![cfg(feature = "jpeg-encode")]
use ravet_surface::image::{ImageAsset, ImageError, ImageFormat, decode_image};
use ravet_surface::image::{JpegOptions, JpegSampling, encode_jpeg};

fn asset(width: u32, height: u32) -> ImageAsset {
    let data = (0..height)
        .flat_map(|y| {
            (0..width).flat_map(move |x| {
                [
                    (x * 5 % 256) as u8,
                    (y * 9 % 256) as u8,
                    ((x * 3 + y * 4) % 256) as u8,
                    255,
                ]
            })
        })
        .collect();
    ImageAsset::from_straight_rgba(width, height, data).unwrap()
}
fn headers(bytes: &[u8]) -> ([u8; 15], Vec<u8>) {
    assert_eq!(&bytes[..2], [255, 216]);
    assert_eq!(&bytes[bytes.len() - 2..], [255, 217]);
    let mut at = 2;
    let mut frame = None;
    let mut quant = Vec::new();
    loop {
        assert_eq!(bytes[at], 255);
        let kind = bytes[at + 1];
        assert_ne!(kind, 194, "baseline only");
        let size = u16::from_be_bytes(bytes[at + 2..at + 4].try_into().unwrap()) as usize;
        let body = &bytes[at + 4..at + 2 + size];
        if kind == 192 {
            frame = Some(body.try_into().unwrap());
        }
        if kind == 219 {
            assert_eq!(body.len(), 65);
            assert_eq!(body[0] >> 4, 0);
            quant.extend_from_slice(&body[1..]);
        }
        if kind == 218 {
            break;
        }
        at += 2 + size;
    }
    (frame.unwrap(), quant)
}
#[test]
fn complete_baseline_quality_and_explicit_sampling_matrix() {
    for (width, height) in [(1, 1), (17, 9), (37, 23)] {
        let input = asset(width, height);
        let saved = input.clone();
        for quality in [1, 50, 90, 100] {
            for sampling in [JpegSampling::S444, JpegSampling::S422, JpegSampling::S420] {
                let options = JpegOptions::new(quality, sampling).unwrap();
                let encoded = encode_jpeg(&input, options, 65536).unwrap();
                assert_eq!(input, saved);
                assert_eq!(
                    encoded,
                    encode_jpeg(&input, options, encoded.len()).unwrap()
                );
                let (frame, quant) = headers(&encoded);
                assert_eq!(frame[0], 8);
                assert_eq!(
                    u16::from_be_bytes(frame[1..3].try_into().unwrap()),
                    height as u16
                );
                assert_eq!(
                    u16::from_be_bytes(frame[3..5].try_into().unwrap()),
                    width as u16
                );
                assert_eq!(frame[5], 3);
                assert_eq!(
                    frame[7],
                    match sampling {
                        JpegSampling::S444 => 0x11,
                        JpegSampling::S422 => 0x21,
                        JpegSampling::S420 => 0x22,
                    }
                );
                assert_eq!(frame[10], 0x11);
                assert_eq!(frame[13], 0x11);
                assert_eq!(quant.len(), 128);
                if quality == 100 {
                    assert!(quant.iter().all(|q| *q == 1));
                }
                let decoded = decode_image(ImageFormat::Jpeg, &encoded).unwrap();
                assert_eq!((decoded.width(), decoded.height()), (width, height));
                assert!(
                    decoded
                        .pixels()
                        .as_chunks::<4>()
                        .0
                        .iter()
                        .all(|p| p[3] == 255)
                );
                if quality == 100 && sampling == JpegSampling::S444 {
                    let maximum = decoded
                        .pixels()
                        .iter()
                        .zip(input.pixels())
                        .map(|(a, b)| u8::abs_diff(*a, *b))
                        .max()
                        .unwrap();
                    println!("original {width}x{height} q100 S444 maximum{maximum}");
                    assert!(maximum <= 8, "{width}x{height}: max{maximum}");
                }
                assert_eq!(
                    encode_jpeg(&input, options, encoded.len() - 1),
                    Err(ImageError::Capacity)
                );
            }
        }
    }
}
#[test]
fn nonopaque_pixels_quality_and_private_output_bounds_reject_explicitly() {
    for quality in [0, 101, 255] {
        assert_eq!(
            JpegOptions::new(quality, JpegSampling::S444),
            Err(ImageError::InvalidInput)
        );
    }
    let options = JpegOptions::new(90, JpegSampling::S420).unwrap();
    let input = asset(37, 23);
    for bound in [0, 1, 2, 32, 128, 16 * 1024 * 1024 + 1] {
        assert_eq!(
            encode_jpeg(&input, options, bound),
            Err(ImageError::Capacity)
        );
    }
    for alpha in [0, 128, 254] {
        let mut data = input.pixels().to_vec();
        *data.last_mut().unwrap() = alpha;
        let partial = ImageAsset::from_straight_rgba(37, 23, data).unwrap();
        let saved = partial.clone();
        assert_eq!(
            encode_jpeg(&partial, options, 65536),
            Err(ImageError::Unsupported)
        );
        assert_eq!(partial, saved);
    }
}
#[test]
#[ignore = "requires explicitly selected local Python/Pillow independent decoder"]
fn independent_pillow_completed_file_decoder_matrix() {
    let root = std::env::temp_dir().join(format!("ravet-w189-{}", std::process::id()));
    std::fs::create_dir(&root).unwrap();
    let mut maximum = 0;
    for (width, height) in [(1, 1), (17, 9), (37, 23)] {
        let input = asset(width, height);
        for quality in [1, 50, 90, 100] {
            for sampling in [JpegSampling::S444, JpegSampling::S422, JpegSampling::S420] {
                let encoded =
                    encode_jpeg(&input, JpegOptions::new(quality, sampling).unwrap(), 65536)
                        .unwrap();
                let file = root.join("profile.jpg");
                std::fs::write(&file, &encoded).unwrap();
                let output=std::process::Command::new("python3").arg("-c").arg("import sys; from PIL import Image; im=Image.open(sys.argv[1]); assert im.size==(int(sys.argv[2]),int(sys.argv[3])); assert not im.info.get('progressive'); sys.stdout.buffer.write(im.convert('RGBA').tobytes())").arg(&file).arg(width.to_string()).arg(height.to_string()).output().unwrap();
                assert!(output.status.success(), "{:?}", output.stderr);
                let decoded = decode_image(ImageFormat::Jpeg, &encoded).unwrap();
                assert_eq!(output.stdout.len(), decoded.pixels().len());
                let delta = decoded
                    .pixels()
                    .iter()
                    .zip(&output.stdout)
                    .map(|(a, b)| u8::abs_diff(*a, *b))
                    .max()
                    .unwrap();
                maximum = maximum.max(delta);
                println!("{width}x{height} q{quality} {sampling:?} independent max{delta}");
                assert!(
                    delta <= 4,
                    "independent codec/filter fixture difference{delta}"
                );
            }
        }
    }
    println!("36 completed JPEG profile independent maximum{maximum}");
    std::fs::remove_dir_all(root).unwrap();
}
