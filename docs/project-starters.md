# Independent offline project starter

The explicit generator creates one offline gain starter in a new directory
outside the RAVET repository. Its parent must already exist; an existing
destination rejects without overwriting files. Consumer projects remain separate
workspaces and are not added to RAVET's Cargo members.

```sh
python3 scripts/new-project.py \
  --destination ../my-audio-project \
  --name my-audio-project \
  --license GPL-3.0-only
```

Package names use 1..64 lowercase ASCII Cargo characters. The manifest licence
is an explicit caller choice and does not change dependency licensing. The
generator writes a publish-disabled independent manifest, local absolute RAVET
and public reference-gain dependencies, source, README and target ignore rule.
It runs no Cargo command, device, host, window or network action by itself.

From the new directory, generate its lockfile and run the synthetic fixture:

```sh
cargo generate-lockfile --offline
CARGO_BUILD_JOBS=2 cargo run --locked
```

The fixture prepares borrowed stereo audio and a sample-timed gain event,
processes four frames, verifies known output and deactivates the processor.
Keep its generated Cargo.lock with the consumer project. Moving RAVET requires
updating absolute dependency paths. Replace the reference processor as consumer
DSP grows. Format exports, MIDI/device/UI integration, product wrappers/assets,
release signing and installer templates are subsequent consumer work.

Generator fixtures verify repository exclusion, manifest independence, invalid
arguments and preservation of existing destinations. A generated project outside
RAVET was locked/built/run offline on macOS and passed its four-frame fixture.
Broader starter types/platforms and production distribution remain pending. See
[decision 0036](decisions/0036-independent-project-starters.md).

When extending a starter into a standalone UI project, use the default
`ravet-webview::StandaloneWindow` Audio/MIDI settings popup. It is enabled unless
explicitly disabled with `StandaloneOptions { io_settings: false, ..Default::default() }`.
The consumer owns its device configuration and audio/MIDI routing; see the
[standalone window contract](native-editor.md#default-standalone-audiomidi-settings).
The existing offline starter remains headless and creates no window.
