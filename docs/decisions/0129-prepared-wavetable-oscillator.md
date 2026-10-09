# 0129: Prepared periodic wavetable oscillator

Status: portable implementation; offline evidence only.

Use a caller-defined uniformly sampled f64 periodic cycle, 2..65536 finite samples
at phases i/N without a duplicated endpoint. Preparation validates rate/frequency
and complete table before allocation, then owns an immutable boxed copy. Linear
interpolation wraps final-to-first; sample emission precedes phase advancement.
A private phase accumulator shares validation/retuning/wrapping with the existing
analytic/polyBLEP oscillator without changing its output/profile. Tables choose
waveform, DC and level; no generated waveform presets or smoothing are inferred.

The 512 KiB maximum owned table excludes allocator/object overhead. Processing,
reset and retuning allocate nothing; prepare/clone/drop belong outside processing.
Retuning rejects invalid frequencies before state mutation and preserves phase;
reset retains frequency/table. This lookup is neither bandlimited nor alias-free.
No headroom/denormal/output sanitation or native deadline claim is imposed.

Tests cover exact cycle landmarks/interpolation/wrap, non-power-of-two tables,
analytic sine interpolation accuracy, phase-retaining changes, reset/partition,
invalid configuration/table rejection before allocation, maximum capacity and
10000 sample/mix/reset heap instrumentation. Existing analytic oscillator tests
remain regression evidence. No format/device/DAW support follows from these tests.
