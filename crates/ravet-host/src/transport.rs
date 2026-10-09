//! Checked portable block-start transport for explicitly opted-in native nodes.
use clack_host::{
    events::{
        EventHeader,
        event_types::{TransportEvent, TransportFlags},
    },
    utils::{BeatTime, SecondsTime},
};
use ravet_foundation::{ProcessError, Transport};

/// Map known portable context to CLAP's block-start fixed-point transport.
/// All absent fields produce None. If any field is known, playing must be known:
/// CLAP has no separate playing-valid flag, so absence is never guessed stopped.
/// Rate must be finite positive; tempo finite positive; positions must fit native
/// signed 31-fractional-bit times after rounding. Sample positions convert via
/// f64 seconds, with floating/fixed-point quantization; no full-i64 exact roundtrip
/// guarantee. Unsupported discontinuity is not serialized and does not reset DSP.
/// Optional details forward recording/loop/pre-roll flags, signature, BPM/sample
/// increment and bar metadata. Active loop bounds require matching known timelines
/// and remain strictly ordered after fixed-point rounding. Inactive supplied loops
/// reject. Missing details preserve the legacy false/zero native fields. No
/// mid-block updates, extrapolation or DSP resets are inferred.
/// Validation failure occurs before native rendering, without allocation.
pub fn hosted_transport(
    transport: Transport,
    sample_rate: f64,
) -> Result<Option<TransportEvent>, ProcessError> {
    if !sample_rate.is_finite() || sample_rate <= 0. {
        return Err(ProcessError::InvalidTransport);
    }
    if transport.playing.is_none()
        && transport.sample_position.is_none()
        && transport.beat_position.is_none()
        && transport.tempo.is_none()
        && transport.details.is_none()
    {
        return Ok(None);
    }
    let playing = transport.playing.ok_or(ProcessError::InvalidTransport)?;
    let mut event = TransportEvent {
        header: EventHeader::default(),
        flags: TransportFlags::empty(),
        song_pos_beats: BeatTime::default(),
        song_pos_seconds: SecondsTime::default(),
        tempo: 0.,
        tempo_inc: 0.,
        loop_start_beats: BeatTime::default(),
        loop_end_beats: BeatTime::default(),
        loop_start_seconds: SecondsTime::default(),
        loop_end_seconds: SecondsTime::default(),
        bar_start: BeatTime::default(),
        bar_number: 0,
        time_signature_numerator: 0,
        time_signature_denominator: 0,
    };
    if playing {
        event.flags |= TransportFlags::IS_PLAYING;
    }
    if let Some(tempo) = transport.tempo {
        if !tempo.is_finite() || tempo <= 0. {
            return Err(ProcessError::InvalidTransport);
        }
        event.flags |= TransportFlags::HAS_TEMPO;
        event.tempo = tempo;
    }
    if let Some(beats) = transport.beat_position {
        event.song_pos_beats = checked_time(beats)?;
        event.flags |= TransportFlags::HAS_BEATS_TIMELINE;
    }
    if let Some(samples) = transport.sample_position {
        event.song_pos_seconds = checked_time(samples as f64 / sample_rate)?;
        event.flags |= TransportFlags::HAS_SECONDS_TIMELINE;
    }
    if let Some(details) = transport.details {
        if details.recording {
            event.flags |= TransportFlags::IS_RECORDING;
        }
        if details.loop_active {
            event.flags |= TransportFlags::IS_LOOP_ACTIVE;
        }
        if details.within_pre_roll {
            event.flags |= TransportFlags::IS_WITHIN_PRE_ROLL;
        }
        if let Some((numerator, denominator)) = details.time_signature {
            if numerator == 0 || denominator == 0 {
                return Err(ProcessError::InvalidTransport);
            }
            event.flags |= TransportFlags::HAS_TIME_SIGNATURE;
            event.time_signature_numerator = numerator;
            event.time_signature_denominator = denominator;
        }
        if let Some(increment) = details.tempo_increment {
            if transport.tempo.is_none() || !increment.is_finite() {
                return Err(ProcessError::InvalidTransport);
            }
            event.tempo_inc = increment;
        }
        if let Some((start, number)) = details.bar {
            if transport.beat_position.is_none() {
                return Err(ProcessError::InvalidTransport);
            }
            event.bar_start = checked_time(start)?;
            event.bar_number = number;
        }
        if let Some((start, end)) = details.beat_loop {
            if !details.loop_active || transport.beat_position.is_none() || start >= end {
                return Err(ProcessError::InvalidTransport);
            }
            event.loop_start_beats = checked_time(start)?;
            event.loop_end_beats = checked_time(end)?;
            if event.loop_start_beats.to_bits() >= event.loop_end_beats.to_bits() {
                return Err(ProcessError::InvalidTransport);
            }
        } else if details.loop_active && transport.beat_position.is_some() {
            return Err(ProcessError::InvalidTransport);
        }
        if let Some((start, end)) = details.sample_loop {
            if !details.loop_active || transport.sample_position.is_none() || start >= end {
                return Err(ProcessError::InvalidTransport);
            }
            event.loop_start_seconds = checked_time(start as f64 / sample_rate)?;
            event.loop_end_seconds = checked_time(end as f64 / sample_rate)?;
            if event.loop_start_seconds.to_bits() >= event.loop_end_seconds.to_bits() {
                return Err(ProcessError::InvalidTransport);
            }
        } else if details.loop_active && transport.sample_position.is_some() {
            return Err(ProcessError::InvalidTransport);
        }
    }
    Ok(Some(event))
}
fn checked_time(value: f64) -> Result<BeatTime, ProcessError> {
    let raw = (value * BeatTime::FACTOR as f64).round();
    if !raw.is_finite() || !(-9223372036854775808.0..9223372036854775808.0).contains(&raw) {
        return Err(ProcessError::InvalidTransport);
    }
    Ok(BeatTime::from_bits(raw as i64))
}
