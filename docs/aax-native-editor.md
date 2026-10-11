# Native AAX custom editor

The optional macOS native Gain AAX profile registers its own GUI factory at each
public mono/stereo descriptor. The default native and stock profiles remain
headless. This is a local prototype using external AAX and VST3 SDKs; its native
view fixtures do not establish Pro Tools acceptance, automation recording, PACE
signing, accessibility, another platform or a callback deadline.

Build the optional profile without installing it:

```sh
python3 scripts/bundle-aax.py --editor --profile native --plugin gain \
  --vst3-sdk /path/to/vst3 --aax-sdk /path/to/AAX
```

This selects the existing `ravet-gain-native/native-editor` feature and a separate
`target/aax/editor/release/` build/bundle. `--editor` rejects stock and Performance
profiles before commands. The Gain model chooses its cache from its actual native
audio companion; unrelated standalone exports are not inferred to share callback
provenance. Parameter IDs, state and the default f32 algorithm remain unchanged.
Explicit [internal precision selection](aax-precision.md) gives the editor the
originating f64 cache while its control/state scalar schema stays f32; actual
selected64 model/view/audio composition is validated separately.

`RavetAaxEditorControls.hpp` provides the reusable bounded checked control bridge
over immutable native schema and public initialized model/automation interfaces.
Construction validates complete parameter count and stable ID order. Consumers
retain the exact native tables, model, automation delegate and library; ID equality
alone does not prove that an arbitrary model originated from that native schema.
All operations belong on serialized MAIN. Reentry returns Busy before SDK calls;
wrong-thread operations return before control/gesture mutation. Canonical scalar
preflight precedes begin/value/end requests, including late-invalid values. The
native scalar schema is f32 while AAX normalized storage is double.

The stock SDK model's public setter/gesture methods call void parameter methods;
their success cannot report an underlying host post refusal. This bridge instead
uses initialized public `AAX_VAutomationDelegate` operations directly, preserving
independent attempted/returned/original result fields for begin, value and end,
including per-ID gesture diagnostics. An end success never erases a prior value
failure. Host post success accepts a request; it does not update model state or DSP.
Only a subsequent acknowledged host model update enters its native SPSC queue.

A refused or thrown begin/end leaves an externally unresolved gesture. Accepted
active begins get one bounded best-effort end attempt at retirement; unresolved
ends are not replayed automatically. Diagnostics distinguish unreturned exceptions
from original returned statuses. The host/new-instance reconciliation policy is
external. A refused retirement end latches this GUI instance closed to new views/
gestures; shared per-ID attempts remain observable after callback contexts retire.
Ownership teardown can finish without claiming a resolved host gesture. Snapshots read actual finite normalized model values into fixed storage;
a late failed read preserves the complete prior output, rather than substituting
the requested UI value.

`RavetAaxWebEditor.hpp` owns the public AAX GUI facade and the native WebKit handle.
Trusted nonthrowing codecs provide static UTF-8 HTML, bounded decoded edits and
snapshot JSON. It retains an independent callback context containing schema, model,
host automation and exact public view-container references. That context contains
no GUI-this pointer. Native deferred destroy retains it through entered callbacks;
after a host post closes the owner, no later value request is made. Context retirement
can balance an accepted begin after the callback returns. MAIN final Release and
destruction are mandatory; wrong-thread Uninitialize retains ownership for MAIN
retry. Replacement during an entered callback fails before view/native mutation.
Codec and reference-count operations must be nonthrowing. Lifecycle SDK queries
must not destroy the GUI during Initialize or SetViewContainer; callback-driven
owner destruction is supported through the independent native callback context.

The SDK `AAX_VViewContainer` destructor deliberately detaches queried references as
a historical host workaround. The owned facade queries `AAX_IACFViewContainer`
directly and never creates that stock proxy. Inherited `GetViewContainer()` stays
null; `ownedViewContainerMain()` is a borrowed public interface that must be retained
explicitly beyond replacement/teardown. This is balanced ownership for compliant
host refcounts, not a remedy for historical host double-deletion behavior. Hosts
keep the parent NSView alive through the retained container and native child teardown;
a queried interface cannot validate a forged native pointer. Only NSView parents
are accepted. Native child resizing is explicit through `resizeMain`; host parent
geometry remains external. `GetViewSize` requires successful initialization and
finite positive dimensions no larger than 16384; preinitialization, malformed
constructor dimensions, wrong-thread calls and owner aliasing preserve the caller
output. Initialization also rejects Busy before any host call while an earlier
deferred callback context remains outstanding. Balanced callback teardown permits
reinitialization only after that context releases; a refused retirement end remains
Unresolved and cannot be reopened or replayed. Creation/attach/resize failures retain their native status
separately from SDK query/base results.

The Gain HTML codec handles exact known native IDs and bounded JSON. Native polling
reads current model snapshots; host ParameterUpdated is an accepted refresh notice,
with no polling/delivery deadline guarantee. Meter delivery, host mouse/modifier
automation menus and generic factory program catalogues are not included. AAX chunk
state is not a generic program-list API.

The following receipts qualify the isolated implementation; reviewed source is now integrated. The complete optional composition passes33
CMake tests, including strict normal/fast controls and actual NSView/WebKit lifecycle.
The ordinary composition passes30 tests. A packaged local editor AAX bundle loads
with the required exports/version and its registered mono/stereo GUI/model/algorithm
factories pass checked browser edit, separate host forwarding, independent native
PCM/tail, refusal and ownership retirement checks. The ordinary registered module
also passes native state/transport/control/silence/retirement contracts; its actual
headless descriptors advertise no GUI even when the linked Rust archive contains
unused editor exports. The existing native-editor Rust codec/library tests8 and
Python packaging tests10 pass. Public qualification and joint gates remain pending.
