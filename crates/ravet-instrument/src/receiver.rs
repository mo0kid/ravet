//! Two-zone MPE voice receiver, separate from legacy single-zone note policy.
use crate::{
    MidiProgramDecoder, MidiProgramRequest, MpeZone, NegotiatedChanges, RpnDecoder, Voice,
    VoiceAllocator, VoiceError, VoiceMode, ZoneSide,
};
use ravet_foundation::{MidiKind, MidiMessage};
/// Fixed 1..128-slot shared-pool lower/upper MPE receiver.
/// Starts in ordinary polyphony. RPN6 on channels 0/15 configures 0..15
/// members. Latest configuration wins overlaps by shrinking/disabling the other
/// zone; an inactive master's channel can become the other zone's member.
/// Every accepted configuration kills all occupied slots/controllers, including
/// unrelated-zone tails, and resets changed zones' bend ranges to 48/2.
///
/// Active zones permit multiple notes per member and compatibility master notes.
/// Expression combines member and master bend separately; pressure/timbre remain
/// separate snapshots for the DSP owner to combine. Per-member expression freezes
/// after note-off, including sustained notes, while master expression continues
/// across the zone's held/sustained/released slots. Member poly-pressure and
/// zone-only sustain/sostenuto/reset/all-notes/all-sounds controls are ignored.
/// Master poly-pressure applies to matching compatibility notes. Unassigned
/// channels are ignored while any zone is active; both disabled use polyphony.
///
/// RPN0 whole-semitone 0..96 ranges are independent per zone/master/member group;
/// fractional updates are ignored. Reset clears notes/controllers/selectors and
/// preserves zones/ranges. Master CC66 captures held zone notes on its rising
/// edge; member CC66 is ignored. MIDI mode switching, sound catalogue/commit,
/// other pedals, MIDI-CI, native device/DAW conformance and synthesis are separate.
/// Bank/program requests route only through active zone masters, or independently
/// through all channels in ordinary polyphony. Requests never alter voices or
/// commit a sound. Zone configuration/reset clears selectors/requests; bend-range
/// changes and CC121 preserve them. Consumer handles recall/publication.
/// Apply/reset/finish use fixed storage without heap, locks or I/O.
pub struct MpeReceiver<const N: usize> {
    allocator: VoiceAllocator<N>,
    decoder: RpnDecoder,
    zones: [Option<MpeZone>; 2],
    ranges: [(f32, f32); 2],
    programs: MidiProgramDecoder,
}
impl<const N: usize> MpeReceiver<N> {
    pub fn new() -> Result<Self, VoiceError> {
        let mut allocator = VoiceAllocator::new(VoiceMode::Polyphonic, 2., 2.)?;
        allocator.mpe_receiver = true;
        Ok(Self {
            allocator,
            decoder: RpnDecoder::default(),
            zones: [None, None],
            ranges: [(48., 2.); 2],
            programs: MidiProgramDecoder::default(),
        })
    }
    /// Lower and upper zones; neither configured means ordinary polyphony.
    pub fn zones(&self) -> (Option<MpeZone>, Option<MpeZone>) {
        (self.zones[0], self.zones[1])
    }
    pub fn voices(&self) -> &[Voice; N] {
        self.allocator.voices()
    }
    /// Last accepted request; not committed sound identity. Reset/any accepted
    /// zone configuration clears all channels; invalid channel returns None.
    pub fn last_program_request(&self, channel: u8) -> Option<MidiProgramRequest> {
        self.programs.last_request(channel)
    }
    /// Configured (member, master) semitone ranges, even for an inactive zone.
    pub fn bend_ranges(&self, side: ZoneSide) -> (f32, f32) {
        self.ranges[usize::from(side == ZoneSide::Upper)]
    }
    pub fn finish(&mut self, index: usize) -> Result<(), VoiceError> {
        self.allocator.finish(index)
    }
    pub fn reset(&mut self) {
        self.allocator.reset();
        self.decoder.reset();
        self.programs.reset();
    }
    fn select(&mut self, index: usize) {
        self.allocator.mode = VoiceMode::Mpe(self.zones[index].expect("selected active zone"));
        (self.allocator.member_range, self.allocator.master_range) = self.ranges[index];
    }
    fn containing(&self, channel: u8) -> Option<usize> {
        self.zones
            .iter()
            .position(|z| z.is_some_and(|z| z.master() == channel || z.contains(channel)))
    }
    pub fn apply(&mut self, message: MidiMessage) -> NegotiatedChanges {
        let mut result = NegotiatedChanges::default();
        if let Some(update) = self.decoder.apply(message) {
            if update.parameter == 6
                && update.coarse
                && (update.channel == 0 || update.channel == 15)
                && update.msb <= 15
            {
                let index = usize::from(update.channel == 15);
                let side = if index == 0 {
                    ZoneSide::Lower
                } else {
                    ZoneSide::Upper
                };
                let old_other = self.zones[1 - index];
                self.zones[index] = if update.msb == 0 {
                    None
                } else {
                    Some(MpeZone::new(side, update.msb).expect("validated members"))
                };
                if update.msb != 0
                    && let Some(other) = old_other
                {
                    let members = other.member_channels().min(14u8.saturating_sub(update.msb));
                    self.zones[1 - index] = if members == 0 {
                        None
                    } else {
                        Some(
                            MpeZone::new(
                                if index == 0 {
                                    ZoneSide::Upper
                                } else {
                                    ZoneSide::Lower
                                },
                                members,
                            )
                            .expect("remaining members"),
                        )
                    };
                }
                for (index, voice) in self.allocator.voices.iter().enumerate() {
                    if voice.note.is_some() {
                        result.voices.changed |= 1u128 << index;
                    }
                }
                self.allocator.reset();
                self.programs.reset();
                self.ranges[index] = (48., 2.);
                if self.zones[1 - index] != old_other {
                    self.ranges[1 - index] = (48., 2.);
                }
                result.configuration_changed = true;
                return result;
            }
            if update.parameter == 0
                && update.coarse
                && update.msb <= 96
                && let Some(index) = self.containing(update.channel)
            {
                if update.channel == self.zones[index].unwrap().master() {
                    self.ranges[index].1 = f32::from(update.msb);
                } else {
                    self.ranges[index].0 = f32::from(update.msb);
                }
                self.select(index);
                self.allocator
                    .update_expression(self.zones[index].unwrap().master(), &mut result.voices);
                result.configuration_changed = true;
                return result;
            }
        }
        if message.kind() == MidiKind::Reset {
            self.reset();
            result.voices.changed = if N == 128 {
                u128::MAX
            } else {
                (1u128 << N) - 1
            };
            return result;
        }
        let status = message.as_bytes()[0];
        if status >= 0xf0 {
            return result;
        }
        if let Some(index) = self.containing(status & 15) {
            self.select(index);
        } else if self.zones.iter().all(Option::is_none) {
            self.allocator.mode = VoiceMode::Polyphonic;
            self.allocator.member_range = 2.;
            self.allocator.master_range = 2.;
        } else {
            return result;
        }
        if matches!(self.allocator.mode, VoiceMode::Polyphonic)
            || matches!(self.allocator.mode, VoiceMode::Mpe(zone) if status & 15 == zone.master())
        {
            result.program_request = self.programs.apply(message);
        }
        result.voices = self.allocator.apply(message);
        result
    }
}
