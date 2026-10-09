# 0100: Actual AUv2 reference meter transfer

Status: implemented through shared reference/native editor and external SDK;
actual component contract evidence.

## Decision

Reuse the unmodified external SDK's output parameter transfer and main-thread
timer, which updates AU parameter storage/listeners and the embedded controller.
No additional SDK source, pointer-sharing or DSP UI read is introduced. The
optional editor's IDs 10..21 carry normalized peak/RMS/diagnostic triples; AU
metadata is readable without writable flags. VST3 hidden metadata need not mean
hidden AU parameters. The UI uses the shared Rust codec/local meter controls.

Levels saturate at four for display, native audio/f64 meters remain unchanged.
Host bypass may skip processing; stopped/empty/fault paths also retain prior
meters. Transfers depend on a running timer/run loop and individual parameter
updates are not a complete-block UI snapshot. No DAW/auval/assistive-tech,
other-architecture/platform or timer deadline evidence is inferred.

## Evidence

Actual loaded component renders synthetic input at 48000 Hz and three Cocoa
editor cycles. Tests verify twelve AU metadata/value records, four displayed RMS
values, changed gain and injected nonfinite input/output flags then recovery.
Shown-window PNG inspection confirms readable meters. Existing gain/bypass/
state/preset/resize, factory 420 render blocks, nested VST3 program36 blocks/six
activations and SDK47 validation continue to pass. Metadata-owned CF names are
released by the probe. No device audio/captured consumer recording is used.
