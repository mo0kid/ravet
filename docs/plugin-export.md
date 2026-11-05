# Plug-in export layer

**Status:** Prototype, under construction for 0.1.0. The design is
[decision 0281](decisions/0281-generic-export-layer.md). The product
description, saved-state envelope, editor protocol and a generic CLAP export
exist so far, in `ravet-plugin` (umbrella features `plugin` and `plugin-clap`).
VST3, AU, AAX and LV2 adapters, the editor transport and the bundle tool follow.

## Product description

A `Product` describes a plug-in once, usually as a `const`:

```rust
use ravet_plugin::{Category, Parameter, Product, StateFormat};

const PARAMETERS: &[Parameter] = &[
    Parameter::linear("gain_db", "Gain", -60.0, 12.0, 0.0).with_unit("dB"),
    Parameter::bypass("bypass", "Bypass"),
];
const PRODUCT: Product = Product::new(
    "Example Gain",
    "Example Audio",
    "1.0.0",
    "com.example.gain",
    Category::Effect,
    StateFormat { tag: *b"EXGAIN01", version: 1 },
)
.with_parameters(PARAMETERS);
```

`Product::resolve` validates the description on a control thread. It checks
names, the three-part version, the reverse-domain plug-in ID, layouts (effects
need audio input), at most 1,024 parameters, keys, defaults and a single bypass
toggle. It then resolves parameter IDs and format identities.

Each parameter has a permanent **key**: an ASCII letter or underscore followed
by up to 63 letters, digits or underscores. Its host ID is either explicit
(`with_id`), so existing sessions keep loading, or derived from the key by
32-bit FNV-1a with the top bit cleared. IDs never depend on declaration order.
Two parameters whose derived IDs collide are rejected; give one an explicit ID.

## Exporting a plug-in

Implement `PluginExport` on a marker type that names the processor and its
product, then call `export_plugin!` once at the root of a `cdylib` crate:

```rust
use ravet_plugin::{PluginExport, Product};

pub struct GainPlugin;
impl PluginExport for GainPlugin {
    type Processor = ravet_gain::GainProcessor;
    const PRODUCT: Product<'static> = /* the product description */;
    fn create() -> ravet_gain::GainProcessor {
        ravet_gain::GainProcessor::default()
    }
}
ravet_plugin::export_plugin!(GainPlugin);
```

Any existing `Processor<f32>` can be exported unchanged. The crate must not
`forbid(unsafe_code)`, because the plug-in entry point is a foreign function;
`deny` is fine. `examples/gain-plugin` exports the gain reference this way, and
`python3 scripts/bundle-clap.py --plugin generic` bundles it. The
[tempo-synced delay](examples.md#tempo-synced-delay-plug-in) in
`examples/delay-plugin` is a fuller example with explicit parameter IDs, host
tempo, a tail and a web editor; `--plugin delay` bundles it.

The processor receives parameter changes as `ParameterValue` events with the
product's host IDs and physical values, already clamped and snapped to range.
`migrate_state` and `import_state` supply the [state hooks](#saved-state).

### CLAP export

The `clap` feature (`plugin-clap` on the umbrella crate) builds the CLAP side:

- The descriptor, audio ports, note input and parameters come from the product.
  Parameters with integer steps, and the bypass toggle, are marked stepped.
- Host values are snapped to their ranges before the processor sees them, and
  recorded in a shared parameter cache. Values the host sets on the main thread
  update the cache immediately, so saving state never misses them.
- State uses the versioned envelope and the product's hooks. Loading state
  sends every value to the processor and resets it before the next block.
- Latency is reported at activation; a later change requests a host restart.
  Tail follows `Processor::tail`.
- Every processing call runs inside `DenormalProcessor` with denormals flushed.
- On macOS, a product with an editor gets the CLAP GUI extension; see
  [editors](#editors).

Limits for now:

- Only the first declared layout is used (no audio-port configurations).
- Processing is f32.
- Note output and product extra state are not exported.
- The editor runs on macOS only; Windows and Linux editors follow.
- The automation reflection racing a concurrent state load can keep the
  automated value for that parameter.

### AAX export

The generic AAX adapter in `crates/ravet-plugin/aax` turns any product into an
unsigned `.aaxplugin` on macOS and Windows from the product's table
(`ravet_plugin_v1`): `python3 scripts/bundle-aax.py --plugin delay --aax-sdk
<external AAX SDK>`, or `--package <crate>` for another product crate. Vendors
sign (PACE-wrap) their own builds. See [AAX](aax.md#generic-aax-adapter-010) for
what maps where, the Windows commands, checks and limits.

## Format identities

All identities derive from the plug-in ID, using the same derivations as RAVET
Foundry:

| Format | Identity |
|---|---|
| CLAP | The plug-in ID |
| VST3 | Processor and controller class IDs from SHA-256 of `<id>/processor` and `<id>/controller` |
| AU | Type from the category (`aufx`, `aumu`, `aumf`); subtype from the plug-in ID, manufacturer from the vendor (four letters each) |
| AAX | Manufacturer, product and one type ID per layout, from SHA-256 of `<id>/MANUFACTURER`, `<id>/PRODUCT`, `<id>/MONO`, `<id>/STEREO` or `<id>/<inputs>x<outputs>` |
| LV2 | `urn:<id>` |

`IdentityOverrides` keeps any of them from an earlier product. Tests pin every
derivation, which never changes within a major version.

## Saved state

`ravet_plugin::state` defines one little-endian envelope for every format:
magic `RAVETSTA`, envelope version, product tag and state version, parameter
ID/value pairs and optional product extra data (up to 16 MiB). Read-only
parameters are not saved.

`state::restore` matches saved values to the current product by ID. Unknown IDs
(removed parameters) are dropped and missing ones take defaults. Values outside
the current range are rejected rather than clamped. State from a newer product
version is rejected. A product's `StateHooks` can `migrate` state written by an
earlier product state version, and `import` data without the RAVET magic, such
as sessions from another framework. Restoring never partially applies state.

These rules implement the saved-state section of the
[compatibility policy](compatibility.md).

## Editor protocol

`ravet_plugin::editor` defines the JSON messages between a plug-in and its web
editor, so products write no editor codec. Messages are objects with a `type`;
parameters are addressed by key, and values are physical unless sent as
`normalized`.

| Direction | `type` | Fields |
|---|---|---|
| Editor → plug-in | `ready` | `protocol` (currently 1) |
| | `begin`, `end` | `key` |
| | `set` | `key`, `value` or `normalized`, inside an open gesture |
| | `commit` | `key`, `value` or `normalized`: one complete edit |
| | `product` | `data`, product-defined |
| Plug-in → editor | `info` | `protocol`, product name/vendor/version/category, parameter metadata |
| | `snapshot` | `sequence`, all values by key, optional `telemetry` and `product` |
| | `changes` | `sequence`, changed values by key |
| | `product` | `data` |

`editor::parse` rejects messages over 64 KiB, malformed JSON, unknown types or
keys, edits to read-only parameters, nonfinite values, and a `protocol` other than
1. Values clamp and snap to the parameter's range. `Gestures` tracks open
gestures per editor: `set` needs an open gesture, `commit` is refused while one
is open on the same parameter, and `close_all` returns the gestures to end when
the editor closes. Several parameters may have gestures open at once.
`sequence` increases with each snapshot or change message, so editors can drop
stale updates.

The message transport inside each web view, and a JavaScript client for editor
pages, arrive with the first format adapter.

## Editors

`Product::with_editor(Editor { html, width, height, resizable })` gives a product
a web editor: one HTML page of up to 1 MiB, between 64 and 4096 pixels in each
direction. Resizable editors may grow to four times their size.

RAVET injects its editor client into the page's `<head>`. The client speaks the
[editor protocol](#editor-protocol) and exposes `window.ravetPlugin`:

| Member | Purpose |
|---|---|
| `info` | Product and parameter metadata, or `null` until it arrives |
| `value(key)`, `normalized(key)` | Current physical or 0–1 value |
| `on("info" \| "values" \| "product", listener)` | Metadata, value updates (`{values, changed}`) and product messages |
| `set(key, value)`, `setNormalized(key, n)` | One complete edit |
| `begin(key)`, `update(key, value)`, `updateNormalized(key, n)`, `end(key)` | A continuous gesture |
| `send(data)` | A product-channel message; `PluginExport::editor_message` may reply |

`examples/gain-plugin/editor/index.html` is a minimal example;
`examples/delay-plugin/editor/index.html` draws knobs and a division selector.

Editor edits update the parameter cache immediately, then travel through a
bounded lock-free queue (256 edits) to whichever thread delivers parameter
events. That thread sends the host gesture-begin, value and gesture-end events,
and gives the processor the values at the start of the block; host automation
later in the block still wins. A complete edit is queued all at once or not at
all. If the queue is full, the edit is dropped and the page keeps showing the
host's value. Closing the editor ends every gesture the page left open.

On macOS the web view asks the plug-in for its next message 20 times a second.
Queued messages (`info`, product replies) go first, then a snapshot, which is
rebuilt only when a value changes. Messages are limited to 64 KiB. A product with
so many parameters that `info` exceeds this receives no `info`; larger editor
assets and products are open work.
