//! Uncompressed big-endian AIFF; file operations belong on worker threads.
use crate::AudioFileError;
use std::io::{Read, Seek, SeekFrom, Write};

/// Supported signed, big-endian PCM widths. AIFF-C is not supported.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AiffEncoding {
    Pcm16,
    Pcm24,
    Pcm32,
}
impl AiffEncoding {
    fn bytes(self) -> usize {
        match self {
            Self::Pcm16 => 2,
            Self::Pcm24 => 3,
            Self::Pcm32 => 4,
        }
    }
}
/// AIFF metadata; 1..64 channels and integer sample rates 1..768000 Hz.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AiffFormat {
    pub channels: u16,
    pub sample_rate: u32,
    pub encoding: AiffEncoding,
}
impl AiffFormat {
    fn stride(self) -> usize {
        usize::from(self.channels) * self.encoding.bytes()
    }
    fn scratch(self, maximum: usize) -> Result<Vec<u8>, AudioFileError> {
        if !(1..=64).contains(&self.channels) || !(1..=768000).contains(&self.sample_rate) {
            return Err(AudioFileError::Unsupported);
        }
        if !(1..=65536).contains(&maximum) {
            return Err(AudioFileError::Capacity);
        }
        Ok(vec![0; self.stride() * maximum])
    }
}
fn be16(bytes: &[u8]) -> u16 {
    u16::from_be_bytes(bytes.try_into().unwrap())
}
fn be32(bytes: &[u8]) -> u32 {
    u32::from_be_bytes(bytes.try_into().unwrap())
}
fn decode_rate(bytes: &[u8]) -> Result<u32, AudioFileError> {
    let exponent = be16(&bytes[..2]);
    let significand = u64::from_be_bytes(bytes[2..].try_into().unwrap());
    // Positive normalized extended80 values only, with no fractional rounding.
    if !(16383..=16402).contains(&exponent) || significand >> 63 != 1 {
        return Err(AudioFileError::Unsupported);
    }
    let shift = 63 - (exponent - 16383);
    if significand & ((1u64 << shift) - 1) != 0 {
        return Err(AudioFileError::Unsupported);
    }
    let rate = (significand >> shift) as u32;
    if !(1..=768000).contains(&rate) {
        return Err(AudioFileError::Unsupported);
    }
    Ok(rate)
}
fn encode_rate(rate: u32) -> [u8; 10] {
    let exponent = 31 - rate.leading_zeros();
    let mut bytes = [0; 10];
    bytes[..2].copy_from_slice(&(16383 + exponent as u16).to_be_bytes());
    bytes[2..].copy_from_slice(&(u64::from(rate) << (63 - exponent)).to_be_bytes());
    bytes
}

/// Prepared interleaved f32 reader over a seekable uncompressed AIFF stream.
/// Unknown chunks are skipped within FORM (at most 4096 chunks). COMM/SSND must
/// be unique and consistent; zero frames may omit SSND. Its offset is honored
/// but nonzero blockSize rejects.
/// Trailing bytes outside FORM are ignored. No metadata/loop markers are exposed.
pub struct AiffReader<R> {
    source: R,
    format: AiffFormat,
    frames: u64,
    position: u64,
    offset: u64,
    scratch: Vec<u8>,
}
impl<R: Read + Seek> AiffReader<R> {
    /// Inspect from offset zero, validate container and allocate bounded scratch.
    pub fn new(mut source: R, maximum_frames: usize) -> Result<Self, AudioFileError> {
        let actual = source.seek(SeekFrom::End(0))?;
        source.seek(SeekFrom::Start(0))?;
        let mut header = [0; 12];
        source.read_exact(&mut header)?;
        if &header[..4] != b"FORM" || &header[8..] != b"AIFF" {
            return Err(AudioFileError::Unsupported);
        }
        let end = u64::from(be32(&header[4..8])) + 8;
        if end < 12 || end > actual {
            return Err(AudioFileError::InvalidRecord);
        }
        let mut common = None;
        let mut sound = None;
        let mut position = 12;
        let mut chunks = 0;
        while position < end {
            chunks += 1;
            if chunks > 4096 {
                return Err(AudioFileError::Capacity);
            }
            if end - position < 8 {
                return Err(AudioFileError::InvalidRecord);
            }
            source.seek(SeekFrom::Start(position))?;
            let mut chunk = [0; 8];
            source.read_exact(&mut chunk)?;
            let size = u64::from(be32(&chunk[4..]));
            let body = position + 8;
            let next = body + size + (size & 1);
            if next > end {
                return Err(AudioFileError::InvalidRecord);
            }
            match &chunk[..4] {
                b"COMM" => {
                    if common.is_some() || size != 18 {
                        return Err(AudioFileError::InvalidRecord);
                    }
                    let mut bytes = [0; 18];
                    source.read_exact(&mut bytes)?;
                    let encoding = match be16(&bytes[6..8]) {
                        16 => AiffEncoding::Pcm16,
                        24 => AiffEncoding::Pcm24,
                        32 => AiffEncoding::Pcm32,
                        _ => return Err(AudioFileError::Unsupported),
                    };
                    common = Some((
                        AiffFormat {
                            channels: be16(&bytes[..2]),
                            sample_rate: decode_rate(&bytes[8..])?,
                            encoding,
                        },
                        u64::from(be32(&bytes[2..6])),
                    ));
                }
                b"SSND" => {
                    if sound.is_some() || size < 8 {
                        return Err(AudioFileError::InvalidRecord);
                    }
                    let mut bytes = [0; 8];
                    source.read_exact(&mut bytes)?;
                    let offset = u64::from(be32(&bytes[..4]));
                    if be32(&bytes[4..]) != 0 {
                        return Err(AudioFileError::Unsupported);
                    }
                    if offset > size - 8 {
                        return Err(AudioFileError::InvalidRecord);
                    }
                    sound = Some((body + 8 + offset, size - 8 - offset));
                }
                _ => {}
            }
            position = next;
        }
        let (format, frames) = common.ok_or(AudioFileError::InvalidRecord)?;
        let scratch = format.scratch(maximum_frames)?;
        let (offset, bytes) = match sound {
            Some(value) => value,
            None if frames == 0 => (end, 0),
            None => return Err(AudioFileError::InvalidRecord),
        };
        if bytes != frames * format.stride() as u64 {
            return Err(AudioFileError::InvalidRecord);
        }
        Ok(Self {
            source,
            format,
            frames,
            position: 0,
            offset,
            scratch,
        })
    }
    pub fn format(&self) -> AiffFormat {
        self.format
    }
    pub fn frames(&self) -> u64 {
        self.frames
    }
    pub fn position(&self) -> u64 {
        self.position
    }
    pub fn maximum_frames(&self) -> usize {
        self.scratch.len() / self.format.stride()
    }
    /// Set a frame in 0..=frames; actual file seek occurs on the next read.
    pub fn seek_frame(&mut self, frame: u64) -> Result<(), AudioFileError> {
        if frame > self.frames {
            return Err(AudioFileError::Capacity);
        }
        self.position = frame;
        Ok(())
    }
    /// Decode bounded complete frames, preserving unused output tail. On read
    /// failure, destination and logical position remain unchanged for retry.
    pub fn read_frames(&mut self, output: &mut [f32]) -> Result<usize, AudioFileError> {
        let channels = usize::from(self.format.channels);
        if !output.len().is_multiple_of(channels) {
            return Err(AudioFileError::InvalidSamples);
        }
        let requested = output.len() / channels;
        if requested > self.maximum_frames() {
            return Err(AudioFileError::Capacity);
        }
        let frames = (requested as u64).min(self.frames - self.position) as usize;
        if frames == 0 {
            return Ok(0);
        }
        let width = self.format.encoding.bytes();
        let bytes = frames * self.format.stride();
        self.source.seek(SeekFrom::Start(
            self.offset + self.position * self.format.stride() as u64,
        ))?;
        self.source.read_exact(&mut self.scratch[..bytes])?;
        let scale = (1u64 << (width * 8 - 1)) as f32;
        for (target, bytes) in output[..frames * channels]
            .iter_mut()
            .zip(self.scratch[..bytes].chunks_exact(width))
        {
            let mut integer = if bytes[0] & 128 != 0 {
                [255; 4]
            } else {
                [0; 4]
            };
            integer[4 - width..].copy_from_slice(bytes);
            *target = i32::from_be_bytes(integer) as f32 / scale;
        }
        self.position += frames as u64;
        Ok(frames)
    }
}

/// Bounded uncompressed AIFF writer. Destination must initially be empty.
/// Call finish explicitly to patch counts/sizes; Drop performs no I/O.
/// Partial I/O failures poison the writer; discard the partial file.
pub struct AiffWriter<W> {
    destination: W,
    format: AiffFormat,
    data_bytes: u32,
    scratch: Vec<u8>,
    failed: bool,
}
impl<W: Write + Seek> AiffWriter<W> {
    pub fn new(
        mut destination: W,
        format: AiffFormat,
        maximum_frames: usize,
    ) -> Result<Self, AudioFileError> {
        let scratch = format.scratch(maximum_frames)?;
        if destination.seek(SeekFrom::End(0))? != 0 {
            return Err(AudioFileError::InvalidRecord);
        }
        destination.seek(SeekFrom::Start(0))?;
        let mut header = [0; 54];
        header[..4].copy_from_slice(b"FORM");
        header[4..8].copy_from_slice(&46u32.to_be_bytes());
        header[8..16].copy_from_slice(b"AIFFCOMM");
        header[16..20].copy_from_slice(&18u32.to_be_bytes());
        header[20..22].copy_from_slice(&format.channels.to_be_bytes());
        header[26..28].copy_from_slice(&(format.encoding.bytes() as u16 * 8).to_be_bytes());
        header[28..38].copy_from_slice(&encode_rate(format.sample_rate));
        header[38..42].copy_from_slice(b"SSND");
        header[42..46].copy_from_slice(&8u32.to_be_bytes());
        destination.write_all(&header)?;
        Ok(Self {
            destination,
            format,
            data_bytes: 0,
            scratch,
            failed: false,
        })
    }
    pub fn frames(&self) -> u64 {
        u64::from(self.data_bytes) / self.format.stride() as u64
    }
    /// Validate all samples before I/O; finite PCM clips to [-1,1] and rounds to
    /// nearest integer. Partial frames, NaN/infinity and oversized blocks reject.
    pub fn write_frames(&mut self, input: &[f32]) -> Result<(), AudioFileError> {
        if self.failed {
            return Err(AudioFileError::Poisoned);
        }
        if !input
            .len()
            .is_multiple_of(usize::from(self.format.channels))
            || input.iter().any(|v| !v.is_finite())
        {
            return Err(AudioFileError::InvalidSamples);
        }
        let width = self.format.encoding.bytes();
        let bytes = input
            .len()
            .checked_mul(width)
            .ok_or(AudioFileError::Capacity)?;
        let total = u64::from(self.data_bytes) + bytes as u64;
        if bytes > self.scratch.len() || total + 46 + (total & 1) > u64::from(u32::MAX) {
            return Err(AudioFileError::Capacity);
        }
        let scale = (1u64 << (width * 8 - 1)) as f64;
        for (sample, target) in input
            .iter()
            .zip(self.scratch[..bytes].chunks_exact_mut(width))
        {
            let integer = (f64::from(sample.clamp(-1., 1.)) * scale)
                .round()
                .clamp(-scale, scale - 1.) as i32;
            target.copy_from_slice(&integer.to_be_bytes()[4 - width..]);
        }
        if let Err(error) = self.destination.write_all(&self.scratch[..bytes]) {
            self.failed = true;
            return Err(error.into());
        }
        self.data_bytes = total as u32;
        Ok(())
    }
    /// Add odd-data padding, patch FORM/COMM/SSND sizes and flush.
    pub fn finish(mut self) -> Result<W, AudioFileError> {
        if self.failed {
            return Err(AudioFileError::Poisoned);
        }
        let pad = self.data_bytes & 1;
        if pad != 0 {
            self.destination.write_all(&[0])?;
        }
        self.destination.seek(SeekFrom::Start(4))?;
        self.destination
            .write_all(&(46 + self.data_bytes + pad).to_be_bytes())?;
        self.destination.seek(SeekFrom::Start(22))?;
        self.destination
            .write_all(&(self.frames() as u32).to_be_bytes())?;
        self.destination.seek(SeekFrom::Start(42))?;
        self.destination
            .write_all(&(8 + self.data_bytes).to_be_bytes())?;
        self.destination.seek(SeekFrom::End(0))?;
        self.destination.flush()?;
        Ok(self.destination)
    }
}
