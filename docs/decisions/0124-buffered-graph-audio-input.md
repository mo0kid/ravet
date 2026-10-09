# 0124: Queued audio input to native graph workers

Status: prototype with portable and loaded public synthetic probes.

## Decision

Extend prepared graph workers with `with_input(graph, consumer, producer, config)`
while keeping the existing no-input constructor. Match explicit graph/ring channel
counts and fixed rates outside the worker; zero auxiliary buses and existing
1024-event graph limit remain. Bound combined planar/interleaved input/output
scratch at 32 MiB, separately from graph/native/queue memory.

Validate outer event/output-storage contracts before input dequeue. Fresh renders
consume one whole-frame input prefix, zero any shortage and publish output once.
Retained output suffixes consume no new input/events, rerender nothing and report
zero input progress. Source/underflow counts describe the rendered block, without
capture/device clocks or resampling. Graph errors may follow input consumption
and native history mutation; they suppress faulty output and retain their error
until explicit reset without further input drain. Malformed outer blocks do not
latch. Reset preserves queued/pending audio and input queue contents.

Keep native graph start/process/stop on the explicit worker; join precedes main
retirement. No device, wake loop, clock recovery, rate conversion or replay is
synthesized. Source producer and output consumer own separate explicit policies.

## Evidence and limits

Portable tests cover exact sample/event timing under partial output publication,
input starvation silence, invalid outer contracts preserving queues, fault latch/
reset, channel/scratch rejection and 1000 heap-free input/output rounds. The loaded
public native gain probe checks 1000 two-channel blocks/16000 input frames,
retained suffixes, a completely starved block, no heap actions and owner retirement.
Offline queued audio does not establish physical capture/duplex, clock alignment,
latency/deadline/long-session/hotplug recovery or broader platform/device support.
