# Rust control-thread runtime services

`ravet-runtime` provides explicit file, HTTPS and background-work APIs. These
services allocate, perform I/O or block and must stay outside audio callbacks.
They are optional dependencies, separate from the portable `ravet` umbrella.
`ravet-runtime-ffi` exposes the same Rust implementation to native SDK adapters.
Its C++ header translates ownership and types, not service behavior.

| Runtime responsibility | RAVET API |
|---|---|
| File reads and replacement | `files::read`, `read_bounded`, `atomic_write` |
| Worker scheduling | `tasks::SerialExecutor` |
| Worker timer | `tasks::PeriodicTask` |
| URL POST | `http::post_json` |
| Unix wall-clock milliseconds | `clock::now_milliseconds`, `unix_milliseconds` |
| User data locations | `paths::home`, `application_data` |
| Native module and raw hardware queries | `ravet_runtime_ffi::system` |
| File chooser | Native surface `ravet::surface::openFile` |

These services have the limits described below. Observable state and local web
controls have separate [state](observable-state.md) and [control](web-controls.md)
guides. Additional format mappings, native drawing/layout and broader platform
facilities remain pending.

## Files

Reads return complete binary bytes or an error, distinguish empty from missing,
and read no more than the limit plus one byte, including growing streams.
An unrepresentable limit is rejected. Atomic writes create parent directories,
write an exclusive same-directory temporary, sync contents, then replace the
destination. Failed replacement preserves the destination and cleans the
temporary. Parent-directory metadata is not synced; power-loss durability is
not promised. Encoding, paths and record migration belong to consumers. The
C++ `readFile` convenience maps read errors to empty text; use Rust or the C ABI
status when that distinction matters.

## HTTPS

HTTPS JSON POST uses pinned ureq 3.4.2, rustls and normal platform certificate
verification. The deadline covers connection and response reading. Responses
stream under a caller-selected 0..=16 MiB bound (default 65,536 bytes); oversized
responses return an error rather than a prefix. Requests allow at most 16 MiB
of payload and 1..=300 seconds (default 15). HTTP error statuses retain their
status/body. Transport errors expose no response body. Redirects, cookie storage,
implicit credentials and environment proxy selection are disabled. URLs must
use HTTPS without embedded credentials or fragments. Invalid user-agent headers
are rejected. No network operation occurs until explicitly requested.

There is no automatic product activation operation in RAVET. Explicit local
[signed-entitlement verification](entitlements.md) is a separate optional data API. Endpoints, payload fields,
purchase data, licence keys, verification policy, trial rules, machine-ID
hashing and record paths remain in private consumers. Raw macOS UUID/Windows
SMBIOS queries perform no hashing or transmission.

## Owned work

The serial executor owns `Send + 'static` tasks and captures. Pending capacity
excludes the running task. Full/stopped queues reject work without executing
it; destruction drains accepted work. Panics are contained, though the normal
panic hook can report them. Last-owner teardown inside a worker safely detaches
the join handle. There is no UI dispatch or real-time scheduling guarantee.

Worker timers serialize callbacks, wait one interval before the first tick,
skip catch-up bursts and stop before normal teardown returns. Rust callbacks
own their captures. Callbacks must return; stopping cannot cancel blocked work.
These worker timers do not replace host UI-loop timers.

## Native boundary

The FFI uses opaque handles, explicit statuses and returned byte buffers with a
matching free function. Nonthrowing native callbacks consume their contexts even
on failed submission/creation; context destruction occurs exactly once. Handle
destruction must not race another call on that handle. Native consumers stop
callbacks before destroying objects captured by reference. The Rust core forbids
unsafe code; the separate FFI crate confines pointer/OS operations to narrow
modules. There is no C++ file/network/thread implementation.

Wall-clock conversion handles both sides of the Unix epoch and truncates fractional
milliseconds toward zero; it defines no trial/expiry rule. Deadlines use monotonic
clocks separately. Environment-derived user locations create no directories. Missing home variables
return `None`; Linux ignores relative XDG_CONFIG_HOME. Module lookup takes an
address inside the consumer module, supporting static or dynamic framework links.

Surface's file chooser owns its callback and returns an empty path on cancellation.
Cocoa/GTK complete asynchronously; Windows runs its modal loop. Call on the UI
thread after toolkit initialization, with an optional native window parent.
Save/folder/multi-selection dialogs and Rust dialog bindings remain pending.

## Validation

```sh
cargo test -p ravet-runtime -p ravet-runtime-ffi --locked
cargo clippy -p ravet-runtime -p ravet-runtime-ffi --all-targets --locked -- -D warnings
```

Synthetic localhost TLS fixtures test POST bodies, HTTP error status, redirect
refusal, chunked response bounds, timeout and untrusted-certificate rejection.
Their certificate/key are disposable test identities, not product credentials.
Files, rejected work, owned captures, self-teardown, timers and ABI ownership have
separate checks on macOS arm64 and Omarchy x64. The native facade probe links the
Rust static library in C++17/C++23 on macOS and C++17 on Linux. Native link system
libraries remain platform-specific; the fixture is `crates/ravet-runtime-ffi/tests/native.cpp`. The Cocoa file-dialog probe runs with native editor tests in
[native editor](native-editor.md). Offline tests do not establish DAW/platform
support. See [decision 0020](decisions/0020-rust-runtime-services.md).


## Explicit opt-in analytics queue

`analytics::AnalyticsQueue` starts disabled and chooses no endpoint, timestamp,
identity, background thread, persistence or network transport. A consumer enables
it after its own consent decision. Disabling immediately discards pending events;
while disabled/empty, even an explicit flush never invokes its sender. Exclusive
ownership serializes consent changes and flush. Revocation cannot retract data
already accepted by a sender.

Validated events have a 1..64 ASCII alphanumeric/_/- name, a caller-supplied Unix
millisecond timestamp and up to 64 unique finite numeric metrics with the same
name rules. Encoded event bytes cap at 4,096. There are no text/contact fields or
automatically attached hardware identifiers. Event meanings and collection policy
stay in private consumers; numeric/name fields are not a privacy sanitizer.
Queue limits are 1..1,024 events and 13..1,048,576 bytes for the complete JSON
batch. Overflow rejects the newest event and preserves accepted events.

Explicit worker `flush` passes one bounded JSON batch to the consumer sender.
An error/panic preserves the complete batch for an explicit retry; success removes
it. The sender chooses destination, credentials, timeout and acceptance policy.
For HTTPS, call `http::post_json` and treat only the consumer's accepted status
range as success. This API does not automatically send, schedule or retry; lost
responses can duplicate accepted batches, so delivery is not exactly-once.
All methods allocate/deallocate or can invoke blocking sender code outside audio.

Fixtures prove default-off/revocation/no sender calls, bounded whole-batch order,
valid JSON, retry preservation, invalid/oversized data and overflow. They use an
in-memory sender and transmit no analytics externally. See
[decision 0041](decisions/0041-opt-in-analytics.md).
