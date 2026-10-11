#pragma once
#include "RavetAaxMeters.hpp"
#include "AAX_IEffectDescriptor.h"
#include "AAX_IComponentDescriptor.h"
#include "AAX_IPropertyMap.h"
#include "AAX_Properties.h"
namespace ravet::aax {
struct AaxMeterRegistrationResult {
    enum class Step:uint32_t{None,Preflight,Field,PropertyMap,Type,Orientation,Ballistics,Description,Complete};
    Step step=Step::Preflight;AAX_Result status=AAX_ERROR_INVALID_ARGUMENT;uint32_t descriptions=0;
    explicit operator bool()const noexcept{return step==Step::Complete&&status==AAX_SUCCESS;}
};
// CONTROL only. Caller retains immutable prepared names/IDs through any SDK
// descriptor borrowers and uses exactly one field per component. Host owns its
// NewPropertyMap objects; original returned refusal is preserved, and prior SDK
// registration calls have no invented rollback. Layout/index comes from the
// caller's actual public typed Context via AAX_FIELD_INDEX, never guessed offsets.
template<size_t Count>AaxMeterRegistrationResult registerAaxMeters(AAX_IEffectDescriptor& effect,AAX_IComponentDescriptor& component,AAX_CFieldIndex field,const AaxPreparedMeters<Count>& prepared){
    AaxMeterRegistrationResult result{};
    if(!prepared.valid()||field<0)return result;
    result.step=AaxMeterRegistrationResult::Step::Field;
    result.status=component.AddMeters(field,prepared.meterIds(),uint32_t(Count));if(result.status)return result;
    for(uint32_t index=0;index<Count;++index){
        const auto* binding=prepared.binding(index);result.step=AaxMeterRegistrationResult::Step::PropertyMap;
        auto* properties=effect.NewPropertyMap();if(!properties){result.status=AAX_ERROR_NULL_OBJECT;return result;}
        result.step=AaxMeterRegistrationResult::Step::Type;result.status=properties->AddProperty(AAX_eProperty_Meter_Type,binding->type);if(result.status)return result;
        result.step=AaxMeterRegistrationResult::Step::Orientation;result.status=properties->AddProperty(AAX_eProperty_Meter_Orientation,binding->orientation);if(result.status)return result;
        result.step=AaxMeterRegistrationResult::Step::Ballistics;result.status=properties->AddProperty(AAX_eProperty_Meter_Ballistics,binding->ballistics);if(result.status)return result;
        result.step=AaxMeterRegistrationResult::Step::Description;result.status=effect.AddMeterDescription(binding->meterId,binding->name,properties);if(result.status)return result;
        ++result.descriptions;
    }
    result.step=AaxMeterRegistrationResult::Step::Complete;result.status=AAX_SUCCESS;return result;
}
} // namespace ravet::aax
