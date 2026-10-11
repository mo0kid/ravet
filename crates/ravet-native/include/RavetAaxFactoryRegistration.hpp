#pragma once
#include "AAX_ICollection.h"
#include "AAX_IEffectDescriptor.h"
#include "AAX_Errors.h"
#include <array>
#include <cstring>
#include <utility>

namespace ravet::aax {
struct NativeAaxParameterFactory { const char *effectId; void *create; };
// CONTROL-only forwarding adapter around borrowed exact SDK descriptor objects.
// Retain host objects/callback libraries through SDK registration and instances.
// Only the initial Create_EffectParameters call is deferred until AddEffect's
// exact public effect identity selects a replacement; descriptor order is irrelevant.
// Original callback is retained for unknown IDs. Every actual host status forwards
// unchanged; registration timing changes and prior host effects have no rollback.
// Owns proxies/copies only, never host descriptors/components/property maps.
template <size_t MaximumFactories = 16, size_t MaximumDescriptors = 16>
class NativeAaxFactoryRegistration final : public AAX_ICollection {
    static_assert(MaximumFactories > 0 && MaximumFactories <= 16 && MaximumDescriptors > 0 && MaximumDescriptors <= 16);
    class Descriptor final : public AAX_IEffectDescriptor {
    public:
        AAX_IEffectDescriptor *target = nullptr;
        void *deferred = nullptr;
        bool submitted = false;
        AAX_IComponentDescriptor *NewComponentDescriptor() override { return target->NewComponentDescriptor(); }
        AAX_Result AddComponent(AAX_IComponentDescriptor *p) override { return target->AddComponent(p); }
        AAX_Result AddName(const char *p) override { return target->AddName(p); }
        AAX_Result AddCategory(uint32_t p) override { return target->AddCategory(p); }
        AAX_Result AddCategoryBypassParameter(uint32_t category, AAX_CParamID id) override { return target->AddCategoryBypassParameter(category, id); }
        AAX_Result AddProcPtr(void *p, AAX_CProcPtrID kind) override {
            if (submitted) return AAX_ERROR_INVALID_ARGUMENT;
            if (kind != kAAX_ProcPtrID_Create_EffectParameters) return target->AddProcPtr(p, kind);
            if (deferred || !p) return AAX_ERROR_INVALID_ARGUMENT;
            deferred = p; return AAX_SUCCESS;
        }
        AAX_IPropertyMap *NewPropertyMap() override { return target->NewPropertyMap(); }
        AAX_Result SetProperties(AAX_IPropertyMap *p) override { return target->SetProperties(p); }
        AAX_Result AddResourceInfo(AAX_EResourceType t, const char *p) override { return target->AddResourceInfo(t, p); }
        AAX_Result AddMeterDescription(AAX_CTypeID t, const char *name, AAX_IPropertyMap *p) override { return target->AddMeterDescription(t, name, p); }
        AAX_Result AddControlMIDINode(AAX_CTypeID t, AAX_EMIDINodeType kind, const char name[], uint32_t mask) override { return target->AddControlMIDINode(t, kind, name, mask); }
    };
    AAX_ICollection &target_;
    std::array<std::array<char, 257>, MaximumFactories> ids_{};
    std::array<void *, MaximumFactories> factories_{};
    std::array<Descriptor, MaximumDescriptors> descriptors_{};
    size_t count_ = 0, used_ = 0;
    bool valid_ = false;
public:
    NativeAaxFactoryRegistration(AAX_ICollection &target, const NativeAaxParameterFactory *profiles, size_t count) : target_(target) {
        if (!profiles || !count || count > MaximumFactories) return;
        for (size_t i = 0; i < count; ++i) {
            if (!profiles[i].effectId || !profiles[i].create) return;
            size_t length = 0; while (length < 257 && profiles[i].effectId[length]) ++length;
            if (!length || length > 256) return;
            for (size_t j = 0; j < i; ++j) if (!std::strcmp(profiles[i].effectId, ids_[j].data())) return;
            std::memcpy(ids_[i].data(), profiles[i].effectId, length); factories_[i] = profiles[i].create;
        }
        count_ = count; valid_ = true;
    }
    NativeAaxFactoryRegistration(const NativeAaxFactoryRegistration &) = delete;
    NativeAaxFactoryRegistration &operator=(const NativeAaxFactoryRegistration &) = delete;
    bool valid() const { return valid_; }
    AAX_IEffectDescriptor *NewDescriptor() override {
        if (!valid_ || used_ == MaximumDescriptors) return nullptr;
        auto *next = target_.NewDescriptor(); if (!next) return nullptr;
        auto &proxy = descriptors_[used_++]; proxy.target = next; return &proxy;
    }
    AAX_Result AddEffect(const char *id, AAX_IEffectDescriptor *value) override {
        if (!valid_ || !id || !value) return AAX_ERROR_NULL_ARGUMENT;
        Descriptor *proxy = nullptr;
        for (size_t i = 0; i < used_; ++i) if (&descriptors_[i] == value) proxy = &descriptors_[i];
        if (!proxy || proxy->submitted) return AAX_ERROR_INVALID_ARGUMENT;
        void *selected = proxy->deferred; bool known = false;
        for (size_t i = 0; i < count_; ++i) if (!std::strcmp(id, ids_[i].data())) { selected = factories_[i]; known = true; break; }
        if (known && !proxy->deferred) return AAX_ERROR_NULL_OBJECT;
        // Consume the proxy before borrowing it in a host call: host refusal may
        // already have effects, and reentrant/repeated registration must not retry.
        proxy->submitted = true;
        if (selected) {
            const auto status = proxy->target->AddProcPtr(selected, kAAX_ProcPtrID_Create_EffectParameters);
            if (status != AAX_SUCCESS) return status;
        }
        return target_.AddEffect(id, proxy->target);
    }
    AAX_Result SetManufacturerName(const char *p) override { return target_.SetManufacturerName(p); }
    AAX_Result AddPackageName(const char *p) override { return target_.AddPackageName(p); }
    AAX_Result SetPackageVersion(uint32_t p) override { return target_.SetPackageVersion(p); }
    AAX_IPropertyMap *NewPropertyMap() override { return target_.NewPropertyMap(); }
    AAX_Result SetProperties(AAX_IPropertyMap *p) override { return target_.SetProperties(p); }
    AAX_Result GetHostVersion(uint32_t *p) const override { return target_.GetHostVersion(p); }
    AAX_IDescriptionHost *DescriptionHost() override { return target_.DescriptionHost(); }
    const AAX_IDescriptionHost *DescriptionHost() const override { return std::as_const(target_).DescriptionHost(); }
    IACFDefinition *HostDefinition() const override { return target_.HostDefinition(); }
};
} // namespace ravet::aax
