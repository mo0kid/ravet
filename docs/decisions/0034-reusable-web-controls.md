# 0034 — Reusable local DOM controls

Status: prototype; exercised in the macOS native WebKit gain editor.

Move parameter-widget behavior from the bespoke gain page into the portable
surface package's local script. Consumer markup provides native range/number,
checkbox/select and button elements. An explicit consumer transport supplies
host edits/gestures/actions. No network bridge or host policy is implicit.

One slider owns a document gesture. Starting another must end the previous one;
failed end retains ownership and makes dispose reject. Pointer cancellation,
blur and key release end gestures; repeated navigation input shares ownership.
Host refresh never sends edits and skips focused/active controls. Numeric choices
retain original option spelling. Native queue overflow/reserved end slots,
asynchronous errors and teardown remain consumer/host responsibilities.

Embed once in a trusted inline document capped at 1 MiB after assembly. The
gain CLAP reference consumes this script through a Rust assembly helper. Native
WebKit fixtures check components and repeated complete reference lifecycles.
Native HTML accessibility/focus semantics do not establish an assistive-tech
audit; broader native rendering/layout/themes/fonts and platforms remain open.
