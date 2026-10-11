#pragma once
#include "RavetAaxEffectParameters.hpp"
#include "RavetRawRouting.h"
#ifndef RAVET_AAX_NATIVE_SAMPLE_BITS
#define RAVET_AAX_NATIVE_SAMPLE_BITS 32
#endif
static_assert(RAVET_AAX_NATIVE_SAMPLE_BITS==32||RAVET_AAX_NATIVE_SAMPLE_BITS==64);
constexpr uint32_t RawRoutingBits=RAVET_AAX_NATIVE_SAMPLE_BITS;
using RavetRawMainModel=ravet::aax::NativeAaxEffectParameters<2,32,false,1024,true,false,0,false,RawRoutingBits,true>;
using RavetRawAuxModel=ravet::aax::NativeAaxEffectParameters<2,32,false,1024,true,true,0,false,RawRoutingBits,true>;
using RavetRawTimedModel=ravet::aax::NativeAaxEffectParameters<2,32,false,1024,false,false,0,true,RawRoutingBits,true>;
inline constexpr const char *RawRoutingIDs[6]={"org.ravet.rawrouting.mono","org.ravet.rawrouting.stereo","org.ravet.rawrouting.aux-detector-first","org.ravet.rawrouting.aux-output-first","org.ravet.rawrouting.delaymono","org.ravet.rawrouting.delaystereo"};
inline RavetRawAuxModel::PreparedProfile rawAuxProfile(bool reversed){
 auto layout=ravet::aax::AaxNativeAudioLayout::main(2,2,1024);layout.busCount=3;layout.inputCapacity=5;layout.outputCapacity=4;
 const auto detector=reversed?2u:1u,monitor=reversed?1u:2u;layout.buses[detector]={1,0};layout.inputFirst[detector]=ravet::aax::AaxNativeAudioLayout::Sidechain;layout.buses[monitor]={0,2};
 return {layout,AAX_eStemFormat_Stereo,AAX_eStemFormat_Stereo};
}
inline AAX_IEffectParameters *createRawRouting(unsigned profile){
 const auto bits=RawRoutingBits;
 if(profile<2){RavetRawMainModel::Identity id{RawRoutingIDs[profile],profile+1,0x52617674,0x52765277,0x52765231+profile,0x52765253};return new RavetRawMainModel(id,ravet_raw_bus_routing_processor_v1(bits),ravet_raw_bus_routing_audio_state_v1(bits),ravet_raw_bus_routing_cached_parameters_v1(bits),ravet_raw_bus_routing_state_v1(bits),ravet_raw_bus_routing_telemetry_v1(bits));}
 if(profile<4){RavetRawAuxModel::Identity id{RawRoutingIDs[profile],2,0x52617674,0x52765277,0x52765231+profile,0x52765253};return new RavetRawAuxModel(id,ravet_raw_bus_routing_processor_v1(bits),ravet_raw_bus_routing_audio_state_v1(bits),ravet_raw_bus_routing_cached_parameters_v1(bits),ravet_raw_bus_routing_state_v1(bits),rawAuxProfile(profile==3),ravet_raw_bus_routing_telemetry_v1(bits));}
 if(profile<6){RavetRawTimedModel::Identity id{RawRoutingIDs[profile],profile-3,0x52617674,0x52765277,0x52765231+profile,0x52765253};return new RavetRawTimedModel(id,ravet_raw_delay8_processor_v1(bits),ravet_raw_delay8_audio_state_v1(bits),ravet_raw_delay8_cached_parameters_v1(bits),ravet_raw_delay8_state_v1(bits),RavetRawTimedModel::TimingProfile{8},ravet_raw_delay8_telemetry_v1(bits));}
 return nullptr;
}
inline RavetRawTimedModel::Owner::TimingTables rawDelay16Candidate(){const auto bits=RawRoutingBits;return {ravet_raw_delay16_processor_v1(bits),ravet_raw_delay16_audio_state_v1(bits),ravet_raw_delay16_cached_parameters_v1(bits),ravet_raw_delay16_state_v1(bits),ravet_raw_delay16_telemetry_v1(bits)};}
