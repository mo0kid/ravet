# 0113: Prepared global native parameter output and node builder

Status: implemented bounded f32/f64 parameter output profile.

Introduce a preparation builder for independent port, transport and input/output
choices. Preserve original factories by delegating to the same private profile.
Inspect current parameter output identities/ranges on main before activation;
read-only IDs are allowed. Each direction's graph/native identities are unique.
Cap metadata/maps/slots at 4096 and allocate all portable output storage on main.
No native cookies or plugin-owned event pointers survive callback scope.

Accept recognized global mapped ParamValue output with native range/step, finite
f32 narrowing, offset/order and capacity checks. Native f64 values round to the
portable event precision and the rounded result must remain valid in cached range.
Filter/count unmapped, unsupported and per-note output. Native event memory remains
a trusted ABI obligation, not a sandboxed validation guarantee. Accepted filtered
output means discard, not delivery acknowledgement.

Mapped rejection poisons the entire block, discards any prefix, silences audio and
returns BackendFailure because native DSP/steady time may have advanced. Verified
successful output gates portable success counters/status. Track filtered/rejected
attempts independently. Zero-frame selected output profiles explicitly flush on
audio, including output-only profiles; input-only/disabled output stays void.
Fresh stopped preparation is required after metadata changes. No implicit retry,
GUI scheduler, recording policy, MIDI/per-note/modulation/gesture output or rescan.

Linked fixtures exercise two precisions across twelve output modes: normal,
range/NaN/overflow/order/offset faults, unmapped/MIDI/per-note filtering, f64
rounding, nonrepresentable f32 and stepped rejection. Output-only empty flush,
capacity/identity checks and builder packed/f64 transport also pass. Loaded dual-
format fixture routes readonly output through static/dynamic graph edges alongside
wet/dry processing, empty flushes, owner swap/stop and main retirement. Heap checks
pass; native evidence is macOS arm64 synthetic, not broad host/device/platform or
physical deadline support.
