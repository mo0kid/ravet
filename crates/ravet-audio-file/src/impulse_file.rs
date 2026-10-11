//! Bounded file reading for explicit impulse-response preparation (off audio).
use crate::{AudioFileError, PlaybackReader};
use ravet_signal::{ImpulseResponseError, ImpulseResponseRequest, PreparedImpulseResponse};

/// Explicit source bounds for [`prepare_impulse_response_file`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ImpulseResponseFileOptions {
    /// Largest accepted source duration in frames, 1..=4,194,304. A declared
    /// longer duration is refused before any read.
    pub maximum_source_frames: usize,
    /// Interleaved read batch, 1..=65,536 frames (capped by the reader maximum).
    pub source_batch_frames: usize,
}

/// Why file preparation stopped. Reads may have advanced the reader.
#[derive(Debug)]
pub enum ImpulseResponseFileErrorKind {
    /// The reader returned an error.
    Reader(AudioFileError),
    /// The reader is not at frame zero or reports unsupported channels/rate.
    Format,
    /// Options or declared/actual duration exceed bounds or the working budget.
    Capacity,
    /// The reader violated its committed-frame contract or declared duration.
    Record,
    /// A decoded sample was nonfinite.
    NonFinite,
    /// The signal preparation refused the decoded response.
    Preparation(ImpulseResponseError),
}

/// Whole-call refusal with the committed source frames observed.
#[derive(Debug)]
pub struct ImpulseResponseFileError {
    /// Refusal reason.
    pub kind: ImpulseResponseFileErrorKind,
    /// Reader position after the last attempted read (0 when refused before reads).
    pub source_frames_read: u64,
}

impl std::fmt::Display for ImpulseResponseFileError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "impulse-response file preparation failed: {:?}",
            self.kind
        )
    }
}
impl std::error::Error for ImpulseResponseFileError {}

const MAX_SOURCE_FRAMES: usize = ravet_signal::IMPULSE_RESPONSE_MAX_SOURCE_FRAMES;

/// Read a complete WAV/AIFF/compressed response through [`PlaybackReader`] at its
/// source rate and prepare it with [`ravet_signal::prepare_impulse_response`].
///
/// Before any read: the reader must be at frame zero with 1..=64 channels and a
/// 1..=768,000 Hz rate; options are bounded; the declared duration (or, when
/// unknown, `maximum_source_frames`) is planned with
/// [`ImpulseResponseRequest::plan`], and planar f64 source storage plus read
/// scratch plus the request's declared bytes must fit
/// `request.maximum_working_bytes`. Decoded f32 samples are widened exactly to f64.
/// Any reader error, contract violation, nonfinite sample, duration mismatch or
/// preparation refusal returns no partial response. There is no rollback or seek
/// of the reader. CONTROL/worker only; allocates and blocks on I/O.
pub fn prepare_impulse_response_file<R: PlaybackReader>(
    reader: &mut R,
    options: ImpulseResponseFileOptions,
    request: &ImpulseResponseRequest<'_>,
) -> Result<PreparedImpulseResponse, ImpulseResponseFileError> {
    let mut read_frames: u64;
    let fail = |kind, read| ImpulseResponseFileError {
        kind,
        source_frames_read: read,
    };
    let channels = reader.channels();
    let rate = reader.sample_rate();
    if !(1..=64).contains(&channels)
        || !(1..=768_000).contains(&rate)
        || reader.position_frames() != 0
    {
        return Err(fail(ImpulseResponseFileErrorKind::Format, 0));
    }
    if !(1..=MAX_SOURCE_FRAMES).contains(&options.maximum_source_frames)
        || !(1..=65_536).contains(&options.source_batch_frames)
        || !(1..=65_536).contains(&reader.maximum_read_frames())
    {
        return Err(fail(ImpulseResponseFileErrorKind::Capacity, 0));
    }
    let declared = reader.declared_frames();
    let bound = declared.unwrap_or(options.maximum_source_frames as u64);
    if bound == 0 {
        return Err(fail(ImpulseResponseFileErrorKind::Record, 0));
    }
    if bound > options.maximum_source_frames as u64 {
        return Err(fail(ImpulseResponseFileErrorKind::Capacity, 0));
    }
    let bound = bound as usize;
    let plan = request
        .plan(rate, channels, bound)
        .map_err(|e| fail(ImpulseResponseFileErrorKind::Preparation(e), 0))?;
    let batch = options
        .source_batch_frames
        .min(reader.maximum_read_frames());
    let source_bytes = channels as u128 * bound as u128 * 8 + (batch * channels * 4) as u128;
    if source_bytes + plan.declared_working_bytes as u128 > request.maximum_working_bytes as u128 {
        return Err(fail(ImpulseResponseFileErrorKind::Capacity, 0));
    }
    let mut planar = Vec::new();
    planar
        .try_reserve_exact(channels * bound)
        .map_err(|_| fail(ImpulseResponseFileErrorKind::Capacity, 0))?;
    planar.resize(channels * bound, 0f64);
    let mut scratch = Vec::new();
    scratch
        .try_reserve_exact(batch * channels)
        .map_err(|_| fail(ImpulseResponseFileErrorKind::Capacity, 0))?;
    scratch.resize(batch * channels, 0f32);
    loop {
        let before = reader.position_frames();
        let result = reader.read_frames(&mut scratch);
        read_frames = reader.position_frames();
        let frames =
            result.map_err(|e| fail(ImpulseResponseFileErrorKind::Reader(e), read_frames))?;
        if frames > batch || Some(read_frames) != before.checked_add(frames as u64) {
            return Err(fail(ImpulseResponseFileErrorKind::Record, read_frames));
        }
        if read_frames > bound as u64 {
            return Err(fail(ImpulseResponseFileErrorKind::Capacity, read_frames));
        }
        if frames == 0 {
            break;
        }
        let first = before as usize;
        for (i, frame) in scratch[..frames * channels]
            .chunks_exact(channels)
            .enumerate()
        {
            for (c, &x) in frame.iter().enumerate() {
                if !x.is_finite() {
                    return Err(fail(ImpulseResponseFileErrorKind::NonFinite, read_frames));
                }
                planar[c * bound + first + i] = f64::from(x);
            }
        }
    }
    if declared.is_some_and(|n| n != read_frames) || read_frames == 0 {
        return Err(fail(ImpulseResponseFileErrorKind::Record, read_frames));
    }
    let frames = read_frames as usize;
    let source: Vec<&[f64]> = planar
        .chunks_exact(bound)
        .map(|channel| &channel[..frames])
        .collect();
    ravet_signal::prepare_impulse_response(rate, &source, request)
        .map_err(|e| fail(ImpulseResponseFileErrorKind::Preparation(e), read_frames))
}
