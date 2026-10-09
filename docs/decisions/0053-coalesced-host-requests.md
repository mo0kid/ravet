# 0053: Coalesced CLAP host request handoff

Empty host handlers silently discard CLAP lifecycle requests. Provide reusable
shared atomic pending kinds for restart, process and main-thread callbacks while
leaving device/application lifecycle ownership explicit. Prepare mailbox storage
and handler clones outside callbacks; publication/draining never allocates or
blocks. Atomic exchange preserves later racing requests. Repetition coalesces.

Main-thread callback service clears only that bit before invoking plugin code,
services one callback per consumer tick, and retains reentrant requests and other
kinds. Consumers explicitly act on or defer restart/process. No native event loop
or automatic audio transitions are inferred. The instance and mailbox must match.

A linked synthetic CLAP plugin publishes requests during initialization and
reentrantly from on_main_thread; tests check delayed and bounded service, retained
lifecycle flags and concurrent publication. The explicit offline inspection example
services callbacks at pre/post-render points and reports terminal pending actions.
Persistent scheduling and extension-specific GUI/parameter requests remain work.
