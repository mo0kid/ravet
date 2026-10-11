#pragma once
#ifndef RAVET_AAX_NATIVE_SAMPLE_BITS
#define RAVET_AAX_NATIVE_SAMPLE_BITS 32
#endif
#include "RavetAaxEffectParameters.hpp"
#include "RavetGain.h"
using RavetGainNativeAaxParameters=ravet::aax::NativeAaxEffectParameters<2,32,false,1024,true,false,0,false,RAVET_AAX_NATIVE_SAMPLE_BITS>;
inline RavetGainNativeAaxParameters *createGainNative(uint32_t channels){
 const RavetGainNativeAaxParameters::Identity identity{channels==1?"org.ravet.gain.mono":"org.ravet.gain.stereo",channels,0x52617674,0x5276476e,channels==1?0x52764731u:0x52764732u,0x52765331};
 return new RavetGainNativeAaxParameters(identity,ravet_native_processor_v1(RAVET_AAX_NATIVE_SAMPLE_BITS),ravet_native_audio_state_v1(RAVET_AAX_NATIVE_SAMPLE_BITS),ravet_native_audio_state_v1(RAVET_AAX_NATIVE_SAMPLE_BITS)->cached_parameters_entry(),ravet_native_state_v1(RAVET_AAX_NATIVE_SAMPLE_BITS));
}
