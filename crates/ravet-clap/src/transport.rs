use crate::BridgeError;
use clack_plugin::events::event_types::{TransportEvent, TransportFlags};
use ravet_foundation::{Transport, TransportDetails};

/// Convert known CLAP block-start fields into portable transport context.
///
/// No context leaves all optional fields absent. Unadvertised fields are ignored.
/// Advertised tempo must be finite and positive; seconds convert to the nearest
/// sample at the validated rate, rejecting overflow rather than saturating.
/// Additional flags, signature, tempo increment, bar and active loop metadata
/// are forwarded only with their matching validity flags. Loop bounds must remain
/// strictly increasing after sample rounding. Inactive loop fields are ignored.
/// Discontinuity detection belongs to the prepared bridge; no mid-block updates.
pub fn translate_transport(
    event: Option<&TransportEvent>,
    sample_rate: f64,
) -> Result<Transport, BridgeError> {
    if !sample_rate.is_finite() || sample_rate <= 0.0 {
        return Err(BridgeError::InvalidTransport);
    }
    let Some(event) = event else {
        return Ok(Transport::default());
    };
    let flags = event.flags;
    let tempo = if flags.contains(TransportFlags::HAS_TEMPO) {
        if !event.tempo.is_finite() || event.tempo <= 0.0 {
            return Err(BridgeError::InvalidTransport);
        }
        Some(event.tempo)
    } else {
        None
    };
    let sample_position = if flags.contains(TransportFlags::HAS_SECONDS_TIMELINE) {
        Some(samples(event.song_pos_seconds.to_float(), sample_rate)?)
    } else {
        None
    };
    let mut details = TransportDetails {
        recording: flags.contains(TransportFlags::IS_RECORDING),
        loop_active: flags.contains(TransportFlags::IS_LOOP_ACTIVE),
        within_pre_roll: flags.contains(TransportFlags::IS_WITHIN_PRE_ROLL),
        ..TransportDetails::default()
    };
    if flags.contains(TransportFlags::HAS_TIME_SIGNATURE) {
        if event.time_signature_numerator == 0 || event.time_signature_denominator == 0 {
            return Err(BridgeError::InvalidTransport);
        }
        details.time_signature = Some((
            event.time_signature_numerator,
            event.time_signature_denominator,
        ));
    }
    if tempo.is_some() {
        if !event.tempo_inc.is_finite() {
            return Err(BridgeError::InvalidTransport);
        }
        details.tempo_increment = Some(event.tempo_inc);
    }
    if flags.contains(TransportFlags::HAS_BEATS_TIMELINE) {
        details.bar = Some((event.bar_start.to_float(), event.bar_number));
        if details.loop_active {
            let bounds = (
                event.loop_start_beats.to_float(),
                event.loop_end_beats.to_float(),
            );
            if bounds.0 >= bounds.1 {
                return Err(BridgeError::InvalidTransport);
            }
            details.beat_loop = Some(bounds);
        }
    }
    if sample_position.is_some() && details.loop_active {
        let bounds = (
            samples(event.loop_start_seconds.to_float(), sample_rate)?,
            samples(event.loop_end_seconds.to_float(), sample_rate)?,
        );
        if bounds.0 >= bounds.1 {
            return Err(BridgeError::InvalidTransport);
        }
        details.sample_loop = Some(bounds);
    }
    Ok(Transport {
        playing: Some(flags.contains(TransportFlags::IS_PLAYING)),
        sample_position,
        beat_position: flags
            .contains(TransportFlags::HAS_BEATS_TIMELINE)
            .then(|| event.song_pos_beats.to_float()),
        tempo,
        discontinuity: false,
        details: Some(details),
    })
}

fn samples(seconds: f64, rate: f64) -> Result<i64, BridgeError> {
    let value = (seconds * rate).round();
    // i64::MAX rounds up to 2^63 in f64; upper bound must be exclusive.
    if !value.is_finite() || !(-9223372036854775808.0..9223372036854775808.0).contains(&value) {
        return Err(BridgeError::InvalidTransport);
    }
    Ok(value as i64)
}
