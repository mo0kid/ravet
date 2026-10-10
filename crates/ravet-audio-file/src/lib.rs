//! Bounded WAV/AIFF stream I/O on control/worker threads and prepared audio handoff.
//! No file methods belong in an audio callback. Ring push/pop and prepared transport use
//! storage without allocation, deallocation, blocking or I/O. Memory playback
//! renders a prepared clip without file I/O or implicit rate conversion.
mod aiff;
mod aiff_buffered;
mod automatic;
mod background;
mod buffered;
#[cfg(feature = "compressed")]
mod compressed;
#[cfg(feature = "compressed")]
mod compressed_buffered;
#[cfg(feature = "compressed")]
mod compressed_indexed;
#[cfg(feature = "compressed")]
mod compressed_stream;
mod pcm_buffered;
mod playback;
mod recording;
mod ring;
mod wave;
pub use aiff::{AiffEncoding, AiffFormat, AiffReader, AiffWriter};
pub use aiff_buffered::{AiffReadAhead, AiffRecorder};
pub use automatic::{
    BufferedPlayback, BufferedPlaybackProgress, ReadAheadExit, ReadAheadPump, ReadAheadStatus,
    ReadAheadWorker,
};
pub use buffered::{ReadAheadProgress, WaveReadAhead, WaveRecorder};
#[cfg(feature = "compressed")]
pub use compressed::{DecodedClip, decode_compressed};
#[cfg(feature = "compressed")]
pub use compressed_buffered::CompressedReadAhead;
#[cfg(feature = "compressed")]
pub use compressed_stream::CompressedReader;
pub use playback::{MemoryPlayback, PlaybackError, PlaybackProgress};
pub use recording::{RecordingExit, RecordingPump, RecordingStatus, RecordingWorker};
pub use ring::{AudioConsumer, AudioProducer, RingError, audio_ring};
pub use wave::{AudioFileError, WaveEncoding, WaveFormat, WaveReader, WaveWriter};

#[cfg(feature = "flac-encode")]
mod flac_encode;
#[cfg(feature = "flac-encode")]
pub use flac_encode::{FlacFormat, FlacPcmEncoding, encode_flac};

#[cfg(feature = "flac-encode")]
mod flac_writer;
#[cfg(feature = "flac-encode")]
pub use flac_writer::FlacWriter;

#[cfg(feature = "flac-encode")]
mod flac_recorder;
#[cfg(feature = "flac-encode")]
pub use flac_recorder::{FlacCapturePolicy, FlacRecorder};

#[cfg(feature = "vorbis-encode")]
mod vorbis;
#[cfg(feature = "vorbis-encode")]
pub use vorbis::{VorbisFormat, VorbisWriter, encode_vorbis};

#[cfg(feature = "vorbis-encode")]
mod vorbis_recording;
#[cfg(feature = "vorbis-encode")]
pub use vorbis_recording::{
    VorbisRecordingCompletion, VorbisRecordingExit, VorbisRecordingStatus, VorbisRecordingWorker,
};

#[cfg(feature = "mp3-encode")]
mod mp3;
#[cfg(feature = "mp3-encode")]
pub use mp3::{Mp3Bitrate, Mp3Failure, Mp3Format, Mp3WriteError, Mp3Writer, encode_mp3};

#[cfg(any(feature = "vorbis-encode", feature = "mp3-encode"))]
mod codec_recording;
#[cfg(feature = "mp3-encode")]
mod mp3_factory;
#[cfg(feature = "vorbis-encode")]
mod vorbis_factory;

#[cfg(feature = "mp3-encode")]
mod mp3_recording;
#[cfg(feature = "mp3-encode")]
pub use mp3_recording::{
    Mp3RecordingCompletion, Mp3RecordingExit, Mp3RecordingStatus, Mp3RecordingWorker,
};
