# 0291 — Generic MIDI instrument evidence and scoped C floating mode

Date: 5 November 2026. Status: implemented in the prototype export layer;
platform/host evidence remains scoped by the [receipt](../WORK_STATUS.md#release-010-instrument-evidence).

## Decision

Use an independent RAVET-owned one-shot click instrument to measure generic
MIDI I/O, CC response, nonzero/changing latency and finite audio tail. Preserve
the existing musical tutorial synth; deterministic fixed sample counts make
host PDC and callback/event timing failures observable without tuning/filter
or envelope tolerances. Notes retrigger one bounded voice; MIDI echo retains
its original offset and is independent of audio lookahead.

Wire the generic CLAP exporter to the existing prepared main-bus bridge's
bounded MIDI output. Advertise MIDI-dialect input/output ports from the product.
Host output refusal discards a suffix without replay; successful audio remains
valid. Apply cached controls before the activation latency notification and
check latency after active parameter flush as well as process. Existing VST3
hidden IMidiMapping controllers, AU callbacks, AAX nodes and LV2 Atom mappings
are retained and exercised through product/host fixtures.

Expose a separate product-library C function wrapping a synchronous callback
in ScopedDenormalMode, rather than exposing raw register values or an enter/exit
token that C could copy, restore twice or move across threads. Return before
callback dispatch on invalid arguments or an unsupported actual environment.
Normal return restores exact prior control and sticky flags; callbacks must
not unwind, longjmp or outlive the call. The existing version-1 product table
layout does not change. All five formats already enter DenormalProcessor around
product DSP; event translation outside that boundary is not covered.

## Evidence and limits

The [guide](../synth-plugin.md) and dated receipt distinguish portable/native
and actual-format tests, validators and named REAPER runs. The REAPER MIDI route
disconnects first-instance audio and uses zero first-instance latency; otherwise
serial host audio compensation would include a discarded/parallel audio path.
Format fixtures independently check output MIDI bytes and offsets, because
an audio render cannot rule out host MIDI-through. AAX SDK 2.8 has no tail
report here and LV2 has no tail port. No physical MIDI, arbitrary host,
Windows instrument, custom editor or deadline support follows from these checks.
