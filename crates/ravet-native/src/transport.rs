//! Native in-process transport conversion with independently present timelines.
use ravet_foundation::{Transport, TransportDetails};
/// V1 availability bits; flags are playing=1, discontinuity=2, recording=4,
/// loop-active=8, pre-roll=16. Details presence distinguishes unknown/all-false.
pub mod native_transport_presence {
    pub const PLAYING: u32 = 1;
    pub const SAMPLES: u32 = 2;
    pub const BEATS: u32 = 4;
    pub const TEMPO: u32 = 8;
    pub const DETAILS: u32 = 16;
    pub const TIME_SIGNATURE: u32 = 32;
    pub const TEMPO_INCREMENT: u32 = 64;
    pub const BEAT_LOOP: u32 = 128;
    pub const SAMPLE_LOOP: u32 = 256;
    pub const BAR: u32 = 512;
}
/// Fixed C layout matching include/RavetNativeTransport.h. Unavailable fields
/// and reserved are zero. Optional positions/tempo remain unknown rather than
/// guessed. Values retain full i64/f64/i32 ranges within their semantic contract;
/// no fixed-point conversion, rescaling, rounding or extrapolation occurs.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct NativeTransportV1 {
    pub present: u32,
    pub flags: u32,
    pub sample_position: i64,
    pub beat_position: f64,
    pub tempo: f64,
    pub tempo_increment: f64,
    pub beat_loop_start: f64,
    pub beat_loop_end: f64,
    pub sample_loop_start: i64,
    pub sample_loop_end: i64,
    pub bar_start: f64,
    pub bar_number: i32,
    pub signature_numerator: u32,
    pub signature_denominator: u32,
    pub reserved: u32,
}
/// Invalid availability/dependencies/scalars or noncanonical native fields.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct NativeTransportError;
impl std::fmt::Display for NativeTransportError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("invalid native transport record")
    }
}
impl std::error::Error for NativeTransportError {}
fn validate(transport: Transport) -> Result<(), NativeTransportError> {
    if transport
        .beat_position
        .is_some_and(|value| !value.is_finite())
        || transport
            .tempo
            .is_some_and(|value| !value.is_finite() || value <= 0.)
    {
        return Err(NativeTransportError);
    }
    if let Some(d) = transport.details {
        if d.time_signature.is_some_and(|(n, den)| n == 0 || den == 0)
            || d.tempo_increment
                .is_some_and(|value| !value.is_finite() || transport.tempo.is_none())
        {
            return Err(NativeTransportError);
        }
        if d.beat_loop.is_some_and(|(a, b)| {
            !d.loop_active
                || transport.beat_position.is_none()
                || !a.is_finite()
                || !b.is_finite()
                || a >= b
        }) || d
            .sample_loop
            .is_some_and(|(a, b)| !d.loop_active || transport.sample_position.is_none() || a >= b)
        {
            return Err(NativeTransportError);
        }
        if d.bar
            .is_some_and(|(start, _)| transport.beat_position.is_none() || !start.is_finite())
        {
            return Err(NativeTransportError);
        }
    }
    Ok(())
}
impl NativeTransportV1 {
    /// Validate and encode an exact block-start snapshot. Inactive loop ranges
    /// reject; loop bounds require their matching known timeline, bars need beats,
    /// and a finite tempo increment requires a known finite positive tempo.
    pub fn from_transport(transport: Transport) -> Result<Self, NativeTransportError> {
        use native_transport_presence as p;
        validate(transport)?;
        let mut out = Self::default();
        if let Some(playing) = transport.playing {
            out.present |= p::PLAYING;
            out.flags |= u32::from(playing);
        }
        if transport.discontinuity {
            out.flags |= 2;
        }
        if let Some(value) = transport.sample_position {
            out.present |= p::SAMPLES;
            out.sample_position = value;
        }
        if let Some(value) = transport.beat_position {
            out.present |= p::BEATS;
            out.beat_position = value;
        }
        if let Some(value) = transport.tempo {
            out.present |= p::TEMPO;
            out.tempo = value;
        }
        if let Some(d) = transport.details {
            out.present |= p::DETAILS;
            out.flags |= (u32::from(d.recording) * 4)
                | (u32::from(d.loop_active) * 8)
                | (u32::from(d.within_pre_roll) * 16);
            if let Some((n, den)) = d.time_signature {
                out.present |= p::TIME_SIGNATURE;
                out.signature_numerator = u32::from(n);
                out.signature_denominator = u32::from(den);
            }
            if let Some(value) = d.tempo_increment {
                out.present |= p::TEMPO_INCREMENT;
                out.tempo_increment = value;
            }
            if let Some((a, b)) = d.beat_loop {
                out.present |= p::BEAT_LOOP;
                out.beat_loop_start = a;
                out.beat_loop_end = b;
            }
            if let Some((a, b)) = d.sample_loop {
                out.present |= p::SAMPLE_LOOP;
                out.sample_loop_start = a;
                out.sample_loop_end = b;
            }
            if let Some((start, number)) = d.bar {
                out.present |= p::BAR;
                out.bar_start = start;
                out.bar_number = number;
            }
        }
        Ok(out)
    }
    /// Decode canonical v1 fields, rejecting unknown presence/flag bits, unused
    /// nonzero fields, invalid signatures/scalars/dependencies and reserved data.
    /// Used signed zero survives exactly; unused floating fields require +0.
    pub fn to_transport(self) -> Result<Transport, NativeTransportError> {
        use native_transport_presence as p;
        let has = |bit| self.present & bit != 0;
        let transport = Transport {
            playing: has(p::PLAYING).then_some(self.flags & 1 != 0),
            sample_position: has(p::SAMPLES).then_some(self.sample_position),
            beat_position: has(p::BEATS).then_some(self.beat_position),
            tempo: has(p::TEMPO).then_some(self.tempo),
            discontinuity: self.flags & 2 != 0,
            details: if has(p::DETAILS) {
                Some(TransportDetails {
                    recording: self.flags & 4 != 0,
                    loop_active: self.flags & 8 != 0,
                    within_pre_roll: self.flags & 16 != 0,
                    time_signature: if has(p::TIME_SIGNATURE) {
                        Some((
                            u16::try_from(self.signature_numerator)
                                .map_err(|_| NativeTransportError)?,
                            u16::try_from(self.signature_denominator)
                                .map_err(|_| NativeTransportError)?,
                        ))
                    } else {
                        None
                    },
                    tempo_increment: has(p::TEMPO_INCREMENT).then_some(self.tempo_increment),
                    beat_loop: has(p::BEAT_LOOP)
                        .then_some((self.beat_loop_start, self.beat_loop_end)),
                    sample_loop: has(p::SAMPLE_LOOP)
                        .then_some((self.sample_loop_start, self.sample_loop_end)),
                    bar: has(p::BAR).then_some((self.bar_start, self.bar_number)),
                })
            } else {
                None
            },
        };
        let canonical = Self::from_transport(transport)?;
        let float_bits = |v: Self| {
            [
                v.beat_position,
                v.tempo,
                v.tempo_increment,
                v.beat_loop_start,
                v.beat_loop_end,
                v.bar_start,
            ]
            .map(f64::to_bits)
        };
        if canonical != self || float_bits(canonical) != float_bits(self) {
            return Err(NativeTransportError);
        }
        Ok(transport)
    }
}
