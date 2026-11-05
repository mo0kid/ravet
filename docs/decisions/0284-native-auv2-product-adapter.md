# 0284: Native AUv2 product adapter contracts

**Date:** 5 November 2026  
**Status:** Implemented prototype on macOS arm64; DAW validation pending.

## Context

[Decision 0281](0281-generic-export-layer.md) requires a native AUBase adapter
over `RavetPlugin.h`. The 0.0.1 references embed VST3, expose normalized AU values
and preserve host bypass separately on preset restore. Those semantics cannot
define the new generic product contract, which has physical parameters and a
single complete state envelope.

## Decision

- Share one adapter implementation across Apple's `AUEffectBase`,
  `MusicDeviceBase` and `AUMIDIEffectBase`. Select the factory from the product's
  AU type. Keep the existing wrapper references explicitly behind `--vst3-sdk`.
- Use permanent product host IDs and physical AU parameter values. Indexed
  labels apply to integer-spaced stepped ranges; text properties always use
  the product's formatter/parser. The table owns snapping and processing policy.
- Put the unmodified envelope under the ClassInfo dictionary's `RAVET` CFData
  key. Complete recall includes the product bypass parameter. Store standard
  component identity and preset-name metadata beside it.
- Deliver scheduled parameters and short MIDI through fixed render storage.
  Preflight schedule lists atomically, expand bounded ramps into sample events,
  preserve same-offset order and carry boundary endpoints into the next block.
  Producers are serialized with rendering. Larger/multi-block ramps reject.
- Cache timing between render calls and notify on the main run loop. Read tail
  independently of latency-change detection because a processor can change its
  tail without changing latency.
- A Cocoa view retains only a weak instance token. Retire the timer, close the
  editor, invalidate the token's instance pointer, then destroy the product.
  View resize/destruction after AU disposal cannot call the table. Each product
  gets unique Objective-C class names derived from its plug-in ID.
- Build Apple's SDK from an explicit external source directory. A small loader
  reads the actual product table at build time to generate its plist. Embed a
  uniquely named product dylib with relative loading and ad hoc signing.

## Evidence and limits

The test gain, instrument and audio-backed MIDI effect passed AU Validation
Tool 1.10.0 on macOS 27.0.1 arm64/Xcode 27.1. In-process contracts exercise
mono/stereo, state, automation, timing, transport, short MIDI loopback, main-thread
dispatch and three retained-view disposal cycles for each mock. The real delay
also passes component render/state/transport and retained-WebKit-view tests.
Exact commands and results are in [work status](../WORK_STATUS.md).

This establishes a prototype, not DAW/device, sandbox, x86_64, commercial signing
or callback deadline support. The current adapter has one main f32 bus and a
1,024-event bound. No f64, auxiliary buses, SysEx/MIDI 2, MusicDevice note-instance
API, factory-program catalogue or zero-audio MIDI processor is advertised. Hosts
must supply a running main loop and serialize lifecycle/render ownership. See
the [AU guide](../audio-unit.md) for current bounds and build commands.
