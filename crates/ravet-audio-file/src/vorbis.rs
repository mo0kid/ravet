//! Optional bounded single-worker Ogg Vorbis encoding with explicit finish.
use crate::AudioFileError as Error;
use std::{
    cell::RefCell,
    io::{self, Write},
    num::{NonZeroU8, NonZeroU32},
    rc::Rc,
};
use vorbis_rs::{
    VorbisBitrateManagementStrategy, VorbisEncoder, VorbisEncoderBuilder, VorbisError,
};
const BATCH: usize = 4096;
const MAX_FRAMES: u64 = 16_777_216;
const MAX_BYTES: u64 = 67_108_864;

/// Explicit mono (8000..96000 Hz) or stereo (16000..96000 Hz) Vorbis I channel
/// order, perceptual quality and Ogg
/// logical-stream serial. Quality is finite -0.2..=1; no random serial is chosen.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct VorbisFormat {
    pub channels: u16,
    pub sample_rate: u32,
    pub quality: f32,
    pub stream_serial: i32,
}
impl VorbisFormat {
    fn validate(self) -> Result<(), Error> {
        let minimum_rate = if self.channels == 2 { 16000 } else { 8000 };
        if !(1..=2).contains(&self.channels)
            || !(minimum_rate..=96000).contains(&self.sample_rate)
            || !self.quality.is_finite()
            || !(-0.2..=1.).contains(&self.quality)
        {
            return Err(Error::Unsupported);
        }
        Ok(())
    }
}
struct Destination<W> {
    sink: Option<W>,
    written: u64,
    maximum: u64,
    blocked: bool,
    capacity_failure: bool,
}
struct SinkGate<W>(Rc<RefCell<Destination<W>>>);
impl<W> Clone for SinkGate<W> {
    fn clone(&self) -> Self {
        Self(Rc::clone(&self.0))
    }
}
impl<W: Write> Write for SinkGate<W> {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        let mut state = self.0.borrow_mut();
        if state.blocked {
            return Err(io::Error::other("Vorbis sink retired or poisoned"));
        }
        if bytes.len() as u64 > state.maximum - state.written {
            state.capacity_failure = true;
            state.blocked = true;
            return Err(io::Error::other("Vorbis output budget exceeded"));
        }
        let result = state.sink.as_mut().expect("owned sink").write(bytes);
        match result {
            Ok(0) if !bytes.is_empty() => {
                state.blocked = true;
                Err(io::Error::new(
                    io::ErrorKind::WriteZero,
                    "Vorbis sink accepted no bytes",
                ))
            }
            Ok(count) => {
                state.written += count as u64;
                Ok(count)
            }
            Err(error) => {
                state.blocked = true;
                Err(error)
            }
        }
    }
    fn flush(&mut self) -> io::Result<()> {
        let mut state = self.0.borrow_mut();
        if state.blocked {
            return Err(io::Error::other("Vorbis sink retired or poisoned"));
        }
        let result = state.sink.as_mut().expect("owned sink").flush();
        if result.is_err() {
            state.blocked = true;
        }
        result
    }
}
// Retain at most one physical page; emit a separate zero-granule priming packet
// page so single-page short streams have unambiguous start/end padding. A first
// packet spanning pages rejects rather than guessing timestamps or sync-searching.
const MAX_PAGE: usize = 65_307;
struct PageSink<W> {
    sink: SinkGate<W>,
    page: Vec<u8>,
    input_sequence: u32,
    output_sequence: u32,
    first_audio: bool,
    header_packets: u8,
    ended: bool,
}
impl<W: Write> PageSink<W> {
    fn complete(&self) -> bool {
        self.page.is_empty() && !self.first_audio && self.header_packets == 3 && self.ended
    }
    fn new(sink: SinkGate<W>) -> Self {
        Self {
            sink,
            page: Vec::with_capacity(MAX_PAGE),
            input_sequence: 0,
            output_sequence: 0,
            first_audio: true,
            header_packets: 0,
            ended: false,
        }
    }
    fn emit(&mut self) -> io::Result<()> {
        let page = &self.page;
        let segments = usize::from(page[26]);
        let lacing = &page[27..27 + segments];
        let body = &page[27 + segments..];
        let sequence = u32::from_le_bytes(page[18..22].try_into().unwrap());
        if sequence != self.input_sequence {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "unexpected Vorbis page sequence",
            ));
        }
        self.input_sequence = self
            .input_sequence
            .checked_add(1)
            .ok_or_else(|| io::Error::other("Vorbis page sequence overflow"))?;
        let flags = page[5];
        let granule = u64::from_le_bytes(page[6..14].try_into().unwrap());
        if self.header_packets < 3 {
            if flags & 5 != 0 {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "continued or EOS Vorbis header page",
                ));
            }
            if (flags & 2 != 0) != (self.header_packets == 0) {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "invalid Vorbis header BOS",
                ));
            }
            let mut begin = 0;
            let mut packet_begin = 0;
            let mut completed = 0;
            for (index, size) in lacing.iter().enumerate() {
                begin += usize::from(*size);
                if *size < 255 {
                    if self.header_packets == 3 {
                        return Err(io::Error::new(
                            io::ErrorKind::InvalidData,
                            "mixed Vorbis header/audio page",
                        ));
                    }
                    let packet = &body[packet_begin..begin];
                    let expected_kind = 1 + self.header_packets * 2;
                    if packet.len() < 7 || packet[0] != expected_kind || &packet[1..7] != b"vorbis"
                    {
                        return Err(io::Error::new(
                            io::ErrorKind::InvalidData,
                            "invalid Vorbis header packet",
                        ));
                    }
                    self.header_packets += 1;
                    completed += 1;
                    packet_begin = begin;
                    if self.header_packets == 3 && index + 1 < lacing.len() {
                        return Err(io::Error::new(
                            io::ErrorKind::InvalidData,
                            "mixed Vorbis header/audio page",
                        ));
                    }
                }
            }
            if packet_begin != body.len() || lacing.last().is_some_and(|size| *size == 255) {
                let mut state = self.sink.0.borrow_mut();
                state.capacity_failure = true;
                state.blocked = true;
                return Err(io::Error::other(
                    "Vorbis header packet spans physical pages",
                ));
            }
            if flags & 2 != 0 && completed != 1 {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "nonisolated Vorbis identification packet",
                ));
            }
            write_page(
                &mut self.sink,
                &page[..27],
                lacing,
                body,
                flags,
                granule,
                self.output_sequence,
            )?;
            self.output_sequence = self
                .output_sequence
                .checked_add(1)
                .ok_or_else(|| io::Error::other("Vorbis output page sequence overflow"))?;
        } else if self.first_audio {
            if flags & 3 != 0 {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "continued first Vorbis audio page",
                ));
            }
            let first = lacing.iter().position(|size| *size < 255);
            let Some(first) = first else {
                let mut state = self.sink.0.borrow_mut();
                state.capacity_failure = true;
                state.blocked = true;
                return Err(io::Error::other(
                    "Vorbis priming packet exceeds one physical page",
                ));
            };
            let first_segments = first + 1;
            let first_bytes: usize = lacing[..first_segments]
                .iter()
                .map(|size| usize::from(*size))
                .sum();
            if first_segments < segments {
                write_page(
                    &mut self.sink,
                    &page[..27],
                    &lacing[..first_segments],
                    &body[..first_bytes],
                    0,
                    0,
                    self.output_sequence,
                )?;
                self.output_sequence = self
                    .output_sequence
                    .checked_add(1)
                    .ok_or_else(|| io::Error::other("Vorbis output page sequence overflow"))?;
                write_page(
                    &mut self.sink,
                    &page[..27],
                    &lacing[first_segments..],
                    &body[first_bytes..],
                    flags,
                    granule,
                    self.output_sequence,
                )?;
                self.output_sequence = self
                    .output_sequence
                    .checked_add(1)
                    .ok_or_else(|| io::Error::other("Vorbis output page sequence overflow"))?;
            } else {
                if granule != 0 || flags & 4 != 0 {
                    return Err(io::Error::new(
                        io::ErrorKind::InvalidData,
                        "unexpected sole Vorbis priming packet timestamp",
                    ));
                }
                write_page(
                    &mut self.sink,
                    &page[..27],
                    lacing,
                    body,
                    flags,
                    granule,
                    self.output_sequence,
                )?;
                self.output_sequence = self
                    .output_sequence
                    .checked_add(1)
                    .ok_or_else(|| io::Error::other("Vorbis output page sequence overflow"))?;
            }
            self.first_audio = false;
        } else {
            write_page(
                &mut self.sink,
                &page[..27],
                lacing,
                body,
                flags,
                granule,
                self.output_sequence,
            )?;
            self.output_sequence = self
                .output_sequence
                .checked_add(1)
                .ok_or_else(|| io::Error::other("Vorbis output page sequence overflow"))?;
        }
        self.ended = flags & 4 != 0;
        Ok(())
    }
}
impl<W: Write> PageSink<W> {
    fn write_inner(&mut self, bytes: &[u8]) -> io::Result<usize> {
        if self.sink.0.borrow().blocked {
            return Err(io::Error::other("Vorbis sink retired or poisoned"));
        }
        if bytes.len() > MAX_PAGE - self.page.len() {
            self.sink.0.borrow_mut().capacity_failure = true;
            return Err(io::Error::other("Vorbis physical page exceeds bound"));
        }
        self.page.extend_from_slice(bytes);
        if self.page.len() >= 27 {
            if &self.page[..4] != b"OggS" || self.page[4] != 0 {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "invalid Vorbis physical page",
                ));
            }
            let header = 27 + usize::from(self.page[26]);
            if self.page.len() >= header {
                let body: usize = self.page[27..header]
                    .iter()
                    .map(|size| usize::from(*size))
                    .sum();
                let expected = header + body;
                if self.page.len() > expected {
                    return Err(io::Error::new(
                        io::ErrorKind::InvalidData,
                        "multiple Vorbis pages in one write",
                    ));
                }
                if self.page.len() == expected {
                    self.emit()?;
                    self.page.clear();
                }
            }
        }
        Ok(bytes.len())
    }
    fn flush_inner(&mut self) -> io::Result<()> {
        if !self.page.is_empty() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "incomplete Vorbis physical page",
            ));
        }
        self.sink.flush()
    }
}
impl<W: Write> Write for PageSink<W> {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        let result = self.write_inner(bytes);
        if result.is_err() {
            self.sink.0.borrow_mut().blocked = true;
        }
        result
    }
    fn flush(&mut self) -> io::Result<()> {
        let result = self.flush_inner();
        if result.is_err() {
            self.sink.0.borrow_mut().blocked = true;
        }
        result
    }
}

fn write_page<W: Write>(
    sink: &mut SinkGate<W>,
    base: &[u8],
    lacing: &[u8],
    body: &[u8],
    flags: u8,
    granule: u64,
    sequence: u32,
) -> io::Result<()> {
    let length = 27 + lacing.len() + body.len();
    {
        let mut state = sink.0.borrow_mut();
        if length as u64 > state.maximum - state.written {
            state.capacity_failure = true;
            state.blocked = true;
            return Err(io::Error::other(
                "Vorbis transformed output budget exceeded",
            ));
        }
    }
    let mut header = [0_u8; 282];
    header[..27].copy_from_slice(base);
    header[5] = flags;
    header[6..14].copy_from_slice(&granule.to_le_bytes());
    header[18..22].copy_from_slice(&sequence.to_le_bytes());
    header[22..26].fill(0);
    header[26] = lacing.len() as u8;
    header[27..27 + lacing.len()].copy_from_slice(lacing);
    let mut crc = 0_u32;
    for byte in header[..27 + lacing.len()].iter().chain(body) {
        crc ^= u32::from(*byte) << 24;
        for _ in 0..8 {
            crc = if crc & 0x80000000 != 0 {
                (crc << 1) ^ 0x04c11db7
            } else {
                crc << 1
            };
        }
    }
    header[22..26].copy_from_slice(&crc.to_le_bytes());
    sink.write_all(&header[..27 + lacing.len()])?;
    sink.write_all(body)
}
/// Incremental lossy Ogg Vorbis writer. Construct/use/finish/drop on one worker;
/// the safe backend is not Send/Sync. All codec/sink methods can allocate/block
/// and belong outside callbacks. Interleaved finite [-1,1] input is explicitly
/// rejected on overload; no clipping, rate conversion, device/clock/path policy.
/// Wrapper retains prepared 4096-frame planar scratch and one physical Ogg page
/// (at most 65307 bytes). Backend working storage remains its contract. Each
/// write accepts at most 65536 complete frames. Headers and the priming packet
/// must terminate within a physical page; continued headers, mixed header/audio
/// pages and spanning priming packets reject rather than guessing timestamps.
/// Whole-call preflight failures preserve state; encoding/I/O/output-capacity
/// failure after acceptance poisons and may leave an irreversible encoded prefix.
/// Sink receives no framework writes/flushes from Drop, including after failure.
/// Caller-provided sink Drop remains its own contract. Only explicit nonempty
/// finish writes EOS, flushes and returns the sink; a failed finish consumes it.
/// No exact PCM/perceptual-quality/deadline claim. Automatic prepared capture
/// uses [`crate::VorbisRecordingWorker`], which creates this codec locally.
///
/// ```compile_fail
/// use ravet_audio_file::{VorbisFormat, VorbisWriter};
/// let writer = VorbisWriter::new(Vec::new(), VorbisFormat {
///     channels: 1, sample_rate: 48000, quality: 0.5, stream_serial: 1,
/// }, 4096, 65536).unwrap();
/// std::thread::spawn(move || drop(writer));
/// ```
pub struct VorbisWriter<W: Write> {
    encoder: Option<VorbisEncoder<PageSink<W>>>,
    destination: Rc<RefCell<Destination<W>>>,
    planar: Vec<Vec<f32>>,
    format: VorbisFormat,
    maximum_frames: u64,
    accepted_frames: u64,
    poisoned: bool,
}
pub(crate) struct VorbisRecovery<W> {
    pub(crate) sink: W,
    pub(crate) error: Option<Error>,
    pub(crate) finalized: bool,
    pub(crate) accepted_frames: u64,
    pub(crate) written_bytes: u64,
}
impl<W: Write> VorbisWriter<W> {
    pub(crate) fn validate_configuration(
        format: VorbisFormat,
        maximum_frames: u64,
        maximum_bytes: u64,
    ) -> Result<(), Error> {
        format.validate()?;
        if !(1..=MAX_FRAMES).contains(&maximum_frames) || !(1..=MAX_BYTES).contains(&maximum_bytes)
        {
            return Err(Error::Capacity);
        }
        Ok(())
    }
    /// Validate mono 8000..96000/stereo 16000..96000 Hz, finite quality -0.2..1,
    /// total frames
    /// 1..16777216 and output bytes 1..67108864 before preparation/header output.
    /// Writes start at the caller's current sink position; no seek/truncate occurs.
    /// Header I/O/capacity errors may leave a partial prefix and consume the sink.
    /// Caller serial identifies one logical stream; chaining/multiplexing requires
    /// distinct caller-selected serials and is outside this writer's contract.
    pub fn new(
        destination: W,
        format: VorbisFormat,
        maximum_frames: u64,
        maximum_bytes: u64,
    ) -> Result<Self, Error> {
        Self::initialize_reclaim(destination, format, maximum_frames, maximum_bytes)
            .map_err(|recovery| recovery.error.expect("initialization error"))
    }
    pub(crate) fn initialize_reclaim(
        destination: W,
        format: VorbisFormat,
        maximum_frames: u64,
        maximum_bytes: u64,
    ) -> Result<Self, VorbisRecovery<W>> {
        if let Err(error) = Self::validate_configuration(format, maximum_frames, maximum_bytes) {
            return Err(VorbisRecovery {
                sink: destination,
                error: Some(error),
                finalized: false,
                accepted_frames: 0,
                written_bytes: 0,
            });
        }
        let channels = usize::from(format.channels);
        let planar = (0..channels).map(|_| vec![0.; BATCH]).collect();
        let destination = Rc::new(RefCell::new(Destination {
            sink: Some(destination),
            written: 0,
            maximum: maximum_bytes,
            blocked: false,
            capacity_failure: false,
        }));
        let mut builder = VorbisEncoderBuilder::new_with_serial(
            NonZeroU32::new(format.sample_rate).unwrap(),
            NonZeroU8::new(format.channels as u8).unwrap(),
            PageSink::new(SinkGate(Rc::clone(&destination))),
            format.stream_serial,
        );
        builder.bitrate_management_strategy(VorbisBitrateManagementStrategy::QualityVbr {
            target_quality: format.quality,
        });
        builder.minimum_page_data_size(Some(0));
        let encoder = match builder.build() {
            Ok(encoder) => encoder,
            Err(error) => {
                let error = map_error(error, &destination);
                let mut state = destination.borrow_mut();
                state.blocked = true;
                let written_bytes = state.written;
                let sink = state.sink.take().expect("owned sink");
                return Err(VorbisRecovery {
                    sink,
                    error: Some(error),
                    finalized: false,
                    accepted_frames: 0,
                    written_bytes,
                });
            }
        };
        Ok(Self {
            encoder: Some(encoder),
            destination,
            planar,
            format,
            maximum_frames,
            accepted_frames: 0,
            poisoned: false,
        })
    }
    pub fn format(&self) -> VorbisFormat {
        self.format
    }
    /// Submitted frames, including a failing accepted codec batch. Not finalized.
    pub fn accepted_frames(&self) -> u64 {
        self.accepted_frames
    }
    /// Bytes actually accepted by the sink, including any failed partial write.
    pub fn written_bytes(&self) -> u64 {
        self.destination.borrow().written
    }
    pub fn is_poisoned(&self) -> bool {
        self.poisoned
    }
    /// Validate shape/finite [-1,1]/per-call and total bounds before acceptance.
    /// Empty input is a no-op; it does not signal encoder EOS. Backend batches
    /// stay at most 4096 frames regardless of call size.
    pub fn write_frames(&mut self, samples: &[f32]) -> Result<(), Error> {
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
        if samples
            .iter()
            .any(|sample| !sample.is_finite() || !(-1. ..=1.).contains(sample))
        {
            return Err(Error::InvalidSamples);
        }
        for interleaved in samples.chunks(BATCH * channels) {
            let count = interleaved.len() / channels;
            for (frame, values) in interleaved.chunks_exact(channels).enumerate() {
                for (channel, value) in values.iter().enumerate() {
                    self.planar[channel][frame] = *value;
                }
            }
            self.accepted_frames += count as u64;
            let encoder = self.encoder.as_mut().expect("active encoder");
            let result = if channels == 1 {
                encoder.encode_audio_block([&self.planar[0][..count]])
            } else {
                encoder.encode_audio_block([&self.planar[0][..count], &self.planar[1][..count]])
            };
            if let Err(error) = result {
                self.poisoned = true;
                self.destination.borrow_mut().blocked = true;
                return Err(map_error(error, &self.destination));
            }
        }
        Ok(())
    }
    /// Explicit nonempty EOS and sink flush. Failure consumes the destination,
    /// with no complete-file/retry/rollback guarantee. Drop never finishes it.
    pub fn finish(self) -> Result<W, Error> {
        let recovery = self.finish_reclaim();
        match recovery.error {
            Some(error) => Err(error),
            None => Ok(recovery.sink),
        }
    }
    pub(crate) fn abort_reclaim(mut self, error: Option<Error>) -> VorbisRecovery<W> {
        self.destination.borrow_mut().blocked = true;
        self.encoder.take();
        let mut state = self.destination.borrow_mut();
        let recovery = VorbisRecovery {
            sink: state.sink.take().expect("owned sink"),
            error,
            finalized: false,
            accepted_frames: self.accepted_frames,
            written_bytes: state.written,
        };
        drop(state);
        recovery
    }
    pub(crate) fn finish_reclaim(mut self) -> VorbisRecovery<W> {
        if self.poisoned {
            return self.abort_reclaim(Some(Error::Poisoned));
        }
        if self.accepted_frames == 0 {
            return self.abort_reclaim(Some(Error::InvalidSamples));
        }
        let result = self.encoder.take().expect("active encoder").finish();
        self.destination.borrow_mut().blocked = true;
        let mut error = match result {
            Ok(pages) if pages.complete() => None,
            Ok(_) => Some(Error::InvalidRecord),
            Err(error) => Some(map_error(error, &self.destination)),
        };
        let mut sink = self
            .destination
            .borrow_mut()
            .sink
            .take()
            .expect("owned sink");
        if error.is_none()
            && let Err(failure) = sink.flush()
        {
            error = Some(Error::Io(failure));
        }
        let written_bytes = self.destination.borrow().written;
        VorbisRecovery {
            sink,
            finalized: error.is_none(),
            error,
            accepted_frames: self.accepted_frames,
            written_bytes,
        }
    }
}
impl<W: Write> Drop for VorbisWriter<W> {
    fn drop(&mut self) {
        self.destination.borrow_mut().blocked = true;
        self.encoder.take();
    }
}
fn map_error<W>(error: VorbisError, destination: &Rc<RefCell<Destination<W>>>) -> Error {
    if destination.borrow().capacity_failure {
        return Error::Capacity;
    }
    match error {
        VorbisError::Io(error) => Error::Io(error),
        _ => Error::InvalidRecord,
    }
}
/// Encode a nonempty complete interleaved finite [-1,1] mono/stereo clip. Input
/// <=4194304 f32 samples and output <=64MiB; lossy worker-only Vorbis encoding.
/// Explicit format/quality/serial, no implicit RNG/thread/path/rate/device policy.
pub fn encode_vorbis(samples: &[f32], format: VorbisFormat) -> Result<Vec<u8>, Error> {
    format.validate()?;
    if samples.is_empty() || samples.len() > 4_194_304 {
        return Err(Error::Capacity);
    }
    let channels = usize::from(format.channels);
    if !samples.len().is_multiple_of(channels)
        || samples
            .iter()
            .any(|sample| !sample.is_finite() || !(-1. ..=1.).contains(sample))
    {
        return Err(Error::InvalidSamples);
    }
    let mut writer = VorbisWriter::new(
        Vec::new(),
        format,
        (samples.len() / channels) as u64,
        MAX_BYTES,
    )?;
    for chunk in samples.chunks(65536 * channels) {
        writer.write_frames(chunk)?;
    }
    writer.finish()
}

#[cfg(test)]
mod page_tests {
    use super::*;
    fn sink() -> PageSink<Vec<u8>> {
        PageSink::new(SinkGate(Rc::new(RefCell::new(Destination {
            sink: Some(Vec::new()),
            written: 0,
            maximum: MAX_BYTES,
            blocked: false,
            capacity_failure: false,
        }))))
    }
    fn page(packet: &[u8], kind: u8, sequence: u32, granule: u64) -> Vec<u8> {
        assert!(packet.len() < 255);
        let mut header = [0; 27];
        header[..4].copy_from_slice(b"OggS");
        let mut output = SinkGate(Rc::new(RefCell::new(Destination {
            sink: Some(Vec::new()),
            written: 0,
            maximum: MAX_BYTES,
            blocked: false,
            capacity_failure: false,
        })));
        write_page(
            &mut output,
            &header,
            &[packet.len() as u8],
            packet,
            kind,
            granule,
            sequence,
        )
        .unwrap();
        output.0.borrow_mut().sink.take().unwrap()
    }
    fn headers<W: Write>(sink: &mut PageSink<W>) {
        sink.write_all(&page(b"\x01vorbis", 2, 0, 0)).unwrap();
        sink.write_all(&page(b"\x03vorbis", 0, 1, 0)).unwrap();
        sink.write_all(&page(b"\x05vorbis", 0, 2, 0)).unwrap();
    }
    #[test]
    fn separate_headers_and_already_isolated_priming_page_need_no_added_page() {
        let mut sink = sink();
        headers(&mut sink);
        let first = page(&[0], 0, 3, 0);
        sink.write_all(&first).unwrap();
        assert!(!sink.complete());
        let last = page(&[0], 4, 4, 1);
        sink.write_all(&last).unwrap();
        assert!(sink.complete());
        let output = sink.sink.0.borrow();
        assert!(
            output
                .sink
                .as_ref()
                .unwrap()
                .ends_with(&[first, last].concat())
        );
        assert_eq!((sink.input_sequence, sink.output_sequence), (5, 5));
    }
    #[test]
    fn partial_header_body_and_spanning_priming_reject_without_search_or_overflow() {
        let mut partial = sink();
        partial.write_all(b"Og").unwrap();
        assert!(!partial.complete());
        assert!(partial.flush().is_err());
        assert_eq!(partial.sink.0.borrow().written, 0);
        let mut body = sink();
        headers(&mut body);
        let page = page(&[1, 2, 3], 0, 3, 0);
        body.write_all(&page[..page.len() - 1]).unwrap();
        assert!(!body.complete());
        assert!(body.flush().is_err());
        let mut spanning = sink();
        headers(&mut spanning);
        let before = spanning.sink.0.borrow().written;
        let mut too_large = vec![0; MAX_PAGE];
        too_large[..4].copy_from_slice(b"OggS");
        too_large[18..22].copy_from_slice(&3_u32.to_le_bytes());
        too_large[26] = 255;
        too_large[27..282].fill(255);
        assert!(spanning.write_all(&too_large).is_err());
        let destination = spanning.sink.0.borrow();
        assert!(destination.capacity_failure && destination.blocked);
        assert_eq!(destination.written, before);
        assert_eq!(spanning.page.len(), MAX_PAGE);
        drop(destination);
        assert!(spanning.write_all(&[0]).is_err());
        assert_eq!(spanning.page.len(), MAX_PAGE);
    }
    #[test]
    fn identification_type_continued_headers_and_mixed_setup_audio_reject() {
        let mut invalid = sink();
        assert!(invalid.write_all(&page(b"\x03vorbis", 2, 0, 0)).is_err());
        assert_eq!(invalid.sink.0.borrow().written, 0);
        let mut continued = sink();
        assert!(continued.write_all(&page(b"\x01vorbis", 3, 0, 0)).is_err());
        assert_eq!(continued.sink.0.borrow().written, 0);
        let mut mixed = sink();
        mixed.write_all(&page(b"\x01vorbis", 2, 0, 0)).unwrap();
        mixed.write_all(&page(b"\x03vorbis", 0, 1, 0)).unwrap();
        let before = mixed.sink.0.borrow().written;
        let mut bytes = page(b"\x05vorbis", 0, 2, 0);
        bytes[26] = 2;
        bytes.insert(28, 1);
        bytes.push(0);
        assert!(mixed.write_all(&bytes).is_err());
        assert_eq!(mixed.sink.0.borrow().written, before);
    }
    type SpyCounts = Rc<std::cell::Cell<(usize, usize)>>;
    struct Spy(SpyCounts);
    impl Write for Spy {
        fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
            let (writes, flushes) = self.0.get();
            self.0.set((writes + 1, flushes));
            Ok(bytes.len())
        }
        fn flush(&mut self) -> io::Result<()> {
            let (writes, flushes) = self.0.get();
            self.0.set((writes, flushes + 1));
            Ok(())
        }
    }
    fn spy_sink() -> (PageSink<Spy>, SpyCounts) {
        let counts = Rc::new(std::cell::Cell::new((0, 0)));
        let source = Spy(Rc::clone(&counts));
        (
            PageSink::new(SinkGate(Rc::new(RefCell::new(Destination {
                sink: Some(source),
                written: 0,
                maximum: MAX_BYTES,
                blocked: false,
                capacity_failure: false,
            })))),
            counts,
        )
    }
    fn prove_retired(mut sink: PageSink<Spy>, counts: SpyCounts) {
        let before = counts.get();
        assert!(sink.sink.0.borrow().blocked);
        assert!(sink.write_all(&page(b"\x01vorbis", 2, 0, 0)).is_err());
        assert!(sink.flush().is_err());
        assert_eq!(counts.get(), before);
        drop(sink);
        assert_eq!(counts.get(), before);
    }
    #[test]
    fn every_parser_or_partial_flush_error_retires_backend_retry_and_drop_io() {
        let (mut invalid, counts) = spy_sink();
        let mut bad = page(b"\x01vorbis", 2, 0, 0);
        bad[0] = b'X';
        let error = invalid.write_all(&bad).unwrap_err();
        assert_eq!(error.to_string(), "invalid Vorbis physical page");
        prove_retired(invalid, counts);
        let (mut sequence, counts) = spy_sink();
        let error = sequence
            .write_all(&page(b"\x01vorbis", 2, 1, 0))
            .unwrap_err();
        assert_eq!(error.to_string(), "unexpected Vorbis page sequence");
        prove_retired(sequence, counts);
        let (mut mixed, counts) = spy_sink();
        mixed.write_all(&page(b"\x01vorbis", 2, 0, 0)).unwrap();
        mixed.write_all(&page(b"\x03vorbis", 0, 1, 0)).unwrap();
        let mut bad = page(b"\x05vorbis", 0, 2, 0);
        bad[26] = 2;
        bad.insert(28, 1);
        bad.push(0);
        let error = mixed.write_all(&bad).unwrap_err();
        assert_eq!(error.to_string(), "mixed Vorbis header/audio page");
        prove_retired(mixed, counts);
        let (mut partial, counts) = spy_sink();
        partial.write_all(b"Og").unwrap();
        let error = partial.flush().unwrap_err();
        assert_eq!(error.to_string(), "incomplete Vorbis physical page");
        prove_retired(partial, counts);
        let (mut overflow, counts) = spy_sink();
        assert!(overflow.write_all(&vec![0; MAX_PAGE + 1]).is_err());
        assert!(overflow.sink.0.borrow().capacity_failure);
        assert!(overflow.page.is_empty());
        prove_retired(overflow, counts);
    }
}
