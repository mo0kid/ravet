use crate::codec_recording::{CodecFactory, CodecRecordingCompletion, CodecRecovery, CodecSession};
use crate::{AudioFileError as Error, Mp3Format, Mp3WriteError, Mp3Writer};
use std::io::{Seek, Write};
pub(crate) struct Mp3Factory;
impl<W: Write + Seek> CodecFactory<W> for Mp3Factory {
    type Format = Mp3Format;
    type Session = Mp3Writer<W>;
    const THREAD_NAME: &'static str = "ravet-mp3-recording";
    fn channels(format: Mp3Format) -> usize {
        usize::from(format.channels)
    }
    fn validate_configuration(format: Mp3Format, frames: u64, bytes: u64) -> Result<(), Error> {
        Mp3Writer::<W>::validate_configuration(format, frames, bytes)
    }
    fn initialize(
        sink: W,
        format: Mp3Format,
        frames: u64,
        bytes: u64,
    ) -> Result<Self::Session, CodecRecovery<W>> {
        Mp3Writer::new(sink, format, frames, bytes).map_err(|failure| {
            let accepted_frames = failure.accepted_frames();
            let written_bytes = failure.written_bytes();
            let file_bytes = failure.file_bytes();
            let (sink, error) = failure.into_parts();
            CodecRecovery {
                sink,
                completion: CodecRecordingCompletion::Failed(error),
                accepted_frames,
                file_bytes,
                written_bytes,
            }
        })
    }
}
impl<W: Write + Seek> CodecSession<W> for Mp3Writer<W> {
    fn write_samples(&mut self, samples: &[f32]) -> Result<(), Error> {
        match Mp3Writer::write_samples(self, samples) {
            Ok(_) => Ok(()),
            Err(Mp3WriteError::Rejected(error)) => Err(error),
            Err(Mp3WriteError::Failed(_)) => Err(Error::Poisoned),
        }
    }
    fn accepted_frames(&self) -> u64 {
        Mp3Writer::accepted_frames(self)
    }
    fn written_bytes(&self) -> u64 {
        Mp3Writer::written_bytes(self)
    }
    fn file_bytes(&self) -> u64 {
        Mp3Writer::file_bytes(self)
    }
    fn abort_reclaim(self, error: Option<Error>) -> CodecRecovery<W> {
        Mp3Writer::abort_reclaim(self, error)
    }
    fn finish_reclaim(self) -> CodecRecovery<W> {
        Mp3Writer::finish_reclaim(self)
    }
}
