# 0209: Native AAX schema/control adoption and SDK parameter ownership

Status: prototype; normal public package passes14 underlying/native SDK
contracts, actual registered mono/stereo Gain factory/algorithm contracts,
export/SDK-version probe and four Python packaging tests. Initial stock mono
lifecycle, complete AAX state and real Pro Tools acceptance remain separate.

An external AAX wrapper's normalized parameter objects do not preserve RAVET's
immutable native schema, exact IDs, physical text or declared discrete roles.
The optional C++17 `RavetAaxParameters.hpp` consumer owns a shared immutable native
cache, validates all descriptors before registration and stages actual Avid SDK
parameter objects. No external SDK implementation is vendored.

CONTROL owns construction, metadata/text formatting, staging, registration and
retirement. Pure prepared scalar preflight uses the immutable cache and does not
borrow DSP or mutate SDK queues. Accepted finite normalized double values narrow
explicitly to Foundation f32, map through native physical ranges/steps/log rules,
then return canonical widened f32 normalized values. This is a precision limit,
not lossless double control. Full native uint32 IDs become exact `p` plus uppercase
hexadecimal without leading zeroes; aliases/suffixes/lowercase are rejected.
Generic schema can represent `pFFFFFFFF`; the concrete Steinberg-backed Gain
consumer additionally rejects that reserved VST3 ID. A native bypass is a unique
writable Boolean with its own exact ID; simulated `Byp` is never an alias.

Metadata must have finite ordered ranges/defaults, recognized roles/scale, bounded
intervals, complete UTF8 names/units and zero padding. Actual SDK objects use
normalized linear storage plus a native physical display adapter. Continuous
objects retain SDK128 steps; discrete intervals declare intervals+1. Read-only
objects suppress automation, and owned setter preflight rejects their updates.
Borrowed SDK object pointers can still be mutated through SDK interfaces; no
arbitrary direct-object read-only enforcement is promised. Physical f32 shortest
text has no implicit unit suffix; Boolean uses On/Off. Parsing accepts complete
ASCII at most128 bytes with surrounding whitespace/single plus and native snapping.
Known malformed IDs/values/text/capacity preserve outputs before SDK mutation.
`AAX_IString::Set` is void and may allocate; arbitrary foreign setter failures do
not have an atomic rollback guarantee.

SDK parameter registration has a critical partial-ownership boundary:
AddParameter sets a delegate, appends its owning vector, then inserts its ID map.
A map allocation exception can leave an adopted pointer absent from ID lookup.
RAVET releases staged ownership before calling AddParameter, determines adoption
using public NumParameters/GetParameter(index), and retires adopted objects only
through public vector-based RemoveParameter. Failure clears its accepted prefix;
untransferred staged objects remain owned. Known complete shape/ID faults precede
all insertion. The caller must preserve exact owned staged object definitions and
provide an exclusively serialized empty manager. SDK notifications already sent
have no rollback. Cloned physical display adapters retain shared immutable cache
ownership after the facade retires, until the last SDK borrower is destroyed.
Actual allocation faults verify vector-before-map behavior and exactly-once cleanup.

Detached helper tests do not establish wrapper adoption. The public Gain reference
uses an owned AAXWrapper_Parameters subclass and a reusable borrowed descriptor
forwarder. Only the externally installed aaxwrapper.cpp translation unit is
compiled under a renamed GetEffectDescriptions entry in a separate object target;
its source is unchanged. The SDK ACF caller resolves the owned registration entry.
The forwarder defers the initial Create_EffectParameters AddProcPtr until exact
public AddEffect identity selects mono or stereo, then forwards the actual host
status and underlying descriptor. Creation order never selects identity. Unknown
IDs retain the original callback. Registration timing changes; earlier host calls
have no rollback. Readiness is consumed before host invocation so refused or
reentrant submissions cannot retry. SDK aggregate registration preserves its
existing last-variant result, which may mask an earlier refused descriptor; success
is not a new promise that every effect was registered.

The concrete consumer stages the complete native set before stock EffectInit,
checks exact existing SDK IDs/count, replaces stock objects and adopts the native
set. Repeated known initialization rejects before existing manager/readiness
mutation. Owned creation/initialization contains allocation exceptions; successful
normal stock statuses still forward unchanged. Exact writable scalar preflight
precedes stock automation requests and accepted block-start UpdateParameter queues.
SetParameterNormalizedValue remains a host request; host acknowledgement is
separate. Complete AAX chunks/state, programs, audio/aux mapping, MIDI/transport,
latency/tail/output, checked backend lifecycle and custom editor remain outstanding.

Actual binary evidence captures the registered Gain factories and native algorithm
callbacks, creates the concrete data model, checks native SDK objects/text/scalar
preflight, and renders mono/stereo Gain and bypass. The ordinary stock realtime
EffectInit/ResetFieldData sequence currently renders stereo but leaves mono
rejected/silent: initial preparation precedes mono descriptor bus arrangement.
The fixture explicitly records this unresolved lifecycle gap, then uses public
enter-offline and exit-offline notifications to reprepare the selected shape and
verify positive offline and realtime rendering at the inherited1024-frame maximum.
No protected setup access, private-layout cast or inferred processing-ready state
is used. This is schema/control adoption with explicit synthetic setup, not checked
AAX lifecycle completeness or Pro Tools/AudioSuite/signing acceptance. Fast-math
receipt covers native helper preflight only, not the whole stock wrapper.


Uninitialize invalidates consumer readiness before SDK parameter retirement, so
later owned callbacks reject before dereferencing removed objects. A fresh
EffectInit can adopt the native set again after cleanup. Failed initialization
leaves the consumer unready; any retained SDK objects require Uninitialize before
another initialization attempt. This is parameter lifetime recovery, separate
from the stock algorithm's initial mono processing gap.
