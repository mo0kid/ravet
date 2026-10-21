# RAVET licensing

Decision recorded on 8 October 2026: **GPLv3 plus separate commercial licensing**.

## Open-source route

RAVET's first-party Rust and Python source, including its current tests and
examples, is available under the **GNU General Public License version 3 only**
(`GPL-3.0-only`). The full, unmodified licence text is in [LICENSE](LICENSE).
Workspace packages declare that identifier in Cargo metadata.

GPL permits commercial use. This route has no RAVET revenue ceiling or
requirement to buy a commercial licence if the user complies with the GPL.
Distribution of a combined product must satisfy applicable GPL requirements,
including corresponding-source availability and recipients' licence rights.
Making only the RAVET library source available does not automatically satisfy
obligations for a combined plug-in or application. The precise boundary of
separate works depends on the integration; this document grants no blanket
exception for linking or plug-in hosting.

The commercial tier limits below do not modify the GPL or apply additional
restrictions to users choosing it. Third-party dependencies retain their own
licences and notices. This source-code policy does not grant trademark rights.
No separate permissive licence has been selected for the current examples or
future generated templates.

## Commercial route

The owner approved the following launch pricing and commercial policy on
8 October 2026. A separate commercial agreement will permit proprietary
distribution on its agreed terms. **This document is not a commercial licence
grant.** Until an agreement is executed, this repository supplies the GPL route.

| Tier | Annual company revenue (USD) | Approved launch price |
|---|---|---|
| Starter | Up to and including $50,000 | Free |
| Indie | Above $50,000, up to and including $500,000 | $199 per developer per year |
| Pro | Above $500,000, with no upper limit | $699 per developer per year |

Prices are in USD, excluding applicable taxes. Exactly $50,000 qualifies for
Starter; exactly $500,000 qualifies for Indie. Revenue is company-wide, rather
than limited to RAVET-based products, measured over the previous twelve months
at purchase or renewal. These bands identify the minimum qualifying tier.

### Approved commercial policy

- Every tier includes every framework feature, supported platform, and supported
  plug-in format. This is not a promise of functionality not yet implemented.
- Unlimited products and no royalties.
- Paid licences include twelve months of updates and permanent commercial rights
  to the versions received during that period. Renewal purchases further updates;
  developers may continue building and selling products using previously licensed
  versions after renewal stops.
- No separate licence fees for build machines or CI.
- Revenue tier is assessed at purchase or renewal. Company growth does not
  retroactively invalidate earlier licences or their retained distribution rights.
- Community support is available across all tiers. Paid customers receive a direct
  reporting channel. Dedicated engineering support and contractual response times
  are priced separately; the base licence includes no response-time guarantee.
- Start with annual billing. Monthly instalments are a possible future offering,
  not an approved launch option.
- Keep the preview phase free. Introduce paid commercial licensing when RAVET can
  reliably ship production plug-ins with usable editors and documentation. Preview
  pricing does not itself grant permission to distribute proprietary products.

### Commercial agreement draft

A Commercial Licence Agreement draft 0.1, kept privately until it is final,
contains the proposed contract and Order Form. It preserves the approved policy
and lists additional drafting choices for owner and legal review. It is not an
executed licence, and its proposed defaults have not been adopted merely by
being drafted. The repository's GPL metadata and LICENSE remain unchanged.

### Contract details still needed

The pricing and policy above are accepted; the final agreement still needs to
specify:

- Revenue calculation, currency conversion, related entities, and funding treatment.
- Developer-seat definition, contractor coverage, reassignment, and purchases
  partway through a licence year.
- Starter enrolment, eligibility reassessment, and the scope of its retained
  commercial rights; whether customers may voluntarily select a higher tier.
- Exact version coverage, support-channel operation, renewal, and termination
  for breaches of the agreement.
- Rights to modify RAVET, and permitted source/binary redistribution.
- Licence scope and treatment of third-party components.

Have the actual commercial agreement reviewed before offering it to customers.
Do not introduce accounts, activation, telemetry, or runtime licence checks
solely because this licensing decision exists.

## Contributions and dependency selection

Before accepting external contributions intended for the commercial edition,
establish contributor terms granting sufficient rights for both the GPL and
commercial routes, including appropriate patent rights. A contributor may
retain copyright while granting those permissions; copyright assignment is
not inherently required. Keep contributor and provenance records.

A DCO sign-off certifies contribution origin and the right to submit under the
stated licence; it is not, by itself, a grant of proprietary relicensing rights.
Do not promise commercial permissions for contributed code without the rights
to grant them.

Check each dependency's exact licence and linking/distribution obligations for
both routes. GPL-compatible does not necessarily mean compatible with the
commercial agreement. A third-party GPL component cannot simply be sold under
RAVET's commercial terms without the necessary permission or replacement.
Preserve required third-party notices and audit normal, build, development,
vendored, and native dependencies before release.

The Cargo identifier describes the open-source licence currently supplied by
this repository. Do not label it `MIT OR Apache-2.0`, `GPL-3.0-or-later`, or use
an invented standard SPDX identifier for the unfinished commercial agreement.
Keep `publish = false` until release preparation is intentionally completed.

## Primary references

- [GNU GPLv3 licence](https://www.gnu.org/licenses/gpl-3.0.html)
- [GNU licensing FAQ](https://www.gnu.org/licenses/gpl-faq.en.html)
- [GNU explanation of selling licence exceptions](https://www.gnu.org/philosophy/selling-exceptions.html)
- [Developer Certificate of Origin](https://developercertificate.org/)
- [Cargo licence metadata](https://doc.rust-lang.org/cargo/reference/manifest.html#the-license-and-license-file-fields)

## Optional local-font shaping and public test fixture

The default-disabled surface/umbrella `text` feature pins HarfRust 0.14.0 (MIT).
Reviewed downloaded manifests for read-fonts 0.45.0/font-types 0.12.6, bitflags,
smallvec, once_cell and stable_deref_trait declare MIT OR Apache-2.0; bytemuck and
its derive declare Zlib OR Apache-2.0 OR MIT. Yoke/zerofrom and their derives
declare Unicode-3.0; synstructure declares MIT. Locked proc-macro tooling retains
its own notices. Preserve applicable dependency notices in both distribution
routes; this scoped manifest review is not a release-wide licence audit.

The unmodified public Noto Sans Arabic fixture is SIL Open Font License 1.1,
with complete OFL, copyright, upstream commit and hashes retained under
[the fixture directory](crates/ravet-surface/tests/fixtures/text/README.md).
RAVET's first-party licence does not replace that font's terms. The fixture is
reusable testing data, rather than a required product font; consumer fonts/assets
and their separate permissions remain the consumer's responsibility.

The optional text feature also pins unicode-bidi 0.3.18 (MIT OR Apache-2.0), with
its Unicode 16.0 tables and separate upstream notices. Selected unchanged public
Unicode bidi vectors retain the complete Unicode License v3 and exact source/
hash under [their fixture directory](crates/ravet-surface/tests/fixtures/bidi/README.md).
These data terms remain separate from RAVET's first-party licence.

The optional text feature pins unicode-linebreak 0.1.5 (Apache-2.0), whose
Unicode 15.0 line-opportunity data is separate from the bidi backend's Unicode
16.0 data. Its downloaded manifest and complete Apache-2.0 licence were reviewed;
it declares no runtime dependencies. Preserve the applicable upstream notice in
both distribution routes. This scoped dependency check is not a release-wide
licence audit.

The optional text feature pins unicode-segmentation 1.13.3 (MIT OR Apache-2.0),
with Unicode 17.0 extended-grapheme tables and no runtime dependencies. Its
upstream manifest, copyright and complete MIT/Apache licence texts were reviewed;
preserve applicable notices in both distribution routes. The separate line-break
and bidi tables retain their earlier Unicode versions. The tiny synthetic
[ligature font fixture](crates/ravet-surface/tests/fixtures/caret/README.md) contains
only first-party rectangular glyphs and a test substitution; no external font
outlines or product assets are included. This is a scoped dependency check.

The default-disabled `ravet-surface/postscript-outlines` feature (selected by
`ravet-webview/postscript-fonts` consumers and the tutorial `postscript-fonts`
feature) pins skrifa 0.48.0 (MIT OR Apache-2.0) with default features off and
`std` on. Its downloaded manifest and complete MIT (Fontations Developers) and
Apache-2.0 licence texts were reviewed. Its selected runtime dependencies are the
already reviewed read-fonts 0.45.0 and bytemuck; the optional core_maths dependency
is not selected, and offline lock resolution adds only skrifa. The 31 OTF/TTF
fixtures under [the PostScript fixture directory](crates/ravet-surface/tests/fixtures/postscript/README.md)
are first-party authored test data; no installed or proprietary font bytes are
included. See [the skrifa notice](crates/ravet-surface/third-party/skrifa.md).
Preserve applicable notices in both distribution routes. This is a scoped
dependency check, not a release-wide licence audit.

## Loudness calibration provenance

The prepared calibrated loudness profile expresses its rational filter equations
independently using mathematical calibration parameter facts attributed to Brecht
De Man's author reference (MIT, copyright 2018 Brecht De Man). No external
implementation body, dependency, coefficient table or programme fixture is copied.
The complete author reference notice is retained in
[the Signal third-party notices](crates/ravet-signal/third-party/loudness-calibration-brecht-de-man.txt)
for provenance; standards documents are linked as mathematical references, not
redistributed.

## Embedded parameter math

Foundation pins pure-Rust libm 0.2.16 (MIT) for parameter mappings without std.
Its downloaded Cargo manifest and upstream licence were reviewed; preserve its
MIT notice when distributing embedded or desktop dependencies. Default desktop
math still uses std. This scoped dependency review does not replace the complete
release dependency audit or grant hardware compatibility.

`ravet-native-dsp` depends directly on the same locked libm 0.2.16 for no_std
Scalar square root, with default features disabled exactly as in Foundation. Its
downloaded manifest declares MIT only, and its `LICENSE.txt` is the MIT licence.
No new package is resolved; preserve the same MIT notice.

## Optional Ogg Vorbis encoding

The default-disabled audio-file `vorbis-encode` feature pins vorbis_rs 0.5.6
(BSD-3-Clause) with defaults disabled, using caller-selected Ogg stream serials.
Its safe bindings use vendored aoTuV/libvorbis and libogg through locked
aotuv_lancer_vorbis_sys 0.1.6 and ogg_next_sys 0.1.5 (BSD-3-Clause). Reviewed
exact manifests/build scripts and complete upstream copyright/licence notices:
ordinary builds use a C compiler via cc and supplied bindings, with no system
codec library, bindgen/libclang or random-serial feature. The binding crate's
archive omits its workspace-root licence, so the complete upstream bindings
notice and archived codec notices are preserved in
[the backend notices](crates/ravet-audio-file/third-party/vorbis.md). Preserve
these notices with source/binary distributions in both licence routes. Other
locked Rust/build dependencies retain their own terms. The newly locked tinyvec
1.13.3 declares Zlib OR Apache-2.0 OR MIT; its manifest and complete licence texts
were reviewed. This scoped review does
not replace the release-wide dependency audit.

## Foundry TOML settings dependencies

Foundry directly pins `toml` 1.1.6+spec-1.1.0 and uses the existing pinned
Serde/serde_json graph. Its new locked parser/serializer dependencies are
`toml_datetime` 1.1.2+spec-1.1.0, `toml_parser` 1.1.4+spec-1.1.0,
`toml_writer` 1.1.2+spec-1.1.0 and `serde_spanned` 1.1.1, all declaring
MIT OR Apache-2.0, plus `winnow` 1.0.4 declaring MIT. Their downloaded exact
Cargo manifests and complete licence texts were reviewed for this addition.
Preserve upstream copyright/licence notices when distributing Foundry. This
scoped review does not establish a complete release dependency audit. Native
WebView2/WebKitGTK dependencies retain their own platform distribution terms.

## Optional CPU raster backend

The default-disabled `ravet-surface/raster` feature pins tiny-skia and
tiny-skia-path 0.12.0 (BSD-3-Clause), arrayref 0.3.9 (BSD-2-Clause) and
strict-num 0.1.1 (MIT). With `raster` alone, only tiny-skia's `std` feature is selected; PNG loading,
SIMD feature and native Skia are disabled. Preserve the complete
[upstream notices](crates/ravet-surface/third-party/raster.md) in source and binary
distribution. Existing transitive components remain part of the release-wide
dependency audit; this feature review does not establish a release support matrix.

The default-disabled audio-file `mp3-encode` feature pins mp3lame-encoder 0.2.5
and mp3lame-sys 0.1.11 with defaults disabled. Both bindings declare LGPL 3.0;
the bundled unmodified LAME 3.100 encoder mostly grants GNU Library GPL
2-or-later, and ReplayGain analysis grants LGPL 2.1-or-later. Decoder/frontend
features are disabled. Unix builds statically compile the codec with configure,
make and a C compiler. Exact published source commits, verified crate archive
hashes, complete license texts and original component attribution are retained
in [the backend notices](crates/ravet-audio-file/third-party/mp3.md). These
components retain their independent obligations: a RAVET commercial agreement
does not relicense them or certify combined/static-link distribution, source/
relinking compliance or patent clearance. Consumers must review their own
distribution route and retain applicable notices/source rights.

## Optional worker image codecs

The separate default-disabled `gif-image` feature pins gif 0.14.2 and weezl
0.1.10, both independently MIT OR Apache-2.0, with defaults disabled and only
std. The configuration selects no color_quant or native C. Complete original
copyright and licence texts, archive hashes and reviewed commits are preserved
in [GIF notices](crates/ravet-surface/third-party/gif.md). Runtime library
obligations remain independent of RAVET's commercial terms; Pillow is only a
first-party fixture-generation tool.

The default-disabled Surface image feature pins png0.18.1 (MIT OR Apache-2.0)
and zune-jpeg0.5.15/core0.5.3 (MIT OR Apache-2.0 OR Zlib). Reviewed exact archive
hashes, manifests and complete compression/backend notices are retained in
[the image notices](crates/ravet-surface/third-party/image.md). Selected pure-Rust
compression includes fdeflate, flate2, miniz_oxide, adler2, simd-adler32 and
crc32fast; each retains its independent copyright/licence terms. Optional
zlib-rs0.6.8 is locked but unselected here. Preserve applicable notices for
source/binary distribution; this scoped review does not replace release-wide
dependency review or establish a platform/support matrix. First-party image
fixtures contain only generated patterns under the repository licence.

The separate default-disabled `jpeg-encode` feature pins `jpeg-encoder` 0.7.1
with defaults disabled and only `std`, without runtime dependencies, native C
or SIMD. Its exact archive hash, manifest, upstream commit and full licence texts
were reviewed. Upstream declares `(MIT OR Apache-2.0) AND IJG`; preserve the
complete [JPEG notices](crates/ravet-surface/third-party/jpeg.md), including the
original Lane/Commander DCT attribution and required executable acknowledgement:
"this software is based in part on the work of the Independent JPEG Group".
These obligations remain independent of RAVET's commercial licence. This scoped
review does not replace the release-wide dependency and distribution review.

The optional external-SDK VST3 hosting provider links caller-owned SDK hosting
implementation. Its CMake build copies the complete external SDK `LICENSE.txt`
into the build directory; retain that MIT notice in linked distributions. Cargo
only supplies the Rust provider ABI/owner and does not fetch or vendor the SDK.
See [VST3 hosting](docs/vst3-hosting.md).

## Optional SVG image import

Default-disabled `svg-image` pins resvg/usvg0.48.1, direct roxmltree0.21.1,
svgtypes0.16.1 and simplecss0.2.2 with direct defaults disabled. Effective tiny-skia
std/SIMD/PNG and roxmltree positions/std remain enabled through transitive
feature unification. Font/system-font/memmap/SVGZ/writer/raster-image defaults
remain off. Complete independent selected and lock-only
[notices](crates/ravet-surface/third-party/svg.md) preserve exact published
MIT/Apache-2.0/BSD/Zlib alternatives and attribution. The
[first-party fixtures](crates/ravet-surface/tests/fixtures/svg/README.md) contain
no downloaded art or font; the existing outlined RAVET logo remains first-party.

The optional SVG graph under all workspace features also retains notices for
bytemuck derive and its proc-macro tooling, numeric traits/euclid and zlib-rs
selected through feature unification. Exact resolved versions and complete
licence files appear in the [SVG notices](crates/ravet-surface/third-party/svg.md).
This dependency evidence does not replace release-wide legal review.

The new primitive local fallback fonts are first-party reusable test assets under
the repository source licensing policy; their [complete generator and hashes](crates/ravet-surface/tests/fixtures/fallback/README.md)
are retained. No new runtime dependency is added for caller-owned font fallback.
The separate unchanged Noto Arabic fixture retains its existing OFL notice.

The [authored two-axis variation fixture](crates/ravet-surface/tests/fixtures/variation/README.md)
contains only primitive first-party rectangles and deliberate cmap aliases under
the repository source licence. Its optional fontTools/HarfBuzz generation/oracle
tools add no runtime dependency. System font catalogue metadata and checked bytes
remain consumer-owned in memory; no installed font asset is redistributed or
licence/embedding permission granted by the native loader.
