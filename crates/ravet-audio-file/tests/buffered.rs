use ravet_audio_file::{
    AudioFileError, WaveEncoding, WaveFormat, WaveReadAhead, WaveReader, WaveRecorder, WaveWriter,
    audio_ring,
};
use std::io::{self, Cursor, Seek, SeekFrom, Write};
fn format() -> WaveFormat {
    WaveFormat {
        channels: 2,
        sample_rate: 48000,
        encoding: WaveEncoding::Float32,
    }
}
fn source() -> Vec<u8> {
    let mut writer = WaveWriter::new(Cursor::new(Vec::new()), format(), 16).unwrap();
    let samples: Vec<_> = (0..13)
        .flat_map(|n| [n as f32 / 16., -(n as f32) / 16.])
        .collect();
    writer.write_frames(&samples).unwrap();
    writer.finish().unwrap().into_inner()
}

#[test]
fn read_ahead_retains_backpressure_suffix_and_drains_exact_file_order() {
    let (producer, mut consumer) = audio_ring(2, 2).unwrap();
    let reader = WaveReader::new(Cursor::new(source()), 5).unwrap();
    let mut ahead = WaveReadAhead::new(reader, producer, 5).unwrap();
    let first = ahead.pump().unwrap();
    assert_eq!(
        (first.read_frames, first.queued_frames, first.pending_frames),
        (5, 2, 3)
    );
    assert_eq!(ahead.pump().unwrap().pending_frames, 3);
    assert_eq!(ahead.reader_position(), 5);
    let mut output = Vec::new();
    loop {
        let mut sample = [99.; 2];
        let count = consumer.pop(&mut sample).unwrap();
        if count != 0 {
            output.extend(sample);
        }
        let status = ahead.pump().unwrap();
        if status.end_of_file && status.pending_frames == 0 && consumer.queued_frames() == 0 {
            break;
        }
    }
    let expected: Vec<_> = (0..13)
        .flat_map(|n| [n as f32 / 16., -(n as f32) / 16.])
        .collect();
    assert_eq!(output, expected);
    assert_eq!(ahead.pump().unwrap().read_frames, 0);
    assert!(consumer.pop(&mut [0.; 2]).unwrap() == 0);
}

#[test]
fn stopped_seek_discards_old_generation_and_rejects_foreign_or_invalid_seek() {
    let (producer, mut consumer) = audio_ring(2, 2).unwrap();
    let (_, mut foreign) = audio_ring(2, 2).unwrap();
    let reader = WaveReader::new(Cursor::new(source()), 5).unwrap();
    let mut ahead = WaveReadAhead::new(reader, producer, 5).unwrap();
    ahead.pump().unwrap();
    assert!(ahead.seek_stopped(&mut foreign, 10).is_err());
    assert!(ahead.seek_stopped(&mut consumer, 14).is_err());
    assert_eq!(ahead.reader_position(), 5);
    assert_eq!(consumer.queued_frames(), 2);
    ahead.seek_stopped(&mut consumer, 10).unwrap();
    assert_eq!(consumer.queued_frames(), 0);
    assert_eq!(ahead.reader_position(), 10);
    assert_eq!(ahead.pump().unwrap().pending_frames, 1);
    let mut output = [99.; 4];
    assert_eq!(consumer.pop(&mut output).unwrap(), 2);
    assert_eq!(output, [10. / 16., -10. / 16., 11. / 16., -11. / 16.]);
    ahead.seek_stopped(&mut consumer, 13).unwrap();
    assert!(ahead.pump().unwrap().end_of_file);
    assert_eq!(consumer.queued_frames(), 0);
}

#[test]
fn recorder_pumps_bounded_prefix_and_finalizes_remaining_frames() {
    let (mut producer, consumer) = audio_ring(2, 8).unwrap();
    let mut recorder = WaveRecorder::new(Cursor::new(Vec::new()), format(), 2, consumer).unwrap();
    assert_eq!(recorder.pump().unwrap(), 0);
    producer
        .push(&[0.25, -0.5, 0.125, -0.25, 0.75, -1.])
        .unwrap();
    assert_eq!(recorder.pump().unwrap(), 2);
    assert_eq!(recorder.frames(), 2);
    drop(producer);
    let file = recorder.finish().unwrap();
    let mut reader = WaveReader::new(file, 8).unwrap();
    assert_eq!(reader.frames(), 3);
    let mut samples = [99.; 8];
    assert_eq!(reader.read_frames(&mut samples).unwrap(), 3);
    assert_eq!(&samples[..6], &[0.25, -0.5, 0.125, -0.25, 0.75, -1.]);
    assert_eq!(&samples[6..], &[99.; 2]);
}

struct FailingFile {
    inner: Cursor<Vec<u8>>,
    budget: usize,
}
impl Write for FailingFile {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        if self.budget == 0 {
            return Err(io::Error::other("injected partial recording failure"));
        }
        let count = bytes.len().min(self.budget);
        self.budget -= count;
        self.inner.write(&bytes[..count])
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}
impl Seek for FailingFile {
    fn seek(&mut self, from: SeekFrom) -> io::Result<u64> {
        self.inner.seek(from)
    }
}
#[test]
fn dequeued_recording_errors_poison_and_configuration_rejects() {
    let (mut producer, consumer) = audio_ring(2, 4).unwrap();
    let file = FailingFile {
        inner: Cursor::new(Vec::new()),
        budget: 46,
    };
    let mut pcm = format();
    pcm.encoding = WaveEncoding::Pcm16;
    let mut recorder = WaveRecorder::new(file, pcm, 2, consumer).unwrap();
    producer.push(&[0.25, -0.5, 0.25, -0.5]).unwrap();
    assert!(matches!(recorder.pump(), Err(AudioFileError::Io(_))));
    assert_eq!(recorder.frames(), 0);
    assert!(matches!(recorder.pump(), Err(AudioFileError::Poisoned)));
    drop(producer);
    assert!(matches!(recorder.finish(), Err(AudioFileError::Poisoned)));
    let (mut producer, consumer) = audio_ring(2, 4).unwrap();
    let mut recorder = WaveRecorder::new(Cursor::new(Vec::new()), pcm, 2, consumer).unwrap();
    producer.push(&[f32::NAN, 0.]).unwrap();
    assert!(matches!(
        recorder.pump(),
        Err(AudioFileError::InvalidSamples)
    ));
    assert!(matches!(recorder.pump(), Err(AudioFileError::Poisoned)));
    let (_, consumer) = audio_ring(1, 4).unwrap();
    assert!(WaveRecorder::new(Cursor::new(Vec::new()), pcm, 2, consumer).is_err());
    let reader = WaveReader::new(Cursor::new(source()), 5).unwrap();
    let (producer, _) = audio_ring(2, 4).unwrap();
    assert!(WaveReadAhead::new(reader, producer, 6).is_err());
}
