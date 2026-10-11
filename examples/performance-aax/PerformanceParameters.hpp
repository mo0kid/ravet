#pragma once
#ifndef RAVET_AAX_NATIVE_SAMPLE_BITS
#define RAVET_AAX_NATIVE_SAMPLE_BITS 32
#endif
#include "RavetAaxEffectParameters.hpp"
#include "RavetPerformance.h"
using RavetPerformanceAaxParameters=ravet::aax::NativeAaxEffectParameters<1,32,true,1024,true,false,0,false,RAVET_AAX_NATIVE_SAMPLE_BITS>;
inline RavetPerformanceAaxParameters *createPerformanceAax(uint32_t channels){
 const RavetPerformanceAaxParameters::Identity identity{channels==1?"org.ravet.performance.mono":"org.ravet.performance.stereo",channels,0x52617674,0x52765073,channels==1?0x52765031u:0x52765032u,0x52765331};
 const auto *audio=ravet_performance_audio_state_v1(RAVET_AAX_NATIVE_SAMPLE_BITS);
 // Exact cache supplied by this originating audio-state companion; no export inference.
 return new RavetPerformanceAaxParameters(identity,ravet_performance_processor_v1(RAVET_AAX_NATIVE_SAMPLE_BITS),audio,audio?audio->cached_parameters_entry():nullptr,ravet_performance_state_v1(RAVET_AAX_NATIVE_SAMPLE_BITS));
}
