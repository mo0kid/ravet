//! Fixed-capacity voice ownership and configured MIDI 1.0 per-note expression.
//! Explicit MPE negotiation and separate normalized/native performance ownership
//! with portable f32/f64 reference sine synthesis.
//! Device I/O and format export remain separate.
mod negotiation;
mod program;
pub use program::{MidiProgramDecoder, MidiProgramRequest};
mod receiver;
pub use receiver::MpeReceiver;
mod performance;
mod performance_sine;
pub use performance::{
    PerformanceBatchError, PerformanceBatchInfo, PerformanceChanges, PerformanceError,
    PerformanceExpression, PerformancePhase, PerformanceStart, PerformanceVoice, PerformanceVoices,
};
pub use performance_sine::PerformanceSine;
mod synthesis;
pub use negotiation::{NegotiatedChanges, NegotiatedVoices, RpnDecoder, RpnUpdate};
use ravet_foundation::{MidiKind, MidiMessage};
pub use synthesis::{
    INSTRUMENT_LEVEL_ID, SINE_PROGRAMS, SineInstrument, SineInstrument64, SineOptions,
};
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ZoneSide {
    Lower,
    Upper,
}
/// Validated single MPE zone. Lower master=0, upper master=15 (zero-based).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MpeZone {
    side: ZoneSide,
    members: u8,
}
impl MpeZone {
    pub fn new(side: ZoneSide, members: u8) -> Result<Self, VoiceError> {
        if !(1..=15).contains(&members) {
            return Err(VoiceError::InvalidConfiguration);
        }
        Ok(Self { side, members })
    }
    /// Number of member channels (excluding the master).
    pub fn member_channels(self) -> u8 {
        self.members
    }
    pub fn master(self) -> u8 {
        if self.side == ZoneSide::Lower { 0 } else { 15 }
    }
    pub fn contains(self, channel: u8) -> bool {
        if self.side == ZoneSide::Lower {
            channel >= 1 && channel <= self.members
        } else {
            channel < 15 && channel >= 15 - self.members
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum VoiceMode {
    Polyphonic,
    Mpe(MpeZone),
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum VoicePhase {
    Idle,
    Held,
    /// Note-off received, but sustain or a captured sostenuto latch retains it.
    Sustained,
    Released,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct NoteKey {
    pub channel: u8,
    pub key: u8,
}
/// Expression snapshots; released tails retain their last expression.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Expression {
    pub member_bend: f32,
    pub master_bend: f32,
    /// Member/channel pressure, or the retained poly-pressure override.
    pub pressure: f32,
    /// Member/channel CC74 value.
    pub timbre: f32,
    /// Separate MPE master pressure; zero in polyphonic mode.
    pub master_pressure: f32,
    /// Separate MPE master CC74; zero in polyphonic mode.
    pub master_timbre: f32,
}
/// One slot's state. The DSP owner calls `finish` after its release envelope ends.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Voice {
    pub note: Option<NoteKey>,
    pub phase: VoicePhase,
    pub velocity: f32,
    pub release_velocity: f32,
    pub expression: Expression,
    age: u64,
    poly_pressure: Option<f32>,
    sostenuto: bool,
}
impl Default for Voice {
    fn default() -> Self {
        Self {
            note: None,
            phase: VoicePhase::Idle,
            velocity: 0.,
            release_velocity: 0.,
            expression: Expression::default(),
            age: 0,
            poly_pressure: None,
            sostenuto: false,
        }
    }
}
impl Voice {
    /// Whether CC66 captured this note on its off-to-on transition. The latch
    /// remains while its pedal is on, including after note-off; later notes are
    /// not captured. Finishing, stealing, killing or resetting clears it.
    pub fn sostenuto_held(&self) -> bool {
        self.sostenuto
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum VoiceError {
    InvalidConfiguration,
    InvalidIndex,
}
/// Changed-slot mask and optional start. A start can steal a previous note;
/// caller resets/retriggers the DSP slot according to its own click policy.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct VoiceChanges {
    pub changed: u128,
    pub started: Option<VoiceStart>,
}
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct VoiceStart {
    pub index: usize,
    pub stolen: Option<Voice>,
}
#[derive(Clone, Copy, Debug, Default)]
struct Channel {
    bend: f32,
    pressure: f32,
    timbre: f32,
    sustain: bool,
    sostenuto: bool,
}
/// Allocation-free 1..128-slot MIDI voice allocator.
/// Polyphonic duplicate notes allocate separate voices; note-off releases the
/// oldest held match. Steal order: idle, oldest released, oldest sustained,
/// oldest held. Configured MPE allows one held note per member channel; a new
/// note releases that channel's old held/sustained notes before allocating. No RPN/zone
/// configuration messages, MIDI 2.0, overlapping zones or DSP are implemented.
pub struct VoiceAllocator<const N: usize> {
    mode: VoiceMode,
    voices: [Voice; N],
    channels: [Channel; 16],
    age: u64,
    member_range: f32,
    master_range: f32,
    mpe_receiver: bool,
}
impl<const N: usize> VoiceAllocator<N> {
    /// Explicit bend ranges 0..96 semitones. Typical configured MPE ranges are
    /// member=48, master=2; polyphonic mode uses member range for channel bend.
    pub fn new(mode: VoiceMode, member_range: f32, master_range: f32) -> Result<Self, VoiceError> {
        if N == 0
            || N > 128
            || !member_range.is_finite()
            || !(0.0..=96.).contains(&member_range)
            || !master_range.is_finite()
            || !(0.0..=96.).contains(&master_range)
        {
            return Err(VoiceError::InvalidConfiguration);
        }
        Ok(Self {
            mode,
            voices: [Voice::default(); N],
            channels: [Channel::default(); 16],
            age: 0,
            member_range,
            master_range,
            mpe_receiver: false,
        })
    }
    pub fn voices(&self) -> &[Voice; N] {
        &self.voices
    }
    /// All notes/controllers/history clear; configuration remains.
    pub fn reset(&mut self) {
        self.voices.fill(Voice::default());
        self.channels.fill(Channel::default());
        self.age = 0;
    }
    pub fn finish(&mut self, index: usize) -> Result<(), VoiceError> {
        *self.voices.get_mut(index).ok_or(VoiceError::InvalidIndex)? = Voice::default();
        Ok(())
    }
    fn relevant(&self, target: u8, voice: u8) -> bool {
        match self.mode {
            VoiceMode::Polyphonic => target == voice,
            VoiceMode::Mpe(zone) => {
                if target == zone.master() {
                    zone.contains(voice) || (self.mpe_receiver && voice == zone.master())
                } else {
                    target == voice && zone.contains(voice)
                }
            }
        }
    }
    fn expression(&self, channel: u8) -> Expression {
        let state = self.channels[channel as usize];
        let master_note = self.mpe_receiver
            && matches!(self.mode, VoiceMode::Mpe(zone) if channel == zone.master());
        Expression {
            member_bend: if master_note {
                0.
            } else {
                state.bend * self.member_range
            },
            master_bend: match self.mode {
                VoiceMode::Mpe(z) => self.channels[z.master() as usize].bend * self.master_range,
                _ => 0.,
            },
            pressure: if master_note { 0. } else { state.pressure },
            timbre: if master_note { 0. } else { state.timbre },
            master_pressure: match self.mode {
                VoiceMode::Mpe(z) => self.channels[z.master() as usize].pressure,
                _ => 0.,
            },
            master_timbre: match self.mode {
                VoiceMode::Mpe(z) => self.channels[z.master() as usize].timbre,
                _ => 0.,
            },
        }
    }
    fn sustained(&self, channel: u8) -> bool {
        match self.mode {
            VoiceMode::Mpe(z) => self.channels[z.master() as usize].sustain,
            _ => self.channels[channel as usize].sustain,
        }
    }
    fn update_expression(&mut self, channel: u8, changes: &mut VoiceChanges) {
        let master = self.mpe_receiver
            && matches!(self.mode, VoiceMode::Mpe(zone) if channel == zone.master());
        for i in 0..N {
            if let Some(note) = self.voices[i].note
                && self.relevant(channel, note.channel)
                && (self.voices[i].phase == VoicePhase::Held
                    || master
                    || (self.voices[i].phase == VoicePhase::Sustained
                        && !(self.mpe_receiver && matches!(self.mode, VoiceMode::Mpe(_)))))
            {
                let mut expression = self.expression(note.channel);
                if master && self.voices[i].phase != VoicePhase::Held {
                    expression.member_bend = self.voices[i].expression.member_bend;
                    expression.pressure = self.voices[i].expression.pressure;
                    expression.timbre = self.voices[i].expression.timbre;
                }
                if let Some(pressure) = self.voices[i].poly_pressure {
                    expression.pressure = pressure;
                }
                if self.voices[i].expression != expression {
                    self.voices[i].expression = expression;
                    changes.changed |= 1u128 << i;
                }
            }
        }
    }
    fn release(&mut self, index: usize, velocity: u8, changes: &mut VoiceChanges) {
        let channel = self.voices[index].note.unwrap().channel;
        self.voices[index].phase = if self.sustained(channel) || self.voices[index].sostenuto {
            VoicePhase::Sustained
        } else {
            VoicePhase::Released
        };
        self.voices[index].release_velocity = f32::from(velocity) / 127.;
        changes.changed |= 1u128 << index;
    }
    fn next_age(&mut self) -> u64 {
        if self.age == u64::MAX {
            // Bounded stack rank renumbering preserves chronological order.
            let old = self.voices.map(|v| v.age);
            for i in 0..N {
                self.voices[i].age = old
                    .iter()
                    .enumerate()
                    .filter(|(j, a)| **a < old[i] || (**a == old[i] && *j < i))
                    .count() as u64;
            }
            self.age = N as u64;
        }
        self.age += 1;
        self.age
    }
    /// Apply one validated short MIDI message. Messages without implemented
    /// voice semantics are ignored. Sustain/sostenuto threshold is >=64. CC66
    /// captures only held notes on its off-to-on transition. Either pedal can
    /// retain a released key; neither revives an already released tail. In MPE,
    /// both pedals are master-only and affect that zone. CC120 kills
    /// immediately, CC123 releases held notes respecting both pedals, CC121 resets
    /// relevant controllers and releases sustained notes. This is receiver policy,
    /// not a declaration of complete MIDI/MPE conformance.
    pub fn apply(&mut self, message: MidiMessage) -> VoiceChanges {
        let mut changes = VoiceChanges::default();
        match message.kind() {
            MidiKind::NoteOn {
                channel,
                key,
                velocity,
            } => {
                if let VoiceMode::Mpe(zone) = self.mode {
                    if !zone.contains(channel) && !(self.mpe_receiver && channel == zone.master()) {
                        return changes;
                    }
                    for i in 0..N {
                        if !self.mpe_receiver
                            && self.voices[i].note.is_some_and(|n| n.channel == channel)
                            && matches!(
                                self.voices[i].phase,
                                VoicePhase::Held | VoicePhase::Sustained
                            )
                        {
                            self.voices[i].phase = VoicePhase::Released;
                            self.voices[i].sostenuto = false;
                            self.voices[i].release_velocity = 0.;
                            changes.changed |= 1u128 << i;
                        }
                    }
                }
                let priority = |phase| match phase {
                    VoicePhase::Idle => 0,
                    VoicePhase::Released => 1,
                    VoicePhase::Sustained => 2,
                    VoicePhase::Held => 3,
                };
                let index = (0..N)
                    .min_by_key(|&i| (priority(self.voices[i].phase), self.voices[i].age, i))
                    .unwrap();
                let stolen = if self.voices[index].phase == VoicePhase::Idle {
                    None
                } else {
                    Some(self.voices[index])
                };
                let age = self.next_age();
                self.voices[index] = Voice {
                    note: Some(NoteKey { channel, key }),
                    phase: VoicePhase::Held,
                    velocity: f32::from(velocity) / 127.,
                    release_velocity: 0.,
                    expression: self.expression(channel),
                    age,
                    poly_pressure: None,
                    sostenuto: false,
                };
                changes.changed |= 1u128 << index;
                changes.started = Some(VoiceStart { index, stolen });
            }
            MidiKind::NoteOff {
                channel,
                key,
                velocity,
            } => {
                if let Some(index) = (0..N)
                    .filter(|&i| {
                        self.voices[i].note == Some(NoteKey { channel, key })
                            && self.voices[i].phase == VoicePhase::Held
                    })
                    .min_by_key(|&i| self.voices[i].age)
                {
                    self.release(index, velocity, &mut changes);
                }
            }
            MidiKind::PitchBend { channel, value } => {
                let centered = i32::from(value) - 8192;
                self.channels[channel as usize].bend =
                    centered as f32 / if centered < 0 { 8192. } else { 8191. };
                self.update_expression(channel, &mut changes);
            }
            MidiKind::ChannelPressure { channel, pressure } => {
                self.channels[channel as usize].pressure = f32::from(pressure) / 127.;
                self.update_expression(channel, &mut changes);
            }
            MidiKind::PolyPressure {
                channel,
                key,
                pressure,
            } => {
                if self.mpe_receiver
                    && matches!(self.mode, VoiceMode::Mpe(zone) if channel != zone.master())
                {
                    return changes;
                }
                for i in 0..N {
                    if self.voices[i].note == Some(NoteKey { channel, key })
                        && (self.voices[i].phase == VoicePhase::Held
                            || (self.voices[i].phase == VoicePhase::Sustained
                                && !(self.mpe_receiver && matches!(self.mode, VoiceMode::Mpe(_)))))
                    {
                        self.voices[i].expression.pressure = f32::from(pressure) / 127.;
                        self.voices[i].poly_pressure = Some(f32::from(pressure) / 127.);
                        changes.changed |= 1u128 << i;
                    }
                }
            }
            MidiKind::ControlChange {
                channel,
                controller,
                value,
            } => match controller {
                74 => {
                    self.channels[channel as usize].timbre = f32::from(value) / 127.;
                    self.update_expression(channel, &mut changes);
                }
                64 | 66 => {
                    if matches!(self.mode,VoiceMode::Mpe(z) if channel!=z.master()) {
                        return changes;
                    }
                    let on = value >= 64;
                    let previous = self.channels[channel as usize].sostenuto;
                    if controller == 64 {
                        self.channels[channel as usize].sustain = on;
                    } else {
                        self.channels[channel as usize].sostenuto = on;
                    }
                    for i in 0..N {
                        if let Some(note) = self.voices[i].note
                            && self.relevant(channel, note.channel)
                        {
                            if controller == 66 {
                                let latched = if on {
                                    self.voices[i].sostenuto
                                        || (!previous && self.voices[i].phase == VoicePhase::Held)
                                } else {
                                    false
                                };
                                if self.voices[i].sostenuto != latched {
                                    self.voices[i].sostenuto = latched;
                                    changes.changed |= 1u128 << i;
                                }
                            }
                            if self.voices[i].phase == VoicePhase::Sustained
                                && !self.sustained(note.channel)
                                && !self.voices[i].sostenuto
                            {
                                self.voices[i].phase = VoicePhase::Released;
                                changes.changed |= 1u128 << i;
                            }
                        }
                    }
                }
                120 | 123 => {
                    if self.mpe_receiver
                        && matches!(self.mode,VoiceMode::Mpe(zone) if channel!=zone.master())
                    {
                        return changes;
                    }
                    for i in 0..N {
                        if self.voices[i]
                            .note
                            .is_some_and(|n| self.relevant(channel, n.channel))
                        {
                            if controller == 120 {
                                self.voices[i] = Voice::default();
                                changes.changed |= 1u128 << i;
                            } else if self.voices[i].phase == VoicePhase::Held {
                                self.release(i, 0, &mut changes);
                            }
                        }
                    }
                }
                121 => {
                    if self.mpe_receiver
                        && matches!(self.mode,VoiceMode::Mpe(zone) if channel!=zone.master())
                    {
                        return changes;
                    }
                    for c in 0..16 {
                        if self.relevant(channel, c) || c == channel {
                            self.channels[c as usize] = Channel::default();
                        }
                    }
                    for i in 0..N {
                        if self.voices[i]
                            .note
                            .is_some_and(|n| self.relevant(channel, n.channel))
                        {
                            self.voices[i].poly_pressure = None;
                            if self.voices[i].sostenuto {
                                self.voices[i].sostenuto = false;
                                changes.changed |= 1u128 << i;
                            }
                        }
                    }
                    self.update_expression(channel, &mut changes);
                    for i in 0..N {
                        if self.voices[i].phase == VoicePhase::Sustained
                            && self.voices[i]
                                .note
                                .is_some_and(|n| self.relevant(channel, n.channel))
                        {
                            self.voices[i].phase = VoicePhase::Released;
                            changes.changed |= 1u128 << i;
                        }
                    }
                }
                _ => {}
            },
            MidiKind::Reset => {
                self.reset();
                changes.changed = if N == 128 {
                    u128::MAX
                } else {
                    (1u128 << N) - 1
                };
            }
            _ => {}
        }
        changes
    }
}

impl std::fmt::Display for VoiceError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "voice error: {self:?}")
    }
}
impl std::error::Error for VoiceError {}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn age_renumber_preserves_oldest_held_steal_order() {
        let mut allocator = VoiceAllocator::<2>::new(VoiceMode::Polyphonic, 2., 2.).unwrap();
        for key in [60, 64] {
            allocator.apply(MidiMessage::from_bytes(&[0x90, key, 127]).unwrap());
        }
        allocator.age = u64::MAX;
        let counts = allocation_counter::measure(|| {
            let change = allocator.apply(MidiMessage::from_bytes(&[0x90, 67, 127]).unwrap());
            assert_eq!(change.started.unwrap().index, 0);
            let next = allocator.apply(MidiMessage::from_bytes(&[0x90, 70, 127]).unwrap());
            assert_eq!(next.started.unwrap().index, 1);
        });
        assert_eq!(counts.count_total, 0);
        assert_eq!(counts.count_current, 0);
    }
}
