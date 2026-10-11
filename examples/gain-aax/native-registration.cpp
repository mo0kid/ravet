#include "GainNativeParameters.hpp"
#if RAVET_AAX_NATIVE_EDITOR
#include "GainNativeEditor.hpp"
#endif
#include "AAX_ICollection.h"
#include "AAX_IEffectDescriptor.h"
#include "AAX_IComponentDescriptor.h"
#include "AAX_IPropertyMap.h"
#include "AAX_Properties.h"
namespace {
#if RAVET_AAX_NATIVE_EDITOR
AAX_IEffectGUI *AAX_CALLBACK gui(){try{return createGainNativeEditor();}catch(...){return nullptr;}}
#endif
AAX_IEffectParameters *AAX_CALLBACK mono(){try{return createGainNative(1);}catch(...){return nullptr;}}
AAX_IEffectParameters *AAX_CALLBACK stereo(){try{return createGainNative(2);}catch(...){return nullptr;}}
AAX_Result variant(AAX_ICollection &host,uint32_t channels){
 auto *descriptor=host.NewDescriptor();if(!descriptor)return AAX_ERROR_NULL_OBJECT;
 auto status=descriptor->AddName("RAVET Gain");if(status)return status;
 status=descriptor->AddCategory(AAX_ePlugInCategory_Dynamics);if(status)return status;
 status=descriptor->AddProcPtr(reinterpret_cast<void*>(channels==1?mono:stereo),kAAX_ProcPtrID_Create_EffectParameters);if(status)return status;
 #if RAVET_AAX_NATIVE_EDITOR
 status=descriptor->AddProcPtr(reinterpret_cast<void*>(gui),kAAX_ProcPtrID_Create_EffectGUI);if(status)return status;
#endif
 auto *component=descriptor->NewComponentDescriptor();if(!component)return AAX_ERROR_NULL_OBJECT;
 using Model=RavetGainNativeAaxParameters;using Context=Model::Context;
 status=component->AddPrivateData(AAX_FIELD_INDEX(Context,owner),sizeof(Model::Owner*));if(status)return status;
 status=component->AddAudioBufferLength(AAX_FIELD_INDEX(Context,frames));if(status)return status;
 status=component->AddAudioIn(AAX_FIELD_INDEX(Context,inputs));if(status)return status;
 status=component->AddAudioOut(AAX_FIELD_INDEX(Context,outputs));if(status)return status;
 auto *properties=component->NewPropertyMap();if(!properties)return AAX_ERROR_NULL_OBJECT;
 const std::pair<AAX_EProperty,AAX_CPropertyValue> records[]{
  {AAX_eProperty_ManufacturerID,0x52617674},{AAX_eProperty_ProductID,0x5276476e},
  {AAX_eProperty_CanBypass,1},{AAX_eProperty_LatencyContribution,0},
  {AAX_eProperty_InputStemFormat,channels==1?AAX_eStemFormat_Mono:AAX_eStemFormat_Stereo},
  {AAX_eProperty_OutputStemFormat,channels==1?AAX_eStemFormat_Mono:AAX_eStemFormat_Stereo},
  {AAX_eProperty_PlugInID_Native,channels==1?0x52764731:0x52764732}};
 for(const auto &record:records){status=properties->AddProperty(record.first,record.second);if(status)return status;}
 status=component->AddProcessProc_Native<Context>(Model::process,properties,nullptr,nullptr);if(status)return status;
 status=descriptor->AddComponent(component);if(status)return status;
 return host.AddEffect(channels==1?"org.ravet.gain.mono":"org.ravet.gain.stereo",descriptor);
}
}
#if defined(_WIN32)
__declspec(dllexport)
#else
__attribute__((visibility("default")))
#endif
AAX_Result GetEffectDescriptions(AAX_ICollection *host){
 if(!host)return AAX_ERROR_NULL_ARGUMENT;
 // Exact first refusal is retained. Prior registration calls have no rollback.
 auto status=host->SetManufacturerName("RAVET");if(status)return status;
 status=host->AddPackageName("RAVET Gain");if(status)return status;
 status=host->SetPackageVersion(1);if(status)return status;
 status=variant(*host,1);if(status)return status;return variant(*host,2);
}
