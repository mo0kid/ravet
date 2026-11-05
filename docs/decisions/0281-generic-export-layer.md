# 0281: Generic plug-in export layer

**Date:** 23 October 2026  
**Status:** Accepted design (23 October 2026). Partial implementation; see the feature catalogue for qualified capabilities.

## Context

[Decision 0279](0279-release-0-1-0-scope-and-distribution.md) commits 0.1.0 to
exporting any RAVET processor and web editor as VST3, CLAP and LV2 on macOS,
Windows and Linux, AU on macOS and AAX on macOS and Windows. Private products
built on 0.0.1 each wrote 1,000–1,900 lines of their own format wrappers,
because every RAVET export was a gain or sine reference.

Much of the machinery underneath is already generic:

- **Processor contract.** `Processor<S>` in `ravet-foundation` covers prepare,
  process, reset, latency, tail, f32/f64, a main bus plus up to eight auxiliary
  buses, timed events, notes, MIDI 1/2 and bounded output events.
- **Parameters.** `ParameterSchema` holds caller-keyed `ParameterId`s.
  `ParameterState` encodes versioned ID/value records with migration by ID.
- **Native C tables.** `ravet-native` exports processor, parameter, cached
  schema, audio-state, state-publication and telemetry tables for any processor,
  through `export_native_*_v1!` macros (up to 1,024 parameters).
- **Format helpers.** C++ helpers exist for VST3, AU, AAX and LV2 under
  `crates/ravet-native/include`. CLAP has generic Rust bridges in `ravet-clap`.
  The AUv3 adapter and `NativeLv2Processor<N>` are generic already.

The format-specific parts are what remain tied to gain or sine:

- Product identity is hard-coded per format.
- VST3 state uses the gain codec.
- Bus setups are mono/stereo only.
- The editor message format is product-specific.
- LV2 Turtle files are written by hand.
- AAX descriptors are hand-written.
- The bundle scripts only accept gain, sine or sidechain.
- Foundry binds its adapters to RAVET's gain/sine DSP instead of the consumer's
  processor.

Several helpers are also templated on a compile-time parameter count.

## Proposed design

### 1. One product description

A Rust `Product` value describes the plug-in once. Every format's metadata is
generated from it:

- name, vendor, version, reverse-domain plug-in ID, URL and category
  (effect, instrument, MIDI effect);
- the accepted bus layouts as an enumerable list. Hosts need this before
  `prepare`, which only accepts or rejects a single layout;
- MIDI/note input and output, latency and tail behaviour, precision (f32, f64
  or both);
- the parameter schema. Each parameter gets a **stable string key** and a
  numeric ID. LV2 symbols, AU parameter addresses and CLAP/VST3 IDs come from
  these. The numeric ID is either given explicitly, so existing sessions keep
  loading, or derived from the key by a fixed hash. It never depends on
  declaration order, as the [compatibility policy](../compatibility.md) requires;
- state product tag and version, editor assets, default size and whether the
  editor can be resized.

Format identities (VST3 FUIDs, AU type/subtype/manufacturer, AAX IDs, LV2
URIs) are derived deterministically from the plug-in ID. Products can override
them to keep identities from an earlier framework.

### 2. One export macro

`ravet::export_plugin!(MyProcessor, PRODUCT)` replaces the six separate native
macros. In a single product library it emits:

- the native tables under fixed generic symbol names. One product per binary
  keeps the names fixed; per-binary Objective-C class names already exist;
- the CLAP entry point, built in Rust on `ravet-clap`;
- a product manifest (JSON) that the build tool reads to generate format
  metadata.

The product writes no `unsafe` code and no format code.

### 3. Generic native adapters

There is one adapter per format, each compiled once against the generic tables.
Parameter counts and bus layouts are read at run time, up to the existing limits.
Adapters are no longer templated on a compile-time count.

| Format | Adapter | SDK (external, never redistributed) |
|---|---|---|
| CLAP | Rust, `ravet-clap` | none (clack) |
| VST3 | C++ component and controller over the tables | Steinberg VST3 SDK |
| AUv2 | Native C++/Objective-C++ `AUBase` subclass | Apple AudioUnitSDK |
| AAX | Native profile on the existing `RavetAax*` helpers | Avid AAX SDK |
| LV2 | Generic `NativeLv2Processor`, with Turtle generated from the manifest | LV2 headers |

The AUv2 adapter is proposed as a native subclass, not the current Steinberg
wrapper with an embedded VST3, so that AU state, lifetimes and bypass are
handled directly. AAX uses RAVET's native profile rather than the stock
wrapper, so latency, buses and descriptors come from the manifest.

### 4. Behaviour every adapter must share

These requirements come from the 0.0.1 products:

- **State.** One versioned envelope, used unchanged by every format: magic,
  envelope version, product tag and version, parameter ID/value pairs, and an
  optional opaque blob from the processor (saved and loaded on the main thread).
  It is stored in the VST3 component state, the AU class-info dictionary, the
  AAX chunk, CLAP state and an LV2 state property. A product **import hook**
  receives any blob without the RAVET magic, so a product can load sessions from
  an earlier framework.
- **Parameter flow.** A value set by the host on the main thread updates the
  state cache immediately, not at the next process call. Each product chooses
  automation timing: sample-accurate (split blocks at event offsets) or one value
  per block.
- **Bypass.** A host bypass maps to the bypass parameter role in every format.
  Products choose a policy: crossfade, hard bypass, or hard bypass with the DSP
  history frozen. Hard bypass copies input to output without scratch-buffer
  validation.
- **MIDI.** Note and MIDI input/output follow the manifest. VST3 maps MIDI
  controllers, pitch bend and aftertouch to hidden parameters generically.
- **Latency and tail** are reported in every format, and the adapter notifies
  the host when they change.
- **Denormals.** `ScopedDenormalMode` wraps every process call.

### 5. Editor

- **Generic protocol.** RAVET defines the editor protocol: set, begin and end
  gesture by parameter key, snapshot request, and a product channel for custom
  messages. The snapshot carries all parameter values, telemetry and the
  product's own payload. Products no longer write editor codecs.
- **Assets.** A local URL scheme serves assets embedded in the binary. Editors
  are no longer limited to 1 MiB of inline HTML, and large data no longer has
  to be chunked into 64 KiB snapshots.
- **Platforms.** One C ABI over the existing WebKit (macOS), WebView2 (Windows)
  and WebKitGTK/X11 (Linux) backends. Every format's view uses it; LV2 gets an
  X11 UI on Linux.
- **Lifetime.** Editor views hold a weak lifetime token, so a host that keeps a
  view after disposing the plug-in (seen with AU) cannot reach freed state.

### 6. Build and bundle tool

The current per-product bundle scripts become one tool, available from the
command line and from Foundry. It:

- builds the product library;
- generates the plists, Turtle files, VST3/AAX descriptors and resources from
  the manifest;
- runs CMake for each selected format, with SDK paths from the project
  settings;
- assembles bundles and runs a signing hook: ad hoc by default, with the
  product's own Developer ID or PACE commands when it supplies them;
- optionally runs the format validators.

Foundry projects then depend on the consumer's own processor, not on RAVET's
reference crates.

## Delivery order

Each step ends with validator and host evidence for its formats, and the public
gain and sine references are rebuilt on the layer.

1. **Product description, state envelope, editor protocol, export macro and
   CLAP.** All Rust, so the design is proven first on all three platforms.
2. **VST3** with the cross-platform editor, on macOS, Windows and Linux.
3. **LV2** with generated Turtle and the Linux UI.
4. **AUv2** native adapter on macOS.
5. **AAX** native profile on macOS and Windows.
6. **Bundle tool and Foundry** switch to the layer. The old reference adapters
   and bundle allow-lists are retired.

## Owner decisions

On 23 October 2026 the owner accepted the design and answered its open questions:

1. **AUv2** uses a native `AUBase` adapter, not the Steinberg wrapper.
2. **AAX** uses RAVET's native profile, not the stock `aax_wrapper`.
3. **The editor protocol** is RAVET-defined, with a product channel.
4. **DW-303 is the pilot.** It becomes RAVET's open-source example product: a
   GPL instrument with a web editor and a standalone application. It stays in
   its own repository and depends on RAVET by release tag. Before that
   repository is made public, its tracked files and history are reviewed for
   reference material whose redistribution rights are unclear, and for
   trademark use. As an instrument, it exercises the note, MIDI, editor and
   standalone paths. Its current hand-written CLAP layer is replaced in
   delivery step 1.

## Consequences if accepted

- `ravet-native`'s gain-specific ABIs (`RavetGain*.h`) and the per-format
  reference adapters are retired after the layer replaces them.
- Existing reference state formats change to the envelope. This is allowed
  under the compatibility policy, because they are prototypes in 0.0.1.
- Windows and Linux runtime evidence is needed for every format. Today only
  macOS arm64 has any.

## VST3 implementation choices (5 November 2026)

The runtime-table adapter lives under `crates/ravet-plugin/adapters/vst3`.
It uses separate product instances for the component and controller and SDK
messages for cache/state synchronization, preserving `kDistributable` without
sending process-local pointers. Editor views hold a weak controller lifetime
token. Audio processing uses fixed event storage and existing native audio,
transport and note helpers. The table owns denormal policy.

Products remain Cargo cdylibs. The adapter loads the named product image beside
its own module, with a module-relative platform loader and local symbol lookup.
This avoids requiring every product to add Rust static-library system-link
flags to the generic CMake build. The bundle must retain both images. The mock
instead links a small C++ table directly so adapter validation can proceed
independently of Rust table delivery.

The v1 table carries channel counts rather than speaker identities or precision
capabilities. This adapter exposes f32 and one main bus per nonempty direction;
mono/stereo have standard arrangements and larger counts use low N speaker
bits. f64, auxiliary buses and richer speaker declarations need explicit contract
work. VST3's maximum parameter ID is 0x7fffffff: larger explicit product IDs
reject rather than being remapped. MIDI input uses a deterministic free range
of 2,080 IDs within that limit. State streams are bounded to 16 MiB and preserve
the product bytes without a VST3-specific header.

macOS/Windows use SDK main-loop timers. Linux uses the host IRunLoop (including
factory context and view-frame fallbacks); without it there is no main-thread
timer service. Timing is sampled on the processor owner after processing and
published atomically; the main loop delivers restart notifications. Mock SDK
validation establishes adapter behavior only, not real editor, product, DAW or
cross-platform support. Commands and current evidence are in [VST3](../vst3.md).
