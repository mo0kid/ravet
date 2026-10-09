# 0105: Caller-scheduled clocked duplex recovery

Status: implemented; physical fault evidence is separate.

A distinct RecoveringClockedDuplex preserves existing matching-rate recovery APIs
while reusing their Session release/retry/generation ownership. Exact IDs and
fixed configurations are validated before native/factory work. Shared clocked
configuration preflight also improves direct opening: invalid clock/rate/tap/
ring/channel configurations now reject before building capture. Sinc variable
preflight centralizes the same inexpensive bounds used by preparation without
coefficient allocations; no additional backend or dependencies are selected.

Missing either device, backend error or DSP panic releases both streams before
fresh raw FIFO, converter/controller and processor construction. Enumeration
errors retain a current pair; ordinary processor faults and FIFO shortages/drops
alone do not reopen. Disable/reopen releases now; enabled service opens later.
Factory state restoration, control-thread polling/backoff and blocking native
calls remain explicit. No fallback, settings changes, timer/hotplug subscription
or native plug-in audio-owner lifecycle is implied.

Constructor/disabled-service tests prove invalid or disabled owners do not call
factories/devices. Shared Session tests establish deterministic release/retry
ordering; a silent exact-ID native probe checks missing IDs, manual reopen and
disable/reenable with processor lifecycle counters. Physical removal/backend
faults, long-session drift, other platforms and deadlines remain unvalidated.
