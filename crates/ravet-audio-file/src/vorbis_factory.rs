use crate::codec_recording::{CodecFactory, CodecRecordingCompletion, CodecRecovery, CodecSession};
use crate::vorbis::{VorbisRecovery, VorbisWriter};
use crate::{AudioFileError as Error, VorbisFormat};
use std::io::Write;
pub(crate) struct VorbisFactory;
fn convert<W>(value: VorbisRecovery<W>) -> CodecRecovery<W> {
    CodecRecovery {
        sink: value.sink,
        completion: if value.finalized {
            CodecRecordingCompletion::Finalized
        } else if let Some(error) = value.error {
            CodecRecordingCompletion::Failed(error)
        } else {
            CodecRecordingCompletion::Aborted
        },
        accepted_frames: value.accepted_frames,
        file_bytes: value.written_bytes,
        written_bytes: value.written_bytes,
    }
}
impl<W: Write> CodecFactory<W> for VorbisFactory {
    type Format = VorbisFormat;
    type Session = VorbisWriter<W>;
    const THREAD_NAME: &'static str = "ravet-vorbis-recording";
    fn channels(format: VorbisFormat) -> usize {
        usize::from(format.channels)
    }
    fn validate_configuration(format: VorbisFormat, frames: u64, bytes: u64) -> Result<(), Error> {
        VorbisWriter::<W>::validate_configuration(format, frames, bytes)
    }
    fn initialize(
        sink: W,
        format: VorbisFormat,
        frames: u64,
        bytes: u64,
    ) -> Result<Self::Session, CodecRecovery<W>> {
        VorbisWriter::initialize_reclaim(sink, format, frames, bytes).map_err(convert)
    }
}
impl<W: Write> CodecSession<W> for VorbisWriter<W> {
    fn write_samples(&mut self, samples: &[f32]) -> Result<(), Error> {
        self.write_frames(samples)
    }
    fn accepted_frames(&self) -> u64 {
        VorbisWriter::accepted_frames(self)
    }
    fn written_bytes(&self) -> u64 {
        VorbisWriter::written_bytes(self)
    }
    fn file_bytes(&self) -> u64 {
        VorbisWriter::written_bytes(self)
    }
    fn abort_reclaim(self, error: Option<Error>) -> CodecRecovery<W> {
        convert(VorbisWriter::abort_reclaim(self, error))
    }
    fn finish_reclaim(self) -> CodecRecovery<W> {
        convert(VorbisWriter::finish_reclaim(self))
    }
}
