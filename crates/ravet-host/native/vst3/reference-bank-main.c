/* Links the ravet-host vst3-reference-bank example staticlib (Rust hosted-audio
   Graph banks) with this caller-linked native provider. main runs on the
   platform main thread, as the Graph-bank registry requires (ADR 0274). */
#include "RavetVst3HostedAudio.h"
#include <stdio.h>
int32_t ravet_vst3_reference_bank_run(const RavetVst3ProviderV2*, const RavetVst3ParameterProviderV1*,
    const RavetVst3ProcessingProviderV1*, const RavetVst3HostedAudioProviderV1*, const char*, const char*);
int main(int argc, char** argv) {
    if (argc != 3) {
        fprintf(stderr, "usage: reference-bank-tests /abs/sidechain.vst3 /abs/event-probe.vst3\n");
        return 2;
    }
    return ravet_vst3_reference_bank_run(ravet_vst3_provider_v2(), ravet_vst3_parameter_provider_v1(),
        ravet_vst3_processing_provider_v1(), ravet_vst3_hosted_audio_provider_v1(), argv[1], argv[2]);
}
