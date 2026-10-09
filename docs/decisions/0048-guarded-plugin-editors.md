# 0048 — guarded trusted plugin editors

## Decision

Own successful CLAP GUI creation with a main-thread instance-borrowing guard.
Negotiate the explicitly selected API/floating mode, bound embedded sizes and
require parent attachment before embedded show. Track only successful idempotent
show/hide transitions and pair every guard destruction with native destroy.
Expose short-lived plugin handles for main-thread state/parameters without
exposing mutable instance replacement through the guard.

## Consequences and scope

Native parent lifetime remains an explicit unsafe boundary: caller-owned native
windows must outlive the guard on the OS UI thread. Native objects cannot be made
safe merely by accepting a copied Rust Window handle. Parent creation/pumping,
GUI requests, focus/scale and callback scheduling are consumer policy. Audio
activation remains independent, permitting already-active audio owners while a
main-thread GUI exists. No GPU/windowing dependency is added to the portable graph.

## Validation

Missing/unsupported editor checks do not create native resources. Local Cocoa
fixtures pass three RAVET Gain and unmodified upstream Clack EGUI gain create/
attach/show/resize-or-reject/hide/drop cycles, including early scope exit and zero
remaining parent child views. This is native lifecycle/accepted-call evidence;
floating/other platforms, visual hide/focus and accessibility remain unvalidated.
