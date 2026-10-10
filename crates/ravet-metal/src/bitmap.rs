use super::{MetalError, MetalImage, MetalRectangle};
use ravet_surface::layout::Rect;

/// Immutable owned packed top-to-bottom premultiplied RGBA8 numeric components.
/// Explicit preparation validates shape and RGB<=alpha; there is no implicit
/// file/network loading, gamma/color conversion, decoder, clock or GPU upload.
#[derive(Debug)]
pub struct MetalBitmap {
    width: u32,
    height: u32,
    pixels: Vec<u8>,
}
impl MetalBitmap {
    /// Adopt 1..4096 dimensions and exactly width*height*4 bytes, at most64 MiB.
    /// Every RGB component must be <=alpha. Validation scans all pixels on the
    /// caller's UI/worker; rejection consumes the supplied allocation.
    pub fn from_premultiplied_rgba(
        width: u32,
        height: u32,
        pixels: Vec<u8>,
    ) -> Result<Self, MetalError> {
        if width == 0 || height == 0 || width > 4096 || height > 4096 {
            return Err(MetalError::InvalidInput);
        }
        if pixels.len() != width as usize * height as usize * 4
            || pixels
                .as_chunks::<4>()
                .0
                .iter()
                .any(|p| p[0] > p[3] || p[1] > p[3] || p[2] > p[3])
        {
            return Err(MetalError::InvalidInput);
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
}
impl MetalImage {
    /// Validate/adopt this public owned readback as an immutable bitmap without
    /// pixel copying. Public field mutation is revalidated; no GPU handle survives.
    pub fn into_bitmap(self) -> Result<MetalBitmap, MetalError> {
        MetalBitmap::from_premultiplied_rgba(self.width, self.height, self.pixels)
    }
}
/// One ordered offscreen scene command. Images cover a destination rectangle
/// using their complete source, proportional clipping and nearest sampling.
/// Assets are borrowed only through synchronous render; no implicit tint/gamma
/// transform or high-quality resampling is selected.
#[derive(Clone, Copy, Debug)]
pub enum MetalPaint<'a> {
    Rectangle(MetalRectangle),
    Image {
        destination: Rect,
        source: &'a MetalBitmap,
    },
}
