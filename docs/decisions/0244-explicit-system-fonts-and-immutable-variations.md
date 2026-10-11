# ADR0244: explicit system fonts and immutable variation instances

Status: accepted contract; local implementation reviewed. Public source
adoption and subsequent qualification are tracked in WORK_STATUS.md.

Keep discovery in an opt-in macOS main-thread WebView service and shaping/rendering
in Surface's existing local owned-font backend. Portable copied catalogue tokens
have process-local owner identity; loaded resources are independent copied SFNT
bytes. Exact style selection rejects unsupported resources rather than substituting.
There is no global registration/download or browser transmission of system bytes.

Native descriptor metadata is not assumed well formed. Actual Eurostile normalized
slant=-97.55555555555556 cannot satisfy the portable trait contract. The private
native path retains validated names/source identity with traits unavailable and a
typed unsupported diagnostic; the ordinary portable constructor stays strict.
Documented Boolean descriptor attributes were observed as CFNumber 0/1, so only
exact finite 0/1 joins CFBoolean admission. Explicit false remains honored, and
missing enabled metadata relies only on the explicit available collection snapshot.
Use IncludeDisabled=false, DisallowAutoActivation=true and PreventAutoActivation
creation; these do not claim unrestricted font availability guarantees.

CoreText copied selected/default tables must match before rebuilding a checked
TrueType SFNT. Source fvar is authoritative. Measured Skia native bounds truncate
four decimals; reconcile only min/max within absolute1e-4 user units with exact
face/tag/default/hidden ownership. Preserve both source/native metadata. Selected
values outside source bounds reject; do not clamp Light/Condensed named instances.
This tolerance does not apply to user coordinates, glyphs, portable metadata or
source tables. Backend user f32 and normalized 2.14 rounding remain explicit.

Admit only fvar1.0 with reserved2, twenty-byte axis records and a checked
axes offset. The backend uses a fixed record stride; future/extended headers
reject before parser interpretations can disagree. This is not whole-font validation.

Instances are immutable and use the same selected coordinates for shaping and
glyf extraction; omitted axes reset to defaults. Bounds/retry tests and first-party
analytic fixtures plus independent HarfBuzz/fontTools and actual CoreText glyph
oracles substantiate the contract. Tight native outline differences are measured
separately from exact advance agreement. A move-only contour in Helvetica 'g'
exposed an independent ink-bounds oracle bug, not a product geometry defect;
preserve its original negative receipt and the corrected segment-only comparison.

A normal standalone paged font/style consumer performs complete load/render/JSON
admission before publication and preserves prior font/image on refused selection.
Bounded exact index/revision commands accept either JSON key order because native
NSDictionary serialization has no order guarantee. The fixture waits an actual
callback acknowledgment rather than assuming a fixed pump interval completes it.
The default Audio/MIDI settings popup remains. Browser/native DOM evidence is
separate from system accessibility/screen-reader certification.
