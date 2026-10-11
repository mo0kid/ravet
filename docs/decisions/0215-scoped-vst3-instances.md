# 0215: Scoped VST3 instance ownership

Status: prototype implemented; main-bus native/Rust composition validated locally,
with further profile and host validation required.

VST3 component/controller ownership needs a retained native module and host
context, explicit non-discardable component lifetime, separate main/control and
audio duties, and whole caller-block validation. A catalogue-only owner cannot
provide those guarantees by releasing interfaces from an audio-thread destructor.

Use a separate ABI-v2 table while retaining the original ABI-v1 table. One shared
native core owns module/factory/context and a bounded identity reservation. The
reservation remains busy through foreign retirement without holding its mutex
over SDK calls. Reserve process retention before any non-discardable creation
attempt and report pinned facade retirement explicitly. The pin retains the
exact context and allows canonical-path v2 reopen without another module entry;
provider executable lifetime remains an explicit caller obligation.

Main-owned Rust child instances borrow their parent. An exclusive higher-ranked
audio scope lends one Send, non-Sync, non-cloneable capability to a joined worker;
main cleanup occurs after return/unwind even when the token is forgotten.
Unexpected failed end retains ownership and poisons the child. Consuming cleanup
failure differs from a non-consuming rejection that returns its owner.

Prepare bounded main f32 scratch and validate complete caller blocks before SDK
processing or output writes. Copy all inputs before publishing output to preserve
raw input/output aliasing, protect owned metadata and silence valid prefixes on
known DSP faults. Foreign pointers remain trusted memory contracts.

This package establishes component/controller lifecycle and the first processing
profile. Events, parameter/state/editor/graph integration, more audio profiles and
host/DAW validation remain separate required work. Controlled SDK fixtures,
actual local reference bundles, Rust contract faults and compile-fail ownership
cases provide complementary evidence; none establishes general compatibility.

See [the instance guide](../vst3-instances.md) for exact bounds and limits.
