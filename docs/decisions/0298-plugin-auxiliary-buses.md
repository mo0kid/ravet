# 0298: Auxiliary buses and sidechains in the plug-in export layer

**Date:** 6 November 2026  
**Status:** Prototype in `ravet-plugin` (product model, CLAP export, native
table) and in the VST3, AUv2, LV2 and AAX adapters. Evidence: offline only, on
macOS arm64 (see [work status](../WORK_STATUS.md#plugin-auxiliary-buses)). No
host, DAW or Windows/Linux adapter build has exercised it.

## Context

The lower layers already carried auxiliary buses: `ProcessConfig` holds up to
eight auxiliary layouts, `ravet-native` prepares up to nine buses, and
`ravet-clap` has `MultiBusBridge`. The export layer (`Product`, the CLAP export,
`RavetPlugin.h` and the four C++ adapters) exposed the main bus only, so
products could not offer sidechains or extra outputs. This should have been
part of the 0.1.0 export layer.

## Decision

- **Product model.** `Product::with_auxiliary_buses(&[AuxiliaryBus])`. Each bus
  has a name and input and output channel counts (0..64 each, at least one
  channel). An input-only bus is a sidechain; there is no separate flag. The
  same buses apply to every main layout. At most eight buses
  (`MAXIMUM_AUXILIARY_BUSES`). New errors `TooManyAuxiliaryBuses` and
  `InvalidAuxiliaryBus(index)`.
- **One processor contract.** The processor always receives every declared bus,
  in declaration order and with the declared channel counts. Formats where a
  host can leave a bus disconnected or inactive supply silence and discard its
  outputs, so processors never see a changing bus set.
- **CLAP** uses `MultiBusBridge` for every product. Ports are the main bus
  (ID 0, flagged main), then each auxiliary bus with channels in that
  direction (ID 1 + declaration index).
- **Native table.** It stays version 1. `RavetPluginProductV1` gains
  `auxiliary_count` and `auxiliaries` (`RavetPluginAuxiliaryBusV1`). `prepare`
  requires `bus_count = 1 + auxiliary_count`, with the auxiliary layouts
  matching the product, and `process` requires the same buses.
- **Bypass policies.** Auxiliary outputs have no dry signal. `Hard` silences
  them while bypassed, `Crossfade` fades them, and `Frozen` (which now supports
  auxiliary buses) clears them and gives its zero-frame parameter block
  zero-frame auxiliary buses.
- **Formats.**
  - VST3: `kAux` buses, inactive by default.
  - AUv2: extra elements, all rendered once per cycle.
  - LV2: optional ports after every existing port, with sidechains marked
    `lv2:isSideChain`.
  - AAX: the first sidechain becomes the single mono side-chain input, and
    mono or stereo output-only buses become aux output stems. Shapes AAX
    cannot represent get silence, or have their outputs discarded, rather than
    failing the plug-in description.

  [Plug-in export](../plugin-export.md#auxiliary-buses-and-sidechains) has the
  full mapping table.
- **AUv2 without a main input.** Hosts and auval read input element 0 as the
  main input. When a product has no main input but has a sidechain, the
  sidechain is element 0. The advertised channel table therefore reports the
  sidechain's channel count as the input count (for example `[2,2]` for a
  stereo instrument with a stereo sidechain), while the adapter still matches
  the main bus internally. The alternative, no AU sidechain on instruments,
  was rejected. Hosts may read that input count as a main input; this needs
  checking in Logic.

## Alternatives considered

- **Sidechain inputs only.** This would not cover multi-output instruments, and
  the lower layers already support general buses.
- **Bumping the table to version 2.** No adapter outside this repository exists,
  and the adapters and product library are built together from the same
  source. The buses belong to version 1 as a late 0.1.0 feature.
- **Letting processors see which buses are connected.** This would vary the bus
  set per block and per format. The fixed set with silence keeps one contract.
