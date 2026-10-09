use std::io::{self, Read, Seek, SeekFrom, Write};
/// Supported little-endian RIFF encodings; extensible/RF64/W64 are not supported.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WaveEncoding {
    Pcm16,
    Pcm24,
    Pcm32,
    Float32,
}
impl WaveEncoding {
    fn bytes(self) -> usize {
        match self {
            Self::Pcm16 => 2,
            Self::Pcm24 => 3,
            _ => 4,
        }
    }
    fn tag(self) -> u16 {
        if self == Self::Float32 { 3 } else { 1 }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct WaveFormat {
    pub channels: u16,
    pub sample_rate: u32,
    pub encoding: WaveEncoding,
}
impl WaveFormat {
    fn validate(self) -> Result<(), AudioFileError> {
        if !(1..=64).contains(&self.channels) || !(1..=768000).contains(&self.sample_rate) {
            return Err(AudioFileError::Unsupported);
        }
        Ok(())
    }
    fn stride(self) -> usize {
        usize::from(self.channels) * self.encoding.bytes()
    }
}
#[derive(Debug)]
pub enum AudioFileError {
    Io(io::Error),
    InvalidRecord,
    Unsupported,
    Capacity,
    InvalidSamples,
    Poisoned,
}
impl From<io::Error> for AudioFileError {
    fn from(error: io::Error) -> Self {
        Self::Io(error)
    }
}
impl std::fmt::Display for AudioFileError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "audio file error: {self:?}")
    }
}
impl std::error::Error for AudioFileError {}
fn scratch(format: WaveFormat, maximum: usize) -> Result<Vec<u8>, AudioFileError> {
    format.validate()?;
    if !(1..=65536).contains(&maximum) {
        return Err(AudioFileError::Capacity);
    }
    Ok(vec![0; format.stride() * maximum])
}
fn u16_at(bytes: &[u8], at: usize) -> u16 {
    u16::from_le_bytes(bytes[at..at + 2].try_into().unwrap())
}
fn u32_at(bytes: &[u8], at: usize) -> u32 {
    u32::from_le_bytes(bytes[at..at + 4].try_into().unwrap())
}
/// Prepared bounded interleaved f32 decoder over a seekable WAV stream.
/// File operations block. Unknown chunks are skipped within declared RIFF size;
/// duplicate fmt/data chunks, truncation and inconsistent byte rates reject.
pub struct WaveReader<R> {
    source: R,
    format: WaveFormat,
    frames: u64,
    position: u64,
    offset: u64,
    scratch: Vec<u8>,
}
impl<R: Read + Seek> WaveReader<R> {
    /// Inspect from stream offset zero and allocate bounded scratch.
    /// Trailing bytes outside RIFF are ignored; the declared RIFF must be complete.
    pub fn new(mut source: R, maximum_frames: usize) -> Result<Self, AudioFileError> {
        let actual_end = source.seek(SeekFrom::End(0))?;
        source.seek(SeekFrom::Start(0))?;
        let mut header = [0; 12];
        source.read_exact(&mut header)?;
        if &header[..4] != b"RIFF" || &header[8..] != b"WAVE" {
            return Err(AudioFileError::Unsupported);
        }
        let end = u64::from(u32_at(&header, 4)) + 8;
        if end < 12 || end > actual_end {
            return Err(AudioFileError::InvalidRecord);
        }
        let mut format = None;
        let mut data = None;
        let mut position = 12u64;
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
            let length = u64::from(u32_at(&chunk, 4));
            let body = position + 8;
            let next = body + length + (length & 1);
            if next > end {
                return Err(AudioFileError::InvalidRecord);
            }
            match &chunk[..4] {
                b"fmt " => {
                    if format.is_some() || !(16..=4096).contains(&length) {
                        return Err(AudioFileError::InvalidRecord);
                    }
                    let mut bytes = [0; 16];
                    source.read_exact(&mut bytes)?;
                    let encoding = match (u16_at(&bytes, 0), u16_at(&bytes, 14)) {
                        (1, 16) => WaveEncoding::Pcm16,
                        (1, 24) => WaveEncoding::Pcm24,
                        (1, 32) => WaveEncoding::Pcm32,
                        (3, 32) => WaveEncoding::Float32,
                        _ => return Err(AudioFileError::Unsupported),
                    };
                    let f = WaveFormat {
                        channels: u16_at(&bytes, 2),
                        sample_rate: u32_at(&bytes, 4),
                        encoding,
                    };
                    f.validate()?;
                    if usize::from(u16_at(&bytes, 12)) != f.stride()
                        || u32_at(&bytes, 8) != f.sample_rate * f.stride() as u32
                    {
                        return Err(AudioFileError::InvalidRecord);
                    }
                    format = Some(f);
                }
                b"data" => {
                    if data.is_some() {
                        return Err(AudioFileError::InvalidRecord);
                    }
                    data = Some((body, length));
                }
                _ => {}
            }
            position = next;
        }
        let format = format.ok_or(AudioFileError::InvalidRecord)?;
        let (offset, length) = data.ok_or(AudioFileError::InvalidRecord)?;
        if length % format.stride() as u64 != 0 {
            return Err(AudioFileError::InvalidRecord);
        }
        Ok(Self {
            source,
            format,
            frames: length / format.stride() as u64,
            position: 0,
            offset,
            scratch: scratch(format, maximum_frames)?,
        })
    }
    pub fn format(&self) -> WaveFormat {
        self.format
    }
    pub fn frames(&self) -> u64 {
        self.frames
    }
    pub fn position(&self) -> u64 {
        self.position
    }
    /// Prepared frame scratch capacity for each bounded read call.
    pub fn maximum_frames(&self) -> usize {
        self.scratch.len() / self.format.stride()
    }
    /// Set requested frame; actual stream seek happens on the next read.
    pub fn seek_frame(&mut self, frame: u64) -> Result<(), AudioFileError> {
        if frame > self.frames {
            return Err(AudioFileError::Capacity);
        }
        self.position = frame;
        Ok(())
    }
    /// Decode up to output capacity, preserving tail after returned frame count.
    /// Output length must be a whole number of interleaved frames and within
    /// preparation maximum. I/O failures leave destination and logical position
    /// unchanged; the next read seeks back before retrying.
    pub fn read_frames(&mut self, output: &mut [f32]) -> Result<usize, AudioFileError> {
        let channels = usize::from(self.format.channels);
        if !output.len().is_multiple_of(channels) {
            return Err(AudioFileError::InvalidSamples);
        }
        let requested = output.len() / channels;
        if requested > self.scratch.len() / self.format.stride() {
            return Err(AudioFileError::Capacity);
        }
        let frames = requested.min((self.frames - self.position) as usize);
        if frames == 0 {
            return Ok(0);
        }
        let bytes = frames * self.format.stride();
        self.source.seek(SeekFrom::Start(
            self.offset + self.position * self.format.stride() as u64,
        ))?;
        self.source.read_exact(&mut self.scratch[..bytes])?;
        for (target, source) in output[..frames * channels]
            .iter_mut()
            .zip(self.scratch[..bytes].chunks_exact(self.format.encoding.bytes()))
        {
            *target = match self.format.encoding {
                WaveEncoding::Pcm16 => {
                    f32::from(i16::from_le_bytes(source.try_into().unwrap())) / 32768.
                }
                WaveEncoding::Pcm24 => {
                    let value = i32::from_le_bytes([
                        source[0],
                        source[1],
                        source[2],
                        if source[2] & 128 != 0 { 255 } else { 0 },
                    ]);
                    value as f32 / 8388608.
                }
                WaveEncoding::Pcm32 => {
                    i32::from_le_bytes(source.try_into().unwrap()) as f32 / 2147483648.
                }
                WaveEncoding::Float32 => f32::from_le_bytes(source.try_into().unwrap()),
            };
        }
        self.position += frames as u64;
        Ok(frames)
    }
}
/// Bounded interleaved WAV recorder on a control/worker thread.
/// Output must initially be empty. Call `finish` explicitly to patch sizes;
/// Drop never performs file I/O. An I/O failure poisons further writing/finalization.
pub struct WaveWriter<W> {
    destination: W,
    format: WaveFormat,
    data_bytes: u32,
    header_bytes: u32,
    scratch: Vec<u8>,
    failed: bool,
}
impl<W: Write + Seek> WaveWriter<W> {
    pub fn new(
        mut destination: W,
        format: WaveFormat,
        maximum_frames: usize,
    ) -> Result<Self, AudioFileError> {
        let scratch = scratch(format, maximum_frames)?;
        if destination.seek(SeekFrom::End(0))? != 0 {
            return Err(AudioFileError::InvalidRecord);
        }
        destination.seek(SeekFrom::Start(0))?;
        let floating = format.encoding == WaveEncoding::Float32;
        let header_bytes = if floating { 58 } else { 44 };
        let mut header = Vec::with_capacity(header_bytes as usize);
        header.extend_from_slice(b"RIFF");
        header.extend((header_bytes - 8u32).to_le_bytes());
        header.extend_from_slice(b"WAVEfmt ");
        header.extend((if floating { 18u32 } else { 16u32 }).to_le_bytes());
        header.extend(format.encoding.tag().to_le_bytes());
        header.extend(format.channels.to_le_bytes());
        header.extend(format.sample_rate.to_le_bytes());
        header.extend((format.sample_rate * format.stride() as u32).to_le_bytes());
        header.extend((format.stride() as u16).to_le_bytes());
        header.extend((format.encoding.bytes() as u16 * 8).to_le_bytes());
        if floating {
            header.extend(0u16.to_le_bytes());
            header.extend_from_slice(b"fact");
            header.extend(4u32.to_le_bytes());
            header.extend(0u32.to_le_bytes());
        }
        header.extend_from_slice(b"data");
        header.extend(0u32.to_le_bytes());
        destination.write_all(&header)?;
        Ok(Self {
            destination,
            format,
            data_bytes: 0,
            header_bytes,
            scratch,
            failed: false,
        })
    }
    pub fn frames(&self) -> u64 {
        u64::from(self.data_bytes) / self.format.stride() as u64
    }
    /// Validate complete block before writing. PCM clips finite values to [-1,1]
    /// and rounds to nearest integer; NaN/infinite PCM input rejects. Float32
    /// preserves bit patterns. Failed file writes can leave partial bytes; discard
    /// that file. Prefix repair/recovery is not attempted automatically.
    pub fn write_frames(&mut self, input: &[f32]) -> Result<(), AudioFileError> {
        if self.failed {
            return Err(AudioFileError::Poisoned);
        }
        if !input
            .len()
            .is_multiple_of(usize::from(self.format.channels))
        {
            return Err(AudioFileError::InvalidSamples);
        }
        let bytes = input
            .len()
            .checked_mul(self.format.encoding.bytes())
            .ok_or(AudioFileError::Capacity)?;
        if bytes > self.scratch.len() {
            return Err(AudioFileError::Capacity);
        }
        let total = u64::from(self.data_bytes) + bytes as u64;
        if total + u64::from(self.header_bytes - 8) + (total & 1) > u64::from(u32::MAX) {
            return Err(AudioFileError::Capacity);
        }
        if self.format.encoding != WaveEncoding::Float32 && input.iter().any(|v| !v.is_finite()) {
            return Err(AudioFileError::InvalidSamples);
        }
        for (value, output) in input
            .iter()
            .zip(self.scratch[..bytes].chunks_exact_mut(self.format.encoding.bytes()))
        {
            if self.format.encoding == WaveEncoding::Float32 {
                output.copy_from_slice(&value.to_le_bytes());
            } else {
                let bits = self.format.encoding.bytes() * 8;
                let scale = (1u64 << (bits - 1)) as f64;
                let integer = (f64::from(value.clamp(-1., 1.)) * scale)
                    .round()
                    .clamp(-scale, scale - 1.) as i32;
                output.copy_from_slice(&integer.to_le_bytes()[..output.len()]);
            }
        }
        if let Err(error) = self.destination.write_all(&self.scratch[..bytes]) {
            self.failed = true;
            return Err(error.into());
        }
        self.data_bytes = total as u32;
        Ok(())
    }
    /// Patch RIFF/data sizes and flush; add an odd-data pad byte when needed.
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
            .write_all(&(self.header_bytes - 8 + self.data_bytes + pad).to_le_bytes())?;
        self.destination
            .seek(SeekFrom::Start(u64::from(self.header_bytes - 4)))?;
        self.destination.write_all(&self.data_bytes.to_le_bytes())?;
        if self.format.encoding == WaveEncoding::Float32 {
            self.destination.seek(SeekFrom::Start(46))?;
            self.destination
                .write_all(&(self.frames() as u32).to_le_bytes())?;
        }
        self.destination.seek(SeekFrom::End(0))?;
        self.destination.flush()?;
        Ok(self.destination)
    }
}
