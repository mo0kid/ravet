//! Explicit static GIF worker decoding with a consumer-selected canvas background.
use super::{ImageAsset, ImageError};
use std::{io::Cursor, num::NonZeroU64};

const MAX_SOURCE: usize = 16 * 1024 * 1024;
const MAX_RECORDS: usize = 8192;
const MAX_PIXELS: usize = 64 * 1024 * 1024;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
/// Explicit logical-screen fill for pixels outside the one image descriptor.
pub enum GifBackground {
    /// Transparent zero RGBA; no background is inferred.
    Transparent,
    /// Opaque global-palette background; a missing global palette rejects.
    LogicalPalette,
}

struct Parser<'a> {
    bytes: &'a [u8],
    at: usize,
    records: usize,
}
impl<'a> Parser<'a> {
    fn take(&mut self, n: usize) -> Result<&'a [u8], ImageError> {
        let end = self.at.checked_add(n).ok_or(ImageError::Decode)?;
        let part = self.bytes.get(self.at..end).ok_or(ImageError::Decode)?;
        self.at = end;
        Ok(part)
    }
    fn byte(&mut self) -> Result<u8, ImageError> {
        Ok(self.take(1)?[0])
    }
    fn record(&mut self) -> Result<(), ImageError> {
        self.records += 1;
        if self.records > MAX_RECORDS {
            return Err(ImageError::Capacity);
        }
        Ok(())
    }
    fn subblocks(&mut self) -> Result<(), ImageError> {
        loop {
            self.record()?;
            let n = usize::from(self.byte()?);
            if n == 0 {
                return Ok(());
            }
            self.take(n)?;
        }
    }
    fn palette(&mut self, packed: u8) -> Result<Option<&'a [u8]>, ImageError> {
        if packed & 128 == 0 {
            return Ok(None);
        }
        Ok(Some(self.take(3 * (1usize << ((packed & 7) + 1)))?))
    }
}
struct Profile<'a> {
    width: u16,
    height: u16,
    left: u16,
    top: u16,
    frame_width: u16,
    frame_height: u16,
    palette: &'a [u8],
    transparent: Option<u8>,
    background: Option<[u8; 4]>,
}
fn dimension(n: u16) -> Result<(), ImageError> {
    if n == 0 {
        Err(ImageError::InvalidInput)
    } else if n > 4096 {
        Err(ImageError::Capacity)
    } else {
        Ok(())
    }
}
fn word(bytes: &[u8]) -> u16 {
    u16::from_le_bytes(bytes.try_into().unwrap())
}
fn preflight(bytes: &[u8]) -> Result<Profile<'_>, ImageError> {
    if bytes.is_empty() {
        return Err(ImageError::InvalidInput);
    }
    if bytes.len() > MAX_SOURCE {
        return Err(ImageError::Capacity);
    }
    let mut p = Parser {
        bytes,
        at: 0,
        records: 0,
    };
    let header = p.take(6)?;
    if header != b"GIF87a" && header != b"GIF89a" {
        return Err(ImageError::Decode);
    }
    let screen = p.take(7)?;
    let width = word(&screen[..2]);
    let height = word(&screen[2..4]);
    dimension(width)?;
    dimension(height)?;
    let global = p.palette(screen[4])?;
    let background = global.and_then(|pal| {
        let at = usize::from(screen[5]) * 3;
        pal.get(at..at + 3).map(|rgb| [rgb[0], rgb[1], rgb[2], 255])
    });
    if global.is_some() && background.is_none() {
        return Err(ImageError::Decode);
    }
    let mut control = false;
    let mut transparent = None;
    let mut image = None;
    loop {
        p.record()?;
        match p.byte()? {
            0x3b => {
                if p.at != bytes.len() {
                    return Err(ImageError::Decode);
                }
                return image.ok_or(ImageError::Decode);
            }
            0x2c => {
                if image.is_some() {
                    return Err(ImageError::Unsupported);
                }
                let desc = p.take(9)?;
                let left = word(&desc[..2]);
                let top = word(&desc[2..4]);
                let frame_width = word(&desc[4..6]);
                let frame_height = word(&desc[6..8]);
                dimension(frame_width)?;
                dimension(frame_height)?;
                if u32::from(left) + u32::from(frame_width) > u32::from(width)
                    || u32::from(top) + u32::from(frame_height) > u32::from(height)
                    || desc[8] & 0x18 != 0
                {
                    return Err(ImageError::Decode);
                }
                let palette = p.palette(desc[8])?.or(global).ok_or(ImageError::Decode)?;
                if transparent.is_some_and(|t| usize::from(t) >= palette.len() / 3) {
                    return Err(ImageError::Decode);
                }
                if !(2..=8).contains(&p.byte()?) {
                    return Err(ImageError::Decode);
                }
                p.subblocks()?;
                image = Some(Profile {
                    width,
                    height,
                    left,
                    top,
                    frame_width,
                    frame_height,
                    palette,
                    transparent,
                    background,
                });
            }
            0x21 => {
                if header != b"GIF89a" {
                    return Err(ImageError::Decode);
                }
                match p.byte()? {
                    0xf9 => {
                        if control || image.is_some() {
                            return Err(ImageError::Unsupported);
                        }
                        control = true;
                        if p.byte()? != 4 {
                            return Err(ImageError::Decode);
                        }
                        let gce = p.take(4)?;
                        if gce[0] & 0xe0 != 0 || p.byte()? != 0 {
                            return Err(ImageError::Decode);
                        }
                        if gce[0] & 2 != 0 || ((gce[0] >> 2) & 7) > 3 {
                            return Err(ImageError::Unsupported);
                        }
                        transparent = (gce[0] & 1 != 0).then_some(gce[3]);
                    }
                    0xfe => p.subblocks()?,
                    0xff => {
                        if p.byte()? != 11 {
                            return Err(ImageError::Decode);
                        }
                        let id = p.take(11)?;
                        if id == b"NETSCAPE2.0" || id == b"ANIMEXTS1.0" {
                            return Err(ImageError::Unsupported);
                        }
                        p.subblocks()?;
                    }
                    _ => return Err(ImageError::Unsupported),
                }
            }
            _ => return Err(ImageError::Decode),
        }
    }
}

/// Decode exactly one static GIF image into a complete logical-screen asset.
/// Caller chooses transparent or valid global-palette background explicitly.
/// Frame transparency replaces canvas pixels; no animation/compositing timeline.
/// Sources are 1..16 MiB; screen/frame dimensions 1..4096 and RGBA output<=64 MiB.
/// Complete preflight limits all blocks/subblocks to8192 and rejects second
/// images, looping applications, plaintext/unknown extensions, user-input flags,
/// invalid palettes, trailing data and incomplete LZW. Comments and other opaque
/// applications are ignored. Interlacing is decoded; delay/disposal scheduling,
/// metadata/color management and orientation are not applied. No file or caller
/// I/O occurs. Codec working allocations, allocator failure and deadlines remain
/// worker/consumer obligations. Enable the default-disabled `gif-image` feature.
pub fn decode_static_gif(
    bytes: &[u8],
    background: GifBackground,
) -> Result<ImageAsset, ImageError> {
    let profile = preflight(bytes)?;
    let fill = match background {
        GifBackground::Transparent => [0; 4],
        GifBackground::LogicalPalette => profile.background.ok_or(ImageError::Unsupported)?,
    };
    let mut options = gif::DecodeOptions::new();
    options.set_color_output(gif::ColorOutput::Indexed);
    options.set_memory_limit(gif::MemoryLimit::Bytes(
        NonZeroU64::new(MAX_PIXELS as u64).unwrap(),
    ));
    options.check_frame_consistency(true);
    options.check_lzw_end_code(true);
    options.allow_unknown_blocks(false);
    let mut decoder = options
        .read_info(Cursor::new(bytes))
        .map_err(|_| ImageError::Decode)?;
    let frame = decoder
        .read_next_frame()
        .map_err(|_| ImageError::Decode)?
        .ok_or(ImageError::Decode)?;
    if frame.left != profile.left
        || frame.top != profile.top
        || frame.width != profile.frame_width
        || frame.height != profile.frame_height
        || frame.transparent != profile.transparent
        || frame.buffer.len()
            != usize::from(profile.frame_width) * usize::from(profile.frame_height)
        || frame
            .buffer
            .iter()
            .any(|i| usize::from(*i) >= profile.palette.len() / 3)
    {
        return Err(ImageError::Decode);
    }
    let count = usize::from(profile.width) * usize::from(profile.height);
    let mut pixels = vec![0; count * 4];
    for p in pixels.as_chunks_mut::<4>().0 {
        *p = fill;
    }
    for (y, row) in frame
        .buffer
        .chunks_exact(usize::from(profile.frame_width))
        .enumerate()
    {
        let first = ((usize::from(profile.top) + y) * usize::from(profile.width)
            + usize::from(profile.left))
            * 4;
        for (pixel, index) in pixels[first..first + row.len() * 4]
            .as_chunks_mut::<4>()
            .0
            .iter_mut()
            .zip(row)
        {
            let at = usize::from(*index) * 3;
            *pixel = [
                profile.palette[at],
                profile.palette[at + 1],
                profile.palette[at + 2],
                if profile.transparent == Some(*index) {
                    0
                } else {
                    255
                },
            ];
        }
    }
    // Decode through the end code rather than letting next_frame_info discard
    // the remaining LZW bytes once the expected pixel buffer has filled.
    // One extra decoded pixel also makes the frame invalid.
    if decoder
        .fill_buffer(&mut [0; 1])
        .map_err(|_| ImageError::Decode)?
    {
        return Err(ImageError::Decode);
    }
    // Complete structural preflight already excludes any second image/trailing file data.
    if decoder
        .read_next_frame()
        .map_err(|_| ImageError::Decode)?
        .is_some()
    {
        return Err(ImageError::Decode);
    }
    ImageAsset::from_straight_rgba(u32::from(profile.width), u32::from(profile.height), pixels)
}
