![RAVET logo](media/ravet_LOGO.svg)

# RAVET

**Rust Audio Visual Engineering Toolkit** — a Rust framework in development
for audio plug-ins and standalone audio applications.

Version **0.0.1** is an early source preview with provisional APIs and scoped
validation. See the [release notes](docs/releases/0.0.1.md) for capabilities,
checks and publication limits.

The first working milestone is a portable mono/stereo gain processor with
sample-timed automation, linear smoothing, and deterministic offline tests.
A loadable CLAP gain prototype is now implemented and tested on macOS arm64.
Reusable parameter/state primitives are implemented, with complete CLAP
[gain/bypass recall](docs/session-state.md) and a prototype macOS
[embedded editor](docs/native-editor.md). Additional
[VST3](docs/vst3.md), [AUv2](docs/audio-unit.md), [AAX](docs/aax.md) and
[LV2](docs/lv2.md) gain references and explicit [audio-device](docs/audio-devices.md)
adapters have scoped prototype evidence. See the catalogue for their limits and
the broader DSP/state/file/MIDI/OSC components.

Start with the [documentation index](docs/README.md) for setup, architecture,
API reference, and the [feature catalogue](docs/FEATURES.md). Documentation is
updated alongside implementation; [maintenance](docs/maintenance.md) describes
the registry, generation, and continuous checks.

## Run the first example

From the workspace root:

```sh
cargo run -p ravet-gain
```

This runs an eight-frame stereo fixture, applies gain events at frames 0 and 4,
checks both channels against known outputs, and prints the result. It does not
open an audio device. The reusable processor is in
[`examples/gain/src/lib.rs`](examples/gain/src/lib.rs). The CLAP adapter uses the
same `Processor` implementation as the explicit device adapters.

For MIDI synth and effect WAV renders, MPE expression and MIDI 2.0 packet
walkthroughs, see [Small examples](docs/examples.md).

## Build and exercise the CLAP plug-in

```sh
python3 scripts/bundle-clap.py
cargo run -p ravet-clap-smoke --locked -- "target/clap/release/RAVET Gain.clap"
```

The bundle command requires Python 3.11+ and builds an optimized plug-in into
Cargo's target directory. On macOS it creates an ad hoc signed `.clap` bundle.
`--profile debug` selects a debug build. It does not install the plug-in into a
system or user plug-in folder. The printed path reflects `CARGO_TARGET_DIR`.
Windows/Linux packaging paths exist but have not been exercised.

The smoke host loads the built bundle and checks mono/stereo ports, gain
parameter metadata/text, sample-timed automation, inactive/active parameter
flush, reset, session state recall, and six activation cycles. Audio processing runs on a separate
thread. Use this command only with the trusted RAVET bundle built here.

With CLAP validator installed, also run:

```sh
clap-validator validate "target/clap/release/RAVET Gain.clap"
```

Current validator, audio smoke-host and native editor probe evidence is recorded
in [work status](docs/WORK_STATUS.md). Native editor checks use a Cocoa main loop
and do not establish DAW compatibility. See the
[CLAP backend decision](docs/decisions/0002-clap-backend.md) for adapter limits.

## Project creation

[RAVET Foundry](docs/foundry.md) creates and builds independent offline, application
and effect/instrument plug-in projects using the RAVET UI. The guide includes
source-build instructions for Foundry on macOS, Windows and Linux, and validation
limits for generated targets.

## Workspace

The [generated package list](docs/FEATURES.md#workspace-packages) follows Cargo
workspace manifests. See [architecture](docs/architecture.md) for crate boundaries
and [feature status](docs/FEATURES.md) for implementation scope.

Portable processing stays independent of host, device and editor backends;
optional codecs and drawing/text backends have their own dependencies and notices.
Foundation uses pure-Rust libm for no_std parameter math. Portable processing
crates forbid unsafe code. The CLAP adapter also forbids unsafe code and uses Clack.
The export/host packages isolate ABI export, dynamic loading, and raw test
fixtures behind narrow unsafe allowances. The explicit macOS WebKit binding
is isolated from portable DSP; Xcode is required for native workspace builds. Gain tests use a development-only
allocation counter.

## Processing contract

Prepare a processor with a sample rate, channel layout, maximum frame count,
and event capacity. Processing accepts variable block sizes, including zero
frames. Audio is planar f32, with distinct constructors for in-place and
separate input/output storage. Views expose only active frames and preserve
unused backing tails.

Input events must fit the prepared capacity and arrive in nondecreasing frame
order. Same-frame events keep their input order. Events take effect before
the sample at their offset; an offset equal to a nonempty block's size is
invalid and belongs at offset zero of the next block. A zero-frame block
accepts offset-zero events without advancing smoothing. Invalid blocks are
rejected as a whole and their active outputs are silenced.

The gain parameter uses the stable `GAIN_ID`, a physical amplitude multiplier
from 0 to 4, with 1 representing unity. Finite out-of-range values are clamped;
nonfinite parameter values reject the block. Unknown IDs and valid note
events are ignored by gain. Default smoothing is 5 ms. Set it to zero with
`GainProcessor::new(0.0)` for immediate automation. Smoothing advances once per
frame across both channels and preserves output when block partitions change.

See the executable API example in the gain crate's Rust documentation and the
[processing decision](docs/decisions/0001-portable-processing.md) for lifecycle,
reset, event, and failure semantics. Public APIs are provisional.

## Development checks

```sh
python3 scripts/check-docs.py
cargo fmt --all -- --check
cargo check --workspace --locked
cargo test --workspace --locked
cargo clippy --workspace --all-targets --locked -- -D warnings
RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps --locked
```

See [Contributing](CONTRIBUTING.md) for documentation updates and checks appropriate
to a change. The [documentation workflow](.github/workflows/documentation.yml)
checks the catalogue and builds API reference artifacts when pushed to GitHub.

The allocation tests measure only the processing thread, including repeated
callbacks, buffer view construction, events, smoothing, resets, and faults.
They check both allocation and deallocation and include a concurrently
allocating worker. CLAP tests also measure 10,000 callbacks through the host ABI
and 10,000 in-place bridge callbacks, including malformed-event paths. The
counter replaces the allocator only in the test binary that uses it;
preparation and assertions happen outside measured callbacks.

Tested development environment: Rust/Cargo 1.99.0 on macOS 27.0.1, arm64.
This is an observed toolchain, not a tested minimum Rust version or a platform
support promise. CLAP is exercised in the official validator and the RAVET
smoke host, with scoped macOS REAPER gain validation. A silent UMC1820 output
probe exercised standalone callbacks; audible output and deadlines remain
unvalidated. Other operating systems lack equivalent runtime evidence.
Offline tests do not establish real-time deadlines.

## Project direction

[Work status](docs/WORK_STATUS.md) records the implementation and next work.
The [architecture guide](docs/architecture.md) and
[decision records](docs/decisions/README.md) describe the design and its reasons.

Extended processing, MIDI short messages and standalone output have scoped
implementations and validation. The [feature catalogue](docs/FEATURES.md)
separates these capabilities from planned integrations. VST2 is outside the
project's scope. Packages remain unpublished.


## Licensing

RAVET uses **GPLv3 plus separate commercial licensing**. The repository's
open-source edition is `GPL-3.0-only`; see [LICENSE](LICENSE).

The approved commercial launch prices are **Starter: free** (company revenue up
to $50,000), **Indie: $199 per developer per year** (above $50,000, up to
$500,000), and **Pro: $699 per developer per year** (above $500,000, unlimited).
Prices are in USD excluding applicable taxes. Paid licences include twelve months
of updates and permanent commercial rights to the versions received; renewal is
optional for continued building and distribution with those versions. All tiers
include all supported features, unlimited products, and no royalties.

The final commercial agreement is pending; this README does not grant commercial
permissions. These thresholds do not restrict GPL-compliant use.

See [LICENSING.md](LICENSING.md) for the current policy, contribution and
dependency requirements, and unresolved commercial terms.

Run the [live documentation portal](docs/documentation-site.md) for searchable guides, feature limits and the workspace API reference.
