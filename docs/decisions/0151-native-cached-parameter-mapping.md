# 0151: Control-cached native parameter mapping

Status: reusable Rust cache and optional native companion implemented and
validated through Rust concurrency/heap contracts and actual-library C/C++
checks. SDK queue integration remains a separate work package.

Do not invoke consumer schema/role providers while converting host automation on
audio. Prepare an immutable bounded schema cache on control, retaining copied
native metadata, borrowed static ranges and a sorted ID/index lookup. Shared cached
reads map through Foundation's existing finite clamp/linear/log/discrete rules,
without user callbacks, DSP borrowing, heap actions, locks or I/O. Keep creation,
destruction and provider calls on control; retire after every borrower stops and
retain originating tables/library/static schema references.

Use at most 1024 descriptors. Preserve declared-order metadata and all u32 IDs;
format-specific reserved IDs/text limits belong to consumers. Automation/bypass/
read-only roles remain declarations, without hidden event filtering. Explicit
native entry pairing identifies the consumer precision while global values remain
f32. Factory/provider panic returns null without poisoning another cache or DSP.
Existing caches do not call providers again, including after provider failure.

Provide complete bounded normalized-event conversion into tagged physical global
parameter records. Validate all IDs, finite 0..1 normalized intent, ordered offsets,
zero-frame timing and capacities before any output mutation. Narrow normalized
f64 intent explicitly to the global normalized f32 contract, then widen physical
f32 exactly into event records. Unknown IDs reject; consumers explicitly filter
or route unsupported host IDs before conversion. Equal-offset order and destination
tails survive. Use two bounded passes and binary cached ID lookup, with no audio
sort/allocation. Limits are 1024 input/output capacity and 65536 frames.

Keep the existing parameter table ABI stable by adding a separate optional cache
companion. It supplies no processor ownership, host queue traversal, automatic
SDK registration, notifications, retry or scheduling. Scoped modulation/expressions,
f64 parameter persistence, consumer callbacks and deadlines remain separate work.
