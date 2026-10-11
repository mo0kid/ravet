# 0217: Fixed-model virtual text rows and pointer identity pins

Status: Accepted

## Decision

VirtualList copies a fixed1..65536 labelled text model with extent<=1000000px,
fixed12..256px rows and0..32 overscan. Mount/reuse a viewport window, with hard512
combined mounted/free row capacity including active and pressed pins. Root owns
virtual focus and stable ARIA identity. Selection is a separate at-most256 model;
range Capacity failure is whole-transition. Scroll never prunes or notifies.

Generate IDs from monotonically namespaced safe-integer module ownership plus
model index, preflight every prospective original-document ID with bounded
lookups, reject collision/exhaustion. Model labels use textContent. Owner tokens
are JS-local references/counters, not Rust source tokens or serialized secrets.

A trusted pointer token retains exact row/index/modelID/pointerID/owner through
pointerup to matching native click. Any fresh down retires old identity before
admission; capture snapshots prevent recycling from reinterpreting the target.
A Capacity click consumes native phase while retaining the same token/row pin
and unchanged model/active/anchor; duplicate click cannot retry consumed phase. Pin until matching click or explicit bounded
cancellation; never recycle that row under a draining click. Both Chromium and
owned AppKit/WK generate trusted PointerEvent down/up/click with matchingpointerID;
WK down buttons0 means primary admission uses button0 rather than buttonsmask.
Keyboard/programmatic intent, including a recognized no-movement navigation
boundary, cancels accepted prior pointer intentions but
Capacity rejection preserves complete ownership. No timeout/global tombstone.

Unsupported transforms/individual translate/rotate/scale/perspective/zoom and
root in-flow or generated spacer/row visible pseudo content/extent mismatch suspend
coordinate admission; member/document/generated/free-node ownership mutation
retires before input or external-node reuse. Exact owned root style/attr cleanup
and disposal checks guard each reentrant write. Callbacks cannot roll back an
already committed model/consumer side effect, so error and postcallback invalidity
remain explicit. Hidden/all-disabled selection models can be explicitly edited
without pretending a usable focus/viewport exists.

## Evidence and consequences

60 focused groups each actual engine, plus separate trusted native pointer and
namespace proof. Trusted press row1 then scroll before release has no selection;
fresh row501click selects once. Large-model End key has14 mounted nodes.
Bounded work does not bound browser layout/global memory or consumer allocation.
Document.hasFocusfalse/nativeAXnegative/screen-reader gaps remain explicit.
Fixed membership, geometry and text profile simplify stale-pointer ownership;
variable heights/custom renderers/dynamic model/tree/grid virtualization and
sorting are separate. See [virtual text list](../virtual-list.md).

Sparse selection arrays reject atomically across fixed list/tree/table and virtual
list models. Actual allowed local iframe navigation proves that retired original
documents cannot notify, while cleanup restores original attributes/styles and
skips replaced/inaccessible WindowProxy listener removal.
