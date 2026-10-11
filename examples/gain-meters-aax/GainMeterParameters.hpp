#pragma once
#ifndef RAVET_AAX_NATIVE_SAMPLE_BITS
#define RAVET_AAX_NATIVE_SAMPLE_BITS 32
#endif
#include "RavetAaxEffectParameters.hpp"
#include "RavetGain.h"
inline const std::array<ravet::aax::AaxMeterBinding,6>& gainMeterBindings(){
 static const std::array<ravet::aax::AaxMeterBinding,6> bindings{{{101,10,"Input L peak",AAX_eMeterType_Input},{102,11,"Input L RMS",AAX_eMeterType_Input},{103,16,"Output L peak",AAX_eMeterType_Output},{104,17,"Output L RMS",AAX_eMeterType_Output},{105,19,"Output R peak",AAX_eMeterType_Output},{106,20,"Output R RMS",AAX_eMeterType_Output}}};return bindings;
}
using RavetGainMetersAaxParameters=ravet::aax::NativeAaxEffectParameters<2,32,false,1024,true,false,0,false,RAVET_AAX_NATIVE_SAMPLE_BITS,false,6>;
inline RavetGainMetersAaxParameters* createGainMeters(uint32_t channels){
 const RavetGainMetersAaxParameters::Identity identity{channels==1?"org.ravet.gainmeters.mono":"org.ravet.gainmeters.stereo",channels,0x52617674,0x52764d47,channels==1?0x524d4731u:0x524d4732u,0x52765331};
 return new RavetGainMetersAaxParameters(identity,ravet_native_processor_v1(RAVET_AAX_NATIVE_SAMPLE_BITS),ravet_native_audio_state_v1(RAVET_AAX_NATIVE_SAMPLE_BITS),ravet_native_audio_state_v1(RAVET_AAX_NATIVE_SAMPLE_BITS)->cached_parameters_entry(),ravet_native_state_v1(RAVET_AAX_NATIVE_SAMPLE_BITS),ravet_native_telemetry_v1(RAVET_AAX_NATIVE_SAMPLE_BITS),gainMeterBindings().data());
}
