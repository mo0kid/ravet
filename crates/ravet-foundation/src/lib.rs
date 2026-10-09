//! Shared processor contracts, borrowed audio buffers, events, and transport.
//!
//! Safe, planar `f32`/`f64` processing with bounded main/auxiliary buses and
//! borrowed input/output events. Individual processors declare their support.
//! Borrowed parameter schemas and fixed-size versioned state are also provided.
//! This crate is independent of editors, plug-in formats, and device backends.
//! Processing implementations must avoid allocation, deallocation, blocking,
//! and I/O. See [`Processor`] for the lifecycle and failure contract.

mod audio;
mod event;
mod midi;
mod midi2;
pub use midi2::{Midi2Error, Midi2Kind, Midi2Message};
mod parameter;
mod processor;
mod program;
pub use program::{ParameterProgram, ProgramBank, ProgramError, ProgramId};
mod ump;
mod ump_sysex;
pub use ump_sysex::{SysExAssembler, SysExAssembly, SysExForm, UmpSysExChunk, UmpSysExError};

pub use ump::{UmpDecoder, UmpError, UmpMidi1, UmpPacket};

pub use audio::{AudioBuffer, BufferError, Sample};
pub use event::{
    Event, EventData, EventError, NoteExpression, NoteId, NotePhase, NoteTarget, OutputEvents,
    ParameterGesturePhase, ParameterId, Transport, TransportDetails,
};
pub use midi::{MidiDecoder, MidiError, MidiKind, MidiMessage};
pub use parameter::{
    ParameterDescriptor, ParameterError, ParameterRange, ParameterScale, ParameterSchema,
    ParameterState,
};
pub use processor::{
    AudioLayout, MAXIMUM_AUXILIARY_BUSES, MAXIMUM_CHANNELS, PrepareError, ProcessBlock,
    ProcessConfig, ProcessError, ProcessStatus, Processor, Tail,
};

#[cfg(target_has_atomic = "64")]
mod snapshot;
#[cfg(target_has_atomic = "64")]
pub use snapshot::{PublishError, SnapshotMailbox, WordSnapshot};
