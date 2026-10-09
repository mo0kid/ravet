# 0085: Optional AUv2 factory program mapping

Status: prototype; actual component contracts, no DAW preset-menu claim.

## Decision and consequences

Reuse the optional nested VST3 catalogue in an explicit default-disabled
`--factory-programs` AU build. A local AUWrapper subclass maps program indices
through count-minus-one normalized intervals, delivers selector changes to the
processor and synchronizes host bypass with complete controls. No external SDK
source is edited/copied. A target-local renamed SDK factory coexists with the
RAVET factory using ordinary AUBase effect dispatch. The document restore
extension is explicitly forwarded; music/instrument/ProcessBufferLists selectors
are outside this reference's declared scope.

AU parameter ID 3 uses nearest index with ties upward and block-start offsets
only; nonzero offsets reject. Explicit factory selection owns complete bypass
recall. Ordinary saved-state/document restore continues preserving host bypass
through the SDK. Negative custom preset metadata keeps AUBase semantics; invalid
positive factory indices reject. Program identity is not persisted independently.

## Validation

Actual factory registration and three instance lifecycles cover names/indices,
repeated/reversed selection, controller/host bypass/audio agreement, normalized
parameter ties/endpoints, offset rejection, custom metadata and invalid-index
preservation. Document restoration and default-build regression are checked
separately. Initial fixture incorrectly rejected negative custom preset metadata;
AUBase intentionally accepts it, so the fixture now asserts preservation.
No auval/DAW/other-architecture/deadline claim follows.
