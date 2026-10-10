# 0182: Native AUv2 schema/control/state adoption

Status: prototype; native helper, actual packaged component and shared bundle validation passed locally.

An owned schema-cache adapter gives AU consumers complete native IDs, CF metadata,
canonical normalized f32 controls and complete physical text conversion. The stock
VST3 AU wrapper otherwise exposes truncated String128 metadata/text, queues values
before generic setter validation and can report guessed empty formatting success.
Cached scalar preflight is separate from CONTROL-only CF allocation and from SDK
queues/storage/notifications. Full UInt32 native IDs remain available to a generic
AU consumer; a VST3-backed component keeps the VST3 reserved-ID restriction.

Complete state validation uses a private native publication object with the exact
schema companion. It decodes control data without borrowing or mutating live DSP.
Native stream validation and AU dictionary identity/type checks precede SDK
restore, whose backend decoder failures are otherwise ignored by the external
wrapper. Saved processor state is authoritative for coherent controller state.
This does not introduce rollback for a later SDK/backend failure, a new audio
state publication or generic host-bypass semantics.

The performance AUv2 consumer owns and uses both helpers for schema/list/info,
control/text and complete state, replacing its duplicated codec/dictionary logic.
Its bounded note/render/factory-property/lifecycle contracts remain separate.
AUBase property interception and normalized-f32 precision are documented rather
than hidden behind a universal wrapper claim. SDK sources remain external.

Focused synthetic helper/actual native table tests and actual bundled-component
regressions establish only their stated local macOS arm64 behavior. They do not
prove DAW/auval, sandbox, devices, another platform/architecture, all SDK/runtime
allocation, callback deadlines or production support.
