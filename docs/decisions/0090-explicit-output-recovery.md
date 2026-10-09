# 0090: Caller-scheduled exact-device output recovery

Status: prototype; controlled transitions and silent native manual restarts.

## Decision and scope

Own an exact output ID, fixed f32 configuration, prepared scratch bound and a
fresh-processor factory. Explicit control-thread service looks up the exact ID,
preserves healthy output, releases missing/failed streams before new attempts,
and starts fresh DSP after return/backend error/panic. No fallback or settings
mutation is implicit. Constructor validates without device/factory work.

Application polling/retry policy remains explicit. No hotplug subscription,
clock compensation, state migration, duplex coordination or native plug-in
lifecycle is added. Enumeration errors preserve live streams; open/start errors
leave none for retry. Block processor faults do not reopen. Disable/reopen/drop
use best-effort pause then stream destruction; backend work may block. Generation
wraps and statistics reset per stream. Consumers own restored state in factories.

## Evidence

Controlled lifecycle tests cover missing/unhealthy/failed-open/healthy/enable/drop
ordering and bounds; constructor does not call factory or native enumeration.
Three silent native UMC1820 output generations exercise explicit reopen and
disable/reenable, with callback/fault and control-thread prepare/deactivate counts.
Physical removal, real backend failures, other platforms, duplex and deadlines
remain unvalidated. Initial panic-only fixture closures needed explicit error
inference; annotated service calls fix compilation.
