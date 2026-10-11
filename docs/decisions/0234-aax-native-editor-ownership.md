# 0234 — Checked native AAX editor ownership and automation

Status: Accepted implementation contract. Reviewed source is integrated;
public qualification is recorded separately in work status.

The native AAX model has schema/state/audio adoption but no custom GUI registration.
The stock SDK GUI uses void view hooks and ignores its model QueryInterface result;
stock parameter setters/gestures also hide underlying host automation post results.
The stock view proxy intentionally detaches interface references at destruction.

Use a reusable MAIN-only checked control bridge and owned `AAX_CEffectGUI` facade,
with optional explicit Gain GUI registration. Keep original SDK attempted/returned
statuses separate from local and native editor errors. Direct public automation
delegate operations preserve begin/value/end refusals; post acceptance is separate
from acknowledged model/DSP delivery. Keep active/unresolved per-ID gesture state;
retirement attempts one end for accepted active begins and does not replay uncertain
ends or fabricate host success. A refused retirement end latches the GUI closed
to new views/gestures, retaining per-ID original attempts independently of context
retirement until host/new-instance reconciliation.

The view facade borrows the initialized base model/controller composition boundary
and owns queried public view-container references directly. Never instantiate the
SDK proxy whose destructor detaches references, compensate its leaks, copy SDK code
or access private layouts. Inherited GetViewContainer remains null and the owned
accessor is explicit. Parent NSView lifetime and compliant host refcount behavior
remain obligations; this does not claim to repair historical host double deletion.

Native callback context independently owns schema/model/automation/host/view through
deferred teardown and entered callback completion. No GUI-this borrower survives GUI
destruction. Closing after a foreign post prevents later value requests, with bounded
gesture retirement after return. Replacement/reentry rejects before mutation; MAIN
final Release is required and wrong-thread uninitialize retains retry ownership.
Keep default headless profiles unchanged and optional editor build separate.

Target evidence: strict normal/fast checked controls, actual NSView attach/resize/
replacement/refusal/failure/deferred destruction, and actual registered module GUI
factory → SDK model → separate host acknowledgment → independent mono/stereo native
PCM/backing-tail assertions pass. The optional composition passes33 CMake tests; ordinary headless30 plus actual
registered no-GUI/native module checks pass. Packaged editor exports and actual
GUI/module contracts pass; existing Rust native-editor8 and Python10 pass. Public
public qualification and joint gates are pending.
No Pro Tools/signing, accessibility, other-platform/editor deadline, host modifier
menu or generic factory-program support follows from these fixtures.

Reinitialization is gated before SDK calls while created/released callback-context
counts differ. This closes the same-GUI Uninitialize/Initialize hole during deferred
native callback teardown. Only balanced completion admits reopening; refused old
end latches unresolved lifetime before context release. Actual WebKit callbacks
exercise both outcomes with original statuses and no extra SDK query or end replay.
