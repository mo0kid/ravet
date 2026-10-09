# 0101: Guarded native meter DAW validation

Status: guarded harness implemented; actual REAPER forwarding passes, visual/assistive-tech checks remain separate.

## Decision

Require an exact public fixture marker and empty project before the native meter
harness opens a log or changes any host state. The fixture generator supplies
that marker alongside public synthetic input. Use an explicit isolated app/
configuration and REAPER's command-line script API when native UI automation is
unavailable. Mute project hardware destinations while retaining processing.
Compare all twelve host meter parameters, log window handles separately and
stop playback/close only the test FX window. No global/user configuration,
production project, private licence/session asset or unknown plug-in loading.

## Evidence and external state

Installed Lua guard fixtures reject missing/incorrect marker and nonempty project
before mutation/log creation. The incorrect-marker case caught a missing path
separator; the corrected script passes all three guard cases. Actual isolated REAPER restart reaches a startup
modal (thread sample confirms a modal loop), and CUA returns cgWindowNotFound.
The owner was asked to dismiss scan/evaluation notices while independent work
continues. No successful host/visual/assistive-tech or new deadline claim follows.

After owner readiness, native UI automation recovered the isolated main window.
Loading/running the script through REAPER's action list passed twelve meter values
and native floating-window handle on REAPER 7.82/macOS arm64. Playback stopped and
the editor closed. Preserve the earlier stalled attempt above as history; no
visual/assistive-tech/other-host/deadline support is inferred from this run.
