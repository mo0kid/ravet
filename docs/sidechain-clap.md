# Native sidechain CLAP reference

`ravet-sidechain-clap` exports `org.ravet.sidechain` / RAVET Sidechain. It has
stereo main input/output and one mono input-only auxiliary detector bus. Every
port advertises f64 support and requires a common sample size. The stateless
reference accepts native f32 or f64 per callback without converting samples.
When the detector magnitude is at least 0.1 and finite, main samples pass through;
otherwise outputs are zero. It has no parameters, saved state or custom editor.
This fixed gate is a bus-validation reference; envelope/frequency/dynamics design
is deliberately separate from its mapping contract.

```sh
python3 scripts/bundle-clap.py --plugin sidechain --profile release
CARGO_BUILD_JOBS=2 cargo run -p ravet-sidechain-clap --example validate-sidechain --locked -- \
  "target/clap/release/RAVET Sidechain.clap"
```

The local bundler builds only the selected package with two workers and creates
a target-directory artifact; macOS uses local ad-hoc signing. It does not install
into system directories. Load only the trusted reference built by this workspace.

The same ABI exercise runs through linked entries and built-library loading. It
checks all port metadata, native f32/f64 threshold timing and sub-f32 precision,
nonfinite detector closure, missing-sidechain whole-block rejection/silence,
zero tail, 6006 processing blocks and three activation/deactivation cycles at
44.1/48/96 kHz. Prepared callback/reset workloads make zero measured framework
heap operations. Two stateless bridges are prepared for precision selection,
with their independent adapter memory budgets; aggregate reference storage is
small for three inputs/two outputs but is not a single global framework budget.
The macOS arm64 REAPER 7.82 isolated test discovers stereo main plus mono
sidechain pins and routes detector input only from track channel three. Its
apply-FX render creates four-channel 96 kHz PCM24 output with an extra default
render tail. The first two channels pass four 0.3-second off/on regions: silence
off, +0.5/-0.25 on, maximum error 1.1920929e-7. Host-selected native precision
was not measured. This establishes the tested DAW auxiliary mapping/render;
physical devices, real-time deadlines and other hosts remain separate.

Use `tools/reaper-validation/prepare-sidechain.py <test-resource-directory>`
then run `tools/reaper-validation/sidechain.lua` through REAPER's ReaScript API
in an empty disposable project. A rerun may replace only its named synthetic
track. The script mutes hardware sends, sets an explicit project media path,
checks pin mapping, applies track FX to a synthetic item and records the exact
output filename in `ravet-validation/sidechain-host.log`. Validate that file:

```sh
python3 tools/reaper-validation/check-sidechain-render.py /explicit/rendered.wav
```

The checker examines actual PCM samples independently of REAPER's decaying live
peak display. Native UI access was unavailable in this run; the documented
command-line `.lua` runner executed the script. No visual editor or other-host
compatibility follows from the render.
