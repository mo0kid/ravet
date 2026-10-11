#pragma once
#ifndef RAVET_AAX_NATIVE_SAMPLE_BITS
#define RAVET_AAX_NATIVE_SAMPLE_BITS 32
#endif
#include "RavetGain.h"
namespace ravet_aax_test {
inline RavetNativeProcessorEntryV1 faultCore{};inline RavetNativeAudioStateEntryV1 faultAudio{};inline RavetNativeCachedParameterEntryV1 faultCache{};inline RavetNativeParameterEntryV1 faultParameters{};inline RavetNativeStateEntryV1 faultState{};
inline int32_t prepareFault=0,resetFault=0,applyFault=0,processFault=0,deactivateFault=0;inline uint32_t prepareCalls=0,processCalls=0,applyCalls=0;
inline const RavetNativeProcessorEntryV1 *coreOrigin(){return &faultCore;}inline const RavetNativeCachedParameterEntryV1 *cacheOrigin(){return &faultCache;}inline const RavetNativeParameterEntryV1 *parametersOrigin(){return &faultParameters;}
inline int32_t faultyPrepare(void *p,const RavetNativeProcessorConfigV1 *c){++prepareCalls;return prepareFault?prepareFault:ravet_native_processor_v1(RAVET_AAX_NATIVE_SAMPLE_BITS)->prepare(p,c);}
inline int32_t faultyReset(void *p){return resetFault?resetFault:ravet_native_processor_v1(RAVET_AAX_NATIVE_SAMPLE_BITS)->reset(p);}
inline int32_t faultyDeactivate(void *p){return deactivateFault?deactivateFault:ravet_native_processor_v1(RAVET_AAX_NATIVE_SAMPLE_BITS)->deactivate(p);}
inline int32_t faultyApply(void *p,const void *cache,const RavetNativeParameterValueV1 *v,uint32_t count){++applyCalls;return applyFault?applyFault:ravet_native_audio_state_v1(RAVET_AAX_NATIVE_SAMPLE_BITS)->apply(p,cache,v,count);}
inline int32_t faultyProcess(void *p,const RavetNativeProcessorBusV1 *b,uint32_t buses,uint32_t frames,const RavetNativeEventV1 *e,uint32_t events,const RavetNativeTransportV1 *t,RavetNativeEventV1 *out,uint32_t capacity,RavetNativeProcessorResultV1 *r){++processCalls;return processFault?processFault:ravet_native_processor_v1(RAVET_AAX_NATIVE_SAMPLE_BITS)->process(p,b,buses,frames,e,events,t,out,capacity,r);}
inline void initializeFaultTables(){
 faultCore=*ravet_native_processor_v1(RAVET_AAX_NATIVE_SAMPLE_BITS);faultCore.prepare=faultyPrepare;faultCore.reset=faultyReset;faultCore.deactivate=faultyDeactivate;faultCore.process=faultyProcess;
 faultParameters=*ravet_native_parameters_v1(RAVET_AAX_NATIVE_SAMPLE_BITS);faultParameters.processor_entry=coreOrigin;
 faultCache=*ravet_native_cached_parameters_v1(RAVET_AAX_NATIVE_SAMPLE_BITS);faultCache.parameters_entry=parametersOrigin;
 faultAudio=*ravet_native_audio_state_v1(RAVET_AAX_NATIVE_SAMPLE_BITS);faultAudio.cached_parameters_entry=cacheOrigin;faultAudio.apply=faultyApply;
 faultState=*ravet_native_state_v1(RAVET_AAX_NATIVE_SAMPLE_BITS);faultState.parameters_entry=parametersOrigin;
}
}
