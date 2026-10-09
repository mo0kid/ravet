//! Fixed MIDI bank-selection staging; choosing/loading sounds is consumer policy.
use ravet_foundation::{MidiKind, MidiMessage};

/// One received request, not proof that a consumer has a matching program.
/// Channels/banks/programs are zero-based wire numbers. Repeated Program Change
/// emits another request so the consumer can choose explicit recall behavior.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MidiProgramRequest {
    pub channel: u8,
    pub bank: u16,
    pub program: u8,
}

#[derive(Clone, Copy, Debug, Default)]
struct ChannelProgram {
    bank: u16,
    last: Option<MidiProgramRequest>,
}

/// Sixteen independent pending MIDI1 bank selectors and last request snapshots.
/// CC0/32 update their own seven-bit half in either order, initially zero. Only
/// Program Change emits a request, using the latest complete pending bank. Later
/// bank changes never alter the last request. CC121/120/123 leave bank/program
/// requests intact; explicit reset/System Reset clears both. Other messages are
/// ignored. No implicit voice reset, program availability/commit, MPE mapping,
/// callback scheduling, file/host/UI work or MIDI2 decoding. Fixed storage and
/// bounded apply/reset/inspection use no heap, locks or I/O.
#[derive(Clone, Copy, Debug, Default)]
pub struct MidiProgramDecoder {
    channels: [ChannelProgram; 16],
}
impl MidiProgramDecoder {
    pub fn reset(&mut self) {
        self.channels.fill(ChannelProgram::default());
    }
    /// Pending bank; None for an invalid zero-based channel.
    pub fn pending_bank(&self, channel: u8) -> Option<u16> {
        self.channels.get(usize::from(channel)).map(|s| s.bank)
    }
    /// Last received Program Change, including requests for unavailable sounds.
    /// None before the first request, after reset, or for an invalid channel.
    pub fn last_request(&self, channel: u8) -> Option<MidiProgramRequest> {
        self.channels.get(usize::from(channel)).and_then(|s| s.last)
    }
    pub fn apply(&mut self, message: MidiMessage) -> Option<MidiProgramRequest> {
        match message.kind() {
            MidiKind::Reset => self.reset(),
            MidiKind::ControlChange {
                channel,
                controller: 0,
                value,
            } => {
                let state = &mut self.channels[usize::from(channel)];
                state.bank = (u16::from(value) << 7) | (state.bank & 127);
            }
            MidiKind::ControlChange {
                channel,
                controller: 32,
                value,
            } => {
                let state = &mut self.channels[usize::from(channel)];
                state.bank = (state.bank & 0x3f80) | u16::from(value);
            }
            MidiKind::ProgramChange { channel, program } => {
                let state = &mut self.channels[usize::from(channel)];
                let request = MidiProgramRequest {
                    channel,
                    bank: state.bank,
                    program,
                };
                state.last = Some(request);
                return Some(request);
            }
            _ => {}
        }
        None
    }
}
