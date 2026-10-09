# 0126: Live replacement in buffered native graph workers

Status: prototype within explicit prepared handoff and queue ownership.

## Decision

Share worker queue/scratch/preflight/fault mechanics between fixed and prepared
f32 dynamic engines. Dynamic workers accept an already-prepared engine with exact
configuration, without repreparing one-shot native nodes. Read the engine's fixed
configuration through a nonmutating accessor. Existing graph handoff limits and
control-owned retirement remain; total in-flight graph/native memory is caller
budgeted independently of worker scratch.

Process at most one proposed graph only on a fresh render after prior audio
suffixes publish. Queued copied samples retain their old graph result. Return
optional handoff outcome separately from rendering/publication; pending calls
have none. Failed candidate start and full retirement retain old processing under
existing dynamic policy. Do not crossfade, drain old tails, migrate state, convert
layouts/rates/latency or infer event/GUI scheduling.

A graph may swap and then fail processing. Preserve the latest graph handoff
outcome through `GraphOutputSession::last_swap`, including such failures. It is
historical through pending publication, outer rejection and fault-latch calls;
current successful calls use progress.swap. Faults retain their error until reset
and publish no faulty block. Explicit worker stop/join precedes main engine drop,
retired graph reclaim and each separately retained native control retirement.

## Evidence and limits

Portable tests cover old suffix order, retirement full/reclaim, invalid outer
contracts preserving offers, candidate start failure, queued input/zero shortage,
once-only generated prefixes, post-swap processing faults and 1000 heap-free
steady blocks. The loaded public two-instance Gain probe replaces an engine while
an old block is partially published, reclaims native ownership on main while new
processing continues, checks 1000 steady blocks and zero heap actions. Offline
macOS arm64 only; no device/GUI/DAW, crossfade/latency/clock/deadline/production
recovery or other-format guarantee.
