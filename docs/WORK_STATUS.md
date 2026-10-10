# RAVET work status

Updated: 10 October 2026. Owner: current implementation session.
Starting point: four documentation-only crates; no Git repository.
Local Git has now been initialized and an initial commit pushed to a private
GitHub remote. See [publication policy](publication.md): the initial private
history captured a consumer scaffold and must be sanitized before going public.

## 0.0.1 source preview preparation — 10 October 2026

Aligned all 35 workspace package versions and lock entries, public VST3/AUv2
reference metadata, mobile fixture versions and the Arduino library to 0.0.1.
Foundry native adapters continue substituting each independent consumer's own
version; project starters retain their own initial product version. Added
[release notes](releases/0.0.1.md) and corrected the README dependency summary.
The existing remote feature-ordering commit is preserved in the release history.

Local preparation checks on macOS 27.0.1 arm64 with Rust 1.99.0: documentation
checker, 43 packaging/documentation Python tests, four host-evidence Python tests,
and all 11 native WebKit CTest probes pass. Fresh 0.0.1 locked workspace check,
all-target/all-feature tests (763 passed, four external decoder diagnostics
ignored), all-target/all-feature Clippy with warnings denied, default/all-feature
doctests (18/23 passed) and default/all-feature Rustdoc with warnings denied pass.
Formatting and Git diff checks pass without first-party warnings. Foundry's
external-consumer workflow and effect/instrument metadata propagation at product
version 2.3.4 pass. Rust artifacts use a separate local build directory because
an earlier programme build occupied the shared target directory. Native evidence
is scoped to the synthetic fixtures; no broader platform, DAW/device or deadline
support is inferred.

The publication path checker passes over staged files and reachable local refs.
A targeted credential-pattern scan found no matches in 1,025 reviewed working
files or 1,392 historical blobs. Public image/audio/font fixture provenance and
retained codec/drawing notices were reviewed; all 265 resolved external packages
in all-feature Cargo metadata declare a licence or licence file. These checks are
bounded source/metadata review, not an exhaustive credential or distribution
licence audit. Cargo publishing remains disabled, commercial terms remain a
draft and no source archive or binary release is being uploaded.

GitHub reports the existing remote is private. The hosted old-history limit in
[publication policy](publication.md) remains unresolved: remove hosted copies or
publish only the clean history to a new repository before public visibility.
Verbatim third-party notices retain original whitespace through scoped Git
attributes; normal checks continue for first-party files. Current validation logs
are local `target/release-0.0.1-*.log` receipts; generated build output is excluded
from the commit. The actual ad hoc signed 0.0.1 release CLAP gain bundle passes
the smoke host's 18 processing blocks and six activation cycles. The official
local CLAP validator reports 34 passed, zero failed/warnings and ten skipped
checks (44 total). This verifies the trusted reference bundle, not general DAW
compatibility or production signing/distribution.

## Knob and fader pointer editing defaults (10 October 2026)

All shared knobs and sliders/faders leave paired numeric editing when pointer
adjustment begins, suppress pointer focus outlines and prevent surrounding label
activation from opening a numeric editor on release. Native range dragging stays
browser-owned; pointer release/cancel leaves controls unfocused so host refresh
can resume. Tab/navigation input restores keyboard styling. Direct numeric
selection/entry remains available. See [web controls](web-controls.md) and the
[updated rotary-control decision](decisions/0132-standalone-settings-and-knobs.md).

On local macOS arm64, the complete 11-test native editor suite passes. The expanded
semantic fixture passes 28 programmatic contracts, including paired number focus,
label activation, host refresh, keyboard input and exact attribute retirement.
Real AppKit knob and native-range fader drags also pass in a visible owned WebKit
window, including number entry and computed keyboard outline checks. Evidence is
in `target/web-pointer-tests.log`, `target/web-pointer-native-tests.log` and
`target/web-pointer-controls/Testing/Temporary/LastTest.log`. This is scoped local
UI evidence; other browser/platform and assistive-technology validation remain
independent. Workspace doctests, warnings-denied workspace Rustdoc, formatting
and the documentation checker pass. Cargo evidence is in
`target/web-pointer-doctests.log` and `target/web-pointer-rustdoc.log`; doctest
compilation also reports two existing `jpeg-encode` feature-configuration warnings.

## WebKit context menu defaults (10 October 2026)

Cocoa/WebKit and Linux/WebKitGTK now install document-start context-menu
suppression in all frames through their shared editor creation paths. Browser
menus no longer compete with consumer right-click reset/custom-menu actions;
mouse/pointer events and context-menu propagation remain available. Embedded and
standalone WebKit consumers inherit the default. See the [native editor
guide](native-editor.md) and [decision 0205](decisions/0205-webkit-context-menus.md).

On local macOS arm64, the new hidden synthetic DOM probe passes file/inline
creation, repeat loads, child frames, early cancellation, right-click reset,
custom-handler propagation and unchanged left clicks. All nine existing native
probes also pass. The initial child-frame fixture used a srcdoc navigation
rejected by the existing navigation policy; an explicitly allowed about:blank
frame fixes the fixture and the failed-test rerun passes. Evidence logs are
`target/web-context-menu-tests.log` and `target/web-context-menu-recheck.log`.
Workspace doctests, warnings-denied workspace Rustdoc and the documentation
checker pass; Cargo evidence is in `target/web-context-menu-doctests.log` and
`target/web-context-menu-rustdoc.log`.
Linux runtime, actual OS context-menu presentation and DAW routing validation
remain pending; Windows/WebView2 retains its existing policy.

## Foundry native directory selection (10 October 2026)

Added accessible folder-icon buttons to all seven SDK/framework/project-directory
fields. Initialized native frontends open Cocoa, Windows COM or GTK directory
choosers, seeded at the current value's nearest existing directory. New-project
browsing chooses a parent and appends the package name, retaining exclusive
creation. Cancel leaves the field unchanged; accepted choices edit the form
without saving. Bounded whitelisted requests and sequenced snapshots preserve
subsequent typed edits and permit reselecting the same folder. The macOS bridge
queues chooser work outside WebKit callback borrows to permit modal-loop events.

On local macOS arm64, 11 Foundry tests and warnings-denied all-target Clippy pass.
The hidden native UI fixture clicks the SDK icon, opens/cancels a real Cocoa
chooser, checks unchanged input, publishes an accepted canonical directory through
the native snapshot bridge, then creates/regenerates an instrument while
preserving consumer source. Browser checks verify all seven accessible icons and
selection handling. Complete workspace doctests and warnings-denied Rustdoc
builds pass, as does the documentation checker. Linux aarch64 Rust cross-check
passes; Windows/Linux native picker runtime validation remains pending. See [Foundry](foundry.md) and
[decision 0194](decisions/0194-foundry-project-management.md). Evidence logs are
`target/foundry-directory-tests.log`, `target/foundry-directory-clippy.log`,
`target/foundry-directory-ui.log`, `target/foundry-directory-linux.log`,
`target/foundry-directory-doctests.log` and `target/foundry-directory-docs.log`.

## RAVET Foundry project manager (10 October 2026)

Implemented the shared Rust engine/CLI and RAVET controls/theme GUI, including
an accent-coloured SVG header and desktop sidebar that stays in place while forms
scroll. Foundry creates external independent workspaces, saves/reopens versioned
TOML, rejects changed managed metadata before regeneration, preserves consumer
DSP/editor/native source and Cargo.lock, and runs explicit background builds with
bounded logs. GUI and CLI expose offline, application and effect/instrument
plug-in starters. Effect exports are CLAP/VST3/AUv2/AUv3/AAX/LV2; instruments expose
CLAP/VST3/AUv2/LV2. SDKs stay external. Native identifiers derive from saved product
identity; export selections and product metadata can change without recopying
source. Platform-build instructions cover macOS Rust, Windows x64 MSVC/WebView2
and Linux GTK3/WebKitGTK4.1/X11 launchers. See [Foundry](foundry.md) and
[decision 0194](decisions/0194-foundry-project-management.md).

Local macOS arm64 evidence: 10 project/controller tests, warnings-denied Foundry
Clippy, debug build, complete workspace doctests and warnings-denied workspace
Rustdoc pass using isolated target directories. The external-consumer workflow
creates/reopens/regenerates/builds all four starters, runs the offline gain fixture,
the standalone four-mode hidden UI/audio/settings fixture and packages both CLAP
roles. The native Foundry hidden WebKit fixture verifies logo/controls, instrument
creation, regeneration and source preservation. A copied standalone test knob was
attached to its owner document to match the new connected-control contract.

Generated native effect LV2/VST3/AUv2/AUv3/AAX and instrument LV2/VST3/AUv2 debug
builds pass with local external SDKs. The official VST3 validator passes 47/47
for each role; actual generated AU components pass the existing gain/performance
local audio/state/lifecycle probes. The generated AAX bundle loads and exposes
its ACF functions and SDK version query. These are local development checks,
not DAW compatibility, AUv3 registration or production AAX/PACE evidence.
Linux aarch64 Rust engine/CLI cross-check passes; Windows/Linux GUI and generated
native plug-in runtime validation remain pending. No MPE/component/module/platform
selector, instrument AAX/AUv3 adapter, universal build, global installation or
production release is claimed. Regeneration is atomic per file rather than as a
group; cancellation owns the direct build child, not arbitrary descendants.

Validation logs: `target/foundry-tests.log`, `target/foundry-clippy.log`,
`target/foundry-consumers.log`, `target/foundry-ui.log`,
`target/foundry-workspace-doctests.log`, `target/foundry-workspace-docs.log`,
`target/foundry-linux.log`, `target/foundry-<effect|instrument>-<format>.log`,
`target/foundry-<effect|instrument>-vst3-validator.log`,
`target/foundry-<effect|instrument>-au-contracts.log` and
`target/foundry-effect-aax-check.log`. The complete reproducible SDK-enabled external workflow passes, including
version regeneration, native packages, signature/metadata checks and both
47-test VST3 validator runs (`target/foundry-native-workflow.log`). It accepts
optional SDK paths, AUv3 packaging and an official VST3 validator.

## Explicit sample-wise waveshapers

Added `ravet-signal::Waveshaper<F>` with inline caller-owned f32/f64 functions,
once-only sample calls, in-place processing and atomically checked separate
buffers. Built-in f64 constructors supply hard clip, tanh and cubic soft clip;
cubic endpoints are +/-2/3 with unity small-signal slope. Drive, gain, smoothing,
DC removal and oversampling remain explicit. The wrapper has no sample history,
latency/tail or reset requirement; custom captured state/resources belong to
callers. See [decision 0149](decisions/0149-sample-wise-waveshapers.md).

On macOS arm64, all 98 signal tests/doctests pass, including five new waveshaper groups for
curve landmarks, symmetry/monotonicity/extreme values, native precision/captured
controls, ordered calls, sample/block/partition identity, atomic length rejection,
explicit f32 oversampling composition and 10000 workloads without counted heap
actions. Signal all-target Clippy with warnings denied, workspace doctests and
workspace Rustdoc with warnings denied pass on the current checkout using a
separate Cargo target directory. The complete 574-test workspace suite also passes
in an isolated snapshot that excludes the two concurrently introduced embedded
workspace members and uses an offline-generated lockfile. The shared workspace
was incomplete at validation startup; its shared build target later reported
missing exports despite their presence in source, while the separate target
passed. Waveshaper source formatting and whitespace checks pass. Workspace
formatting reports unrelated in-progress embedded/foundation edits.

Documentation generation/checking passes in the isolated snapshot. On the shared
checkout, `check-docs.py --write` and `check-docs.py` report missing feature source
coverage for the concurrent embedded packages. The catalogue is refreshed from
the current registry/manifests with the checker's existing renderer; full package
coverage validation remains pending those registrations. This is portable offline
evidence without new host/device, platform support, alias-free or callback-deadline
claims.

## Implemented

| Work | Status | Paths and evidence |
|---|---|---|
| W01 — baseline | Integrated | README, decision record, this status, local Git |
| W02 — portable contract | Integrated, provisional | `crates/ravet-foundation`; configuration, borrowed buffers, events, transport, lifecycle; contract fixtures including a no-input test instrument |
| W03 — gain and smoothing | Integrated for M1 | `crates/ravet-signal` linear smoother; reference gain processor in `examples/gain`; exact ramps, retarget/reset, sample-rate conversion, event timing, stereo storage and partition tests |
| W04 — offline and heap instrumentation | Integrated for M1 and extended to CLAP | Offline fixtures, bridge and host ABI heap tests, concurrent worker isolation |
| W05 — parameters, state and meters | Portable APIs integrated; CLAP prototype integrated | Validated schemas/migration, version-two gain/bypass recall with legacy loading, sample-timed bypass/meters, committed-edit gestures and callback heap tests; decisions 0010/0012 |
| W07 — embedded editor | macOS prototype integrated | `crates/ravet-webview`, gain CLAP GUI/inline HTML, `tools/editor-smoke`; Cargo-built WebKit binding, visible gain/bypass/meters, live drag gestures and native lifecycle evidence; decisions 0013/0015 |
| W06 — CLAP feasibility | Prototype integrated | `crates/ravet-clap`, `examples/gain-clap`, `tools/clap-smoke`, `scripts/bundle-clap.py`; built macOS bundle, named development host, official validator |
| Documentation foundation | Integrated locally; CI configured | `docs/README.md`, guides, `docs/features.toml`, generated catalogue, `scripts/check-docs.py`, contributor/agent instructions, PR template, and documentation workflow |
| Small feature examples | Portable DSP integrated; live UI prototype | `examples/tutorials`; eight-voice shared-envelope subtractive synth, live gain/MPE, MIDI 2.0 inspector, knobs/piano keyboard and default standalone I/O popup; explicit offline fixtures retained; [guide](examples.md) |

The completed packages retain `publish = false` and inherit `GPL-3.0-only`.
The portable crates and `ravet-clap` retain `unsafe_code = forbid`. The CLAP
export and smoke host use `deny`, with local allowances for the entry macro,
trusted dynamic loading, and raw ABI test fixtures. Clack dependencies are
confined to the adapter/export/host graph; portable runtime dependencies remain
local workspace crates.

## Validation

Executed on macOS 27.0.1 arm64 with rustc/Cargo 1.99.0:

- `cargo fmt --all -- --check`
- `cargo check --workspace --locked`
- `cargo test --workspace --locked`: 49 behavior/instrumentation tests and one documentation example.
- `cargo clippy --workspace --all-targets --locked -- -D warnings`: passes in
  the isolated eight-package state snapshot. The shared checkout currently
  fails on `clippy::derivable_impls` in the concurrently developed
  `examples/shine/src/lib.rs`; those files were left intact.
- `cargo doc --workspace --no-deps --locked` with rustdoc warnings denied.
- `cargo run -p ravet-gain --locked`: stereo fixture matches known output.
- `cargo tree -p ravet --edges normal`: portable dependencies remain local.
- `python3 scripts/bundle-clap.py`: optimized, ad hoc signed macOS CLAP bundle.
- `cargo run -p ravet-clap-smoke --locked -- "target/clap/release/RAVET Gain.clap"`:
  18 checked blocks across mono/stereo and six activation cycles on a separate
  audio thread; parameter text, inactive/active flush, automation, reset, and
  session save/restore while active.
- CLAP validator 0.4.1: 34 passes, 10 skips, zero failures. Final JSON evidence
  is in `target/clap/release/validation.json`. All five state checks pass.
  Skips include preset discovery, note ports, f64, and optional port activation/configuration.

The heap workload uses 48 kHz stereo, block sizes 0/1/3/64, two gain events per
block, default 5 ms smoothing, and periodic reset. Each in-place/separate mode
runs 10,000 callbacks and checks zero allocation count and zero net allocation
count/bytes (detecting frees of previously allocated storage). Fault/inactive
paths are also measured. CLAP tests additionally measure 10,000 callbacks
through the host ABI (including active flush, reset, and malformed events), and
10,000 in-place bridge callbacks, with no allocation or deallocation. Separate
fixtures verify legal host aliasing, zero frames, output tails, event overflow
before host event access, truncated payloads, invalid layouts, and f64 rejection.
State tests also measure 1,000 restore-consuming callbacks through process,
active flush, and reset with zero allocation/deallocation. A deterministic host
fixture restores midway through a callback and verifies that the new state wins
and is applied on the next block. Portable state fixtures cover version-one
compatibility, malformed records, bounded reads, short I/O, and stream failures.
Deliberate allocate/free probes verify the counter;
a separate worker allocates concurrently without affecting callback counts.

Development-only dependency: `allocation-counter` pinned to 0.8.1, declared
MIT/Apache-2.0 in its distributed manifest, with no further dependencies.
Its [documentation](https://docs.rs/allocation-counter/0.8.1/allocation_counter/)
and downloaded allocator source were inspected for thread-local counting and
net deallocation semantics. It replaces the global allocator in the test
binary that imports it. Native malloc and host/FFI allocations are outside this
counter's evidence. The owner has selected GPLv3 plus commercial licensing; see
[LICENSING.md](../LICENSING.md) for the GPL route and pending commercial terms.

## Concurrent workspace changes

A separate change began adding `examples/shine` during state validation. Its
initial missing build target temporarily prevented workspace Cargo commands, so
an isolated eight-package snapshot first passed tests, Clippy, and Rustdoc.
After the target became available, shared-checkout workspace tests passed too:
49 behavior tests and one Rustdoc example, including four concurrently added
delay/oversampling tests. These checks do not establish Shine product behavior;
its implementation and feature evidence remain owned by that separate change.

The proprietary consumer has now moved to a separate private sibling project,
with its own workspace and product validation. It is no longer a RAVET workspace
member. Reusable delay, feedback diffusion, and fixed 2x conversion remain in
`ravet-signal`; their generic tests are public framework evidence. Product
source, assets, fixtures, wrappers, and licence tests stay outside this checkout.
The original private commit still contains an early scaffold; the publication
path checker is expected to fail until history cleanup before public release.

## Documentation foundation validation

Executed locally on 8 October 2026 with Python 3.14.7 and Rust/Cargo 1.99.0:

- `python3 scripts/check-docs.py`: feature metadata, source/test references,
  coverage of all eight workspace packages, generated output, and local links pass.
- `python3 -m unittest discover -s scripts/tests -v`: 12 checker regression
  tests pass, including stale output, deleted evidence, missing package coverage,
  invalid registry records, and Markdown link handling.
- `cargo test --workspace --doc --locked`: the executable gain API example passes.
- `RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps --locked -p ravet`:
  all eight package references build.
- Documentation workflow YAML parses with both expected jobs. GitHub execution
  and artifact upload remain pending; there is no remote configured.

## Historical remaining work — initial M1 snapshot

M1 is implemented for one f32 main bus and input events. The test instrument
only exercises the contract; it is not a synth product. There is no output
event writer, auxiliary bus or device adapter yet. Reusable parameter attachments, portable
parameter schemas, complete state/migration and gain bypass/metering are now
implemented. CLAP exposes gain/bypass recall and a committed-edit gesture/meter
bridge. The macOS CLAP prototype now embeds gain/bypass controls and input/output
meters through an explicit Rust/WebKit binding. A general Rust widget API remains pending; reusable metadata/control attachments
are implemented separately. Product-specific
gain CLAP session state is a complete gain/bypass prototype that reads legacy
scalar state; the portable parameter schema and explicit migration layer are
implemented separately. The CLAP prototype exposes ports and automation, with its
original adapter limits recorded in decision 0002 and new state behavior in
the [state guide](session-state.md).

Callback duration distributions/deadline misses have not been measured. REAPER
7.82 on macOS arm64 has now exercised the gain CLAP paths recorded below. Other
DAWs, devices and production compatibility remain unverified. Native Linux
evidence and Windows compile evidence retain the limits recorded separately.
Validator and native smoke fixtures remain regression gates.

Next bounded work packages:

1. W07 follow-up: broader widget APIs and editor platforms;
   retain native lifecycle checks and record actual host-loop requirements.
2. W06 follow-up: broaden the single-REAPER validation to additional host versions
   and platforms; retain the validator/ABI and synthetic render checks.
3. W05/W07 follow-up: further editor backends and attachment integrations.
   Portable schemas, complete CLAP gain/bypass recall, committed-edit host gestures,
   coalesced meter reads and macOS gain GUI/native binding are implemented.
4. W08: separate standalone device adapter; use the same gain processor.

Keep format/window backend choices provisional until these experiments provide
host evidence. The documentation workflow is configured; add runtime CI/platform
matrices when supported targets are exercised.


## Licensing decision — 8 October 2026

The owner selected GPLv3 plus separate commercial licensing. Root LICENSE
contains GPLv3 and all current workspace packages inherit GPL-3.0-only.
The owner also accepted launch pricing: Starter free up to $50K company revenue,
Indie $199/developer/year above $50K through $500K, and Pro $699/developer/year
above $500K with no upper limit. Prices are USD excluding applicable taxes.
Company revenue is assessed over the previous twelve months at purchase or
renewal. Paid licences include twelve months of updates and permanent rights to
build and distribute with versions received. All tiers include all supported
features, unlimited products, and no royalties; CI/build machines carry no fees.
Launch billing is annual, with paid licensing following a free preview when
production plug-ins, editors, and documentation are reliable. Community support
covers all tiers; paid customers receive a direct reporting channel, with
engineering support and contractual response times priced separately.
The final commercial agreement, detailed revenue/seat definitions, Starter
retained rights, support operation, and contributor terms remain open.
This decision does not change the implementation status above or publish any
package. See [decision 0006](decisions/0006-distribution-policy.md).


## Commercial agreement draft — 8 October 2026

Prepared [draft 0.1](legal/COMMERCIAL_LICENSE_DRAFT.md) at the owner's request.
It includes the accepted prices and permanent version rights, an Order Form,
and proposed details for seats, Starter registration, group revenue,
third-party scope, support, verification, liability, and breach termination.
The draft separates accepted business policy from proposed legal defaults.
It awaits owner and legal review and is not an executed commercial grant.
No runtime code, Cargo licence metadata, feature status, or publication
setting changed. Documentation links and catalogue freshness were checked with
`python3 scripts/check-docs.py`; this is documentation validation, not legal
validation.

## Reusable private-consumer components — 8 October 2026

Added prepared double-precision delay/feedback diffusion and fixed 2x half-band
conversion to `ravet-signal`; added explicit control-thread XML/hash/legacy integer
verification in `ravet-data`; added a bounded coherent word mailbox to foundation.
A separately built Cocoa/WebKit editor backend now has a native main-loop probe.
These are framework components. Proprietary DSP, assets, identities, SDK wrappers,
state/licence fixtures, and comparison tools remain in a separate private consumer.

Executed locally on macOS arm64 with Rust/Cargo 1.99.0 and Xcode 27.0:

- Workspace tests, two Rustdoc examples, and API documentation with warnings denied
  pass after adding data services and the mailbox.
- Generic data tests cover bounded/malformed XML, Unicode/entity roundtrip,
  SHA-256, and signature byte ordering. Mailbox tests cover a newer restore winning
  over stale audio reflection and 20,000 successful concurrent publications
  without mixed word snapshots.
- The native WebKit probe passes three create/message/snapshot/navigation/
  detach/reattach/destroy cycles and verifies callbacks stop after destruction.
  It does not measure native heap behavior or callback deadlines.
- A follow-up native probe passes with opaque layer backing, configurable
  background colour, and suppression of identical snapshots. Delivery now allows
  one JavaScript evaluation in flight. Host repaint stability remains subject to
  consumer testing; these checks do not establish DAW support.
- The publication checker now examines commit path changes so a rename of an
  identical blob cannot hide a private historical path. Four disposable Git
  fixture tests pass; all 16 Python checker tests pass together.
- The public publication checker deliberately fails on the earlier private
  scaffold in reachable history. The GitHub remote was verified private; no
  public visibility change or history rewrite was performed.

The editor's Rust binding, gain integration, broader platform/deployment support,
DAW embedding, and deadline measurements remain pending. Private consumer host
validation is recorded outside this repository and does not establish public
framework format support.

## Delay processing optimization — 8 October 2026

Removed floating remainder from bounded fractional-delay cursor wrapping and
integer division from cursor advancement. Added `read_integer` for whole-sample
distances, and used it inside prepared feedback diffusion. Generic tests compare
both read paths across repeated wraps of lengths 3, 5, and 17, finite clamping,
nonfinite fractional distances, and oversized integer distances. These changes
retain the prepared storage and allocation-free lifecycle. Product CPU
benchmarks and sound comparisons remain private consumer evidence; this record
does not establish a callback deadline or a framework-wide performance ratio.

Added a prepared `PhaseRotation` arithmetic kernel for caller-owned quadrature
banks. Local tests exercise 16 phases over 8,192 steps with positive, negative,
zero, and near-Nyquist increments and reanchoring every 128 steps. Signal
unit tests include dense fractional-delay interpolation across arbitrary-length
wraps and integer/fractional read equivalence. Consumers must establish
their own numerical error budget and reanchor cadence. Oversampling entry points
now expose inline bodies to consumer compilers; their stream semantics remain
unchanged.

Added `AudioBuffer::map_stereo_frames` to select storage once and map safe active
prefixes with zero-filled missing inputs. Three tests cover in-place read-before-
write swaps, separate instrument/mono/stereo inputs, unequal capacities, zero
frames, and atomic rejection of unsupported shapes. See
[decision 0011](decisions/0011-stereo-frame-mapping.md). This helper retains the
processor's existing preparation and event-validation obligations.

Final local framework verification passes workspace tests (including Rustdoc),
Clippy with warnings denied, warning-free API documentation, formatting, and
documentation consistency. Quadrature rotation and diffusion use explicit FMA
on AArch64/FMA targets and separate operations elsewhere; guides describe the
target-dependent double rounding. Consumer-specific sound and CPU acceptance
results remain recorded privately rather than implying a framework-wide
performance or host support guarantee.


## Portable parameters, state, bypass and meters — 8 October 2026

Completed the first incremental W05 package: foundation now provides borrowed
validated parameter metadata, linear/logarithmic and discrete value mappings,
fixed-size complete records and explicit stable-ID migration. Gain adds a complete
version-two gain/bypass record that migrates legacy version one with bypass off.
Restore validates both values before mutation and snaps smoothing. Portable
bypass switches at sample offsets while smoothing continues. Signal provides
sample peak/RMS windows; gain records input/output per successful nonempty block
and offers a separate one-attempt coherent 20-word telemetry mailbox.
See [the guide](parameters.md) and [decision 0010](decisions/0010-parameter-state-and-meters.md).

Executed locally on macOS arm64 with Rust/Cargo 1.99.0:

- Workspace tests pass, including existing CLAP ABI/state/allocator fixtures.
  Thirteen new parameter, migration, bypass and meter tests cover malformed
  records, extreme mappings, same-frame event order, block partitioning,
  stereo/tails, empty/fault behavior, nonfinite audio and legacy recall.
- 1,000 complete encode/decode/restore, bypass, meter publish/read and reset
  cycles report zero Rust allocation/deallocation; existing 10,000-callback
  gain and host workloads also pass with automatic block metering enabled.
- Workspace check, formatting and Clippy with warnings denied pass. Both
  existing Rustdoc examples pass; workspace API docs build with warnings denied.
- Documentation generation/checks and all 16 Python checker tests pass.
- Rebuilt ad hoc signed optimized CLAP bundle passes the smoke host's 18 blocks
  and six activation cycles. CLAP validator reports 34 passes, 10 skips,
  zero failures and zero warnings.

The CLAP export still advertises gain alone and uses its original scalar state
protocol. New portable bypass metadata, complete restore, host gestures and meter
UI integration remain follow-up work. Bypass has no crossfade; meters coalesce
blocks without retaining intermediate peaks. No device, DAW, native allocation
or real-time deadline evidence is claimed. Existing unrelated workspace work
was preserved; no dependencies, commits or publication settings were added.


## CLAP complete state, bypass and editor control — 8 October 2026

Completed the next W05 increment. CLAP now advertises gain and stepped bypass
(including the standard bypass flag), parameter text conversion and sample-timed
bypass automation. Save emits complete version-two state; load accepts both
versions and migrates legacy records with bypass off. A single atomic word holds
both values and a 31-bit restore generation. Restore still wins over stale
callback reflection. The product main-thread control API queues 32 committed
edits and emits complete host gestures with bounded backpressure handling.
Successful callbacks publish coalesced input/output meters for Rust editor code.

Executed locally on macOS arm64 with Rust/Cargo 1.99.0:

- Workspace tests pass. New tests cover complete bypass recall and malformed
  complete records, legacy bypass-off migration, bounded partial stream I/O,
  edit validation/scheduling, queue fullness/wrapping, partial output resumption,
  stale gesture closure and synchronization with newer restores.
- 10,000 concurrently submitted/consumed edits preserve value order. This
  stress fixture may yield/retry on the test threads; production callback
  operations remain bounded without retry loops.
- 1,000 active committed-edit flush/reset cycles and 1,000 bypass processing/
  meter publication cycles show zero Rust heap allocation/deallocation. Existing
  host ABI probes also pass. A concurrent ABI restore now verifies gain and
  bypass both win over stale callback automation and reach the next block.
- Workspace check, formatting and Clippy with warnings denied pass. Both Rustdoc
  examples pass and all workspace API documentation builds with warnings denied.
- All 16 Python checker tests and regenerated documentation checks pass.
- The rebuilt optimized ad hoc signed macOS CLAP bundle passes 18 smoke blocks
  and six activation cycles, now checking two-parameter metadata and text.
  Validator reports 34 passes, 10 skips, zero failures and zero warnings.
  Text evidence is in `target/clap/release/validation-complete-state.log`.

[Decision 0012](decisions/0012-clap-complete-state-and-edits.md) records state and
edit ordering. [The control guide](editor-control.md) describes overflow and
host output backpressure. This is a Rust product control bridge, not an embedded
GUI or general attachment system. Native Rust/WebKit binding, visible controls,
continuous drag gestures and DAW embedding remain the next work. No dependency,
commit or publication changes were made, and unrelated workspace work remains.


## Cargo native gain editor — 8 October 2026

Completed the first W07 integration: the default macOS gain CLAP build now
provides a fixed 480 × 390 embedded Cocoa editor with gain and bypass controls,
input/output stereo peak/RMS readings, and visible edit errors/host-wait status.
Slider changes commit on release through the existing bounded host gesture queue.
Host automation and state recall use coherent parameter snapshots; audio meters
remain coalesced. The HTML is compiled into the bundle with no external assets.

Added the explicit `ravet-webview` Cargo package for the existing Cocoa/WebKit
backend, main-thread ownership and teardown, bounded snapshot copying, panic
containment and reentrancy guards. Native attachment is an isolated unsafe
boundary. The portable umbrella dependency graph remains local and excludes
WebKit/JSON. The optional default `native-editor` feature can be disabled.
Native compilation requires Xcode tools; macOS 12 is a build deployment target,
not a verified minimum supported OS. Runtime JSON dependencies and their licences
are recorded in [the native editor guide](native-editor.md); ownership and scope
are recorded in [decision 0013](decisions/0013-cargo-native-gain-editor.md).

Executed locally on macOS 27.0.1 arm64, Rust/Cargo 1.99.0 and Xcode 27.0:

- Workspace tests pass, including existing audio/host Rust heap probes and three
  binding tests for wrong-thread creation, bounded/reentrant snapshot handling,
  and copying before callback context destruction. Editor protocol tests reject
  malformed, unknown-shape and nonfinite messages.
- Formatting, workspace Clippy with warnings denied, both Rustdoc examples, and
  workspace API documentation with warnings denied pass. The gain CLAP export
  also checks with `--no-default-features`.
- Regenerated catalogue, documentation checks and all 16 Python checker tests
  pass; the registry covers all eleven workspace packages.
- The rebuilt optimized ad hoc signed CLAP bundle passes 18 audio smoke blocks
  across six activation cycles. Validator reports 34 passes, 10 skips, zero
  failures and zero warnings; evidence is in
  `target/clap/release/validation-native-editor.log`.
- The hidden Cocoa editor host passes three create/attach/show/hide/destroy
  cycles, verifies DOM gain/bypass edits and six emitted gesture events, visible
  invalid-parameter errors, host automation, live audio meter delivery and audio
  processing after GUI destruction. Expected show-before-parent failures are
  checked. The captured `target/editor/gain-editor.png` was visually inspected
  without clipping.
- The older file-backed native lifecycle fixture timed out once during concurrent
  WebKit startup. Two sequential reruns passed; after the final native ownership
  change, rebuilding and running CTest sequentially passed again (3.47 seconds).
  That fixture still has its original four-second startup window.

This is a fixed embedded macOS prototype. Floating/resizable windows, continuous
drag gestures, general parameter attachments, other native platforms and DAW
embedding remain pending. Native heap behavior and real-time deadlines have not
been measured. Meter displays have no peak retention or decay, and bypass has no
crossfade. No commits or publication changes were made; unrelated shared-checkout
work was preserved.


## Windows and Linux native C++ editor prototypes

Added WebView2/HWND and GTK3 X11/WebKitGTK 4.1 backends behind the existing
native C++ API, with finite backing colours, local/inline pages, bounded messages,
coalesced snapshots, explicit teardown and Linux UI-loop pumping. The portable
Rust dependency graph and macOS-only Cargo webview API are unchanged. Consumer
products and format wrappers remain private siblings. See
[decision 0014](decisions/0014-cross-platform-native-web.md).

Validation on 8 October 2026 used local Ubuntu 24.04 containers in Colima on the
macOS arm64 development machine: native Linux arm64 and QEMU Linux x64, GCC 13,
CMake 3.28, GTK3 and WebKitGTK 4.1. Both lifecycle probes passed three cycles,
malformed snapshot rejection, unchanged suppression, changed updates, navigation
restriction, reattachment, callback-triggered destruction and stopped callbacks.
The x64 probe took about 18 seconds under emulation; it is not a performance
measurement. Root-container probes alone disabled the WebKit sandbox for their
isolated process. Production backends retain its default.

The Windows x64 backend and lifecycle executable cross-compiled using MinGW
GCC 13 and Microsoft WebView2 SDK 1.0.3650.58. Windows graphical execution is
pending; no Windows editor, DAW, deadline or native heap support is
claimed. The Windows runtime must be installed separately. Linux consumers need
X11/XWayland and an explicit host UI pump; native Wayland parenting is excluded
from this prototype. The Rust wrapper/gain GUI still expose macOS only.

The same generic x64 lifecycle executable also passed on a native Omarchy
machine under Hyprland/XWayland, with WebKitGTK 4.1 2.52.6 and JSON-GLib 1.10.8.
It ran as the desktop user with the default WebKit sandbox, covering all three
create/message/snapshot/coalescing/navigation/reattach/destroy cycles. WebKit
emitted GBM buffer warnings during this probe; visible rendering and embedding
in a DAW remain separate pending checks. This is native backend evidence, not
an Omarchy host-support or CPU claim.


## Continuous editor gestures — 8 October 2026

Extended W05/W07 with `GainMain` begin/update/end methods and live macOS gain
slider input. Pointer movement now publishes accepted values before release
within one host gesture. Numeric/bypass edits remain complete transactions.
Only one continuous gesture is open at a time; conflicting begins/complete edits
reject. Commands use the existing fixed 32-slot queue and 96-iteration callback
budget. Begin/value commands reserve a slot for end; rejected values are visible
and never overwrite an older queued update. Restore closes an idle old gesture
on the next scheduled callback and discards remaining values from that drag.
Editor destruction queues end and requests host scheduling. See
[decision 0015](decisions/0015-continuous-editor-gestures.md) and the updated
[control guide](editor-control.md).

Executed locally on macOS 27.0.1 arm64, Rust/Cargo 1.99.0 and Xcode 27.0:

- Workspace tests pass, including new cross-callback live gesture, one-event
  backpressure, reserved end under full updates, and idle-restore cancellation
  fixtures. Public main-thread methods check validation, conflicts, canonical
  clamping, scheduling and values applied before end.
- 1,000 live queue gestures and 1,000 active-flush gesture cycles report zero
  Rust heap allocation/deallocation. Existing ABI/process/meter probes pass.
- Formatting, workspace Clippy with warnings denied, both Rustdoc examples,
  warning-free workspace API documentation and a no-default-features export check
  pass. Regenerated catalogue/docs and all 16 Python checker tests pass.
- The rebuilt optimized ad hoc signed bundle passes all three Cocoa editor
  cycles with a value before release, a second value/end in a later flush, and
  exactly one begin/end. Destroying an editor during a new open drag queues end;
  the subsequent active audio callback receives begin/end and processes correctly.
  The fixture's expected show-before-parent error messages remain intentional.
- The built bundle passes 18 audio smoke blocks and six activation cycles. CLAP
  validator reports 34 passes, 10 skips, zero failures and zero warnings; text
  evidence is in `target/clap/release/validation-continuous-gestures.log`.

Delivery depends on host scheduling/output acceptance. Overflow can reject the
final slider value; end remains enqueueable but no bounded delivery time is
guaranteed. DAW embedding, native allocation and callback deadlines remain
unverified. Resizing and reusable framework attachments remain follow-up work.
No dependencies, commits or publication settings changed. Unrelated platform
backend work in the shared checkout was preserved.


## Parameter attachments and editor resizing — 8 October 2026

Implemented reusable `ravet-surface` attachments with borrowed schema metadata,
stable IDs, physical/normalized linear/logarithmic/discrete edits, explicit
commit/begin/update/end transport and host refresh without feedback. Failed
transport operations preserve ownership; submitted values remain separate from
displayed host snapshots. `GainMain` implements the transport trait and the
macOS page callbacks now own gain/bypass attachments. Native teardown explicitly
ends an active attachment. No native/host/JSON dependency entered the portable
surface crate; its only runtime dependency is local foundation metadata.

The gain editor now accepts both-axis resizing from 480×390 through 1200×900
logical Cocoa pixels with no fixed aspect ratio. The view and WebKit child resize
together and the page fills its viewport. Invalid dimensions reject before
mutation; native owned-view bounds are finite positive dimensions through 16384.
This new native resize entry point is currently macOS-only. See
[attachments](parameter-attachments.md) and
[decision 0016](decisions/0016-attachments-and-resizing.md).

Executed locally on macOS 27.0.1 arm64, Rust/Cargo 1.99.0 and Xcode 27.0:

- Workspace tests and Clippy with warnings denied pass. Three Rustdoc examples
  pass, including the new attachment refresh example. Workspace API documentation
  builds with warnings denied; the editor-free gain export checks too.
- New attachment tests cover mapping, strict nonfinite rejection, refresh without
  feedback, accepted submit versus displayed state, conflicts and rejected
  begin/update/end ownership. Existing audio/host heap fixtures pass after GUI
  attachment integration.
- The rebuilt signed bundle passes three native editor cycles at minimum,
  intermediate (640×520) and maximum sizes. Host extension sizes and JavaScript
  viewport dimensions agree with no scroll overflow. Invalid/oversized requests
  preserve the prior size; oversized adjustment caps to the maximum.
- The C++ lifecycle fixture additionally verifies nonfinite/zero/oversized resize
  rejection and matching container/child dimensions after valid resizing. CTest
  passes after these checks (3.15 seconds).
- Native screenshot `target/editor/gain-resizable.png` was inspected without
  clipping. Actual DAW visual behavior requires separate evidence.
- The built bundle passes 18 audio smoke blocks and six activations. Validator
  reports 34 passes, 10 skips, zero failures and zero warnings; evidence is in
  `target/clap/release/validation-attachments-resize.log`.
- All 16 Python documentation/publication checker tests pass, as do regenerated
  catalogue/reference checks. The portable umbrella tree remains local crates.

Prepared a separate official REAPER 7.82 evaluation copy and isolated resource
directory under `target/reaper-validation` at the owner's request. Its CLAP scan
cache discovers `org.ravet.gain`. Native UI automation failed with a closed pipe
and a subsequent timeout; startup dialog handling currently requires the owner.
The public ReaScript session/automation harness and synthetic render checker are
ready in `tools/reaper-validation`; two checker fixtures pass, including rejection
of incorrect audio. These fixtures do not constitute an actual DAW render or
session pass. Actual REAPER execution/results remain pending and are tracked
separately from the completed implementation. See [host validation](host-validation.md).


## REAPER host validation — 8 October 2026

Completed the remaining single-host validation package using official REAPER
7.82/macOS-arm64 on macOS 27.0.1, the optimized ad hoc signed gain CLAP bundle
and the disposable configuration under `target/reaper-validation`. Native UI
control recovered after the owner cleared startup dialogs. The normal REAPER
resource directory and production sessions were not used.

- The ReaScript harness passes CLAP discovery/instantiation, gain and bypass
  metadata/host edits, editor creation/reopening, complete state recall from two
  saved projects and gain automation playback at 0.5 and 2.0. Live track peaks
  match 0.125/0.25 and 0.5/1.0 respectively (about 1.2e-7 maximum peak error).
  Evidence: `target/reaper-validation/config/ravet-validation/host-validation.log`.
- REAPER rendered four-second 48 kHz stereo 24-bit PCM track stems. The render
  checker passes wet gain 1.5, dry bypass and both stepped automation regions.
  Every checked sample matches the expected amplitudes exactly; source fades and
  smoother settling regions are excluded. Evidence: generated `wet.wav`,
  `dry.wav`, `automation.wav` and `render-check.log` in that fixture directory.
- Native UI inspection showed input peak/RMS −12.0/−6.0 dB and wet output
  −8.5/−2.5 dB at gain 1.5. Numeric gain 2.5 and an actual slider drag to about
  3.308 reached REAPER and changed output meters. Plug-in bypass restored dry
  peaks/meters. Write-mode recording produced automation points containing the
  changed GUI gain, verified by the observer and saved in `ui-recording.rpp`.
  Evidence: `ui-validation.log`; this checks recorded values, not every possible
  gesture mode or envelope interpolation.
- REAPER's FX-chain window zoom expanded the embedded page to its capped maximum
  with all controls/meters visible; restoring its window returned the smaller
  layout. Audio/meters and controls continued during resizing. The floating
  wrapper's corner drag did not change its size in this experiment, so that
  interaction is not claimed as validated. Exact minimum/intermediate/maximum
  dimensions remain covered by the independent CLAP native resize fixture.

Harness corrections made during actual execution: match stable CLAP identifiers
`0:1`/`1:2` instead of selecting REAPER's additional bypass wrapper, and seek
playback with `SetEditCurPos(...,true)` rather than moving only the edit cursor.
An initial live-peak attempt failed; a fresh project rerun passed with numerical
peak logging, so this does not establish uninterrupted device behavior. Initial
automatic renders selected the muted Master mix and were silent. Explicitly
applying Selected tracks (stems) in the render dialog produced the passing audio;
the guide records this requirement and the exploratory automatic renderer was
removed from the public harness. The verified wet render was retained from the
first successful stem render after a later exploratory run replaced the working
copy with silence. No plug-in processing change was needed for these harness
issues. Startup scan and evaluation handling remain host-owned.

This completes reusable attachments, bounded macOS resizing and the requested
REAPER validation milestone. It does not establish other DAWs/versions, release
signing, native heap behavior, callback deadlines or general production support.
Broader host/device/platform work remains listed separately above. REAPER's main
test window is left open with playback stopped.

Final checks pass: regenerated catalogue/local references, all 16 documentation
and publication checker tests, both render-checker fixtures, formatting, three
workspace Rustdoc examples and workspace API documentation with warnings denied.


## Cooperative Linux host GTK initialization — 8 October 2026

The native Linux web backend now asks a loaded SWELL provider to initialize
GTK through its LOAD_GTK extension before creating widgets. Both globally
exported host APIs and locally loaded libSwell.so providers are discovered;
no new host library is loaded. An unavailable capability returns a null editor
rather than replacing the host's process-wide GDK event dispatcher.

Native Omarchy/XWayland checks establish the integration boundary: a GDK host
compatibility shim continues receiving events before/after view creation and
destruction, while the unsupported-loader case declines creation and preserves
events. Replacing the cooperative call with the previous direct gtk_init_check
reproduces lost host events in the same test. The existing standalone native
create/message/snapshot/coalescing/navigation/reattach/destroy probe also passes.
The real SWELL libraries from REAPER 7.79 and 7.82 respectively decline and
accept editor creation in a separate native probe. These are toolkit-library
checks, not a successful REAPER session/editor/audio audition. The actual DAW
retry is pending. See decision 0014 and the native editor guide.


## Linux editor viewport and painted-pixel checks — 8 October 2026

The GTK box now expands the web child to fill the requested editor instead of
assigning its one-pixel minimum height. The portable lifecycle fixture now
checks page viewport dimensions as well as message/state behavior. Its new
assertion fails with the previous layout and passes after explicit expansion.

Added RAVET_WEB_LINUX_SOFTWARE_RENDERING (framework CMake default OFF), selecting
WebKit's non-accelerated policy for these views before loading documents. It
does not modify other host views or sandbox settings. On native Omarchy with
NVIDIA RTX 2060 / WebKitGTK 2.52.6 / XWayland, the normal accelerated paint path
produces no expected pixels and emits GBM allocation errors. The software mode
produces the exact two-color 320x180 native GTK paint image without those errors.
The software native lifecycle/viewport probe passes too. Actual DAW painting
and UI CPU cost still need independent evidence. The new painted-pixel test
writes its checked PNG to the native test build directory.

CMake/CTest integration passes all four native web tests in the Ubuntu x64
container with software rendering selected; its root-only sandbox override was
confined to that test process. Native Omarchy probes use the normal sandbox.
Workspace Rustdoc tests, warnings-denied documentation build and documentation
catalogue/link checks pass.

## macOS activating mouse press — 8 October 2026

The Cocoa editor web view now accepts the activating mouse press. A native
two-window AppKit probe sends a drag to an inactive editor while another owned
window is key, checking page begin/change/end messages. The prior backend drops
all three phases; the corrected backend delivers all three. Both this probe
and the existing native lifecycle probe pass on macOS 27.0.1 arm64. This
establishes the owned Cocoa-view behavior, not every DAW's window routing.
The policy is recorded in decision 0014 and the native editor guide.


## Extended processing and output devices (8 October 2026)

The owner expanded the implementation scope beyond earlier gain milestones.
The feature catalogue records scoped implementations and remaining integrations;
earlier gain milestones do not establish complete framework coverage.

W08 extends portable processing to f32/f64, up to 64 channels per bus direction,
eight ordered auxiliary buses, bounded emitted events and latency/tail metadata.
Gain/CLAP explicitly retain their narrower mono/stereo f32 subset. CLAP forwards
validated block-start transport and tracks seeks across accepted callbacks.
Foundation/gain tests and 11 bridge tests pass, including f64 precision, auxiliary
fault silencing, output-event prefix preservation and invalid transport rejection.
These transport changes have not yet been exercised in REAPER.

W09 adds explicit `ravet-device`/`ravet-device-probe` CPAL output-only prototypes.
Four adapter tests pass, including 1,000 eight-channel variable callbacks with
zero counted Rust allocation/deallocation, correct interleaving and sample
position, malformed frame rejection and fault silencing. On macOS 27.0.1 arm64,
the default UMC1820 output opened at 20 channels/96 kHz for a one-second silent
probe: 189 callbacks, 96,768 frames, zero processor faults/backend errors/panics.
This establishes scoped stream/callback execution, not audible output, latency,
real-time deadlines, duplex/MIDI support or any other platform. Cargo builds
are capped at two jobs. Input/duplex, MIDI, recording and device recovery remain
pending.

W10 adds portable validated MIDI 1.0 short messages, running-status decoding and
timed input/output event representation. Four MIDI tests pass for all channel
status families, pitch-bend resolution, zero-velocity note-off, interleaved
real-time messages, common-message cancellation and malformed/SysEx recovery.
SysEx payloads, MIDI devices, MPE and instrument exports remain pending.


W11 (9 October) adds six stable f64 biquad designs, prepared complex radix-two
FFT, direct f64 FIR convolution and selectable 1x/2x/4x/8x f32 oversampling.
Signal's 25 unit tests plus a prepared-kernel heap fixture pass. New tests compare
independent DFT/convolution/impulse recurrences, response landmarks, partitions,
reset and validation bounds. The shared 1,000-workload fixture reports zero
counted allocation/deallocation. Direct FIR has O(taps) cost; there is no long-IR
performance or deadline claim. Wider DSP work remains planned.

Local SDK inspection found Apple headers, VST3/AudioUnit/AAX resources outside
RAVET and installed PACE wraptool 5.10.4 at the owner's Eden location. These are
build prerequisites only, not completed wrappers, signing or host evidence.
No SDKs, signing credentials or proprietary consumer code entered this checkout.

W11 also adds validated sample-counted linear ADSR with current-level retrigger/
release, exact targets and zero-duration transitions. Its heap path is included
in the prepared-kernel fixture; synthesis voice routing remains pending.


Final extended-foundation/device/MIDI/DSP validation on 9 October: workspace
checks pass with 124 tests including three Rustdoc examples, Clippy all targets
with warnings denied, warning-denied API HTML, explicit workspace doctests,
formatting and documentation checks. The umbrella normal dependency tree remains
local portable crates; CPAL is confined to the explicit device graph. Rebuilt
macOS CLAP bundle passes 18 checked audio blocks/six activation cycles in the
smoke host; official validator reports 34 success, 10 skips, no failures. This
includes validator transport fuzz workloads but does not add REAPER transport
or broader DAW evidence. Logs are retained under the ignored target directory.

The final silent output probe was repeated on 9 October after callback containment
changes: UMC1820 20-channel/96 kHz, 189 callbacks/96,768 frames, no reported
processor faults/backend errors/panics. All 16 documentation/publication checker
regression tests also pass. The broader framework backlog remains incomplete.


Concurrent runtime work began after that successful 124-test snapshot, adding
`ravet-runtime` and `ravet-runtime-ffi`. A later shared-checkout documentation/
formatting check was temporarily blocked first by the absent FFI manifest, then
by its missing feature reference and a missing runtime HTTP test module. Those
in-progress files were preserved. Documentation for the completed device/MIDI/
DSP package snapshot passes in an isolated target directory; the latest
runtime additions are outside the 124-test evidence and need their own checks.

## Public documentation scope — 9 October 2026

External framework comparison notes and the comparison plan were preserved in
private sibling notes outside the checkout. Public guides, decision records,
the historical handoff and feature descriptions now state RAVET's capabilities
and limits directly. Original historical wording is preserved in those private
notes. Stable feature IDs, implementation statuses and recorded validation
outcomes are retained. Contributor guidance and filename ignore rules keep
future comparison plans outside the repository.

The regenerated catalogue, evidence/package coverage and local links pass the
documentation checker. All 16 documentation/publication checker regression
tests pass; the content and filename audit finds no removed framework references
among 186 existing tracked or non-ignored files. Git whitespace checks pass.

Existing Git history and local checkpoint refs still contain earlier references;
this documentation cleanup does not rewrite them. Publication still requires
the content and history review described in [publication](publication.md).

## Native reference adapters — 9 October 2026

W12 adds the reusable gain reference's Rust static ABI and external-SDK VST3
processor/controller. Two ABI tests include 10,000 prepared process/reset/restore
iterations with no Rust heap operations. Native contracts cover mono/stereo,
in-place/separate buffers, sample bypass, complete state/controller recall,
zero-frame flush, whole-stream rejection and tails. SDK 3.8.1 validator loads the
actual macOS arm64 bundle and reports 47 passes, zero failures. This is a gain
reference prototype; generic mappings, custom editor and actual DAW evidence
remain pending. No SDK or private consumer implementation was copied.

W13 adds an external-SDK AUv2 component embedding that VST3 artifact. Actual
AudioComponent in-process tests exercise 48 kHz stereo f32, normalized gain,
bypass, gain preset recall, preserved host bypass and backing tails. The SDK's
host bypass is intentionally separate from preset recall. The external Apple
library builds with Xcode 27 at deployment target 12.0; that minimum is not a
runtime compatibility claim. Local bundles are ad hoc signed and kept in target;
no global host installation, production signing, auval or DAW evidence is added.

W14 builds a native mono/stereo AAX reference with the external VST3/Avid SDKs.
The macOS arm64 artifact loads and exposes all eight inspected AAX/VST3 factory
entry points; its SDK query succeeds with word 0x20801345980. Underlying native
DSP/controller contracts pass. This adds no Pro Tools render/state/acceptance
evidence. Authorized PACE signing or a developer host is an external prerequisite;
no credentials or commercial signing actions were used.

W15 adds mono/stereo LV2 descriptors, block-level gain/bypass ports and Turtle
metadata. Actual packaged binary contracts pass for gain smoothing, bypass,
control recall, malformed controls, zero-frame runs, lifecycle, retained tails
and in-place aliases. External official LV2 1.18.10 headers are used only from
target; licence notices accompany artifacts. macOS arm64 only, no host metadata
validation, DAW session or Linux runtime claim.


## Reusable Rust runtime services — 9 October 2026

The owner clarified that replacing consumer-local JUCE code with reusable RAVET
APIs is the objective, with Rust used wherever possible and product authorization
kept private. Added safe `ravet-runtime` files/HTTPS/owned-workers/timers/user-path
APIs and separate `ravet-runtime-ffi` native ownership and narrow OS queries.
C++ facades contain no networking, storage or scheduling implementation. Private
consumer code now uses these APIs; endpoints, payloads, trial/licence rules,
machine hashing and product paths remain outside this repository.

Rust unit/integration/ABI tests pass on macOS 27.0.1 arm64 and Omarchy x64 with
Rust 1.98.1. HTTP tests use synthetic localhost TLS, including untrusted-certificate
rejection, status/body handling, redirect refusal, chunked bounds and deadline.
The standalone native facade links the Rust static library and passes in
C++17/C++23 on macOS and C++17 on Omarchy, exercising UTF-8/binary files, captured
input lifetime, rejected work, self-teardown and timer shutdown. The initial
native probe incorrectly captured a local future by reference beyond its scope;
correcting the fixture to own it resolves that test-only failure.

Added a generic Cocoa/Windows/GTK native file chooser, with no product import
policy. The Cocoa owned-window cancellation probe passes, including the caller's
extension filter. GTK source compiles with warnings denied on Omarchy; actual
GTK chooser and Windows compile/runtime evidence remain pending. An existing
first-mouse probe run failed to acquire its required test-window focus; an
isolated rerun passes. No focus-policy implementation was changed by this package.

Workspace tests, explicit doctests, warnings-denied Rustdoc and Clippy checks
pass. Catalogue/package/link checks and formatting pass. Generic widgets and
reusable format adapters are still separate work; this package is not complete
JUCE parity, a production DAW support claim or a live activation test. Contracts
and limits are in [runtime services](runtime-services.md) and decision 0020.

W16 adds explicit control-thread hierarchical state, atomic transactions, grouped
undo/redo, bounded polling subscriptions/resync and typed complete preset records.
Five integration tests plus the Rustdoc example pass, as does warning-denied
package Clippy. Tests include every truncated record prefix, independent invalid
records, transaction rollback, branching and journal/history limits. No GUI/host
or audio-thread state delivery claim follows from these control-thread tests.

The owner confirms an authorized PACE signing configuration exists, but keeps
its contents private because NDA disclosure scope is uncertain. Production
signing remains the product release workflow; framework implementation continues
independently. No protected signing configuration was requested for upload or
read, and the AAX artifact retains its development-only signature.

W17 adds f64 phase-continuous sine/polyBLEP saw/square/naive triangle, peak
attack/release detection, a soft-knee explicit-sidechain compressor and prepared
arbitrary-ratio sinc conversion. Seven integration fixtures pass for phase,
independent exponential/curve landmarks, arbitrary-ratio partitions/counts,
backpressure, DC/passband/stopband, reset and heap-free paths. Fixed numerical
profiles have documented quality/latency limits; no callback deadline or general
spectral support claim is added.

W18 adds explicit bounded PCM16/24/32/Float32 RIFF streaming, frame seeking,
record finalization and an atomic interleaved SPSC audio ring. Five fixtures pass
for independent chunk data, truncated/malformed inputs, quantization, tails,
odd padding, fault-injected partial I/O, backpressure and 100000 ordered stereo
frames across two threads. Prepared ring exchanges count zero Rust heap actions.
Memory-stream and synthetic handoff evidence does not establish device recording
or a complete playback transport. Other codecs and scheduling remain pending.

W19 adds fixed-capacity portable MIDI voice allocation and statically configured
lower/upper member expression. Four integration fixtures plus an age-rollover
unit test pass for duplicate-note ownership, deterministic steal priority,
sustain, pressure/bend/timbre, master/member expression and channel reuse. Heap
fixtures include 10000 workloads and rollover. This is documented receiver policy,
not dynamic MPE negotiation, hardware validation or a synth/format export.
W18 also gains explicit usize counter-wrap and Float32 nonfinite-bit fixtures.

W20 adds f32 input enumeration/selection/capture and matching-rate FIFO duplex
effects, using the shared prepared audio ring and output panic containment.
Four input/duplex fixtures plus existing output/containment fixtures pass;
1000 capture/render workloads count no Rust heap actions. Package/probe Clippy
passes with warnings denied. Clock drift compensation, sample-exact alignment,
integrated recording, MIDI and hotplug recovery remain separate work.

Physical W20 probe on 9 October, macOS 27 arm64: UMC1820 f32 18-channel input /
20-channel silent output at 96 kHz. One-second capture reports 190 callbacks,
97280 accepted frames, no dropped/invalid frames or backend errors. Output
reports 188 callbacks, 96256 frames, zero DSP/backend/panic errors and zero input
underflow frames. Only counters were retained; no sample recording was made.
This verifies this interface's callback delivery, not audible processing,
round-trip latency, drift over long sessions or hotplug recovery.

W21 adds prepared acyclic f32 routing with stable node IDs, compiled topology,
serial/parallel channel mixing, latency compensation and conservative tails.
Six fixtures pass for gain/events, independent delay impulses/partitions, faults,
invalid topology, aliases and failed reprepare; 1000 workloads count no Rust heap
operations. GraphStatus deliberately exposes later-node partial progress and
requires reset after faults. This is not third-party discovery, GUI hosting or
a Processor contract with arbitrary-node rollback.

W22 adds bounded OSC codecs and explicit worker/control UDP endpoints. Four
fixtures pass for independent network bytes, truncated/malformed packets,
optional scalar/bundle roundtrips, structural limits and localhost oversize/
timeout/recovery. Warning-denied package Clippy passes. Scheduling, pattern
matching, nested timetag semantics and audio event delivery remain separate.

New VST3 REAPER script is prepared for normalized control/audio/state/automation
checks in a fresh target resource directory. Its first startup scanned unrelated
system VST3 paths and stalled in vendor activation/scanning dialogs; no new DAW
validation pass is claimed from that launch. Only isolated test processes were
stopped before retrying with prior test scanner metadata; no user licence data
or normal application settings were copied.


## W23 — uniform partitioned convolution — 9 October 2026

Added a prepared single-channel f64 FFT overlap-add engine with responses up
to 262,144 taps, power-of-two partitions 16..4,096 and exact one-partition
latency. Caller block sizes do not affect transform boundaries. Three targeted
fixtures pass: delayed independent direct sums over multiple response/block
sizes, maximum response endpoints and invalid preparation, and 1,000 prepared
processing/reset workloads with zero counted Rust allocation/deallocation.
No live response swap, multichannel backend, SIMD or real-time deadline is
claimed. Updated the device crate introduction to cover implemented input/FIFO
duplex behavior. See decision 0031 and the signal-processing guide.


## W24 — explicit MIDI connections — 9 October 2026

Added optional `ravet-midi-device` with pinned MIT midir 0.11.0, exact backend-ID
selection, explicit input/output ownership and prepared complete-packet handoff.
Short MIDI 1.0 and seven-bit complete SysEx are bounded to 1,024 bytes; full rings
drop newest input, malformed packets reject, timestamps remain connection-local
microseconds. Four fixtures pass including 10,000 threaded maximum-payload
transfers, usize counter rollover and 10,000 heap-instrumented queue cycles.
The explicit native virtual-endpoint example on macOS CoreMIDI transferred six
note/controller/bend/clock/SysEx packets in order and closed both endpoints. It
sent only to its temporary virtual destination; no physical MIDI device was used.
No hotplug/reconnect, fragmented SysEx, UMP, audio scheduling or other runtime
backend/deadline is claimed.

## VST3 suspended-edit recall and REAPER evidence — 9 October 2026

REAPER 7.82 on macOS arm64 discovered and instantiated the actual public VST3
bundle. Initial startup-driven playback ran before audio dialogs cleared and
returned zero peaks; after normal startup, live wet/bypass output worked but
recalling edits saved while host bypass suspended processing failed. Fixed the
actual gap with complete controller getState/setState and validated SDK
connection-point restore synchronization, including restore-before-connect.
Native regression contracts and the 47-test Steinberg validator pass. REAPER
then passed wet/bypassed synthetic stereo peaks, complete wet/dry session recall,
normalized automation before/after seeking and generic parameter-window creation.
No custom VST3 editor, physical sound output, deadline or other DAW is claimed.
Only disposable test resources were changed; master hardware volume stayed zero.


## W25 — buffered file/device workers — 9 October 2026

Added explicit WAV read-ahead and recording pumps over prepared rings. Read-ahead
retains unqueued decoded suffixes, reports EOF separately from buffered audio
and supports stopped seeks that validate matching endpoints before discarding
an old generation. Recording drains bounded scratch, poisons on dequeued errors
and finalizes after stopped-producer drain. Four worker fixtures pass for tiny
ring backpressure/order, stopped/foreign seeks, bounded final drain and injected
partial-write/nonfinite PCM failure. A fifth prepared-device integration records
and decodes 201 exact synthetic stereo frames with zero capture drops. These
fixtures use memory streams and synthetic input; no physical recording, implicit
worker/transport, file destination or capture permission is selected.

The controller recall change initially regressed AU preset rendering because the
SDK wrapper's controller can lag AU parameter calls until UI notifications run.
The SDK's `IVst3ToAUWrapper` marker now preserves its original component-based
state behavior. Actual AU bundle render/gain-recall/host-bypass tests pass again,
and the rebuilt AAX bundle passes its export/version probe. AAX host/signing
validation remains separate.


The final VST3 REAPER regression also unbypasses a restored dry session without
a new gain edit and verifies resumed half-gain DSP. A zero master fader caused
intermittent decaying meter observations; the harness now keeps master processing
at unity and explicitly zeros every hardware-send gain. The complete harness
passes again with physical destinations silent. Automation is observed inside
its held regions, with bounded repositioning when deferred UI timing crosses
a loop boundary. This is processing/state evidence, not deadline evidence.


## W26 — reusable local web controls — 9 October 2026

Moved gain slider/number and bypass widget behavior into `ravet-surface` local
DOM components, adding reusable numeric choices and action buttons. Explicit
consumer transport owns physical edits/gestures; one slider owns a document
gesture, held navigation keys group updates, focused/active refresh sends no
feedback, and rejected ends retain ownership for retryable disposal. The Rust
inline assembly helper validates one marker and a 1 MiB assembled document cap.
The actual rebuilt gain CLAP/WebKit probe passed three complete create/attach/
resize/edit/gesture/automation/meter/hide/destroy cycles. Additional in-page
checks cover all four components, pointer cancellation, held-key grouping,
rejected begin/end, disposal and noncanonical numeric option spelling. The
resulting native screenshot was inspected. Broader layout/drawing/fonts/assets/
themes, assistive-tech audit and other native runtime backends remain pending.


## W27 — explicit preset-file libraries — 9 October 2026

Added optional `ravet-preset` outside portable state/umbrella dependencies. An
explicit existing directory/product identity and decoding limits supply sorted
metadata discovery, validated atomic replacement through existing runtime file
mechanics and undoable complete selected-file recall. Three filesystem fixtures
pass for sorting/replacement, missing/malformed/foreign/destination-limit failures
that preserve state/history, rejected pre-write replacement, Unix symlink skipping,
portable ASCII stem validation and the 4,096 total-directory-entry limit. No
default path, watcher, categories/Unicode titles, host program/discovery extension
or automatic DSP publication is claimed.


## W28 — runtime contract workflow configuration — 9 October 2026

Configured GitHub workspace formatting/tests/lints/Rustdoc for macOS, Linux and
Windows, with explicit Linux ALSA build dependencies, two Cargo jobs, independent
matrix failures and timeouts. A separate macOS job builds/development-signs the
public CLAP bundle and exercises its ABI smoke host. Workflow YAML, matrix,
permissions and worker-cap settings parsed/checked locally. The rebuilt CLAP ABI
host passes 18 blocks and six lifecycle cycles. Remote matrix execution, other
platform runtime and licensed-SDK/DAW/device/GUI/deadline jobs remain pending.
No changes were pushed and no artifacts were published.

## Current outstanding scope — 9 October 2026

The current catalogue is authoritative for implemented/prototype limits; the
initial M1 remaining-work section above is historical. Further work includes:

- Generic format exports: instruments, f64/aux/event/output/transport mappings,
  custom editors, AUv3 and host preset/program registration. The gain references
  cover narrower, individually validated contracts.
- Device recovery/hotplug, adaptive duplex clocks, full standalone transport/UI,
  physical recording and additional MIDI device/platform validation; UMP and
  full expression conformance/instrument export; single-zone negotiation added in W30.
- Native drawing/layout/fonts/assets/themes/window APIs and accessibility audits
  beyond reusable local DOM controls and existing webview prototypes.
- Additional audio codecs, live transport/seek/loop scheduling and further DSP
  quality/performance profiles beyond prepared current kernels.
- Third-party plug-in discovery/loading/editor ownership and dynamic/f64/event
  graphs beyond the prepared static portable routing contract.
- Executed remote/platform/DAW matrices, application templates/installers, release
  signing/notarization and production deadline/reliability evidence. AAX release
  signing remains a private consumer workflow; Pro Tools validation is pending.
- Graphics/GPU, animation/video, and explicit consumer opt-in analytics/unlocking
  facilities beyond current runtime networking and file/worker services.

These remaining scopes prevent a claim of overall framework feature completeness.


## W29 — independent offline starter — 9 October 2026

Added an explicit project generator with a public offline gain template. New
consumers stay outside RAVET in independent, publish-disabled workspaces; existing
destinations reject without overwriting, and name/licence inputs are explicit.
Two generator fixtures pass for manifest/scope boundaries and existing-file
preservation. A generated synthetic project in a disposable directory outside
RAVET was locked and built offline with two Cargo jobs, then processed/checked
four stereo frames successfully. No persistent proprietary consumer was added,
workspace membership changed, device opened or release artifact published.
Further format/device/UI and installer/release starters remain pending.


## Final local regression evidence for W21–W29 — 9 October 2026

On macOS 27.0.1 arm64 / Rust-Cargo 1.99.0, two-job workspace all-target tests
pass 190 unit/integration fixtures; five Rustdoc examples pass separately.
Warning-denied workspace Clippy and Rustdoc, formatting, generated catalogue/
link checks and diff whitespace checks pass. All 18 Python checker/generator
fixtures pass. Native WebKit shared-control/reference lifecycle checks, CoreMIDI
virtual loopback, the generated external offline starter and rebuilt CLAP ABI
smoke checks pass. The VST3 SDK validator passes 47 tests; actual AU bundle
render/recall and AAX export/version probes pass with the controller fix.
REAPER VST3 wet/dry recall, resumed bypassed edits, automation and generic-window
checks pass. A final host API check confirms stopped transport and zero hardware
send gains; the disposable test main window remains open. No Rust compiler/build
processes remain running. Overall outstanding scope is recorded above.


## W30 — RPN negotiation and memory transport — 9 October 2026

Implemented per-channel fixed-storage RPN decoding and opt-in single-zone voice
negotiation in `ravet-instrument`. Lower/upper configuration, default reset,
shared member/independent master bend ranges and active snapshot refresh are
covered. Invalid/null/NRPN/reset selectors preserve the intended receiver state;
released tails freeze. Three new fixtures include 10,000 prepared negotiations
without Rust heap operations. This does not establish complete MPE conformance,
controller hardware support or instrument-format export.

Added `ravet-audio-file::MemoryPlayback`: bounded copied interleaved clips with
play/pause/stop/seek, half-open loops, EOF silence and rejection without transport
mutation. Three fixtures cover exact frame boundaries, command faults and 10,000
prepared operations without heap activity. The synthetic WAV transport example
connects worker-side decoding to looping and pause. Streaming seek/clock mapping,
resampling, command queues, smoothing and physical playback remain separate work.


The same package also closes the master-expression snapshot gap: pressure and
CC74 from the master are retained separately from member values, refresh active
members, and freeze on release. A further fixture verifies both zone sides,
sustain, controller reset and polyphonic zero master values. The synth consumer
owns combination policy; overlapping zones and instrument exports remain pending.


## W31 — integrated portable instrument — 9 October 2026

`SineInstrument<N>` consumes voice ownership and linear envelopes in a portable
no-input mono/stereo f32 processor. Sample-timed notes, phase-continuous member/
master bend, selected-side RPN negotiation, sustain, deterministic stealing and
release completion are integrated. Complete block validation preserves state on
fault; zero-frame events and inactive/failed preparation behavior are covered.
Three new fixtures compare analytic sine/attack samples, split-block equality,
stereo initialization/tails, release and 10,000 synthesis workloads without Rust
heap activity. The offline executable renders only to memory. Native instrument
exports, complete state/editor support and physical controller/device deadlines
remain unvalidated and unfinished.


## Local regression evidence for W30–W31 — 9 October 2026

On the existing macOS arm64/Rust 1.99.0 environment, with two Cargo workers:
workspace all-target tests pass (200 unit/integration tests), workspace doctests
pass (5), warning-denied Clippy and Rustdoc pass. Formatting, documentation
freshness/link checks and diff whitespace checks pass. The Python script suite
passes 18 tests. Both synthetic transport and synthesis executable examples
pass without opening devices or writing audio files. New API evidence remains
offline; unchanged native format and REAPER evidence is recorded above rather
than expanded into a new instrument or platform claim.


## W32 — native CLAP main bus and MIDI instrument — 9 October 2026

Added `MainBusBridge<P,S>` with prepared native f32/f64 storage, no-input or
0..64-input/1..64-output main buses, selected-port timed short MIDI, global
parameters and block-start transport. Three bridge fixtures cover native f64
precision across 64 channels, MIDI length/port/timestamp validation, instrument
fault preservation and 10,000 callbacks without Rust heap activity. Auxiliary
buses/output events and CLAP note-ID/modulation mappings remain pending.

Built RAVET Sine CLAP (`org.ravet.sine`) with stereo f32 output, MIDI-only note
input, a stable level and atomic 16-byte level state. Linked and actual-binary
fixtures pass 3,009 blocks across three activation cycles, including timed notes,
stereo release silence, active/inactive flush, invalid-load/block preservation,
active state recall and 3,000 callbacks with no heap operations. Stale audio
reflection cannot overwrite a newer restore. The local CLAP validator reports
34 passed/10 skipped/0 failed. Its 768 kHz test exposed an unnecessary DSP rate
cap; finite positive rates now validate actual envelope count capacity and an
explicit overflow fixture rejects before processing. Other DAW/platform and
production-signing evidence remains separate.


## W33 — explicit opt-in analytics — 9 October 2026

Added a default-disabled bounded numeric event queue in the optional runtime
crate. Explicit consent enable/revoke, complete JSON byte/event limits, newest
overflow rejection and whole-batch delivery retry are implemented. No endpoint,
identifier, background worker, persistence or transmission is selected. Three
fixtures verify default-off/empty/revoked no-send behavior, valid ordered JSON,
failed retry preservation and invalid/oversized inputs with a synthetic sender.
Consumer policy and production delivery remain private and unvalidated.

W32 workspace regression checks pass 207 unit/integration tests plus five
doctests, warning-denied Clippy/Rustdoc and rebuilt gain ABI/validator checks
(34 passed/10 skipped/0 failed). REAPER scanned the new sine descriptor, but the
isolated host's startup/UI did not respond sufficiently to execute its new script;
DAW instrument audio/state/automation evidence is still pending. The script uses
synthetic MIDI and silences hardware sends before playback. No new host support
claim is made from descriptor scanning alone.


## W34 — local signed entitlements — 9 October 2026

Added canonical bounded claims and explicit consumer-selected public-key/product
verification in the optional data crate. The domain-separated envelope uses
pinned ed25519-dalek 2.2.0 strict verification; weak/invalid/duplicate trust keys
reject. Successful immutable grants verify half-open time validity and cached
feature checks require current time again. No private-key API, endpoint, stored
grant, hardware/clock query or product gating policy is selected.

Three synthetic fixture groups pass: canonical ordering and exact validity edges,
stale-cache denial, every changed token byte/truncated prefix, trailing/oversized
records, wrong product/key, weak trust and malformed claims. Real consumer keys,
licence/session fixtures and activation UI remain outside this repository.

## W35 — bounded AIFF PCM streams — 9 October 2026

Added seekable big-endian AIFF PCM16/24/32 readers/writers with bounded scratch,
exact integer extended80 rates, validated FORM/chunk/frame lengths, SSND offsets,
explicit seeking/finalization, preserved read retry and poisoned partial writes.
Three fixture groups pass all widths and rate edges, independent reversed chunks,
every truncated prefix, malformed metadata/duplicates, odd padding and injected I/O.
Local macOS Core Audio afinfo recognizes the generated stereo PCM24 48 kHz file;
afconvert decodes all 1024 frames exactly to expected PCM16 samples. No physical
recording, compressed codec or WAV-worker integration is claimed.

W33/W34 full regression validation passes 213 unit/integration tests, five doctests,
warning-denied Clippy/Rustdoc and documentation/Python checks.

## W36 — portable visual mechanisms — 9 October 2026

Added finite geometry, bounded allocation-free row/column layouts, explicit-clock
scalar tweens and bounded escaped local SVG drawing with semantic themes and
explicit font styling. Four fixture groups pass geometry/capped redistribution,
shrink/resize/failure preservation, 10000 heap-free layout/tween calls, animation
retarget/edges/time overflow and hostile text/command/document bounds. Local
collaborative browser inspection renders 640-wide dark/time500 and 360-wide
light/time1000 examples without SVG parser errors; measured text fits the viewBox.
Native toolkit/accessibility, font assets, GPU/video and scheduling remain separate.

## W37 — optional compressed clip decoding — 9 October 2026

Added off-by-default worker FLAC/MP3/Ogg-Vorbis decoding with pinned Symphonia
0.5.5 (separate MPL-2.0 dependency), bounded accepted input/output/packet capacity,
stable signal specification, finite samples, whole-result errors and enabled
gapless trim. Two fixture groups pass 4096-frame stereo rates/counts, exact FLAC
PCM, lossy tone error/channel RMS, capacity and malformed/truncated declared files.
Only generated public tones are embedded; no product/session/licence assets.

Independent FFmpeg Float32 agreement RMS is 0 for FLAC and approximately 9.47e-8
for MP3. Vorbis overlap agrees at approximately 3.36e-8, with an observed duration
difference: FFmpeg 3968 frames versus declared/Symphonia 4096. Broader conformance,
compressed writing/stream seeking and dependency-internal total memory/deadlines
remain unclaimed. Runtime matrix now configures all-feature checks; remote runs
remain pending. W35/W36 full default-feature regression checks pass 220 tests plus
five doctests and warning-denied Clippy/Rustdoc.

## W32 follow-up — REAPER instrument validation — 9 October 2026

The isolated REAPER 7.82/macOS 27.0.1 arm64 test now passes RAVET Sine discovery,
timed synthetic MIDI stereo output, generic parameter-window presence, saved
level recall and two automation values. Measured stereo peaks are approximately
0.04 for level0.8, 0.02 for level0.4 and 0.04 after the second automation point.
Playback finishes stopped and every hardware destination remains muted. The test
retains the already-running 96 kHz device rate rather than overriding hardware.

Earlier pending/failed observations remain historical: first playback stayed at
position zero; a preferences reopen later blocked in Core Audio on the host main
thread. Restarting only the disposable copy, completing its evaluation notice and
retaining device rate resolved this run. No production project, global audio service, private consumer or production-signing
configuration was modified.

## W38 — explicit CLAP hosting boundary — 9 October 2026

Added optional bounded non-loading candidate discovery, byte-preserving native
catalogues and retained Clack entry access. Fixtures cover sorted unique explicit
paths, symlink/nested exclusion, non-loading invalid native bytes, directory/entry/
candidate limits and three live activation cycles after catalogue destruction.
Descriptor copies reject empty/oversized fields and preserve non-UTF8 bytes.

Built the unmodified upstream Clack gain example from official v0.2 commit
27ca28391b57a61bad4bb5099288a7bf9247350d under ignored target storage. Local macOS
arm64 explicit discovery/load/catalogue, four-byte state roundtrip and three
stereo f32 blocks pass with peak0.25 for input0.25. The same inspect command passes
RAVET Gain/Sine 32-/16-byte states and stereo effect/no-input instrument lifecycles.
No vendor source/binary is added to tracked repository files. The example supplies
only empty callbacks and a narrow main-bus host; GUI/richer callbacks, other formats,
crash isolation and broad third-party compatibility remain separate.

## W39 — bounded CLAP generated events — 9 October 2026

Added opt-in prepared generated output storage and explicit process/flush host
queues to the native main-bus bridge. Global parameters and selected-port MIDI/
normalized notes preserve event timing/order with explicit 7-bit conversion.
Representability preflight rejects whole unsupported streams; host refusal reports
accepted prefix/discarded suffix while preserving already-successful audio/DSP.
Old zero-output methods reject nonzero capacity before processing. No retry/replay.

Four fixture groups pass exact output fields/ports/padding/quantization, zero-time
flush, unrepresentable stream atomicity, host prefix refusal, input/DSP discard and
10000 prepared callbacks without heap operations. Exported MIDI-output-port and
other-format host evidence remains pending.

## W40 — guarded native plugin editors — 9 October 2026

Added a negotiated main-thread GUI guard borrowing the live plugin instance,
with explicit unsafe parent/transient lifetime, bounded embedded size/resize,
parent-before-show, idempotent accepted show/hide and paired hide/destroy on Drop.
Native callbacks/state can use short-lived handles while audio ownership remains
independent. Missing extension/unsupported API tests reject before native creation.

Local macOS Cocoa fixtures pass three child-view create/attach/show/resize-or-reject/
hide/drop cycles for RAVET Gain and unmodified upstream Clack EGUI gain at v0.2
commit27ca28391b57a61bad4bb5099288a7bf9247350d, including early scope exit and no
remaining parent children. The upstream GUI also passes catalogue/four-byte state/
three-block stereo audio inspection. Its vendor/GPU dependencies stay under ignored
target storage. No floating/other-platform, visual hide/focus/accessibility or
all-plugin compatibility claim follows. Runtime CI configures the own-bundle guard
fixture only; remote execution remains pending.

W37–W39 all-feature workspace regression checks pass 229 tests, five doctests and
warning-denied Clippy/Rustdoc. Rebuilt sine actual ABI/validator still passes 3009
blocks and 34 passed/10 skipped/0 failed. No exported MIDI-output port host evidence
is inferred from that existing zero-output instrument.

## W41: Portable UMP framing

Added fixed-storage exact packet framing for every message-type length, retaining
reserved payloads, with incremental truncation/reset and explicit big-endian
serialization. Message semantics and native UMP delivery remain separate work.
The W40 full regression exposed a parallel preset-test temporary-path collision;
a process-local sequence now prevents same-clock-tick names from colliding.
W40 doctests, Clippy and warning-denied Rustdoc passed; its failed test run is not
reported as a successful full regression.

## W42: MIDI 2.0 channel voice codec

Added every defined MT=4 channel voice status with validated sender widths and
full-resolution data. Independent word fixtures and exhaustive group/channel
round trips pass, including relative signed extrema, zero-velocity Note On,
unknown attributes, ignored reserved fields and bank validity. Native UMP delivery
and instrument expression policy remain unimplemented. W41 framing tests and
all preset tests pass after the temporary-path race correction.

## W43: UMP short messages and SysEx

Added MIDI 1.0 MT=1/2 semantic representation, validated SysEx7/SysEx8 chunk
codecs and single-address caller-buffer assembly with explicit SysEx8 abort.
Three framing/short-message and two SysEx test groups pass along with the
MIDI 2 codec tests; foundation Clippy passes. The W41/W42 full all-feature
workspace run passes after the preset-test path fix, with doctests, Clippy and
warning-denied Rustdoc also passing. Whole-stream routing/negotiation and native
UMP devices remain work.

## W44: Sample-timed UMP transport

Added framed UMP portable events and CLAP main-bus input/output routing. The
old MIDI 1-only translator remains available. Processor semantic policy and
reference MIDI 1 dialect declarations stay explicit. W43 full all-feature
workspace checks pass 237 tests and five doctests, with clean Clippy/Rustdoc.
The all-message-type prepared echo fixture is the W44 acceptance check; native
MIDI 2 DAW/device performance remains unvalidated.

W44 acceptance passes: all sixteen UMP message types retain ordered packet words
and selected output port, unused native words zero, with zero framework heap
operations across 10,000 prepared callbacks. The complete CLAP output test target
passes five groups.

## W45: Core CLAP host request handoff

Added coalesced atomic lifecycle requests and one-callback-per-tick main-thread
service. Actual linked synthetic CLAP initialization and reentrant callback tests
pass alongside concurrent publication; restart/process transitions remain
consumer-owned. The offline inspection example services explicit pre/post-render
callbacks and reports terminal pending requests. W44 rebuilt sine ABI passes
3,009 blocks/three activations; full all-feature workspace tests, doctests,
Clippy and warning-denied Rustdoc pass.

W45 full all-feature workspace checks pass 240 tests and five doctests, clean
Clippy/warning-denied Rustdoc, 18 Python checks, formatting and documentation.
The upstream Clack Gain actual bundle also passes updated host catalogue/state/
three-block rendering with request handlers. Rebuilt sine validator passes
34 successes/10 skips/no failures after UMP transport.

## W46: Bounded hosted parameter access

Added owned parameter metadata and explicit refresh/plain reads/inactive edits
under an exclusive live-instance guard. Active sample-timed automation remains
the audio consumer's responsibility. Linked Gain and synthetic malformed metadata
fixtures provide contract acceptance evidence; updated trusted offline inspection
prints hosted values.

W46 linked parameter acceptance passes two test groups. The upstream Clack Gain
actual bundle exposes one parameter (ID 1, plain value 1) through the new guard
and still passes four-byte state and three-block stereo rendering. Host discovery
test directories now also include a process-local sequence, applying the observed
same-clock-tick fixture fix consistently to parallel host tests.

W46 full all-feature workspace checks pass 242 tests and five doctests with
clean Clippy and warning-denied Rustdoc.

## W47: Native f64 graphs

Graph, node processors, route gains and delay/scratch storage now specialize
in one native f32/f64 precision, default f32. Graph-owned prepared storage remains
64 MiB with byte-counted capacity. Six existing f32 groups and two new f64 groups
pass, including sub-f32 precision, compensated delays, partition/reset, channel
aliases, capacity/nonfinite gain rejection and 10,000 heap-free callback workloads.
Graph Clippy passes. Native f64 host/device/deadline evidence remains pending.

W47 full all-feature workspace checks pass 244 tests and five doctests with
clean Clippy and warning-denied Rustdoc.

## W48: Prepared dynamic graph handoff

Added bounded prepared whole-graph handoff, one FIFO replacement per valid block
and control-owned retirement. Full retirement leaves current rendering active;
invalid outer blocks preserve proposals. Three initial groups pass FIFO/overflow/
config/control-loss/destructor-thread checks, 10,000 graph reuse cycles and 100
separate-thread transfers without callback heap operations. Native f64 handoff
is also exercised. rtrb 0.4.0 is pinned (MIT OR Apache-2.0), outside the umbrella.
State transfer/crossfade/tail drain and native deadline evidence remain separate.

W48 full all-feature workspace checks pass 248 tests and five doctests with
clean Clippy and warning-denied Rustdoc.

## W49: Explicit generated graph events

Added filtered host/node event edges and combined audio/event topology. Default
broadcast remains unchanged. Explicit mode prepares bounded input/output/host
writers within the 64 MiB graph audio/event budget. Merge overflow clears host
outputs and reports route-aware EventFault after possible earlier node progress.
Four initial acceptance groups pass filters/ordering/zero frames, overflow/reset,
failed edits/combined cycles and 10,000 callback heap checks; native f64 opaque
UMP routing is also exercised. Native host/device deadlines remain pending.

W49 acceptance passes all five event-routing groups, including native f64 UMP
transport. Full all-feature workspace checks pass 253 tests and five doctests
with clean Clippy, warning-denied Rustdoc, formatting and documentation checks.

## W50: Native CLAP auxiliary bus mapping

Added ordered native f32/f64 main/auxiliary mapping with independently compact
input/output port indexes. Complete host input is copied/preflighted before DSP;
all native output ports silence on rejection/fault. Combined sample/event scratch
is capped at 64 MiB. Three new fixture groups pass instrument-sidechain/output
auxiliary routing, exact f64 precision, malformed timing/precision/layout/fault
rejection, output discard and 10,000 allocation-free callback/flush/reset cycles.
Full all-feature workspace checks pass 256 tests and five doctests with clean
Clippy and warning-denied Rustdoc. Native exporter/DAW evidence is separate.

## W51: Exported fixed-threshold sidechain reference

Added a public stateless stereo main/mono detector CLAP reference with native
f32/f64 selection and fixed 0.1 threshold. Linked ABI acceptance passes 6006
blocks across three 44.1/48/96 kHz activation cycles, precision/port metadata,
nonfinite detector closure, missing-sidechain silence and prepared heap checks.
Actual bundle/validator/DAW evidence is recorded as executed below.

W51 built macOS arm64 sidechain ABI passes 6006 blocks/three cycles with no
audio-thread diagnostic logging. External CLAP validator 0.4.1 reports 21 passed,
23 skipped, zero failed. REAPER 7.82 discovers three input/two output pins and
maps detector only from channel three. Actual apply-FX four-channel 96 kHz PCM24
render passes 115200 first-two-channel frames across four off/on regions, max
error 1.1920929e-7. Precision chosen by REAPER was not measured. Native UI was
unavailable (cgWindowNotFound); the documented Lua CLI runner executes normally.

The first live-meter test observed peak decay after detector closure, so actual
PCM files provide acceptance. Initial apply-FX used REAPER's default Documents
media location; the generated file was moved into the isolated directory and
the script now sets RECORD_PATH and current project filename explicitly before
rendering. REAPER retains four source channels and a default one-second tail;
only declared first-two-channel interior regions are checked. Earlier accessor
checks were replaced by independent file-sample validation. No production
project/device settings or signing configuration were modified.

W51 full all-feature workspace checks pass 257 tests and five doctests, with
clean Clippy, warning-denied Rustdoc, 20 Python checks, formatting, whitespace
and documentation checks.

## W52: Two-zone MPE voice receiver

Added a separate shared 1..128-slot receiver with lower/upper RPN negotiation,
latest-overlap precedence, shared-channel/master compatibility notes, independent
zone ranges and member expression frozen after note-off while master expression
continues on zone tails. Existing single-zone/sine behavior remains unchanged.
Four new groups pass all 256 size pairs in both orders, channel reuse, polyphony,
master notes/ranges, isolation/sustain/tail rules, fallback/reset and 10,000 heap
workloads. The public MPE specification informed these rules; native conformance,
MIDI-CI/program/mode/extra-pedal and new synthesis integration remain separate.

W52 full all-feature workspace checks pass 261 tests and five doctests, clean
Clippy and warning-denied Rustdoc. Legacy instrument/voice acceptance still passes.

## W53: Prepared UMP device handoff

Added unique bounded SPSC UMP endpoints with exact numeric words, explicit
protocol labels and opaque timestamps. Whole-batch framing preflight precedes
publication; full/abandoned queues retain an accepted prefix and discard suffix.
Three groups pass all framed types, invalid whole batches, overflow/abandonment
and 10,000 prepared/cross-thread heap workloads. rtrb 0.4.0 (MIT OR Apache-2.0)
was already pinned and now also serves the device crate outside the umbrella.
Native UMP connection and clock/negotiation evidence remain separate.

## W54: Native CoreMIDI UMP endpoints

Added a separate macOS backend crate with narrow guarded native CoreMIDI block/
port ownership and Rust exact-ID selection/framing/queues. Native input callback
entry is nonblocking/serialized; close gates and waits outside callbacks before
freeing Rust state on the owner. Protocol/host-clock tick units are explicit.
cc 1.6.0 (MIT OR Apache-2.0) was already pinned and builds the macOS shim.

A target-only SDK feasibility probe preserved exact MIDI 2 group/channel/key/
16-bit velocity. The reusable backend's actual virtual probe passes 96 MIDI 1/2
packets across all 16 groups and six owned endpoint cycles, including exact-ID
enumeration, diagnostics, producer teardown and native endpoint removal. No
hardware messages were sent. Rust callback entry tests pass 10,000 handoffs and
30,000 invalid metadata entries with zero heap operations. Physical devices,
protocol negotiation, hotplug, other OS and callback deadlines remain separate.

W54 native shutdown stress also passes ten owned destinations closed during
worker submissions, followed by producer teardown, bounded retained packet
checks and exact-ID endpoint removal. Native handles remain on their respective
owner threads; only IDs and separate consumers cross threads.

W53/W54 full all-feature workspace checks pass 265 tests and five doctests,
clean Clippy and warning-denied Rustdoc. These are local macOS checks; native
CoreMIDI evidence above remains separately scoped.

## W55: Deferred CLAP host parameter callbacks

Added opt-in host extension handlers with prepared owner-thread exact-ID clear/
rescan handoff and atomic shared flush/core requests. Clear overflow retains the
64-ID prefix and faults rather than clearing unrelated references. Four groups
pass actual native initialization/main-thread extension calls, exact flags/order,
overflow, concurrent flush/core preservation and 10,000 zero-heap workloads.
The offline inspection example now advertises these callbacks, rejects overflow
and reports bounded snapshots; its process calls serve initialization flush.
Automatic reference storage, refresh/recovery and active scheduling remain work.

W55 full all-feature workspace checks pass 269 tests and five doctests, clean
Clippy/Rustdoc, Python checks, formatting, whitespace and documentation checks.
The actual built gain bundle also passes offline inspection through ParameterHost.

## W56: Hosted audio-port and timing metadata

Added owned inactive directional snapshots bounded to nine ports/64 channels,
with IDs/flags/type bytes and validated equal-channel opposite in-place pairs.
Failed inspection/refresh preserves the old snapshot; active refresh rejects.
Two groups pass linked sidechain/instrument layouts, nine malformed/capacity
modes, directional ID reuse, byte preservation, active rejection, exact 65537-
sample latency, finite/infinite tail and 10,000 zero-heap tail queries.

The actual built sidechain bundle's metadata command reports two inputs (stereo
main/mono detector), one stereo output, common native precision/in-place main
IDs and absent latency declaration. No layout negotiation/hosted graph or
native change-notification support follows from these metadata checks.

W56 full all-feature workspace checks pass 271 tests and five doctests, clean
Clippy and warning-denied Rustdoc. Native metadata evidence remains separately
scoped to the explicitly loaded public reference bundle.

## W57: Deferred hosted audio configuration notifications

Added combined parameter/port/latency/tail-aware host handlers and one prepared
atomic audio-change mailbox. Main/audio callbacks only publish; consumers own
deactivation/timing queries, prepared rebuilding and retirement. Two new groups
pass actual linked main-thread rescan, activation latency and audio-thread tail
calls, coalescing/deferral, unknown-bit masking and 10,000 zero-heap workloads.
The offline inspection example integrates these handlers and reports timing/
changes, without automatic native graph/device scheduling.

W57 full all-feature workspace checks pass 273 tests and five doctests, clean
Clippy/Rustdoc, 20 Python checks, formatting, whitespace and documentation.
Actual built sine inspection passes combined handlers, zero-input stereo ports,
state roundtrip and three blocks, reporting absent latency/infinite tail on the
correct owners. No automatic native restart/graph scheduling is claimed.

## W58: Portable native f64 gain and meters

Added separate f64 gain audio with existing shared f32 gain/bypass control/state/
smoothing policy. Native scaled-squares meters retain finite extrema/subnormal
levels and explicit nonfinite diagnostics; optional f32 telemetry saturates.
Three new groups pass exact sub-f32 audio/bypass/events, extreme RMS, subnormal/
nonfinite readings, state/reset/smoothing/zero-frame and rejected-state retention,
plus 10,000 zero-heap prepared f64 workloads. Native format precision mappings
and UI meter transport remain separately scoped.

W58 full all-feature workspace checks pass 276 tests and five doctests with clean
Clippy and warning-denied Rustdoc. Portable native f64 audio does not imply host
sample-size support.

## W59: Separate native f64 gain ABI

Added RavetGain64 native lifecycle/process symbols over the f64 reference, with
f32 controls/shared state codec. Both ABI precisions expand one bounds/alias/fault
implementation; old f32 fixtures still pass. Two additional groups pass exact
f64 samples, cross-channel aliasing, backing tails, bypass/state recall, invalid
preparation and 10,000 zero-heap native workloads. Actual linked public-header
C++ acceptance passes 6006 blocks/six mono/stereo 44.1/48/96 kHz cycles, without
SDKs or hardware. Native format sample-size mappings require separate evidence.

## W60: VST3 native f64 gain mapping

Added separate setup-selected f32/f64 native handles with shared complete state,
exact native sample pointers, precision mismatch rejection and actual per-channel
output silence flags. C++ contracts pass 6006 f64 normal/zero-frame blocks across
six mono/stereo 44.1/48/96 kHz cycles, sub-f32 values/aliases, sample bypass,
malformed preservation, backing tails, precision switching, wrong-size rejection
and both-precision silence flags. The actual dynamic SDK 3.8.1 validator passes
47 tests/zero failures including its now-exercised 64-bit suite. Prior informative
wrong-silent-flag messages disappear after actual-output flag reporting.

W59/W60 full all-feature workspace checks pass 278 tests and five doctests, clean
Clippy/Rustdoc, 20 Python checks, formatting, whitespace and documentation.
REAPER native precision for this change remains unobserved. Embedded AU/AAX
wrappers need separately rebuilt regression evidence, and AAX signing/Pro Tools
acceptance remains the private consumer workflow.

W60 embedded-wrapper regressions also pass: rebuilt AUv2 actual-bundle normalized
parameters, stereo audio, bypass/state/host-bypass and tails; rebuilt AAX bundle
required exports and SDK version probe. These preserve existing tested paths,
not AU/AAX native f64 or Pro Tools acceptance. AAX remains ad hoc signed only.

## W61: Lazy compressed worker reader and exact seeks

Added optional lazy FLAC/MP3/Vorbis reads with retained packet suffixes, exact
atomic fresh-decoder replay seeks and explicit deferred-fault poison/recovery.
Three groups compare all actual synthetic encoded formats with whole decode for
partitioned reads/tails and forward/backward/packet/end positioning, invalid
outputs, failed-seek preservation and deferred malformed-stream recovery.
Encoded input remains bounded memory; replay is O(target) outside callbacks.
Indexed/file-backed seeking and automatic playback/ring scheduling remain work.

## W62: Compressed prepared-ring read-ahead

Added explicit bounded compressed worker buffering into the existing prepared
SPSC audio ring, with retained suffixes, verified-EOF reporting, decode poison and
atomic stopped seek/generation clearing. Three groups pass all actual compressed
fixtures for backpressure/generation/end delivery, worker-to-consumer exact order
and measured heap-free handoffs, plus deferred corruption and stopped recovery.
Physical playback, rate/clock scheduling and automatic recovery remain separate.
The first fixture incorrectly used a non-power-of-two ring capacity; corrected
to the documented capacity before rerunning acceptance.

W61/W62 full all-feature workspace checks pass 284 tests and five doctests, clean
Clippy/Rustdoc, 20 Python checks, formatting, whitespace and documentation.
The first W61 Clippy pass requested the standard is_multiple_of form; corrected
before this combined successful check. Worker/consumer fixtures remain offline
synthetic evidence, not physical device/clock or deadline validation.

## W63: Optional integer PCM FLAC writing

Added single-worker complete signed PCM16/24 encoding through pinned flacenc
0.5.1 (Apache-2.0), default/parallel features disabled. Input channels/rates/
precision/sample count and returned byte limits are explicit. Three groups pass
exact roundtrips for all eight channel counts/two precisions, 1/17/4097/4113-frame
boundaries/extrema, rate edges, invalid inputs and source preservation.

Initial one-frame output revealed backend STREAMINFO min/max below 16; normalized
to the planned fixed 4096 size per RFC 9639, excluding the final block from the
minimum without padding audio. Independent FFmpeg CRC-checked decoding then
matches all six PCM16/24 1/4097/4113 stereo files byte for byte. Optional encoding
compiles without the separate decode feature. Native recording and other encoder
formats remain work; backend internals have no total-memory/deadline guarantee.

W63 full all-feature workspace checks pass 287 tests and five doctests, clean
Clippy/Rustdoc, 20 Python checks, formatting, whitespace and documentation.
The encoder remains opt-in and complete-clip; physical recording, native clocks
and non-FLAC compressed writers remain independent work.

## W64: Held keyboard gestures and reusable meter text

Sliders now retain gesture ownership across multiple held navigation keys and
unrelated modifier releases. Added a native-meter component with full-level
peak/RMS dB accessible/visible text, overload and invalid-audio indications,
validation before mutation, bounded custom text and explicit disposal. The gain
page consumes it. Actual rebuilt macOS WebKit editor passes three complete
lifecycle cycles with all five component fixtures and host control/meter checks.
Native screenshot inspection shows readable unclipped text. Browser tab order
and ArrowRight submission produce begin/value/end exactly once. T3 snapshot
calls failed twice; DOM and keyboard tools worked, but the document lost focus
after automation, leaving visible-focus and assistive-tech audits unvalidated.

W64 full all-feature workspace checks pass 287 tests and five doctests, clean
Clippy/Rustdoc, 20 Python checks, formatting, whitespace and documentation.

## W65: Owned CoreMIDI virtual sources

Added explicit protocol-selected virtual sources with lifetime unique IDs and
exact-word distribution to subscribers. Native macOS 27.0.1 arm64 source probe
passes 198 deliveries across six MIDI 1/2 lifecycles and all 16 groups, two
subscribers, independent input close, source removal, stale-ID/direction mismatch
and invalid-name rejection. Source close preserves subscriber producer ownership;
input close ends it. No hardware connections, negotiation or persistent-ID policy.

The W65 destination regression also passes 96 exact packets/six lifecycles and
ten concurrent owner-close/worker-send cycles. All-feature workspace checks pass
287 tests and five doctests before the subsequent registration changes.

## W66: Borrowed CLAP parameter metadata registration

Added preparation-time portable-schema/ordered-options validation and complete
count/info metadata. Physical ranges remain exact; incompatible strings, IDs,
steps and bypass reject before exposure. Gain consumes the registration and
marks active output-affecting changes as requiring process. Two adapter groups
cover boundaries/metadata and 10,000 heap-free reads; a linked host-extension
check verifies actual gain metadata. Product state/formatting/flush/scheduling
remain consumer policy, and other-format registration remains separate work.

W66 rebuilt gain smoke/validator passes (34 success, 10 skips, no failures).
All-feature workspace passes 290 tests and five doctests with clean Clippy.

## W67: Incremental lossless FLAC output

Added explicit worker-owned empty seekable sinks, retained 4096-frame blocks,
input/output budgets, error poison and consumed finalization with MD5/count/frame
metadata. Three groups pass all eight channels/both precisions/partitions/tails,
input preservation and partial write/seek/flush failures. Initial minimum frame
size zero incorrectly survived finalization; first-frame initialization fixes it.
A five-channel PCM24 fixture exposed a ~192 MiB prediction size; preflight
rejected before serialization. A constant/verbatim fallback now handles oversized
prediction choices, shared by complete-clip encoding, with exact samples.
Backend internal allocations/deadlines remain outside the wrapper guarantee.

W67 independent FFmpeg CRC-checked decoding agrees byte for byte with all seven
actual incremental records, including five-channel prediction fallback. Full
all-feature workspace passes 293 tests/five doctests, clean Clippy/Rustdoc,
20 Python checks, formatting, whitespace and documentation.

## W68: Prepared-ring FLAC capture workers

Added bounded worker float/PCM scratch, explicit reject/clip overload policy,
nearest/ties-away quantization with signed endpoint mapping, post-dequeue fault
poison and stopped tail drain/finalization. Four groups pass boundaries/overload/
nonfinite/capacity/partial-output errors and exact 8193-frame threaded PCM24 order
with heap-free producer handoff. Completion gates producer teardown before file
finalization. The public synthetic example builds with encoder alone and writes
actual PCM16/24 stereo capture files; physical recording/clock recovery remains
separate. No automatic capture device, thread, path or permission policy.

W68 independent FFmpeg CRC-checked decoding matches both actual 8193-frame
PCM16/24 stereo capture files exactly. All-feature workspace passes 297 tests
and five doctests, clean Clippy/Rustdoc, 20 Python checks, formatting, whitespace
and documentation.

## W69: Rust-owned native editor C ABI

Added a macOS C ABI over retained Rust WebView ownership and counted callbacks,
explicit main-thread status checks and optional exactly-once foreign context
release. Gain-native offers default-disabled linkage. Actual compiled public-
header C++ client passes three native lifecycle cycles, wrong-thread rejection/
retained-destroy retry and message/snapshot self-destruction with context release
after callback return. The first fixture queried before DOM readiness; bounded
readiness polling fixes it. Two Rust groups cover snapshot validation/failure
ownership. Native gain64 regression contracts also pass. Format editor adapters
and other-platform bindings remain independent work.

W69 full all-feature workspace passes 299 tests/five doctests, clean Clippy/
Rustdoc, 20 Python checks, formatting, whitespace and documentation. Actual
native public-header C++ editor checks and gain64 contracts pass separately.

## W70: Optional actual-bundle VST3 gain editor

Added explicit macOS native-editor bundling, Rust bounded gain UI codecs and an
SDK IPlugView with normalized host gestures/controller refresh, rejected-value
rollback and retained/retried end ownership. Actual exported bundle passes three
Cocoa lifecycle cycles, same-view reattachment, gain/bypass/held keys/automation
refresh, rejected values/end removal retention and resizing. Native screenshot
inspection confirms readable controls/status. SDK f32/f64/state contracts and all
47 validator tests pass. Two Rust codec groups verify schema/JSON/output bounds.
The synthetic probe needed the complete IPlugView header; JavaScript fixture
locals are scoped to avoid redeclaration across repeated edits. REAPER/focus/
assistive-tech, meter delivery and other-format editor mapping remain separate.

W70 full all-feature workspace passes 301 tests/five doctests, clean Clippy/
Rustdoc, 20 Python checks, formatting, whitespace and documentation.

## W71: Optional actual-component AUv2 Cocoa editor

Added explicit editor-enabled nested VST3 bundling and actual component Cocoa UI
protocol validation. Three cycles pass gain/bypass edits verified in f32 rendered
audio, external host refresh, preset recall/refresh and subsequent edits, resize
and view-before-unit teardown. VST3 validator/editor and existing AU audio/state/
bypass/tail contracts also pass. Initial raw setter did not notify UI; AUParameterSet
fixes that host path. Bulk preset recall needs all-parameter listener notification
to refresh caches for later same-value changes. setFrameSize did not forward;
setFrame exercises the SDK mapping. Hidden snapshots were flat; showing the
owned disposable host window produces readable controls/status, then closes it.
No external SDK source was copied or changed. Meter delivery, actual DAW/auval/
focus/assistive-tech/sandbox/other architecture remain separately unvalidated.

W71 shared native CLAP editor regression passes three complete lifecycle cycles.
All-feature workspace passes 301 tests/five doctests, clean Clippy/Rustdoc,
20 Python checks, formatting, whitespace and documentation.

## W72: Captured sostenuto voice ownership

Added per-voice held-edge capture, independent sustain/sostenuto release and
changed-latch reporting. Four groups pass threshold/repeat/duplicate/later-note
behavior, both release orders, channel/zone/master isolation and slot lifecycle,
with 128-slot heap-free cycles. Sample-timed sine integration retains captured
notes then releases with identical partitioned output. Native export regression
and full workspace checks follow; controller/DAW conformance stays separate.

W72 rebuilt sine actual-bundle passes 3018 blocks/three activations, including
nine heap-instrumented pedal/audio blocks. Validator: 34 pass/10 skip/0 fail.
All-feature workspace passes 306 tests/five doctests, clean Clippy/Rustdoc,
20 Python checks, formatting, whitespace and documentation.

## W73: Complete parameter program banks and staged MIDI requests

Added borrowed complete catalogues with stable IDs/address uniqueness, canonical
state/capacity validation and copied selection proposals. Separate fixed MIDI
selectors stage either bank half and emit repeated/unavailable Program Change
requests, preserving consumer state until explicitly accepted. Five groups cover
metadata/states/capacities/channel isolation/reset and heap-free selection. The
UTF8-size fixture initially moved its still-borrowed string; keeping it alive
fixes the test. Reference/export/host selection integration remains separate.

W73 all-feature workspace passes 311 tests/five doctests, clean Clippy/Rustdoc,
formatting, whitespace and documentation. No dependency/host mapping change.

## W74: Sample-timed factory program recall in sine/CLAP

Added complete Default/Quiet/Full level programs and chosen-master MIDI bank/
Program Change routing with exact event timing, voice/phase preservation and
unknown-address preservation. Two portable groups pass timing/partition/order/
master/reset/fault behavior. CLAP changed active publications coalesce deferred
main-thread optional value rescans; stale reflection preserves newer restore.
A linked parameter-aware host passes three cycles, deferred/coalesced refresh,
complete state and heap-free callbacks. Actual-bundle/validator checks follow;
host program menus and DAW recall remain independent work.

W74 rebuilt sine actual-bundle passes 3024 blocks/three activations, including
analytic program levels and main-thread saved program state. Validator: 34 pass/
10 skip/0 fail. All-feature workspace passes 314 tests/five doctests, clean
Clippy/Rustdoc, 20 Python checks, formatting, whitespace and documentation.

## W75: Master-routed program requests in MPE receivers

Added independent master bank/program staging to both negotiated voice receivers,
with explicit requests separate from voice/configuration changes, ordinary
per-channel fallback and ignored member/unassigned bank messages. Zone/reset
clears all selectors; range updates/CC121 preserve. Sine now consumes negotiated
requests, removing duplicate decoder ownership. Two groups pass both sides/
zones/reset/preservation and 10000 heap-free cycles per side; native export
regression/full checks follow. Sound commit and native conformance remain separate.

W75 rebuilt sine actual-bundle passes 3024 blocks/three activations; validator
34 pass/10 skip/0 fail. All-feature workspace passes 316 tests/five doctests,
clean Clippy/Rustdoc, formatting, whitespace and documentation.

## W76: Complete gain programmes and native catalogue ABI

Added Unity/Silence/Bypass complete schema states and immutable native copied
metadata with stable IDs/addresses/zero-padded names. Portable/native Rust groups
pass both precision restoration/state/render, atomic rejection and heap-free
metadata. The codec fixture initially passed scalars; using its complete state
fixes it. Actual C++ public-header client passes metadata/rejection and codec/
DSP renders for both precisions; gain64 regression also passes. Native format
menu/selection mapping remains the next integration work.

W76 all-feature workspace passes 318 tests/five doctests, clean Clippy/Rustdoc,
formatting, whitespace and documentation. Clippy requested Option propagation;
Rustdoc interpreted bare array values as links. Using `?` and inline-code values
fixes both checks. Native metadata/audio client and gain64 regression pass.

## W77: Optional actual-bundle VST3 factory programs

Added explicit default-disabled unit/list/nonautomatable selector metadata over
Rust programmes, complete controller host-value refresh and timed processor
expansion with whole-block expanded-capacity rejection. Actual bundle passes
36 blocks/six f32/f64 activations, names/flags, notification rollback/reentrant
write rejection, host value delivery, complete state/audio, zero-frame and 512/
513-point limits. SDK validator passes all 47 tests; Cocoa editor three-cycle
regression passes. Initial compile lacked UString's header; then validator found
empty names from a copying helper. Borrowed output UString fixes that mapping.
Default-build regression/checks follow; DAW/other-format mapping remains work.

W77 default flag-off and programme-enabled bundles both pass SDK 47/47 and
three-cycle Cocoa editor probes; final programme-enabled bundle also passes
36-block/six-activation programme contracts. Twenty Python checks, whitespace
and documentation pass. No Rust API changed beyond W76's verified catalogue.

## W78: Optional actual-component AUv2 factory programs

Added a local AU subclass over the optional nested VST3 catalogue, correcting
endpoint normalization and queuing complete controls with coherent host bypass.
Block-start ID3 selector supports nearest/ties-up; nonzero offsets reject.
Three actual component lifecycles pass names, repeated/reversed factory selection,
custom metadata preservation, parameter endpoints/ties, invalid indices and
ClassInfoFromDocument recall across 420 render blocks. Initial fixture wrongly
rejected negative custom metadata accepted by AUBase; corrected preservation
assertions pass. External SDK sources remain untouched. Default-disabled and
final enabled builds pass AU audio/state/tail and three-cycle editor probes;
embedded VST3 SDK validator passes 47/47 and enabled program contracts pass.
Twenty Python checks, documentation and whitespace pass. No auval/DAW menu claim.

## W79: Prepared downward-expansion noise gate

Added finite-floor explicit-sidechain f64 gating over the rectified detector,
with prepared ratio/threshold/quadratic knee and reset-preserved configuration.
Three groups pass independent dB/knee/endpoint, detector exponential timing,
invalid settings and 10000 heap-free sample/reset iterations. Initial formatting
caught fractional literals without Rust's leading zero; corrected syntax passes.
All-feature workspace passes 321 tests/five doctests, clean Clippy/Rustdoc,
formatting, whitespace and documentation. No native/deadline gate claim.

## W80: Prepared low/high shelving filters

Added separate shelf-kind/slope design semantics over stable normalized biquads.
Three groups pass endpoint/half-dB midpoint, reciprocal response and monotonic
512-bin grids over both sides/three frequencies/three slopes/five gains, plus
boost/cut impulse cancellation, partition identity, reset and heap-free loops.
All-feature workspace passes 324 tests/five doctests, clean Clippy/Rustdoc,
formatting, documentation and whitespace. Higher-order composition remains work;
no numerical-all-corners, headroom or native deadline claim.

## W81: Fixed cascades and even-order Butterworth designs

Added fixed 1..16 section coefficient/state arrays, explicit arbitrary sections
and low/high pass orders 2..32. Three groups pass all-order independent warped
response/cutoff/endpoints, rendered cutoff RMS, partition identity, reset and
changed-coefficient history against independent processors, invalid bounds and
heap-free loops. Signal tests, workspace Clippy, five doctests, warning-denied
Rustdoc, formatting, documentation and whitespace pass. W80's workspace remains
the preceding full-workspace evidence; W81 does not claim a new full run.
Odd-order/band/family/order-selection and native deadlines remain separate work.

## W82: Prepared sample-peak limiter

Added one-channel instantaneous peak attack/exponential release limiting with
prepared ceiling and final symmetric rounding clamp. Three groups pass extreme
finite amplitudes/both polarities, unity/memoryless reference, independent release
recurrence, retrigger, partition/reset, invalid/nonfinite recovery and 100000
heap-free samples. Signal tests, workspace Clippy, five doctests, warning-denied
Rustdoc, formatting, documentation and whitespace pass. No true/intersample-peak,
linked-channel, distortion or native deadline guarantee.

## W83: Caller-scheduled exact-device output recovery

Added explicit output recovery/fresh-processor ownership without fallback,
settings changes or implicit polling. Controlled transitions cover missing,
unhealthy, failed-open retry, healthy preservation, disable/reopen/drop and
constructor validation without factory/native work. UMC1820/macOS arm64 20-output
96 kHz three generations each pass 49 callbacks/25088 frames with zero faults/
backend errors/panics and exact control-thread prepare/deactivate counts.
Explicit reopen/disable/reenable exercised; physical removal and real backend
faults remain unvalidated. Device tests, workspace Clippy, five doctests,
warning-denied Rustdoc, formatting/documentation/whitespace pass. Panic-only
fixtures initially needed error inference; then Clippy requested removing Copy
config clones. Those corrections pass. Duplex recovery remains separate work.

## W84: Caller-scheduled exact-pair duplex recovery

Shared stop-before-open sessions now own fixed matching-rate input/output pairs
with fresh FIFO/DSP. Constructor bounds validate without factory/native work.
Actual UMC1820/macOS arm64 18-input/20-output 96 kHz missing exact input/output
IDs wait without opening. Three manual restart generations pass output 49
callbacks/25088 frames each; input 50/50/49 callbacks and 25600/25600/25088 frames.
Dropped/invalid/backend/DSP/panic/underflow counters all zero in this fixture;
exact control-thread prepare/deactivate counts pass. Device tests, workspace
Clippy, five doctests, warning-denied Rustdoc, formatting, docs and whitespace
pass. Physical disconnect/real faults/drift/other-platform/deadline unvalidated.

## W85: Optional local-font horizontal run shaping

Added owned local font/face and explicit directed single-run shaping through
optional pinned HarfRust 0.14.0, with byte clusters/positions/advances/missing
coverage and bounded input/output profiles. Public unmodified Noto Arabic fixture
retains exact upstream/hash/OFL; no proprietary asset is included. Three portable
groups and eight independent installed HarfBuzz 14.4.0 runs pass, including Arabic
joining/marks/joiners, Latin and explicit kerning disable. All-feature workspace
passes 336 tests/five doctests; clean Clippy/default+all-feature Rustdoc, 20 Python
checks, formatting/docs/whitespace pass. Clippy requested collapsing language
validation; corrected check passes. Default umbrella tree omits text dependencies.
Backend transient memory/time, paragraphs/fallback, outlines/raster/native/GPU/
accessibility and broad font/script/platform support remain separate work.

## W86: Typed SVG paths and local TrueType glyph outlines

Added typed finite path topology/curves and optional default glyf extraction;
shaped SVG text renders local glyph paths with baseline/Y conversion and whole-
run bytes/count rollback. Surface default/all-feature tests, workspace Clippy,
five doctests, warning-denied default/all-feature Rustdoc, formatting/docs/
whitespace pass. Initial OutlinePen import needed its public outline submodule.
The XML fixture initially expected 24 paths; 26 glyphs less one empty space
correctly yields 25. Actual 640x240 SVG is 18529 bytes/no text/font URLs; browser
getBBox checks all ink within viewport. Preview screenshot/text snapshots fail
on its client; independent librsvg PNG inspection shows readable Latin, joined
Arabic/marks. This is vector/fixture evidence, no browser screenshot/native GPU/
raster API/accessibility claim. CFF/color/bitmap/variable-instance/paragraph and
other renderer/profile work remain independent unfinished work.

## W87: Optional single-line Unicode bidirectional ordering

Added pinned Unicode 16.0 ordering, byte levels and logical ranges in visual
order, plus shared single-line separator validation for shaping. Three groups
pass 32 unchanged official reference vectors, isolates/marks/UTF8 coverage,
auto/forced base, empty lines and exact capacity. Surface default/all-feature
tests, workspace Clippy, five doctests, warning-denied default/all-feature
Rustdoc, formatting/docs/whitespace pass. Controlled Arabic/Latin/numbers SVG
has 26 paths/23289 bytes and no text/font URLs; browser geometry lies within
640x180 and independent librsvg PNG inspection passes. No full Unicode suite,
script/fallback/paragraph/caret/accessibility or native renderer claim.

## W88: Prepared simultaneous-output linear SVF

Added bounded one-channel f64 trapezoidal low/band/high output processing and
history-preserving prepared coefficient replacement. Three groups pass static
biquad impulse comparisons/weighted reconstruction, changing coefficients versus
independent nodal solves, exact bounds/nonfinite reset and heap-free loops.
Signal tests, workspace Clippy, five doctests, warning-denied default/all-feature
Rustdoc, formatting/docs/whitespace pass. No nonlinear/self-oscillating profile,
arbitrary modulation stability or native deadline guarantee.

## W89: Prepared damped synthetic mono reverb

Added four parallel lowpass-feedback combs/two canonical allpasses with bounded
prepared storage and live wet mixing. Three groups pass independent time-indexed
impulse/noise references, first arrival/finite fixture decay, partition/reset,
dry-retained histories, heap-free loops, maximum lengths and nonfinite recovery.
Signal tests, workspace Clippy, five doctests, warning-denied default/all-feature
Rustdoc, formatting/docs/whitespace pass. Clippy requested compound assignment
in the independent reference; corrected pass. No perceptual quality/physical-room/
exact overall T60/stereo/other-platform/native deadline claim.

## W90: Real half-spectrum FFT and prepared cosine windows

Added N/2+1-bin real transforms backed by existing full complex FFT scratch,
plus five explicit symmetric/periodic window families and sampled gain/energy/
noise bandwidth. Three groups pass direct DFT/roundtrip/Parseval/endpoints,
window landmarks/analytic gains, coherent-bin correction, atomic rejections and
heap-free application. Signal tests, workspace Clippy, five doctests, warning-
denied default/all-feature Rustdoc, formatting/docs/whitespace pass. Initial
mirror indexing needed a local index to avoid overlapping Rust borrows. No
real-only optimization/SIMD/overlap-add/native deadline claim.

## W91: Serialized native gain meter records

Added complete fixed C-layout f32/f64 block meter reads under serialized DSP
ownership. Actual static-library C++ validates layouts and 38 blocks across
mono/stereo/rates, f64 precision/large RMS, counts/diagnostics, empty/fault
retention/reset and null rejection. All three native audio/program/meter targets
pass. Native Rust tests, default/all-feature workspace Clippy, five doctests,
warning-denied default/all-feature Rustdoc, formatting/docs/whitespace pass.
Initial generic reading closure needed direct type-inferred array maps. A default
build exposed an unused text-only glyph helper; cfg-gated it and default checks
pass. Editor delivery remains separate work; no concurrent DSP UI read/deadline.

## W92: Native VST3 host-mediated meter delivery

Native editor builds add twelve hidden/read-only/nonautomatable meter parameters,
processing-owned snapshots and Rust UI JSON/four local meter controls. Actual
exported f32 processor-to-host-to-controller-to-WKWebView cycles pass rendered
RMS/diagnostic/accessible text; SDK-side f32/f64 metadata/queues/no-queue hosts
pass. Default DSP-only and final editor/program builds each pass SDK 47 tests;
program36-block/six-activation and editor lifecycle/edit contracts pass. Native
image inspection shows readable four-meter UI. Rust/default+all-feature Clippy,
five doctests, warning-denied default+all-feature Rustdoc, 20 Python checks,
formatting/docs/whitespace pass. Initial C++ fixture newline was corrected;
Clippy's constant-chunk suggestion was resolved with a fixed four-value array.
No REAPER/assistive-tech/complete-block UI atomicity/deadline claim. Hosts must
forward output parameters; absent/rejected queues drop telemetry without DSP fault.

## W93: Actual AUv2 native meter transfer

Shared editor telemetry traverses the unmodified SDK output transfer/main-thread
timer. Actual loaded component passes twelve readable/not-writable metadata and
normalized values, four displayed RMS/gain changes, injected input/output
nonfinite diagnostics/recovery and three editor cycles. Shown native PNG
inspection passes. Component audio/state/tail, factory420 blocks, nested VST3
SDK47/program36 blocks/six activations and editor regressions pass. Metadata
CF names are released. Documentation/whitespace checks pass. Host bypass and
stopped/empty/fault renders retain prior meters; no complete-block UI atomicity,
DAW/auval/assistive-tech/timer deadline or other platform/architecture claim.

## W94: Native REAPER meter harness, host run pending

Added public-marker/empty-project guards before logging/host mutation, silent
hardware routing and twelve meter comparisons. Three evidence-check groups pass,
including installed-Lua missing/incorrect marker/nonempty project protection.
The guard test caught a missing marker path separator; corrected before host execution.
Restarted only the named isolated REAPER copy to load the new binary. CUA still
returns cgWindowNotFound; a thread sample confirms startup modal state before
script logging. Owner readiness is pending while independent work continues.
No actual REAPER meter-forwarding, visual or assistive-tech claim. The prior
session/automation/render evidence remains historical and distinct.

## W95: Bounded live sinc rate adjustment

Added one-percent-maximum prepared variable-rate range/conservative cutoff,
history/pending-position-preserving setters and reset retaining adjustment.
Three groups pass piecewise partition/DC/continuity, independent100000-input
counts at drift/extreme settings, reset/fresh history, bounds/atomic rejection,
pending position and heap-free changes. Fixed resampler regressions, signal tests,
workspace Clippy, five doctests, warning-denied default/all-feature Rustdoc,
formatting/docs/whitespace pass. No automatic FIFO controller, physical clock
compensation, broad spectral or native deadline claim; integration continues.

## W96: Bounded FIFO-depth clock controller

Added fixed-state smoothed proportional/clamped/slew-limited ppm feedback with
explicit occupancy/time/configuration bounds. Three groups pass independent
30000-callback positive/negative drift and jitter models, expected equilibrium,
smoothing/slew/saturation/reset, variable-resampler coupling, atomic rejection
and heap-free loops. Final clamp protects correction bounds against rounding;
fractional/extreme bounds pass. Signal tests/workspace Clippy, five doctests,
warning-denied default/all-feature Rustdoc, formatting/docs/whitespace pass.
Physical clocks, arbitrary configuration stability and native duplex integration
remain separate; integration is continuing.

## W97: Prepared and native clocked duplex conversion

Added opt-in raw FIFO/controller/per-channel sinc conversion into the existing
output-clock duplex processor, preserving matching-rate behavior. Signed +/-500
ppm models with 6000 variable callbacks each preserve stereo/DC and bounded
occupancy without underflow; priming/starvation/reprime, malformed-frame rejection,
callback counts, deactivation, pre-prepare rejection and 1000 heap-free workloads
pass. All ravet-device tests, default/all-feature workspace Clippy, five doctests,
default/all-feature warning-denied Rustdoc, formatting/docs/whitespace pass.

On macOS 27.0.1 arm64, actual explicit UMC1820 18-input 96 kHz to Mac mini Speakers
2-output 48 kHz f32 streams pass three fresh silent pairs and pause/resume with
exact control-thread preparation/deactivation. Three additional ten-second pairs
each render 938 callbacks/480256 frames before resume, with zero dropped capture,
underflow, invalid-buffer/backend/DSP/panic counts. Final queue depths are
2292/2051/2051 and corrections 1408.5447/16.2635/16.3185 ppm; initial priming is
0/0/512 output frames. Earlier short pairs reached the 2000 ppm configured limit.
Only counters persist, with no input recordings or audible output. These bounded
runs validate this conversion/lifecycle path, not physical phase alignment,
long-session clock estimation/stability, arbitrary settings, hotplug/recovery,
other devices/platforms or deadlines. W94 REAPER remains pending its startup modal.

## W98: Caller-scheduled clocked duplex recovery

Added exact-ID clocked recovery sharing existing session release/retry/generation
ownership. Invalid configuration preflight now precedes direct capture opening;
sinc rate/tap/ppm preflight shares preparation bounds without table allocation.
Constructor/disabled-service no-factory checks, shared session ordering tests,
complete device/signal tests, default/all-feature workspace Clippy, five doctests,
default/all-feature warning-denied Rustdoc, formatting/docs/whitespace pass.

Actual UMC1820 18-input 96 kHz to Mac mini Speakers 2-output 48 kHz silent pairs
on macOS 27.0.1 arm64 pass absent exact input/output IDs without factory work,
manual reopen and disable/reenable across three generations. Output callbacks/
frames are 51/26112, 48/24576, 51/26112; capture drops, conversion underflow and
invalid-buffer/backend/DSP/panic counters are zero. Exact three control-thread
prepares/deactivations pass. No input recordings persist. This validates the
manual recovery paths; physical unplug, real backend faults, long-session drift,
other devices/platforms and deadlines remain unvalidated. W94 remains pending.

## W99: Bounded hosted CLAP audio-owner lifecycle handoff

Added main-activated/stopped token proposals, private non-Send/non-Sync running
ownership, audio-owner start/process/reset/stop and bounded stopped retirement.
Initial/candidate start failures preserve ownership/current processing; full
retirement prevents candidate consumption; abandoned control preserves current
and pending ownership. Explicit finish/drain returns all tokens for matching
main-thread deactivation. Graph/device routing remains separate.

Linked actual synthetic callbacks pass thread-ID checks across six instances,
initial/candidate failure, backpressure, full/gone offers, reset, 6000 tagged output
blocks and exact main deactivation/destruction counts. Linked gain additionally
retains audio/pending ownership after control abandonment. Successful swaps and
rendering pass heap instrumentation; compile-fail Rustdoc enforces non-Send running
ownership. Actual RAVET Gain and prepared upstream Clack Gain bundles each pass
2000 stereo f32 blocks/32000 frames across two audio-owner starts/stops and a
bounded swap with zero measured allocations/main deactivation on macOS arm64.

All host tests, default/all-feature workspace Clippy, six doctests, default/all-
feature warning-denied Rustdoc, formatting/docs/whitespace pass. The only lock
change adds already-existing rtrb 0.4.0 (MIT OR Apache-2.0) to ravet-host's direct
dependencies. No general plugin compatibility, port/config/event routing, native
device scheduling, crash isolation, physical deadlines or other-platform claim.
W94 REAPER remains pending its startup modal; independent graph integration continues.

## W100: Explicit graph audio-owner lifecycle sessions

Added optional no-op processor start/stop hooks and non-Send/non-Sync borrowed
static/dynamic graph sessions. Nodes start topologically and stop in reverse;
failed/error/unwind starts roll earlier nodes back. Dynamic failed candidates
retire while the old graph renders; successful old nodes stop before control
retirement. Default portable rendering stays unchanged. Active preparation
rejects before changes; guards must end before control transfer/deactivation.

Three session groups pass owner-thread/reverse-order/error/panic rollback,
invalid-input/backpressure/retained rendering, control-only deactivation/drop,
heap-free swaps/rendering and native f64 default-hook precision. Foundation/graph
regressions and full all-feature workspace acceptance pass (375 tests including
doctests); default/all-feature workspace Clippy, eight doctests, default/all-
feature warning-denied Rustdoc, formatting/docs/whitespace pass. These are lifecycle
mechanics and offline evidence; hosted CLAP node port/native buffer mapping and
device scheduling remain dependent implementation work, without platform/deadline
claims. W94 REAPER remains pending its startup modal.

## W101: Prepared hosted CLAP f32 main audio graph node

Added main-owned instance/activation and a Send prepared native graph node for
explicit owner sessions, matching zero/one main port per direction with bounded
f32 scratch and stopped-token return. Fixed-config/shape rejection retains main
instance ownership before activation; main audio start rejects before native
callbacks. Declared graph timing is explicit. Node-local clocks, deferred stopped
reset, success-status telemetry, backend nonrollback/silencing and one-shot
retirement/inactive instance recovery are documented. Core callback service is
explicit/main-owned, without implicit restart/polling.

Four linked native fixture groups pass actual main/audio thread IDs, cold/warm
prepare, shape/phase/active rejection, malformed/empty blocks, partial native
runtime fault silence, retained status/sleep output initialization, steady time/
reset, callback service, dynamic wet/dry swap, stop before control graph/instance
retirement, exact deactivation/destruction and prepared heap checks. Actual RAVET
Gain and prepared upstream Clack Gain bundles each pass 2000 stereo f32 graph
blocks/32000 frames and main stopped retirement with zero measured allocations
on macOS arm64, under explicit known zero-latency/no-tail gain probe profiles.

Host/graph/foundation regressions, default/all-feature workspace Clippy, nine
doctests, default/all-feature warning-denied Rustdoc, formatting/docs/whitespace
pass. Only existing internal foundation and development graph packages were added
to host dependencies. Native events/transport/f64/auxiliary/editor/active parameter
mapping, other format/device scheduling, broad plugin/platform/deadline evidence
remain dependent work. W94 REAPER remains pending its startup modal.

## W102: Native f64 hosted main audio graph nodes

Added a separate f64 factory/node with typed double scratch/native ABI buffers and
advertised-port checks before activation. Shared private lifecycle retains f32
behavior and one-shot main/control retirement. f64 scratch is bounded to 64 MiB
outside the graph cap. Public memoryless stereo x2 cdylib fixture supports both
formats without parameters/state/UI/events/assets.

Five linked node groups pass; f64 retains sub-f32 increments, 1e200/tiny values
and exact routed outputs through 1000 heap-free owner blocks. Loaded public
fixture passes 2000 blocks/32000 frames per precision, native wet/dry dynamic
swap/stop/main retirement and zero measured allocations on macOS arm64. Existing
own/upstream gain bundles repeat f32 acceptance; actual Gain f64 request rejects
with Precision because its CLAP ports remain f32 only.

Host/graph/foundation regressions, default/all-feature workspace Clippy, nine
doctests, default/all-feature warning-denied Rustdoc, formatting/docs/whitespace
pass. No new external dependency. Native multiport/events/transport/editor/active
parameter/device mapping and broader format/platform/production work continue.
W94 remains pending its REAPER startup modal.

## W103: Explicit packed native CLAP audio ports in graphs

Added separate f32/f64 packed-port factories while preserving original main-only
validation. Native input/output port boundaries remain intact in prepared buffers;
graph routes address directional native enumeration/channel order. Summed layout
checks precede activation; nine ports and 64 total channels per direction bound
storage. Foundation auxiliary buses stay empty; pairing/negotiation is explicit.

Eight linked node groups pass, including separate stereo auxiliary input/output
mapping, distinct per-port processing, wide f64 values, summed-layout/auxiliary
precision rejection before activation, owner retirement and heap instrumentation.
Actual exported Sidechain CLAP passes 2000 blocks/32000 frames per precision,
alternating detector open/closed samples, native wet/dry routing, dynamic swap/
stop/main retirement with zero measured allocations on macOS arm64. Loaded dual-
format fixture and own/upstream f32 gain regressions also pass.

Host/graph/foundation regressions, default/all-feature workspace Clippy, nine
doctests, default/all-feature warning-denied Rustdoc, formatting/docs/whitespace
pass. No dependency changes. Native events/transport/editor/active parameter/
device scheduling and broader format/platform/production work continue. W94
REAPER remains pending its startup modal.

## W104: Opt-in hosted CLAP block-start transport

Added checked allocation-free portable-to-native transport and stopped main-thread
node opt-in for both precisions. Disabled behavior stays unchanged. Any known
context requires known playing; tempo/beat/seconds flags advertise only known
fields. Native fixed-point bounds/quantization and f64 sample conversion are
explicit. Invalid context clears output before native calls/history/steady-time
changes. Discontinuity has no native flag and does not reset DSP.

Nine linked node groups and three transport groups pass known/absent context,
negative positions, positive tempo, invalid/missing/range rejection, native clock/
call preservation, owner checks and heap instrumentation. Loaded public fixture
validates native context and alternates x2/silent output from playing/stopped flags
across 2000 graph blocks/32000 frames in each precision, wet/dry dynamic swaps,
main retirement and zero measured allocations on macOS arm64. Main-only fixture,
own/upstream f32 gain and packed f32/f64 sidechain regressions repeat acceptance.

Host/graph/foundation regressions, default/all-feature workspace Clippy, nine
doctests, default/all-feature warning-denied Rustdoc, formatting/docs/whitespace
pass. Full all-feature workspace acceptance also passes (388 tests including
doctests). No dependency changes. Native input/output events/editor/active parameters/
device scheduling, richer transport and broader format/platform/production work
continue. W94 REAPER remains pending its startup modal.

## W94 follow-up: Actual REAPER native meter forwarding

After the owner reported ready, CUA successfully inspected the isolated empty
REAPER main window, loaded the guarded public native-meters.lua through the
Actions list and ran it. REAPER 7.82/macOS arm64 discovered the editor-enabled
RAVET Gain VST3, muted the test hardware destination, loaded stereo synthetic
input and returned all twelve meter IDs. Observed normalized peaks/RMS were
0.0625/0.125 input and 0.125/0.25 output, invalid counters zero, within 0.0002.
Native editor floating-window handle passed; playback stopped and the editor
closed. Latest main window inspection retains only the disposable unsaved project.

Evidence: target/reaper-native/config/ravet-validation/native-meter-host.log.
Earlier modal/cgWindowNotFound observations remain historical. This closes the
actual DAW parameter-forwarding harness run; no visual/assistive-tech/other-host/
platform/deadline claim. Implementation programme continues independently.

## W105: Prepared hosted graph global parameter input

Added explicit f32/f64 parameter factories selecting main-only/packed native audio,
1..4096 bounded input slots and unique product-to-native identity mappings.
Current writable metadata/ranges/steps inspect on main before activation. Strict
whole-block unknown/other-kind/range/step rejection precedes native calls. Ordered
physical f32 values widen to native f64 with global targets/empty cookies. Audio
zero-frame flush dispatch retains process/steady counters; fixed snapshots require
fresh preparation after metadata changes. Native output remains void here.

Four linked parameter groups pass pre-activation metadata/capacity/readonly/
ambiguous-ID rejection, actual aliases/targets/cookies, same-frame last-event
ordering, wide f64 buffers and 4000 malformed blocks per precision preserving
native gain across subsequent rendering. Each precision renders 6000 native
blocks/48000 frames plus 1000 active zero-frame flushes with zero measured heap
actions. Linked real Gain independently passes 240-sample smoothing, sample-timed
bypass and empty-flush target/state checks. Loaded public stepped gain fixture
passes 2000 graph blocks/32000 frames and two empty flushes per precision, dynamic
wet/dry swaps/owner stop/main retirement with zero measured allocations on macOS
arm64. Existing loaded gain/sidechain/transport regressions retain acceptance.

Host/graph/foundation tests, default/all-feature workspace Clippy, nine doctests,
default/all-feature warning-denied Rustdoc, formatting/docs/whitespace pass. No
new dependency. Native output/MIDI/per-note/gesture/editor/device integration and
broader format/UI/platform/production work continue. W94 host forwarding is now
completed by the follow-up above; prior startup-modal observations are historical.

## W106: Native global parameter output and preparation builder

Added a builder selecting independent fixed port/transport/input/output profiles,
preserving original factories. Main validates output identities/ranges and permits
read-only native parameters. Typed global output narrows to finite/range/step-valid
portable f32 with bounded slots, offset/order checks and no cookie/pointer retention.
Unmapped/unsupported/per-note output filters and counts. Mapped runtime rejection
clears whole audio/event blocks with nonrollback BackendFailure; successful output
gates success counters/status. Output-only zero-frame profiles dispatch audio flush.

Ten linked graph-node and eight parameter groups pass, including before-activation
output capacity/identity checks, exact outer capacity, output-only zero flush,
readonly aliases and builder packed/f64 transport. Twelve native output modes per
precision each run 1000 heap-instrumented blocks: normal, range/NaN/offset/order/
capacity faults, unmapped/MIDI/per-note filters, f64 rounding, f32 nonrepresentable
and stepped rejection. Rejected output still leaves native gain advanced, verified
by main parameter read after orderly retirement. Native callbacks measure no heap
actions. Existing parameter input/real Gain regressions retain acceptance.

Loaded public fixture passes 2000 graph blocks/32000 frames and two empty flushes
per precision, readonly events at exact final-frame/zero offsets through host/node
graph edges, dynamic wet/dry swaps/owner stop/main retirement, with zero measured
allocations on macOS arm64. Previous input/transport/sidechain/own/upstream gain
loaded regressions pass. Host/graph/foundation tests, default/all-feature workspace
Clippy, nine doctests, default/all-feature warning-denied Rustdoc, formatting/docs/
whitespace pass. No dependency change. Native note/MIDI/per-note/modulation/gesture/
editor/device integration and broader formats/UI/platform/production remain work.

## W107: Selected native note/MIDI graph input

Added explicit inactive native note-input index/dialect selection to the f32/f64
preparation builder. Shared bounded input storage preserves mixed global
parameters and native CLAP notes, MIDI/MPE or framed UMP. Whole-block validation
rejects dialect/kind/value and zero-frame performance before any native mutation.
CLAP notes use wildcard note IDs, MIDI velocity quantizes, and UMP words/groups
remain unchanged without inferred translation. Note-only profiles need no params
extension; original factories retain their strict contracts.

Codec tests cover targets, short-message padding, zero velocity quantization and
one/two/four-word UMP preservation. Four linked note groups cover index-versus-ID,
missing extension, unsupported dialect/capacity, all four dialects per precision,
same-frame parameter/note order and whole-prefix rejection including empty
blocks. Each mixed profile runs 2000 successful blocks plus 2000 rejected calls,
without measured heap actions and with native gain unchanged after rejection.
The linked real sine note-only node checks timed sound and release silence.

Loaded public fixture passes 2000 graph blocks/32000 frames and two parameter
flushes per dialect/precision, through event edges, wet/dry routes, dynamic swaps
and main retirement. Loaded RAVET Sine passes 1000 no-input stereo blocks/4096000
frames, delayed onset, release silence and zero measured allocations. Evidence
is macOS arm64 synthetic offline processing; no device playback or broader host,
platform or deadline claim. An initial probe filtered notes at its parameter-only
edge; selecting the explicit all-event edge corrected the probe and all eight
loaded profiles pass.

Full all-feature workspace tests pass 403 tests including doctests. Default and
all-feature workspace Clippy, nine default doctests, both warning-denied Rustdoc
builds, formatting, documentation and whitespace checks pass. No new dependency;
enabled the pinned Clack note-ports feature. Native performance output, per-note/
modulation/gestures/editor/device and broader format/UI/platform/production work
continue; this is not overall programme completion.

## W108: Selected native performance graph output

Added independent inactive native note-output index/dialect selection, sharing
bounded output slots with optional global parameters. Concrete CLAP notes with
wildcard IDs narrow normalized velocity to portable f32; short MIDI/MPE and framed
UMP retain payload/group with unused native padding ignored. Unsupported ports,
dialects and unrepresentable note targets/identities filter explicitly. Selected
invalid values/statuses, offsets/order/capacity and zero-frame performance output
poison the block with nonrollback BackendFailure and clear audio/event prefixes.
Performance-only profiles need no params mapping/extension. Metadata stays fixed.

Two codec groups cover concrete/wildcard/specific targets, velocity narrowing and
invalid ranges, short MIDI padding/status/data, preserved one/two/three/four-word
UMP and dialect filtering. Seven linked note groups pass, including output-only
metadata/index-versus-ID, malformed MIDI/velocity, overflow, filtered ports/note
IDs, mixed parameter output, zero-frame flush and advanced native state after
output rejection. Both precision fault groups run seven profiles of 1000 blocks
plus flush; output-only runs 1000 blocks. No measured callback heap actions.

Loaded public echo fixture passes all four dialects in f32/f64, 2000 graph blocks/
32000 frames and two parameter flushes per profile, exact performance offsets then
read-only gain through all-event graph edges, wet/dry live swaps and main
retirement. macOS arm64 synthetic processing only, without device playback or
broader host/platform/deadline claims. Existing parameter/input/sine regressions
retain acceptance. Host/graph/foundation tests, default/all-feature workspace
Clippy, nine default doctests, both warning-denied Rustdoc builds, formatting/docs/
whitespace pass. No new dependency. Per-note identities/expressions/modulation/
gestures/editor/device and broader formats/UI/platform/production remain work.

## W109: Main-owned editors for hosted graph nodes

Retained native graph control now creates the existing exclusive-borrowed editor
while the separate audio node/graph runs on its owner. Borrowing prevents main
reclaim/drop until GUI destruction, with a compile-fail ownership example.
The editor guard services at most one coalesced core callback per consumer tick,
preserving restart/process flags and deferred re-requests. Native parent, OS main
thread/event loop and further GUI/parameter/restart scheduling stay explicit.

Linked absent-extension rejection preserves usable control/audio ownership. Actual
RAVET Gain and upstream Clack Gain EGUI bundles each pass three embedded editor
create/parent/show/resize/hide/drop cycles while processing 3000 graph blocks/
48000 frames, exact synthetic stereo unity output and zero measured audio heap
actions. Deliberately coalesced callback service preserves restart/process flags;
main child cleanup, owner stop and inactive instance return pass. macOS arm64
only, no device/DAW/focus/accessibility/platform/deadline claim. The initial probe
prepared its node twice; allowing graph preparation to own that transition fixed
the probe. Corrected warning-free native runs pass for both bundles.

Host/graph tests, default/all-feature workspace Clippy, ten default doctests,
both warning-denied Rustdoc builds, formatting/docs/whitespace pass. Tool package
now uses existing graph/foundation/allocation-counter dependencies; lock metadata
updated, with no new external package. Device callback owner integration and
per-note/expression/modulation/gestures, broader formats/UI/platform/production
remain independent work; overall programme is still active.

## W110: Buffered f32 graph-worker device output

Added control-prepared no-input graph output pumping through an existing SPSC
queue. Its borrowed non-Send worker session starts/stops graph/native nodes on
the worker, retains exact whole-frame backpressure suffixes without rerendering,
and returns stopped storage for main retirement. RingOutput provides bounded
queue consumption, invalid-block dequeue preservation, underflow silence and
relaxed live source/underflow diagnostics. Worker/device rate/priming/wake/join,
clocking and recovery remain explicit; pump has no wait/thread/I/O behavior.
Worker/consumer scratch caps are 32/16MiB outside graph/native/queue caps.

Five portable groups pass constructor rejection, partial/full retention and
frame order, 1000 heap-free pump/consume blocks, graph fault/no-publication/reset,
underflow and malformed dequeue preservation. Worker stop precedes main
retirement, with a compile-fail non-Send session example. The initial lifecycle
assertion omitted graph preparation's main deactivate; corrected counts pass.

The loaded public Sine silent probe passes 256000 consumed offline frames,
counting 17408 underflow frames during deliberately unclocked consumption,
without processing/backend/panic failures. The explicit UMC1820/macOS arm64
20-channel 96kHz silent native run passes 10 seconds, 1875 callbacks/960000
consumed frames, zero queue underflows/processor faults/backend errors/panics,
964352 graph-rendered frames and orderly worker stop/main graph/native return.
Graph pump heap instrumentation passes; offline device rendering also measures
no heap actions. This is one short silent hardware observation, not clock,
long-session/drift/hotplug/audible/deadline or broad device/platform support.

Device/host/graph tests, default/all-feature workspace Clippy, eleven default
doctests, both warning-denied Rustdoc builds, formatting/docs/whitespace pass.
Device/tool packages use existing graph/host/queue/Clack/allocation-counter
packages; lock metadata updated without new external packages. Device probe
keeps its previous default CLI via default-run, with narrow trusted native-load
unsafe allowance under deny policy. Input/events/dynamic graph/device recovery,
per-note/modulation/gestures and broader formats/UI/platform/production remain
independent work; this prototype does not complete the programme.

## W111: Bounded graph-worker event input acknowledgement

Buffered graph workers now permit 0..4096 explicit input slots and a borrowed
ordered stream for each fresh fixed render. Progress acknowledges consumption
only after a successful new render; retained suffix pumps leave new input/context
untouched, avoiding repeated parameter/note delivery under backpressure. Outer
validation rejects malformed input before nodes; errors retain nonrollback graph
semantics and are not safe automatic delivery retries. No timestamp/clock/late
retiming or asynchronous event scheduler is inferred; output capacity stays zero.

Six graph-output groups pass, including once-only offset parameters across full/
partial queue publication, false pending acknowledgements, malformed outer input
without node mutation/publication, existing order/underflow/fault/reset/retirement
and measured heap-free pumping. Loaded public Sine offline note mode passes
selected native MIDI input through the queue, delayed onset/finite stereo sound,
release silence and 256000 consumed frames. Corrected final probe reports 16384
underflow frames during deliberately unclocked offline consumption, no faults/
backend errors/panics, zero measured worker/render heap actions and main return.
Silent offline mode retains acceptance. Notes mode rejects device arguments;
physical output remains silent. This is not audible hardware/MIDI/clock evidence.

Device/host/graph tests, default/all-feature workspace Clippy, eleven default
doctests, both warning-denied Rustdoc builds, formatting/docs/whitespace pass.
No dependency change. Worker output events/async control queues/input/live graph
edits/recovery, per-note/modulation/gestures and broader formats/UI/platform/
production remain work; overall programme remains active.

## W112: Caller-owned generated events from graph workers

Buffered output workers now permit 0..4096 output slots with explicit graph/node
routes. pump_io accepts exact caller-owned capacity and returns a valid event
prefix once per successful fresh render, regardless of partial/zero immediate
queue publication. Pending suffix calls leave storage untouched and return zero
output count. Empty-output convenience methods remain; positive output capacity
requires pump_io. Event offsets describe render blocks, without inferred device
clock, audible correlation or control/GUI delivery. Errors return no valid prefix
or faulty audio publication, without rolling back prior native histories/input.

Seven graph-output groups pass, adding absent/excess output capacity rejection
before node execution, exact generated offset/prefix, no output replay across full/
partial pending publication and measured heap-free pumping. Loaded public native
parameter fixture passes 1000 two-node worker graph blocks/8000 frames, stepped
sample input/read-only output through explicit graph edges, a four-frame queue
retaining eight-frame blocks, once-only telemetry, exact audio and zero measured
heap actions. Owner stop/main graph/native instance return pass. This synthetic
macOS arm64 probe opens no device/GUI and does not promise clocks/deadlines.

Device/host/graph tests, default/all-feature workspace Clippy, eleven default
doctests, both warning-denied Rustdoc builds, formatting/docs/whitespace pass.
No dependency change. Cross-thread input/output control queues, live graph/input/
recovery, per-note/modulation/gestures and broader formats/UI/platform/production
remain work; overall programme remains active.

## W113: Complete bounded graph-worker/control event queues

Added fixed whole-batch SPSC event queues for explicitly scheduled control input
or worker output. Preparation bounds 1..65536 frames, 0..4096 events and 1..64
queued batches, with <=32MiB queue values plus one retained consumer batch. Safe
pinned rtrb chunk APIs atomically publish headers/tags/context/events/unused slots.
Full/invalid input publishes no prefix; observed disconnect rejects. A borrowed
batch remains pending until explicit acknowledgement after consumption/discard;
dropping its borrow cannot replace it. Tags/context stay opaque, with no native
transport/clock/retiming/replay or automatic scheduler inferred.

Three queue groups pass fixed capacity/full rejection, retained/empty batches,
context and mixed parameter/note/MIDI/UMP/same-frame order, malformed/disconnect
and 10000 cross-thread publish/read/ack rounds with zero measured heap actions on
both owners. Loaded public parameter fixture passes 1000 complete main/worker
input-output rounds/8000 native frames, exact mapped read-only telemetry/tags,
retained audio suffixes/no repeated native input, zero measured worker heap actions
and owner stop/main retirement. Corrected final probe handles control teardown
while waiting. Synthetic macOS arm64 only, without GUI/device/clock/deadline claim.

Device/host/graph tests, default/all-feature workspace Clippy, eleven default
doctests, both warning-denied Rustdoc builds, formatting/docs/whitespace pass.
Device now directly uses already pinned rtrb 0.4.0; lock metadata updated without
new external package. Async event queues are usable primitives; actual GUI/device
clock/lifecycle policy, live input/edits/recovery, per-note/modulation/gestures and
broader formats/UI/platform/production remain independent work.

## W110–W113 event-capacity correction

Review during W114 found that earlier W111/W112 observations incorrectly stated
4096 usable graph-render events per direction. Graph rendering has always capped
input/output at 1024; the worker now checks the shared public graph limit before
node preparation. Current guides/API/catalogue are corrected. Independent complete
batch queues retain their 4096-slot limit, and callers must fit receiving graph
profiles. The small native probes above never exercised the overstated range;
their measured evidence remains unchanged. This correction does not increase
render capacity or claim new host/device support.

## W114: Explicit parameter gesture boundaries

Added typed stable-ID global begin/end events, parameter/performance graph filter
semantics and complete worker queue round trips. Generated CLAP output maps
boundaries without a MIDI port, preserving preflight/prefix refusal. Hosted graph
output opts in explicitly, aliases cached writable mappings and permits zero-frame
parameter flush. Disabled/unmapped boundaries filter; read-only/order/offset/
capacity faults clear the block without rolling back native histories. Pairing,
repeated ends, ownership, cancellation and recording remain receiver policy.

Linked f32/f64 tests cover seven output profiles per precision with 1000 blocks,
flushes, disabled filtering, malformed output and measured zero heap actions.
Loaded public gesture fixture passes 2000 graph blocks/32000 frames per precision,
wet/dry routing, two flushes, live swaps, owner stop and main retirement with zero
measured allocations. Portable queue/filter and generated CLAP tests pass. The
capacity correction above adds pre-node rejection coverage for 1025-slot worker
profiles while preserving owner cleanup.

All-feature workspace tests pass 427 tests including sixteen doctests; default
and all-feature Clippy, eleven default doctests, both warning-denied Rustdoc builds,
formatting/docs/whitespace pass. Synthetic native macOS arm64 only, with no new
DAW/device/recording/deadline claim. Native gesture input, modulation/per-note,
actual GUI/device-clock/lifecycle integration and broader formats/UI/platform/
production remain independent work; overall programme remains active.

## W115: Global modulation distinct from saved base values

Added finite signed physical global parameter modulation events, parameter graph
filtering and complete worker queue validation/round trips. CLAP input translation
is explicit and disabled by default on event/effect/main/auxiliary bridges; native
amounts narrow to finite f32, scoped events ignore, and flush offsets normalize to
zero. Generated global output widens to native f64 without a MIDI port and retains
preflight/prefix refusal. Receiver DSP owns combining/clipping/smoothing/reset and
persistence; no saved-state changes or metadata flags are inferred.

Portable input/output tests include signed/out-of-base-range amounts and zero-frame
flush; queue invalid whole batches publish nothing. CLAP tests pass ignored default/
scoped input, same-frame order, finite narrowing, invalid timing/nonfinite rejection,
10000 heap-free translations/flushes, 1000 heap-free generated output blocks and
1000 prepared consumer blocks. End-to-end consumer tests preserve base through
modulation/flush and reject invalid whole input before DSP/history mutation.

All-feature workspace tests pass 432 tests including sixteen doctests; default/
all-feature Clippy, eleven default doctests, both warning-denied Rustdoc builds,
formatting/docs/whitespace pass. No dependency change. Offline fixtures establish
no native hosted modulation, reference plug-in modulation, DAW/device/deadline or
per-note/other-format support. Those and GUI/device/format/UI/platform/production
remain independent work; overall programme remains active.

## W116: Mapped hosted native global modulation

Native graph preparation now independently opts input/output modulation into
explicit writable parameter mappings, caching native modulatable flags on main.
Unsupported input rejects before native mutation, with finite f32 amounts widened
to f64 and null global cookies. Enabled output narrows finite native f64 amounts
to f32 independently of scalar base range/step checks. Disabled/unmapped/scoped
output filters; selected data/order/offset/capacity faults clear the block with
nonrollback BackendFailure. Zero-frame flush works without render advancement.
Reusable CLAP registration has a default-false explicit global modulation option;
metadata alone enables neither bridge translation nor processor DSP.

Linked f32/f64 tests pass twelve 1000-block profiles per precision, fractional
modulation of a stepped base, disabled input, native flag mismatch, scoped/unmapped
filtering, malformed output/overflow, shared capacity and parameter flushes.
Unknown modulation after a valid scalar prefix leaves native base unchanged; a
subsequent empty flush confirms it. A selected output missing the native flag
rejects after native base mutation, proving nonrollback behavior. Metadata and
input/output selection-before-activation checks pass. Loaded public fixture
passes 1000 native blocks/8000 frames per precision, two flushes, exact base/
modulation aliases, measured zero audio heap actions and owner stop/main retirement.

Host/CLAP regression tests, default/all-feature workspace Clippy, eleven default
doctests, both warning-denied Rustdoc builds, formatting/docs/whitespace pass.
No dependency change. Synthetic macOS arm64 only, without broad host/device/
deadline, per-note/modulation matrix/rescan or other-format claim. Queued audio
input, GUI/device-clock/lifecycle/recovery and broader format/UI/platform/production
remain independent work; overall programme remains active.


## Live documentation portal (9 October 2026)

Implemented a MkDocs Material portal over canonical guides, feature metadata,
decision records and workspace Rustdoc. The isolated Python dependency set is
pinned; guide search, responsive navigation, theme switching and copied code
controls are configured. The live server watches canonical input directories,
regenerates its catalogue without changing tracked Markdown, and rebuilds
warning-denied Rustdoc after workspace input changes. The generated staging
folder is explicitly unwatched to prevent recursive rebuilds. Strict site builds
require fresh canonical metadata. Guide-only builds omit stale API output.
API generation clears cached HTML first, excluding obsolete package references
from the combined artifact. CI now builds and uploads a combined site artifact;
no public deployment is set.
Decision 0124 records the publication and source-of-truth boundary.

Local macOS arm64 validation: documentation checker and staging regression tests,
strict complete portal build, workspace Rustdoc doctests and warning-denied
Rustdoc build passed. A browser displayed the portal and confirmed that adding
and then editing a temporary guide automatically rebuilt the page and refreshed
its visible text; the temporary guide was removed afterwards. The site search
index contains guide headings and content; Rustdoc is served beside the guides
with its own search. These observations do not establish hosted deployment,
other Python/platform environments, DAW compatibility or real-time guarantees.

## W117: Queued audio input to hosted graph workers

Added an explicit with_input constructor alongside the no-input worker profile,
matching graph/input/output ring channels with combined worker scratch<=32MiB.
Fresh fixed f32 renders consume one queued prefix, zero shortages and return
source/underflow frame counts. Pending output suffixes consume no new audio or
events. Outer event/storage validation precedes dequeue. Graph faults may consume
input/advance histories; faulty publication is suppressed and the worker retains
its error until explicit reset without draining more input. Reset preserves
queued/pending audio and input ring data. Worker/main native ownership is unchanged.

Four new portable groups pass timing through a half-block output queue, untouched
input on malformed event/storage, input starvation silence, fault latch/reset,
channel/combined-memory rejection and 1000 heap-free rounds. Existing eight
worker-output groups and device/host/graph regressions pass. Loaded public Gain
passes 1000 queue-fed native stereo blocks/16000 source frames plus a starved
block, exact suffixes, no heap actions and owner stop/main retirement. Existing
loaded Sine worker output and queued native parameter telemetry probes also pass.

Default/all-feature workspace Clippy, eleven default doctests, both warning-denied
Rustdoc builds, formatting/docs/whitespace pass. No dependency change. All new
native evidence is offline macOS arm64; no physical capture/duplex, independent
clock alignment, latency/deadline/long-session/hotplug/recovery claim. Those and
GUI/live edits/per-note/format/UI/platform/production remain independent work;
overall programme remains active.

## Small feature examples (9 October 2026)

Added the `ravet-examples` workspace package under `examples/tutorials` with four
small executable walkthroughs. MIDI synth renders C4/E4/G4, sample-timed sustain
and completed release through the existing portable sine instrument. Effect
renders stereo tones through the existing gain processor with 5 ms smoothing,
nonaligned automation and exact bypass timing. Both write explicit stereo PCM16
48 kHz WAV files outside processing and refuse existing destinations. The guide
links the existing Sine/Gain CLAP exports rather than adding duplicate wrappers.
MPE demonstrates RPN6/RPN0, independent member pressure/timbre/bend, shared master
bend and released expression ownership. MIDI 2.0 preserves 16-bit velocity and
32-bit expression through typed codecs, incremental UMP framing and validated
sample-offset events, including ordered same-frame packets.

Executed on macOS 27.0.1 arm64 with Rust/Cargo 1.99.0: all three new executable
walkthrough tests pass, including decoded WAV timing, pedal hold/release, stereo
automation, in-block bypass, overwrite preservation and expression/packet traces.
Default workspace check and 418 workspace tests pass; workspace all-target
Clippy with warnings denied, explicit workspace Rustdoc tests, warning-denied
workspace API build, formatting, documentation checker, 26 Python script tests
and strict documentation site builds with and without API pages pass. Generated both default WAV
artifacts under `target/examples/`. The current Rust Clippy constant-size chunk
lint also required three equivalent iterator changes in the existing device
probe and clocked-input tests; the affected clocked-input suite passes again.
Those iterator changes preserve behavior and need no capability/contract update.

Only local workspace dependencies were added. These are offline teaching runners,
not standalone device applications; the MPE walkthrough does not synthesize
pressure/timbre, and MIDI 2.0 does not add synthesis, negotiation or a plug-in
export. No new platform, DAW, physical controller or real-time deadline support
is claimed. Existing backend validation limits remain separate.

## W118: Buffered worker input-clock conversion

Added an explicit prepared worker stage reusing existing sinc/FIFO correction with
an internal channel-preserving relay, publishing graph-rate f32 capture into a
separate queue. Matched channels/rates/raw capacity/target/sinc bounds validate at
construction; one fixed output block adds<=16MiB to converter/queue memory.
Destination full/partial publication retains exact suffixes without reconversion,
raw consumption or controller advancement. Existing priming/underflow/reprime
and independent statistics policy remains, without inferred timestamps/latency.

Three new groups pass priming-zero/retained/full publication, configuration
rejection and 1000 nominal 96k-to-48k stereo rounds with finite/scaled samples,
settled levels, no heap actions or converter underflow. Existing clocked tests
pass. Loaded public native Gain graph passes 1000 queued blocks/16000 graph input
frames with and without this conversion stage, retained half-block output, a
separately starved graph-input block, no measured heap actions and owner stop/main
retirement. No device/GUI/DAW opens; offline nominal conversion establishes no
independent physical clock/drift alignment or deadline/long-session/recovery claim.

Full all-feature workspace tests and default/all-feature workspace Clippy pass,
as do eleven default doctests, both warning-denied Rustdoc builds and formatting/
docs/whitespace. The independently added tutorial package is preserved and included
in the broad checks; its catalogue entry subsequently arrived. Device probe adds
a direct dependency on existing workspace signal APIs; lock metadata also includes
the tutorial package, without new external packages for this converter change.
Live worker graph replacement, actual GUI/device clock/recovery, per-note and
broader formats/UI/platform/production remain independent work; programme active.

## W119: Live prepared replacement in buffered native graph workers

Shared fixed-worker queue/scratch/event/preflight/fault mechanics with an already-
prepared dynamic f32 engine. Exact configuration is checked without reprepare.
Candidates are considered only on fresh renders after old copied output suffixes
publish. Progress separates swap outcome from processing; pending calls have none.
Failed candidate start or full retirement preserves old rendering. Historical
last_swap remains observable when a swap precedes a processing fault, through
outer rejection/pending/fault-latch calls. No crossfade/tail drain/state migration.

Six portable groups pass old/new suffix order, bounded retirement/reclaim, invalid
outer input preserving offers, failed start, exact configuration, queued input/
underflow, once-only generated prefix and post-swap fault observation. A 1000-block
workload measures zero heap actions and checks owner start/stop/main destruction.
Loaded two-instance public Gain passes 1002 queue-fed stereo native blocks/16032
frames, replacement after the old suffix, main old-native retirement while new
processing continues, 1000 steady blocks and no measured heap actions. Clocked
input, Sine output and queued parameter telemetry native regressions pass.

Device/host/graph regressions, default/all-feature workspace Clippy, eleven default
doctests, both warning-denied Rustdoc builds, formatting/docs/whitespace pass.
No dependency change. Offline macOS arm64 only; actual GUI/device-clock/lifecycle/
recovery, per-note/extended transport and broader formats/UI/platform/production
remain independent work. Overall programme remains active.


## W120: Extended block-start transport metadata

Added optional details for recording/active-loop/pre-roll flags, time signature,
BPM-per-sample tempo increment, bar start/zero-based number and active beat/sample
loop bounds. Both CLAP directions validate advertised metadata and matching known
timelines; sample/fixed-point rounding must preserve endpoint order. Missing
context preserves unknown details. No timeline extrapolation/reset/mid-block update.

Two ingress groups and two additional hosted groups pass flags/units/negative
pre-roll, absence, malformed signature/increment, dependencies and collapsed loop
bounds; 10000 conversions per direction measure no heap actions. Complete worker
batches retain details unchanged. Existing gain bridge/native graph regressions
and full workspace all-feature tests pass. Offline adapter evidence only; extended
metadata has no new loaded DAW/physical transport/deadline claim. Decision 0127.

## W121: Prepared synthetic stereo reverb

Reused bounded mono networks with common L/2+R/2 excitation, sample-rounded
0..10ms right-delay spread and wet width interpolation. Dry stereo is preserved;
opposite-polarity input cancels wet excitation. Complete mix validation preserves
histories; reset preserves configuration. Twelve delays cap at 133094 f64 samples.

Three new groups and existing mono regressions pass exact first impulse timing,
width/dry/cancellation, decay, zero-spread mono equivalence, partition/reset,
maximum memory, malformed configuration/mix and nonfinite recovery. Processing,
reset and mix across 10000 frames measure zero heap actions. Full workspace
all-feature tests pass; portable evidence only, no guaranteed decorrelation,
physical-room/freeze/modulation/native deadline claim. Decision 0128.

## W122: Prepared periodic wavetable oscillator

Added immutable copied 2..65536 finite f64 cycles, wrapped linear interpolation
and a private shared phase accumulator preserving the existing oscillator profile.
Retuning retains phase; reset retains frequency/table. Table allocation/destruction
stay outside processing. No bandlimited/alias-free synthesis or smoothing claim.

Three new groups and all signal regressions pass exact segments/final-to-first
wrap/non-power-of-two tables, analytic sine error, retuning/reset/partition,
invalid pre-allocation rejection and maximum capacity. 10000 sample/retuning/reset
operations measure zero heap actions. Portable offline evidence only. Decision 0129.

## W123: Explicit CLAP parameter gesture input

Added default-disabled raw begin/end input to all CLAP bridges/translator and the
hosted graph builder. Hosted input requires a writable map before activation;
disabled/unknown input rejects the complete prefix before native process/flush.
Timing/order/capacity retain existing policy; flush ingress offsets normalize zero.
Repeated ends are raw boundaries without scalar edits/pairing/recording inference.

Two translator groups pass mixed boundaries/values, ignored defaults, raw ends,
flush/timing/capacity and 10000 heap-instrumented rounds. New linked native f32/f64
groups pass mapped echo/time, audio/base preservation, valid-prefix then unknown
and disabled atomic rejection, zero-frame repeated ends and 1000 heap-free blocks
per precision. Missing input map rejects before activation. Existing output gesture,
modulation/note/native graph and public bridge regressions pass. This is synthetic
offline ABI evidence, not third-party receiver/DAW/UI recording validation.

For W120-W123: default/all-feature workspace Clippy, default doctests, both
warning-denied Rustdoc builds, formatting, docs and whitespace pass. Concurrent
example UI compile errors were corrected (float syntax, JSON expression, optional
template error, mutable snapshot borrow and unused import), preserving its work.
No dependency added by these packages. Wider per-note/standalone/recovery/formats/
UI/platform/production programme remains active. Decision 0130.


## W124: Positive multiplicative parameter smoothing

Added finite strictly positive f32 geometric smoothing with explicit u32 sample
counts, identical-target continuity, current-effective retargeting, reset and
exact final snap. Zero/negative endpoints use NonPositiveValue; nonfinite keeps
its existing error. Invalid edits preserve full progress. f64 log-difference ratio
design avoids extreme endpoint division overflow; sample work uses multiplication
and direction-aware endpoint clipping, including ratios rounded to unity.

Three new groups pass geometric landmarks, independent log interpolation,
positive subnormal-to-maximum monotonic transitions, retarget/reset/partition,
very long rounded-unity rising/falling ramps and invalid edits. Prepared samples,
retuning/reset measure no heap actions. All signal regressions, default/all-feature
workspace lint, default doctests, both warning-denied Rustdoc builds, formatting/
docs/whitespace pass. No dependency change. Portable evidence only; zero/signed
crossings, duration smoothing and native deadline claims remain outside this
profile. Wider programme remains active. Decision 0131.


## W125: Explicit note identities in hosted native graph routes

Added portable NoteWithId On/Off with concrete channel/key and full-u32 local
identity. Scalar checks preserve zero-velocity On; performance graph filters and
complete queues retain the event without voice/uniqueness/lifetime inference.
Opt-in hosted CLAP directions require selected CLAP ports before activation and
preserve native0..i32::MAX IDs. Default input rejects, output filters specific IDs;
wildcard native output retains legacy notes. Invalid input rejects the complete
prefix pre-native; selected raw IDs<-1 reject instead of Clack's wildcard collapse.
Generated MIDI output rejects identified streams before any host prefix, retaining
already rendered audio/DSP rather than dropping identity.

Portable validation/prefix tests, performance graph filtering and 10000 complete
queue roundtrips pass. Native unit tests pass0/i32::MAX/zero On/negative raw IDs.
Linked f32/f64 fixture profiles pass1000 blocks per input/output selection, exact
echo/timestamps/audio and disabled/invalid-prefix state preservation, with zero
measured heap actions. Missing/unsupported direction rejects before activation.
Generated-MIDI rejection passes1000 callbacks per identity with audio preserved
and no host push attempts/heap actions.

Full workspace all-feature snapshot passes488 tests including doctests; targeted
graph filtering, default doctests, both warning-denied Rustdoc builds, default/all-
feature workspace lint, formatting/docs/whitespace pass. Concurrent standalone
window build/object archive and example UI syntax/checked Option branch issues
were corrected while preserving its implementation. No new dependency. Synthetic
offline macOS arm64 evidence only, no third-party voice/DAW/deadline claim. Scoped
expression/parameter/lifecycle and wider formats/UI/platform/production work
continue. Overall programme remains active. Decision0132.

## Interactive examples, default I/O popup and piano controls (9 October 2026)

Replaced default WAV/trace entry-point behavior with live macOS windows. The
synth has eight voices, phase-preserving sine/saw/square/triangle switching,
low-pass VCF, VCA and one shared ADSR per voice. Prepared filter tables and a
fixed patch queue keep design/JSON/device ownership outside processing. The
effect plays demo tones or selected input; MPE pressure/timbre affect audio.
MIDI 2.0 remains an interactive full-resolution codec inspector.

Moved the standalone host window into ravet-webview. Its native title-bar
Audio/MIDI popup is enabled by default and explicitly removable. Standard
settings expose output/input channel pairs, supported output rates, requested
buffer size with nominal milliseconds and independently selected MIDI inputs.
The consumer retains device validation/lifecycle. These examples support up to
16 MIDI inputs and 64 device channels; output pairs duplicate stereo and selected
input pairs average into stereo. Embedded plug-in routing remains host-owned.

Added shared rotary controls and a two-octave piano widget with short overlapping
black keys, chords, glissando, cancellation/blur release and nonfeedback external
note display. Synth and MPE fit at both 900x780 and 760x650 logical content sizes;
voice details scroll inside their bounded panel. Disabled piano keys retain full
opacity, so white-key edges cannot show through the black-key overlay; the native
fixture checks every stopped key's opacity. All four native fixtures passed again
after this fix, and the stopped synth screenshot was inspected. Shared CSS and
SVG semantic palette tokens use Amber, selected by the user after reviewing five
alternatives on 9 October 2026.

On this macOS arm64 checkout, 449 workspace runtime tests pass, including four
portable subtractive tests and three example audio-adapter tests. Measured 1000
synth patch/process/reset cycles and 1000 selected-channel effect blocks have no
heap operations. Seven no-default-feature example tests pass. Native hidden
WebKit fixtures pass all four tabs, title-bar activation/full opt-out, knobs,
channel/rate/buffer/MIDI lists, piano geometry/chords/glissando, release and window
bounds; screenshots were inspected under target/examples-ui. They open no devices.
The live synth preview was relaunched for user evaluation, without automatically
selecting or starting a device. The palette comparison cycles through five
variants in the collaborative browser; its navigation was visually checked.

Workspace Clippy, 11 doctests, warning-denied Rustdoc, formatting, the documentation
checker and 26 Python tests pass. The first workspace doctest invocation overlapped
independent changes to foundation note-expression exports and used stale compiled
exports; rebuilding and rerunning workspace doctests passed. Existing unrelated
work is preserved. These results establish no broad desktop/device/controller
support, MIDI timestamp/sample alignment, long-session stability or deadline.
See [decision 0132](decisions/0132-standalone-settings-and-knobs.md).

## Stereo delay effect example (9 October 2026)

Replaced gain in the interactive effect tab and explicit WAV fixture with a
portable mono/stereo delay composed from prepared RAVET delay lines and linear
smoothers. Added 1..2000 ms time, 0..0.95 feedback and wet/dry knobs, ping-pong
cross-feedback and dry bypass that preserves internal repeats. Time glides over
20 ms; feedback/mix/routing use 5 ms ramps. Storage is prepared for two seconds
per channel; processing and reset allocate nothing. Live demo input now uses
staggered tapered bursts so repeats can be heard without an input device. Amber
styling, the default Audio/MIDI popup and selected live-input routing are reused.
The gain plug-in export remains separate; no delay plug-in wrapper is claimed.

On this macOS arm64 checkout, 14 example tests pass with default features and
11 portable tests pass without them. Four new delay tests check stereo and
ping-pong impulse timing/decay, partition-independent automation, bypass tail
retention, late invalid-event rejection without history changes, mono/max delay
and 1000 prepared control/process cycles plus reset with zero heap operations.
The WAV walkthrough checks echo placement and sample-timed bypass. All four
native UI fixtures pass again, including delay knob/check-box snapshots and
effect bounds at 900x780 and 760x650. Screenshots were inspected and the live
effect preview was opened without automatically starting a device.

The strengthened fixture found that earlier minimum-size checks had tested a
fixed browser viewport after native window resize. Fixed standalone container
autoresizing and now assert actual browser dimensions first. The MPE layout
needed additional compact spacing at the real smaller viewport. This supersedes
the earlier minimum-window validation evidence above.

Example Clippy, formatting, workspace doctests, warning-denied Rustdoc and the
documentation checker pass. These checks open no hardware devices and establish
no broad device/DAW support or real-time deadline.
See [decision 0133](decisions/0133-stereo-delay-example.md).

Follow-up transport, source and preference work: the main button now toggles
Start/Stop using current device selections, opening settings only when output is
unconfigured. Demo loop is an effect-panel control and can switch to live input
while playing; the standard popup contains device inputs/No input only. Demo
bursts repeat every 0.9 seconds, with both channels tested over three complete
cycles and live-source switching checked. Standard standalone Audio/MIDI
selections persist as bounded versioned native preferences keyed by stable
window title. Restore selects available exact IDs without opening devices;
hidden fixtures use process-local memory and do not alter user preferences.

Fifteen default-feature example tests and six webview tests pass. Native fixtures
check main-button stop/restart, source toggling, absence of demo entries in device
settings, complete output/input/pair/rate/buffer/MIDI restoration across newly
created windows, missing-device handling and no automatic start. All four tabs
pass actual viewport and document-bound checks at 900x780/760x650; meter readouts
stay inside their panels. Inspected screenshots now have the expected native
pixel dimensions. The demo sound and persistent native disk integration still
need user evaluation on actual chosen devices; no broad hardware claim follows.

## UI text selection defaults (9 October 2026)

Shared web styles disable label, heading and other body text selection in the
example editors and standard Audio/MIDI popup. Inputs, textareas, editable
content and preformatted/code output retain selection for editing and copying.
The policy includes the WebKit-prefixed property and remains overridable by
consumer CSS.

Collaborative browser checks pass for 53 label/heading/hint/readout elements,
numeric inputs, textarea/editable content, packet output and text-input
selection. Workspace doctests, warning-denied Rustdoc, formatting, documentation
and whitespace checks pass. The existing hardware-free native UI fixture was
attempted twice and stopped at its exact 760x650 viewport-size wait; this run
does not establish native selection or resize validation. No hardware device
was opened and no broader platform support is claimed.


## W126: Scoped standard expressions in hosted native graphs

Added portable wildcard channel/key/ID NoteTarget and seven standard expressions
with finite unit ranges and exact f64 values. Performance routes and complete
worker queues retain scope and bits. Explicit selected CLAP input/output builder
flags require matching directions before activation; invalid/disabled input
rejects whole prefixes pre-native, disabled/unknown/unselected output filters,
and selected malformed/capacity output faults retain native history. Generated
MIDI output rejects expressions before any prefix rather than lose scope.
Clack 0.2.0 omits Expression from its raw type accessor; conversion now matches
all seven public wire discriminators directly.

Portable validation, graph filtering and 10000 queue roundtrips pass; 21000 native
unit conversions retain all expression kinds/scopes/values without heap actions.
Linked f32/f64 fixtures pass 1000 blocks per selected profile, exact echo/time/audio,
complete input rejection, output capacity/nonrollback faults and preparation
requirements without measured heap actions. All-feature workspace tests, default/
all-feature workspace lint, default doctests and both warning-denied Rustdoc
builds, formatting/docs/whitespace pass. An initial workspace snapshot used an
older concurrent example test; the current-source rerun passes. No dependency
change or third-party voice/DAW/deadline claim. Native exported expression profiles,
note lifecycle/per-note parameters and the wider programme continue.
See [decision 0134](decisions/0134-scoped-note-expressions.md).


## W127: Explicit native performance ports in exported CLAP adapters

Added opt-in native note/expression input to the prepared translator/effect/main/
multibus adapters and native generated output to main/multibus. Independent
0..256 selectors preserve raw MIDI/UMP routing. Concrete On/Off retains optional
native IDs and checked f32 velocity; all seven expressions retain wildcard scope
and f64 values. Malformed selected data rejects the complete input pre-DSP with
silence. Native performance is ignored on input flush; zero-frame process rejects
representable selected events. Generated native too-wide IDs/performance in flush
reject whole delivery before host prefixes while retaining rendered audio/DSP.

Seven new groups pass all expression kinds/scopes, endpoints/zero On, raw field
rejection/default filtering, exact native output, independent MIDI/UMP ports,
pre-DSP prefix rejection, host refusal and parameter flush. Prepared translation
10000 calls and main f32/f64/auxiliary f64/mixed-port 1000-block workloads measure
no heap actions. CLAP/host/public reference regressions, default/all-feature
workspace lint, default doctests, both warning-denied Rustdoc builds and formatting/
docs/whitespace pass. Rebuilt public Gain bundle smoke passes 18 blocks/six
activation cycles; validator passes 34, skips 10, fails/warns zero. No dependency
change. Offline adapter/default-reference ABI evidence only; no new native
expression instrument, DAW or deadline claim. Wider programme continues.
See [decision 0135](decisions/0135-exported-native-performance-ports.md).

## Uniform example scaling and aspect lock (9 October 2026)

Replaced width-responsive panels and delay-only knob resizing with one fixed
900x780 example design canvas. Native content resizing now scales the complete
interface uniformly, including text, controls, panels and positions, immediately
from browser resize events and without the previous scale cap. All example tabs
opt into a 900:780 native content aspect-ratio lock. Added an explicit optional
StandaloneOptions lock; other consumers retain freely resizable windows by
default. Locked programmatic requests fit the largest matching rectangle within
the requested bounds, subject to the native minimum size. The device settings
popup retains its independent layout and persisted selections.

The native fixture passes all four tabs at 720x624, 900x780 and 1080x936, and
constrains wide/short and narrow/tall requests. It checks actual browser sizes,
all visible component dimensions and positions against their baseline, absence
of clipped content and bounded meter readings. Smaller delay and larger MPE
screenshots were inspected under target/examples-ui-aspect. This supersedes the
previous partial scaling/reflow policy; the earlier validation remains historical.
No devices are opened by these fixtures. Example/webview tests, Clippy, workspace
doctests, warning-denied Rustdoc, formatting and documentation checks pass on this
macOS arm64 checkout; no broad platform or hardware claim follows.


## W128: Scoped key release and choke

Added NoteOffScoped with finite f32 release velocity and NoteChoke with scalar-
free matching target. Full-u32 portable scope passes performance graph routes and
complete worker queues. Selected hosted CLAP directions opt into scoped input/
output before activation; disabled/invalid input rejects whole prefixes pre-native.
Output preserves wildcard-channel/key Off/choke, leaving concrete Off's existing
identity selection intact. Shared raw scope checks reject malformed negatives
before normalization. Choke's native ignored velocity is accepted rather than
validated as a scalar. Exported native profiles also carry release/choke; default
MIDI rejects whole scoped streams before host prefixes.

Portable validity/prefix and performance-filter tests, 10000 complete queued
roundtrips, raw native direction/range/default tests and linked f32/f64 profiles
pass. Linked 1000-block profiles cover exact timed fixture audio/echo, disabled
input, invalid prefix, capacity/nonrollback and retirement without measured heap
actions; selected malformed output retains native history and unrelated output
filters. Exported 1000-block scope roundtrip/ignored velocity and generated MIDI
1000-callback rejection groups pass. Full all-feature workspace snapshot passes
517 tests including doctests. Default/all-feature workspace lint, default doctests,
both warning-denied Rustdoc builds and formatting/docs/whitespace pass after
removing redundant test parentheses. Rebuilt public Gain smoke and validator pass
18 blocks/six activation cycles and 34 passed/10 skipped/zero failed or warnings.
No dependency or public voice-policy change. Offline scope/ABI evidence only, no
third-party/DAW/deadline claim; note completion and scoped parameters continue
alongside broader formats/devices/UI/platform/production work.
See [decision 0136](decisions/0136-scoped-note-release.md).


## W129: Exported scoped parameter values, modulation and metadata

Added distinct finite f64 scoped value/modulation events with matching target and
routed note input endpoint, independent of saved/global f32 values. Parameter
graph filters include them; performance excludes them and complete queues retain
full bits. Exported translator/effect/main/multibus add independent scoped input
and main/multibus scoped output selectors, exact native IDs/fields/values and zero-
time parameter flush. Raw exact all--1 scope alone is global; malformed negatives
can no longer collapse into a global edit through Clack. Selected malformed input
rejects pre-DSP; missing generated routes/too-wide IDs reject whole host delivery
while retaining rendered audio. Eight scoped automation/modulation metadata flags
are explicit default-false options, with NONE/ALL const presets; public Gain keeps
NONE and global flags remain independent.

Portable range/scope/prefix tests, parameter graph filtering, 10000 complete queue
roundtrips/translator workloads and all 256 metadata flag combinations pass without
measured prepared heap actions. Six adapter groups cover native f32/f64 and f64
auxiliary 1000-block process/flush precision, global independence, raw malformed
fields, atomic input rejection and whole generated route/width rejection with
audio preserved. CLAP/host/public-reference regressions, default/all-feature
workspace lint, default doctests, both warning-denied Rustdoc and formatting/docs/
whitespace pass. Rebuilt Gain smoke passes 18 blocks/six activation cycles; native
validator passes 34/skips 10/fails or warns zero. No dependency or voice-policy
change. Offline adapter evidence only; hosted scoped metadata validation/voice
application and the wider programme continue.
See [decision 0137](decisions/0137-exported-scoped-parameters.md).


## W130: Hosted scoped parameter validation and delivery

Prepared mapped native graph input/output now opt into scoped f64 values and
modulation independently of global parameters, note output and other performance
profiles. Both directions select the originating CLAP note input endpoint. Cached
writable native metadata checks most-specific ID/key/channel/port support, value
ranges and stepped integrality; finite signed modulation has separate bounds.
Invalid or disabled inputs reject whole prefixes before native mutation. Unknown
IDs/unselected ports filter; selected output faults discard complete portable
output without rolling back native history. Zero-time parameter flush and exact
raw global scope remain distinct from performance timing. Metadata changes need
retirement/repreparation. No new dependency or voice-allocation policy.

All 256 metadata flag combinations and six linked test groups pass: f32/f64
1000-block selected direction/capacity profiles, full-f64 echo/timing, separate
saved global state, granularity flags, rejected prefixes, value bounds/steps,
fractional modulation, zero-frame flush and invalid preparation. Prepared heap
workloads count zero allocations/deallocations. Full all-feature workspace
snapshot passes 533 tests including doctests; default/all-feature workspace lint,
default doctests, both warning-denied Rustdoc builds, formatting/docs/whitespace
pass. Rebuilt public Gain smoke passes 18 blocks/six activation cycles and native
validator passes 34/skips 10/fails or warns zero. Linked synthetic evidence only;
no third-party instrument, DAW or deadline claim. Broader format/device/UI/platform
work remains active alongside note completion and further DSP profiles.
See [decision 0138](decisions/0138-hosted-scoped-parameters.md).


## W131: Prepared fourth-order crossover

Added atomic LR4 control-thread coefficient design and fixed-storage one-channel
f64 low/high processing. Each branch cascades two identical second-order
Butterworth sections; cutoff is half amplitude and recombination is a unity-
magnitude second-order all-pass, without waveform identity. Exact block lengths
preflight before outputs/history; empty blocks no-op, reset clears and unsmoothed
replacement retains all four histories. No dependency change.

Five new groups pass independent bilinear response, all-pass impulse/energy,
cutoff sinusoidal power, exact partition/reset/replacement and invalid design/
response/shape checks. 10000 prepared replacement/reset/block workloads count
zero allocations/deallocations. Signal regressions, default/all-feature workspace
lint, default doctests, both warning-denied Rustdoc builds, formatting/docs/
whitespace pass. Electrical offline evidence only, without loudspeaker alignment,
automatic routing/modulation or callback deadline claims.
See [decision 0139](decisions/0139-linkwitz-riley-crossover.md).


## W132: Directional note completion notifications

Added portable NoteEnd matching targets distinct from key release/choke, with
carrier offsets for delivery order rather than exact termination time. Graph
performance filters and complete worker queues retain them. Hosted output selects
the originating CLAP note input endpoint independently of note output/maps/
identities; native timestamp/velocity are ignored and carrier order follows the
previous retained offset or zero. Raw scope validates before normalization.
Default/unselected/wildcard ports filter; selected invalid scope/capacity discard
portable output without native rollback. Portable hosted completion input rejects
whole prefixes; parameter-only zero-frame flush rejects output. Exported main/
multibus adapters select independent originating input endpoints, retain exact
matching IDs/fields, reject missing routes/wide IDs/flush before host prefixes and
retain audio/DSP progression. No automatic voice tracking or dependency change.

Portable prefix/range, graph filters, 10000 complete queue roundtrips and raw
native scope/ignored maximum timestamp/NaN velocity/flush tests pass. Four linked
groups cover f32/f64 1000-block no-note-output/no-output-map profiles, exact
carrier ordering, filtering, rejected controller input, capacity/nonrollback and
saved native history without counted heap actions. Four exported groups cover
f32/f64/main/multibus routes, preflight/width, host-prefix refusal, flush, input
filtering and 1000 prepared processing/flush workloads. Full all-feature workspace
snapshot passes 549 tests including doctests; default/all-feature workspace lint,
default doctests, both warning-denied Rustdoc builds, formatting/docs/whitespace
pass. Rebuilt Gain smoke passes 18 blocks/six activation cycles; native validator
passes 34/skips 10/fails or warns zero. Offline synthetic/ABI evidence only, no
public reference voice/third-party/DAW/deadline claim. Broader independent work
continues. See [decision 0140](decisions/0140-note-completion.md).


## W133: Prepared stereo fractional-delay chorus

Added a stereo f64 modulated delay effect with independent feedback histories,
shared sine LFO/right phase offset and bounded storage for every permitted live
control edit. Preparation validates 8000..192000 Hz and complete controls before
allocating at most 76802 f64 values. Fractional depth scales the center delay,
with a one-sample causal minimum. Atomic live edits retain phase/histories; zero
rate freezes phase, exact dry mode retains wet processing, reset retains controls
and invalid channel lengths preserve samples/history. No dependency change.

Five groups pass independent indexed full-history modulation/feedback recurrence,
fractional impulse/feedback polarity/stereo phase, exact partition/reset/dry
history/frozen phase, rejected construction/edits/shapes and maximum bounded
preparation. 10000 prepared process/edit/reset workloads count zero heap actions.
Signal regressions and default/all-feature workspace lint, default doctests, both
warning-denied Rustdoc builds and formatting/docs/whitespace pass after removing
an unnecessary integer cast in a test. No bandlimited modulation, automatic
latency/tail, headroom/denormal or native deadline claim. Generic native-format
processing infrastructure and the wider programme continue.
See [decision 0141](decisions/0141-prepared-stereo-chorus.md).

## Pre-push completed-work validation (9 October 2026)

An isolated completed-work snapshot passed 515 runtime tests and 11 doctests
with `cargo test --workspace --locked` on macOS arm64. Workspace Clippy with
warnings denied, warning-denied Rustdoc, formatting, the documentation checker
and all 26 Python script tests passed. The snapshot includes the live examples,
standard persistent Audio/MIDI settings, Amber controls and uniform aspect-locked
scaling. The concurrently edited native planar adapter refactor was excluded and
left in the working tree. These checks add no hardware, DAW or deadline claim.
The existing remote remains private; known proprietary paths in initial Git
history still prevent public publication.


## W134: Reusable native planar processing

Added the optional SDK-independent ravet-native crate with prepared f32/f64
main/auxiliary planar adaptation, bounded copied input/output planes and generated
events. Whole-stream validation precedes DSP; every input copies before foreign
output writes, including cross-bus aliases. Shape/alignment/output-overlap faults
silence supported active output ranges and retain tails. Exact typed transport
and generated events remain consumer-delivered; audio/reset panic poisons the
adapter until fresh stopped preparation. Both Gain C precision paths now use
this reusable core while retaining their product symbols/state and event layout.

Six generic test groups and five Gain groups pass, covering f32/f64 cross-bus
aliases, extreme f64 values, scoped/generated events, zero-frame processing,
instrument/no-input initialization, rejection/history, panic poisoning and
prepared allocation/deallocation checks. Default/all-feature workspace lint,
default doctests, both warning-denied Rustdoc builds, formatting/docs/whitespace
pass. Rebuilt actual VST3 passes 47 SDK validator tests plus Cocoa editor and
36-block/six-activation program contracts. AUv2 render/state/Cocoa editor and
420-block factory-program contracts pass; LV2 mono/stereo binary contracts and
AAX binary-entry probe pass. Native f64/program/meter C++ contracts pass. Local
macOS arm64 evidence only; duplicate-library linker warnings remain in SDK builds.
AAX is locally ad-hoc signed, without PACE or Pro Tools validation. Generic
metadata/event/state marshalling and SDK registration remain separate work; no
new DAW/device/deadline claim. See [decision 0142](decisions/0142-native-planar-adapter.md).


## W135: Complete native event marshalling

Added canonical version-one C-layout records for every portable event kind, full
u32 note/parameter IDs, explicit wildcard scope, exact widened f32 global values/
velocities and full f64 scoped values/expressions. MIDI short messages and all UMP
framing types retain their bytes/numeric words. Whole bounded stream conversion
preflights values/timing/capacity before destination mutation; equal-time order,
zero-frame offsets and tails remain exact. Unknown tags/bits, unused fields and
lossy scalar conversion reject. Gain now shares the separate unchanged legacy
global parameter layout/decoder, retaining existing product C signatures.

Six new groups pass exhaustive scopes/types, canonical golden records, invalid
prefixes, precision/offset/order/capacity bounds, 1000 full-capacity conversion
workloads and 1000 complete f32/f64 native instrument audio/event cycles with no
counted heap actions. Six planar and eight all-feature Gain regressions pass.
Default/all-feature workspace lint and default doctests pass; both warning-denied
Rustdoc builds pass after correcting one code span that Rustdoc interpreted as a
link. A repeated planar test launch briefly stalled in dyld before entering its
harness, then resumed and passed without intervention. Formatting/docs/whitespace
pass. Rebuilt actual VST3 validator passes 47/0 plus Cocoa editor and program
contracts; AUv2 audio/state/editor/420-block programs, LV2 binary contracts and
AAX entry probe pass. Five native C/C++ contracts now include both C11/C++17
header layout checks. macOS arm64 native evidence only; AAX remains ad-hoc/entry
only, without PACE/Pro Tools validation. Endpoint routing, host delivery/refusal,
state, generic factories and broader programme remain independent work.
See [decision 0143](decisions/0143-native-event-records.md).

## Hosted documentation source ownership — 9 October 2026

Moved the hosted reference's beginner introduction, practical guides, reviewed
tutorial chapters, line explanations and rendering tools into
[scripts/docs/hosted](../scripts/docs/hosted/README.md). The website publisher
reads this repository; its generated documentation is excluded from the website's
Git history. The website retains its presentation and service configuration.

Validated locally on macOS: the documentation checker, two hosted-navigation
regression tests, an all-features workspace Rustdoc build with warnings denied,
and 15 website/navigation checks. The index includes direct API and method
bookmarks; formatting and external-link behavior were inspected at desktop and
mobile widths. Public deployment was not performed.


## W136: Exact native block-start transport

Added a canonical v1 C-layout snapshot with independently available playing,
sample/beat timelines, tempo, details, signatures, increments, loops and bars.
Unknown states remain unknown; i64/f64/i32 values retain their complete precision
and semantic ranges. Dependencies validate before conversion: active ordered
loop ranges need their timelines, increments need tempo, bars need beats and
signatures need nonzero u16 components. Unknown bits, unused/reserved fields and
invalid values reject without guessed context, quantization or extrapolation.
Direct typed adapter pass-through stays unchanged.

Six groups pass all 32768 availability/flag combinations, full extreme metadata,
negative positions/signed zero, malformed native/portable snapshots and 10000
complete prepared f32/f64 processing cycles per precision with zero counted heap
actions. C11/C++17 header layout and native Gain precision/program/meter contracts
pass. Full all-feature workspace snapshot passes 572 tests including doctests.
Default/all-feature lint, default doctests, both warning-denied Rustdoc builds and
formatting/docs/whitespace pass after making bit-mask precedence explicit. Some
workspace executables stalled before their harness in dyld, then resumed; a
local temporary copy of the limiter binary independently passed too. Existing
duplicate validate example-name Cargo warnings remain. Synthetic local macOS
arm64 evidence only, without additional SDK/DAW/device/deadline claims. Generic
exported processor entry/lifecycle infrastructure and the wider programme continue.
See [decision 0144](decisions/0144-native-transport-records.md).


## Feature overview priority — 9 October 2026

Reordered the [feature registry](features.toml) and regenerated the catalogue
so reusable processing, graphs, plug-in integration and DSP lead the overview.
State, MIDI, device, file, UI and runtime capabilities precede reference
implementations and teaching examples; development tools, plans and exclusions
follow. Stable IDs, statuses, descriptions, limits and evidence are unchanged.
The [maintenance guide](maintenance.md) records the ordering policy for new entries.

Validation: documentation checker, all 12 documentation-checker regression tests
and whitespace checks pass. Parsed registry comparison confirms all 176 feature
records are preserved with identical contents.

Rebuilt the sibling website's static documentation snapshot for the local PHP
preview at port 8088. Browser inspection of `/ravet/docs/FEATURES/` confirms
portable processing leads and the four teaching entries occupy positions
162–165. The hosted publisher completed with a fresh all-feature Rustdoc build
with warnings denied; all ten website tests and both hosted-navigation tests
pass. The hosted tooling guide now explains that static previews require a
publisher rebuild after source edits. No public deployment was performed.

Public follow-up: published the ordering-only documentation change as commit
`e8fff3031ab1482cd53488e7da3c736f73f617a9` from an isolated checkout, preserving
the shared checkout's in-progress native changes. The published registry retains
all 172 existing records with identical contents. Website deployment
[37971051914](https://github.com/mo0kid/djwaudio.co/actions/runs/37971051914)
passed its documentation and website checks. Public browser inspection confirms
`https://djwaudio.co/ravet/docs/FEATURES/` starts with portable processing and
places the teaching entries at 158–161; other site files were still uploading
at that inspection. The local checkout retains additional unpublished features.


## W137: Generic exported native processor lifecycle

Added a consumer-selected immutable version-one entry table and opaque owner
composing prepared main/auxiliary f32/f64 audio, complete native input/generated
events, optional strict transport, lifecycle and latency/tail declarations.
Default+Send processors construct through contained control callbacks; complete
canonical configuration and native output capacity validate before DSP. Foreign
table/handle/precision, library lifetime and record/sample ranges remain caller
obligations. Supported output prefixes silence on faults; status/count always
rewrite valid result storage, with no generated delivery on faults or DSP rollback.
Poison persists until fresh stopped preparation. Gain exports this generic entry
alongside its existing product ABI.

Six new groups pass 1000 complete multibus/scoped/event/transport cycles per
precision with zero counted heap actions, no-input completion/silence behavior,
full latency/tail ranges, configuration/refusal and factory/prepare/audio/reset/
timing-query panic containment/recovery. Existing planar/event/transport/Gain
regressions pass. Default/all-feature workspace lint, default doctests, both
warning-denied Rustdoc builds and formatting/docs/whitespace pass. Actual-library
C++ entry contracts compare both interfaces over 2000 timed/cross-alias blocks,
zero-frame controls, rejected prefixes, wide f64 audio and lifecycle/timing. Six
native C/C++ contracts pass; rebuilt VST3 validator47/0 plus editor/programs,
AUv2 render/state/editor/420-block programs, LV2 binaries and AAX entry probe pass.
Local macOS arm64 evidence only; AAX remains ad-hoc/entry only, with no new DAW/
device/platform/deadline support claim. Generic parameter/state metadata and SDK
registration/editor infrastructure remain independent work; programme continues.
See [decision 0145](decisions/0145-native-processor-entry.md).


## Publication history cleanup — 9 October 2026

Replaced the publishable branch history with a fresh public-framework snapshot
and removed earlier local checkpoint refs. Original history and the complete
tracked/non-ignored working tree were backed up outside the checkout in a private
sibling directory. Existing uncommitted framework work remains uncommitted.
The private remote's main branch is updated with an explicit force-with-lease;
repository visibility remains private. Hosted cached commits and other clones
require separate review before publication; a branch rewrite is not evidence of
server-side object deletion. See [the publication guide](publication.md).


Cleanup validation: the publication checker passes across all remaining local
refs, the documentation checker passes for both the working tree and an isolated
export of the clean committed root, all 28 Python regression tests pass, and Git
object integrity checks pass. The original private root is absent from the local
object database after reflog expiry and garbage collection. The remote advertises
only the clean main branch; GitHub reports no pull requests, releases or workflow
artifacts. Hash comparison confirms every other backed-up working file is
unchanged. GitHub's commit API still resolves the old private root by SHA, so
server-side deletion is unresolved and visibility remains private. A purge
request draft with the original commit IDs is retained in the private backup.


## W138: Bounded native global schemas and state

Added an optional paired companion table with immutable stable-ID global f32
schemas, UTF-8 names/units, linear/log/discrete mapping and explicit automation/
bypass/read-only roles. Complete provider snapshots revalidate/reorder by declared
IDs before native output. Complete canonical state validates before consumer
atomic restore; explicit consumer decoding can migrate legacy records within an
8208-byte bound. State calls require exclusive stopped ownership. Gain-native uses
thin f32/f64 facades, keeping portable DSP backend-independent and retaining its
complete 32-byte-v2 and legacy 20-byte-v1 codecs.

Six groups pass native metadata/ranges/roles/UTF-8 limits, registration/version/
capacity/provider panics, mismatched/reordered snapshot IDs/ranges, native state
length/product/version/value/duplicate/order rejection and shared-owner snapshot/
restore poisoning/recovery. 1000 complete snapshot/load/save/process workloads per
precision count zero heap actions. Existing native/Gain regressions, default/all-
feature workspace lint, default doctests, both warning-denied Rustdoc builds and
formatting/docs/whitespace pass. Actual-library C++ checks pass metadata/mapping,
2000 complete recall/audio cycles, canonical codec parity, legacy migration and
atomic rejection; seven native C/C++ contracts pass. Rebuilt VST3 validator47/0
plus editor/programs, AUv2 render/state/editor/420-block programs, LV2 binary and
AAX entry-only probes pass. Local macOS arm64 evidence only; no additional DAW/
platform/deadline claim. Live publication requires explicit audio-owner contracts
and remains next, alongside generic SDK registration/scoped metadata/UI and the
wider programme. See [decision 0146](decisions/0146-native-parameter-state-companion.md).


## W139: Explicit native live global state publication

Added an opt-in prepared audio-parameter contract and independent schema-bound
atomic publication objects. Control decodes/revalidates complete state; audio
applies newly accepted revisions before process and conditionally reflects base
state afterward. Cached schemas avoid audio metadata-provider calls. Repeated
apply skips restore, own reflection is not reapplied, and newer control restores
win. One-attempt Busy/Stale preserves results without spinning. Coalescing,
retirement, exact pairing/library lifetime and callback contracts remain explicit.
Preparation/deactivation clear accepted revision; reset preserves it. Audio-provider
panics poison the shared owner until fresh preparation; decoder panics have no DSP
owner to poison. Gain-native exposes the extension for both precisions and retains
canonical/legacy decoding.

Five new Rust groups pass complete record/capacity/layout/failed-output validation,
f32/f64 apply/process/reflect revision and identity semantics, atomic rejection,
panic recovery and concurrent controller/audio/observer coherence, including
conditional audio automation reflection. Each precision's 1000 complete cycles
counts zero heap actions and no audio metadata-provider calls. All589 workspace
runtime tests/doctests pass, plus28 Python regressions; default/all workspace lint,
default doctests, both warning-denied Rustdoc builds and formatting/docs/whitespace
pass. One workspace executable temporarily stalled at macOS dyld entry before its
harness and later resumed; this was not a test failure or callback observation.

The actual-library C++ state probe passes2000 f32/f64 apply/audio/reflect cycles,
coherent snapshots/canonical bytes/legacy migration, stale newer-restore protection
and retention of Gain automation smoothing. All8 native C/C++ contracts pass.
Rebuilt VST3 validator47/0 and editor/program probes, AUv2 render/state/editor/
420-block programs, LV2 binaries and local ad-hoc AAX entry probes pass. Local
macOS arm64 contract evidence only, without additional DAW/device/platform/deadline
or PACE/Pro Tools claims. Generic SDK registration/mapping remains independent;
the programme continues. See [decision0147](decisions/0147-native-live-state-publication.md).


## W140: Reusable VST3 controller schema registration

Added an optional external-SDK C++17 header that preflights a complete bounded
native global schema before adding VST3 root-unit parameters. It rejects invalid
metadata/table/roles/defaults, duplicate/conflicting or host-reserved IDs and
oversized/malformed UTF-16 host text instead of clipping. Complete UTF-8 conversion
preserves supplementary characters. SDK parameter objects call native pure
linear/log/discrete mapping on control, retain global f32 physical values and
explicit automation/bypass/read-only roles, and keep their table/library alive.
Numeric formatting retains fractional/arbitrary stepped choices; strict bounded
locale-independent decimal/exponent parsing rejects invalid/suffixed/overflow/
nonfinite text without mutation. Explicit boolean bypass roundtrips On/Off labels.
Gain's actual controller now registers both base controls through this helper;
factory/state/processing/editor/programme behavior remains separately owned.

SDK-linked contracts pass unrelated log/fractional/Unicode/default/role metadata,
whole preflight failure/container preservation, ASCII127/supplementary127 UTF-16
boundaries, invalid numeric text/signs/whitespace and bypass text roundtrips,
conflicts/reserved IDs/table bounds, and actual native Gain f32/f64 schemas/mappings.
Both bundlers that build VST3 contracts now include the new contract target.
C++17 syntax,28 Python regressions and formatting/docs/whitespace pass. Rebuilt
VST3 schema/processing contracts, validator47/0 without the initial bypass text
roundtrip diagnostic, actual Cocoa editor/programs, AUv2 render/state/editor/
420-block programs, LV2, ad-hoc AAX entry and8 native C/C++ contracts pass. W139's
589-test workspace/lint/Rustdoc snapshot covers unchanged underlying Rust APIs.
Local macOS arm64/SDK3.8.1 evidence only; no new DAW/platform/deadline or production
AAX claim. This helper is control-only: audio-cached mapping, other SDK/factory/
state/event/editor infrastructure and the wider programme continue. See
[decision0148](decisions/0148-vst3-native-schema-registration.md).


## Prepared standalone comb filters — 9 October 2026

Added one-channel f64 feedforward and feedback comb filters with fixed whole-sample
delays, unity direct input and positive/negative coefficients. Preparation validates
1..1048576 samples before one bounded allocation; feedback requires finite
absolute coefficient below one. Coefficient edits retain histories and reject
before mutation; zero coefficient returns input exactly while advancing history.
Sample/in-place processing and reset retain allocations. Kind/delay changes require
new preparation. Existing private damped reverb comb behavior remains independent.

Six new test groups pass signed impulse timing/decay, independent indexed input/
output recurrence across coefficient edits, analytic complex frequency gains,
partition/empty/reset/zero-history continuity, atomic invalid preparation/edits
and exact maximum-delay/storage bounds. 10000 prepared process/edit/reset
workloads per kind count zero heap actions. All signal tests/doctests, signal
Clippy with warnings denied, workspace doctests and warning-denied workspace
Rustdoc pass in an isolated macOS arm64 snapshot. That snapshot excludes the
concurrently introduced ravet-embedded and embedded-gain workspace members; its
574 runtime tests/doctests pass. The checkout's six comb tests and signal Clippy
also pass using a fresh target directory. Source formatting and all 28 Python
regressions pass. Checkout documentation coverage remains dependent on the concurrent
embedded-package registration; no device/DAW/platform/deadline claim is added.
See [decision 0150](decisions/0150-prepared-comb-filters.md).

## Embedded processing foundation — 9 October 2026

Completed the no_std foundation boundary, fixed-capacity Rust audio adapter,
public allocation-free immediate stereo gain/C bridge, native Arduino C++11
stereo subset and independent embedded-rust/arduino/pi-linux starters.
See [embedded projects](embedded-projects.md) and
[decision 0152](decisions/0152-embedded-processing.md). The selected available
reference boards are ESP8266-D1 and Raspberry Pi 3 Model B+; STM32/Compute Module
models and audio codecs remain unspecified. No physical board was exercised.

Executed locally on macOS arm64 with Rust/Cargo 1.99.0:

- Foundation and embedded/reference tests pass both with default features and
  with defaults disabled; the six new Rust fixtures cover integer PCM saturation,
  interleaving/tails, complete preflight, zero-frame/instrument paths, timed
  gain/strict rejection, native C aliasing/layout and callback/reset/fault heap
  counts. Measured allocation/deallocation counts remain zero.
- `python3 scripts/check-embedded.py --cross` passes host tests, the actual host
  Rust static-library C fixture and native C++11 contracts with exceptions/RTTI
  disabled. Library checks pass for thumbv6m-none-eabi, thumbv7em-none-eabihf,
  aarch64-unknown-linux-gnu and armv7-unknown-linux-gnueabihf. A Cortex-M4/M7
  hard-float reference static archive builds with explicit abort/spinning panic
  policy. This is not final firmware-link/flash or Linux runtime evidence.
- A generated independent embedded-rust consumer library builds for
  thumbv7em-none-eabihf; a generated pi-linux consumer locks/builds/runs on this
  macOS host and prints `RAVET embedded stereo gain PASS`. Pi execution is pending.
- Generator regression tests pass for independent manifests, no-default embedded
  dependencies, local Arduino library/sketch copies and existing-directory/scope
  protection.
- Full workspace tests, Clippy with warnings denied, required workspace Rustdoc
  tests, warning-denied API build and workspace formatting check pass. All 29
  Python regression tests and the documentation checker pass.
- The Arduino OfflineGain sketch compiles/links for `esp8266:esp8266:d1` (D1 R1)
  and `esp8266:esp8266:d1_mini` (D1 R2/mini), with ESP8266 core 3.1.2 and
  Arduino CLI 1.5.2-rc.1 in isolated target-directory tooling. Firmware reports 29,188 RAM bytes, 59,747 instruction/cache RAM bytes
  and 236,116 flash code bytes including core/SDK overhead. It was not uploaded
  or executed on a board; this establishes build evidence only.

The [embedded workflow](../.github/workflows/embedded.yml) configures Ubuntu
contracts/cross checks, with remote execution pending. These checks do not
establish board/codec/DMA/cache/interrupt behavior, live Pi devices, Arduino
flashing or real-time deadlines. Existing desktop signal/graph/runtime/editor
crates are not converted into allocator-free firmware dependencies.

The embedded user guide and starters stay board-neutral for Arduino. The D1
profiles above are compile fixtures, not recommended audio targets. Select
hardware using measured DSP workload, CPU/FPU, RAM, codec interfaces and DMA/
interrupt behavior; synthetic gain build success does not establish suitability
for substantial audio applications.


## W141: Control-cached native global parameter mapping

Added an immutable control-prepared native schema cache with copied metadata,
static ranges/roles and sorted binary ID lookup, plus a separate paired foreign
table preserving earlier ABI layouts. Cached concurrent metadata/scalar reads and
complete normalized event conversion call no consumer providers or DSP and use
no heap actions, locks or I/O. Full normalized ID/value/timing/capacity validation
precedes output mutation; f64 normalized intent explicitly narrows to the global
f32 contract and physical f32 widens exactly into tagged event records. Equal-
offset order and tails survive. Creation/drop and providers stay on control, with
explicit exact pairing/library/static-reference lifetime and stopped retirement.
No hidden role filtering or unknown-ID dropping occurs. Gain exports both precision
companions.

Six Rust groups pass schema/mapping/roles/full-ID/layout checks, whole bad-prefix/
unknown/nonfinite/offset/order/capacity rejection, zero-frame/empty/oversize caches,
provider-panic isolation and shared foreign-reader concurrency. Each precision's
1000 full1024-event metadata/map/convert workloads and four readers'10000 cached
workloads count zero heap actions/provider calls. Existing native/Gain regressions
and scoped lint pass. Default/all-feature workspace lint, workspace doctests, both
warning-denied Rustdoc builds and formatting/docs/whitespace pass after refreshing
stale generated Signal metadata/archive artifacts that omitted concurrently added
public exports; shared sources were preserved. Workspace package coverage now
includes the independently added embedded reference.

Actual-library C++ checks pass both native Gain precision profiles,2000 normalized
conversions, declared order/ties/tails, whole rejection and failed scalar/metadata
outputs. All9 native C/C++ contracts pass. Rebuilt VST3 validator47/0/schema/editor/
programs, AUv2 render/state/editor/420-block programs, LV2 binary and ad-hoc AAX
entry checks pass. Local macOS arm64 evidence only; no additional DAW/platform/
deadline or PACE/Pro Tools claim. Prepared SDK queue/profile integration is next;
the broader programme continues. See [decision0151](decisions/0151-native-cached-parameter-mapping.md).


## AUv3 and mobile native adapters — 9 October 2026

Added a reusable paired f32 native session, an AUAudioUnit processor adapter,
programmatic CoreAudioKit parameter editor and public Gain extension/containing
apps. AU parameters retain physical stable-ID schema values; complete live state
includes bypass. Rendering validates bounded input before DSP, handles timed
changes and ramps that continue across blocks, caches stopped timing, maps host
transport and supports an explicit short MIDI/framed UMP input profile. Main-queue
parameter refresh keeps cached host values synchronized. Ordinary concurrent
control publication does not turn into an audio-stream failure; a raced ramp-base
snapshot retains its last observed values.

Apple mobile has explicit AVAudioEngine/AVAudioSession lifecycle, preferred
configuration, microphone permission, route/interruption/media-reset handling,
retained state and endpoint-ID MIDI reconnection. Android has AAudio prepared
planar/interleaved conversion, callback chunking, zero-timeout capture, shortage
silence/counters, atomic native failure reporting, stopped stream retirement and a
1024-slot immediate MIDI queue. Public native apps expose Audio/MIDI settings by
default and stop on backgrounding. SDKs remain external. A single build command
creates Apple bundles or Android libraries/development APKs; local fixture
signing, native alignment verification and RAVET/libm/NDK notices stay explicit.

Executed on macOS 27.0.1 arm64/Xcode 27:

- MacOS AU in-process contracts pass three lifecycle cycles, physical parameter
  schema, audio/tails, timed bypass, complete live restore, whole-list rejection,
  cross-block ramps, host bypass and public render/schedule wrapper, cached timing,
  transport conversion, opt-in short MIDI/UMP/truncated-UMP rejection, zero-frame
  calls, resource stop/reprepare and programmatic editor construction.
- Arm64 iOS device and simulator app/extension builds pass with native compiler
  warnings denied. The signed iPhone 17 Pro/iOS 26.0 simulator app installs; its
  explicit in-process AU contract runner passes. A normally launched app shows
  readable gain/bypass/Start/Stop and Audio/MIDI controls in an inspected screenshot.
  This evidence exercises an app process rather than AU extension discovery/IPC.
- Android arm64/API 26 native code cross-builds with NDK r29 (29.0.14206865),
  native warnings denied. JDK 17/API 36 Java compiles with all warnings denied.
  Direct SDK/build-tools 36.0.0 packaging produces a development APK with verified
  signature and 16 KiB ZIP/native-library alignment. No Android runtime was run.
- Scripted AAudio-double contracts pass oversized callback chunking/prefix/tails,
  live controls and invalid recall, bounded MIDI fill/overflow/delivery, short
  capture silence/counting, stream errors, rate mismatch/open/capture failure,
  repeated stop/recovery and balanced ownership. 1000 callbacks race 10000 control
  edits without stopping audio. These are ownership/logic tests, not hardware or
  AAudio deadline evidence.
- 46 targeted native/Gain Rust tests and 29 Python script/checker tests pass.
  Workspace Rustdoc examples pass (13) with a fresh isolated target directory;
  shared-target attempts had linked stale signal exports and failed the new comb/
  waveshaper examples despite their current exports, so the clean-target evidence
  is recorded separately. Warning-denied workspace Rustdoc build, formatting,
  Python compilation, documentation catalogue/link checks and whitespace pass.

The mobile CI workflow is configured, not an executed GitHub platform result.
Full support remains open: named third-party AUv3 hosts/IPC, physical mobile
routes/devices/interruption/MIDI, Android execution, production provisioning and
callback measurements need actual evidence. Auxiliary buses, generated output,
factory programmes, SysEx, f64 selection, a native instrument reference, persisted
standalone sessions and background audio remain further profiles. Android duplex
requires matched rates/channels and has no independent-clock conversion. See
[the guide](mobile.md) and [decision 0153](decisions/0153-auv3-mobile-native-adapters.md).

Follow-up checks in the same work package: canonical native recall wins stale
AU superclass parameter caches; the regression passes on macOS and the iOS
simulator. All AU adapter/editor sources compile with application-extension
restrictions enabled. Apple and Android standalone controls use scrolling layouts
for smaller viewports; final Apple device/simulator builds and Android APK
packaging pass, and the updated iPhone UI screenshot was inspected. Extension
metadata includes the factory class and display name required by the installed
Xcode template/simulator installer. Hardware/host acceptance targets were requested
from the owner; the full-support gate remains open.


## W142: Prepared VST3 cached global parameter input

Added an optional external-SDK C++17 input profile with control-created native
caches, sorted IDs/roles, explicit read-only policy and distinct mapping/routing
results. Gain's actual processor uses separate f32/f64 profiles. A reusable walker
reads each global SDK point once into <=16KiB stack scratch and validates all raw
queue/total counts, getter results, finite normalized values and ordered offsets,
including unknown IDs, before any prepared visitor runs. Program expansion and
stable bounded insertion remain consumer-owned and unchanged. Visitor failure may
retain private scratch, so pending state/DSP/output publication waits for success.
Cached mapping/validation use no provider calls, framework heap actions, locks or
I/O; host and visitor obligations remain explicit.

SDK-linked normal/fast-math targets pass actual native Gain precision profiles,
explicit read-only routing/failed output preservation, whole malformed/unknown/
count/getter/nonfinite/timing/capacity preflight before visitors, zero-frame/full
capacity, single point reads, retained traversal/ties and visitor failure scope.
1000 cached queue workloads count zero C++ new/delete and metadata reads; W141's
Rust cache workloads establish the separate Rust heap/provider evidence. Tests
inject foreign NaN/infinity bytes and classify referenced IEEE memory as integer
bits, avoiding invalid by-value floating assumptions under fast-math.

Both VST3 contract bundlers build both input targets. Rebuilt four SDK contracts,
validator47/0, actual Cocoa editor/programs, AUv2 render/state/editor/420-block
programs, LV2 binaries, ad-hoc AAX entry and9 native C/C++ contracts pass. Syntax,
formatting/docs/whitespace pass; W141's current workspace lint/doctest/Rustdoc
snapshot covers unchanged underlying Rust. Local macOS arm64/SDK3.8.1 evidence
only; no new DAW/platform/deadline or production AAX claim. Continuous host curves,
gestures/scoped/performance/transport, generic SDK factories/state/processing/
output/editors and the wider programme remain active. See
[decision0154](decisions/0154-vst3-cached-parameter-input.md).


## W143: Explicit native audio-owner complete state

Added a separate cache-paired companion for complete physical f32 base-state
restore/snapshot on exclusively prepared generic processors. All IDs/ranges and
completeness validate before one atomic consumer restore; complete snapshots
revalidate/reorder before writes, preserving failed fields and successful tails.
No byte codec/schema/role provider runs in framework audio paths. Audio hooks are
now available without a 64-bit atomic gate; live publication remains separately
gated. Direct calls do not invent generations or modify accepted live revisions;
consumer protocol ordering and needed-recall scheduling remain explicit.

Five Rust groups pass unrelated/full-u32 identities, reordered provider state,
malformed/capacity/null/alignment/atomic rejection, mismatched provider IDs/ranges,
provider failure isolation, shared poison/reset/reprepare, zero-frame automation,
empty/max1024 complete state and 1000 heap/provider-free cycles per precision.
Actual Gain C++2000 restore/render/snapshot cycles pass wide f64 audio, whole-state
rejection, zero-frame automation, retained smoothing, lifecycle and failed/tail
outputs. C11/C++17 layouts pass. See
[decision 0155](decisions/0155-native-direct-audio-state.md).

## W144: Explicit prepared native telemetry

Added independent stable-ID native readout descriptors/caches and a paired generic
processor companion. Control metadata preflights UTF-8/count/duplicate/display
bounds before caching. Complete exclusive prepared snapshots validate all IDs,
finite raw f64 values and known nonfinite-input flags before reordered writes.
Exact u64 windows and extreme/tiny/signed-zero f64 values survive; display hints
do not clip observations. Metadata reads are shared/provider-free; consumer
measurement, retention/reset and UI/host publication remain explicit.

Seven Rust groups pass metadata limits/preallocation rejection, full identities,
reordering/malformed/unknown/duplicate/nonfinite/flag/pointer/capacity failures,
provider isolation, shared poison/reprepare and concurrent cached readers. Both
precisions run1000 heap/provider-free cycles; max1024 snapshots run1000 heap-free
cycles, four shared readers run10000 cycles each, and empty/null/tail boundaries
pass. Gain exposes existing12 meter fields10..21. Actual C++2000 stereo cycles
pass raw wide/tiny/RMS levels, exact counts/nonfinite diagnostics, mono/reset/
zero/rejected retention, failed fields and tails. C11/C++17 layouts pass.

The combined snapshot passes626 all-feature workspace tests/doctests. A test-only
Clippy initializer warning was corrected; refreshed telemetry, default/all-feature
workspace lint, default doctests and both warning-denied Rustdoc builds pass.
Formatting/docs/whitespace pass. Rebuilt four SDK contracts, VST3 validator47/0,
Cocoa editor/program contracts, AUv2 rendering/state/editor/420-block programs,
LV2 binaries, ad-hoc AAX entry and11 native C/C++ contracts pass. The first two
new linked tests assumed unique table addresses; equivalent promoted Rust tables
had different addresses with matching callbacks. Corrected callback-profile checks
pass; table address is not owner/type identity. The prepared C++ client draft also
passes2000 complete actual-library cycles and zero C++ new/delete, with invalid
profiles/optional capabilities/factory cleanup covered; actual SDK migration is
next. Local macOS arm64 evidence only; no new DAW/device/deadline/platform or
production AAX signing claim. Wider programme remains active. See
[decision 0156](decisions/0156-native-prepared-telemetry.md).


## W145: Reusable native C++ client and SDK reference adoption

Added an SDK-independent C++17 owner composing exact native lifecycle and optional
complete audio state/separate telemetry caches. Control acquisition/retirement,
exclusive DSP callbacks, shared cached metadata, optional capability queries,
invalid-profile rejection and partial-factory cleanup remain explicit. Promoted
tables may differ in address; supported bindings match callback profiles, with
exact types/library/foreign ranges still caller obligations. Normal forwarding
uses no C++ heap/locks/I/O; native result/silence/poison and consumer duties remain.

Gain's actual VST3 processor now consumes two precision-matched clients and tagged
events. Preparation marks complete state pending; the first wholly validated
callback, including zero-frame flush, restores the current publication. Complete
base snapshots feed the existing generation/CAS protocol, preserving newer
controller restores and ongoing smoothing. Generic telemetry preserves meter IDs,
normalization and host/editor delivery. Programs/controller codec/editor policy
and legacy public C exports remain consumer-owned; no transport or performance
mapping was added in this package.

Actual-library C++2000 complete callback cycles pass optional capabilities,
invalid/mismatched profiles, equivalent promoted tables, differing callback
rejection, factory cleanup, wide audio/tails/lifecycle and zero C++ new/delete.
SDK contracts add malformed unknown queue preflight, first valid/same-precision
reprepare recall, zero-frame recall/automation and unchanged-publication retained
ramps at both precisions. All four SDK contracts, validator47/0, actual Cocoa
editor/programs, AUv2 render/state/editor/420-block programs, LV2, ad-hoc AAX entry
and12 native C/C++ contracts pass. Syntax/docs/format/whitespace pass; W144's626
workspace tests/doctests/lint/Rustdoc cover unchanged Rust. Local macOS arm64 only,
no new DAW/device/deadline/platform or production AAX signing claim. Transport,
performance, generic factories/buses/state/editors and wider programme remain
active. See [decision 0157](decisions/0157-native-cpp-client.md).


## W146: Explicit VST3 block-start transport mapping

Added an external-SDK C++17 mapper and immutable context requirements. Null context
means canonical unknown transport; non-null context preserves exact i64 samples,
known playing/recording/loop state and available finite f64 music/positive tempo/
u16 signature. Supported scalar preflight preserves failed output. Advertised
cycles are finite, ordered when active, and map only with active loop/known music;
inactive/positionless bounds are omitted with loop state retained. Bar start has
no exact SDK bar number, so no native pair is guessed. Further SDK context fields,
discontinuity, pre-roll and missing availability remain unrepresented explicitly.

Shared reference-based IEEE classification now lives in an SDK-independent header,
retaining cached parameter-input finite checks. Normal/fast-math fixtures pass1024
availability profiles, exact i64 extrema/f64 signed zero/u16 bounds, null/partial/
unavailable malformed fields, complete invalid scalar rejection and10000 mappings
with zero C++ new/delete. SDK processor contracts exercise requested flags and
malformed context before pending restore/automation/DSP, then valid context at both
precisions. Gain forwards canonical context to its generic owner but ignores
musical timing; no transport-driven synthesis or DAW timeline claim follows.

Both bundlers build both mapper targets. All six SDK contracts, validator47/0,
actual VST3 Cocoa editor/programs, AUv2 render/state/editor/420-block programs, LV2,
ad-hoc AAX entry and12 native C/C++ contracts pass. All29 Python regressions and
syntax/docs/format/whitespace pass; W144's626 workspace snapshot covers unchanged
Rust. Local macOS arm64/SDK3.8.1 evidence only; no new device/deadline/platform or
production signing claim. Performance/scoped events, generic factories/buses/
state/editor and wider programme remain active. See
[decision 0158](decisions/0158-vst3-block-start-transport.md).


### Consumer-selected mobile deployment targets — 9 October 2026

The consumer chooses physical iPhone/iPad and Android models, OS versions,
audio/MIDI routes and AUv3 host/version acceptance targets; a fixed project-wide
hardware allowlist is not required. The mobile guide now describes the consumer
validation matrix. This clarifies the earlier request for named release targets;
selection alone adds no runtime support evidence. Existing app Audio/MIDI choices
remain the runtime selection mechanism.

The fixture builder now accepts Apple deployment minimums and Android minimum
API, compile/target SDK and installed build-tools. Adapter API minimums are
validated before build commands. Apple Rust/CMake receive the same OS minimum;
Android compiler/CMake/DEX/manifest receive the same minimum API. The optional
Gradle fixture also accepts minimum/compile/target SDK properties. Defaults and
arm64 architecture profiles remain unchanged.

On the existing macOS arm64/Xcode/NDK r29 environment, an iOS simulator build
with deployment target 18.0 passed compilation and local signing. An Android
API-28/compile-SDK-36/target-SDK-35 build passed native/Java compilation, APK
signing and 16-KiB alignment checks; `aapt2 dump badging` confirmed minimum 28
and target 35, and CMake recorded API 28. Four configuration regression tests
cover selectable versions, defaults and incompatible minimum/SDK combinations.
The Python script suite passed (33 tests). No physical device or third-party
AUv3 host was exercised by this work; optional Gradle properties were not run.
Workspace Rustdoc tests and API documentation with warnings denied also passed
using the isolated `target/mobile/doc-check` directory. The documentation checker
passed after regenerating the feature catalogue.


## W147: Exact portable mute expression and explicit CLAP limits

Portable volume now accepts finite 0..=4, retaining both zeros and positive
subnormals without clamping; zero mutes without implying release/completion.
Native v1 layouts remain unchanged. Hosted/exported CLAP retains its independent
strictly positive range, rejecting selected zero input before DSP and complete
unrepresentable generated streams before host writes while retaining DSP/audio.
Native scope/bit round trips, worker whole-batch handoff and CLAP filtering/input/
output boundaries pass. See [decision0159](decisions/0159-zero-volume-note-expression.md).

## W148: Reusable VST3 performance input

The optional SDK C++17 mapper preserves unquantized velocities, full signed-ID
bits except the absent -1 sentinel, note cents expansion, poly pressure and six
standard expressions. Explicit unknown-expression policy and complete raw/list/
expanded capacity/offset preflight precede private visitors. Fixed scratch reads
getters once and retains tie order; unsupported variable payloads are rejected.
Normal/fast-math fixtures include10000 full-list cycles with zero C++ heap actions.
Gain declares no performance input. See
[decision0160](decisions/0160-vst3-performance-input.md).

## W149: Reusable VST3 performance output

Whole canonical-stream representability precedes host calls. Exact velocity,
identity and normalized expression fields survive; standard expressions require
ID-only scope. Tuning unit conversion may round. Integer IEEE volume checks
reject subnormal division loss even under fast-math. Unsupported wildcard/choke/
completion/payload semantics are not substituted. Mutable host events cannot
alter native sources; refusal reports an exact accepted prefix without replay.
Normal/fast fixtures pass1000 maximum-capacity zero-heap cycles and every refusal
prefix. Gain declares no performance output. See
[decision0161](decisions/0161-vst3-performance-output.md).

## W150: Prepared native VST3 audio mapping and refreshed host evidence

A reusable control-prepared profile maps up to 9 logical buses/64 channels per
direction to independent compact SDK directions at f32/f64 precision. Complete
shape/pointer/alignment/overflow/output-overlap validation precedes DSP/state;
input/output aliases remain allowed. Zero-frame omission is explicit. Failed
configuration/output mappings retain prior values. Fault cleanup clears valid
active prefixes only at either supported SDK precision, including a rejected
processing-precision mismatch, preserving tails and counting cleared channels.
Gain now adopts this mapper while retaining mono/stereo effect declarations.
Normal/fast fixtures cover10000 ordinary maps and1000 maximum576-channel maps
without C++ heap actions. Actual SDK overlap rejection preserves pending state;
corrected callbacks recover. The first rebuilt format run exposed overly narrow
precision-mismatch cleanup; the corrected helper and refreshed contracts pass.
See [decision0162](decisions/0162-vst3-native-audio-buses.md).

The combined W147–W150 snapshot passes628 all-feature workspace tests/doctests,
default/all-feature Clippy, default doctests, both warning-denied Rustdoc builds,
formatting/docs/whitespace and29 Python tests. Rebuilt CLAP smoke/validator passes
44 checks (34pass,10skip,0fail/warn). Refreshed format gates pass12 SDK contracts,
VST3 validator47/0, three Cocoa editor modes/programs36 blocks/six cycles, AUv2
render/state/editor/programs420 blocks, LV2, ad-hoc AAX entry and12 native C/C++
contracts. These are local macOS arm64/SDK3.8.1 observations, not production AAX
signing, generic instrument export, other-platform or callback-deadline proof.

A fresh independently identified disposable REAPER7.82/macOS arm64 instance loaded
the rebuilt VST3 binary on macOS27.0.1, with BlackHole16ch at48kHz/512 frames.
The public guarded synthetic-input harness passed discovery, all12 normalized
meter readings, gain/bypass IDs and native floating-window creation. It muted
hardware destinations, stopped playback and closed the FX window; the stopped
fixture was inspected afterward. The harness now logs its enumeration count and
only public Gain candidates. A prior process retained a stale catalogue; a fresh
process using the committed arm64 VST path resolved it. No visual/accessibility,
performance-event/multibus instrument, physical output or deadline claim follows.
Evidence remains in ignored target/reaper-w150/config/ravet-validation/native-meter-host.log.
The wider programme remains active; portable identified performance voices and
actual instrument adoption are next.


## W151: Explicit normalized performance voice ownership

Added an independent fixed 1..128-slot receiver with full optional u32 identities,
unquantized f32 velocities and seven exact f64 expressions. Conjunctive targets,
same-address ID retrigger, atomic address-conflict rejection, stacked unidentified
notes, release-tail expression updates, bounded chronological stealing and original
target completion are explicit. Complete timed semantic preflight simulates on a
fixed copied bank without changing live state or reserving output implicitly.

Six groups pass every target scope, full identities, signed zeros/subnormals,
defaults/mute/ranges, malformed atomic rejection, release/retrigger/steal/choke/finish
policy, independent chronological oracle, late timed conflicts and zero/max limits.
10000 maximum-bank control workloads and1000 full1024-event preflights count no
Rust heap actions. Existing MIDI/MPE policy is preserved. See
[decision0163](decisions/0163-native-performance-voice-ownership.md).

## W152: Transactional f32/f64 performance sine consumer

Added an independent native-precision mono/stereo reference using this ownership
policy, timed ADSR, linear volume/expression, explicit balance, tuning and vibrato.
Pressure/brightness remain exact readable state without timbre shaping. Whole
generic validation precedes private scratch rendering with fixed copied DSP/voice
state; only success commits live samples/completions/history. Late semantic or
completion overflow faults preserve phase, envelopes, voices and level. Actual
in-block envelope completion permits later address reuse of that ID. Positive
output capacity enables original-target completion; zero suppresses notifications
while internal lifetime still ends. Preparation owns bounded scratch allocation.

Eight precision-specific groups pass analytic unquantized attack/audio, exact
partition equality, timed mute/unmute/balance, zero-frame/-zero ownership, release
completion/order/tails, malformed/identity/output-overflow atomicity and continued
history, actual in-block ID reuse, notification suppression, extreme rates and
prepare/reset/deactivate/state limits. Each precision runs100 maximum128-voice
render/release/completion cycles with zero Rust heap actions. See
[decision0164](decisions/0164-transactional-performance-sine.md).

The combined snapshot passes642 all-feature workspace tests/doctests, instrument
Clippy and all-feature workspace Clippy, default workspace doctests, both warning-
denied Rustdoc builds,33 Python tests and format/docs/whitespace checks. A macOS
loader delay occurred before test code on the workspace volume; sampling showed
only dyld startup. Identical checksum-verified public test executables ran normally
from task-local temporary storage through Cargo's supported runner, including
doctests. No source, security setting or repository runner default changed. These
are local macOS arm64 offline observations, with no new native facade/SDK export,
DAW, physical device, other-platform or callback-deadline claim. The wider programme
continues with a public native instrument reference and actual SDK integration.


## W153: Public native performance reference — completed locally

A separate public 16-voice f32/f64 archive exposes consumer-specific native
processor, schema, cache, direct-audio-state and optional publication entries.
Canonical level state retains the public RAVETPS product identity and does not
serialize voice history. The actual linked C++ fixture passes both precisions,
codec identity and atomic malformed-state rejection, exact performance ownership,
analytic rendering, mute/release/completion, tails and lifecycle. Its 2000 prepared
workload cycles count no C++ new/delete actions. Workspace default Clippy,
doctests, both warning-denied Rustdoc builds, formatting and documentation checks
pass. Execution uses the checksum-verified local runner described in W152.

See [decision 0165](decisions/0165-public-native-performance-reference.md) and the
[native guide](native-processing.md). This archive has no SDK instrument factory,
controller, native editor or DAW validation. SDK instrument adoption is next;
these local results do not establish physical-device or deadline support.


## W154: Public VST3 performance instrument — SDK and offline DAW validation passed

The public performance archive now has a distinct external-SDK processor and
controller: no audio input, mono/stereo f32/f64 output, one 16-channel event input,
Level schema/state and five audible note-expression declarations. Recall shares
the transactional event stream with automation and performance input, preserving
voice progress and pending state after late faults. Completion is suppressed
explicitly because VST3 has no native NoteEnd equivalent. No custom editor is
declared; the host can provide generic controls.

The linked actual-archive SDK contracts pass 1000 prepared cycles across both
precisions/layouts with no C++ heap actions, analytic audio, exact velocity and
signed IDs, cents/absolute expression order, mute/release/ID reuse, exact phase
and envelope preservation after late faults, canonical state/text rejection,
controller connection recall and the shared 1024-event capacity. The dynamically
loaded bundle passes all 47 SDK validator tests, zero failures, on the local
macOS arm64/Xcode toolchain. Strict C++ syntax and formatting checks, four DAW
fixture/guard regressions and the documentation checker pass. The strict fixture
command suppresses an external SDK final-class virtual-destructor warning;
framework warnings remain errors. Rust processing is unchanged from W152/W153's
workspace validation.

A new isolated REAPER test copy initially scanned unrelated system plug-ins.
After cancellation, the guarded script rejected discovery before inserting tracks
or playing audio; no DAW evidence follows from that failed run. Startup notice
automation returned an unchanged evaluation window. Only the owned test process
was restarted with its explicit public plug-in path; validation was still pending
at that point.
No production project, licence, physical route or global audio service was used.
See [decision 0166](decisions/0166-vst3-performance-instrument.md).

The subsequent owned instance discovered and instantiated the actual public
VST3 instrument. Its live transport-region check stalled and supplied no live
audio proof. A guarded offline apply-FX fixture then passed three stereo 48 kHz
DAW renders for Level .8, saved/restored .8 and held .4/.8 automation. Independent
PCM checks pass peak/RMS (.04/.028284 and .02/.014142), A4 frequency, channel
agreement and pre/post-note silence in twelve interior regions. `lsof` confirms
the prepared public VST3 binary loaded in the explicit isolated REAPER 7.82
process; the script verifies muted hardware destinations and stopped transport.
No native editor, host note-expression editing, live device/deadline or other
host/platform claim follows. W154's implementation/SDK/offline-host milestone
is complete; remaining format/framework work continues independently.


## W155: Reusable LV2 native control/state — completed locally

Gain now adopts a reusable native f32 processor/cache/publication profile and
portable LV2 state extension. The actual module passes existing stereo/mono
audio/lifecycle/alias contracts plus canonical state/default/malformed recalls,
restore versus host control precedence, atomic control/audio faults, optional
URID map and host refusal. 4000 prepared run/save cycles count no C++ heap or
URID-map calls; 20000 audio blocks overlap coherent state saves. Root review
caught invalid-constructor channel indexing; guarded connect/run and direct
invalid-profile fixtures now preserve sentinels for zero/oversized channels and
missing table callbacks. Misalignment and oversized callbacks preserve tails.
Strict C++17 warning checks pass against external LV2 1.18.10 on macOS27 ARM.
No Atom/MIDI/UI/f64/auxiliary, DAW/Linux-runtime or hardRT claim follows. See
[decision0167](decisions/0167-lv2-native-schema-state.md).

## W156: Verified indexed native-FLAC seeks — completed locally

Explicit CompressedReader::new_indexed_flac verifies a known-duration native
FLAC stream once and retains compressed packets rather than complete PCM. Exact
seeks binary-search packet offsets and decode one containing packet. The default
FLAC/MP3/Vorbis replay policy is unchanged. Existing stopped CompressedReadAhead
generation replacement accepts the indexed reader and retains failed seeks.

Six existing compressed regressions, four new integration groups and the new
actual-decoder-call unit test pass, along with package all-feature Clippy. Tests
use independent PCM16/24 oracles across1/2/8 channels, packet boundaries/EOF,
malformed digest/count/truncation, failed seeks/tails and stopped queue generations.
Instrumentation proves one decode per arbitrary seek in a shared33-packet index
and zero at EOF/invalid targets. Input/payload/index/frame limits, upfront decode
cost and worker allocations are explicit. No lossy/Ogg/file-backed indexed seek,
physical clock/recording or deadline claim follows. See
[decision0168](decisions/0168-indexed-flac-worker-seeks.md).

## W157: Bounded composite focus and piano ownership — completed locally

Reusable RovingFocus manages1..256 fixed members with orientation/RTL/wrap,
Home/End, disabled/hidden/inert filtering, explicit refresh and original-tabindex
restoration. Piano now has one Tab stop and chromatic arrow navigation, separate
Space/Enter ownership and exact note release on focus moves, hiding/removal,
disabling and teardown. Observer/listener lifetime is explicit.

Thirty focused checks initially passed in collaborative Chromium and hidden native WebKit;
the surface crate's15 all-feature tests/doctest pass. Actual T3 Tab/ArrowRight/Tab
traverses first member, second member and following control; piano navigation
moves MIDI60to61 and Enter emits one matching original-source on/off pair.
Chromium document.hasFocus remains false; this is DOM keyboard/contract evidence,
not visual/native focus or assistive-technology validation. An initial WebKit
relative-script fixture was blocked by local-file read restrictions; inline
fixture assembly matching production embedding then passed. No screen-reader,
other-platform or device claim. See
[decision0169](decisions/0169-bounded-web-focus.md).


### Shared W155–W157 verification

The fixed source snapshot passes647 all-feature workspace tests/doctests,
all-feature all-target workspace Clippy with warnings denied, default workspace
doctests, both default/all-feature warning-denied Rustdoc builds, formatting,
33 script regressions, four DAW fixture/evidence regressions, catalogue/link
checks and whitespace checks. Cargo was serialized across agents with two
compiler jobs and the checksum-identical local test runner. No redundant native
host/device/platform claims follow from these workspace checks. Next packages
resume after this accepted snapshot.

Subsequent W157 review hardened iframe ownership: node creation, hit testing and
keyboard/blur/visibility listeners now bind to the piano root's owner document
and live window. Constructors reject missing/closed windows before replacing
markup or tab state. The expanded38 Chromium and38 hidden WebKit checks pass,
including parent-versus-child event isolation and exact-source release/teardown.
Embedding/frame lifetime remains consumer-owned. Updated Rustdoc/workspace
checks are grouped with the next parallel packages.


## W159: Automatic read-ahead and buffered playback — completed locally

W159 — automatic read-ahead ownership and buffered playback completed.

ReadAheadWorker<P>/ReadAheadPump and prepared BufferedPlayback now integrate
existing WAV and replay/indexed compressed read-ahead sources. Explicit start
runs an automatic bounded worker with retained suffix/backpressure and 2 ms
polling park timeout. Callback play/pause/render uses prepared ring storage and
atomics; source cursor advances only for consumed frames; running underruns stall
source position and silence suffixes; paused callbacks consume nothing. Verified
EOF requires full pending publication and queue drain. Control seek/stop joins,
positions and restarts; foreign/failed seeks preserve original generations.
shutdown returns owned pump plus original pump error; panic cannot report EOF.

Executed on local macOS 27 ARM64:
- python3 target/programme-command-runner.py cargo test -p ravet-audio-file --all-features --test automatic --test compressed_stream --test compressed_buffered --test compressed_indexed --test buffered --locked: 21 passed (7 new automatic groups; 14 scoped regression groups).
- python3 target/programme-command-runner.py cargo clippy -p ravet-audio-file --all-features --all-targets --locked -- -D warnings: passed.
- scoped fmt passed; joint workspace doctests/warning-denied Rustdoc/docs remain root-owned.

Tests cover concurrent retained suffixes, 1000 heap-free paused callbacks and active
heap-free consumption, exact pause/resume/EOF drain, gated true underrun/cursor stall,
malformed buffer atomic rejection, failed/foreign stopped generations, successful
stop/seek, source reclamation while ring-full, pump error prefix/original-error/source
ownership, successful fault recovery, panic distinct from EOF, exact whole indexed
FLAC playback/recall and unchanged NaN/signed-zero/infinite/subnormal WAV float bits.

Limits: explicit source-rate/channel playback and start; control-only join/seek/drop,
cooperative stop cannot cancel backend I/O or guarantee deadline. No clock alignment,
resampling, streamed loop/live seek/mailbox/crossfade/device/physical recording claim.
Thread creation failure retains pump; restart failure cannot undo a completed seek.
Panicked pump ownership is lost during worker unwind and shutdown returns Poisoned.
Custom ReadAheadPump implementers own bounded pumping/matching/seek contracts.

## W161: Distinct CLAP validator example names — completed locally

Sine and sidechain now explicitly name their Cargo examples validate-sine and
validate-sidechain, preserving the validator source paths. Commands and runtime
workflow follow those names; workspace builds no longer infer colliding validate
artifacts. Cargo metadata confirms exactly one distinct example per package.
Both renamed commands pass against the existing public built CLAP bundles:
sine MIDI/state/audio acceptance passes3024 blocks over three activation cycles;
sidechain native f32/f64 port/audio/fault/heap acceptance passes6006 blocks over
three activation cycles. Catalogue regeneration and documentation checking pass.
No newly built bundle, DAW/platform or device claim follows from this command fix.


## W158: Packaged LV2 MIDI performance instrument — completed locally

The public native performance archive now has a zero-input mono/stereo f32 LV2
instrument with ordered bounded Atom MIDI notes, generic Level control and
canonical complete recall. Actual-module contracts cover analytic pitch/velocity/
timing, exact splits, late-fault voice-history rollback, pending restore,
channel-scoped release, full event budget and2000 prepared heap/URID-free cycles.
The explicit --plugin performance bundler produces a self-contained .lv2 directory
with metadata, module and licence notices. Both Performance and original Gain
packaged-module contracts pass; three Python boundary tests pass. Shared build/
package/verification mechanics reside in scripts/lv2_bundle.py. No host install,
LV2 DAW/Linux/device/deadline claim. See decision0170.

## W160: Bounded shaped paragraph layout — completed locally

ParagraphLayout combines explicit width/line spacing/physical alignment with
Unicode line opportunities and retained paragraph bidi context across soft wraps.
Original UTF8 content/separator ranges and glyph clusters remain available;
TrueType SVG paragraph outlines roll back complete output on late faults.
Six new paragraph groups and21 surface tests/doctests plus package lint pass.
The collaborative Chromium preview renders joined Arabic/Latin paragraphs with
192 finite outline paths inside the720x420 source bounds; no SVG text nodes.
Unicode15 line breaking and Unicode16 bidi, explicit font/style, greedy wrapping,
unbreakable overflow, whitespace and byte/line/run/glyph limits are documented.
No implicit fallback/dictionary/justification/caret/native GPU/accessibility or
other-platform claim. See decision0172.

## W162: AIFF prepared worker handoff — completed locally

W162 — AIFF PCM read-ahead/recording worker adapters completed.

AiffReadAhead/AiffRecorder reuse extracted private PCM handoff mechanics with
unchanged public WAV buffering/recording semantics. AIFF read-ahead also implements
ReadAheadPump, so automatic worker ownership and buffered playback accepts AIFF.
Matching scratch/channel bounds, retained suffixes, stopped-generation clearing,
retry-preserving reads, dequeue poison and one-ring-capacity final drains remain
explicit. Stream PCM conversion/count/FORM/padding/flush rules are unchanged.

Executed locally on macOS27 ARM64:
- python3 target/programme-command-runner.py cargo test -p ravet-audio-file --all-features --test aiff_buffered --test aiff --test buffered --test automatic --locked: 19 passed (5 new AIFF groups + 14 stream/WAV/automatic regression groups).
- python3 target/programme-command-runner.py cargo clippy -p ravet-audio-file --all-features --all-targets --locked -- -D warnings: passed.
- package fmt passed; root owns grouped workspace doctest/docs/registry/diff gates.

Exact tests cover PCM16/24/32 at 1/2/8 channels, backpressure suffix/seek generations,
reader I/O failure retry, allocation-free callback transfer, bounded final recorder
drains with mono PCM24 odd padding, dequeued nonfinite/partial-write poison and
actual automatic AIFF playback/stopped recall. Existing WAV/automatic fixtures
pass after extraction. No AIFF-C/new widths/metadata/markers/resampling/device/path/
physical clock/recording/deadline claim added.


### Shared W158–W162 verification

The accepted source snapshot passes 665 all-feature workspace tests/doctests,
all-target all-feature Clippy with warnings denied, default workspace doctests,
default/all-feature warning-denied Rustdoc, formatting, 39 script regressions,
four DAW evidence regressions, catalogue/link checks and whitespace checks.
Cargo was serialized with two compiler workers and checksum-identical local
executable launching. Later W163–W167 work begins after this accepted snapshot;
its evidence is recorded separately. No new DAW/device/platform support follows.


## W165: Explicit VST3 reference bundle profiles — completed locally

The public bundler now accepts --plugin performance while preserving default
Gain behavior and its existing paths/options. Build/package/validation mechanics
are shared in scripts/vst3_bundle.py. Performance uses its own build root and
contract target; Gain-only editor/program switches reject before mutation.
Selected generated bundles receive public notices, local signing and dynamic SDK
validation; unrelated stale outputs are neither signed nor loaded.

The complete Performance pipeline passes its archive contract and 47 SDK tests;
the original default Gain pipeline passes all 12 CTest contracts and 47 SDK tests.
Three new Python profile/rejection/error-propagation boundary groups pass and were
included in the accepted shared regression snapshot. Local execution used external
SDK 3.8.1 on macOS27 ARM64 and checksum-identical test launching for the known
volume startup delay. No host installation, production signing, native editor/
program revalidation, publication or other-platform claim. See decision0176.


## W167: Explicit local-video worker extraction — completed locally

The new separate ravet-media prototype opens bounded explicitly selected local
video and extracts first-track timestamped complete RGBA frames through a narrow
AVFoundation boundary. Native assets stay on their creation worker/UI thread;
returned data is owned and no window/audio/device/network URL/playback clock is
selected. Paths/files/duration/dimensions and complete frame output are bounded;
native decoder transient allocations/I/O/time remain OS-owned and unbounded.

Three actual H.264/MP4 integration groups plus the thread-transfer compile-fail
doctest pass on macOS27 ARM64. They check independent size/duration/source times,
out-of-order frames, red/blue regions and top-white-strip orientation, malformed/
missing/empty/oversized/non-UTF8 paths, rejected timestamps and twelve worker
create/extract/drop cycles. A native draft probe exposed an inverted raster row
transform; the final renderer removes it and the independent orientation test
passes. Lossy/native color conversion tolerance is explicit. All-target package
Clippy with warnings denied passes. Shared workspace/API docs are grouped with
the next accepted parallel snapshot. No general codec/colorimetric/AV sync/live
playback/GPU-window/other-platform support follows. See decision0178.


## W164: Prepared paragraph source carets and selections — completed locally

CaretMap derives Unicode extended-grapheme byte boundaries, bidi/wrap affinity
positions, nearest-line/pen hit tests and complete caller-buffer visual selection
boxes from immutable paragraph source/layout identity. Lookup/hit/selection avoid
heap actions after preparation; invalid ranges/capacity preserve output slots.
Ligature interiors use explicit equal-grapheme interpolation; unsupported backend
clusters reject. Unicode17 graphemes remain separate from Unicode15 breaks and
Unicode16 bidi. Hard separators have no selection pen width.

Six focused groups and all 27 surface tests/doctest pass, plus package all-feature
all-target Clippy with warnings denied. The first-party generated ffi fixture
regenerates byte-identically and shapes as one glyph; independent caret positions
are 0/24/48/72 at size72. Tests include RTL combining, bidi/wrap affinity,
CRLF/empty lines, invisible ties/zero-width selections, extreme finite coordinates,
full source capacity and 3000 heap-free prepared operation iterations. Collaborative
Chromium renders 73 finite paths in720x320 with distinct byte3 affinities and two
disjoint bidi selection boxes. No interactive editor/IME/clipboard/native caret,
screen-reader, font GDEF/ink precision or complete Unicode support claim. Root
review found no new source-mapping/selection issue. See decision0175.


## W163: AUv2 native performance MusicDevice reference — completed locally

Implemented separate public aumu/RvPS/Ravt component, native performance archive plus embedded VST3, zero input/mono-stereo f32. Owned SDK subclass/factory provides absent-identity MIDI channel/key release; fractional StartNote tuning and unquantized float velocity with nonwrapping opaque group/key IDs; bounded expanded queue and timed Level; malformed canonical state and AU dictionary preflight before SDK mutation; coherent processor-authoritative saved streams; caller-supported silence/tails on rejected render. Explicit maximum-frame factory-selector preflight is necessary because AUBase bypasses the virtual property setter. Invalid formats/capacities reject before configuration; null property outputs are guarded. Full-width UInt32 MIDI words are preflighted before SDK byte narrowing; malformed status/data, undefined system statuses and nonzero unused arguments reject while retaining queued notes. Defined non-note short channel/system messages are ignored; SysEx start/end are unsupported. SDK files remain external.

Shared au_bundle.py reuses root vst3_bundle runtime; --plugin performance produces a self-contained public component including embedded module and applicable notices. Gain stays default and retains editor/program flags. No global host installation or distribution signing.

Executed:
- Whole final Performance AU bundler via programme runner PASS: target/w163-final-bundle.log, actual packaged component native contracts plus nested performance VST3 contracts/SDK47 tests, 0 failures. Component target/au-performance/release/AU/ravet_performance_au.component.
- Whole default Gain AU bundler regression PASS: target/w163-gain-regression.log; original VST3 SDK47/0 and actual AU gain lifecycle/audio/bypass/state/tails contracts.
- Actual probe mono/stereo48k: analytic timed440Hz attack/velocity, same-key channels, fractional identified notes/group/float velocity, same-frame MIDI zero/order, offset8 Level, zero-frame control, coherent complete Level state, malformed product/identity/controller-type rejection, maximum1023 MIDI batch/rejection, invalid active offset recovery, AUBase4097 vs4096 prefix silence/tail, SysEx reject, 200 render/release cycles. Invalid f64/interleaved-stereo/nonfinite rate and zero/65537/UINT32_MAX capacities reject; previous capacity retained.
- Final focused actual packaged-module probe after full-width MIDI hardening PASS: target/w163-midi-final-contracts.log; 9 malformed full-width/short-message cases preserve the previously accepted timed note and six defined ignored channel/system cases leave analytic audio unchanged. C++ rebuild warning-free: target/w163-midi-build.log. Bundler/runtime were unchanged after the whole-bundle pass.
- Python AU runtime boundary suite4/4 PASS, strict C++17 fixture syntax -Wall -Wextra -Werror PASS, git diff --check PASS, docs checker PASS before root registry integration. No Rust API/manifest changes in W163.

These are local macOS arm64 actual-binary/in-process AudioComponentAPI results, not DAW discovery/session/auval, physical devices, sandbox, allocation/deadline, other architecture/platform, accessibility or production support.


## W166: Automatic recording worker ownership — completed locally

W166 / ADR0177 automatic recording worker ownership COMPLETE, pending root grouped workspace gates.

Implementation:
- RecordingWorker<P>/RecordingPump/RecordingStatus/RecordingExit drive existing WAV, AIFF and optional FLAC bounded recorders with explicit prepare/start/stop/bounded-final-drain/finalize and owned source/original-error reclamation.
- Existing automatic read-ahead thread ownership extracted into private BackgroundWorker without changing playback contract; OS spawn failure retains source, cooperative stop/join/drop remain control-only, worker panic never reports success, no implicit Drop finalization.
- Queue token preflight rejects foreign producer before stop; direct finish_stopped requires reclaimed unique producer. from_recorder accepts hidden callback producers; AudioProducer Drop permanently publishes Release retirement, AudioConsumer diagnostics/worker Acquire retirement proves no later publication before joined bounded final drain. Live/leaked producers reject before stop. CPAL/physical shutdown timing is external, not established offline.
- Recording faults preserve original error/source until reclaimed; saved error returned by finish once. Consuming finish fault loses sink and remains Failed; Finalized only on successful complete sink return. Explicit accepted-prefix overload and backend count semantics unchanged; prepared capture push remains heap/lock/I/O-free.

Exact successful scoped commands, all via serial programme runner:
python3 target/programme-command-runner.py cargo test -p ravet-audio-file --all-features --test recording --test automatic --test aiff_buffered --test buffered --test flac_recorder --locked
  PASS 26 tests (6 new recording groups + 20 regression groups).
python3 target/programme-command-runner.py cargo test -p ravet-device --test recording_worker --test input --locked
  PASS 7 tests (2 actual PreparedCapture hidden-producer ownership integration tests + 5 input regressions), default features.
python3 target/programme-command-runner.py cargo test -p ravet-audio-file --test recording --test automatic --locked
  PASS 11 tests (5 default recording + 6 default playback groups).
python3 target/programme-command-runner.py cargo clippy -p ravet-audio-file --all-features --all-targets --locked -- -D warnings
  PASS.
python3 target/programme-command-runner.py cargo clippy -p ravet-device --test recording_worker --locked -- -D warnings
  PASS.
python3 target/programme-command-runner.py cargo fmt -p ravet-audio-file -p ravet-device --check
  PASS.
git diff --check
  PASS.

Environment correction: default ravet-device dependency initially reused stale pre-RecordingWorker audio-file artifact despite changed unconditional source. Narrow cargo clean -p ravet-audio-file removed only generated package artifacts and forced correct dependency rebuild; all above then passed. No broad clean, Cargo.lock modification, private project/process changes or backend claims.

Files: new src/background.rs, src/recording.rs; extracted src/automatic.rs; adapters buffered.rs/pcm_buffered.rs/aiff_buffered.rs/flac_recorder.rs; public ring retirement/lib exports; tests/recording.rs and ravet-device/tests/recording_worker.rs. Guide audio-files.md and ADR0177.
Root integration record: target/w166-feature-record.toml. Add automatic background.rs to existing automatic-read-ahead sources; existing audio-handoff-ring summary can mention permanent producer retirement + diagnostic, tests add recording.rs and device recording_worker.rs, limits clarify live/leaked producers never prove stopped. Input feature existing no-integrated-recording limit should narrow to no physical validated integrated recording; synthetic automatic adapter is now exercised.
No physical capture/route, deadline, long-session clock drift, resampling or platform claim.

Root review confirms original source/error retention, explicit consuming-finish
failure and producer-retirement acquire/release ownership. No new issue found.


## W163–W167 accepted shared validation snapshot

After integration of AUv2 performance, paragraph caret/selection, public VST3
profile packaging, automatic recorder ownership and local-video frames:
`target/w163-167-gates.log` completed successfully through the serial programme
runner, two compiler jobs and checksum-preserving local executable runner.
All-feature workspace tests and doctests:683 passed, zero failed/ignored.
Workspace/all-target/all-feature Clippy with warnings denied, default workspace
doctests, default and all-feature warning-denied Rustdoc, workspace format,
documentation catalogue,43 script regression tests,4 host-evidence regression
tests and git diff whitespace checks all passed. This accepts the current scoped
contracts; platform/DAW/device/deadline limitations remain as recorded per package.


## W168: Explicit Metal offscreen rectangles — implemented locally

Separate `ravet-metal` macOS package outside the umbrella/processing graph owns
an explicit thread-confined device/queue/pipeline/shared texture and bounded
instance buffer. Portable f64 rectangles clip then convert to f32 endpoints;
ordered source-over premultiplied RGBA8Unorm readback returns independent owned
images, with no window or implicit gamma/color-space conversion. Limits:
1..4096 dimensions,0..4096 raw input rectangles,64 MiB output and67,108,864
conservative clear/covered-pixel work. Complete input/work faults reject before
submission; native failure exposes no partial public frame. Native resources,
creation/submission/wait/readback/destruction are not globally time/memory bounded.

Actual local macOS27 ARM64 M4 GPU strict native probe passed after correcting a
reserved shader parameter and adding explicit failed-library/function guards.
`target/w168-metal-tests.log`: three independent GPU integration groups PASS,
including13x9 non256-aligned packed readback, CPU half-open/source-over pixel
oracle (component tolerance2), clipped extremes, empty capacity/work rejection,
owned prior image stability, repeated reuse and12 worker-owned rendering cycles.
Thread-transfer compile-fail doctest also PASS. All-target package Clippy with warnings denied PASS (`target/w168-metal-clippy.log`);
next shared workspace/doc gates remain pending. No display-window/path/text/image rendering,
antialiasing, physical presentation cadence, other-GPU/platform or deadline claim.


## W171: Reusable AUv2 native schema/control/state adoption — completed locally

Added NativeAuParameters fixed-bound immutable native-cache owner: full UInt32 IDs, complete retained CF names/units and normalized AU f32 metadata/list; declared writable-ID/value/offset preflight before SDK mutation; native range/log/step/ties-up canonicalization; full physical round-trip formatting and bounded strict ASCII parsing with unchanged outputs on failure. CF lifecycle/text is control-owned, cached scalar conversion does not borrow DSP or consumer providers. AU normalized-f32 and Apple SDK property-dispatch quirks remain explicit.

Added NativeAuState private publication validator paired to the exact schema/native state tables: decodes both native streams and exact typed AU dictionary identities before SDK restore, accepts explicit native migrations, cohere validates processor-authoritative saved streams before dictionary mutation. It never borrows live DSP or publishes a restore consumed by audio. Backend rollback/host bypass/notifications remain consumer responsibilities.

PerformanceAU consumes schema/list/info/getter/setter/text/state helpers, refuses invalid-helper initialization and replaces duplicated codec/CF guards. Render/note/lifecycle/preparation limits remain unchanged. CMake and performance bundler execute the new native-helper fixture alongside actual component tests.

Executed:
- Whole normal Performance AU bundle command via shared programme runner PASS, target/w171-bundle.log: actual self-contained component, native helper fixture, nested Performance VST3 contract and external SDK47tests/0failures. Selected performance fixture retains all mono/stereo200-cycle timing/state/fault checks and now tests actual native schema IDs/name/role, physical text complete conversion, malformed text outputs preserved, unknown UInt32 getter/setter and element/offset preflight preserving queued notes.
- Actual default Gain component regression PASS, target/w171-gain-regression.log (registration/schema/stereo/audio/bypass/complete gain recall/preserved host bypass/tails); no Gain-source or default-profile behavior changes.
- Native helper fixture PASS actual Gain/Performance tables and Gain20-byte explicit legacy-state migration; full UINT32_MAX IDs; log/step/ties-up and readonly roles; complete128-UTF16-unit supplementary name past String128 limit; caller-retained names after helper retirement; bad text/embeddedNUL/oversize/mapping failure and malformed schema/table/factory/companion rejection with safe retirement. State wrong types/partial-preset/codec/save coherence rejection is isolated and atomic before live mutation.
- Both fixtures strict C++17 -Wall -Wextra -Werror syntax PASS; normal CMake build warning-free, target/w171-build.log; AU runtime Python4/4 PASS; docs checker and git diff --check PASS before root registry integration. No public Rust API/manifest change; shared Cargo was only invoked by normal native bundle under programme lock after the audio-agent lock-safe notice.

Local macOS arm64 external-SDK/native actual-binary/component evidence only. No global installation, SDK source vendoring, DAW discovery/session/auval, physical devices, sandbox, other architecture/platform, all runtime heap or callback deadline claims.


## W169: Bounded source text editing and layout identity — completed locally

TextEditBuffer owns<=4096 UTF8 source/replacement bytes and<=64 exact snapshots
across undo/redo. Grapheme-safe directional selection, replacement/backward/forward
delete, post-join forward caret snapping and exact no-ops/errors preserve contracts.
Unique nonwrapping process-local owner identity and monotonic revision invalidate
foreign/stale/undo-restored tokens and PreparedTextLayout source caches. Selection
changes conservatively invalidate cache identity without entering/clearing history.
Font/style identity remains caller-owned; no native/DOM/IME/clipboard adapter inferred.

Root source review found no new issue. target/w169-tests.log:21 focused checks
(2 owner/revision exhaustion,7 editing,6 paragraph,6 caret) PASS.
Current compiler deprecated Atomic.fetch_update; rename to try_update preserves
contract, then2 exhaustion checks PASS (target/w169-exhaustion-final.log).
All-feature/all-target warning-denied package Clippy PASS (target/w169-clippy.log);
2 docs PASS including new edit/undo example (target/w169-doctests.log).
Documentation/diff PASS before registry integration (target/w169-docs.log).
No dependency/lock change, native input/accessibility/deadline or platform claim.


## W172: Explicit Metal bitmap scenes — implemented locally

Added immutable validated packed premultiplied RGBA8 assets, zero-copy validated
readback adoption and synchronous ordered rectangle/image composition. Whole
scene/resource/work preflight enforces prepared command count,16 distinct bitmap
objects,64 MiB distinct uploads (clipped objects included),64 MiB output and
67,108,864 conservative clear/covered pixels. Clipped f32 destination/source UV,
nearest edge-clamped numeric sampling and source-over preserve explicit color
semantics. Fresh native shared textures stay private to each call; immutable CPU
assets transfer independently of thread-confined renderer/native ownership.

Actual strict native two-by-two nearest-scale/top-bottom probe PASS. Public Rust
`target/w172-metal-tests.log`:3 new actual-GPU groups plus3 rectangle regressions
and thread-transfer compile-fail doc PASS. Independent mixed-scene pixel oracle
covers clipping/scaling/translucency/ordered overlay; invalid exact shape/premul,
17 distinct inputs/raw command/64 MiB upload budgets, duplicate dedup, retained
images and post-rejection reuse are checked. Explicit local VideoReader fixture
frames at0/1s upload at exact2x with every pixel matching decoded source bytes;
no implicit reader, hidden color transform or playback/window is created.
All-target package Clippy with warnings denied PASS (`target/w172-metal-clippy.log`).
Positive16-object boundary follow-up PASS (`target/w172-images-boundary.log`,3groups). Shared workspace
API/documentation gates follow the next stable snapshot. No native window, text/
path rendering, high-quality resampling, cross-GPU/platform or deadline claim.


## W174: Prepared AUv2 native planar bus mapping — completed locally

Added NativeAuAudioBusMap stopped configuration and fixed caller-owned NativeAuAudioBlock. Full native config/layout preflight, explicit independent compact input/output AU indexing for main/input-only/output-only/mixed logical buses, exact planar list/channel shapes, sample byte capacities/alignment/address overflow and all-output-disjoint checks precede pointer publication. Inputs may alias any output. Failed configure/map retains previous valid profile/native views; mapping never writes sample data/tails. Mapped-block copies retarget views to owned arrays. Explicit zero-frame omission keeps declared native channels with null arrays; exact zero-frame lists still validate metadata. No host routing registration is inferred.

Performance Initialize configures mapper to its prepared zero-input/mono-stereo f32 layout; Render validates SDK output buffers before transferring parameters/native processing. Its owned caller selector also preflights valid prepared raw output lists before AUBase processing/copying, rejecting aliases without advancing live phase/envelope or consuming accepted queued notes. Existing AUBase oversized rejection and bounded supported caller silence/tails remain explicit. Cleanup clears retained pointer views only after callbacks stop.

Executed:
- Whole normal performance AU bundler via shared programme runner PASS, target/w174-bundle.log: native audio + parameter/state fixtures, actual self-contained component and nested VST3 contracts/SDK47tests/0failures.
- Final owned caller guard and allocation-counter changes focused rebuilt/re-signed actual component PASS, target/w174-final-component.log. Mono/stereo timing/Level/state/fault/200-cycle suite plus aliased caller supported-prefix/tail/unrelated-buffer preservation; exact recovered oscillator+ADSR decay proves unadvanced live state; accepted queued note survives invalid caller buffers and appears at its original offset next valid block. The phase oracle explicitly includes the native10ms decay following5ms attack.
- Final native audio fixture PASS, target/w174-final-audio.log: four logical main/input-only/output-only/mixed buses retain source order through compact directions; maximum9by64(576output channels), late full/partial overlaps, unaligned/overflow/insufficient capacities, single-buffer interleaving, disconnected late input, wrong counts, oversized active slice, invalid configured/unused/empty layouts, copy view ownership, zero-frame policies and actual native Gain output/in-place + Performance analytic bindings/tails. 2000 maximum accepted and2000 late-fault mapping calls while tracking C++new/delete record0 allocations/retirements, exercising libc++std::sort on both paths. This does not prove all runtime/SDK/Rust heap behavior or deadlines.
- Existing default Gain actual component regression PASS, target/w174-gain-regression.log; no Gain source/profile/default behavior changes.
- Strict C++17 -Wall -Wextra -Werror audio and component fixtures PASS; owned CMake final build warning-free, target/w174-final-build.log; final component fixture rebuild target/w174-final-fixture-build.log. AU Python runtime4/4 PASS; docs checker and git diff --check PASS before root registry integration. No Rust/manifest/dependency changes.

Prototype limits: macOS arm64 actual native/component/offline evidence only. No generic AU host auxiliary routing, global installation, vendored SDK sources, DAW discovery/session/auval, devices, sandbox, other architecture/platform, format negotiation, f64 AU buffers or production/deadline support claims. Framework buffer mapping is separate from SDK scheduling, host registration and native processor ownership.


Validation observation: the first recovered-phase oracle omitted the native10ms
ADSR decay after240samples; its failure alone did not establish advanced phase.
The final complete oscillator/envelope oracle above passes. Raw-caller preflight
is now explicit before AUBase rather than relying on SDK-substituted buffers.


## W170: Optional bounded Ogg Vorbis worker encoding — completed locally

W170 / ADR0181 COMPLETE: optional bounded single-worker Ogg Vorbis encoding.

Public encode_vorbis/VorbisWriter/VorbisFormat, default-disabled vorbis-encode exact vorbis_rs0.5.6 default-features=false; safe local !Send codec, explicit rate/quality/serial, 4096-frame planar batches, total/call/output bounds, finite [-1,1] whole-call rejection, accepted-source/actual-output counts, original partial I/O faults, explicit EOS/flush and no implicit Drop output. Complete source <=4,194,304samples; calls<=65536frames; total<=16,777,216frames; encoded<=64MiB. Mono8..96kHz, stereo16..96kHz, quality-0.2..1.

Bounded first-priming-packet Ogg framing identifies completed identification/comment/setup headers; max physical page65,307bytes, fixed282header scratch, no sync search/trimming. Packet/lacing/body/serial preserved, first packet isolated at granule0, remainder/following sequences/CRC rewritten with checked counters; transformed bytes budgeted. Continued/mixed headers/spanning priming reject; explicit finish rejects buffered partial or missing EOS. Every parser/partial flush/I/O/budget failure gates backend retry/automaticDrop immediately, including constructor failure.

Measured stereo8k backend sustained second-channel degradation isolated independently from framing: channel0 whole46.25dB, channel1 whole7.26/interior6.98dB, direct and reframed native output identical, lag/gain/window edges do not repair. Safe builder offers no coupling control; wrapper rejects stereo<16k before sink writes. No hidden resampling/quality policy. Accepted11 mono/stereo profiles 8/16/22.05/44.1/48/96k as applicable measured FFmpeg native whole-stream37.8849..48.1078dB, exact8193frames and Symphonia PCM agreement<1e-4. Native FFmpeg6.1.1 used /Applications/miniconda3/bin/ffmpeg -c:a vorbis (not libvorbis decoder). Tiny1/17 tails exact independently; backend packet oracle bitexact and exactly+27 transformed bytes in tested streams.

Dependencies: lock added only vorbis_rs0.5.6,aotuv_lancer_vorbis_sys0.1.6,ogg_next_sys0.1.5,tinyvec1.13.3, no existing-version changes. Existing cc/errno/thiserror suffice. Binding exactpublished sourcecommit6934784e7188e98dd053370c79604c72b201e97a and complete BSD binding/libogg/aoTuV/libvorbis notices retained third-party/vorbis.md; LICENSING and guide updated. C compiler needed; no systemcodec/bindgen/libclang/defaultRNG.

Scoped gates (all Cargo through target/programme-command-runner.py, serial lock/jobs2/local identical runner):
- cargo test -p ravet-audio-file --all-features --locked PASS69 unit/integration/doc-tests (2 native tests ignored), before last additional parser retirement test.
- FINAL cargo test -p ravet-audio-file --all-features --lib --test vorbis --locked PASS6 library+7 integration tests,2 native tests ignored; latest parser retirement regression included.
- FINAL cargo test -p ravet-audio-file --all-features --test vorbis ffmpeg_native_decoder --locked -- --ignored --nocapture PASS1 independently invoked native decoder oracle,11 supportedprofiles.
- cargo test -p ravet-audio-file --features vorbis-encode --test vorbis --locked PASS5tests,2 native tests ignored (compressed-specific tests absent).
- cargo test -p ravet-audio-file --all-features --test vorbis diagnose_low_rate --locked -- --ignored --nocapture PASS1 diagnostic before lower-stereo wrapper rejection, evidence retained above.
- FINAL cargo clippy -p ravet-audio-file --all-features --all-targets --locked -- -D warnings PASS after last parser test/type alias.
- cargo clippy -p ravet-audio-file --features vorbis-encode --all-targets --locked -- -D warnings PASS.
- FINAL cargo test -p ravet-audio-file --all-features --doc --locked PASS1 !Send compile-fail contract.
- cargo fmt -p ravet-audio-file PASS; git diff --check PASS.

Parent owns registry/features/work-status/index and joint workspace docs/lint/gates. Suggested registry target/w170-feature-record.toml. ADR0181 scopedimplemented. No codec working-memory ceiling, bitexact/perceptual/automaticcapture/physical recording/RT deadline or cross-platform claim. W175 worker-created !Send capture integration approved separately, target-only until shared snapshot releases freeze.


## W173: Bounded browser textarea editing — completed locally

TextEditor owns one consumer-labelled connected live-document textarea, configured
maximum1..4096 UTF-8 bytes (committed source0..maximum), LF scalar source, browser
Intl.Segmenter grapheme-safe directional byte selections and combined0..64 undo/redo
snapshots. Exact object-identity revisions, nonwrapping exhaustion, quiet consumer
resynchronization, native DOM reconciliation and one pending composition/history
transaction preserve explicit local ownership. Browser Unicode data is unpinned
and independent of Rust EditRevision/Unicode17; no Rust transport/ack protocol is
implied. Native preedit, browser layout/history and OS work remain opaque.

Root review found token changes during selection synchronization could incorrectly
classify a later validation failure as a callback exception. Explicit notification
exception tracking now preserves original direct-call exceptions and correct native
InvalidInput/Capacity/RevisionExhausted diagnostics; committed source/history remain
intact, while legal synchronized selection may advance its revision.

Actual Chromium and hidden WebKit both20/20 DOM contract groups PASS,
target/w173-native-final.log, including changed-selection validation failures and
throwing callbacks. Focus/piano38-case native regression PASS,
target/w173-focus-regression.log. Trusted Chromium typing, backward selection,
deletion, undo/redo, newline and Tab exit also passed. document.hasFocus=false
limits that evidence to DOM keyboard behavior; synthetic composition does not
validate native IME, visual focus, screen readers or mobile keyboards. CMake/docs/
diff focused checks passed; workspace Rust/API gates follow the shared snapshot.


## W168–W174 programme snapshot — accepted locally with concurrent-member limit

Before an independently developed Foundry workspace member was added during the
run, all-feature workspace tests/doc-tests passed712 with zero failures and2
explicit native Vorbis diagnostics ignored (already executed separately).
Evidence: target/w168-174-gates.log. The later Clippy step detected its newly added
manifest; offline metadata resolution added Foundry plus serde_spanned/toml/
toml_writer, no existing package identity/version changes. A complete retry found
its then-missing HTML include; no unrelated application source was repaired.

Remaining programme Cargo gates passed with that concurrent member explicitly
excluded: all-target/all-feature Clippy with warnings denied, default doc-tests,
warning-denied default and all-feature Rustdoc. Evidence:
target/w168-174-gates-remaining.log. Whole-workspace format check reported only
concurrent Foundry differences; explicit programme package formatting passed.
The owner subsequently added Foundry's registry evidence, allowing unmodified
check-docs.py --write/check to pass.43 packaging/documentation Python and4 host
evidence Python tests plus git diff --check passed:
target/w168-174-final-checks-retry.log. This snapshot does not validate Foundry or
claim a final complete-current-workspace gate; its full build/format evidence
remains separate. W175–W180 implementation tracks resume after this scoped snapshot.


W175 / ADR0186 Worker-created automatic Vorbis capture ownership — complete 2026-10-10.

A dedicated prepared-ring controller creates/uses/finalizes/drops the genuinely !Send codec on its single worker. Only transferable Send preparation/result crosses threads. Acknowledged pause/resume, bounded stopped ring-tail drain, exact producer identity/permanent retirement preflight, retained startup/pump/finish errors, private-constructed Finalized/Aborted/Failed completion evidence, and recovered destination/consumer/counts implemented. Outer Ok proves recovery only and must not be confused with finalized file evidence. Codec/sink panic loses unwound local owners and reports Poisoned; no synthetic recovery. Spawn failure preserves preparation. Abort/Drop never drain/EOS/flush; downstream sink destructor separate. Existing W170 public writer consuming new/finish error signatures unchanged.

Default-disabled ravet-device vorbis-recording forwards optional vorbis-encode for actual PreparedCapture hidden-producer integration. Synthetic concurrent capture validates no callback heap use, accepted source counts and exact final Ogg granule; no physical CPAL shutdown, input clock drift, device deadline or total backend memory claim. Existing W170 supported profiles/parser/frame/output bounds remain.

Focused tracked receipts, all via python3 target/programme-command-runner.py:
PASS cargo test -p ravet-audio-file --all-features --lib --test vorbis --test vorbis_recording --locked: 7 library +7 writer +11 worker =25;2 optional external decoder/diagnostic groups ignored (independent evidence recorded W170).
PASS cargo test -p ravet-device --features vorbis-recording --test vorbis_recording --locked: 2 actual PreparedCapture integration groups.
PASS cargo test -p ravet-audio-file --features vorbis-encode --lib --test vorbis_recording --locked: 6 library +11 worker=17, without compressed dependency.
PASS cargo clippy -p ravet-audio-file --all-features --all-targets --locked -- -D warnings.
PASS cargo clippy -p ravet-device --features vorbis-recording --test vorbis_recording --locked -- -D warnings.
PASS cargo test -p ravet-audio-file --all-features --doc --locked: 2, worker example +writer !Send compile-fail.
PASS cargo fmt -p ravet-audio-file -p ravet-device -- --check before final ADR status update; final queued recheck no Rust changes.
PASS git diff --check scoped audio-file/device forwarding/tests/audio-files guide/ADR.
Isolated earlier unique own-lock package independently passed25 tests,Clippy,2 doctests before tracked release.

Tests exercise prepared abort/no codec headers, shape/format/budget rejection, injected OS spawn failure/retry, worker-local sink thread evidence, concurrent fixed-ring capture/backpressure, repeated32 pause/resume handshakes, failed live/foreign/leaked proof preserving state, stopped/retired tail drain including neverstartedworker, exact independent decoded timing+lossyPCM, empty stream failed ownership, wholebatch invalid source with preserved queue suffix, separate dequeued/accepted counts on capacity, startup partial-header original sink error, postacceptance sink/output faults, flush failure, paused abort preserving suffix, no finalization I/O on abort/Drop, panic without false ownership/completion.


## W177: AUv2 block-start host transport preflight — completed locally

Added immutable NativeAuTransportSnapshot and stopped-independent native availability sidecar. Each supported supplied callback executes at most once; State2 takes precedence with no legacy fallback after error. Missing/error tuples remain unknown. Successful incomplete/malformed tuples reject atomically. Foreign returned float sentinels use integer-bit initialization and IEEE finite checks so optimized fast-math cannot silently retain zero fields. Samples receive finite/integral/range checks before i64 casts; signature numerators/denominators receive exact bounds before u16 casts. Actual changed/discontinuity/cycling and State2 recording survive direct native collection, while unsupported bar/clock/pre-roll/sample-loop meanings are not inferred.

Canonical native signature requires DETAILS. Signature-only collection therefore uses unknown/default detail flags, with callback sidecar distinguishing those placeholders and legacy unknown recording from actually supplied State2 recording. All pre-roll values remain unknown/default. The native codec has no per-flag DETAILS availability; consumers needing that distinction must use the sidecar. The external VST3 projection requires actually known playing and samples before publishing a nonnull context, retaining partial independent native music/signature in the snapshot while explicitly omitting it from that bridge. No system clock or other missing fields are invented.

Performance Render adopts collector/projector after audio/note preflight and before transferring pending SDK parameter intent or invoking native processing. Malformed successful host callbacks produce supported-prefix silence/tail preservation without consuming accepted timed MIDI/Level or advancing native voice history. Errors/absence remain valid unknown transport. AUBase still owns timestamp caching including failed renders; tests recover with a new timestamp. Performance DSP currently ignores transport, so acceptance proves boundaries/scheduling rather than transport-driven DSP.

Executed:
- Whole normal Performance AU bundler through shared programme runner PASS, target/w177-bundle.log: nested public Performance VST3 contracts plus SDK47/0, Apple SDK build, self-contained ad hoc signed public component/notices, all five AU validators. Actual mono/stereo component tests include typed HostCallbackInfo, +2^63 fault silence/backing tails, preserved accepted timed MIDI and Level recovered at -2^63, fractional/missing returned signatures, callback errors/State2 precedence, legacy and absent callbacks, plus existing note/state/audio fault and 200 synthetic voice cycles.
- Normal and -O3 -ffast-math native transport fixtures PASS through the same packaged CMake targets. Four thousand accepted and four thousand rejected collector/projection cycles each use zero counted ordinary C++ heap actions. Initialized missing values, -2^63/+2^63 and neighboring range bounds, fractional sample/signature, nonfinite foreign bytes, strict Boolean/loop/order, independent availability, sidecar/default DETAILS and incomplete SDK projection are covered. Actual authoritative Rust performance zero-frame processing accepts complete/signature-only/music-only/legacy-only/unknown collected snapshots and rejects missing DETAILS dependency, reserved-field and unused-coordinate corruption.
- Existing default Gain actual component regression PASS, target/w177-gain-regression.log. Gain sources/profile/transport behavior unchanged.
- AU Python runtime4/4, documentation checker and git diff --check PASS before root feature registry integration. No Rust/Cargo/lock metadata changes on this track.

Prototype limits: local macOS arm64 native/actual component/offline evidence. Foreign callback coherence/ownership and callback allocation remain host obligations. No DAW timeline/auval/global registration, device/sandbox/other architecture, all-SDK/Rust heap or deadline/production distribution support claim. SDK sources stay external.


## W178–W179: Local document ownership and popup menus — completed locally

Parameter gestures now share ownership per original live document, with connected
same-document paired inputs validated before mutation and owner-window lifecycle
routing. Rejected end/disposal retains ownership until accepted retry. PopupMenu
composes one level of 1..256 consumer-created native action buttons, unique bounded
action strings, roving focus, native keyboard activation, outside dismissal and
exact owned-attribute restoration. Focus restoration and notification share a
reentrancy guard; disposal during focus cancels action and nested trigger labels
retain trigger ownership.

Integrated Chromium and hidden WebKit each passed31 owner-document,25 popup,
20 textarea and38 focus/piano groups (114 total). Receipts:
target/w178-179-integrated-chromium.json and native owner-documents/popup-menu/
text-editor/focus logs. Trusted keyboard cases explicitly reopen between keys
because hidden automation blurs the window; document.hasFocus=false establishes
DOM behavior only. No actual screen-reader/native-window focus certification,
cross-origin/shadow access or automatic frame/stylesheet retirement claim.


## W176: Optional CPU paths/strokes and local-font pixels — completed locally

Default-disabled surface/umbrella raster features pin tiny-skia/path0.12.0 with
std only, plus arrayref0.3.9/strict-num0.1.1; exact primary archives/checksums and
complete upstream BSD/MIT notices retained. Minimal offline resolution adds only
those four packages, no removed/version-changed identities; current concurrent
Foundry dependency changes are preserved separately. Owned PMA RGBA8 canvases,
finite affine typed paths, explicit fill/stroke/AA and whole-call budgets reject
before pixels/counters change. Optional text prepares local TrueType geometry
without retained source/font borrows. Positive thin AA uses backend coverage;
zero-width/dash/fallback/window/presenter/gamma policies are not inferred.

Promoted focused tests PASS5 raster-only and7 raster+text groups,
target/w176-focused-gates.log; actual local Metal upload/readback PASS1 comparing
every curved translucent path/Arabic glyph component within1byte. The first Metal
lint rejected a test-only constant chunks_exact; corrected as_chunks passes
all-target/all-feature Surface/Metal Clippy with warnings denied and3 surface
doctests, target/w176-remaining-gates.log. Warning Rustdoc exposed an unescaped
numeric interval; corrected interval code formatting now passes. Final default-free
all-feature LinuxARM64 and iOSARM64 compiler checks PASS,
target/w176-doc-cross-gates.log; those are not target-runtime support claims.

Independent rectangle/source-over equations and fontTools synthetic glyph bounds,
winding/cutout/affine/caps/AA, raw4096draws/cumulative65536commands/67108864work and
whole-run failure preservation cover bounded contracts. A first font oracle forgot
horizontal-side-bearing normalization; corrected independent FontTools BoundsPen
matches actual x[3,100),y[20,90) ink. Public480x240 preview includes Latin/Arabic,
cubic/quadratic stroke and evenodd cutout; target/w176-draft/preview.png inspected.
Backend working memory/flattening/time remain outside input/output/work budgets;
no native editor/GPU path/deadline/other-runtime certification follows.

## W181: Local labelled tabbed panels — completed locally

Fixed1..256 consumer native-button/panel pairs, stable IDs, manual/automatic
activation, independent focus/selection, native Tab navigation and exact owned
attributes restore are implemented. Original-document membership/type/ID/nesting
retirement and null/first eligible lifecycle fallback preserve bounded association.
Root review added full selected/eligibility/actual-focus checks before and after
notification; fallback focus handlers cannot return stale success and callback
membership mutation retires immediately. Quiet focus:false updates explicitly
leave retiring-panel focus policy to consumers; failed focus retains committed
selection with observer/explicit refresh reconciliation.

Integrated Chromium and hidden WebKit both25tabs+31owner-doc+25popup+20textarea+
38focus/piano=139 groups PASS, target/w181-integrated-chromium.json and native
integrated logs. Trusted manual arrows/Enter/Tab and automatic arrows/Space pass,
target/w181-keyboard.json. Hidden document.hasFocus=false/explicit focus-event
wrappers limit proof to DOM behavior; no screenreader/native OS focus certification.
CMake5targets/check-docs/scoped diff pass; shared Cargo/API gates follow.


## W180: Prepared LV2 block-start time:Position transport — completed locally

Added standalone Lv2TimePositionTypes and Lv2TimePosition collection with stopped cached exact URIs and fixed bounded parsing. Complete sequence/header/timestamp/order/property/padding/type/context/duplicate preflight precedes candidate publication. Long signed sample coordinates preserve full i64 range, Double beats preserve exact f64 including signed zero, Float tempo widens exactly; signature bounds/integrality precede casts. Unknown properties and unrelated bounded events stay opaque without recursive validation claims. Missing independent values remain unavailable. At most one offset0Position supplies context, later positions fully validate then omit, no extrapolation/field carry/mid-block dispatch. Unsupported valid speeds reject rather than become stopped. Signature requires canonical DETAILS; recording/loop/pre-roll false flags remain unknown/default placeholders because native DETAILS lacks per-flag availability and this profile supplies no such flags.

NativeLv2Processor gains an explicit fourth template bool Transport=false requiring its existing instrument Atom input. Gain and default MIDI-only processing/URI requirements remain unchanged. The public Performance opts in, advertises Position alongside MIDI, caches all URIs on stopped control and passes the exact direct native snapshot or null when unknown. Full audio/control/MIDI/time preflight precedes pending complete-state apply, control publication and DSP, preserving accepted recall/control/phase/history on malformed time and supported prefix silence/tails. Performance DSP currently ignores transport.

Executed:
- Whole normal Performance LV2 bundle through shared programme runner PASS, target/w180-performance-bundle.log: self-contained public .lv2 with manifest/performanceTTL/module/notices/local ad hoc signature; actual mono/stereo full instrument suite plus normal/optimized-fast-math native transport and forwarded helper validators. Mixed Position+MIDI analytic split/audio timing; missing required mapped time URI; later unsupported speed whole-block faults preserving exact phase/Level/pending complete state; valid exact-minimum sample recovery; supported silence/tails;2000prepared note/release cycles with zero counted ordinary C++ heap actions or URI maps.
- Whole default Gain LV2 bundle PASS, target/w180-gain-bundle.log: default CMake optionalPerformance flag cleared, actual packaged Gain lifecycle/controls/bypass/state/zero-frame/tails/alias/fault contracts,4000ordinary C++heap-free run/save cycles and concurrent coherent save test. Gain factory/control/state behavior unchanged.
- Direct collector normal and -O3 -ffast-math strict C++17 -Wall -Wextra -Werror builds/execution PASS. Exact i64min/max, original Double precision/signedzero, Float widening, independent/partial/unknown fields, type/context/body/extent/padding/unit/order/aliasURI/duplicate-object/property/bound faults, unsupported speeds/nonfinite foreign bytes, maximum128properties/1024events and prior-output retention.2000accepted plus2000late-rejected cycles have zero counted ordinary C++heap actions. Authoritative actual Rust native zero-frame processing accepts complete/signature-only/music-only/sample-only/unknown snapshots and rejects missing canonical DETAILS dependency/unavailable details flags.
- Strict forwarded-native helper fixture PASS: controlled matching native tables deliver exact transport to real Rust processor; late invalid Position yields zero apply/process calls and supported-prefix silence/tail retention; recovery applies pending complete recall and connected control intent; unknown is null; default MIDI-only helper requires no time URIs, ignores opaque unsupported time body and delivers timed notes with null context. No arbitrary foreign-table authentication claim.
- LV2 Python runtime3/3 and docs checker PASS; focused build logs contain no owned warnings. No Rust/Cargo/lock metadata changes on this track. Final git diff --check PASS after root registry integration.

Prototype limits: local macOS arm64 actual-native/packaged-module/offline checks only. No Linux runtime, DAW discovery/session timeline, devices, metadata validation, hardRTCapable, all-runtime/backend heap or deadline/production support. Foreign pointers/ranges remain host obligations, no generic rollback after backend failure/state apply, and SDK sources stay external.


## W175–W181 shared programme snapshot — accepted locally

All-feature full workspace tests/doc-tests PASS746, zero failures,2 known external
Vorbis diagnostic groups ignored (previously executed independently). Current
concurrent Foundry member now participates successfully; the earlier W168–174
exclusion remains a historical limit for that older snapshot only. Whole workspace
all-target/all-feature Clippy with warnings denied, default doc-tests,
warning-denied default+all-feature Rustdoc, full formatting, unmodified docs check,
43 packaging/documentation Python tests,4 host-evidence Python tests and diff check
PASS. Evidence target/w175-181-gates.log, final command process exit0. No skips or
member exclusions used for this snapshot. Individual AU/LV2/native/DOM/Metal/device
recording evidence and limits remain in the package records above.

Tracked source hold released. Next independently prepared packages MP3 encoding,
worker image codecs, modal dialogs and native AU MIDI output remain drafts until
focused review/validation; shared success does not make those APIs implemented or
establish physical device/other-platform/DAW/production/deadline support.


## W182: Optional bounded MP3 worker encoding — completed locally

Default-disabled mp3-encode supplies complete/incremental MPEG1 Layer III encoding
with explicit mono/stereo32/44.1/48k and CBR192/256/320 highest-quality profiles.
Whole finite normalized source preflight precedes codec work; prepared4096-frame
batches/output scratch, total source and physical-write budgets include finalized
Info-frame replacement. Write+Seek must start at EOF; original seek/I/O failures
retain sink ownership and diagnostics without unsafe restoration retries. Failed
writes expose a borrowed retained original error; consuming finish recovers it.
Abort/Drop never encode/finalize/write/seek/flush and intentionally discard public
abort diagnostics. Exact parsed MPEG/Info/LAME3.100 CRC/delay/padding/source/file
counts precede final rewrite, end restoration and explicit flush.

Tracked focused gates PASS:17 all-feature library/matrix/fault tests;10 MP3-only
library/fault tests;all-target/all-feature Clippy with warnings denied;3 doctests;
scoped formatting/diff. Explicit independent native FFmpeg completed-file test
PASS72 bit-rate/rate/channel/length profiles with exact source counts and maximum
PCM difference3.359e-5 versus independent Symphonia. Physical data-frame bitrate,
header/frame counts/tag CRC and one-frame partition scratch stability pass.
Sink fixtures cover retryable whole-call rejection, payload-identical original
errors, partial/zero/overreported writes, failed seeks, metadata rewrite/flush,
poisoned no-retry and nonfinalizing abort/Drop. Evidence target/w182-status.txt.

Raw gain/SNR exposes the pinned192kbps preset's0.97 input scale without framework
compensation;256/320 use1.0. Gapless-aware completed-file readers preserve exact
source count; seekless stream players can retain padding. Bundled static LAME3.100
requires Unix configure/make/C tools; only local macOS arm64 evidence. Complete
independent LGPL3 binding, LibraryGPL2-or-later core and LGPL2.1-or-later ReplayGain
notices/40 component attributions are retained; RAVET commercial terms do not
relicense or certify combined/static distribution. No physical capture, generic
perceptual, all-platform or deadline claim. Automatic MP3 capture is W186 pending.

Joint W182/W183 offline lock resolution adds14 exact package identities, removes
none, preserves every existing package version/identity and changes only the two
owned direct package dependency lists. Shared/API documentation gates follow.


## W183: Optional owned PNG/JPEG worker assets — completed locally

Default-disabled Surface/umbrella image features provide explicit byte codecs,
validated immutable straight RGBA8, consuming integer PMA conversion and bounded
private-sink PNG output. Whole PNG chunk/JPEG marker-and-entropy preflight checks
source/extents/dimensions/completion before framework output allocation; backend
strict validation retains CRC/Adler checks. Static PNG expansion/Adam7/high-byte
16-bit and selected8-bit mono/RGB/YCbCr baseline/progressive JPEG are deliberate
profiles. Numeric components receive no ICC/gamma/EXIF orientation transform.
Source16MiB, dimensions4096/output64MiB,8192records and32scans are framework limits;
backend working memory/allocation/time remain unbounded worker obligations.

Tracked focused PASS:6 independent-oracle/fault/budget groups;actual Metal decoded
PNG PMA adoption/readback1group, all components within1byte;Surface+Metal all-target
all-feature Clippy warnings denied;4 Surface doctests;warning-denied Rustdoc;
ARM64Linux+iOS compilation only. Evidence target/w183-public-gates.log, final
process exit0. First-party Python/Pillow generator reproduces all24 PNG/JPEG/raw
fixture files byte-for-byte; provenance retains hashes and exact local tools.
Seven JPEG profiles differ at most3 numeric component bytes from Pillow, with4byte
fixture tolerance. Truncated prefixes, trailing bytes, CRC/Adler corruption,
animation, header/scan/marker bounds and output budgets reject explicitly.

Exact png0.18.1/zune-jpeg0.5.15/core0.5.3 and complete backend/transitive notices
were reviewed against primary archive hashes. JPEG std-only/safe runtime; selected
PNG pure-Rust miniz backend, optional zlib-rs lock entry unselected. Default normal
graph adds no image dependency. No JPEG encoding/GIF/WebP/animation/SVG parser,
window, color-management, all-platform/device or deadline claim. Shared/API gates
follow alongside W182–185.


## W185: Bounded AUv2 native MIDI output delivery — completed locally

Added exact canonical native short MIDI and ordinary nonidentity-note conversion without invented identity/completion semantics. Finite/exact-f32 scalar, unused positive-zero/detail/word fields, MIDI status/length/data and active-slice checks precede publication. Fixed external CoreMIDI owner validates whole ordered lists, stages complete packet storage and delivers one callback with exact render timestamp/output index. Reentrant stage/delivery cannot mutate borrowed serialized packet bytes. Fresh failed stages preserve diagnostics but invalidate readiness, and every delivery attempt consumes readiness. Empty output needs no callback; missing callback/time and host refusal cannot replay stale data. Typed receipts preserve exact host error and attempted records, with acceptance unknown and no output/history rollback.

A separate synthetic actual external-SDK MusicDevice AudioComponent owns retained output-name metadata and copied callbacks, performs prepared staging before a real public native Performance process, then delivers controlled fixture records. Production Performance continues advertising no MIDI output. Raw factory selectors prove owned exact-size/scope/element/stopped setter validation; observed public AudioToolbox size/element normalization and cached protocol routing are explicit limits. No original-facade rejection/atomicity is claimed from raw SDK results.

Executed:
- Normal Performance AU bundle through shared programme runner PASS, target/w185-bundle.log: nested VST3 SDK47/0, bundled real Performance component and all eight AU validators. Actual synthetic component properties/render/native ordering, max1024notes, exact render timestamp/index/bytes, host refusal/no replay, supported late-fault silence/tail retention/pre-DSP process count and recovery PASS.
- Default existing Gain actual-component regression PASS, target/w185-gain-regression.log: local checksum-identical executable against unchanged Gain bundle, native parameters/stereo rendering/bypass/coherent recall and tails.
- Promoted direct normal and -O3 -ffast-math strict C++17 -Wall -Wextra -Werror builds and execution PASS, target/w185-strict.log and target/w185-fast-strict.log. Real Rust native ingress accepts every supported defined MIDI channel/system form and ordinary note; malformed canonical cases reject authoritatively. Actual native full-identity NoteEnd is unsupported and produces no host callback. Exact packet contents/order, minimum1/max1024template bounds, foreign address overflow, late faults/prior retained diagnostics, readiness consumption, status-77 refusal and actual serialized packet bytes across reentrant calls PASS.2000maxaccepted/late-fault cycles count zero ordinary C++ heap allocation/deallocation; this excludes backend/host/CoreMIDI deadline and allocation guarantees.
- AU Python runtime4/4, documentation checker and git diff --check PASS. No Rust/Cargo/lock metadata changes on this track. Final source headers/fixtures are public; external SDK remains outside repository.


## W184: Local native modal dialogs — completed locally

ModalDialog composes a consumer-created native dialog with bounded labels/values,
explicit initial focus, browser-owned modality and guarded close/form/cancel
completion. One original-document owner survives queued live close through
notification; one node owner prevents replacement replay. Disposal cancels
callbacks, restores original attributes/returnValue and releases document
admission only after synchronous native close/focus and callback unwind. A
per-node tombstone remains until actual trusted close; synthetic events cannot
drain it. Different nodes remain usable after retired-node event abandonment,
while same-node reuse requires actual drain without guessed timeout/deadline.
Unsupported external reopening/open removal receives guarded native cleanup;
previous-focus restoration under that ownership violation is not guaranteed.

Chromium and hidden WebKit29 modal groups each PASS, then public integrated
29modal+139existing=168 groups each PASS. Six native CMake targets build PASS;
trusted Tab/ShiftTab/Escape closes exactly once and restores prior outside focus.
Receipts target/w184-integrated-chromium.json, target/w184-integrated-*.log,
target/w184-keyboard.json. Hidden preview hasFocus=false/browser focus-boundary
and explicit fixture focus dispatch limits remain. Native Chromium close events
were delayed but completed without framework source/timing workaround. Removing
retired nodes or closing frames/windows may abandon scheduling; no node reuse,
OS focus, accessibility/screenreader or drain deadline is inferred. Strict docs
passes, tracked source frozen for shared W182–185 validation.


## W182–W185 shared programme snapshot — accepted locally

All-feature workspace tests/doc-tests PASS766, zero failures,3 external decoder
checks ignored in the shared run (2 Vorbis diagnostics and1MP3 FFmpeg matrix,
explicitly executed independently). Workspace all-target/all-feature Clippy with
warnings denied, default doc-tests, warning-denied default+all-feature Rustdoc,
full formatting, strict documentation check,43 packaging/documentation Python
and4 host-evidence Python tests and diff check PASS. Evidence
 target/w182-185-gates.log, final process exit0. Foundry participates, no member
exclusions. Individual MP3/oracle/Metal/DOM/AU receipts and limits remain above;
no new DAW/device/other-platform/runtime-deadline support follows.

Accepted package source hold released. W186 automatic MP3 capture, W187 AU native
64-bit profiles, W188 concrete control semantic/disabled contracts and W189 JPEG
writing remain isolated drafts until their own review/gates. Concurrent native
context-menu work belongs to its separate owner and requires its own runtime
receipts; shared Cargo commands do not establish that native capability's behavior.


## W187: Prepared AUv2 native f64 mapping and complete formats — completed locally

Generalized fixed prepared audio block/map storage to explicit float/double types while preserving original f32 aliases/API. Double mapping uses8byte samples/alignment without conversion, preserves native main/input-only/output-only/mixedaux order through compact independent directional AU element arrays, and performs full shape/capacity/address/outputdisjoint preflight before candidate publication. Copy-retargeting, failed-map retention, zero-frame shape/omission and foreign ownership rules remain unchanged.

Added complete stopped ASBD/native-entry profiles. Exact structural table precision/functions and native configuration/directional counts, finite equal sample rate, native packed float encoding, mono planar-flag exception, chosen bits/bytes/frame/packet, channel shape and reserved0 all validate before accepted profile/config/map publication. No temporary mixed profile is dispatched. SDK mutations/allocations/notifications remain consumer owned; candidate helper atomicity is not arbitrary SDK rollback. Stock Apple StreamFormat setter copies36bytes/discards original reserved/padding, and unchanged active ASBD can succeed before writable checks, explicitly excluded from direct helper original-facade guarantees. Parameter companion creates its handle via its own processor_entry; separate exported table address equality is not assumed.

Separate actual fixed64 MusicDevice component retains distributed Performance32 factory, owns stopped mono/stereo format negotiation and native64 setup/render, rejects changed initialized formats and supports exact unchanged no-op. Complete f32 Level ClassInfo state validates component identity/native codec before stopped recall; voices/history, normalized AU parameter UI and cross-precision migration remain outside this fixture. Caller faults retain backing tails and perform no native DSP before supported silence/recovery.

Executed:
- Normal public Performance AU bundle through shared runner PASS, target/w187-bundle.log: nested VST3 SDK47/0 and all eleven AU validators, including unchanged distributed32 component plus actual64 helper/component validators. No owned warnings.
- Default existing Gain actual component regression PASS, target/w187-gain-regression.log: checksum-identical local executable validates parameters/stereo render/bypass/coherent complete recall/tails against unchanged Gain component.
- Strict C++17 -Wall -Wextra -Werror normal and -O3 -ffast-math draft builds/execution PASS, target/w187-audio64-normal.log and target/w187-audio64-fast.log; promoted equivalent binaries execute in normal bundle. Actual Gain64 .25+2^-40 survives exactly through mapped output/in-place processing, real Performance64 timed analytic notes and zero-frame bindings, full main/aux direction order, float-aligned/double-unaligned pointer rejection,8byte capacity, alias/overlap/address/count/order/copy/late fault retention and maximum576outputs.2000maximum accepted/late-fault maps count no ordinary C++ heap allocation/deallocation. Complete format profile tests cover exact32 matching regression,64 metadata precision, malformed late ASBD bytes/flags/rates/nonfinite/reserved/channels, foreign array overflow/missing/count faults and prior profile retention.
- Strict C++23 actual synthetic AudioComponent PASS, target/w187-component.log and normal package validator: stopped stereo64 negotiation/readback and f32/interleaved/bytes/rate/channel rejection; initialized changed format rejection/unchanged no-op; timed MusicDevice notes/real native64 AudioUnitRender analytic stereo; late caller-byte fault before DSP with bounded complete-sample silence/tails/recovery; complete native64 Level save/changed recall, wrong subtype/malformed stable-ID rejection retaining accepted Level; stopped mono44100 reprepare preserving Level and exact221-sample attack oracle.
- AU Python runtime4/4, docs checker and git diff --check PASS. No Rust/Cargo/lock metadata changes on this track. External SDK remains outside repo.


## W188: Local control semantic ownership — completed locally

Custom Knob enabled state coordinates owned tabindex/ARIA and paired native number
disabling/restoration. Rejected or throwing end retains original ownership and
blocks edits/re-enable until accepted retry. Healthy disabled host refresh remains
eligible under existing focus/active rules. Original pair retirement permits only
end recovery; callbacks revalidate admission after existing-owner end before a new
begin/edit and again before DOM painting. An accepted host message is irreversible
if its callback retires the target; stale UI painting is suppressed and accepted
begin retains end recovery. Meter explicit labels remove competing aria-labelledby
and restore names exactly; live pair checks and formatter guards reject stale
display writes after callback disposal/removal/adoption.

Public integrated25new+168prior=193 groups PASS Chromium and hidden WebKit; seven
native CMake targets build PASS. Trusted keys skip disabled knob/number, produce
one enabled begin/value/end and no disabled edit, target/w188-keyboard.json.
Receipts target/w188-integrated-chromium.json and seven native integrated logs.
All12 component roles/names/states use native/DOM properties, not native AX or
screenreader evidence. Hidden hasFocus=false/window-focus limits remain explicit.
Strict docs/diff pass; no new dependencies. Shared W186–189 gates follow.


## W190: Owned WebKit native accessibility inspection — unavailable content evidence

A bounded informational probe inspected an owned synthetic WKWebView through
public modern and legacy NSAccessibility getters, then through own-process
AXUIElement reads in a separately owned visible validation window. Native
application/window/group/title metadata was readable. The remote WebKit proxy
returned kAXErrorCannotComplete (-25204) for its role; neither test button nor
input native names could be observed. All variants reported unavailable content
evidence (exit 3), not successful control-tree validation.

Strict C++17 ARC builds with -Wall/-Wextra/-Werror PASS; legacy deprecation
suppression is confined to the documented compatibility helper. Traversal was
bounded to 512 records, depth 16 and 1024-code-unit strings. Each owned window
and native view was closed/destroyed. No application activation, accessibility
actions, trust/settings change, private SPI, other-process traversal or DOM
substitute was used. Receipts remain local under target/w190-feasibility-*.log
and target/w190-feasibility-notes.md. This exact profile does not establish native
content access, screen-reader support or general platform inaccessibility;
existing DOM semantics remain separately validated.


## W189: Optional bounded opaque JPEG encoding — completed locally

The separate default-disabled Surface/umbrella jpeg-encode feature adds checked
quality 1..100 and explicit 4:4:4/4:2:2/4:2:0 baseline 8-bit encoding. Whole-source
alpha preflight requires opaque pixels before constructing codec/private output;
consumers composite first. A private 1..16 MiB gate publishes only complete
owned bytes, preserves source pixels and performs no caller I/O. Average chroma
filtering and standard Huffman tables are explicit; metadata/color management,
perceptual quality, working allocations and deadlines remain consumer limits.

Exact jpeg-encoder 0.7.1 selects std only, defaults/SIMD disabled, no runtime
dependencies/native C. Primary archive hash and upstream commit were verified;
complete MIT/Apache-2.0/IJG texts, original DCT attribution and required executable
acknowledgement are retained independently of RAVET commercial terms. Minimal
locked metadata added this one package, removed none, changed no existing
versions and changed only Surface's existing dependency list. The image-only
dependency graph excludes the encoder.

Promoted public tests PASS three groups, including the explicitly invoked
independent Pillow decoder across all 36 generated dimension/quality/sampling
profiles. Actual headers, quantization, sampling, source preservation, exact
output capacity and alpha/quality faults validate; strict Zune/Pillow maximum
component difference is 2 within a 4-byte fixture tolerance. Six existing image
groups, all-target/all-feature Surface warning-denied Clippy, five doc-tests,
warning-denied Rustdoc and Linux/iOS ARM64 compiler-only checks PASS. Evidence
target/w189-public-gates.log, exit 0; strict documentation/diff checks PASS.
These are local codec fixtures, not a general fidelity or runtime platform claim.
See [image assets](image-assets.md) and
[decision0201](decisions/0201-bounded-opaque-jpeg-encoding.md).


## W186: Worker-created automatic MP3 capture — completed locally

The shared private local-codec recording controller retains encoder construction,
operations and destruction on its worker, without a Send requirement on codec
sessions. Existing Vorbis public APIs and genuinely non-Send ownership remain
unchanged. The MP3 factory adds matching stopped/retired producer proof, acknowledged
pause/resume, bounded final tail and explicit Finalized/Aborted/Failed(original)
completion with recovered seekable destination/consumer. Outer Ok proves recovered
ownership, not finalization; accepted/dequeued source frames, file bytes and
physical writes remain distinct. First operational failure stops later dequeue
and returns its original error; abort/Drop never drain/finalize/rewrite/seek/flush.
Control joins and sink I/O may block without a deadline; panic can lose unwound
local ownership. Existing writer profiles/gapless/gain/licence limits apply.

Tracked focused checks PASS 49 audio-file groups and four actual PreparedCapture
retirement/overload/heap groups, warning-denied all-target/all-feature audio-file
and device Clippy, and four doc-tests: 57 passed, three external codec oracles
ignored here and previously executed independently. MP3 worker decoded PCM clears
the existing 256 kbps >45 dB raw SNR fixture threshold. Individual MP3-only and
Vorbis-only feature/doc/lint receipts pass; scoped formatting, strict docs and
diff checks pass. Packet target/w186-integration.md; no audio-file manifest or
lock changes, only default-disabled device mp3-recording forwarding. Native CPAL
capture/shutdown, drift, cancellation/deadlines and other targets remain outside
these synthetic capture checks. Shared W186–189 workspace gates now follow.
