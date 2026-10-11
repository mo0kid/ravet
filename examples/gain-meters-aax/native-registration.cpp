#include "GainMeterParameters.hpp"
#include "RavetAaxMeterRegistration.hpp"
#include "AAX_ICollection.h"
#include "AAX_IEffectDescriptor.h"
#include "AAX_IComponentDescriptor.h"
#include "AAX_IPropertyMap.h"
#include "AAX_Properties.h"
namespace {
const ravet::aax::AaxPreparedMeters<6>& preparedMeters(){
 // Borrowed descriptor strings/IDs retain process lifetime. Metadata queries and
 // unavoidable cache retirement complete on CONTROL before any SDK mutation.
 static const ravet::aax::AaxPreparedMeters<6> value(ravet_native_processor_v1(RAVET_AAX_NATIVE_SAMPLE_BITS),ravet_native_telemetry_v1(RAVET_AAX_NATIVE_SAMPLE_BITS),gainMeterBindings().data());return value;
}
AAX_IEffectParameters *AAX_CALLBACK mono(){try{return createGainMeters(1);}catch(...){return nullptr;}}
AAX_IEffectParameters *AAX_CALLBACK stereo(){try{return createGainMeters(2);}catch(...){return nullptr;}}
AAX_Result variant(AAX_ICollection &host,uint32_t channels){
 auto *descriptor=host.NewDescriptor();if(!descriptor)return AAX_ERROR_NULL_OBJECT;
 auto status=descriptor->AddName("RAVET Gain Meters");if(status)return status;
 status=descriptor->AddCategory(AAX_ePlugInCategory_Dynamics);if(status)return status;
 status=descriptor->AddProcPtr(reinterpret_cast<void*>(channels==1?mono:stereo),kAAX_ProcPtrID_Create_EffectParameters);if(status)return status;
 auto *component=descriptor->NewComponentDescriptor();if(!component)return AAX_ERROR_NULL_OBJECT;
 using Model=RavetGainMetersAaxParameters;using Context=Model::Context;
 status=component->AddPrivateData(AAX_FIELD_INDEX(Context,owner),sizeof(Model::Owner*));if(status)return status;
 status=component->AddAudioBufferLength(AAX_FIELD_INDEX(Context,frames));if(status)return status;
 status=component->AddAudioIn(AAX_FIELD_INDEX(Context,inputs));if(status)return status;
 status=component->AddAudioOut(AAX_FIELD_INDEX(Context,outputs));if(status)return status;
 const auto registered=ravet::aax::registerAaxMeters(*descriptor,*component,AAX_FIELD_INDEX(Context,meterTaps),preparedMeters());if(!registered)return registered.status;

 auto *properties=component->NewPropertyMap();if(!properties)return AAX_ERROR_NULL_OBJECT;
 const std::pair<AAX_EProperty,AAX_CPropertyValue> records[]{
  {AAX_eProperty_ManufacturerID,0x52617674},{AAX_eProperty_ProductID,0x52764d47},
  {AAX_eProperty_CanBypass,1},{AAX_eProperty_LatencyContribution,0},
  {AAX_eProperty_InputStemFormat,channels==1?AAX_eStemFormat_Mono:AAX_eStemFormat_Stereo},
  {AAX_eProperty_OutputStemFormat,channels==1?AAX_eStemFormat_Mono:AAX_eStemFormat_Stereo},
  {AAX_eProperty_PlugInID_Native,channels==1?0x524d4731:0x524d4732}};
 for(const auto &record:records){status=properties->AddProperty(record.first,record.second);if(status)return status;}
 status=component->AddProcessProc_Native<Context>(Model::process,properties,nullptr,nullptr);if(status)return status;
 status=descriptor->AddComponent(component);if(status)return status;
 return host.AddEffect(channels==1?"org.ravet.gainmeters.mono":"org.ravet.gainmeters.stereo",descriptor);
}
}
#if defined(_WIN32)
__declspec(dllexport)
#else
__attribute__((visibility("default")))
#endif
AAX_Result GetEffectDescriptions(AAX_ICollection *host){
 if(!host)return AAX_ERROR_NULL_ARGUMENT;
 if(!preparedMeters().valid())return AAX_ERROR_INVALID_ARGUMENT;
 // Exact first refusal is retained. Prior registration calls have no rollback.
 auto status=host->SetManufacturerName("RAVET");if(status)return status;
 status=host->AddPackageName("RAVET Gain Meters");if(status)return status;
 status=host->SetPackageVersion(1);if(status)return status;
 status=variant(*host,1);if(status)return status;return variant(*host,2);
}
