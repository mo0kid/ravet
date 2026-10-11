//! Optional owned PNG/JPEG worker assets with explicit codec/input/output limits.
//!
//! Enable the default-disabled `image` feature. Numeric straight RGBA8 remains
//! independent of color management, EXIF orientation, windows and presentation.
//! Source bytes are at most 16 MiB; dimensions are 1..4096 and output at most
//! 64 MiB. Parsing uses at most 8192 PNG chunks/JPEG markers and 32 JPEG scans.
//! Backend working memory/time and allocator failure remain separate obligations.
//!
//! ```
//! use ravet_surface::image::{ImageAsset, ImageFormat, decode_image, encode_png};
//! let asset = ImageAsset::from_straight_rgba(2, 1, vec![200, 100, 50, 128, 1, 2, 3, 0])?;
//! let png = encode_png(&asset, 65536)?;
//! assert_eq!(decode_image(ImageFormat::Png, &png)?, asset);
//! assert_eq!(asset.into_premultiplied_rgba(), vec![100, 50, 25, 128, 0, 0, 0, 0]);
//! # Ok::<(), ravet_surface::image::ImageError>(())
//! ```
use std::io::{Cursor, Write};
const MAX_ENCODED: usize = 16 * 1024 * 1024;
const MAX_PIXELS: usize = 64 * 1024 * 1024;
const MAX_MARKERS: usize = 8192;
/// Input/codec/output rejection; decoder/encoder backend errors are scoped here.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ImageError {
    InvalidInput,
    Capacity,
    Unsupported,
    Decode,
    Encode,
}
impl std::fmt::Display for ImageError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{self:?}")
    }
}
impl std::error::Error for ImageError {}
/// Caller-selected format; no file, network or format-sniffing policy is inferred.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ImageFormat {
    Png,
    Jpeg,
}
/// Immutable owned straight numeric RGBA8; ICC, gamma and orientation not applied.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ImageAsset {
    width: u32,
    height: u32,
    pixels: Vec<u8>,
}
fn length(width: u32, height: u32) -> Result<usize, ImageError> {
    if width == 0 || height == 0 {
        return Err(ImageError::InvalidInput);
    }
    if width > 4096 || height > 4096 {
        return Err(ImageError::Capacity);
    }
    let n = (width as usize)
        .checked_mul(height as usize)
        .and_then(|n| n.checked_mul(4))
        .ok_or(ImageError::Capacity)?;
    if n > MAX_PIXELS {
        return Err(ImageError::Capacity);
    }
    Ok(n)
}
impl ImageAsset {
    /// Adopt exact packed top-to-bottom straight RGBA8, dimensions 1..4096/64 MiB.
    /// Shape rejection consumes the supplied allocation without changing other assets.
    pub fn from_straight_rgba(
        width: u32,
        height: u32,
        pixels: Vec<u8>,
    ) -> Result<Self, ImageError> {
        if pixels.len() != length(width, height)? {
            return Err(ImageError::InvalidInput);
        }
        Ok(Self {
            width,
            height,
            pixels,
        })
    }
    pub fn width(&self) -> u32 {
        self.width
    }
    pub fn height(&self) -> u32 {
        self.height
    }
    pub fn pixels(&self) -> &[u8] {
        &self.pixels
    }
    /// Transfer the straight pixel allocation without copying.
    pub fn into_straight_rgba(self) -> Vec<u8> {
        self.pixels
    }
    /// Consume pixels and multiply RGB by alpha with integer nearest rounding.
    pub fn into_premultiplied_rgba(mut self) -> Vec<u8> {
        for p in self.pixels.as_chunks_mut::<4>().0 {
            let alpha = u16::from(p[3]);
            for c in &mut p[..3] {
                *c = ((u16::from(*c) * alpha + 127) / 255) as u8;
            }
        }
        self.pixels
    }
}
fn source(bytes: &[u8]) -> Result<(), ImageError> {
    if bytes.is_empty() {
        Err(ImageError::InvalidInput)
    } else if bytes.len() > MAX_ENCODED {
        Err(ImageError::Capacity)
    } else {
        Ok(())
    }
}
fn png_structure(bytes: &[u8]) -> Result<(u32, u32), ImageError> {
    if bytes.get(..8) != Some(b"\x89PNG\r\n\x1a\n") {
        return Err(ImageError::Decode);
    }
    let mut at = 8usize;
    let mut count = 0;
    let mut dims = None;
    let mut idat = false;
    let mut ended_idat = false;
    while at < bytes.len() {
        count += 1;
        if count > MAX_MARKERS {
            return Err(ImageError::Capacity);
        }
        let header = bytes
            .get(at..at.checked_add(8).ok_or(ImageError::Decode)?)
            .ok_or(ImageError::Decode)?;
        let size = u32::from_be_bytes(header[..4].try_into().unwrap()) as usize;
        let kind = &header[4..];
        if !kind.iter().all(u8::is_ascii_alphabetic) || kind[2] & 32 != 0 {
            return Err(ImageError::Decode);
        }
        let end = at
            .checked_add(12)
            .and_then(|n| n.checked_add(size))
            .filter(|n| *n <= bytes.len())
            .ok_or(ImageError::Decode)?;
        let body = &bytes[at + 8..end - 4];
        match kind {
            b"IHDR" => {
                if dims.is_some() || at != 8 || size != 13 {
                    return Err(ImageError::Decode);
                }
                let w = u32::from_be_bytes(body[..4].try_into().unwrap());
                let h = u32::from_be_bytes(body[4..8].try_into().unwrap());
                length(w, h)?;
                dims = Some((w, h));
            }
            b"IDAT" => {
                if dims.is_none() || ended_idat {
                    return Err(ImageError::Decode);
                }
                idat = true;
            }
            b"IEND" => {
                if size != 0 || !idat || end != bytes.len() {
                    return Err(ImageError::Decode);
                }
                return dims.ok_or(ImageError::Decode);
            }
            b"acTL" | b"fcTL" | b"fdAT" => return Err(ImageError::Unsupported),
            b"PLTE" => {
                if idat {
                    return Err(ImageError::Decode);
                }
            }
            _ => {
                if kind[0] & 32 == 0 {
                    return Err(ImageError::Unsupported);
                }
                if idat {
                    ended_idat = true;
                }
            }
        }
        at = end;
    }
    Err(ImageError::Decode)
}
fn decode_png(bytes: &[u8]) -> Result<ImageAsset, ImageError> {
    let (width, height) = png_structure(bytes)?;
    let mut decoder =
        png::Decoder::new_with_limits(Cursor::new(bytes), png::Limits { bytes: MAX_PIXELS });
    decoder.set_ignore_text_chunk(true);
    decoder.set_ignore_iccp_chunk(true);
    decoder.ignore_checksums(false);
    decoder.set_transformations(png::Transformations::EXPAND | png::Transformations::STRIP_16);
    let mut reader = decoder.read_info().map_err(|_| ImageError::Decode)?;
    let count = reader.output_buffer_size().ok_or(ImageError::Capacity)?;
    if count > MAX_PIXELS {
        return Err(ImageError::Capacity);
    }
    let mut raw = vec![0; count];
    let info = reader
        .next_frame(&mut raw)
        .map_err(|_| ImageError::Decode)?;
    if info.width != width
        || info.height != height
        || info.bit_depth != png::BitDepth::Eight
        || info.buffer_size() != count
    {
        return Err(ImageError::Decode);
    }
    reader.finish().map_err(|_| ImageError::Decode)?;
    let channels = match info.color_type {
        png::ColorType::Grayscale => 1,
        png::ColorType::GrayscaleAlpha => 2,
        png::ColorType::Rgb => 3,
        png::ColorType::Rgba => 4,
        _ => return Err(ImageError::Unsupported),
    };
    if channels == 4 {
        return ImageAsset::from_straight_rgba(width, height, raw);
    }
    let mut pixels = Vec::with_capacity(length(width, height)?);
    for p in raw.chunks_exact(channels) {
        match channels {
            1 => pixels.extend([p[0], p[0], p[0], 255]),
            2 => pixels.extend([p[0], p[0], p[0], p[1]]),
            3 => pixels.extend([p[0], p[1], p[2], 255]),
            4 => pixels.extend_from_slice(p),
            _ => unreachable!(),
        }
    }
    ImageAsset::from_straight_rgba(width, height, pixels)
}
fn jpeg_structure(bytes: &[u8]) -> Result<(u32, u32, u8), ImageError> {
    if bytes.get(..2) != Some(b"\xff\xd8") {
        return Err(ImageError::Decode);
    }
    let mut at = 2usize;
    let mut markers = 0;
    let mut scans = 0;
    let mut dims = None;
    loop {
        if bytes.get(at) != Some(&255) {
            return Err(ImageError::Decode);
        }
        while bytes.get(at) == Some(&255) {
            at += 1;
        }
        let marker = *bytes.get(at).ok_or(ImageError::Decode)?;
        at += 1;
        markers += 1;
        if markers > MAX_MARKERS {
            return Err(ImageError::Capacity);
        }
        if marker == 0xd9 {
            if scans == 0 || at != bytes.len() {
                return Err(ImageError::Decode);
            }
            return dims.ok_or(ImageError::Decode);
        }
        if matches!(marker, 0 | 0xd0..=0xd8) {
            return Err(ImageError::Decode);
        }
        if !matches!(
            marker,
            0xc0 | 0xc2 | 0xc4 | 0xda | 0xdb | 0xdd | 0xe0..=0xef | 0xfe
        ) {
            return Err(ImageError::Unsupported);
        }
        let len = bytes.get(at..at + 2).ok_or(ImageError::Decode)?;
        let len = u16::from_be_bytes(len.try_into().unwrap()) as usize;
        if len < 2 {
            return Err(ImageError::Decode);
        }
        let end = at
            .checked_add(len)
            .filter(|n| *n <= bytes.len())
            .ok_or(ImageError::Decode)?;
        let body = &bytes[at + 2..end];
        at = end;
        if matches!(marker, 0xc0 | 0xc2) {
            if dims.is_some() || scans != 0 || body.len() < 6 {
                return Err(ImageError::Decode);
            }
            if body[0] != 8 || !matches!(body[5], 1 | 3) {
                return Err(ImageError::Unsupported);
            }
            if body.len() != 6 + 3 * usize::from(body[5]) {
                return Err(ImageError::Decode);
            }
            let height = u32::from(u16::from_be_bytes(body[1..3].try_into().unwrap()));
            let width = u32::from(u16::from_be_bytes(body[3..5].try_into().unwrap()));
            length(width, height)?;
            dims = Some((width, height, body[5]));
        }
        if marker == 0xda {
            let (_, _, components) = dims.ok_or(ImageError::Decode)?;
            scans += 1;
            if scans > 32 {
                return Err(ImageError::Capacity);
            }
            if body.is_empty()
                || body[0] == 0
                || body[0] > components
                || body.len() != 4 + 2 * usize::from(body[0])
            {
                return Err(ImageError::Decode);
            }
            // Bounded entropy walk distinguishes escaped FF, restart and true marker.
            loop {
                let value = *bytes.get(at).ok_or(ImageError::Decode)?;
                if value != 255 {
                    at += 1;
                    continue;
                }
                let start = at;
                while bytes.get(at) == Some(&255) {
                    at += 1;
                }
                let next = *bytes.get(at).ok_or(ImageError::Decode)?;
                if next == 0 {
                    if at != start + 1 {
                        return Err(ImageError::Decode);
                    }
                    at += 1;
                    continue;
                }
                if (0xd0..=0xd7).contains(&next) {
                    markers += 1;
                    if markers > MAX_MARKERS {
                        return Err(ImageError::Capacity);
                    }
                    at += 1;
                    continue;
                }
                at = start;
                break;
            }
        }
    }
}
fn decode_jpeg(bytes: &[u8]) -> Result<ImageAsset, ImageError> {
    use zune_jpeg::zune_core::{
        bytestream::ZCursor, colorspace::ColorSpace, options::DecoderOptions,
    };
    let (width, height, components) = jpeg_structure(bytes)?;
    let color = if components == 1 {
        ColorSpace::Luma
    } else {
        ColorSpace::RGBA
    };
    let options = DecoderOptions::new_safe()
        .set_use_unsafe(false)
        .set_strict_mode(true)
        .set_max_width(4096)
        .set_max_height(4096)
        .jpeg_set_max_scans(32)
        .jpeg_set_out_colorspace(color);
    let mut decoder = zune_jpeg::JpegDecoder::new_with_options(ZCursor::new(bytes), options);
    decoder.decode_headers().map_err(|_| ImageError::Decode)?;
    if decoder.dimensions() != Some((width as usize, height as usize)) {
        return Err(ImageError::Decode);
    }
    if !matches!(
        decoder.input_colorspace(),
        Some(ColorSpace::Luma | ColorSpace::RGB | ColorSpace::YCbCr)
    ) {
        return Err(ImageError::Unsupported);
    }
    let expected = length(width, height)? / if components == 1 { 4 } else { 1 };
    if decoder.output_buffer_size() != Some(expected) {
        return Err(ImageError::Decode);
    }
    let mut raw = vec![0; expected];
    decoder
        .decode_into(&mut raw)
        .map_err(|_| ImageError::Decode)?;
    if components == 3 {
        return ImageAsset::from_straight_rgba(width, height, raw);
    }
    let mut pixels = Vec::with_capacity(length(width, height)?);
    for g in raw {
        pixels.extend([g, g, g, 255]);
    }
    ImageAsset::from_straight_rgba(width, height, pixels)
}
/// Decode one complete caller-selected static PNG or 8-bit baseline/progressive
/// grayscale/RGB/YCbCr JPEG. PNG expands palette/sub8-bit/tRNS and strips16 high
/// bytes, verifies CRC and Adler, rejects animation/trailing/incomplete streams.
/// JPEG has at most32 scans and exact SOI/EOI/marker extents; CMYK/YCCK unsupported.
/// Backend pixel differences/filtering remain codec policy. PNG text/ICC ignored;
/// no gamma/ICC/EXIF orientation transform. Allocating worker/UI API, no deadline.
pub fn decode_image(format: ImageFormat, bytes: &[u8]) -> Result<ImageAsset, ImageError> {
    source(bytes)?;
    match format {
        ImageFormat::Png => decode_png(bytes),
        ImageFormat::Jpeg => decode_jpeg(bytes),
    }
}
struct BoundedVec {
    bytes: Vec<u8>,
    maximum: usize,
    failed: bool,
}
impl Write for BoundedVec {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        if self.failed || bytes.len() > self.maximum - self.bytes.len() {
            self.failed = true;
            return Err(std::io::Error::other("image output capacity"));
        }
        self.bytes.extend_from_slice(bytes);
        Ok(bytes.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}
/// Complete owned PNG; errors publish no partial bytes. Backend Drop can attempt
/// a private IEND, with the owned sink gated on capacity failure; no caller I/O.
pub fn encode_png(asset: &ImageAsset, maximum_bytes: usize) -> Result<Vec<u8>, ImageError> {
    if maximum_bytes == 0 || maximum_bytes > MAX_ENCODED {
        return Err(ImageError::Capacity);
    }
    let mut output = BoundedVec {
        bytes: Vec::new(),
        maximum: maximum_bytes,
        failed: false,
    };
    let result = (|| {
        let mut encoder = png::Encoder::new(&mut output, asset.width, asset.height);
        encoder.set_color(png::ColorType::Rgba);
        encoder.set_depth(png::BitDepth::Eight);
        let mut writer = encoder.write_header()?;
        writer.write_image_data(&asset.pixels)?;
        writer.finish()
    })();
    if result.is_err() {
        return Err(if output.failed {
            ImageError::Capacity
        } else {
            ImageError::Encode
        });
    }
    Ok(output.bytes)
}

#[cfg(feature = "jpeg-encode")]
mod jpeg;
#[cfg(feature = "jpeg-encode")]
pub use jpeg::{JpegOptions, JpegSampling, encode_jpeg};

#[cfg(feature = "gif-image")]
mod gif;
#[cfg(feature = "gif-image")]
pub use gif::{GifBackground, decode_static_gif};
