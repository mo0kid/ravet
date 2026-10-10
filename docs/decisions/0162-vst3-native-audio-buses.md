# 0162: Prepared VST3 native audio buses

Status: implemented; local code and format validation passed.

## Decision

Add a stopped-control fixed native configuration profile with independent compact
SDK input/output bus indices, retaining logical main/auxiliary processing order.
Every native bus is nonempty, with 0..64 channels per direction and up to 9 buses.
Zero-channel directions consume no SDK slot. Validate config version/reserved/
unused fields, finite positive sample rate,1..65536 frames and1024 event limits;
failed configure retains the previous valid profile. Native DSP preparation may
still reject consumer layouts or resource limits independently.

Complete callback mapping checks exact SDK directional/channel/precision shapes,
aligned pointer arrays/sample pointers, address overflow and disjoint output
sample ranges. Inputs may alias outputs across any buses. Fixed stack range
storage and bounded sorting avoid heap/locks/I/O; no samples/silence flags change.
Foreign live ownership and writable metadata/event/private-storage exclusions
remain caller obligations, not something integer address checks can prove.

Zero frames permit complete omission of both SDK directions or exact prepared
metadata; sample arrays are never read then. Positive frames require every
pointer even when flags advertise silence; no null-input synthesis occurs.
Fault cleanup visits only supported exact output shapes/frames at either known
SDK precision, including a rejected processing-precision mismatch; it skips
invalid sample pointers, clears valid active prefixes and reports precisely the
cleared channel bits while preserving tails. Overlapping valid output ranges are
safe to clear, although normal processing rejects them.

Gain adopts mapping before pending state/transport/queue/DSP progression and
replaces its direct fault pointer walk with bounded cleanup. It retains existing
mono/stereo effect registration; generic speaker arrangements, bus declarations,
SDK instrument factories and voice/controller advertisement are separate work.

## Validation and limits

Normal/fast-math fixtures cover both precisions, instrument/sidechain/output-only/
bidirectional independent indices, aliases across buses, exact/omitted/partial
zero-frame shape, whole malformed metadata/pointers/ranges/overlap rejection,
retained failed destinations/profiles, safe cleanup/tails/flags,10000 regular
maps and1000 maximum576-channel maps with zero C++ new/delete. SDK processor
contracts add overlap rejection before pending restore and corrected callback
recovery. No generic exported instrument/multibus, DAW/device/deadline or other
platform support follows from local fixtures.
