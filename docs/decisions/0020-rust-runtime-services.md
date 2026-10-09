# 0020 — Reusable Rust runtime services, private consumer policy

## Context

Removing a consumer dependency alone does not build reusable framework APIs.
General files, networking and worker ownership implemented in product adapters leave the next
consumer repeating work. On 9 October 2026 the owner confirmed that RAVET should
remain Rust wherever possible, with product authorization outside the framework.

## Decision

Implement bounded files/HTTPS, owned workers/timers and user paths in the safe,
explicit `ravet-runtime` Rust crate. `ravet-runtime-ffi` exposes narrow native
ownership and OS module/raw-hardware queries. C++ translates types/lifetimes only;
it does not implement storage, scheduling or HTTP. Neither package enters the
portable umbrella graph. Native file dialogs remain small surface OS UI shims;
their Rust binding is pending.

Consumer endpoints, licensing/trial rules, hashing/salts, record paths, payloads
and credentials stay private. Generic mechanism includes no automatic activation,
telemetry or background network requests.

Accepted tasks own inputs; queue rejection destroys them without execution.
Normal destruction drains accepted work, while worker self-teardown detaches
its join handle safely. Timers own callbacks and serialize calls. Foreign callers
must respect the documented pointer, thread and teardown contracts.

## Consequences and limits

ureq/rustls and tempfile add optional control-thread dependencies. Transport
has explicit limits and normal TLS verification, without redirects or proxy
discovery. Power-loss durability and cancellation of blocked tasks are not
promised. Generic widgets and format adapters still need Rust-first work; these
services do not establish complete framework coverage or DAW support.

## Implementation and evidence

Rust APIs and native facade are implemented and consumed by a private adapter.
Synthetic localhost TLS and files/worker/ABI tests establish scoped macOS arm64
and Omarchy x64 contracts; native facade probes exercise the Rust static library. Cocoa dialog cancellation has a separate owned-window probe.
Executed results and other-platform limits belong in [work status](../WORK_STATUS.md);
contracts are in [runtime services](../runtime-services.md).
