# Dependency notices for plug-in exports and standalone applications

**Status:** Audit of the 0.1.0 plug-in export surface, recorded on 5 November
2026 from `Cargo.lock` at `release/0.1.0` (628af5d) with the changes of
[decision 0287](decisions/0287-export-robustness-and-notices.md), extended the
same day (from `eb4d2f5`) to the web views and standalone applications with
[decision 0292](decisions/0292-webview-robustness-and-system-notices.md). This page
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
are those in each downloaded crate archive. The
[web views and standalone applications](#web-views-and-standalone-applications)
section adds `ravet-webview` (all features) and the device crates used by
standalone applications (`ravet-device`, `ravet-midi-device`, default
features). Opt-in codec and image features keep their own scoped reviews in
[LICENSING.md](../LICENSING.md) and are not covered here.

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
| Microsoft C runtime and Windows SDK import libraries | Windows builds (CRT linked statically) | Microsoft's terms, accepted when cargo-xwin downloads them; see [the Windows C runtime](#the-windows-c-runtime) | No file required |
| glibc, libgcc_s, libstdc++ | Linux builds | System libraries, dynamically linked (glibc LGPL-2.1+; GCC Runtime Library Exception) | No |

Plug-in editors open on macOS only in this release (see
[plug-in export](plugin-export.md#linux-and-windows-builds)). A Windows or Linux
product that enables the `editor` feature, or any standalone application built
on `StandaloneWindow`, links `ravet-webview` and with it the WebView2 or
WebKitGTK components described below.

## Web views and standalone applications

### Rust crates

`ravet-webview` links no third-party Rust crate on any platform. Its
build-only dependencies are not distributed: `cc` 1.6.0, `pkg-config` 0.3.34,
`shlex` 2.0.1 and `find-msvc-tools` 0.1.14 (all MIT OR Apache-2.0).

The device crates add, for `aarch64-apple-darwin`, `x86_64-pc-windows-msvc`
and `x86_64-unknown-linux-gnu`:

| Crate | Version | Licence (manifest) | Licence files in the archive | Platforms |
|---|---|---|---|---|
| cpal | 0.18.2 | Apache-2.0 | LICENSE | all |
| midir | 0.11.0 | MIT | LICENSE | all |
| rtrb | 0.4.0 | MIT OR Apache-2.0 | LICENSE-MIT, LICENSE-APACHE | all |
| dasp_sample | 0.11.0 | MIT OR Apache-2.0 | **none** | all |
| log | 0.4.34 | MIT OR Apache-2.0 | LICENSE-MIT, LICENSE-APACHE | all |
| libm | 0.2.16 | MIT | LICENSE.txt | all |
| bitflags | 2.13.2 | MIT OR Apache-2.0 | LICENSE-MIT, LICENSE-APACHE | macOS, Linux |
| libc | 0.2.190 | MIT OR Apache-2.0 | LICENSE-MIT, LICENSE-APACHE | macOS, Linux |
| cfg-if | 1.0.5 | MIT OR Apache-2.0 | LICENSE-MIT, LICENSE-APACHE | Windows, Linux |
| core-foundation, core-foundation-sys | 0.10.1, 0.8.7 | MIT OR Apache-2.0 | LICENSE-MIT, LICENSE-APACHE | macOS |
| coreaudio-rs | 0.14.2 | MIT/Apache-2.0 | LICENSE-MIT, LICENSE-APACHE | macOS |
| coremidi, coremidi-sys | 0.9.2, 3.2.1 | MIT | LICENSE | macOS |
| mach2 | 0.6.0 | BSD-2-Clause OR MIT OR Apache-2.0 | LICENSE-BSD, LICENSE-MIT, LICENSE-APACHE | macOS |
| objc2, objc2-encode, objc2-foundation, block2 | 0.6.5, 4.1.0, 0.3.2, 0.6.2 | MIT | **none** | macOS |
| objc2-core-foundation, objc2-core-audio, objc2-core-audio-types, objc2-audio-toolbox, dispatch2 | 0.3.2 (dispatch2 0.3.1) | Zlib OR Apache-2.0 OR MIT | **none** | macOS |
| alsa, alsa-sys | 0.11.0, 0.4.0 | Apache-2.0/MIT; MIT | LICENSE-MIT, LICENSE-APACHE; LICENSE | Linux |
| windows, windows-core, windows-collections, windows-future, windows-link, windows-numerics, windows-result, windows-strings, windows-threading | 0.62.2 and companions | MIT OR Apache-2.0 | license-mit, license-apache-2.0 | Windows |
| parking_lot, parking_lot_core, lock_api, scopeguard, smallvec | 0.12.5, 0.9.12, 0.4.14, 1.2.0, 1.16.2 | MIT OR Apache-2.0 | LICENSE-MIT, LICENSE-APACHE | Windows |

Not linked: the procedural macros `windows-implement` and `windows-interface`
and their `proc-macro2`, `quote`, `syn` and `unicode-ident` dependencies run at
build time only (Windows). The generator lists them anyway, because Cargo
reports them as normal edges; the extra notices are harmless. `pkg-config` is
build-only on Linux.

All are permissive. Nine macOS crates and `dasp_sample` publish no licence file
in their archive. Where the declared licence offers MIT, the generator writes
the MIT text with the manifest's authors (or "the *crate* contributors") and
says it is reconstructed; for the Zlib option no notice is required in a
binary distribution. Distributors who want the authoritative copyright lines
should take them from the upstream repositories (`madsmtm/objc2`,
`rustaudio/sample`).

### Native components

| Component | Used by | Terms | Commercial route | What must ship |
|---|---|---|---|---|
| WebView2 SDK headers (`RAVET_WEBVIEW2_SDK`, compiled in) and `WebView2Loader.dll` (shipped beside the product binary) | Windows web views | BSD-3-Clause-style `LICENSE.txt` of the `Microsoft.Web.WebView2` NuGet package (checked for 1.0.4314.35); its `NOTICE.txt` lists third-party components of the package | Compatible: binary redistribution with the notice and disclaimer | `WebView2-SDK-LICENSE.txt` and `WebView2-SDK-NOTICE.txt`, written by the notice generator |
| Microsoft Edge WebView2 Runtime | Windows web views | Microsoft's runtime terms; installed on the user's machine, not redistributed by RAVET or its bundles | Not shipped | Nothing; installers that bundle the runtime take on Microsoft's runtime distribution terms |
| GTK 3/GDK, WebKitGTK 4.1 with JavaScriptCoreGTK, JSON-GLib, GLib/GObject/GIO | Linux web views | LGPL 2.1 or later (parts of WebKitGTK BSD-style); system-provided, linked dynamically, not redistributed | Compatible when dynamically linked: the product must not prohibit replacing the libraries or reverse engineering to debug such changes | `SYSTEM-LIBRARIES-LGPL.txt`: names the libraries, states the replacement and debugging permission, and carries the LGPL 2.1 text from the build machine |
| ALSA library (`libasound`, through alsa-sys) | Linux audio and MIDI devices | LGPL 2.1 or later; system-provided, linked dynamically | As above | Listed in `SYSTEM-LIBRARIES-LGPL.txt` |
| mingw `eventtoken.h` shim (`crates/ravet-surface/native/mingw/EventToken.h`) | mingw builds of the Windows web view | First-party: a one-line `#include_next` forwarder; the forwarded header is the toolchain's | — | Nothing |
| CoreAudio, CoreMIDI, WASAPI, WinMM and other system audio/MIDI APIs | Device crates | Operating-system components, dynamically linked | — | Nothing |

A Windows product that links `ravet-webview` must also ship
`WebView2Loader.dll` beside its binary, where the web view loads it; the
notice files above cover it. The bundle scripts do not yet copy the DLL for
Windows editor builds.

### The Windows C runtime

Windows builds made with `--target windows` link the Microsoft C runtime
statically (Rust `+crt-static`, C++ `/MT`; see
[`scripts/windows_cross.py`](../scripts/windows_cross.py)). cargo-xwin downloads
the CRT and Windows SDK only after you accept Microsoft's licence terms for
them. Those terms are the licence that applies; RAVET grants none. Microsoft
documents static linking of the CRT into an application as a supported
deployment, and no CRT notice file is required in the product. Who may use the
tools for what kind of development (for example, whether a Visual Studio or
Build Tools licence is needed for commercial work) is decided by those terms,
not by RAVET; check them for your organisation. On the GPL route, the C
runtime is a System Library of the compiler under GPL-3.0 section 1.

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
  toolchain has no `rust-docs` component;
- Windows products that link `ravet-webview`: `WebView2-SDK-LICENSE.txt` and
  `WebView2-SDK-NOTICE.txt`, copied from `RAVET_WEBVIEW2_SDK`. Without that
  variable a reminder is printed and the files are not written;
- Linux products that link `ravet-webview` or `alsa-sys`:
  `SYSTEM-LIBRARIES-LGPL.txt`, with the LGPL 2.1 text found on the build
  machine (Arch, Debian/Ubuntu or Fedora locations), or a link and a printed
  reminder when none is found.

Crates whose archive has no licence file get the reconstructed MIT notice
described above, and a reminder is printed for each.

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
to add that library's notices. Standalone applications have no bundle script;
run the generator for the application crate and the target, and ship its
output with the application. For your own product crate, run the generator
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
- **Web views and standalone applications, both routes.** The device-crate
  graph and the WebView2 SDK are permissive and compatible with both routes
  when their notices ship. The LGPL system libraries on Linux are compatible
  with both routes because they are dynamically linked and not distributed;
  on the commercial route the product's own licence must still allow users to
  replace them and to reverse engineer the product to debug such changes (LGPL
  2.1 section 6). Bundling those libraries into an installer adds the LGPL's
  source obligations. The static Windows C runtime follows Microsoft's terms
  on both routes.

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
