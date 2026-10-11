# 0246: Prepared hosted VST3 events, parameters and block context

Status: prototype implemented; local macOS arm64 native fixtures, safe Rust wire
tests and actual Gain/Performance module evidence only.

## Context

Scoped VST3 instances processed main-bus f32 audio with timed parameter points
only. Hosting instruments and effects needs SDK event lists, output parameter
changes and block-start process context without letting plug-in-owned text,
data or interface pointers escape into Rust, and without allocating on AUDIO.

## Decision

1. Add an exact-paired processing companion table alongside the unchanged
   instance ABI-v2, state and parameter tables. CONTROL refresh copies the event
   bus catalogue, optional MIDI-mapping assignments and context requirements;
   unselected buses stay inactive.
2. Input events, points and context are validated and copied into prepared
   Rust storage before the native call; all nine SDK input kinds and ten output
   kinds use one explicit wire format with bounded counts and payloads.
3. Native processing reserves both queue families before any mutation. A
   refusal before dispatch preserves state; a late fault silences valid output
   prefixes, publishes no partial batch and requires child retirement. Reports
   distinguish refusal, attempted, returned and thrown outcomes.
4. The containing native core owns retained SDK batches, controller-handler
   keepers and a complete numeric ownership snapshot, so foreign callbacks
   cannot write into sibling instances or reclaimed storage.

## Consequences

Output capacity is explicit; insufficient capacity faults the child after DSP.
No DAW, third-party plug-in, foreign-heap or deadline claim follows. Native
f64, auxiliary buses and Graph ownership are a separate decision (0268).
