# 0114: Selected hosted graph note/MIDI input

Status: implemented within the bounded selected-port contract.

## Decision

Select one inactive native note input index and advertised dialect explicitly.
Reuse the fixed prepared input event queue with optional global parameter input.
CLAP notes retain channel/key with wildcard note IDs; MIDI/MPE use short MIDI and
quantized portable note velocities; MIDI2 preserves framed UMP. Do not infer
translation, group routing, MPE configuration or per-note expression policy.

Check at most 256 note ports and 4096 shared input slots on main. No plugin cookie
or borrowed event pointer crosses ownership. Decode/encode the complete portable
input before native process, preserving offsets and same-frame order. Reject
performance in zero-frame blocks before any parameter prefix can flush: CLAP
params.flush is not note processing. Note-only profiles do not require params.
Metadata changes require retirement and new preparation.

## Evidence and limits

Linked codec tests cover short padding, normalized velocity rounding, native
Pckn and framed word preservation. Synthetic mixed streams exercise f32/f64,
all four dialects, strict rejection and no callback heap actions. The public
fixture checks index 1 versus ID 101 and exact native target/payload. Loaded
fixture graph routing and loaded public sine instrument provide separate actual
binary evidence on macOS arm64. This does not establish arbitrary plugin, DAW,
device, platform or real-time deadline support. Native output, gestures,
modulation, note expressions and note-port rescan service remain separate work.
