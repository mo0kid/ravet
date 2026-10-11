#pragma once
#include "public.sdk/source/vst/aaxwrapper/aaxwrapper_parameters.h"
#include "RavetGain.h"
#include "RavetAaxParameters.hpp"

// Owned data model around public external SDK virtual callbacks. The exact stock
// wrapper continues to own algorithm/state/transport; this adopts native schema.
class RavetGainAaxParameters final : public AAXWrapper_Parameters {
    ravet::aax::NativeAaxParameters<2> schema_;
    bool ready_ = false;
    bool declared(AAX_CParamID id) const { return ready_ && schema_.info(id); }
    bool writable(AAX_CParamID id) const { const auto *p = ready_ ? schema_.info(id) : nullptr; return p && !(p->flags & RAVET_PARAMETER_READ_ONLY); }
public:
    explicit RavetGainAaxParameters(int32_t index) : AAXWrapper_Parameters(index), schema_(ravet_native_cached_parameters_v1(32)) {}
    ~RavetGainAaxParameters() override { ready_ = false; mParameterManager.RemoveAllParameters(); }
    AAX_Result EffectInit() override {
        if (mParameterManager.NumParameters()) return AAX_ERROR_INVALID_ARGUMENT;
        ready_ = false;
        try {
            if (!schema_.valid() || !getWrapper() || !schema_.masterBypass()) return AAX_ERROR_NULL_OBJECT;
            for (uint32_t i = 0; i < schema_.count(); ++i) if (schema_.info(schema_.id(i))->id == UINT32_MAX) return AAX_ERROR_INVALID_PARAMETER_ID;
            ravet::aax::NativeAaxParameters<2>::Prepared objects{};
            if (!schema_.prepare(objects)) return AAX_ERROR_NULL_OBJECT;
            const auto status = AAXWrapper_Parameters::EffectInit();
            if (status != AAX_SUCCESS) return status;
            if (mParameterManager.NumParameters() != int32_t(schema_.count())) return AAX_ERROR_INVALID_PARAMETER_ID;
            for (uint32_t i = 0; i < schema_.count(); ++i) {
                const auto *parameter = mParameterManager.GetParameter(int32_t(i));
                if (!parameter || std::strcmp(parameter->Identifier(), schema_.id(i))) return AAX_ERROR_INVALID_PARAMETER_ID;
            }
            mParameterManager.RemoveAllParameters();
            if (!schema_.registerEmpty(mParameterManager, objects)) return AAX_ERROR_NULL_OBJECT;
            ready_ = true; return AAX_SUCCESS;
        } catch (...) { mParameterManager.RemoveAllParameters(); return AAX_ERROR_NULL_OBJECT; }
    }
    AAX_Result Uninitialize() override {
        ready_ = false;
        return AAXWrapper_Parameters::Uninitialize();
    }
    AAX_Result GetNumberOfParameters(int32_t *output) const override {
        if (!ready_ || !output) return AAX_ERROR_NULL_ARGUMENT;
        *output = int32_t(schema_.count()); return AAX_SUCCESS;
    }
    AAX_Result GetMasterBypassParameter(AAX_IString *output) const override {
        const auto *id = ready_ ? schema_.masterBypass() : nullptr;
        if (!id || !output) return AAX_ERROR_INVALID_PARAMETER_ID;
        if (output->MaxLength() < std::strlen(id)) return AAX_ERROR_INVALID_ARGUMENT;
        output->Set(id); return AAX_SUCCESS;
    }
    AAX_Result GetParameter(AAX_CParamID id, AAX_IParameter **output) override {
        if (!declared(id)) return AAX_ERROR_INVALID_PARAMETER_ID;
        if (!output) return AAX_ERROR_NULL_ARGUMENT;
        const auto next = mParameterManager.GetParameterByID(id);
        if (!next) return AAX_ERROR_INVALID_PARAMETER_ID;
        *output = next; return AAX_SUCCESS;
    }
    AAX_Result GetParameterIndex(AAX_CParamID id, int32_t *output) const override {
        if (!declared(id)) return AAX_ERROR_INVALID_PARAMETER_ID;
        if (!output) return AAX_ERROR_NULL_ARGUMENT;
        *output = int32_t(schema_.index(id)); return AAX_SUCCESS;
    }
    AAX_Result GetParameterIDFromIndex(int32_t index, AAX_IString *output) const override {
        if (!ready_ || index < 0 || uint32_t(index) >= schema_.count()) return AAX_ERROR_INVALID_PARAMETER_ID;
        if (!output) return AAX_ERROR_NULL_ARGUMENT;
        const char *id = schema_.id(uint32_t(index));
        if (output->MaxLength() < std::strlen(id)) return AAX_ERROR_INVALID_ARGUMENT;
        output->Set(id); return AAX_SUCCESS;
    }
    AAX_Result GetParameterName(AAX_CParamID id, AAX_IString *output) const override {
        if (!declared(id)) return AAX_ERROR_INVALID_PARAMETER_ID;
        if (!output) return AAX_ERROR_NULL_ARGUMENT;
        const auto *info = schema_.info(id);
        if (output->MaxLength() < info->name_length) return AAX_ERROR_INVALID_ARGUMENT;
        output->Set(reinterpret_cast<const char *>(info->name)); return AAX_SUCCESS;
    }
    AAX_Result GetParameterNameOfLength(AAX_CParamID id, AAX_IString *output, int32_t length) const override {
        if (!declared(id)) return AAX_ERROR_INVALID_PARAMETER_ID;
        if (length <= 0 || uint32_t(length) < schema_.info(id)->name_length) return AAX_ERROR_INVALID_ARGUMENT;
        return GetParameterName(id, output);
    }
    AAX_Result GetParameterIsAutomatable(AAX_CParamID id, AAX_CBoolean *output) const override {
        if (!declared(id)) return AAX_ERROR_INVALID_PARAMETER_ID;
        if (!output) return AAX_ERROR_NULL_ARGUMENT;
        *output = writable(id) && (schema_.info(id)->flags & RAVET_PARAMETER_AUTOMATABLE); return AAX_SUCCESS;
    }
    AAX_Result GetParameterNumberOfSteps(AAX_CParamID id, int32_t *output) const override {
        if (!declared(id)) return AAX_ERROR_INVALID_PARAMETER_ID;
        if (!output) return AAX_ERROR_NULL_ARGUMENT;
        *output = int32_t(mParameterManager.GetParameterByID(id)->GetNumberOfSteps()); return AAX_SUCCESS;
    }
    AAX_Result GetParameterDefaultNormalizedValue(AAX_CParamID id, double *output) const override {
        if (!declared(id)) return AAX_ERROR_INVALID_PARAMETER_ID;
        if (!output) return AAX_ERROR_NULL_ARGUMENT;
        return schema_.defaultValue(id, *output) ? AAX_SUCCESS : AAX_ERROR_INVALID_PARAMETER_ID;
    }
    AAX_Result SetParameterDefaultNormalizedValue(AAX_CParamID id, double) override {
        return declared(id) ? AAX_ERROR_UNIMPLEMENTED : AAX_ERROR_INVALID_PARAMETER_ID;
    }
    AAX_Result GetParameterType(AAX_CParamID id, AAX_EParameterType *output) const override {
        if (!declared(id)) return AAX_ERROR_INVALID_PARAMETER_ID;
        if (!output) return AAX_ERROR_NULL_ARGUMENT;
        *output = mParameterManager.GetParameterByID(id)->GetType(); return AAX_SUCCESS;
    }
    AAX_Result GetParameterOrientation(AAX_CParamID id, AAX_EParameterOrientation *output) const override {
        if (!declared(id)) return AAX_ERROR_INVALID_PARAMETER_ID;
        if (!output) return AAX_ERROR_NULL_ARGUMENT;
        *output = mParameterManager.GetParameterByID(id)->GetOrientation(); return AAX_SUCCESS;
    }
    AAX_Result GetParameterValueInfo(AAX_CParamID id, int32_t selector, int32_t *output) const override {
        if (!declared(id)) return AAX_ERROR_INVALID_PARAMETER_ID;
        if (!output) return AAX_ERROR_NULL_ARGUMENT;
        int32_t next = 0; const auto status = AAXWrapper_Parameters::GetParameterValueInfo(id, selector, &next);
        if (status == AAX_SUCCESS) *output = next;
        return status;
    }
    AAX_Result GetParameterNormalizedValue(AAX_CParamID id, double *output) const override {
        if (!declared(id)) return AAX_ERROR_INVALID_PARAMETER_ID;
        if (!output) return AAX_ERROR_NULL_ARGUMENT;
        double next = 0.; const auto status = AAXWrapper_Parameters::GetParameterNormalizedValue(id, &next);
        if (status != AAX_SUCCESS) return status;
        return schema_.value(id, next, *output) ? AAX_SUCCESS : AAX_ERROR_INVALID_ARGUMENT;
    }
    AAX_Result SetParameterNormalizedValue(AAX_CParamID id, double input) override {
        double next = 0.;
        if (!writable(id)) return AAX_ERROR_INVALID_PARAMETER_ID;
        if (!schema_.control(id, input, next)) return AAX_ERROR_INVALID_ARGUMENT;
        return AAXWrapper_Parameters::SetParameterNormalizedValue(id, next);
    }
    AAX_Result UpdateParameterNormalizedValue(AAX_CParamID id, double input, AAX_EUpdateSource source) override {
        double next = 0.;
        if (!writable(id)) return AAX_ERROR_INVALID_PARAMETER_ID;
        if (!schema_.control(id, input, next)) return AAX_ERROR_INVALID_ARGUMENT;
        return AAXWrapper_Parameters::UpdateParameterNormalizedValue(id, next, source);
    }
    AAX_Result SetParameterNormalizedRelative(AAX_CParamID id, double delta) override {
        if (!writable(id)) return AAX_ERROR_INVALID_PARAMETER_ID;
        if (!ravet::native::scalar::finite(delta)) return AAX_ERROR_INVALID_ARGUMENT;
        double previous = 0.; const auto status = GetParameterNormalizedValue(id, &previous);
        if (status != AAX_SUCCESS) return status;
        return SetParameterNormalizedValue(id, previous + delta);
    }
    AAX_Result GetParameterValueFromString(AAX_CParamID id, double *output, const AAX_IString &text) const override {
        if (!declared(id)) return AAX_ERROR_INVALID_PARAMETER_ID;
        if (!output) return AAX_ERROR_NULL_ARGUMENT;
        return schema_.parse(id, text, *output) ? AAX_SUCCESS : AAX_ERROR_INVALID_ARGUMENT;
    }
    AAX_Result GetParameterStringFromValue(AAX_CParamID id, double value, AAX_IString *output, int32_t maximum) const override {
        if (!declared(id)) return AAX_ERROR_INVALID_PARAMETER_ID;
        if (!output) return AAX_ERROR_NULL_ARGUMENT;
        return schema_.format(id, value, maximum, *output) ? AAX_SUCCESS : AAX_ERROR_INVALID_ARGUMENT;
    }
    AAX_Result GetParameterValueString(AAX_CParamID id, AAX_IString *output, int32_t maximum) const override {
        double value = 0.; const auto status = GetParameterNormalizedValue(id, &value);
        return status == AAX_SUCCESS ? GetParameterStringFromValue(id, value, output, maximum) : status;
    }
    AAX_Result TouchParameter(AAX_CParamID id) override { return writable(id) ? AAXWrapper_Parameters::TouchParameter(id) : AAX_ERROR_INVALID_PARAMETER_ID; }
    AAX_Result ReleaseParameter(AAX_CParamID id) override { return writable(id) ? AAXWrapper_Parameters::ReleaseParameter(id) : AAX_ERROR_INVALID_PARAMETER_ID; }
    AAX_Result UpdateParameterTouch(AAX_CParamID id, AAX_CBoolean touch) override {
        if (!writable(id)) return AAX_ERROR_INVALID_PARAMETER_ID;
        return touch == 0 || touch == 1 ? AAXWrapper_Parameters::UpdateParameterTouch(id, touch) : AAX_ERROR_INVALID_ARGUMENT;
    }
};
