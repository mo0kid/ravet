# 0277: Device nominal-rate admission before playback

Status: Implemented; device-free validation only. Physical-device, virtual-device,
audible pitch and later route/rate-change qualification remain separate.

## Context

Processors were prepared at requested CPAL stream rates without checking the
rate the device accepted. The reported macOS failure was wrong instrument pitch
after selecting 48 kHz. A rate held by another process can refuse a change;
a listener timeout can also occur when the nominal rate already equals the
request. Returning an opaque backend error prevents callers from choosing a
known current rate. CPAL 0.18.2's CoreAudio defaults query a stream ASBD through
`kAudioDevicePropertyStreamFormat`, rather than the nominal-rate property.

## Decision

Use one shared control-thread stream admission function around all seven direct
CPAL builds. Query after successful construction; drop a mismatched or unverifiable
stream before returning it. Recheck input after duplex output builds to detect
shared-device rate changes. Higher graph/file/recording/recovery adapters continue
using these openers. Do not move hardware queries into portable DSP or callbacks.

On CoreAudio, resolve the public CPAL UID through the system object's
`kAudioHardwarePropertyTranslateUIDToDevice`, then read global/main
`kAudioDevicePropertyNominalSampleRate`. The small private FFI module owns its
created CFString and validates OSStatus, output sizes, unknown device IDs and
finite positive integral u32 rates. It uses existing system frameworks and adds
no Cargo dependency. No CPAL private layout or AudioDeviceID is assumed.
Other backends retain a direction-specific current CPAL default-config rate proxy;
it does not establish a measured hardware clock or cross-platform negotiation.

Expose `device_nominal_sample_rate` and explicit `SampleRateMismatch` variants in
the four stream-opening error enums. Keep their provisional exhaustive contracts;
callers must add a match arm. Clocked input errors retain their directional nested
`Input(OpenError)` wrapper. Reclaimable errors continue returning pending retirement
proofs even when a successfully constructed native stream was rejected.

Map only explicit CPAL rate-update timeout/failure or rate-listener disconnection
messages to mismatch when the query succeeds. Preserve unrelated errors and the
original build error when querying fails. Equal requested/actual timeout values
are intentional: no usable stream exists. Do not silently retry or start at a
new rate; callers must prepare again and decide whether to retry.

Standalone windows delegate devices to their consumers. Add an optional typed
snapshot signal showing requested/actual values and an explicit supported-rate
selection button. New output selections prefer the current nominal rate; supported
saved selections keep their existing restore behavior. The public interactive
examples supply nominal snapshots and typed failure signals, including independent
input failures, without automatic start.

## Validation and limits

Device-free tests cover equality, mismatch, failed query, explicit rate messages,
equal-rate timeout, unrelated backend errors, shared-clock second-build changes,
CoreAudio numeric/receipt checks, and ordinary/paired callback retirement ownership.
The actual popup script has a device-free DOM/bridge fake checking nominal defaults,
supported saved settings, actual-rate selection, equal-rate/unsupported-rate errors,
independent/stale input signals and explicit Start. A hidden WebKit fixture is also
provided; native execution requires an environment permitting WebKit services.
No tests enumerate or open real devices. Admission is an opening-time snapshot;
later rate changes, rerouting and virtual-device conversion need separate runtime
handling and target evidence. See [work status](../WORK_STATUS.md).
