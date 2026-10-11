# RAVET decision records

Decision records explain choices that affect architecture, contracts, or
distribution. They preserve reasons and consequences; the
[feature catalogue](../FEATURES.md) describes current capabilities.

| Record | Decision | Status |
|---|---|---|
| [0001](0001-portable-processing.md) | Portable processing contract | Implemented for the offline gain milestone; public API provisional |
| [0002](0002-clap-backend.md) | Clack CLAP backend | Selected for the gain prototype; host and editor integration pending |
| [0003](0003-gain-session-state.md) | Gain session state | Legacy version-one record retained; CLAP protocol extended by 0012 |
| [0004](0004-prepared-dsp-primitives.md) | Prepared DSP primitives and private consumers | Delay, feedback diffusion, fixed 2x conversion implemented; product code remains private |
| [0005](0005-native-web-editor.md) | Local WebKit native editor | Backend prototype; Cargo/CLAP integration in 0013, scoped macOS REAPER evidence recorded |
| [0006](0006-distribution-policy.md) | Distribution and licensing | GPLv3 plus commercial model and launch pricing selected; agreement pending |
| [0007](0007-control-data-services.md) | Explicit control-thread data services | Bounded XML, hashing, and legacy signature compatibility implemented |
| [0008](0008-word-snapshots.md) | Bounded coherent word snapshots | Complete atomic publications and stale-update rejection implemented |
| [0009](0009-quadrature-rotation.md) | Caller-owned quadrature rotation | Prepared arithmetic kernel implemented; consumers own phase and reanchoring |
| [0011](0011-stereo-frame-mapping.md) | Stereo frame mapping | Safe active-prefix mapping implemented; processor validation remains required |
| [0010](0010-parameter-state-and-meters.md) | Portable parameter state, bypass and meter windows | Implemented for portable APIs and gain; CLAP integration follows in 0012 |
| [0012](0012-clap-complete-state-and-edits.md) | Complete CLAP state, bypass and editor gestures | Prototype integrated; macOS GUI in 0013, REAPER gain/bypass recall exercised |
| [0013](0013-cargo-native-gain-editor.md) | Cargo native binding and embedded gain editor | macOS prototype integrated; main-loop probe and scoped REAPER validation |
| [0014](0014-cross-platform-native-web.md) | Platform web editors behind the native C++ boundary | Linux lifecycle prototype and Windows compile evidence; Rust wrapper remains macOS only |
| [0015](0015-continuous-editor-gestures.md) | Continuous editor gestures | Gain CLAP control and macOS live slider prototype; REAPER gain recording exercised |
| [0016](0016-attachments-and-resizing.md) | Parameter attachments and resizable gain editor | Portable bindings implemented; macOS resize prototype |
| [0017](0017-extended-processing.md) | Extended portable processing and transport | Portable extensions and CLAP block-start context implemented; further mappings pending |
| [0018](0018-output-devices.md) | Explicit standalone output devices | f32 output prototype; scoped UMC1820 silent callback evidence |
| [0020](0020-rust-runtime-services.md) | Rust-owned runtime and private authorization policy | Files/HTTP/workers/timers implemented; native facade and OS queries isolated |
| [0019](0019-signal-kernels.md) | Prepared numerical signal kernels | Biquads, FFT, FIR, ADSR and selectable oversampling implemented; wider DSP programme pending |
| [0021](0021-vst3-gain-native.md) | Shared Rust DSP and external VST3 SDK | Gain reference prototype, C++ contracts and SDK validator evidence |
| [0022](0022-auv2-reference.md) | External AUv2 wrapper over shared reference DSP | Normalized controls and host-owned bypass preset semantics |
| [0023](0023-native-format-references.md) | AAX/LV2 reference adapters | Shared Rust DSP, external SDKs and distinct host-validation gates |
| [0024](0024-observable-state.md) | Explicit transactional state store | Stable IDs, bounded undo/journal and undoable complete presets |
| [0025](0025-synthesis-dynamics-resampling.md) | Portable synthesis, dynamics and resampling | Explicit quality, sidechain, latency and streaming backpressure |
| [0026](0026-audio-streams.md) | WAV streams and prepared SPSC audio handoff | Explicit I/O failure, finalization and frame backpressure |
| [0027](0027-midi-voice-ownership.md) | Bounded voice ownership and configured expression | Explicit receiver/steal/sustain policy, separate DSP and negotiation |
| [0028](0028-input-duplex-devices.md) | Explicit capture and FIFO duplex | Whole-frame backpressure, underflow silence and independent clocks |
| [0029](0029-prepared-audio-graphs.md) | Static processor routing and explicit fault progress | Prepared topology/latency; later-node failures cannot roll back DSP |
| [0030](0030-osc-codecs-transport.md) | Bounded OSC codecs and explicit UDP | Wire/transport separate from scheduling and dispatch |
| [0031](0031-partitioned-convolution.md) | Uniform partitioned convolution | Exact partition latency, explicit flushing and prepared FFT history |
| [0032](0032-midi-device-packets.md) | Explicit MIDI connections and packet handoff | Bounded complete packets, raw timestamps and optional native backend |
| [0033](0033-buffered-wave-workers.md) | Buffered WAV workers | Retained read-ahead suffixes, stopped seeks and explicit recording drain |
| [0034](0034-reusable-web-controls.md) | Local DOM control components | Shared gestures, explicit transport, refresh and retryable disposal |
| [0035](0035-explicit-preset-files.md) | Explicit preset libraries | Bounded metadata discovery, validated selection and reused persistence |
| [0036](0036-independent-project-starters.md) | Independent project starters | Public templates generate separate explicit consumer workspaces |

| [0037](0037-single-zone-negotiation.md) | Opt-in single-zone negotiation | Bounded selectors, explicit zone reset and live bend ranges |
| [0038](0038-memory-playback-transport.md) | Prepared memory playback | Exclusive sample-counted commands separate from file workers |

| [0039](0039-portable-sine-instrument.md) | Portable sine instrument | Integrated voice/envelope lifecycle without format or device dependency |

| [0040](0040-clap-main-bus-instrument.md) | Native CLAP main bus and MIDI instrument | Native precision, explicit dialect and atomic reference state |

| [0041](0041-opt-in-analytics.md) | Explicit opt-in analytics | Default off, bounded numeric events and consumer-controlled delivery |

| [0042](0042-local-signed-entitlements.md) | Local signed entitlement records | Explicit public-key trust and time/product checks without activation policy |

| [0043](0043-bounded-aiff-streams.md) | Bounded uncompressed AIFF streams | Exact integer rates, prepared frame I/O and explicit finalization |

| [0044](0044-portable-visual-mechanisms.md) | Portable layout, SVG and animation | Bounded explicit geometry/drawing/clock separate from native backends |

| [0045](0045-optional-compressed-decoding.md) | Optional compressed clip decoding | Selected worker codecs and bounded whole-result output |

| [0046](0046-explicit-clap-hosting.md) | Explicit CLAP hosting boundary | Non-loading discovery, trusted bounded metadata and retained entry lifetimes |

| [0047](0047-clap-output-events.md) | CLAP generated event delivery | Explicit representability/preflight, host prefix rejection and DSP progression |

| [0048](0048-guarded-plugin-editors.md) | Guarded plugin GUI lifecycle | Borrowed live instance, explicit native parent and paired destroy |

| [0049](0049-ump-framing.md) | Fixed-storage UMP framing | Opaque packet boundaries and explicitly selected serialization |

| [0050](0050-midi2-channel-voice.md) | Full-resolution MIDI 2.0 channel voice | Explicit semantic fields without implicit conversion or profile policy |

| [0051](0051-ump-short-sysex.md) | UMP short messages and SysEx | Fixed payload chunks and atomic caller-buffer assembly |

| [0052](0052-sample-timed-ump.md) | Sample-timed UMP | Opaque portable events and explicit CLAP packet routing |

| [0053](0053-coalesced-host-requests.md) | Coalesced core host requests | Deferred bounded callback service and explicit lifecycle ownership |

| [0054](0054-hosted-parameters.md) | Bounded hosted parameters | Owned plain metadata and validated inactive dispatch |

| [0055](0055-native-graph-precision.md) | Native graph precision | One selected f32/f64 precision and byte-counted prepared storage |

| [0056](0056-prepared-graph-handoff.md) | Prepared graph handoff | Bounded FIFO replacement and control-owned retirement |

| [0057](0057-explicit-graph-events.md) | Explicit graph event scheduling | Combined topology, bounded stable merges and nonrollback event faults |

| [0058](0058-clap-auxiliary-buses.md) | CLAP auxiliary bus mapping | Independent compact directional ports and bounded whole-block scratch |

| [0059](0059-two-zone-mpe-receiver.md) | Two-zone MPE voices | Shared pool, overlap precedence and separate expression policy |

| [0060](0060-prepared-ump-handoff.md) | UMP device handoff | Fixed packets, explicit clock labels and bounded prefix publication |

| [0061](0061-core-midi-ump-endpoints.md) | CoreMIDI UMP endpoints | Exact IDs, explicit clocks and gated callback teardown |

| [0062](0062-host-parameter-requests.md) | Host parameter requests | Deferred exact-ID clears, rescan and flush with explicit overflow faults |

| [0063](0063-hosted-audio-configuration.md) | Hosted audio metadata | Owned directional snapshots and thread-correct optional timing queries |

| [0064](0064-host-audio-change-requests.md) | Host audio change requests | Coalesced native notifications deferred to owner-thread scheduling |

| [0065](0065-native-f64-gain.md) | Native f64 gain | Shared f32 control policy, native audio and scaled RMS meters |

| [0066](0066-native-f64-gain-abi.md) | Native f64 gain ABI | Separate opaque handles with shared validation/state policy |

| [0067](0067-vst3-native-f64.md) | Native VST3 gain precision | Separate prepared handles, shared control recall and output flags |

| [0068](0068-compressed-worker-streams.md) | Compressed worker reads/seeks | Lazy pending packets and atomic exact replay seeking |

| [0069](0069-compressed-read-ahead.md) | Compressed read-ahead | Retained worker suffixes and generation-safe stopped ring seeks |

| [0070](0070-worker-flac-encoding.md) | Worker FLAC encoding | Signed integer profiles, fixed blocks and short-tail metadata |

| [0071](0071-meter-text-and-held-keys.md) | Meter text and held keys | Full-level accessible text and multi-key gesture ownership |

| [0072](0072-owned-ump-sources.md) | Owned virtual UMP sources | Explicit source distribution and independent subscriber lifetime |

| [0073](0073-clap-parameter-registration.md) | Borrowed CLAP parameter registration | Exact physical metadata and explicit format validation |

| [0074](0074-incremental-flac-writer.md) | Incremental FLAC output | Retained blocks, final metadata and bounded prediction fallback |

| [0075](0075-flac-capture-workers.md) | FLAC capture workers | Explicit quantization, dequeue poison and stopped finalization |

| [0076](0076-native-editor-abi.md) | Rust-owned native editor ABI | UI-thread handles and callback-retained foreign context |

| [0077](0077-vst3-gain-editor.md) | Optional VST3 gain editor | Rust UI codecs and SDK-owned normalized gestures |

| [0094](0094-single-line-bidi.md) | Single-line Unicode bidi | Explicit/first-strong base and logical ranges in visual run order |
| [0093](0093-svg-local-glyph-outlines.md) | SVG local glyph outlines | Typed finite paths and whole-run outline rollback |
| [0092](0092-local-font-shaping.md) | Optional local-font run shaping | Explicit direction, glyph clusters and UI-owned backend work |
| [0091](0091-explicit-duplex-recovery.md) | Explicit duplex recovery | Exact-pair polling with fresh FIFO/DSP and shared release ownership |
| [0090](0090-explicit-output-recovery.md) | Explicit output recovery | Exact-ID polling, fresh DSP and stop-before-reopen ownership |
| [0089](0089-sample-peak-limiter.md) | Sample-peak limiter | Instant attack, exponential release and explicit sampled ceiling |
| [0088](0088-fixed-filter-cascades.md) | Fixed filter cascades | Stable sections, independent histories and even-order Butterworth designs |
| [0087](0087-shelving-filters.md) | Prepared shelving filters | Separate monotonic slope semantics over existing stable biquads |
| [0086](0086-downward-noise-gate.md) | Prepared downward noise gate | Explicit sidechain, detector timing and finite attenuation floor |
| [0085](0085-auv2-factory-programs.md) | Optional AUv2 factory programs | Complete block-start recall and coherent host bypass |
| [0084](0084-vst3-factory-programs.md) | Optional VST3 factory programs | Unit/list/selector metadata and complete host-owned delivery |
| [0083](0083-native-gain-programs.md) | Complete native gain programmes | Immutable metadata and complete control proposals without DSP ownership |
| [0082](0082-mpe-program-requests.md) | Master-routed MPE program requests | Independent master staging and explicit consumer recall |
| [0081](0081-sine-program-recall.md) | Sample-timed sine factory recall | Complete level programs and deferred CLAP value refresh |
| [0080](0080-complete-program-banks.md) | Complete parameter program banks | Immutable catalogue and staged bank/Program Change requests |
| [0079](0079-sostenuto-ownership.md) | Captured sostenuto note ownership | Explicit held-edge capture, independent pedal release and isolated masters |
| [0078](0078-auv2-gain-editor.md) | Optional AUv2 gain editor | Actual Cocoa protocol and notifying host refresh |

Number gaps reflect planned decisions in the historical handoff, not missing
records. Create a record when a significant choice needs to be understood by
future contributors. Include context, the decision, consequences, scope, and
validation evidence. Link it from this index and the affected feature records.

If a choice changes, identify the superseding record and update current guides
and feature status in the same change. Preserve the earlier rationale as history.

- [0095: Linear state-variable filter](0095-linear-state-variable-filter.md)

- [0096: Prepared mono reverb](0096-prepared-mono-reverb.md)

- [0097: Real spectra and windows](0097-real-spectra-and-windows.md)

- [0098: Native gain meter snapshots](0098-native-gain-meter-snapshots.md)

- [0099: VST3 host meter delivery](0099-vst3-host-meter-delivery.md)

- [0100: AUv2 meter transfer](0100-auv2-meter-transfer.md)

- [0101: Guarded native host validation](0101-guarded-native-host-validation.md)

- [0102: Bounded live resampling](0102-bounded-live-resampling.md)

- [0103: FIFO clock controller](0103-fifo-clock-controller.md)

- [0104: Clocked duplex conversion](0104-clocked-duplex.md)

- [0105: Clocked duplex recovery](0105-clocked-duplex-recovery.md)

- [0106: Native audio-owner handoff](0106-native-audio-owner-handoff.md)

- [0107: Graph audio-owner sessions](0107-graph-audio-owner-sessions.md)

- [0108: Hosted main audio graph node](0108-hosted-main-audio-graph-node.md)

- [0109: Native f64 hosted graph node](0109-hosted-native-f64-graph-node.md)

- [0110: Hosted packed audio ports](0110-hosted-packed-audio-ports.md)

- [0111: Hosted block-start transport](0111-hosted-block-start-transport.md)

- [0112: Hosted graph parameter input](0112-hosted-graph-parameter-input.md)

- [0113: Hosted graph parameter output](0113-hosted-graph-parameter-output.md)

- [0114: Hosted graph note/MIDI input](0114-hosted-graph-note-input.md)

- [0115: Hosted graph performance output](0115-hosted-graph-note-output.md)

- [0116: Hosted graph editor ownership](0116-hosted-graph-editor-ownership.md)

- [0117: Buffered graph-worker output](0117-buffered-graph-worker-output.md)

- [0118: Graph-worker event input](0118-graph-worker-event-input.md)

- [0119: Graph-worker generated output](0119-graph-worker-event-output.md)

- [0120: Graph-worker complete event queues](0120-graph-worker-event-queues.md)

- [0121: Parameter gesture events](0121-parameter-gesture-events.md)

- [0122: Global parameter modulation](0122-global-parameter-modulation.md)

- [0123: Hosted global modulation](0123-hosted-global-modulation.md)

| [0124](0124-live-documentation.md) | Canonical live documentation portal and integrated Rustdoc | Implemented locally; CI artifact configured; no public deployment |

- [0124: Buffered graph audio input](0124-buffered-graph-audio-input.md)

- [0125: Buffered clocked input worker](0125-buffered-clocked-input-worker.md)

- [0126: Buffered live graph workers](0126-buffered-live-graph-workers.md)

- [0127: Extended block-start transport](0127-extended-block-transport.md)

- [0128: Prepared stereo reverb](0128-prepared-stereo-reverb.md)

- [0129: Prepared wavetable oscillator](0129-prepared-wavetable-oscillator.md)

- [0130: Explicit parameter gesture input](0130-parameter-gesture-input.md)

- [0131: Multiplicative smoothing](0131-multiplicative-smoothing.md)

- [0132: Native note identities](0132-native-note-identities.md)

- [0132: Default standalone settings and rotary controls](0132-standalone-settings-and-knobs.md)

- [0133: Stereo delay teaching example](0133-stereo-delay-example.md)

- [0134: Scoped note expressions](0134-scoped-note-expressions.md)

- [0135: Exported native CLAP performance ports](0135-exported-native-performance-ports.md)

- [0136: Scoped note release and choke](0136-scoped-note-release.md)

- [0137: Exported scoped parameters](0137-exported-scoped-parameters.md)

- [0138: Hosted scoped parameters](0138-hosted-scoped-parameters.md)

- [0139: Linkwitz–Riley crossover](0139-linkwitz-riley-crossover.md)

- [0140: Directional note completion](0140-note-completion.md)

- [0141: Prepared stereo chorus](0141-prepared-stereo-chorus.md)

- [0142: Generic native planar adapter](0142-native-planar-adapter.md)

- [0143: Complete native event records](0143-native-event-records.md)

- [0144: Native block-start transport records](0144-native-transport-records.md)

- [0145: Consumer-selected native processor entry](0145-native-processor-entry.md)

- [0146: Native global schemas and state companion](0146-native-parameter-state-companion.md)

- [0147: Explicit native live state publication](0147-native-live-state-publication.md)

- [0148: Native schema registration for VST3](0148-vst3-native-schema-registration.md)

- [0151: Control-cached native parameter mapping](0151-native-cached-parameter-mapping.md)

- [0149: Explicit sample-wise waveshapers](0149-sample-wise-waveshapers.md)

- [0150: Prepared comb filters](0150-prepared-comb-filters.md)

- [0152: Embedded processing boundaries](0152-embedded-processing.md)

- [0153: AUv3 and mobile native adapters](0153-auv3-mobile-native-adapters.md)

- [0154: Prepared VST3 cached parameter input](0154-vst3-cached-parameter-input.md)

- [0155: Explicit native audio-owner complete state](0155-native-direct-audio-state.md)

- [0156: Explicit prepared native telemetry](0156-native-prepared-telemetry.md)

- [0157: Native C++ client and SDK reference adoption](0157-native-cpp-client.md)

- [0158: Explicit VST3 block-start transport](0158-vst3-block-start-transport.md)

- [0159: Explicit zero-volume note expression](0159-zero-volume-note-expression.md)

- [0160: Explicit VST3 performance input](0160-vst3-performance-input.md)

- [0161: Explicit VST3 performance output](0161-vst3-performance-output.md)

- [0162: Prepared VST3 native audio buses](0162-vst3-native-audio-buses.md)

- [0163: Native performance voice ownership](0163-native-performance-voice-ownership.md)

- [0164: Transactional native performance synthesis](0164-transactional-performance-sine.md)

- [0165: Public native performance reference](0165-public-native-performance-reference.md)

- [0166: Public VST3 performance instrument](0166-vst3-performance-instrument.md)

- [0167: LV2 native schema, processing and complete state](0167-lv2-native-schema-state.md)

- [0168: Explicit verified native-FLAC packet indexes](0168-indexed-flac-worker-seeks.md)

- [0169: Bounded composite focus and piano note ownership](0169-bounded-web-focus.md)

- [0171: Automatic read-ahead ownership and buffered playback](0171-automatic-read-ahead-playback.md)

- [0170: Public LV2 MIDI performance instrument](0170-lv2-midi-performance-instrument.md)

- [0172: Bounded local-font paragraph layout](0172-bounded-paragraph-layout.md)

- [0173: Shared PCM handoff and AIFF worker adapters](0173-aiff-pcm-worker-adapters.md)

- [0176: Explicit public VST3 build profiles](0176-public-vst3-profile-bundling.md)

- [0178: Explicit bounded local-video worker extraction](0178-bounded-local-video-worker.md)

- [0175: Prepared grapheme caret and selection maps](0175-grapheme-caret-selection.md)

- [0174: Public AUv2 native performance MusicDevice](0174-auv2-native-performance-instrument.md)

- [0177: Automatic recorder ownership and explicit finalization](0177-automatic-recording-workers.md)

- [0179: Explicit bounded Metal offscreen rectangles](0179-metal-offscreen-rectangles.md)

- [0182: Reusable AUv2 native schema, control and state](0182-auv2-native-schema-control-state.md)

- [0180: Bounded source text editing and layout identity](0180-bounded-source-text-editing.md)

- [0183: Explicit bounded Metal bitmap scenes](0183-metal-bitmap-scenes.md)

- [0185: Prepared AUv2 native planar bus mapping](0185-auv2-prepared-native-audio-buses.md)

- [0181: Optional bounded Ogg Vorbis worker encoding](0181-bounded-vorbis-worker-encoding.md)

- [0184: Bounded textarea editing frontend](0184-bounded-textarea-frontend.md)

- [0194: Foundry project management](0194-foundry-project-management.md)

- [0186: Worker-created automatic Vorbis recording](0186-worker-created-vorbis-recording.md)

- [0187: Optional CPU path and local-font rasterization](0187-optional-cpu-raster.md)

- [0188: AUv2 block-start transport](0188-auv2-block-start-transport.md)

- [0189: Local control owner-document routing](0189-control-owner-document.md)

- [0190: Local one-level popup menus](0190-local-popup-menu.md)

- [0192: Local labelled tabbed panels](0192-local-tabbed-panels.md)

- [0191: Prepared LV2 block-start time:Position](0191-lv2-block-start-time-position.md)

- [0193: Bounded MP3 worker encoding](0193-bounded-mp3-worker-encoding.md)

- [0195: Owned worker image codecs](0195-owned-worker-image-codecs.md)

- [0196: Bounded native modal dialogs](0196-bounded-native-modal-dialog.md)

- [0197: Bounded AUv2 native MIDI output](0197-auv2-native-midi-output.md)

- [0205: WebKit browser context menu suppression](0205-webkit-context-menus.md)

- [0199: Prepared AUv2 native f64 formats](0199-auv2-native-f64-formats.md)

- [0200: Local control semantic ownership](0200-local-control-semantics.md)

- [0198: Worker-created automatic MP3 recording](0198-worker-created-mp3-recording.md)

- [0201: Bounded opaque JPEG encoding](0201-bounded-opaque-jpeg-encoding.md)

- [0204: Local resizable split-pane layout](0204-local-split-pane-layout.md)

- [0208: Fixed flat-list selection](0208-fixed-list-selection.md)

- [0203: Explicit Performance AUv2 audio precision and checked lifecycle](0203-auv2-performance-audio-precision.md)

- [0206: Explicit static GIF assets](0206-explicit-static-gif-assets.md)

- [0207: Prepared file playback at an explicit output rate](0207-prepared-output-rate-file-playback.md)

- [0209: Native AAX parameter schema and registered control adoption](0209-aax-native-schema-control.md)

- [0211: Fixed hierarchical tree expansion and selection](0211-fixed-tree-expansion-selection.md)

- [0210: Explicit trusted VST3 catalogue ownership](0210-trusted-vst3-catalogues.md)

- [0214: Fixed readonly native table grid](0214-fixed-readonly-table-grid.md)

- [0213: Checked native AAX lifecycle and bounded global intent](0213-aax-checked-native-lifecycle.md)

- [0212: Converted buffered file playback](0212-converted-buffered-file-playback.md)

- [0217: Fixed-model virtual text list](0217-fixed-virtual-text-list.md)

- [0218: Native AAX short-note instrument adoption](0218-aax-native-short-note-instrument.md)

- [0216: Reclaimable native output ownership](0216-reclaimable-native-output.md)

- [0219: Initialized AAX transport capture](0219-aax-native-transport-capture.md)

- [0215: Scoped VST3 instance ownership](0215-scoped-vst3-instances.md)

- [0220: Explicit grid and measured native window consumption](0220-explicit-grid-measured-window.md)

- [0221: Explicit integer-rate long-file scheduling](0221-explicit-integer-rate-file-scheduling.md)

- [0222: Prepared native AAX audio bus mapping](0222-aax-prepared-native-bus-mapping.md)
  fixes explicit capacities/source-order mapping and actual registered Rust adoption.
- [0223: Opt-in standalone content constraints](0223-standalone-content-constraints.md)
  defines portable finite fitting, native bound restoration and resize/policy reentry.

- [0224: Indexed integer PCM seeking](0224-indexed-integer-pcm-seeking.md)

- [0225: Owned standalone window lifecycle](0225-owned-standalone-window-lifecycle.md)

- [0226: AAX generated short-MIDI output](0226-aax-generated-short-midi-output.md)

- [0227: Owned stopped VST3 state transactions](0227-owned-vst3-state-transactions.md)

- [0228: Output-clock file/capture processing](0228-output-clock-file-capture-processing.md)

- [0229: Explicit color preparation](0229-explicit-color-preparation.md)

- [0230: Prepared AAX initial and acknowledged dynamic timing](0230-aax-prepared-timing.md)

- [0231: Retire paired clocked callbacks before original-owner recovery](0231-reclaimable-clocked-duplex-ownership.md)

- [0232: Owned bounded SVG image import](0232-owned-svg-image-import.md)

- [0233: Owned VST3 controller parameters and timed DSP input](0233-owned-vst3-parameters.md)

- [0234: Checked native AAX editor ownership and automation](0234-aax-native-editor-ownership.md)

- [0235: Explicit low-cost streaming interpolation profiles](0235-low-cost-streaming-interpolation.md)

- [ADR0236: Explicit native semantic companion for custom editor content](0236-explicit-native-semantic-companion.md)

- [0237: One temporal output per input for selectable fractional delay](0237-selectable-causal-fractional-delay.md)

- [0238: Bounded selected-order Butterworth sections and verified specifications](0238-selected-order-butterworth-designs.md)

- [0239: FIR coefficient generation with explicit empirical and numerical policies](0239-fir-coefficient-design-family.md)

- [0242: Bounded Chebyshev I/II poles, zeros and edge selection](0242-selected-order-chebyshev-designs.md)

- [0241: Caller-owned contextual local font fallback](0241-contextual-local-font-fallback.md)

- [0243: Bounded elliptic poles, zeros and specification selection](0243-selected-order-elliptic-designs.md)

- [0240: Explicit native precision behind AAX float audio](0240-aax-native-precision.md)

- [0245: Bounded sampled equiripple FIR design](0245-sampled-equiripple-fir-designs.md)

- [0246: Prepared hosted VST3 events, parameters and block context](0246-hosted-vst3-events-context.md)

- [0247: Complete short MIDI AAX input and shared precision synthesis](0247-aax-short-midi-input.md)

- [0249: Canonical runtime native companion tables](0249-native-canonical-tables.md)

- [0248: Configured elliptic allpass halfband and oversampling](0248-configured-iir-halfband-design.md)

- [0250: Exact native client origin and balanced acquisition](0250-native-client-acquisition.md)

- [0251: Native peak and RMS ballistics](0251-native-peak-rms-ballistics.md)

- [0244: Explicit system fonts and immutable variation instances](0244-explicit-system-fonts-and-immutable-variations.md)

- [0252: First-order topology-preserving filters](0252-first-order-topology-preserving-filters.md)

- [0254: Raw short-MIDI AAX effects, auxiliary buses and one AUDIO admission](0254-aax-raw-effects-auxiliary.md)

- [0253: Exact-face wrapped mixed-direction paragraphs](0253-font-stack-paragraphs.md)

- [0255: Prepared panning curves and pair ramps](0255-prepared-panning-curves-and-pair-ramps.md)

- [0256: Paired dry/wet queues and causal latency](0256-paired-dry-wet-queues-and-causal-latency.md)

- [0257: Six-stage native modulated feedback phaser](0257-six-stage-native-modulated-feedback-phaser.md)

- [0258: Native nonlinear multimode ladder](0258-native-nonlinear-multimode-ladder.md)

- [0259: Explicit selected PostScript outlines and native-source consumption](0259-explicit-postscript-outlines.md)

- [0261: Exact LV2 native core and shared schema admission](0261-lv2-exact-origin-admission.md)

- [0260: Typed graph stages and shared-control mono ownership](0260-typed-chain-shared-mono-ownership.md)

- [0262: Explicit prepared AAX native meter taps](0262-aax-native-metering.md)

- [0263: Prepared interpolated peak estimates](0263-prepared-interpolated-peak-estimates.md)

- [0264: Prepared calibrated loudness and loudness range](0264-prepared-calibrated-loudness-range.md)

| [0265](0265-lv2-native-generated-output.md) | Complete bounded native LV2 short MIDI output | Implemented; fresh native replay and public workspace gate passed; no DAW, Linux or deadline claim |
| [0266](0266-scoped-native-floating-control.md) | Scoped native floating-control ownership | Implemented in `ravet-native-dsp`; native AArch64 and Rosetta x86_64 qualification |
| [0267](0267-explicit-packed-register-and-slice-family.md) | Explicit packed values and prepared vector slices | Implemented in `ravet-native-dsp`; native AArch64 and Rosetta x86_64 qualification |
| [0268](0268-hosted-vst3-graph-banks.md) | Hosted VST3 native 32/64 aux buses and owned Graph banks | Prototype; mock-provider, native fixture and actual Gain/Performance evidence, no DAW or deadline claim |
| [0269](0269-auv3-asynchronous-host-ownership.md) | Asynchronous AUv3 host ownership and joined main-bus rendering | Prototype; in-process native/Rust evidence, out-of-process rendering unvalidated |
| [0270](0270-native-convolution-schedules.md) | Native-precision zero-latency, uniform and nonuniform convolution banks | Implemented; offline numerical, allocation and Graph-consumer evidence only, no deadline claim |
| [0271](0271-hosted-vst3-editor-ownership.md) | Hosted VST3 editor ownership (macOS NSView) | Prototype; injected view, provider and actual Gain hidden-window evidence, no DAW, third-party editor or accessibility claim |
| [0272](0272-impulse-response-preparation-and-live-convolution-transitions.md) | Source-rate impulse-response preparation and live convolution transitions | Implemented; offline analytic, allocation and concurrency evidence only, no deadline claim |
| [0273](0273-hosted-vst3-graph-framework-events.md) | Framework events in hosted VST3 Graph nodes | Implemented; prototype with mock-provider and actual Gain/Performance evidence, no DAW, third-party plug-in or deadline claim |
| [0275](0275-convolution-transition-priming-and-latency-alignment.md) | History priming and latency alignment for live convolution transitions | Implemented; offline exact-reference, allocation and concurrency evidence only, no deadline claim |
