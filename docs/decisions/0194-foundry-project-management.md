# 0194 — Foundry project management

Status: prototype implemented; target runtime validation, module selection and
production distribution remain separate work.

RAVET Foundry is the reusable framework project creation/configuration/build
application. Consumer code stays outside the repository/workspace. Its UI uses
RAVET controls/theme and native editor backends: the Rust StandaloneWindow on
macOS, thin Windows/Linux C++ launchers over the existing native surface backends.
The shared Rust engine exports a narrow UI-thread native ABI for those launchers.
Foundry explicitly opts out of audio settings because it manages projects;
generated standalone audio applications retain the standard default settings.

A versioned, bounded, unknown-field-rejecting `ravet-project.toml` stores settings.
Cargo remains the consumer build system; the first templates need no second
IDE-specific project format. Name/template/effect-or-instrument role are immutable after creation. The
engine writes product metadata through a tracked build script so subsequent
settings changes do not require rewriting editor/DSP source. Source is copied
once, then consumer-owned; the offline/effect templates consume public gain processing through explicit local
dependencies; instruments copy the sine CLAP and native performance references. Copied templates retain their
framework licence regardless of caller-selected Cargo metadata.

Creation uses an exclusive new directory and canonical parent/framework checks.
Regeneration validates all tracked files before writing, rejects edited managed
files/symlinks, serializes cooperating operations and stages per-file replacement.
The group is not crash-atomic; ordinary I/O failure attempts rollback. Concurrent
external filesystem writers are outside this prototype contract. Ownership
metadata is retained with consumer source; lock/staging files are transient.

Native actions submit to one explicit worker, with bounded coalesced snapshots
and output. Cargo/process invocation uses arguments rather than a shell. Closing
requests cancellation of the direct child, with descendant ownership outside the
contract. Creation is offline; builds explicitly resolve dependencies, lock and
compile trusted consumer code. Packaging is local/development-only. Effect adapters include CLAP/VST3/AUv2/
AUv3/AAX/LV2; instrument adapters include CLAP/VST3/AUv2/LV2. SDK paths stay external,
and missing SDKs reject requested builds before Cargo resolution. All native
adapters are copied once so changing selected formats preserves consumer edits.
Managed headers/plists/TTL/CMake separate product metadata from source; stable
native class IDs derive from product identity. Native and CLAP references have
separate processor bindings; Foundry does not claim arbitrary DSP or MPE parity.
Formats and modules are separate concerns; module/platform selectors are plans.

Source-build instructions describe actual native launcher paths without claiming
unexecuted Windows/Linux runtime support. Generated standalone UI is currently
macOS-only, independently of Foundry's cross-platform frontend design. Broader
configuration/instrument exporters and production distribution remain plans.

Directory browsing runs on the initialized native UI thread through Cocoa,
Windows COM or GTK, with bounded whitelisted field requests. The macOS bridge
queues requests outside callback borrows before entering the native modal loop,
so WebKit snapshots can continue during chooser events. Choices publish a
sequenced transient snapshot; repeating a choice applies once per new sequence,
and later snapshots preserve intervening user edits. Cancel does not change the
field or save settings. A new-project selection chooses an existing parent and
appends the current package name, preserving exclusive destination creation.
