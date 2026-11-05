# 0283: Generic LV2 adapter

**Date:** 5 November 2026  
**Status:** Implemented as a prototype in `crates/ravet-plugin/lv2` (local
macOS arm64 evidence only). Implements the LV2 row of
[decision 0281](0281-generic-export-layer.md) §3 over the table fixed by
[decision 0282](0282-plugin-policies-and-native-table.md).

## Context

The LV2 adapter must turn any product exporting `ravet_plugin_v1` into an
`.lv2` bundle without product code. LV2 differs from the other formats in
four ways that need a decision: metadata is Turtle that hosts read before
loading the binary; each plug-in URI has exactly one port set; parameters are
control ports that hosts store themselves; and UIs are separate binaries that
hosts may run in another process.

## Decision

- **Generic binaries plus the product library.** One DSP module and one UI
  module serve every product. The bundle carries the product library under the
  fixed name `ravet_product.{so,dylib,dll}`, and both modules load it from their
  own directory at run time. No adapter code is compiled per product, and the
  product library is never unloaded.
- **Turtle from the table.** A build-time tool loads the built product library
  and writes `manifest.ttl` and `plugin.ttl`. The port layout comes from one
  shared function that the adapter also uses at run time, so indexes always
  agree. Turtle is therefore generated on the build platform.
- **One port set: the preferred layout.** The first declared layout is
  exported; the others are not. Fixed ports come first (audio, `events_in`,
  optional `midi_out`, `latency`), then one control port per parameter in
  declaration order. Parameter symbols are the parameter keys, which are
  permanent; the bypass parameter becomes the inverted `lv2:enabled` port.
- **An Atom input on every plug-in.** `events_in` always accepts
  `time:Position`, so tempo-aware effects get transport, and MIDI when the
  product takes notes. Positions update a clock that the adapter advances
  between host reports, since LV2 hosts send positions only on changes.
- **State is the envelope; ports win only when they change.** The whole
  envelope is one `atom:Chunk` property. After a restore, the port values
  present at the next block become the baseline, so stale ports cannot undo a
  restore while later port changes still apply.
- **`urid:map` is optional.** Without it the plug-in still processes audio and
  controls, ignoring Atom ports and state. This lets feature-less hosts such as
  `lv2apply` run it.
- **The UI talks to the DSP through control ports only.** `instance-access` is
  not universal and UIs may be out of process, so the UI owns a mirror instance
  of the table that is never processed. Port notifications set the mirror's
  values; the mirror's editor edits go back through the UI write function to
  the same control ports, with `ui:touch` for gestures.
- **UI only where the table hosts editors.** The table hosts Cocoa views only in
  this release, so product bundles declare the UI on macOS by default. The UI
  module itself supports X11 and Win32 parents and is tested on them with the
  mock table.

## Consequences

- Any product bundles with `scripts/bundle-lv2.py --plugin <name>` or
  `--crate <package>`; the 0.0.1 hand-written references remain until step 6 of
  decision 0281 retires them.
- Mono-only hosts get no mono variant of a stereo-first product. Products that
  need one must list it first or ship a second product.
- The LV2 editor sees parameter values only: no processor telemetry, product
  extra state or product messages that need the processor.
- Adding parameters to the end of the list keeps existing port indexes;
  reordering or removing parameters changes indexes, although hosts that store
  values by symbol keep them.
