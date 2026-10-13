# 0278: Classify stream notifications before recovery

Status: Implemented; device-free classification, recovery and callback-retirement
validation. No new physical-device, platform or real-time deadline qualification.

## Context

CPAL 0.18.2 invokes its error callback for both failures and stream notifications.
On CoreAudio, `kAudioDeviceProcessorOverload` produces `ErrorKind::Xrun`; a
default-device route change produces `ErrorKind::DeviceChanged` while the stream
remains active. RAVET counted every invocation as `backend_errors`. A single late
startup buffer could therefore latch an audio fault in a consumer and make
caller-scheduled recovery rebuild an otherwise active stream.

## Decision

All seven direct CPAL error callbacks use one shared atomic classification helper,
including both error guards in reclaimable clocked duplex. Higher graph, file and
recording paths inherit it through their existing stream openers. Match CPAL's
error kind, never its message: `Xrun` increments `xruns`, `DeviceChanged` increments
`route_changes`, and all other (including future unknown) kinds increment
`backend_errors`. Each invocation increments exactly one counter. No callback
logging, allocation, device queries, retry or stream lifecycle work is introduced.

Add public `u64` fields `xruns` and `route_changes` to `OutputStatistics` and
`InputStatistics`. Nested stream statistics and retirement receipts retain both
without duplicating them into unrelated FIFO/clock/file statistics. Counters
retain wrapping relaxed-atomic semantics and reset with a fresh stream. Consumers
constructing or destructuring these provisional structs exhaustively must account
for the fields. The tutorial fault indicator and probe failure checks continue
using genuine backend errors/panics (and their existing DSP checks).

Recovery health remains based on `backend_errors` and existing panic counters.
Xruns and automatic rerouting alone preserve the live stream/pair and generation.
Explicit reopen, exact-ID disappearance, enumeration error handling, retirement
ownership and ordinary DSP/FIFO fault policies remain as before. This deliberately
classifies every other CPAL kind conservatively as a backend failure; no additional
kind-specific retry policy is inferred.

## Validation and limits

Constructed CPAL errors test both input/output snapshots for every current error
kind, including `DeviceNotAvailable`, and misleading message text. Tests cover
counter overflow and heap-free classification, notification-preserving recovery
generations, genuine failure/panic recovery, and notification retention across
ordinary and paired callback retirement. No device is enumerated or opened.

Notification counts are backend-dependent and cannot establish audible quality,
clock alignment, later rate negotiation or callback deadlines. Device-free tests
do not qualify real route changes or physical overload behavior. See the
[device guide](../audio-devices.md) and [work status](../WORK_STATUS.md).
