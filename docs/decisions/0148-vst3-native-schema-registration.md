# 0148: Native schema registration for VST3 controllers

Status: reusable optional SDK header and actual Gain controller migration
implemented and validated with SDK-linked contracts and rebuilt native references.
Broader processing/factory/editor generalization remains separate work.

Expose immutable bounded native global schemas through a reusable VST3 controller
helper, using an externally supplied SDK. Keep SDK code/source outside RAVET and
retain the native table/library until the resulting SDK parameters are destroyed.
The helper belongs to control ownership and does not convert audio automation.

Preflight all descriptors before inserting any into the controller container:
native table/version/capacity, field validity, complete UTF-8 to UTF-16 conversion,
unique host IDs, existing-container conflicts, boolean bypass range and finite
normalized defaults. Reject VST3's reserved all-ones ID and text exceeding 127
UTF-16 code units instead of clipping or remapping identities. Supplementary
characters consume two units. Native schema bounds remain wider than this host
registration boundary. Use the root unit without inferring parameter groups.

SDK parameter objects call the native pure mapping callbacks, preserving global
f32 physical values and linear/log/discrete semantics. These callbacks can acquire
schema metadata; they are control-only. Unrepresentable/nonfinite values or native
mapping failure return NaN instead of guessing. Use numeric SDK text formatting
at its selected precision. Override the SDK's discrete integer formatter because
native stepped choices may be fractional. Bounded numeric parsing preserves its
output on rejection. Parse complete locale-independent ASCII decimal/exponent
text with optional sign/outer whitespace; reject invalid text, suffixes, overflow
and nonfinite values. The macOS SDK numeric scanner accepts invalid input as zero,
so it cannot supply this validation. Roundtrip `On`/`Off` only for explicit boolean
bypass; other discrete/two-choice ranges use numeric physical text. Custom choice
labels remain consumer policy.

Complete metadata/preflight failure preserves the existing container. SDK/control
allocation/insertion exceptions do not have an allocation-failure transaction
guarantee. Flags preserve explicit automation/bypass/read-only declarations; they
do not create event filtering or permissions. Host factories, process/event/state
mapping, groups/programs/editors, notifications and scheduling remain separate
interfaces. Actual host/platform/deadline evidence is not inferred from contracts.
