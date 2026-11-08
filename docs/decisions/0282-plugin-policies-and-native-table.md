# 0282: Plug-in automation and bypass policies, and the generic native table

**Date:** 5 November 2026  
**Status:** Implemented as a prototype in `ravet-plugin` (offline evidence only).
Refines [decision 0281](0281-generic-export-layer.md), sections 2 and 4.

## Context

Decision 0281 requires every format to share automation timing, bypass and
denormal behaviour, and the non-CLAP adapters to be written once against generic
native tables. Four C++ adapters (VST3, AUv2, AAX, LV2) are written in parallel
against one header, `crates/ravet-plugin/include/RavetPlugin.h`, so its runtime
behaviour had to be fixed precisely, and the policies had to be implemented once
rather than per adapter.

## Decision

- **One processor wrapper for every format.** `PolicyProcessor` applies the
  product's `AutomationPolicy` and `BypassPolicy`; `DenormalProcessor`
  (`FlushBoth`) wraps it. The CLAP export and the native table both run exactly
  this stack, so the policies cannot diverge between formats.
- **Bypass semantics.** Non-`Processor` policies consume the bypass parameter
  and require one at resolve time. `Hard` and `Crossfade` keep running the
  processor and mix to a prepared dry copy of the input, switching or starting
  the ramp at the event's offset. `Frozen` stops the processor (frozen history),
  copies input to output without scratch (`AudioBuffer::pass_through`), still
  delivers parameter events through a zero-frame block, and switches on block
  boundaries. This refines 0281's wording: the scratch-free path is the frozen
  policy; hard bypass keeps the processor running so that leaving it does not
  replay stale history. The dry path is not latency-compensated; products that
  need that keep `Processor` and implement bypass themselves. *(Amended by
  [0297](0297-latency-compensated-bypass.md): the dry path is now delayed by
  the processor's latency.)*
- **Per-block automation** moves every parameter event to offset 0 ahead of the
  block's other events, preserving their order.
- **One table, `ravet_plugin_v1`,** instead of the several `ravet-native` tables
  0281 listed: it carries product data, identities, parameter metadata and
  text, the cache, state with hooks, processing, timing, editor edits and the
  editor view. Host-side changes and editor edits mark parameters dirty in a
  lock-free bitset; the next block delivers them at offset 0 (at most 1,024
  processor events per block; the rest wait). A state load triggers a
  zero-frame block with every value, then a reset, before the next block.
- **Faults.** A panic in any table function returns 2; processing calls then
  return 2 until the next successful `prepare`, which recreates the processor.
  Parameter, state and editor calls keep working, so a host can still save the
  session.

## Consequences

Adapters convert host parameter values to physical values but need not round
them to exact floats, never implement bypass or automation timing, and report
editor gestures by polling `take_edits`. Products get identical behaviour in
CLAP and the other formats. Only the macOS Cocoa editor view is hosted by the
table in this release; Win32 and X11 views return 1 until the Windows and Linux
web views land. No host has loaded the table yet; DAW evidence follows the
adapters.
