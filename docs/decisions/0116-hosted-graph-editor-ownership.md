# 0116: Main-owned editors for hosted graph nodes

Status: implemented within the guarded native editor contract.

## Decision

Expose guarded editor creation through the native graph's retained main control.
Reuse PluginEditor with exclusive instance borrowing instead of copying raw
plugin pointers or creating a second owner. The separately owned native audio
node can run while main owns its GUI. Borrowing prevents reclaim/drop while the
editor exists; destroy it before stopped-token deactivation and instance return.
Add bounded coalesced callback servicing to the editor guard, preserving other
core request flags. Caller owns native parent, OS main thread and event loop.

## Evidence and limits

Linked tests verify absent GUI rejection retains usable main/audio ownership.
The native graph-hosted probe checks three editor cycles with 1000 graph blocks
per cycle, native child cleanup/resize, output, no callback heap actions and
orderly main retirement. Actual bundle evidence is macOS arm64 only. This does
not add automatic GUI request/parameter-flush/restart/device scheduling or
establish focus/accessibility/DAW/platform/deadline support. Native parent lifetime
remains the explicit unsafe editor API contract.
