# RAVET documentation

RAVET provides portable processing/DSP, prepared graphs, observable state and
preset files, WAV/AIFF workers, MIDI/OSC and optional runtime/device services.
The shared Rust gain reference exports CLAP, VST3, AUv2/AUv3, AAX and LV2 prototypes
with different validation limits; CLAP includes a macOS embedded editor.
Use these guides to run the examples, understand the processing boundary, and
track which capabilities are ready to use. Public Rust APIs remain provisional.

| Read | Purpose |
|---|---|
| [Live documentation site](documentation-site.md) | Searchable portal, automatic rebuilding and integrated API reference |
| [0.0.1 source preview](releases/0.0.1.md) | Release scope, licensing and publication limits |
| [Getting started](getting-started.md) | Requirements, offline example, CLAP bundle, and local API documentation |
| [Small examples](examples.md) | MIDI synth, effect plug-in, MPE expression and MIDI 2.0 walkthroughs |
| [Embedded projects](embedded-projects.md) | no_std STM32 processing, native Arduino C++ subset and Pi Linux starters |
| [RAVET Foundry](foundry.md) | Graphical project creation, saved settings, builds and platform source-build instructions |
| [Project starters](project-starters.md) | Create an independent offline consumer workspace outside RAVET |
| [Feature catalogue](FEATURES.md) | Current capabilities, implementation limits, source, and test evidence |
| [Architecture](architecture.md) | Crate boundaries, ownership, lifecycle, and real-time obligations |
| [Extended processing](extended-processing.md) | Precision, buses, bounded output events and block-start transport |
| [Plug-in hosting](plugin-hosting.md) | Explicit bounded CLAP discovery, trusted catalogues and entry ownership |
| [VST3 hosting](vst3-hosting.md) | Caller-linked trusted catalogue ownership and explicit native linking |
| [VST3 parameters](vst3-parameters.md) | Owned stopped controller metadata and bounded timed DSP input |
| [VST3 state](vst3-state.md) | Owned stopped component/controller bytes and explicit partial failure |
| [VST3 instances](vst3-instances.md) | Main-owned components/controllers and scoped main-bus f32 processing |
| [VST3 processing](vst3-processing.md) | Paired events, parameter output and block context for hosted instances |
| [AUv3 hosting](au-hosting.md) | Optional macOS asynchronous AUv3 ownership and joined main-bus f32 audio |
| [Audio graphs](audio-graphs.md) | Prepared topology, routing, latency compensation and explicit fault progress |
| [Audio files](audio-files.md) | Bounded WAV/AIFF streams, worker frame buffers and prepared memory transport |
| [Color preparation](color-preparation.md) | Explicit sRGB/linear-light transfer and alpha association |
| [SVG images](svg-images.md) | Optional bounded resource-free SVG import and prepared pixels |
| [Image assets](image-assets.md) | Optional owned static PNG/JPEG worker codecs and explicit pixel adoption |
| [CPU raster](cpu-raster.md) | Optional owned path/stroke/local-font RGBA8 rendering |
| [Metal drawing](metal-drawing.md) | Explicit macOS offscreen rectangles/bitmap scenes and owned GPU readback |
| [System fonts](system-fonts.md) | Explicit copied catalogue/style selection and immutable font variations |
| [Font-stack paragraphs](fallback-paragraphs.md) | Exact-face wrapped mixed-direction paragraphs, grapheme-safe bidi and SVG/CPU output |
| [PostScript outlines](postscript-outlines.md) | Optional selected-instance CFF1/CFF2 outlines and explicit macOS PostScript font loading |
| [Local video](local-video.md) | Explicit macOS local-video metadata and timestamped worker frame extraction |
| [Audio devices](audio-devices.md) | Explicit output adapter, selection, configuration and callback probe |
| [Signal processing](signal-processing.md) | Prepared filters, FFT, FIR convolution and selectable oversampling |
| [Native DSP boundary](native-dsp.md) | Scoped native floating-point policy, explicit packed values and prepared vector slices |
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
| [Grid workspace](grid-layout.md) | Explicit row/column spans and measured standalone content size |
| [Window lifecycle](window-lifecycle.md) | Exact owned window state and veto-aware closure |
| [Window constraints](window-constraints.md) | Opt-in content bounds, aspect fitting and native callback guards |
| [Control semantics](control-semantics.md) | Explicit knob disabled/end ownership and meter label/formatter lifecycle |
| [Modal dialogs](modal-dialogs.md) | Browser-owned modality, guarded completion and explicit retirement drains |
| [Web controls](web-controls.md) | Reusable local slider/number, toggle, choice and action-button components |
| [Editor control bridge](editor-control.md) | CLAP committed-edit gestures, coherent parameter reads and meter handoff |
| [Native semantics](native-semantics.md) | Explicit custom-control semantic snapshots and guarded AppKit intent delivery |
| [Native editor](native-editor.md) | macOS WebKit prototype, ownership, and main-loop probe |
| [Sidechain CLAP reference](sidechain-clap.md) | Native f32/f64 stereo main buses and mono auxiliary detector validation |
| [MIDI sine CLAP reference](sine-clap.md) | Native instrument bundle, timed MIDI and atomic level recall |
| [Native client acquisition](native-client-acquisition.md) | Exact core origin and balanced CONTROL ownership |
| [Native table identity](native-table-identity.md) | Canonical runtime processor and companion table acquisition |
| [Native processing adapter](native-processing.md) | Prepared f32/f64 processing, events/transport, lifecycle, schemas, explicit state and telemetry for native shims |
| [VST3 reference](vst3.md) | Shared Rust DSP, external SDK build and validator/contracts |
| [Reference VST3 modules](vst3-reference-modules.md) | Keyed Sidechain and Event Probe test modules for hosted auxiliary buses and rich events |
| [AUv3 and mobile](mobile.md) | Native Apple/Android adapters, public app fixtures and validation limits |
| [AudioUnit v2 reference](audio-unit.md) | External SDK component build, normalized controls and in-process render/state checks |
| [AAX reference](aax.md) | External SDK build, registered native parameter/control fixtures and explicit lifecycle/host/signing limits |
| [AAX internal precision](aax-precision.md) | Explicit native32/64 DSP behind host-f32 buffers |
| [AAX timing](aax-timing.md) | Checked initial latency and prepared acknowledged replacement |
| [AAX native editor](aax-native-editor.md) | Optional owned WebKit editor and checked host automation |
| [Performance AAX](performance-aax.md) | Native no-input short-note instrument, Level state and local actual-module validation |
| [AAX transport](aax-transport.md) | Initialized public SDK capture, exact raw ticks, rounded PPQ and independent availability |
| [AAX MIDI output](aax-generated-midi.md) | Explicit generated short messages and partial SDK delivery |
| [AAX audio buses](aax-bus-mapping.md) | Explicit prepared main/aux mapping and initialized registered Rust adoption |
| [AAX short MIDI input](aax-short-midi.md) | Explicit raw complete short-MIDI instrument input and the MidiSine native32/64 reference |
| [AAX raw routing](aax-raw-routing.md) | Raw short MIDI for main and multibus effects, both auxiliary directions and one AUDIO admission |
| [AAX meters](aax-meters.md) | Opt-in prepared host meter taps and the separate Gain meter reference |
| [LV2 reference](lv2.md) | Mono/stereo descriptors, control ports and actual-binary checks |
| [LV2 MIDI output](lv2-output.md) | Prepared short MIDI staging, complete Atom capacity and output fault semantics |
| [Host validation](host-validation.md) | Disposable REAPER checks, synthetic renders and manual interaction scope |
| [Runtime validation](runtime-validation.md) | Configured workspace platform matrix and macOS CLAP ABI workflow |
| [Gain session state](session-state.md) | Product state format and CLAP restore synchronization |
| [Contributing](../CONTRIBUTING.md) | Development checks and documentation requirements for changes |
| [Maintaining documentation](maintenance.md) | Sources of truth, catalogue updates, and automated checks |
| [Public publication boundary](publication.md) | Private consumer separation and Git-history review before publication |
| [Work status](WORK_STATUS.md) | Validation history and the next bounded work packages |
| [Decision records](decisions/README.md) | Reasons for architectural and distribution choices |
| [Licensing](../LICENSING.md) | GPL edition and accepted commercial pricing/policy |

Caller-owned [local font fallback](font-fallback.md) retains exact selected faces
through shaping and SVG/CPU rendering.

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

Use the catalogue for current capabilities and work status for execution progress.
