//! Explicit sRGB/linear-sRGB transfer and linear-light source-over preparation.
//!
//! Caller-declared profiles apply to numeric RGBA8; no ICC, display profile,
//! metadata inference or implicit renderer change occurs. Preparation belongs on
//! UI/worker threads. Caller-buffer operations allocate nothing; dimensions are
//! 1..4096 and packed image bytes at most 64 MiB. No deadline is certified.
//!
//! ```
//! use ravet_surface::{color::LinearRgba, drawing::Color};
//! let overlay = LinearRgba::from_srgba(Color {
//!     red: 255, green: 255, blue: 255, alpha: 128,
//! });
//! let backdrop = LinearRgba::from_srgba(Color::rgb(0, 0, 0));
//! assert_eq!(overlay.source_over(backdrop).to_srgba(), Color::rgb(188, 188, 188));
//! ```
use crate::drawing::Color;

/// Standard sRGB primaries with encoded or linear-light normalized components.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ColorEncoding {
    /// Standard nonlinear sRGB transfer.
    Srgb,
    /// Linear-light components with the same sRGB primaries.
    LinearSrgb,
}
/// RGB association in the declared encoded numeric space, not another space.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AlphaMode {
    /// RGB independent of alpha; hidden RGB can remain at alpha zero.
    Straight,
    /// RGB multiplied by alpha in this view's declared encoding.
    Premultiplied,
}
/// Rejected profile, image shape, output or normalized scalar input.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ColorError {
    /// Invalid scalar, image shape, premultiplied channel or output size.
    InvalidInput,
    /// Dimension or packed-byte capacity exceeded.
    Capacity,
}
impl std::fmt::Display for ColorError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{self:?}")
    }
}
impl std::error::Error for ColorError {}

fn decode(value: f64, encoding: ColorEncoding) -> f64 {
    match encoding {
        ColorEncoding::LinearSrgb => value,
        ColorEncoding::Srgb if value <= 0.04045 => value / 12.92,
        ColorEncoding::Srgb => ((value + 0.055) / 1.055).powf(2.4),
    }
}
fn encode(value: f64, encoding: ColorEncoding) -> f64 {
    match encoding {
        ColorEncoding::LinearSrgb => value,
        ColorEncoding::Srgb if value <= 0.0031308 => value * 12.92,
        ColorEncoding::Srgb => 1.055 * value.powf(1. / 2.4) - 0.055,
    }
    .clamp(0., 1.)
}
fn byte(value: f64) -> u8 {
    (value.clamp(0., 1.) * 255.).round() as u8
}
/// Finite normalized straight linear-sRGB channels, independent of display policy.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LinearRgba {
    channels: [f64; 4],
}
impl LinearRgba {
    /// Reject nonfinite or out-of-range channels before constructing a value.
    pub fn new(red: f64, green: f64, blue: f64, alpha: f64) -> Option<Self> {
        let channels = [red, green, blue, alpha];
        channels
            .iter()
            .all(|v| v.is_finite() && (0. ..=1.).contains(v))
            .then_some(Self { channels })
    }
    /// Copied straight normalized channels in red, green, blue, alpha order.
    pub fn components(self) -> [f64; 4] {
        self.channels
    }
    /// Declare a drawing color to be straight encoded sRGB and decode it.
    /// Hidden RGB at alpha zero is retained.
    pub fn from_srgba(color: Color) -> Self {
        Self::from_bytes(
            [color.red, color.green, color.blue, color.alpha],
            ColorEncoding::Srgb,
            AlphaMode::Straight,
        )
    }
    /// Encode straight sRGB with nearest half-up byte quantization.
    /// Hidden RGB at alpha zero is retained; linear RGBA8 quantization is lossy.
    pub fn to_srgba(self) -> Color {
        let [red, green, blue, alpha] = self.to_bytes(ColorEncoding::Srgb, AlphaMode::Straight);
        Color {
            red,
            green,
            blue,
            alpha,
        }
    }
    /// Composite this foreground over backdrop in linear light.
    /// Alpha stays linear; a completely transparent result has zero RGB.
    pub fn source_over(self, backdrop: Self) -> Self {
        let sa = self.channels[3];
        let ba = backdrop.channels[3];
        let alpha = (sa + ba * (1. - sa)).clamp(0., 1.);
        let mut channels = [0., 0., 0., alpha];
        if alpha > 0. {
            for (index, channel) in channels[..3].iter_mut().enumerate() {
                *channel = (self.channels[index] * (sa / alpha)
                    + backdrop.channels[index] * (ba / alpha) * (1. - sa))
                    .clamp(0., 1.);
            }
        }
        Self { channels }
    }
    fn from_bytes(pixel: [u8; 4], encoding: ColorEncoding, association: AlphaMode) -> Self {
        let alpha = f64::from(pixel[3]) / 255.;
        let mut channels = [0., 0., 0., alpha];
        for (index, channel) in channels[..3].iter_mut().enumerate() {
            let encoded = match association {
                AlphaMode::Straight => f64::from(pixel[index]) / 255.,
                AlphaMode::Premultiplied if pixel[3] == 0 => 0.,
                AlphaMode::Premultiplied => f64::from(pixel[index]) / f64::from(pixel[3]),
            };
            *channel = decode(encoded, encoding);
        }
        Self { channels }
    }
    fn to_bytes(self, encoding: ColorEncoding, association: AlphaMode) -> [u8; 4] {
        let alpha = self.channels[3];
        let mut pixel = [0, 0, 0, byte(alpha)];
        for (index, output) in pixel[..3].iter_mut().enumerate() {
            let encoded = encode(self.channels[index], encoding);
            *output = byte(match association {
                AlphaMode::Straight => encoded,
                AlphaMode::Premultiplied => encoded * alpha,
            });
        }
        pixel
    }
}
/// Validated immutable packed RGBA8 with explicit encoding and alpha association.
/// No metadata or decoder policy is inferred; alpha itself is always linear.
#[derive(Clone, Copy, Debug)]
pub struct RgbaView<'a> {
    width: u32,
    height: u32,
    pixels: &'a [u8],
    encoding: ColorEncoding,
    association: AlphaMode,
}
impl<'a> RgbaView<'a> {
    /// Admit exact top-to-bottom RGBA8, dimensions 1..4096 and at most 64 MiB.
    /// Premultiplied RGB must be <=alpha in every pixel, including the last.
    pub fn new(
        width: u32,
        height: u32,
        pixels: &'a [u8],
        encoding: ColorEncoding,
        association: AlphaMode,
    ) -> Result<Self, ColorError> {
        if width == 0 || height == 0 {
            return Err(ColorError::InvalidInput);
        }
        if width > 4096 || height > 4096 {
            return Err(ColorError::Capacity);
        }
        let length = (width as usize)
            .checked_mul(height as usize)
            .and_then(|n| n.checked_mul(4))
            .ok_or(ColorError::Capacity)?;
        if length > 64 * 1024 * 1024 {
            return Err(ColorError::Capacity);
        }
        if pixels.len() != length {
            return Err(ColorError::InvalidInput);
        }
        if association == AlphaMode::Premultiplied
            && pixels
                .as_chunks::<4>()
                .0
                .iter()
                .any(|p| p[..3].iter().any(|c| *c > p[3]))
        {
            return Err(ColorError::InvalidInput);
        }
        Ok(Self {
            width,
            height,
            pixels,
            encoding,
            association,
        })
    }
    /// Width in pixels.
    pub fn width(self) -> u32 {
        self.width
    }
    /// Height in pixels.
    pub fn height(self) -> u32 {
        self.height
    }
    /// Immutable packed top-to-bottom pixels with the admitted profile.
    pub fn pixels(self) -> &'a [u8] {
        self.pixels
    }
    /// Caller-declared color component transfer.
    pub fn encoding(self) -> ColorEncoding {
        self.encoding
    }
    /// Alpha association in the declared encoded space.
    pub fn alpha_mode(self) -> AlphaMode {
        self.association
    }
    /// Convert to caller-declared output profile without allocation.
    /// Exact output length is checked before writing. Identical profiles copy
    /// exact bytes. Other conversions unpremultiply before transfer, and multiply
    /// after target transfer. Zero-alpha premultiplied RGB is canonical zero;
    /// straight hidden RGB is retained. Byte rounding is nearest half-up.
    /// RGBA8 linear transfer is lossy; arbitrary roundtrip is not promised.
    pub fn convert_into(
        self,
        encoding: ColorEncoding,
        association: AlphaMode,
        output: &mut [u8],
    ) -> Result<(), ColorError> {
        if output.len() != self.pixels.len() {
            return Err(ColorError::InvalidInput);
        }
        if encoding == self.encoding && association == self.association {
            output.copy_from_slice(self.pixels);
            return Ok(());
        }
        if encoding == self.encoding {
            // No transfer is required. Exact rational association preserves
            // half-up ties instead of adding a decode/encode floating roundtrip.
            for (source, target) in self
                .pixels
                .as_chunks::<4>()
                .0
                .iter()
                .zip(output.as_chunks_mut::<4>().0)
            {
                target[3] = source[3];
                let alpha = u32::from(source[3]);
                for channel in 0..3 {
                    let value = u32::from(source[channel]);
                    target[channel] = match association {
                        AlphaMode::Straight if alpha == 0 => 0,
                        AlphaMode::Straight => ((value * 255 + alpha / 2) / alpha) as u8,
                        AlphaMode::Premultiplied => ((value * alpha + 127) / 255) as u8,
                    };
                }
            }
            return Ok(());
        }
        for (source, target) in self
            .pixels
            .as_chunks::<4>()
            .0
            .iter()
            .zip(output.as_chunks_mut::<4>().0)
        {
            *target = LinearRgba::from_bytes(*source, self.encoding, self.association)
                .to_bytes(encoding, association);
        }
        Ok(())
    }
}
/// Composite equally-sized views using linear-light Porter-Duff source-over.
/// Entire shape/output preflight completes before any output write; this performs
/// no allocation or native/file access. Safe borrows prevent input/output alias.
/// Alpha is linear, transfer applies only to RGB, and a transparent result has
/// zero RGB. Output premultiplication occurs in the chosen encoded numeric space.
pub fn composite_source_over(
    source: RgbaView<'_>,
    backdrop: RgbaView<'_>,
    encoding: ColorEncoding,
    association: AlphaMode,
    output: &mut [u8],
) -> Result<(), ColorError> {
    if source.width != backdrop.width
        || source.height != backdrop.height
        || output.len() != source.pixels.len()
    {
        return Err(ColorError::InvalidInput);
    }
    for ((foreground, background), target) in source
        .pixels
        .as_chunks::<4>()
        .0
        .iter()
        .zip(backdrop.pixels.as_chunks::<4>().0)
        .zip(output.as_chunks_mut::<4>().0)
    {
        let foreground = LinearRgba::from_bytes(*foreground, source.encoding, source.association);
        let background =
            LinearRgba::from_bytes(*background, backdrop.encoding, backdrop.association);
        *target = foreground
            .source_over(background)
            .to_bytes(encoding, association);
    }
    Ok(())
}
