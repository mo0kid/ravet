//! Bounded WAV/AIFF stream I/O on control/worker threads and prepared audio handoff.
//! No file methods belong in an audio callback. Only ring push/pop use prepared
//! storage without allocation, deallocation, blocking or I/O. Memory playback
//! renders a prepared clip without file I/O or implicit rate conversion.
mod aiff;
mod buffered;
#[cfg(feature = "compressed")]
mod compressed;
#[cfg(feature = "compressed")]
mod compressed_buffered;
#[cfg(feature = "compressed")]
mod compressed_stream;
mod playback;
mod ring;
mod wave;
pub use aiff::{AiffEncoding, AiffFormat, AiffReader, AiffWriter};
pub use buffered::{ReadAheadProgress, WaveReadAhead, WaveRecorder};
#[cfg(feature = "compressed")]
pub use compressed::{DecodedClip, decode_compressed};
#[cfg(feature = "compressed")]
pub use compressed_buffered::CompressedReadAhead;
#[cfg(feature = "compressed")]
pub use compressed_stream::CompressedReader;
pub use playback::{MemoryPlayback, PlaybackError, PlaybackProgress};
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
