#ifndef RAVET_VST3_HOSTED_AUDIO_H
#define RAVET_VST3_HOSTED_AUDIO_H
#include "RavetVst3Processing.h"
#ifdef __cplusplus
extern "C" {
#endif
/* Additive companion. Existing provider/processing/state/parameter ABIs retain
   their exact layouts and domains. Every pointer belongs to the paired origins. */
enum { RAVET_VST3_HOSTED_AUDIO_MAX_BUSES=16, RAVET_VST3_HOSTED_AUDIO_MAX_CHANNELS=64 };
typedef struct RavetVst3HostedAudioInfo {
    uint32_t struct_size, input_buses, output_buses, reserved;
} RavetVst3HostedAudioInfo;
typedef struct RavetVst3HostedAudioBus {
    uint32_t struct_size, index, channels, bus_type, flags, reserved;
    uint64_t arrangement;
    char name[384];
} RavetVst3HostedAudioBus;
/* Array positions are native bus indices. Explicitly supply ALL advertised buses.
   Inactive buses remain in native ProcessData; packed active channel order is
   increasing native bus index separately in each direction. */
typedef struct RavetVst3HostedAudioBusRequest {
    uint64_t arrangement;
    uint32_t active, reserved;
} RavetVst3HostedAudioBusRequest;
typedef struct RavetVst3HostedAudioProfile {
    uint32_t struct_size, maximum_frames, sample_bits, input_buses, output_buses, reserved;
    double sample_rate;
    RavetVst3HostedAudioBusRequest inputs[16], outputs[16];
} RavetVst3HostedAudioProfile;
typedef struct RavetVst3HostedAudioTiming {
    uint32_t struct_size, latency_samples, tail_kind, reserved;
    uint64_t tail_samples;
} RavetVst3HostedAudioTiming;
/* Control-only provenance. stages100..120 match the private operation stages;
   121=latency getter and122=tail getter exist only in THIS companion domain.
   failure0=none,1=local invalid,2=returned foreign refusal,3=foreign throw,
   4=capacity,5=allocation,6=malformed metadata. Returned applies only to actual
   tresult-returning SDK methods; getters and local failures cannot invent it. */
enum { RAVET_VST3_HOSTED_ATTEMPTED=1, RAVET_VST3_HOSTED_RETURNED=2,
       RAVET_VST3_HOSTED_THROWN=4, RAVET_VST3_HOSTED_INVALIDATED=8 };
typedef struct RavetVst3HostedAudioReport {
    uint32_t struct_size, stage, failure, index, flags;
    int32_t native_result;
} RavetVst3HostedAudioReport;
typedef struct RavetVst3HostedInputChannel64 { const double* samples; uint32_t capacity, reserved; } RavetVst3HostedInputChannel64;
typedef struct RavetVst3HostedOutputChannel64 { double* samples; uint32_t capacity, reserved; } RavetVst3HostedOutputChannel64;
typedef struct RavetVst3HostedAudioBlock64 {
    uint32_t struct_size, frames, input_channels, output_channels;
    const RavetVst3HostedInputChannel64* inputs;
    const RavetVst3HostedOutputChannel64* outputs;
} RavetVst3HostedAudioBlock64;
/* New process32 uses the existing float block layout with this companion's
   checked packed counts0..64, not the old main-only entry's counts0..2. */
typedef struct RavetVst3HostedAudioProviderV1 {
    uint32_t struct_size, version;
    const RavetVst3ProviderV2* instance_origin;
    const RavetVst3ProcessingProviderV1* processing_origin;
    int32_t (*refresh_audio)(RavetVst3Instance*, RavetVst3HostedAudioInfo*, RavetVst3HostedAudioReport*);
    int32_t (*audio_bus)(const RavetVst3Instance*, uint32_t, uint32_t, RavetVst3HostedAudioBus*, RavetVst3HostedAudioReport*);
    int32_t (*prepare)(RavetVst3Instance*, const RavetVst3HostedAudioProfile*, RavetVst3HostedAudioReport*);
    int32_t (*timing)(const RavetVst3Instance*, RavetVst3HostedAudioTiming*, RavetVst3HostedAudioReport*);
    int32_t (*process32)(RavetVst3Audio*, const RavetVst3AudioBlock*, const RavetVst3ParameterPoint*, uint32_t,
        const RavetVst3EventRecord*, uint32_t, const uint8_t*, uint32_t, const RavetVst3ContextRecord*,
        RavetVst3ProcessingOutputs*, RavetVst3ProcessingReport*);
    int32_t (*process64)(RavetVst3Audio*, const RavetVst3HostedAudioBlock64*, const RavetVst3ParameterPoint*, uint32_t,
        const RavetVst3EventRecord*, uint32_t, const uint8_t*, uint32_t, const RavetVst3ContextRecord*,
        RavetVst3ProcessingOutputs*, RavetVst3ProcessingReport*);
} RavetVst3HostedAudioProviderV1;
/* Caller-linked provider executable remains loaded, as required by exact origins.
   All CONTROL methods are MAIN-only; AUDIO methods use sole bound worker and
   whole read admission. Native lifecycle still ends only through paired v2 MAIN.
   Callback ranges/liveness and foreign compliance remain caller trust. */
const RavetVst3HostedAudioProviderV1* ravet_vst3_hosted_audio_provider_v1(void);
#ifdef __cplusplus
}
#endif
#endif
