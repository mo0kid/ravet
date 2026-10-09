# 0112: Prepared hosted graph global parameter input

Status: implemented explicit f32/f64 input-only profile.

Keep audio-only factories/config rejection unchanged. Add explicit parameter
factories selecting main-only/packed audio, mapping product graph IDs to current
native IDs and requesting bounded 1..4096 input events. Inspect current parameter
metadata on main before activation; unique graph/native IDs, existence/writable
flags and validated plain ranges/steps are required. Store no metadata cookies or
borrowed native pointers. Sorted mappings use bounded binary search in callbacks.

Validate shape/timing/whole input before native calls. Only global ParameterValue
is admitted; strict unknown-ID/other-kind/range/step rejection preserves native
history. Values are physical f32 widened to native f64, independent of audio
precision. Preserve offsets/same-frame order and empty global cookies. Native
parameter event storage allocates on main and never grows on audio; its private
Send boundary is justified by constructors that retain no pointers/cookies and
only scoped immutable native reads. It does not grant arbitrary Clack events Send.

Positive-frame calls deliver input to process. Zero-frame input dispatches active
parameter flush on the audio owner, without DSP frames/steady-time advancement;
diagnostics count dispatch, not acknowledgement. Output is void in this profile.
No per-note/gesture/MIDI/output mapping, recording/GUI automation or implicit
metadata refresh. State/rescan/range changes require orderly stop/reclaim and a
fresh prepared snapshot. Plain values do not infer normalization or automation
permission policy beyond selected writable identities.

Linked dual-format stepped fixture verifies mapping/targets/cookies, same-frame
ordering, invalid whole-block preservation, zero flush and heap checks. Linked
real Gain independently checks smoothing/bypass timing and flush. Loaded public
fixture passes dynamic wet/dry routes/swaps/flush/main retirement in both formats
on macOS arm64; broad compatibility/device/platform/deadline claims remain separate.
