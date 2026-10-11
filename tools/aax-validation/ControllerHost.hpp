#pragma once
#include "AAX_IACFController.h"
#include "AAX_IACFAutomationDelegate.h"
#include "AAX_IString.h"
#include "AAX_UIDs.h"
#include "acfresult.h"
#include <cstring>
#include <functional>
#include <set>
#include <string>

// Synthetic host implements public versioned SDK interfaces. No private SDK
// layout or product fixture is used. Stack lifetime exceeds every SDK borrower.
class ControllerHost final : public AAX_IACFController, public AAX_IACFAutomationDelegate {
public:
    acfUInt32 references = 1;
    mutable std::function<void()> onCurrent;
    std::function<void()> onRegister;
    bool exposeController = true, exposeAutomation = true;
    AAX_CSampleRate rate = 48000.;
    AAX_EStemFormat input = AAX_eStemFormat_Mono, output = AAX_eStemFormat_Mono;
    AAX_Result rateStatus = AAX_SUCCESS, inputStatus = AAX_SUCCESS, outputStatus = AAX_SUCCESS;
    std::string effect = "org.ravet.gain.mono";
    std::set<std::string> registered;
    uint32_t rateQueries=0,inputQueries=0,outputQueries=0,latencyUpdates=0;
    uint32_t touches=0,releases=0; mutable uint32_t requests=0,currentValues=0;
    ACFRESULT ACFMETHODCALLTYPE QueryInterface(const acfIID &iid, void **value) override {
        if (!value) return ACF_E_INVALIDARG;
        *value = nullptr;
        if (exposeController && !std::memcmp(&iid,&IID_IAAXControllerV1,sizeof(iid))) *value=static_cast<AAX_IACFController*>(this);
        else if (exposeAutomation && !std::memcmp(&iid,&IID_IAAXAutomationDelegateV1,sizeof(iid))) *value=static_cast<AAX_IACFAutomationDelegate*>(this);
        if (!*value) return ACF_E_NOINTERFACE;
        AddRef(); return ACF_OK;
    }
    acfUInt32 ACFMETHODCALLTYPE AddRef() override { return ++references; }
    acfUInt32 ACFMETHODCALLTYPE Release() override { return --references; }
    AAX_Result GetEffectID(AAX_IString *value) const override {if(!value)return AAX_ERROR_NULL_ARGUMENT;value->Set(effect.c_str());return AAX_SUCCESS;}
    AAX_Result GetSampleRate(AAX_CSampleRate *value) const override {++const_cast<ControllerHost*>(this)->rateQueries;if(rateStatus)return rateStatus;if(!value)return AAX_ERROR_NULL_ARGUMENT;*value=rate;return AAX_SUCCESS;}
    AAX_Result GetInputStemFormat(AAX_EStemFormat *value) const override {++const_cast<ControllerHost*>(this)->inputQueries;if(inputStatus)return inputStatus;if(!value)return AAX_ERROR_NULL_ARGUMENT;*value=input;return AAX_SUCCESS;}
    AAX_Result GetOutputStemFormat(AAX_EStemFormat *value) const override {++const_cast<ControllerHost*>(this)->outputQueries;if(outputStatus)return outputStatus;if(!value)return AAX_ERROR_NULL_ARGUMENT;*value=output;return AAX_SUCCESS;}
    AAX_Result GetSignalLatency(int32_t *value) const override {if(value)*value=0;return value?AAX_SUCCESS:AAX_ERROR_NULL_ARGUMENT;}
    AAX_Result GetCycleCount(AAX_EProperty,AAX_CPropertyValue*)const override{return AAX_ERROR_UNIMPLEMENTED;}
    AAX_Result GetTODLocation(AAX_CTimeOfDay*)const override{return AAX_ERROR_UNIMPLEMENTED;}
    AAX_Result SetSignalLatency(int32_t value)override{if(value<0)return AAX_ERROR_INVALID_ARGUMENT;++latencyUpdates;return AAX_SUCCESS;}
    AAX_Result SetCycleCount(AAX_EProperty*,AAX_CPropertyValue*,int32_t)override{return AAX_ERROR_UNIMPLEMENTED;}
    AAX_Result PostPacket(AAX_CFieldIndex,const void*,uint32_t)override{return AAX_ERROR_UNIMPLEMENTED;}
    AAX_Result GetCurrentMeterValue(AAX_CTypeID,float*)const override{return AAX_ERROR_UNIMPLEMENTED;}
    AAX_Result GetMeterPeakValue(AAX_CTypeID,float*)const override{return AAX_ERROR_UNIMPLEMENTED;}
    AAX_Result ClearMeterPeakValue(AAX_CTypeID)const override{return AAX_ERROR_UNIMPLEMENTED;}
    AAX_Result GetMeterClipped(AAX_CTypeID,AAX_CBoolean*)const override{return AAX_ERROR_UNIMPLEMENTED;}
    AAX_Result ClearMeterClipped(AAX_CTypeID)const override{return AAX_ERROR_UNIMPLEMENTED;}
    AAX_Result GetMeterCount(uint32_t *value)const override{if(value)*value=0;return value?AAX_SUCCESS:AAX_ERROR_NULL_ARGUMENT;}
    AAX_Result GetNextMIDIPacket(AAX_CFieldIndex*,AAX_CMidiPacket*)override{return AAX_ERROR_UNIMPLEMENTED;}
    AAX_Result RegisterParameter(AAX_CParamID id)override{if(onRegister)onRegister();return registered.insert(id).second?AAX_SUCCESS:AAX_ERROR_INVALID_PARAMETER_ID;}
    AAX_Result UnregisterParameter(AAX_CParamID id)override{registered.erase(id);return AAX_SUCCESS;}
    AAX_Result PostSetValueRequest(AAX_CParamID,double)const override{++requests;return AAX_SUCCESS;}
    AAX_Result PostCurrentValue(AAX_CParamID,double)const override{++currentValues;if(onCurrent)onCurrent();return AAX_SUCCESS;}
    AAX_Result PostTouchRequest(AAX_CParamID)override{++touches;return AAX_SUCCESS;}
    AAX_Result PostReleaseRequest(AAX_CParamID)override{++releases;return AAX_SUCCESS;}
    AAX_Result GetTouchState(AAX_CParamID,AAX_CBoolean *value)override{if(value)*value=false;return value?AAX_SUCCESS:AAX_ERROR_NULL_ARGUMENT;}
    IACFUnknown *unknown() { return static_cast<AAX_IACFController*>(this); }
};
