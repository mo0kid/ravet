#pragma once
#include <stdint.h>
// Shared raw bus layout for consumer-owned Rust/C/C++ format shims. This header
// defines no plug-in factory, event codec, state protocol or exported functions.
// Choose the matching native precision; preparation order is main, auxiliaries.
// Pointer arrays and samples remain live/aligned for counts/active frames under
// exclusive callback ownership. Zero-frame calls permit null arrays. Inputs may
// alias outputs, including across buses; overlapping output ranges reject.
// Metadata/events/processor/adapter storage never overlap writable samples.
typedef struct RavetNativeBusF32 {
    uint32_t input_channels;
    uint32_t output_channels;
    const float* const* inputs;
    float* const* outputs;
} RavetNativeBusF32;
typedef struct RavetNativeBusF64 {
    uint32_t input_channels;
    uint32_t output_channels;
    const double* const* inputs;
    double* const* outputs;
} RavetNativeBusF64;
