# 0122: Global parameter modulation events

Status: implemented portable routing and prototype CLAP mappings.

## Decision

Keep a finite physical-unit `ParameterModulation { id, amount }` separate from
base parameter values. Amount is a replacement modulation signal, not a delta
added to the preceding modulation amount. Consumer DSP owns combining it with
base values, clipping, smoothing, reset and persistence. Raw events do not change
state, synthesize automation/recording or infer per-note scope. Parameter graph
filters include matching modulation; performance filters exclude it. Complete
worker queues validate finite amounts and retain their order/identity.

Default CLAP input behavior continues to ignore modulation. Explicit
`with_global_parameter_modulation(true)` on the translator or prepared effect,
main/auxiliary bridge maps global native amounts to finite f32, validates all
input timing before DSP and normalizes parameter-flush offsets to zero. Scoped
native modulation remains ignored; disabled kinds still count toward timing and
capacity checks. Generated output maps portable amounts to global native f64
without a MIDI port, preserving whole-stream preflight and host prefix refusal.

Exporters must advertise matching native modulation flags and processors must
implement their policy. The bridge does not synthesize metadata flags, saved
base state, native parameter validation or a modulation matrix. CLAP's
[event contract](https://github.com/free-audio/clap/blob/main/include/clap/events.h)
and [parameter extension](https://github.com/free-audio/clap/blob/main/include/clap/ext/params.h)
define native global/scoped behavior.

## Evidence and limits

Portable tests cover finite signed amounts, input/output validation including
zero-frame flush, graph filtering and complete queue rejection/round trips.
CLAP tests cover opt-in/default behavior, ignored scoped events, overflow/nonfinite
rejection, same-frame order, no-port generated output/host prefix refusal and
heap-free translation/processing. A custom test consumer keeps saved base values
separate through process/flush and confirms invalid input never enters DSP.
No reference plug-in modulation support, host/device/deadline, per-note/native
hosting or other-format capability is established by these offline fixtures.
