# 0145: Consumer-selected native processor entry

Status: reusable native entry and public Gain export implemented and validated
through Rust/C/C++ and rebuilt native bundles. Generic format factories/state/
editors remain separate work.

Compose prepared planar adaptation and v1 event/transport conversion in a generic
opaque processor owner with one immutable consumer-selected entry symbol. Select
native f32/f64 implementations through explicit table precision; require Default
and Send processors for construction and stopped transfer to their audio owner.
Expose create/destroy/prepare/reset/deactivate/process and prepared latency/tail
query callbacks. Keep SDKs, product metadata/state/editor and registration outside
the core rather than encoding Gain conventions into this table.

Use a fixed v1 configuration with complete main/auxiliary layouts and explicit
capacities; reject unknown fields/unused buses before preparation. Allocate copied
planes, generated events and decoded input storage on control. Require complete
prepared native output capacity before DSP. Decode all input records and supplied
transport before invoking the processor; preserve complete generated records and
sample precision without heap actions on framework normal callback paths.

Foreign arrays/ranges/handle/table precision and library lifetime remain caller
obligations. Metadata/result/event storage cannot overlap writable samples. Copy
all inputs before outputs, silence supported declared prefixes on faults and keep
backing tails. Faults expose zero generated events without rolling back progressed
DSP. Panics poison the owner until fresh stopped preparation; reset/deactivation
cannot conceal poison. Control callback unwinding is contained; consumer aborting
panics/destructors and exception handlers carry no recovery/deadline promise.

Publish the generic entry alongside Gain's existing ABI and compare actual-library
processing across both public interfaces. Synthetic entries exercise multibus,
timed/scoped/generated events, exact context and exceptional lifecycle paths.
This supplies a reusable foreign processor boundary, not SDK registration,
parameter/state protocols, host notifications or automatically supported formats.
