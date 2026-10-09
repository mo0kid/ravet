# 0017 — Extended portable processing and block-start transport

Status: portable contracts implemented; CLAP transport integration implemented.
Further format/device mappings remain pending.

## Decision

Generalize borrowed audio and processor/block types over sealed f32/f64 sample
types, defaulting to f32 to preserve existing consumers. Use bounded fixed-array
configuration for up to eight auxiliary buses and 64 channels per bus direction.
Borrow bus views and output event storage per callback. Event writers reject
invalid/full pushes while preserving their accepted prefix; processors explicitly
clear all outputs/events on whole-block faults.

Keep precision/layout support explicit in each processor/adapter. Gain and the
CLAP effect bridge reject extended configurations they do not map. Add default
zero latency/no-tail declarations without implying compensation or host restart
notifications. Add separate CLAP block-start transport conversion with range
validation and accepted-callback continuity tracking. Keep the old process entry
point for callers without transport; flush does not change continuity tracking.

## Evidence and limits

Portable fixtures exercise f64 four-channel sidechain processing, event output,
backing-tail preservation, malformed auxiliary frames and output capacity.
Bridge fixtures exercise optional transport fields, invalid tempo/overflow,
negative positions, continuity/seeks, reset and rejected-block tracking. Native
host transport, output events, auxiliary buses and f64 host negotiation remain
separate work. See [guide](../extended-processing.md) and
[work status](../WORK_STATUS.md).
