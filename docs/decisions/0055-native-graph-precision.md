# 0055: Native precision in prepared graphs

Extend the scheduler from f32 to a single explicitly selected f32/f64 precision,
superseding the precision limit in decision 0029. Generic graphs, node processors,
route gains and scratch/delay storage all use the same sealed sample type. No
sample conversion is hidden in route execution. f32 remains the default.

Graph-owned preparation limits count 64 MiB of bytes rather than assuming four
bytes per sample, preserving the existing f32 ceiling and halving f64 sample
capacity. Topology, alias copying, latency/tail combination, input validation
and explicit nonrollback fault/reset behavior are unchanged. This is still a
static acyclic scheduler, not a Processor implementation or dynamic editor.

Existing f32 tests pass. f64 fixtures preserve sub-f32 resolution through route
gains, parallel latency compensation and channel swaps; split-block/reset checks
and 10,000 render/reset callbacks show zero framework heap operations. Byte
capacity and nonfinite native gains reject. Native f64 host/device runtime and
deadlines remain separate validation.
