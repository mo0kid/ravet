# Dependency notices for plug-in exports

**Status:** Audit of the 0.1.0 plug-in export surface, recorded on 5 November
2026 from `Cargo.lock` at `release/0.1.0` (628af5d) with the changes of
[decision 0286](decisions/0286-export-robustness-and-notices.md). This page
records what RAVET's maintainers found and what the bundle scripts ship. It is
not legal advice and grants no licence; consult your own adviser about your
distribution. See [LICENSING.md](../LICENSING.md) for RAVET's two licensing
routes.

## Scope

The audited surface is `ravet-plugin` with its `clap`, `native` and `editor`
features and the crates it pulls in (`ravet-foundation`, `ravet-clap`,
`ravet-native`, `ravet-native-dsp` with `std` and `processor`,
`ravet-webview`), plus `ravet-signal` as used by `examples/delay-plugin`. The
graph was resolved with

```sh
cargo tree --locked -p ravet-delay-plugin -e normal,build --target <triple>
```

for `aarch64-apple-darwin`, `x86_64-pc-windows-msvc` and
`x86_64-unknown-linux-gnu`; `examples/gain-plugin` resolves the same set.
Licences are the `license` field of each locked manifest; the licence files
are those in each downloaded crate archive. Other RAVET components (audio
files, surfaces, devices, the standalone runner) keep their own scoped
reviews in [LICENSING.md](../LICENSING.md) and are not covered here.

## Rust crates in a product binary

| Crate | Version | Licence (manifest) | Licence files in the archive | Reached through | Platforms |
|---|---|---|---|---|---|
| bitflags | 2.13.2 | MIT OR Apache-2.0 | LICENSE-MIT, LICENSE-APACHE | clack-plugin | all |
| block-buffer | 0.10.4 | MIT OR Apache-2.0 | LICENSE-MIT, LICENSE-APACHE | sha2 | all |
| cfg-if | 1.0.5 | MIT OR Apache-2.0 | LICENSE-MIT, LICENSE-APACHE | sha2 | all |
| clack-common | 0.2.0 | MIT OR Apache-2.0 | LICENSE-MIT, LICENSE-APACHE | clack-plugin | all |
| clack-extensions | 0.2.0 | MIT OR Apache-2.0 | LICENSE-MIT, LICENSE-APACHE | ravet-plugin (`clap`), ravet-clap | all |
| clack-plugin | 0.2.0 | MIT OR Apache-2.0 | LICENSE-MIT, LICENSE-APACHE | ravet-plugin (`clap`), ravet-clap | all |
| clap-sys | 0.5.0 | MIT/Apache-2.0 | LICENSE-MIT, LICENSE-APACHE | clack-common | all |
| cpufeatures | 0.2.17 | MIT OR Apache-2.0 | LICENSE-MIT, LICENSE-APACHE | sha2 | all |
| crypto-common | 0.1.7 | MIT OR Apache-2.0 | LICENSE-MIT, LICENSE-APACHE | sha2 | all |
| digest | 0.10.7 | MIT OR Apache-2.0 | LICENSE-MIT, LICENSE-APACHE | sha2 | all |
| generic-array | 0.14.7 | MIT | LICENSE | sha2 | all |
| itoa | 1.0.18 | MIT OR Apache-2.0 | LICENSE-MIT, LICENSE-APACHE | serde_json | all |
| libc | 0.2.190 | MIT OR Apache-2.0 | LICENSE-MIT, LICENSE-APACHE | cpufeatures | macOS |
| libm | 0.2.16 | MIT | LICENSE.txt (also carries musl and FreeBSD/Sun notices) | ravet-foundation, ravet-native-dsp | all |
| memchr | 2.8.3 | Unlicense OR MIT | COPYING, LICENSE-MIT, UNLICENSE | serde_json | all |
| serde_core | 1.0.229 | MIT OR Apache-2.0 | LICENSE-MIT, LICENSE-APACHE | serde_json | all |
| serde_json | 1.0.151 | MIT OR Apache-2.0 | LICENSE-MIT, LICENSE-APACHE | ravet-plugin | all |
| sha2 | 0.10.9 | MIT OR Apache-2.0 | LICENSE-MIT, LICENSE-APACHE | ravet-plugin (format identities) | all |
| typenum | 1.20.1 | MIT OR Apache-2.0 | LICENSE, LICENSE-MIT, LICENSE-APACHE | generic-array | all |
| zmij | 1.0.23 | MIT | LICENSE-MIT | serde_json | all |

Build-time only (not linked): `version_check` 0.9.5 (MIT/Apache-2.0), the
build-script dependency of generic-array. Test-only dependencies are not
distributed.

RAVET's own crates in the graph are first-party: `GPL-3.0-only` in this
repository, and offered under the commercial agreement on the commercial
route. The editor client script (`crates/ravet-plugin/editor/client.js`) and
the macOS web view are first-party too.

Every third-party crate above has a permissive licence. Each is compatible with
distribution under GPL-3.0-only (for dual-licensed crates, through the MIT or
Apache-2.0 option) and with proprietary distribution on the commercial route,
provided its copyright and licence notice ships with the binary. No crate in
the graph is under a copyleft licence (no MPL, LGPL or GPL).

The Rust standard library is linked into every binary. Its notice is the
toolchain's `share/doc/rust/COPYRIGHT-library.html` (installed with the
`rust-docs` component).

## Native SDKs and system components

| Component | Used by | Terms | Shipped with the bundle |
|---|---|---|---|
| CLAP API (through clap-sys) | CLAP export | CLAP is MIT; clap-sys MIT OR Apache-2.0 | In `THIRD-PARTY-NOTICES.txt` |
| VST3 SDK (external checkout) | VST3 adapter, compiled in | The supplied SDK's `LICENSE.txt` (MIT from VST3 SDK 3.8; earlier SDKs carry Steinberg's proprietary/GPLv3 dual licence) | `notices/Steinberg-VST3-LICENSE.txt` |
| Apple AudioUnitSDK (external checkout) | AUv2 adapter, compiled in | Apache-2.0 (its `LICENSE.txt`) | `notices/Apple-AudioUnitSDK-LICENSE.txt` |
| LV2 headers (external checkout) | LV2 adapter, headers only | ISC (`COPYING`) | `LV2-SDK-LICENSE.txt` |
| Avid AAX SDK (external) | AAX adapter, compiled in | Avid's terms for the SDK version supplied; distribution and PACE signing need the vendor's own Avid agreement | A reminder, `notices/AAX-SDK-NOTICE.txt`, not the SDK licence |
| macOS frameworks (Cocoa, WebKit, AudioToolbox …), libc++, libSystem | macOS builds | Operating-system components, dynamically linked, not redistributed | No |
| Microsoft C runtime and Windows SDK import libraries | Windows builds (CRT linked statically) | Microsoft's terms, accepted when cargo-xwin downloads them | No; review Microsoft's redistribution terms for the static CRT |
| glibc, libgcc_s, libstdc++ | Linux builds | System libraries, dynamically linked (glibc LGPL-2.1+; GCC Runtime Library Exception) | No |

There is no WebView2 or WebKitGTK dependency in this release: editors open on
macOS only (see [plug-in export](plugin-export.md#linux-and-windows-builds)).

## What the bundle scripts ship

Every bundle script runs `scripts/third_party_notices.py` for the product
crate and the target it builds (`x86_64-pc-windows-msvc` for `--target
windows`). It writes:

- `THIRD-PARTY-NOTICES.txt`: each linked third-party crate's name, version,
  declared licence and repository, followed by the full text of every
  `LICENSE*`, `LICENCE*`, `COPYING*`, `COPYRIGHT*`, `NOTICE*` and `UNLICENSE*`
  file in its archive. A crate whose archive has no licence file is listed with
  a note (none in this graph).
- `RAVET-LICENSE.txt`: RAVET's GPL licence text. Commercial licensees follow
  their agreement instead.
- `RUST-STANDARD-LIBRARY-COPYRIGHT.html`, or a short `.txt` pointer when the
  toolchain has no `rust-docs` component.

| Script | Bundle | Notices location |
|---|---|---|
| `bundle-clap.py` | `.clap` | macOS: `Contents/Resources/notices/`. Linux and Windows (single file): a `<Name> notices` directory beside `<Name>.clap` |
| `bundle-vst3.py` (`generic_vst3_bundle.py`) | `.vst3` | `Contents/Resources/notices/`, with `Steinberg-VST3-LICENSE.txt` |
| `bundle-au.py` (`au_native_bundle.py`) | `.component` | `Contents/Resources/notices/`, with `Apple-AudioUnitSDK-LICENSE.txt` |
| `bundle-aax.py` (`aax_plugin.py`) | `.aaxplugin` | `Contents/Resources/notices/`, with `AAX-SDK-NOTICE.txt` |
| `bundle-lv2.py` (`lv2_generic.py`) | `.lv2` | Bundle root, with `LV2-SDK-LICENSE.txt` and `RAVET-LICENSING.md` |

Single-file CLAP plug-ins have no resources folder, so an installer must carry
the `<Name> notices` directory with them. `bundle-au.py --product-library`
packages a prebuilt library whose crate graph is unknown; it prints a reminder
to add that library's notices. For your own product crate, run the generator
directly:

```sh
python3 scripts/third_party_notices.py <crate> <directory> [--target <triple>] [--manifest-path <Cargo.toml>]
```

## By licensing route

- **GPL-3.0-only route.** All Rust crates above, the CLAP API, the LV2
  headers, the AudioUnitSDK and the MIT VST3 SDK are compatible with
  distribution under GPL-3.0-only. The combined product must still meet the
  GPL's source-availability terms. The AAX SDK's own terms decide whether an
  AAX build can be distributed under the GPL at all; check the licence of the
  SDK version you use.
- **Commercial route.** The same permissive components allow proprietary
  distribution with their notices retained. The commercial agreement does not
  relicense third-party components, so their notices must still ship. AAX
  distribution also needs the vendor's own Avid agreement and PACE signing.

## Opt-in review

The export surface links 20 third-party crates, all pure Rust with no C build
step.

- `ravet-webview` (Cocoa and WebKit, plus a compiled Objective-C++ archive on
  macOS) is now behind the `ravet-plugin` `editor` feature (umbrella
  `plugin-editor`). `clap` and `native` alone build no editor.
- `sha2` and its RustCrypto helpers derive the VST3 and AAX identities at run
  time, so they stay required.
- `serde_json` is part of the editor protocol and of the
  `PluginExport::editor_message` signature, so it stays required in 0.1.0.
