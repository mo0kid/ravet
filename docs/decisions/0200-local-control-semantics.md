# ADR0200: Local control semantic and disabled ownership

Status: Accepted

## Context

A custom ARIA slider needs the same practical disabled admission as native inputs.
The existing knob supplied a role and numeric state but still edited under
aria-disabled. An explicit meter label could also lose to a pre-existing label
reference. Consumer transport/formatter callbacks can retire nodes synchronously.

## Decision

Add bounded boolean knob enabled state, native paired-number disabling and exact
owned tabindex/disabled restoration. Retain healthy disabled host refresh under
existing focused/active guards. Disabled or unavailable controls admit no new UI
edit; refused ends retain original ownership and block further edit/re-enable until
accepted retry. Original-document observation/explicit refresh supplies lifecycle
checks; foreign retirement performs recovery without new presentation writes.

Reserve document ownership and guard transport callbacks. Recheck eligibility after
own/competing end callbacks before new host messages, and after accepted sends before
DOM painting. Preserve accepted begin ownership when callback retirement requires
end recovery. A host-accepted value/edit cannot be rolled back by UI rejection.
Consumers own acknowledgement, document retirement and global focus transfer.

Meter explicit label temporarily removes competing aria-labelledby and restores
original name attributes. Validate a live original meter/reading pair and recheck
it after guarded formatter callbacks; reject recursive updates and stale display
writes after disposal/removal/adoption. Do not implement the accessible-name
algorithm or claim platform accessibility from DOM attribute checks.

## Evidence and limits

Actual Chromium/hidden WebKit fixtures exercise all twelve component semantics,
disabled traversal/input rejection, retryable ownership, transport/formatter
reentry and retirement, exact cleanup and host/UI acceptance boundaries. Trusted
Chromium keyboard evidence exercises enabled navigation and disabled rejection.
Preview document.hasFocus=false, DOM-derived semantic observations and lack of
actual screenreader/native accessibility-tree evidence remain explicit. No new
runtime dependency, native-window ownership or platform certification is supplied.
