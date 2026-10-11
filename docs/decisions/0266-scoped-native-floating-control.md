# 0266: Scoped native floating-control ownership

Status: accepted; implemented in `ravet-native-dsp` and qualified within the stated
CPU profiles (native AArch64, Rosetta-translated x86_64).

Portable numerical defaults remain independent of current-thread CPU policy.
A separate `ravet-native-dsp` boundary isolates narrow private register assembly
from the portable crates' unsafe-forbid contracts. The core requires neither
std nor an allocator. Optional Foundation Processor ownership is explicit.

The guard saves/restores exact selected-family controls and cumulative status;
scope-generated sticky flags are isolated rather than silently merged. A thread
marker prohibits Send/Sync, and there is no implicit stop/deactivate or arbitrary
public register setter. Current-thread policy affects only executed operations;
compiler/library arithmetic semantics and x87/half/SVE profiles are not inferred.

The x86_64 path uses aligned complete FXSAVE memory and the actual MXCSR_MASK,
including architectural zero-mask fallback without DAZ. Reserved unsupported
flush writes are rejected; all SIMD exception masks must be set for FlushBoth.
The AArch64 path admits FZ only with AH=0 and exception traps disabled. Unrelated
controls stay unchanged. Read-back rejection restores prior control/status.
One or two bounded 512-byte FXSAVE reads are disclosed rather than hidden as a
zero-cost/deadline guarantee. Register access assumes an ordinary OS user process.

Processor construction refusal returns the exact uncalled child. Preparation
reports only its actual child PrepareError, retaining adapter ownership; every
AUDIO block is separately preflighted and admitted on its actual owner thread.
Native admission failure retains the first original error and silences outputs
without child execution. Child failures/unwind do not imply DSP rollback. Manual
session hooks remain caller-owned; stopped extraction refuses active, prepared
or uncertain lifecycle state without heap allocation. A successful stop cannot
prove a panicked child prepare was inactive; explicit deactivation is required.

Primary instruction facts were reviewed from the
[Intel SDM](https://cdrdv2-public.intel.com/835781/325462-sdm-vol-1-2abcd-3abcd-4.pdf),
[Intel FTZ/DAZ guide](https://www.intel.com/content/www/us/en/docs/dpcpp-cpp-compiler/developer-guide-reference/2025-0/set-the-ftz-and-daz-flags.html)
and [Arm architecture supplement](https://documentation-service.arm.com/static/6526e1bd9e189a266cef8412).
Arm's [FPCR self-synchronization explanation](https://documentation-service.arm.com/static/6749c761c7fc0d1f211dc2f9)
clarifies that FPCR writes do not require an intervening context synchronization.
No vendor implementation/table is imported.

Actual AArch64 and separately translated x86_64 qualification exercise runtime
signed operand/result flushing, special classes, exact restored control/status,
nested/thread-local/unwind scopes and failure paths. Independent actual Graph
IIR/FIR consumers qualify both precisions and buffer views. Unsupported CPU
families, physical x86 hardware, host/device support and deadlines remain separate.
