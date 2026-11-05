# Deterministic generic instrument

`examples/synth-plugin` exports **RAVET Evidence Synth** (`org.ravet.evidence-synth`)
through `export_plugin!`. Its ordinary `EvidenceSynth` processor has no format
or device code. The musical subtractive synth remains in
`examples/tutorials/src/synth.rs`; this example instead makes exact host timing
and routing observable. Both are RAVET-owned implementations.

A positive note-on retriggers one 64-sample bipolar click: the first sample is
`velocity / 4`, then its sign alternates. Notes overlap by retriggering one
voice; note-off does not truncate the one-shot. It is a test instrument, with
no editor or audio input and stereo f32 output. The fixed sample counts do not
change with sample rate. Every output sample is initialized.

| Permanent ID/key | Default | Behaviour |
|---|---|---|
| 1 / `cc_response` | On | MIDI CC 1 on any channel scales the current click by `value / 127`; Off ignores that scale. The initial/reset CC value is 127. |
| 2 / `lookahead` | On | Delay audio by 32 samples and report latency 32; Off reports 0. A change clears delayed audio and requests the adapter's host timing update. |

Notes and validated short MIDI are echoed unchanged at their original offsets,
including CC and note-off. MIDI echo is **not audio-delay compensated**. Prepared
output capacity is bounded, processing/reset allocate nothing, and malformed
known parameter values or insufficient supplied output capacity reject before
DSP/control mutation. An absent output-event sink disables echo. Reset clears
the click/delay and restores CC 1 while retaining both parameter values. State
uses the generic parameter envelope. The conservative declared tail is 64 plus
latency samples (96 by default), with exact silence after the one-shot drains.

## Build and check

```sh
cargo test -p ravet-synth-plugin --locked
python3 scripts/bundle-plugin.py --package ravet-synth-plugin \
  --formats clap,vst3,au,aax,lv2 --validate
```

Use the [SDK settings](bundle-plugin.md) for formats needing external SDKs;
AU/AAX require their supported build platforms. The default output is
`target/plugins/debug/ravet-synth-plugin.<format>`. The native-table test checks
exact samples, MIDI bytes/offsets, CC, finite tail, reset and latency-change
acknowledgement. The CLAP test checks real ABI rendering, ports, latency/tail and
MIDI output, including allocation counts. VST3, AU, AAX and LV2 product fixtures also
check this instrument's actual format entry points. The VST3 fixture loads the real product library through the adapter classes and checks hidden CC mapping, preserved note IDs/output offsets, exact audio/tail and latency notification; the official validator separately loads the final module. Its independent adapter mock remains a regression check.

## REAPER fixture

```sh
python3 tools/reaper-validation/instrument.py --format clap \
  --bundle target/plugins/debug/ravet-synth-plugin.clap --reaper /path/to/reaper
# Use --format vst3 or lv2 and that format's bundle for the other cases.
python3 -m unittest discover -s tools/reaper-validation -p 'test*.py'
```

The sibling of the [Delay harness](host-validation.md) reuses its new work
folder, private configuration, scan paths, process timeout and normal-resource
snapshot guard. It starts REAPER with that work folder as its current directory.
Only native macOS/Linux CLAP, VST3 and LV2 runs are provided by this sibling;
its `--windows-root` route is refused. It never copies a licence fixture or
normal REAPER configuration. On macOS its private app uses the distinct bundle
identifier `org.ravet.reaper-instrument-harness`.

The generated MIDI item contains notes at 0.5 and 1 seconds (velocity 127),
with CC 1 = 64 at 0.9 seconds. Four two-second stereo 48 kHz renders check:

- `pdc`: 32 reported samples, with REAPER PDC aligning both click onsets to
  samples 24,000 and 48,000.
- `zero`: latency changed to 0, with the same aligned onsets.
- `cc_off`: latency 32, CC response disabled; both amplitudes are 0.25.
- `echo`: first-instance audio pins disconnected and its latency 0; a second
  instance receives MIDI and produces the expected CC-scaled clicks. Its
  latency is 32. No first-instance audio contributes to this render.

Every rendered sample is checked (tolerance 1e-5 for host PCM rounding), including
polarity, amplitude and silence after exactly 64 samples. The second click is
`0.25 * 64 / 127` except in `cc_off`. The script also checks REAPER's reported
first-instance PDC field. Two serial instruments with both lookaheads enabled
would cause REAPER to compensate two audio delays despite undelayed MIDI echo;
the fixture deliberately avoids that ambiguous timing setup. Format fixture
checks independently verify generated MIDI bytes and offsets; the REAPER render
alone does not inspect MIDI packet bytes or exclude a host MIDI-through route.

See the [dated receipt](WORK_STATUS.md#release-010-instrument-evidence) and
[support matrix](support-matrix.md#instrument-evidence). Linux REAPER 7.82 passes
CLAP/VST3/LV2. macOS host attempts are recorded as startup failures, with no
instrument render evidence. Validators and offline tests do not establish
DAW/device support, audible quality, polyphony or real-time deadlines. AAX SDK
2.8 exposes no tail report in this adapter, and LV2 has no exported tail port;
the shared processor's finite tail is measured in their audio tests.
