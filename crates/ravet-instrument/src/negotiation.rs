//! Opt-in, single-zone MIDI RPN negotiation; no device or MIDI-CI activity.
use crate::{
    MidiProgramDecoder, MidiProgramRequest, MpeZone, VoiceAllocator, VoiceChanges, VoiceError,
    VoiceMode, ZoneSide,
};
use ravet_foundation::{MidiKind, MidiMessage};

/// One absolute Data Entry update to a selected registered parameter.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RpnUpdate {
    pub channel: u8,
    pub parameter: u16,
    pub msb: u8,
    pub lsb: u8,
    /// True for CC6; false for CC38. RPN6 uses only CC6.
    pub coarse: bool,
}
#[derive(Clone, Copy)]
struct Selection {
    registered: bool,
    msb: u8,
    lsb: u8,
    data: Option<u8>,
    fine: u8,
}
impl Default for Selection {
    fn default() -> Self {
        Self {
            registered: false,
            msb: 127,
            lsb: 127,
            data: None,
            fine: 0,
        }
    }
}
/// Fixed per-channel RPN selector and absolute Data Entry decoder.
/// CC101/100 may arrive in either order. NRPN selection (CC99/98), CC121
/// and System Reset invalidate pending RPN data. Null RPN (127/127) emits
/// nothing. Changing selection clears data; CC38 requires a preceding CC6
/// for that selection. Each CC6 infers an omitted LSB as zero. Increment/
/// decrement (CC96/97) and NRPN values are deliberately not decoded.
#[derive(Default)]
pub struct RpnDecoder {
    channels: [Selection; 16],
}
impl RpnDecoder {
    pub fn reset(&mut self) {
        self.channels.fill(Selection::default());
    }
    pub fn apply(&mut self, message: MidiMessage) -> Option<RpnUpdate> {
        if message.kind() == MidiKind::Reset {
            self.reset();
            return None;
        }
        let MidiKind::ControlChange {
            channel,
            controller,
            value,
        } = message.kind()
        else {
            return None;
        };
        let state = &mut self.channels[usize::from(channel)];
        match controller {
            101 | 100 => {
                if !state.registered {
                    *state = Selection::default();
                    state.registered = true;
                }
                let selected = if controller == 101 {
                    &mut state.msb
                } else {
                    &mut state.lsb
                };
                if *selected != value {
                    *selected = value;
                    state.data = None;
                    state.fine = 0;
                }
                None
            }
            98 | 99 | 121 => {
                *state = Selection::default();
                None
            }
            6 | 38 if state.registered && (state.msb, state.lsb) != (127, 127) => {
                if controller == 6 {
                    state.data = Some(value);
                    state.fine = 0;
                } else {
                    state.data?;
                    state.fine = value;
                }
                Some(RpnUpdate {
                    channel,
                    parameter: (u16::from(state.msb) << 7) | u16::from(state.lsb),
                    msb: state.data?,
                    lsb: state.fine,
                    coarse: controller == 6,
                })
            }
            _ => None,
        }
    }
}
/// Voice changes, accepted zone/range configuration and optional program request.
/// A request does not commit sound state or alter voices; consumers perform exact
/// catalogue lookup/publication under their own switching policy.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct NegotiatedChanges {
    pub voices: VoiceChanges,
    pub configuration_changed: bool,
    pub program_request: Option<MidiProgramRequest>,
}
/// Explicit opt-in receiver for one chosen lower or upper MPE zone.
/// Starts polyphonic with a two-semitone bend range. RPN6 on the chosen
/// master enables 1..15 members or disables with zero; other masters and
/// values >15 are ignored. Accepted zone messages kill all slots/controllers
/// (including release tails), set member/master ranges to 48/2, and report
/// changed occupied slots so the DSP owner can stop them. Disabling returns
/// to polyphonic 2/2. Repeated valid zone messages also reset.
///
/// In MPE mode RPN0 CC6 values 0..96 set the independent master range or
/// shared member range, refreshing held/sustained snapshots. CC38 fractional
/// ranges are ignored. Polyphonic RPN0, overlapping zones, MIDI-CI, synthesis/format export are outside this receiver.
/// Polyphony accepts bank/program requests independently on all channels; an
/// active zone accepts them only on its master. Zone configuration/reset clears
/// program selectors/requests; bend-range changes and CC121 preserve them.
/// Prepared apply/reset/finish neither allocate nor perform I/O.
pub struct NegotiatedVoices<const N: usize> {
    side: ZoneSide,
    decoder: RpnDecoder,
    allocator: VoiceAllocator<N>,
    programs: MidiProgramDecoder,
}
impl<const N: usize> NegotiatedVoices<N> {
    pub fn new(side: ZoneSide) -> Result<Self, VoiceError> {
        Ok(Self {
            side,
            decoder: RpnDecoder::default(),
            allocator: VoiceAllocator::new(VoiceMode::Polyphonic, 2., 2.)?,
            programs: MidiProgramDecoder::default(),
        })
    }
    pub fn mode(&self) -> VoiceMode {
        self.allocator.mode
    }
    /// Current (member, master) ranges in semitones.
    pub fn bend_ranges(&self) -> (f32, f32) {
        (self.allocator.member_range, self.allocator.master_range)
    }
    pub fn voices(&self) -> &[crate::Voice; N] {
        self.allocator.voices()
    }
    /// Last accepted request; not a committed sound. Reset/zone configuration
    /// clears all channels; invalid channels return None.
    pub fn last_program_request(&self, channel: u8) -> Option<MidiProgramRequest> {
        self.programs.last_request(channel)
    }
    pub fn finish(&mut self, index: usize) -> Result<(), VoiceError> {
        self.allocator.finish(index)
    }
    /// Clear notes, controllers and selectors; preserve the negotiated zone/ranges.
    pub fn reset(&mut self) {
        self.allocator.reset();
        self.decoder.reset();
        self.programs.reset();
    }
    pub fn apply(&mut self, message: MidiMessage) -> NegotiatedChanges {
        let mut result = NegotiatedChanges::default();
        if let Some(update) = self.decoder.apply(message) {
            let master = if self.side == ZoneSide::Lower { 0 } else { 15 };
            if update.parameter == 6
                && update.coarse
                && update.channel == master
                && update.msb <= 15
            {
                for (i, voice) in self.allocator.voices.iter().enumerate() {
                    if voice.note.is_some() {
                        result.voices.changed |= 1u128 << i;
                    }
                }
                self.allocator.reset();
                self.programs.reset();
                self.allocator.mode = if update.msb == 0 {
                    VoiceMode::Polyphonic
                } else {
                    VoiceMode::Mpe(MpeZone::new(self.side, update.msb).expect("validated members"))
                };
                self.allocator.member_range = if update.msb == 0 { 2. } else { 48. };
                self.allocator.master_range = 2.;
                result.configuration_changed = true;
                return result;
            }
            if update.parameter == 0
                && update.coarse
                && update.msb <= 96
                && let VoiceMode::Mpe(zone) = self.allocator.mode
                && (update.channel == zone.master() || zone.contains(update.channel))
            {
                if update.channel == zone.master() {
                    self.allocator.master_range = f32::from(update.msb);
                } else {
                    self.allocator.member_range = f32::from(update.msb);
                }
                self.allocator
                    .update_expression(zone.master(), &mut result.voices);
                result.configuration_changed = true;
                return result;
            }
        }
        let status = message.as_bytes()[0];
        if status >= 0xf0
            || match self.allocator.mode {
                VoiceMode::Polyphonic => true,
                VoiceMode::Mpe(zone) => status & 15 == zone.master(),
            }
        {
            result.program_request = self.programs.apply(message);
        }
        result.voices = self.allocator.apply(message);
        result
    }
}
