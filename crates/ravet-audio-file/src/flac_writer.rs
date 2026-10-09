//! Incremental single-worker FLAC output with explicit finalization.
use crate::{AudioFileError as Error, FlacFormat};
use flacenc::{
    bitsink::ByteSink,
    component::{BitRepr, Stream, StreamInfo},
    config::Encoder,
    error::{Verified, Verify},
    source::{Context, Fill, FrameBuf},
};
use std::io::{Seek, SeekFrom, Write};
const BLOCK: usize = 4096;
/// Optional incremental PCM16/24 FLAC writer for an empty seekable destination.
/// Accepts at most 65536 whole interleaved frames per write and retains only one
/// 4096-frame pending block, one backend frame buffer and one serialized frame.
/// Backend working allocations remain its contract; all methods are worker-only.
/// Invalid input preserves prior state/output. Encoding/I/O/capacity failure after
/// acceptance poisons the writer; output may contain an irreversible prefix.
/// Finish emits the sole short final block, rewrites STREAMINFO count/MD5/frame
/// sizes and flushes before returning ownership. Drop does not finalize; partial
/// files are not complete recordings. No float quantization, path/thread/device
/// policy, implicit file truncation or physical recording/deadline claim.
pub struct FlacWriter<W> {
    destination: W,
    format: FlacFormat,
    maximum_frames: u64,
    maximum_bytes: u64,
    accepted_frames: u64,
    written_bytes: u64,
    frame_number: usize,
    pending: Vec<i32>,
    frame: FrameBuf,
    context: Context,
    info: StreamInfo,
    config: Verified<Encoder>,
    fallback: Verified<Encoder>,
    poisoned: bool,
}
impl<W: Write + Seek> FlacWriter<W> {
    /// Require a destination of length zero, 1..8 channels/1..96000 Hz, a maximum
    /// frame count 1..2^36-1 fitting usize, and byte budget at least 42 (header).
    /// Validation/preparation precede header output; native I/O errors may mutate
    /// destination position or leave a partial header. The sink is caller-owned.
    pub fn new(
        mut destination: W,
        format: FlacFormat,
        maximum_frames: u64,
        maximum_bytes: u64,
    ) -> Result<Self, Error> {
        if !(1..=8).contains(&format.channels) || !(1..=96000).contains(&format.sample_rate) {
            return Err(Error::Unsupported);
        }
        if maximum_frames == 0
            || maximum_frames >= 1u64 << 36
            || usize::try_from(maximum_frames).is_err()
            || maximum_bytes < 42
        {
            return Err(Error::Capacity);
        }
        let channels = usize::from(format.channels);
        let mut config = Encoder::default();
        config.block_size = BLOCK;
        config.multithread = false;
        let config = config.into_verified().map_err(|_| Error::Unsupported)?;
        let mut fallback = Encoder::default();
        fallback.multithread = false;
        fallback.subframe_coding.use_fixed = false;
        fallback.subframe_coding.use_lpc = false;
        let fallback = fallback.into_verified().map_err(|_| Error::Unsupported)?;
        let frame = FrameBuf::with_size(channels, BLOCK).map_err(|_| Error::Unsupported)?;
        let context = Context::new(format.encoding.bits(), channels);
        let mut info = StreamInfo::new(
            format.sample_rate as usize,
            channels,
            format.encoding.bits(),
        )
        .map_err(|_| Error::Unsupported)?;
        info.set_block_sizes(BLOCK, BLOCK)
            .map_err(|_| Error::InvalidRecord)?;
        info.set_frame_sizes(0, 0)
            .map_err(|_| Error::InvalidRecord)?;
        let pending = Vec::with_capacity(channels * BLOCK);
        if destination.seek(SeekFrom::End(0))? != 0 {
            return Err(Error::InvalidRecord);
        }
        destination.seek(SeekFrom::Start(0))?;
        let header = header(&info)?;
        destination.write_all(&header)?;
        Ok(Self {
            destination,
            format,
            maximum_frames,
            maximum_bytes,
            accepted_frames: 0,
            written_bytes: header.len() as u64,
            frame_number: 0,
            pending,
            frame,
            context,
            info,
            config,
            fallback,
            poisoned: false,
        })
    }
    pub fn format(&self) -> FlacFormat {
        self.format
    }
    /// Accepted frames, including retained pending input and any poisoned prefix.
    pub fn accepted_frames(&self) -> u64 {
        self.accepted_frames
    }
    pub fn pending_frames(&self) -> usize {
        self.pending.len() / usize::from(self.format.channels)
    }
    /// Header plus complete serialized frames submitted successfully to the sink.
    /// A failed write may have submitted an additional partial frame.
    pub fn written_bytes(&self) -> u64 {
        self.written_bytes
    }
    pub fn is_poisoned(&self) -> bool {
        self.poisoned
    }
    /// Validate the complete call before acceptance, then emit full fixed blocks.
    /// Empty writes are no-ops. Per-call/total input bounds and signed precision
    /// violations preserve a usable writer. Output-budget failure during encoding
    /// poisons; earlier successful output and accepted pending samples remain.
    pub fn write_frames(&mut self, samples: &[i32]) -> Result<(), Error> {
        if self.poisoned {
            return Err(Error::Poisoned);
        }
        let channels = usize::from(self.format.channels);
        if !samples.len().is_multiple_of(channels) {
            return Err(Error::InvalidSamples);
        }
        let frames = samples.len() / channels;
        if frames > 65536 || frames as u64 > self.maximum_frames - self.accepted_frames {
            return Err(Error::Capacity);
        }
        let limit = 1i32 << (self.format.encoding.bits() - 1);
        if samples.iter().any(|v| *v < -limit || *v >= limit) {
            return Err(Error::InvalidSamples);
        }
        let mut offset = 0;
        while offset < samples.len() {
            let count = (BLOCK * channels - self.pending.len()).min(samples.len() - offset);
            self.pending
                .extend_from_slice(&samples[offset..offset + count]);
            self.accepted_frames += (count / channels) as u64;
            offset += count;
            if self.pending.len() == BLOCK * channels
                && let Err(error) = self.emit()
            {
                self.poisoned = true;
                return Err(error);
            }
        }
        Ok(())
    }
    fn emit(&mut self) -> Result<(), Error> {
        self.frame
            .fill_interleaved(&self.pending)
            .map_err(|_| Error::InvalidRecord)?;
        let mut frame = flacenc::encode_fixed_size_frame(
            &self.config,
            &self.frame,
            self.frame_number,
            &self.info,
        )
        .map_err(|_| Error::InvalidRecord)?;
        // Some high-dynamic-range prediction choices produce excessive residual
        // coding. Never serialize beyond a raw-PCM-sized frame; use the backend's
        // constant/verbatim profile instead, without changing samples or offsets.
        let raw_bound = self.pending.len() * self.format.encoding.bits() + 1024;
        if frame.count_bits() > raw_bound {
            frame = flacenc::encode_fixed_size_frame(
                &self.fallback,
                &self.frame,
                self.frame_number,
                &self.info,
            )
            .map_err(|_| Error::InvalidRecord)?;
        }
        let bits = frame.count_bits();
        if bits > 262144 * 8 {
            return Err(Error::Capacity);
        }
        let bytes = bits.div_ceil(8) as u64;
        if bytes > self.maximum_bytes - self.written_bytes {
            return Err(Error::Capacity);
        }
        let mut sink = ByteSink::with_capacity(bits);
        frame.write(&mut sink).map_err(|_| Error::InvalidRecord)?;
        self.destination.write_all(sink.as_slice())?;
        self.written_bytes += sink.as_slice().len() as u64;
        if self.frame_number == 0 {
            self.info
                .set_frame_sizes(bytes as usize, bytes as usize)
                .map_err(|_| Error::InvalidRecord)?;
        }
        self.info.update_frame_info(&frame);
        self.context
            .fill_interleaved(&self.pending)
            .map_err(|_| Error::InvalidRecord)?;
        self.frame_number += 1;
        self.pending.clear();
        Ok(())
    }
    /// Consume a nonempty recording; output/header/flush failure returns an error
    /// and drops the sink without a complete-file guarantee. No retry/rollback.
    pub fn finish(mut self) -> Result<W, Error> {
        if self.poisoned {
            return Err(Error::Poisoned);
        }
        if self.accepted_frames == 0 {
            return Err(Error::InvalidSamples);
        }
        if !self.pending.is_empty() {
            self.emit()?;
        }
        self.info
            .set_block_sizes(BLOCK, BLOCK)
            .map_err(|_| Error::InvalidRecord)?;
        self.info.set_total_samples(self.context.total_samples());
        self.info.set_md5_digest(&self.context.md5_digest());
        let header = header(&self.info)?;
        self.destination.seek(SeekFrom::Start(0))?;
        self.destination.write_all(&header)?;
        self.destination.seek(SeekFrom::Start(self.written_bytes))?;
        self.destination.flush()?;
        Ok(self.destination)
    }
}
fn header(info: &StreamInfo) -> Result<Vec<u8>, Error> {
    let stream = Stream::with_stream_info(info.clone());
    let mut sink = ByteSink::with_capacity(42 * 8);
    stream.write(&mut sink).map_err(|_| Error::InvalidRecord)?;
    if sink.as_slice().len() != 42 {
        return Err(Error::InvalidRecord);
    }
    Ok(sink.into_inner())
}
