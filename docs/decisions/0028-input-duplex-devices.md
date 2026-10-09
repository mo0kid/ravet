# 0028 — Explicit input capture and FIFO duplex effects

Status: accepted for device prototypes, 9 October 2026.

Extend the explicit CPAL adapter with configured f32 input and independently
clocked input/output streams. Reuse prepared whole-frame SPSC handoff rather
than blocking or locking the output callback. Capture overflow drops newest
suffix; output underflow supplies zeros, with atomic counters for both.

Nominal rates must match. This does not synchronize device clocks; automatic
clock drift compensation, sample-exact duplex alignment and round-trip latency
remain separate work. Pause retains queued frames/history; reopen clears them.
Use shared output panic containment. No audio samples are logged or automatically
recorded. Workers choose WAV recording and transport policy.

Offline fixtures verify queue/fault/partition behavior and prepared heap counts.
Physical callback evidence is recorded separately in work status. See
[the device guide](../audio-devices.md).
