//! Explicit opaque baseline JPEG encoding on allocating UI/worker threads.
use super::{BoundedVec, ImageAsset, ImageError, MAX_ENCODED};
/// Explicit chroma sampling; backend quality presets never choose this policy.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum JpegSampling {
    S444,
    S422,
    S420,
}
/// Validated quality1..100 and sampling. Numeric RGB uses codec YCbCr conversion.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct JpegOptions {
    quality: u8,
    sampling: JpegSampling,
}
impl JpegOptions {
    /// Validate quality1..100 without clamping or choosing chroma policy.
    pub fn new(quality: u8, sampling: JpegSampling) -> Result<Self, ImageError> {
        if !(1..=100).contains(&quality) {
            return Err(ImageError::InvalidInput);
        }
        Ok(Self { quality, sampling })
    }
}
/// Encode an opaque validated asset into owned baseline8-bit JPEG.
/// Nonopaque pixels reject before encoding; consumers explicitly composite first.
/// Output budget1..16MiB; failure publishes no partial bytes or external I/O.
/// Quality is lossy, no perceptual/RSS/deadline guarantee. ICC/EXIF not emitted.
///
/// ```
/// use ravet_surface::image::{ImageAsset, ImageFormat, JpegOptions, JpegSampling,
///     decode_image, encode_jpeg};
/// let source = ImageAsset::from_straight_rgba(1, 1, vec![100, 150, 200, 255])?;
/// let encoded = encode_jpeg(&source, JpegOptions::new(90, JpegSampling::S444)?, 65536)?;
/// let decoded = decode_image(ImageFormat::Jpeg, &encoded)?;
/// assert_eq!((decoded.width(), decoded.height()), (1, 1));
/// # Ok::<(), ravet_surface::image::ImageError>(())
/// ```
pub fn encode_jpeg(
    asset: &ImageAsset,
    options: JpegOptions,
    maximum_bytes: usize,
) -> Result<Vec<u8>, ImageError> {
    if maximum_bytes == 0 || maximum_bytes > MAX_ENCODED {
        return Err(ImageError::Capacity);
    }
    if asset
        .pixels()
        .as_chunks::<4>()
        .0
        .iter()
        .any(|pixel| pixel[3] != 255)
    {
        return Err(ImageError::Unsupported);
    }
    let mut output = BoundedVec {
        bytes: Vec::new(),
        maximum: maximum_bytes,
        failed: false,
    };
    let mut encoder = jpeg_encoder::Encoder::new(&mut output, options.quality);
    encoder.set_sampling_factor(match options.sampling {
        JpegSampling::S444 => jpeg_encoder::SamplingFactor::F_1_1,
        JpegSampling::S422 => jpeg_encoder::SamplingFactor::F_2_1,
        JpegSampling::S420 => jpeg_encoder::SamplingFactor::F_2_2,
    });
    encoder.set_chroma_subsampling_method(jpeg_encoder::ChromaSubsamplingMethod::Average);
    encoder.set_progressive(false);
    encoder.set_optimized_huffman_tables(false);
    let result = encoder.encode(
        asset.pixels(),
        asset.width() as u16,
        asset.height() as u16,
        jpeg_encoder::ColorType::Rgba,
    );
    if result.is_err() {
        return Err(if output.failed {
            ImageError::Capacity
        } else {
            ImageError::Encode
        });
    }
    Ok(output.bytes)
}
