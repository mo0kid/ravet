# Prepared hosted VST3 events and block context

The caller-linked processing companion extends the main f32 instance owner with
explicit stopped event-bus selection, copied MIDI controller assignments, typed
owned input events, bounded output events and normalized output parameters.
It retains the exact ProviderV2 table identity; state and parameter companions
remain separate. No SDK interface or plug-in-owned text pointer escapes into Rust.
Native code and loaded plug-ins remain trusted. Hosted f64, auxiliary audio
and Graph ownership are a separate companion; NSView editors use the separate
[editor companion](vst3-hosting.md#hosted-editors-macos-nsview).

## Prepare on the control thread

Acquire the native processing table and construct its unsafe trusted Provider
alongside the instance/parameter providers. Keep their executable code loaded
through retirement, and through process exit after exceptional native pinning.
Call `refresh_parameters` first, including for a controllerless component with
an empty parameter catalogue. Then call `refresh_processing` with explicit u16
input/output selection masks. Unselected event buses remain inactive.

The owned Catalogue records all copied bus names, channels, main/auxiliary kind
and default-active flag. It independently records optional IMidiMapping and
IProcessContextRequirements availability. Selected buses support 1..16 channels;
there are at most 16 event buses per direction. Mapping inspection runs only on
CONTROL, and copies all selected channel/controller assignments. Later refresh
failure leaves earlier owned catalogues intact and makes the new processing
metadata unavailable until a complete successful refresh.

Prepare the exact companion with a main-f32 Profile: 0-input mono/stereo
instrument or equal-input/output mono/stereo effect, positive sample rate up to
768000 and maximum 1..65536 frames. Preparation allocates native queue families
and activates selected event buses. The native event-pool keeper has a 64 MiB
allocation cap per containing module core; the measured macOS arm64 layout admits
six simultaneously processing-prepared children, each with about 10.1 MB of event
pool storage plus a separate parameter pool. Six event pools and their keeper
occupy 60,598,752 bytes. Each child has sixteen retained batch slots; that is
independent of the number of simultaneously prepared children. Start the instance and lend its sole AUDIO
capability through `with_audio`; joined workers follow the existing scoped owner
contract. CONTROL owners and processing Provider are neither Send nor Sync.

## Prepare storage and input

InputEvents allocates initialized event records and aligned UTF-16 payload storage
on CONTROL. Fixed limits are 2048 events and 65536 payload bytes. It copies all
nine SDK input kinds: note on/off, poly pressure, opaque counted SysEx data,
note-expression scalar/integer/text, chord and scale. Integer expressions preserve
u64, input note IDs allow -1 or nonnegative values, and flags retain 0xc001.
Unknown and output-only legacy controller event kinds are not input routes.

Push performs full scalar/order/payload/capacity validation before mutation.
UTF-8 text is converted into prepared storage without callback allocation; NUL,
malformed counted output UTF-16 and overlong text reject. SDK pools independently
budget required internal text terminators. The revised Rust admission also tracks
that separate 65536-byte native budget, including alignment and two bytes per text
terminator, before mutating owned storage. Wire payload layout and its selected
capacity stay unchanged; a two-byte wire text can still fit a two-byte carrier. SysEx payloads preserve every byte;
this carrier does not parse or add/remove MIDI framing. Clearing storage reuses
its fixed allocation. Sample offsets are nondecreasing, below active frames, or
exactly zero for a zero-frame block.

InputPoints can stage normalized explicit points and mapped short MIDI controls.
Copied IMidiMapping assignments resolve CC0..127, channel pressure128 and pitch
bend129 using the actual SDK IDs. CC/pressure normalize by127 and bend by16383.
Bus/channel identities are explicit; unsupported/unassigned messages return an
error. Same-ID same-offset points are conflicts. Points are never sorted, merged
or silently dropped. Native input admission additionally checks writable IDs. Read-only and hidden
parameters cannot be externally driven; a selected MIDI assignment to an unknown,
read-only or hidden ID refuses the entire refresh. No partial assignment catalogue
is published. This policy follows the inspected external SDK parameter flags.

Context preserves exact signed sample/system/continuous positions and independent
availability flags. Present musical positions, cycle and tempo must be finite;
sample rate matches preparation. Absent fields are ignored and native staging
supplies zero. Copied context requirements are requests, not permission to invent
unavailable transport. Do not round exact integer positions through f64.

## Publish complete outputs

OutputStorage allocates fixed event, aligned payload, queue-ID and point arrays.
Use `OutputStorage::maximum()` for the full 2048/65536/64/2048 limits unless the
consumer has separately established every possible output bound. Smaller backing
is a deliberate limit: a legitimate larger plug-in batch is discovered after DSP,
silences active audio and requires retiring/recreating the child.
`Audio::process_events` accepts audio slices, points, InputEvents and optional
Context, then returns an immutable Output view borrowing that storage. Rust
prevents a subsequent render into the same storage while the view remains live.
The method validates the complete native wire reply before exposing any slices.
All ten SDK output event kinds are admitted, including legacy MIDI controller
output and reserved negative SDK output note IDs. No portable unsigned identity
remapping occurs. Output queue IDs preserve empty queues and known read-only
parameter IDs; point order and normalized values remain independently checked.

Native processing reserves both queue families before activation or input/DSP
mutation. A pure refusal preserves the old audio/output batch. After DSP starts,
native return failure, throw, retained-list protocol fault or insufficient output
capacity invalidates processing, silences valid active audio prefixes, preserves
backing tails and publishes no partial event/parameter batch. DSP history may
have advanced; no rollback is implied. Stop and retire/recreate the failed child.

Output text must provide readable aligned non-null storage including a final NUL,
even for zero-length text. Output parameter offsets must increase strictly within
each ID queue; repeated or earlier offsets are protocol faults, not sorted or
replaced. These are explicit admission rules, and violations fault the child.

Successful CONTROL replies must carry the exact refresh/prepare stage and no
AUDIO publication/history/list facts. Failed operations preserve independent
native diagnostics; CONTROL exception provenance remains incomplete.

Report exposes original returned SDK status only when it was actually returned.
Throw, invalidation, possible history advancement, complete publication and
individual list faults remain independent facts. A malformed wire reply retires
the Rust processing capability even when its flags do not establish dispatch;
a valid pre-dispatch native refusal permits retry. Controller output changes
require explicit correct-thread consumer adoption, never implicit AUDIO edits.

## Validation boundary

The initial frozen native SDK interface fixtures check all event kinds,
Unicode/binary copying, context, atomic paired publication and local/late refusals.
The separate revised native fixtures execute 10000 rich calls in both normal
and fast profiles, with nine input and ten output event kinds, all thirteen
context presence bits, copied binary/UTF16 payloads, parameter queues and
zero/variable frames. Measured C++ `operator new/delete` counts are zero; these
do not measure malloc/calloc/realloc or arbitrary plug-in allocation. Seventy
fixture build/run commands and two compiler probes pass. The maintained CMake
build separately executes twenty-two registered normal/fast fixtures. Actual
compiler dependency files, external SDK sources and library hashes are bound to
the receipts; reused SDK libraries do not acquire a new compilation identity.

The initial Rust wire callbacks passed 24 unit tests and 17 doctests in debug and
release, strict lint/Rustdoc/format and a separately scoped mock-only heap check.
The separate revised Rust owner passes 37 unit tests and 17 doctests in debug
and release, strict lint/Rustdoc/format. Tests capture every input wire kind and
argument, validate complete rich output batches, distinguish malformed reports,
check native terminator budgets and cumulative refusal preservation, and reject
malformed CONTROL success before publishing metadata. The mock wire callback
has a separate 10000-call Rust heap measurement. Thirteen wire layouts and 121
literal field offsets agree with an independently compiled C11 check on the
measured 64-bit profile. These are controlled-fixture results, separate from
plug-in and host execution.

An actual macOS arm64 Rust-to-native SDK-loader consumer passes Gain mono/stereo
and audible Performance instrument checks in debug and release against freshly
built VST3 module artifacts, including a separate seven-command replay of the
revised Rust owner. The reused native runtime archive has byte-identical compiled
first-party dependencies to the revised native tree. Those modules use frozen
W241 C++ composition and the
independently qualified canonical W227-243 Rust archives. Gain timed normalized
controls, 5 ms smoothing and zero-frame retargeting match independent expected
audio; instrument note/expression/Level/context matches a separate closed-form
signal with maximum absolute error approximately 5.596e-11. Source, executable,
SDK compiler dependency and archive hashes are bound to the consumer receipts.
This is in-process evidence, separate from actual DAW support.

Sibling-instance metadata protection and CONTROL exception provenance are
described below. Hosted native f64, auxiliary buses and Graph ownership use the
separate [hosted audio companion](vst3-hosting.md#hosted-native-3264-aux-buses-and-owned-graph-banks).
Other platforms and real-time deadline evidence remain unestablished.

## Core-owned controller callbacks and exact provenance

These native ownership rules apply to every paired companion. Authored SDK
fixtures establish no real plug-in/DAW validation, memory isolation,
malloc/foreign SDK heap or deadline support.

The containing native core owns a fixed 256-slot ControllerHandlerKeepers value. Each handler has one permanent keeper reference; foreign AddRef/Release only change an always-lock-free counter and never destroy the callback on AUDIO. Original MAIN under admitted exclusive CONTROL reclaims only detached retired handlers with exactly that permanent reference. Instance attachment lasts through destruction, so public retire() does not permit premature reclamation. The core's full object covers fixed keeper/sparse slot storage, while actual allocated handler objects are also included in the numeric ownership snapshot. Counting existing child handler ranges again conservatively, the exact maximum is 11,077 + 256 = 11,333 nonempty numeric extents within the unchanged 12,288 capacity. Child/AUDIO bounds remain 256 each, state/parameter/processing keeper bounds unchanged. A core can refuse creation because all 256 handler slots are retained even when it has no live child; releasing those references followed by MAIN reclaim permits healthy creation again.

Canonical creation allocates a local wrapper/Instance and reserves a handler, inserts the provisional wrapper into the core registry, then publishes full numeric ownership before any foreign create/initialize/setComponentHandler callback. The successful child count and caller output publish only after all foreign initialization succeeds. Failed cleanup retains provisional registry membership through foreign retirement/destructor work, and removes it afterward. Thus an initial synchronous setter QI already sees whole provisional child/Instance, core, facade, keeper, handler and immutable class catalogue ownership. Allocation/capacity failures before foreign work leave output/count unchanged.

Handler QI checks the whole 16-byte selector and aligned pointer destination for arithmetic overflow, null, owned aliases and mutual overlap before dereferencing selector or writing destination. Handler self, containing keeper, full current siblings/scratch/catalogue capacities and immutable metadata have no destination exemption. Active original-MAIN QI inside exact CONTROL uses that ownership and refreshes the complete numeric core view immediately before checks; later CONTROL capacity changes cannot bypass the snapshot. Off-thread or retired/frozen QI uses a one-attempt Read admission held through all selector/refcount/output operations and refuses during CONTROL, including same-thread retired reentry. No waiting/unlocked fallback is introduced. Active restart remains a bounded atomic mailbox with no implicit recovery; retired restart refuses. Edit gestures remain unsupported. SDK refcount/destructor methods must not throw, and pointer liveness remains the caller's obligation.

After attempted setComponentHandler(nullptr), a retired handler still carrying foreign references pins its exact containing CoreRegistry reservation before further foreign terminate/interface release/module teardown. This sticky pin preserves actual host/image/core protection through facade retirement and reopen; shared_ptr lifetime alone would not prevent moduleExit. Healthy controllers that clear their normal reference do not force retention pinning. Foreign worker final release returns to the permanent keeper reference, and original MAIN later deletes the storage. Old handlers borrow only immutable/core-owned numeric protection and exact-core callbacks; they never dereference a retired Instance/facade. Their original numeric protection remains effective after core reopen, while their retired restart remains unavailable. A pinned image stays pinned even after MAIN reclaims a handler. Foreign SDK allocations and allocator/shared_ptr control blocks are not advertised as owned range coverage.

Unclassified parameter/processing exceptions are local metadata refusal, failure17/catalogue-invalid16, native_result0, caller function stage and no ATTEMPTED/RETURNED/THROWN. Independently measured INVALIDATED is retained when the current owner actually lost compatibility. Explicit bad_alloc/capacity/lifecycle and attributed malformed metadata retain their existing classification. Wrapped actual SDK exceptions retain original Instance stage100..120, attempted|thrown/native0, and actual returned failures retain attempted|returned/exact native status. No wire layout/domain/history changes are introduced. The native fixture distinguishes an unattributed first-party throw from wrapped SDK throw, returned4242 and measured invalidation; root-owned safe Rust validation remains separate.

The dedicated fixture uses actual SDK interfaces and a first-party substituted loaded-module provider. It qualifies early healthy setter QI; self/keeper/sibling/class/full scratch/selector alias preservation; original-MAIN active CONTROL success/off-thread refusal; failed create and throwing/refusing handler setup cleanup; child/facade retirement, exact retained-core reopen and stale restart refusal; worker final release with zero measured C++delete followed by MAIN reclaim; actual 256 retired owners/257 refusal before foreign callback and release recovery; and 10,000 real QI attempts versus 2,000 CONTROL publications with balanced references. The concurrent fixture checks legal successful/refused outcomes and no reference leak, while the inherited deterministic admission rendezvous separately demonstrates exclusion through a held destination write. It does not prove a scheduler deadline or that a particular race interleaving occurs.

The complete qualification is a fresh exact-source normal/fast suite, external SDK/version/compiler/dependency/archive before-after hashes and actual CMake fixture registration/build/run. Original R2/R3 guards and receipts remain unchanged; R4's initial indentation-warning negative receipt and exact source are preserved separately rather than relabelled as passing.
