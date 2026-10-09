# 0018 — Explicit standalone output devices

Status: output-only prototype implemented; broader device programme pending.

## Context

Standalone processing needs device selection and a native callback boundary.
These dependencies must stay outside the portable framework graph. Device buffer
sizes may vary independently of a processor's prepared capacity.

## Decision

Use pinned CPAL 0.18.2 in an explicit `ravet-device` crate. Control threads select
host-local IDs and stream settings and prepare an owned processor before stream
creation. The first adapter accepts f32 output-only processors. Divide callbacks
into bounded blocks using preallocated planar scratch; preserve a logical sample
position across block partitions. Enforce fault/silence output and reject partial
interleaved frames. Report backend faults through atomic counts and require
control-thread reopening. Contain unwinding processor panics and disable DSP.

## Consequences and limits

There are extra planar/interleaved copies and zeroing. No allocation or locking
occurs in normal framework rendering. Backend internals and exceptional panic
unwinding are outside that guarantee. No implicit resampling/format conversion,
duplex synchronization, MIDI scheduling, hotplug recovery or deadline guarantee
is supplied. Deactivation happens when the callback-owned renderer is destroyed
after stream shutdown, on the backend's teardown thread. Processors must allow
that ownership transfer. Portable umbrella users acquire no device dependency.

## Validation

Adapter tests verify partitioning, order, position, limits and fault silencing;
an allocation counter covers 1,000 callbacks. A one-second silent CPAL probe
executed on macOS arm64 with UMC1820 20-channel 96 kHz output. This proves callbacks
on that configuration, not audible output or device latency. See
[device guide](../audio-devices.md) and [work status](../WORK_STATUS.md).
