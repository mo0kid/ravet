#pragma once
#ifndef RAVET_AAX_NATIVE_SAMPLE_BITS
#define RAVET_AAX_NATIVE_SAMPLE_BITS 32
#endif
#include "RavetAaxWebEditor.hpp"
#include "RavetGain.h"
#include "RavetNativeAudioState.h"
#include "RavetGainEditor.h"
using RavetGainNativeAaxEditor=ravet::aax::AaxWebEditor<2,RAVET_AAX_NATIVE_SAMPLE_BITS>;
inline RavetGainNativeAaxEditor::Codec gainAaxEditorCodec(){
 return {ravet_gain_editor_html,ravet_gain_editor_message,
 [](const double* values,uint32_t count,uint32_t status,uint8_t* output,size_t capacity)->size_t{
  return count==2?ravet_gain_editor_snapshot(values[0],values[1],status,output,capacity):0;
 }};
}
inline RavetGainNativeAaxEditor* createGainNativeEditor(){
 return new RavetGainNativeAaxEditor(ravet_native_audio_state_v1(RAVET_AAX_NATIVE_SAMPLE_BITS)->cached_parameters_entry(),gainAaxEditorCodec(),540,340);
}
