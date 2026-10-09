# 0091: Caller-scheduled exact-pair duplex recovery

Status: prototype; controlled transitions and silent native manual restarts.

## Decision and scope

Share the output owner's stop-before-open session mechanics with a duplex stream
handle. Bind exact input/output IDs, matching-rate fixed f32 configurations,
scratch/FIFO bounds and a fresh-processor factory. Missing either ID or either
backend failure/output panic releases the pair. Known missing input stops before
looking up output; other enumeration failures preserve the pair. Healthy streams,
FIFO underflow/overflow and ordinary processor faults do not reopen.

A successful restart owns fresh FIFO/DSP and starts input before output. Failed
open/start leaves none. Disable/reopen/drop release both streams; best-effort
pause precedes potentially blocking destruction. No fallback/settings changes,
physical recording, implicit scheduling, synchronization or drift compensation.
Application owns poll/backoff/state restoration and native plug-in lifecycle.

## Evidence

Pair constructor bounds validate without factory or enumeration; shared controlled
session tests cover removal/failure/retry/enable/drop. Native exact absent input/
output IDs and three silent UMC1820 pair generations exercise fresh open, explicit
reopen and disable/reenable with callback/fault/control-thread lifecycle counts.
This does not establish real hardware disconnect/backend-failure recovery,
clock drift, other-platform support or deadlines.
