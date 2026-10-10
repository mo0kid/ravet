#pragma once
#include <stdint.h>
#include "RavetNativeParameters.h"
#ifdef __cplusplus
extern "C" {
#endif
/* Optional explicit live global state, on targets with 64-bit atomics. Match the
   exact originating parameter/processor tables, precision and library lifetime.
   Publication objects own cached immutable schemas/atomic words, never DSP.
   create/destroy/publish/save_state use CONTROL ownership. snapshot can observe
   concurrently. apply/reflect exclusively own PREPARED AUDIO DSP. Shared object
   calls may overlap; every caller/range/library must outlive the call. Destroy
   only after all readers/writers/audio owners have stopped using the object.
   All foreign ranges are live/aligned/disjoint from each other and both handles.

   Codes: 0 success, 1 rejection, 2 panic/owner poison, 3 Busy, 4 Stale. One-attempt
   publication/read never waits or spins. Retain last accepted audio state on
   Busy; consumer owns bounded retry policy, scheduling and stopped retirement.
   Newer complete control restores can coalesce. No automatic SDK registration,
   host notifications or arbitrary consumer callback deadline guarantee. */
typedef struct RavetNativeStateRevisionV1 { uint64_t instance, generation; } RavetNativeStateRevisionV1;
/* In-process identity and even generation, not persisted state. Neither may
   wrap within its relevant library/read/conditional-update lifetime. */
typedef struct RavetNativeStateApplyV1 {
    RavetNativeStateRevisionV1 revision;
    uint32_t changed, reserved;
} RavetNativeStateApplyV1;
/* changed=1 newly accepted state, 0 already accepted; reserved=0. No re-restore
   for the same identity/generation, preserving ongoing smoothing/history.
   Failed calls leave every output unchanged. apply before process, reflect after
   process. Reflection validates/reorders complete global BASE values; transient
   modulation and voices are excluded. Conditional reflection cannot overwrite a
   newer control restore; its own accepted revision is not reapplied next block.
   Processor hooks explicitly opt into no allocation/deallocation/blocking/I/O
   and atomic rejection. Hook panics poison the paired owner until fresh stopped
   preparation, which clears accepted revision; reset preserves it.

   create(NULL,0) chooses schema defaults, otherwise complete consumer-decoded
   state <=8208 bytes (including explicit legacy migration). publish validates
   complete state before attempting atomic publication. snapshot capacity covers
   parameter_count (<=1024); save covers paired state_bytes (<=8208). Successful
   prefixes preserve tails; save always emits canonical product/version state. */
typedef struct RavetNativeStateEntryV1 {
    uint32_t version, byte_size, sample_bits, parameter_count;
    const RavetNativeParameterEntryV1 *(*parameters_entry)(void);
    void *(*create)(const uint8_t *, uint32_t length);
    void (*destroy)(void *);
    int32_t (*publish)(const void *, const uint8_t *, uint32_t length, RavetNativeStateRevisionV1 *);
    int32_t (*snapshot)(const void *, RavetNativeParameterValueV1 *, uint32_t capacity, RavetNativeStateRevisionV1 *);
    int32_t (*save_state)(const void *, uint8_t *, uint32_t capacity, RavetNativeStateRevisionV1 *);
    int32_t (*apply)(void *processor, const void *publication, RavetNativeStateApplyV1 *);
    int32_t (*reflect)(void *processor, const void *publication, RavetNativeStateRevisionV1 *);
} RavetNativeStateEntryV1;
/* Public Gain symbol; consumer export macro can choose another. 32/64 select
   the originating precision; unsupported precision/invalid metadata -> NULL. */
const RavetNativeStateEntryV1 *ravet_native_state_v1(uint32_t sample_bits);
#ifdef __cplusplus
}
#endif
