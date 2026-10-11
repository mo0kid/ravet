# ADR 0253: exact-face wrapped mixed-direction paragraphs

Status: implemented scoped profile; accepted package evidence is separate from
broader UI plans.

## Decision

Add `fallback_paragraph` under the existing optional `text` feature. The caller
supplies an ordered immutable font stack, exact selected variation instances,
complete contiguous grapheme-safe script spans and explicit base direction.
The layout owns original source/style and borrows only those exact faces; no
script guessing, family matching or resource discovery occurs during layout.

Keep two distinct original-source boundaries. Unicode 16 bidi sees full original
paragraph text and per-line visual reordering, so U+2028 does not reset an open
isolate. Contextual shaping sees complete hard-domain content between admitted
LF/CRLF/CR/U+2028/U+2029 separators. Soft wrap, script, direction and fallback
items retain that context and true domain BOT/EOT flags. Each glyph cluster maps
to unchanged original UTF-8 bytes; existing caret/edit tokens remain distinct.

Choose whole Unicode 17 graphemes by actual covered shaping, merge same-face
homogeneous units, and validate final covered glyphs again. Unresolved source
retains explicit replacement ranges. Greedy Unicode 15 break opportunities are
filtered by complete grapheme boundaries; preserve whitespace and oversized
first unbreakable units. Explicit physical alignment stays separate from bidi.

Share coverage/cache/shape/candidate counters across all hard domains and
discarded line measurements, with atomic complete layout publication. These
caps bound admitted own work/output, not arbitrary backend memory or latency.
Both SVG and CPU consumption use exact matching face instances and preflight a
whole operation; SVG restores bytes/command counts on late failure and CPU
prepares an owned geometry object before canvas publication.

## Consumer and evidence

A normal standalone window shares the pure Rust model with portable export.
It keeps exact available Arial Regular, explicit local alternatives, bounded
commands/revisions/payloads and the default Audio/MIDI settings popup. System
font bytes remain memory-only. Layout/render/oversized-document failure retains
prior accepted source/options/image; measured native width reflows independently
of programmatic control changes. PNG is bounded transport of opaque Rust pixels,
whose decoded browser hash is checked, rather than browser font matching.

The first native SVG image settle failed because viewBox-only image transport
has no declared intrinsic dimensions. Actual Chromium measured 158×150 against
expected 112×106. The consumer now supplies explicit width/height without changing
`SvgDrawing`'s existing document API. Preserve that original failure. The separate
WK authored fixture also initially awaited the default settings wrapper's
undefined return instead of image publication; capturing the consumer's async
function in that fixture fixes the test assumption. Ordinary native snapshots
are separately checked through the settings wrapper.

Acceptance includes fixed independent visual/wrap vectors, exact contextual
HarfBuzz IDs/clusters/positions including selected variations, native 41-group
Rust model evidence and separate 65-group authored-image transport receipts in
Chromium/WKWebView. Browser trusted width click/Enter/Space and native scripted
buttons/resize/close are different evidence. Initial browser document focus was
false, later true after trusted interaction. No screen-reader, OS trusted-key,
all-Unicode, vertical/rich-text or audio-deadline claim is inferred.

## Consequences

This completes bounded selected-face paragraph composition and whole renderer
consumption, not automatic system fallback or editing/cache integration. Existing
single-run fallback and single-font paragraph APIs retain their contracts.
No new dependency/version or proprietary/system font asset is added. The pinned
bidi/line/grapheme Unicode versions remain explicit and independent. See the
[paragraph guide](../fallback-paragraphs.md) and
[fixture provenance/oracle](../../crates/ravet-surface/tests/fixtures/fallback-paragraph/README.md).

## Corrected line publication and recovery

The original scalar visual-run boundary rejected legal Arabic trailing ZWJ,
RTL Latin+ZWJ and emoji tag clusters when L1 reset their trailing BN scalars.
The corrected path performs original-scalar L1 then uses the final retained
scalar's level for each whole grapheme before L2; all-X9 units use their own
L1 level. Prepend+Latin/Arabic observations demonstrate why differing retained
levels must also be reconciled rather than rejected. This is an explicit atomic
grapheme rendering refinement, distinct from scalar-exact UAX #9 conformance.

A successfully evaluated native snapshot is cached even if the asynchronous
image verifier rejects it. The consumer now releases busy ownership on rejection,
keeps the last verified image and shows a recovery diagnostic. Exact read-only
`{"refresh":1}` republishes the actual current model revision with a separate
bounded monotonic delivery counter. Equal-revision admission requires an explicit
pending refresh and newer delivery; image/geometry/hash validation remains whole.
No timer or automatic retry is promised. Envelope-capacity failure follows the
same diagnostic-revision publication as preparation failure.

Delivery exhaustion is terminal. Native model command admission and programmatic
transitions refuse edits before mutation at delivery u32::MAX. If the MAX delivery
itself was rejected, a pending explicit refresh receives one terminal envelope
acknowledgment (`refreshExhausted: true`), without modifying state revision or
delivery. It is admitted only for a pending refresh at MAX after all envelope,
image, geometry and pixel validation; it retains the previous verified image,
releases busy ownership, disables further commands and shows reopen guidance.
Unsolicited/replayed terminal acknowledgments have no effect. Own-window fixture
counter injection reaches this boundary without iterating four billion requests.
