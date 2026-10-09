# 0026 — Explicit WAV streams and prepared frame handoff

Status: accepted, 9 October 2026.

Keep file formats and blocking I/O in an explicit crate outside the processing
umbrella. Start with validated seekable RIFF WAV and bounded scratch rather than
claiming codec coverage from a dependency. Reader failure is retryable without
output mutation; partial writer failure poisons the recording. Finalize explicitly.

Use unique SPSC producer/consumer endpoints with atomic f32 bit slots and complete
frame publication. Prepared transfers allocate nothing; caller chooses backpressure
and underflow policies. Powers of two permit safe wrapped frame counters. Workers,
transport seeks and device recording are separate lifecycle integrations.

Independent chunk fixtures, fault injection, quantization and threaded ordering
provide behavior evidence; memory-stream tests do not establish physical recording
or callback deadlines. See [the guide](../audio-files.md).
