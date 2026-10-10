//! Worker-built compressed FLAC packet index; no PCM clip is retained.
use crate::{AudioFileError, compressed::error};
use std::{io::Cursor, sync::Arc};
use symphonia::core::{
    audio::{SampleBuffer, SignalSpec},
    codecs::{CODEC_TYPE_FLAC, CodecParameters, Decoder, DecoderOptions},
    errors::Error,
    formats::{FormatOptions, Packet},
    io::MediaSourceStream,
    meta::{Limit, MetadataOptions},
    probe::Hint,
};
const MAX_SAMPLES: usize = 16_777_216;
const MAX_PACKETS: usize = 65_536;
const MAX_BYTES: usize = 67_108_864;
#[cfg(test)]
std::thread_local! { static DECODE_CALLS: std::cell::Cell<usize> = const { std::cell::Cell::new(0) }; }
struct FlacIndex {
    parameters: CodecParameters,
    packets: Vec<Packet>,
    spec: SignalSpec,
    frames: usize,
}
/// Only immutable, fully decoded/verified FLAC packets are shared between seeks.
pub(super) struct IndexedState {
    index: Arc<FlacIndex>,
    decoder: Box<dyn Decoder>,
    next_packet: usize,
    pending: Vec<f32>,
    offset: usize,
    position: usize,
    eof: bool,
}
fn convert(
    decoder: &mut dyn Decoder,
    packet: &Packet,
    expected_spec: Option<SignalSpec>,
) -> Result<(SignalSpec, Vec<f32>), AudioFileError> {
    #[cfg(test)]
    DECODE_CALLS.with(|count| count.set(count.get() + 1));
    let decoded = decoder.decode(packet).map_err(error)?;
    let spec = *decoded.spec();
    if expected_spec.is_some_and(|previous| previous != spec) {
        return Err(AudioFileError::Unsupported);
    }
    let channels = spec.channels.count();
    if !(1..=64).contains(&channels) || !(1..=768_000).contains(&spec.rate) {
        return Err(AudioFileError::Unsupported);
    }
    if decoded.capacity() > 65_536 {
        return Err(AudioFileError::Capacity);
    }
    if decoded.frames() == 0
        || decoded.frames() as u64 != packet.dur
        || packet.trim_start != 0
        || packet.trim_end != 0
    {
        return Err(AudioFileError::InvalidRecord);
    }
    let samples = decoded.frames() * channels;
    let mut converted = SampleBuffer::<f32>::new(decoded.capacity() as u64, spec);
    converted.copy_interleaved_ref(decoded);
    if converted.samples().len() != samples
        || converted.samples().iter().any(|sample| !sample.is_finite())
    {
        return Err(AudioFileError::InvalidSamples);
    }
    Ok((spec, converted.samples().to_vec()))
}
impl IndexedState {
    pub(super) fn new(encoded: Arc<[u8]>, maximum: usize) -> Result<Self, AudioFileError> {
        // Ogg and lossy codecs have different preroll and reservoir requirements.
        if !encoded.starts_with(b"fLaC") {
            return Err(AudioFileError::Unsupported);
        }
        let source = MediaSourceStream::new(Box::new(Cursor::new(encoded)), Default::default());
        let mut format = symphonia::default::get_probe()
            .format(
                &Hint::new(),
                source,
                &FormatOptions::default(),
                &MetadataOptions {
                    limit_metadata_bytes: Limit::Maximum(65_536),
                    limit_visual_bytes: Limit::Maximum(0),
                },
            )
            .map_err(error)?
            .format;
        let track = format.default_track().ok_or(AudioFileError::Unsupported)?;
        if track.codec_params.codec != CODEC_TYPE_FLAC {
            return Err(AudioFileError::Unsupported);
        }
        let track_id = track.id;
        let parameters = track.codec_params.clone();
        if parameters.n_frames.is_none_or(|frames| frames == 0) {
            return Err(AudioFileError::Unsupported);
        }
        if parameters
            .n_frames
            .is_some_and(|frames| frames > maximum as u64)
        {
            return Err(AudioFileError::Capacity);
        }
        let mut decoder = symphonia::default::get_codecs()
            .make(&parameters, &DecoderOptions { verify: true })
            .map_err(error)?;
        let mut packets = Vec::new();
        let mut specification = None;
        let mut frames = 0usize;
        let mut bytes = 0usize;
        loop {
            let packet = match format.next_packet() {
                Ok(packet) => packet,
                Err(Error::IoError(io)) if io.kind() == std::io::ErrorKind::UnexpectedEof => break,
                Err(failure) => return Err(error(failure)),
            };
            if packets.len() == MAX_PACKETS {
                return Err(AudioFileError::Capacity);
            }
            if packet.track_id() != track_id || packet.ts != frames as u64 {
                return Err(AudioFileError::InvalidRecord);
            }
            bytes = bytes
                .checked_add(packet.data.len())
                .ok_or(AudioFileError::Capacity)?;
            if bytes > MAX_BYTES {
                return Err(AudioFileError::Capacity);
            }
            let (spec, samples) = convert(decoder.as_mut(), &packet, specification)?;
            specification = Some(spec);
            frames = frames
                .checked_add(samples.len() / spec.channels.count())
                .ok_or(AudioFileError::Capacity)?;
            if frames > maximum || frames * spec.channels.count() > MAX_SAMPLES {
                return Err(AudioFileError::Capacity);
            }
            packets.push(packet);
            while !format.metadata().is_latest() {
                format.metadata().pop();
            }
        }
        let spec = specification.ok_or(AudioFileError::InvalidRecord)?;
        if parameters
            .n_frames
            .is_some_and(|declared| declared != frames as u64)
            || decoder.finalize().verify_ok == Some(false)
        {
            return Err(AudioFileError::InvalidRecord);
        }
        let index = Arc::new(FlacIndex {
            parameters,
            packets,
            spec,
            frames,
        });
        Self::at(index, 0)
    }
    fn at(index: Arc<FlacIndex>, frame: usize) -> Result<Self, AudioFileError> {
        if frame > index.frames {
            return Err(AudioFileError::Capacity);
        }
        // Verification covered the complete stream at construction. A suffix
        // cannot verify the original stream MD5, so candidate decoders disable it.
        let decoder = symphonia::default::get_codecs()
            .make(&index.parameters, &DecoderOptions { verify: false })
            .map_err(error)?;
        let next_packet = if frame == index.frames {
            index.packets.len()
        } else {
            index
                .packets
                .partition_point(|packet| packet.ts <= frame as u64)
                - 1
        };
        let mut state = Self {
            index,
            decoder,
            next_packet,
            pending: Vec::new(),
            offset: 0,
            position: frame,
            eof: false,
        };
        if frame == state.index.frames {
            state.eof = true;
        } else {
            let start = state.index.packets[next_packet].ts as usize;
            state.next()?;
            state.offset = (frame - start) * state.channels();
        }
        Ok(state)
    }
    fn next(&mut self) -> Result<bool, AudioFileError> {
        if self.next_packet == self.index.packets.len() {
            self.pending.clear();
            self.offset = 0;
            self.eof = true;
            return Ok(false);
        }
        let (_, pending) = convert(
            self.decoder.as_mut(),
            &self.index.packets[self.next_packet],
            Some(self.index.spec),
        )?;
        self.pending = pending;
        self.offset = 0;
        self.next_packet += 1;
        Ok(true)
    }
    pub(super) fn channels(&self) -> usize {
        self.index.spec.channels.count()
    }
    pub(super) fn sample_rate(&self) -> u32 {
        self.index.spec.rate
    }
    pub(super) fn position(&self) -> usize {
        self.position
    }
    pub(super) fn declared(&self) -> Option<u64> {
        self.index.parameters.n_frames
    }
    pub(super) fn eof(&self) -> bool {
        self.eof
    }
    pub(super) fn packet_count(&self) -> usize {
        self.index.packets.len()
    }
    pub(super) fn seek(&self, frame: usize) -> Result<Self, AudioFileError> {
        Self::at(Arc::clone(&self.index), frame)
    }
    pub(super) fn read(&mut self, output: &mut [f32]) -> Result<usize, AudioFileError> {
        let channels = self.channels();
        let mut used = 0;
        while used < output.len() {
            if self.offset == self.pending.len() && !self.next()? {
                break;
            }
            let count = (output.len() - used).min(self.pending.len() - self.offset);
            output[used..used + count]
                .copy_from_slice(&self.pending[self.offset..self.offset + count]);
            self.offset += count;
            used += count;
            self.position += count / channels;
        }
        Ok(used / channels)
    }
}

#[cfg(all(test, feature = "flac-encode"))]
mod tests {
    use super::*;
    use crate::{FlacFormat, FlacPcmEncoding, FlacWriter};
    #[test]
    fn each_candidate_decodes_one_packet_and_shares_the_verified_index() {
        const FRAMES: usize = 131_075;
        let mut writer = FlacWriter::new(
            Cursor::new(Vec::new()),
            FlacFormat {
                channels: 1,
                sample_rate: 48000,
                encoding: FlacPcmEncoding::Pcm16,
            },
            FRAMES as u64,
            1_048_576,
        )
        .unwrap();
        let samples: Vec<i32> = (0..FRAMES).map(|i| (i % 32_768) as i32).collect();
        for block in samples.chunks(65_536) {
            writer.write_frames(block).unwrap();
        }
        let bytes: Arc<[u8]> = Arc::from(writer.finish().unwrap().into_inner());
        DECODE_CALLS.with(|count| count.set(0));
        let state = IndexedState::new(bytes, FRAMES).unwrap();
        assert_eq!(state.packet_count(), 33);
        DECODE_CALLS.with(|count| assert_eq!(count.get(), 34)); // verification + initial packet
        for frame in [0, 4095, 4096, 65_536, FRAMES - 1, 2, 131_072] {
            DECODE_CALLS.with(|count| count.set(0));
            let mut candidate = state.seek(frame).unwrap();
            assert!(Arc::ptr_eq(&state.index, &candidate.index));
            DECODE_CALLS.with(|count| assert_eq!(count.get(), 1));
            assert_eq!(candidate.position(), frame);
            let mut output = [99.; 1];
            assert_eq!(candidate.read(&mut output).unwrap(), 1);
            assert_eq!(output[0], samples[frame] as f32 / 32768.);
            DECODE_CALLS.with(|count| assert_eq!(count.get(), 1));
        }
        DECODE_CALLS.with(|count| count.set(0));
        assert!(state.seek(FRAMES).unwrap().eof());
        assert!(state.seek(FRAMES + 1).is_err());
        DECODE_CALLS.with(|count| assert_eq!(count.get(), 0));
        assert_eq!(state.position(), 0);
    }
}
