#ifndef RAVET_VST3_HOSTED_AUDIO_CATALOGUE_HPP
#define RAVET_VST3_HOSTED_AUDIO_CATALOGUE_HPP
#include "RavetVst3HostedAudio.h"
#include <array>
namespace ravet::vst3::hosted_audio {
// Inline copied records introduce no escaping name allocation. The containing
// Instance's numeric extent protects all copied scalars and UTF-8 capacities.
struct CopiedCatalogue {
    std::array<RavetVst3HostedAudioBus,16> inputs{},outputs{};
    uint32_t inputCount=0,outputCount=0;
    const auto& bus(bool output,uint32_t index)const noexcept{return (output?outputs:inputs)[index];}
    uint32_t count(bool output)const noexcept{return output?outputCount:inputCount;}
};
}
#endif
