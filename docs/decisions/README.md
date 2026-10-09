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
