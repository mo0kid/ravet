# RAVET documentation

RAVET provides portable processing/DSP, prepared graphs, observable state and
preset files, WAV/AIFF workers, MIDI/OSC and optional runtime/device services.
The shared Rust gain reference exports CLAP, VST3, AUv2, AAX and LV2 prototypes
with different validation limits; CLAP includes a macOS embedded editor.
Use these guides to run the examples, understand the processing boundary, and
track which capabilities are ready to use. Public Rust APIs remain provisional.

| Read | Purpose |
|---|---|
| [Live documentation site](documentation-site.md) | Searchable portal, automatic rebuilding and integrated API reference |
| [Getting started](getting-started.md) | Requirements, offline example, CLAP bundle, and local API documentation |
| [Small examples](examples.md) | MIDI synth, effect plug-in, MPE expression and MIDI 2.0 walkthroughs |
| [Project starters](project-starters.md) | Create an independent offline consumer workspace outside RAVET |
| [Feature catalogue](FEATURES.md) | Current capabilities, implementation limits, source, and test evidence |
| [Architecture](architecture.md) | Crate boundaries, ownership, lifecycle, and real-time obligations |
| [Extended processing](extended-processing.md) | Precision, buses, bounded output events and block-start transport |
| [Plug-in hosting](plugin-hosting.md) | Explicit bounded candidate discovery, trusted CLAP catalogues and entry ownership |
| [Audio graphs](audio-graphs.md) | Prepared topology, routing, latency compensation and explicit fault progress |
| [Audio files](audio-files.md) | Bounded WAV/AIFF streams, worker frame buffers and prepared memory transport |
| [Audio devices](audio-devices.md) | Explicit output adapter, selection, configuration and callback probe |
| [Signal processing](signal-processing.md) | Prepared filters, FFT, FIR convolution and selectable oversampling |
| [CoreMIDI UMP](core-midi-ump.md) | Exact native IDs, protocol-selected macOS ports and guarded owned endpoint lifetime |
| [MIDI](midi.md) | Short messages, device packets, voice/RPN expression and portable sine synthesis |
| [Parameters and meters](parameters.md) | Portable schemas, complete state, bypass and bounded meter transfer |
| [Coherent snapshots](snapshots.md) | Bounded multi-word publication between control and audio threads |
| [Observable state](observable-state.md) | Transactional trees, undo/redo, revision subscriptions and presets |
| [Preset files](preset-files.md) | Explicit bounded discovery, atomic replacement and undoable selected-file recall |
| [OSC](osc.md) | Bounded packet codecs and explicit worker UDP endpoints |
| [Control data](control-data.md) | Bounded XML and cryptographic compatibility outside audio callbacks |
| [Local signed entitlements](entitlements.md) | Explicit consumer trust, bounded claims and time-rechecked verification |
| [Rust runtime services](runtime-services.md) | Bounded files/HTTPS, owned workers/timers and native ownership facade |
| [Parameter attachments](parameter-attachments.md) | Reusable stable-ID UI bindings, normalized mappings and explicit gesture ownership |
| [Portable drawing](surface-drawing.md) | Bounded geometry/layout, themed SVG typography and explicit-clock animation |
| [Web controls](web-controls.md) | Reusable local slider/number, toggle, choice and action-button components |
| [Editor control bridge](editor-control.md) | CLAP committed-edit gestures, coherent parameter reads and meter handoff |
| [Native editor](native-editor.md) | macOS WebKit prototype, ownership, and main-loop probe |
| [Sidechain CLAP reference](sidechain-clap.md) | Native f32/f64 stereo main buses and mono auxiliary detector validation |
| [MIDI sine CLAP reference](sine-clap.md) | Native instrument bundle, timed MIDI and atomic level recall |
| [VST3 reference](vst3.md) | Shared Rust DSP, external SDK build and validator/contracts |
| [AudioUnit v2 reference](audio-unit.md) | External SDK component build, normalized controls and in-process render/state checks |
| [AAX reference](aax.md) | External SDK build and binary-entry probe; host/signing prerequisites |
| [LV2 reference](lv2.md) | Mono/stereo descriptors, control ports and actual-binary checks |
| [Host validation](host-validation.md) | Disposable REAPER checks, synthetic renders and manual interaction scope |
| [Runtime validation](runtime-validation.md) | Configured workspace platform matrix and macOS CLAP ABI workflow |
| [Gain session state](session-state.md) | Product state format and CLAP restore synchronization |
| [Contributing](../CONTRIBUTING.md) | Development checks and documentation requirements for changes |
| [Maintaining documentation](maintenance.md) | Sources of truth, catalogue updates, and automated checks |
| [Public publication boundary](publication.md) | Private consumer separation and Git-history review before publication |
| [Work status](WORK_STATUS.md) | Validation history and the next bounded work packages |
| [Decision records](decisions/README.md) | Reasons for architectural and distribution choices |
| [Licensing](../LICENSING.md) | GPL edition and accepted commercial pricing/policy |
| [Commercial agreement draft](legal/COMMERCIAL_LICENSE_DRAFT.md) | Proposed contract, Order Form, and owner/legal review notes; no commercial grant |

## API reference

Rustdoc comments live beside their implementations. Build and open the umbrella
crate's reference from the workspace root:

```sh
cargo doc --workspace --no-deps --locked --open -p ravet
```

The generated crate navigation also includes the gain processor's executable
API example, the CLAP bridge, and the development host. API documentation is
built from the current checkout; generated HTML stays in Cargo's target directory.

## Keeping the reference current

Update documentation in the same change as behavior, public APIs, dependencies,
or support claims. The [feature registry](features.toml) and Cargo manifests
generate the catalogue; CI checks freshness and local references, tests Rustdoc
examples, and builds an API reference artifact for each push and pull request.
See [maintenance](maintenance.md) for the commands and review responsibilities.

[PROJECT_HANDOFF.md](../PROJECT_HANDOFF.md) preserves the longer planning history.
Use the catalogue for current capabilities and work status for execution progress.
