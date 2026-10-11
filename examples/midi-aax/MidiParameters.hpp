#pragma once
#include "RavetAaxEffectParameters.hpp"
#include "RavetMidiSine.h"
#ifndef RAVET_AAX_NATIVE_SAMPLE_BITS
#define RAVET_AAX_NATIVE_SAMPLE_BITS 32
#endif
using RavetMidiAaxParameters=ravet::aax::NativeAaxEffectParameters<1,32,true,1024,true,false,0,false,RAVET_AAX_NATIVE_SAMPLE_BITS,true>;
inline RavetMidiAaxParameters *createMidiAax(uint32_t channels){
 const RavetMidiAaxParameters::Identity identity{channels==1?"org.ravet.midisine.mono":"org.ravet.midisine.stereo",channels,0x52617674,0x52764d53,channels==1?0x52764d31u:0x52764d32u,0x52764d43};
 const auto *audio=ravet_midi_sine_audio_state_v1(RAVET_AAX_NATIVE_SAMPLE_BITS);
 return new RavetMidiAaxParameters(identity,ravet_midi_sine_processor_v1(RAVET_AAX_NATIVE_SAMPLE_BITS),audio,audio?audio->cached_parameters_entry():nullptr,ravet_midi_sine_state_v1(RAVET_AAX_NATIVE_SAMPLE_BITS),ravet_midi_sine_telemetry_v1(RAVET_AAX_NATIVE_SAMPLE_BITS));
}
