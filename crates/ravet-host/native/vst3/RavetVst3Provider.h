#ifndef RAVET_VST3_PROVIDER_H
#define RAVET_VST3_PROVIDER_H
#include <stdint.h>
#ifdef __cplusplus
extern "C" {
#endif
typedef struct RavetVst3Module RavetVst3Module;
enum { RAVET_VST3_OK = 0, RAVET_VST3_INVALID = 1, RAVET_VST3_CAPACITY = 2,
       RAVET_VST3_METADATA = 3, RAVET_VST3_NATIVE = 4, RAVET_VST3_THREAD = 5,
       RAVET_VST3_ALLOCATION = 6, RAVET_VST3_RETIRED_NATIVE = 7,
       RAVET_VST3_CHILDREN = 8, RAVET_VST3_PROCESS_PINNED = 9, RAVET_VST3_LIFECYCLE = 10 };
typedef struct RavetVst3FactoryRecord {
    uint32_t struct_size;
    int32_t flags;
    uint32_t class_count;
    int32_t host_context_result;
    char vendor[64], url[256], email[128];
} RavetVst3FactoryRecord;
typedef struct RavetVst3ClassRecord {
    uint32_t struct_size;
    uint8_t id[16];
    int32_t cardinality;
    uint32_t class_flags;
    uint32_t vendor_from_factory;
    char category[32], name[192], vendor[192], version[192], sdk_version[192], subcategories[128];
} RavetVst3ClassRecord;
typedef struct RavetVst3Provider {
    uint32_t struct_size;
    uint32_t version;
    int32_t (*open)(const uint8_t*, uint32_t, RavetVst3Module**, int32_t*);
    int32_t (*factory)(const RavetVst3Module*, RavetVst3FactoryRecord*);
    int32_t (*class_info)(const RavetVst3Module*, uint32_t, RavetVst3ClassRecord*);
    int32_t (*close)(RavetVst3Module*, int32_t*);
} RavetVst3Provider;
// Provider executable and its host/context code must remain loaded until OS
// process exit whenever a non-discardable core can be pinned. The ABI retains
// native modules, not an independently unloaded provider DLL.
// Independent v2 extension; v1 table size/layout and retirement codes stay intact.
typedef struct RavetVst3Instance RavetVst3Instance;
typedef struct RavetVst3Audio RavetVst3Audio;
typedef struct RavetVst3AudioProfile {
    uint32_t struct_size, maximum_frames, input_channels, output_channels;
    double sample_rate;
} RavetVst3AudioProfile;
typedef struct RavetVst3InputChannel { const float* samples; uint32_t capacity, reserved; } RavetVst3InputChannel;
typedef struct RavetVst3OutputChannel { float* samples; uint32_t capacity, reserved; } RavetVst3OutputChannel;
typedef struct RavetVst3AudioBlock {
    uint32_t struct_size, frames, input_channels, output_channels;
    const RavetVst3InputChannel* inputs;
    const RavetVst3OutputChannel* outputs;
} RavetVst3AudioBlock;
typedef struct RavetVst3ProviderV2 {
    uint32_t struct_size, version;
    int32_t (*open)(const uint8_t*, uint32_t, RavetVst3Module**, int32_t*);
    int32_t (*factory)(const RavetVst3Module*, RavetVst3FactoryRecord*);
    int32_t (*class_info)(const RavetVst3Module*, uint32_t, RavetVst3ClassRecord*);
    int32_t (*close)(RavetVst3Module*, int32_t*);
    int32_t (*create)(RavetVst3Module*, uint32_t, RavetVst3Instance**, int32_t*);
    int32_t (*prepare)(RavetVst3Instance*, const RavetVst3AudioProfile*, int32_t*);
    int32_t (*start)(RavetVst3Instance*, int32_t*);
    int32_t (*begin_audio)(RavetVst3Instance*, RavetVst3Audio**, int32_t*);
    int32_t (*process)(RavetVst3Audio*, const RavetVst3AudioBlock*, int32_t*);
    int32_t (*end_audio)(RavetVst3Audio*, int32_t*);
    int32_t (*stop)(RavetVst3Instance*, int32_t*);
    int32_t (*destroy)(RavetVst3Instance*, int32_t*);
    int32_t (*restart_requests)(const RavetVst3Instance*, uint32_t*);
} RavetVst3ProviderV2;
// v2 CONTROL methods require original platform main thread. begin_audio creates
// one exclusively borrowed capability, moved to exactly one AUDIO thread; caller
// must stop/join it before main end_audio. No AUDIO lifecycle/destruction method.
// Parent close rejects live children; child destroy rejects attached AUDIO.
// close consumes on OK, RETIRED_NATIVE, or explicit PROCESS_PINNED. Process pins
// retain the host/context/image until OS process exit, including failed first
// createInstance; v2 reopen reuses the same core/context, v1 excludes pinned cores.
// Caller-owned contiguous f32 main blocks only; exact capacity, readable/writable
// ranges, distinct output prefixes and metadata/owner exclusion are checked before
// native DSP. Foreign validity/lifetime is caller trust, not memory isolation.
const RavetVst3ProviderV2* ravet_vst3_provider_v2(void);
// The table and implementation remain loaded through every open owner. Native
// calls require valid readable/writable records and the platform main thread;
// same-thread checks do not establish main-thread suitability. open requires a
// null output handle; failed open preserves it. Queries require exact struct_size
// and preserve the caller record on failure. close consumes on OK or
// RETIRED_NATIVE (original context-clear error reported); other rejection retains
// ownership. Retirement never retries failed context clearing.
const RavetVst3Provider* ravet_vst3_provider_v1(void);
#ifdef __cplusplus
}
#endif
#endif
