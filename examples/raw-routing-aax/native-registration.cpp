#include "RawRoutingParameters.hpp"
#include "AAX_ICollection.h"
#include "AAX_IEffectDescriptor.h"
#include "AAX_IComponentDescriptor.h"
#include "AAX_IPropertyMap.h"
#include "AAX_Properties.h"
namespace {
template<unsigned Profile>AAX_IEffectParameters *AAX_CALLBACK create(){try{return createRawRouting(Profile);}catch(...){return nullptr;}}
template<unsigned Profile,class Model>AAX_Result variant(AAX_ICollection& host){
 auto* descriptor=host.NewDescriptor();if(!descriptor)return AAX_ERROR_NULL_OBJECT;
 auto status=descriptor->AddName("RAVET Raw Routing");if(status)return status;
 status=descriptor->AddProcPtr(reinterpret_cast<void*>(create<Profile>),kAAX_ProcPtrID_Create_EffectParameters);if(status)return status;
 auto* component=descriptor->NewComponentDescriptor();if(!component)return AAX_ERROR_NULL_OBJECT;
 using Context=typename Model::Context;
 status=component->AddPrivateData(AAX_FIELD_INDEX(Context,owner),sizeof(typename Model::Owner*));if(status)return status;
 status=component->AddAudioBufferLength(AAX_FIELD_INDEX(Context,frames));if(status)return status;
 status=component->AddAudioIn(AAX_FIELD_INDEX(Context,inputs));if(status)return status;
 status=component->AddAudioOut(AAX_FIELD_INDEX(Context,outputs));if(status)return status;
 if constexpr(Profile==2||Profile==3){
  if constexpr(Profile==2){status=component->AddSideChainIn(AAX_FIELD_INDEX(Context,sidechainFirst));if(status)return status;}
  status=component->AddAuxOutputStem(AAX_FIELD_INDEX(Context,outputs),AAX_eStemFormat_Stereo,"Detector monitor");if(status)return status;
  if constexpr(Profile==3){status=component->AddSideChainIn(AAX_FIELD_INDEX(Context,sidechainFirst));if(status)return status;}
 }
 status=component->AddMIDINode(AAX_FIELD_INDEX(Context,notes),AAX_eMIDINodeType_LocalInput,"MIDI Input",0xffff);if(status)return status;
 auto* properties=component->NewPropertyMap();if(!properties)return AAX_ERROR_NULL_OBJECT;
 constexpr uint32_t channels=Profile<2?Profile+1:(Profile<4?2:Profile-3);const auto stem=channels==1?AAX_eStemFormat_Mono:AAX_eStemFormat_Stereo;
 const std::pair<AAX_EProperty,AAX_CPropertyValue> records[]={{AAX_eProperty_ManufacturerID,0x52617674},{AAX_eProperty_ProductID,0x52765277},{AAX_eProperty_CanBypass,1},{AAX_eProperty_Constraint_MultiMonoSupport,0},{AAX_eProperty_InputStemFormat,stem},{AAX_eProperty_OutputStemFormat,stem},{AAX_eProperty_PlugInID_Native,0x52765231+Profile}};
 for(const auto& record:records){status=properties->AddProperty(record.first,record.second);if(status)return status;}
 status=ravet::aax::AaxNativeTiming::declareInitial(properties,Profile>=4?8:0);if(status)return status;
 status=component->AddProcessProc_Native<Context>(Model::process,properties,nullptr,nullptr);if(status)return status;
 status=descriptor->AddComponent(component);if(status)return status;
 return host.AddEffect(RawRoutingIDs[Profile],descriptor);
}
}
__attribute__((visibility("default"))) AAX_Result GetEffectDescriptions(AAX_ICollection* host){
 if(!host)return AAX_ERROR_NULL_ARGUMENT;auto status=host->SetManufacturerName("RAVET");if(status)return status;status=host->AddPackageName("RAVET Raw Routing");if(status)return status;status=host->SetPackageVersion(1);if(status)return status;
 status=variant<0,RavetRawMainModel>(*host);if(status)return status;status=variant<1,RavetRawMainModel>(*host);if(status)return status;status=variant<2,RavetRawAuxModel>(*host);if(status)return status;status=variant<3,RavetRawAuxModel>(*host);if(status)return status;status=variant<4,RavetRawTimedModel>(*host);if(status)return status;return variant<5,RavetRawTimedModel>(*host);
}
