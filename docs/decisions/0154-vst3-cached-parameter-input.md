# 0154: Prepared VST3 cached parameter input

Status: reusable external-SDK input helpers and actual Gain migration implemented
and validated in normal/fast-math SDK-linked contracts and rebuilt native
references. Broader SDK processing/factory/event integration remains separate.

Prepare separate precision-matched native cache profiles on control. Retain sorted
target IDs/roles, exact cache/table/library lifetime and stopped destruction. Reject
invalid native entry shape, reserved VST3 IDs and duplicate targets before use.
Audio mapping calls cached native ranges only, with explicit read-only policy and
distinct Value/Unknown/Rejected outcomes. Nonautomatable flags do not silently
filter input; consumers choose unsupported-ID/program routing explicitly.

Read each native global point once into bounded stack scratch before invoking any
visitor. Validate all queue/total-point counts, getter results, finite 0..1 values
and per-queue sample offsets, including unknown IDs. Use at most 1024 queues/total
points, 65536 frames and 16KiB point scratch. Preserve queue traversal/equal-offset
order; consumers own stable global merging and bounded expansion. Complete raw
rejection has no visitor effects. Visitor failure may retain private scratch
effects, so publish outputs/apply pending state/invoke DSP only after success.

Classify IEEE finite values by integer inspection of referenced memory, keeping
validation effective under fast-math compilation. Avoid floating NaN sentinels
and by-value malformed floating arguments whose assumptions can be optimized away;
tests inject malformed foreign bytes and run normal/fast-math targets. Host and
visitor callbacks still own their audio constraints and loaded-object lifetimes.

Gain retains its existing native DSP/state/editor/program APIs, but uses both
prepared cache profiles and the shared raw queue walker. Optional program recall
still expands complete gain/bypass values into bounded private events, and stable
insertion retains cross-queue sample/tie ordering. No generic factory, continuous
curve interpolation, gestures/modulation/scoped/note/transport mapping, generated
SDK delivery or host scheduling is inferred.
