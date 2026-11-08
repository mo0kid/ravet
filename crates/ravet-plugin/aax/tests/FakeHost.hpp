#pragma once
// Synthetic AAX host pieces for the generic adapter's checks. They implement the
// public SDK interfaces only; no private SDK layout is used. This is not Avid's
// validator (DSH/aaxval) and says nothing about Pro Tools.
#include "TransportControllerHost.hpp"
#include "AAX_ICollection.h"
#include "AAX_IComponentDescriptor.h"
#include "AAX_IEffectDescriptor.h"
#include "AAX_IEffectParameters.h"
#include "AAX_IMIDINode.h"
#include "AAX_IPropertyMap.h"
#include "AAX_IACFViewContainer.h"
#include <algorithm>
#include <array>
#include <cstdint>
#include <cstring>
#include <cstdio>
#include <cstdlib>
#include <map>
#include <memory>
#include <stdexcept>
#include <string>
#include <vector>

namespace fake {

inline void require(bool ok, const std::string &what) {
    if (!ok) throw std::runtime_error(what);
}

struct Properties final : AAX_IPropertyMap {
    std::map<AAX_EProperty, AAX_CPropertyValue> values;
    AAX_CBoolean GetProperty(AAX_EProperty p, AAX_CPropertyValue *v) const override {
        const auto i = values.find(p);
        if (i == values.end() || !v) return false;
        *v = i->second;
        return true;
    }
    AAX_CBoolean GetPointerProperty(AAX_EProperty, const void **) const override { return false; }
    AAX_Result AddProperty(AAX_EProperty p, AAX_CPropertyValue v) override { values[p] = v; return AAX_SUCCESS; }
    AAX_Result AddPointerProperty(AAX_EProperty, const void *) override { return AAX_SUCCESS; }
    AAX_Result AddPointerProperty(AAX_EProperty, const char *) override { return AAX_SUCCESS; }
    AAX_Result RemoveProperty(AAX_EProperty p) override { values.erase(p); return AAX_SUCCESS; }
    AAX_Result AddPropertyWithIDArray(AAX_EProperty, const AAX_SPlugInIdentifierTriad *, uint32_t) override { return AAX_ERROR_UNIMPLEMENTED; }
    AAX_CBoolean GetPropertyWithIDArray(AAX_EProperty, const AAX_SPlugInIdentifierTriad **, uint32_t *) const override { return false; }
    IACFUnknown *GetIUnknown() override { return nullptr; }
    int32_t at(AAX_EProperty p) const {
        const auto i = values.find(p);
        require(i != values.end(), "missing property " + std::to_string(int(p)));
        return i->second;
    }
};

struct MidiNodeRecord { AAX_EMIDINodeType type; std::string name; uint32_t mask; };
struct AuxOutputStem { int32_t stem; std::string name; };

inline uint32_t stemChannels(int32_t stem) {
    return stem == AAX_eStemFormat_None ? 0 : AAX_STEM_FORMAT_CHANNEL_COUNT(AAX_EStemFormat(stem));
}

struct Component final : AAX_IComponentDescriptor {
    mutable std::vector<std::unique_ptr<Properties>> maps;
    Properties *properties = nullptr;
    AAX_CProcessProc process = nullptr;
    int32_t audioIn = -1, audioOut = -1, frames = -1, privateField = -1, privateSize = 0, sideChain = -1;
    // A host without side-chain support refuses it; auxLimit is how many aux
    // output stems it accepts before refusing (0: no aux output support).
    bool refuseSideChain = false;
    size_t auxLimit = SIZE_MAX;
    std::map<int32_t, int32_t> privateFields; // field → size, beyond the owner
    std::map<int32_t, MidiNodeRecord> midi;
    std::vector<AuxOutputStem> aux;
    AAX_Result Clear() override { return AAX_SUCCESS; }
    AAX_Result AddAudioIn(AAX_CFieldIndex p) override { audioIn = p; return AAX_SUCCESS; }
    AAX_Result AddAudioOut(AAX_CFieldIndex p) override { audioOut = p; return AAX_SUCCESS; }
    AAX_Result AddAudioBufferLength(AAX_CFieldIndex p) override { frames = p; return AAX_SUCCESS; }
    AAX_Result AddSampleRate(AAX_CFieldIndex) override { return AAX_ERROR_UNIMPLEMENTED; }
    AAX_Result AddClock(AAX_CFieldIndex) override { return AAX_ERROR_UNIMPLEMENTED; }
    AAX_Result AddSideChainIn(AAX_CFieldIndex p) override {
        if (refuseSideChain) return AAX_ERROR_UNIMPLEMENTED;
        sideChain = p;
        return AAX_SUCCESS;
    }
    AAX_Result AddDataInPort(AAX_CFieldIndex, uint32_t, AAX_EDataInPortType) override { return AAX_ERROR_UNIMPLEMENTED; }
    AAX_Result AddAuxOutputStem(AAX_CFieldIndex, int32_t stem, const char name[]) override {
        if (aux.size() >= auxLimit) return AAX_ERROR_UNIMPLEMENTED;
        aux.push_back({stem, name});
        return AAX_SUCCESS;
    }
    AAX_Result AddPrivateData(AAX_CFieldIndex p, int32_t n, uint32_t) override {
        if (privateField < 0) {
            privateField = p;
            privateSize = n;
        } else {
            privateFields[p] = n;
        }
        return AAX_SUCCESS;
    }
    AAX_Result AddTemporaryData(AAX_CFieldIndex, uint32_t) override { return AAX_ERROR_UNIMPLEMENTED; }
    AAX_Result AddDmaInstance(AAX_CFieldIndex, AAX_IDma::EMode) override { return AAX_ERROR_UNIMPLEMENTED; }
    AAX_Result AddMeters(AAX_CFieldIndex, const AAX_CTypeID *, uint32_t) override { return AAX_ERROR_UNIMPLEMENTED; }
    AAX_Result AddMIDINode(AAX_CFieldIndex p, AAX_EMIDINodeType t, const char n[], uint32_t m) override { midi[p] = {t, n, m}; return AAX_SUCCESS; }
    AAX_Result AddReservedField(AAX_CFieldIndex, uint32_t) override { return AAX_ERROR_UNIMPLEMENTED; }
    AAX_IPropertyMap *NewPropertyMap() const override { maps.emplace_back(new Properties); return maps.back().get(); }
    AAX_IPropertyMap *DuplicatePropertyMap(AAX_IPropertyMap *) const override { return nullptr; }
    AAX_Result AddProcessProc_Native(AAX_CProcessProc p, AAX_IPropertyMap *m, AAX_CInstanceInitProc, AAX_CBackgroundProc, AAX_CSelector *) override {
        process = p;
        properties = static_cast<Properties *>(m);
        return AAX_SUCCESS;
    }
    AAX_Result AddProcessProc_TI(const char[], const char[], AAX_IPropertyMap *, const char[], const char[], AAX_CSelector *) override { return AAX_ERROR_UNIMPLEMENTED; }
    AAX_Result AddProcessProc(AAX_IPropertyMap *, AAX_CSelector *, int32_t) override { return AAX_ERROR_UNIMPLEMENTED; }
    int32_t fieldIndex(AAX_EMIDINodeType type) const {
        for (const auto &node : midi)
            if (node.second.type == type) return node.first;
        return -1;
    }
    int32_t fieldCount() const {
        int32_t last = std::max({privateField, frames, audioIn, audioOut, sideChain});
        for (const auto &node : midi) last = std::max(last, node.first);
        for (const auto &field : privateFields) last = std::max(last, field.first);
        return last + 1;
    }
    uint32_t auxChannels() const {
        uint32_t total = 0;
        for (const auto &stem : aux) total += stemChannels(stem.stem);
        return total;
    }
};

struct Descriptor final : AAX_IEffectDescriptor {
    Component component;
    Properties properties, effect;
    std::string name;
    uint32_t category = 0;
    void *createParameters = nullptr, *createGui = nullptr;
    bool componentAdded = false;
    AAX_IComponentDescriptor *NewComponentDescriptor() override { return &component; }
    AAX_Result AddComponent(AAX_IComponentDescriptor *p) override { componentAdded = p == &component; return componentAdded ? AAX_SUCCESS : AAX_ERROR_INVALID_ARGUMENT; }
    AAX_Result AddName(const char *n) override { name = n; return AAX_SUCCESS; }
    AAX_Result AddCategory(uint32_t c) override { category |= c; return AAX_SUCCESS; }
    AAX_Result AddCategoryBypassParameter(uint32_t, AAX_CParamID) override { return AAX_SUCCESS; }
    AAX_Result AddProcPtr(void *p, AAX_CProcPtrID kind) override {
        if (kind == kAAX_ProcPtrID_Create_EffectParameters) createParameters = p;
        if (kind == kAAX_ProcPtrID_Create_EffectGUI) createGui = p;
        return AAX_SUCCESS;
    }
    AAX_IPropertyMap *NewPropertyMap() override { return &effect; }
    AAX_Result SetProperties(AAX_IPropertyMap *p) override { return p == &effect ? AAX_SUCCESS : AAX_ERROR_INVALID_ARGUMENT; }
    AAX_Result AddResourceInfo(AAX_EResourceType, const char *) override { return AAX_SUCCESS; }
    AAX_Result AddMeterDescription(AAX_CTypeID, const char *, AAX_IPropertyMap *) override { return AAX_SUCCESS; }
    AAX_Result AddControlMIDINode(AAX_CTypeID, AAX_EMIDINodeType, const char[], uint32_t) override { return AAX_SUCCESS; }
    AAX_IEffectParameters *newParameters() const {
        using Create = AAX_IEffectParameters *(AAX_CALLBACK *)();
        return reinterpret_cast<Create>(createParameters)();
    }
};

struct Collection final : AAX_ICollection {
    std::vector<std::unique_ptr<Descriptor>> descriptors;
    bool refuseSideChain = false, refuseAuxOutputs = false;
    // Per-descriptor aux stem limits, in NewDescriptor order (default: no limit).
    std::vector<size_t> auxLimits;
    std::vector<std::pair<std::string, Descriptor *>> effects;
    Properties properties;
    std::string manufacturer, package;
    uint32_t version = 0;
    AAX_IEffectDescriptor *NewDescriptor() override {
        descriptors.emplace_back(new Descriptor);
        descriptors.back()->component.refuseSideChain = refuseSideChain;
        const size_t index = descriptors.size() - 1;
        descriptors.back()->component.auxLimit =
            refuseAuxOutputs ? 0 : index < auxLimits.size() ? auxLimits[index] : SIZE_MAX;
        return descriptors.back().get();
    }
    AAX_Result AddEffect(const char *id, AAX_IEffectDescriptor *p) override { effects.emplace_back(id, static_cast<Descriptor *>(p)); return AAX_SUCCESS; }
    AAX_Result SetManufacturerName(const char *n) override { manufacturer = n; return AAX_SUCCESS; }
    AAX_Result AddPackageName(const char *n) override { if (package.empty()) package = n; return AAX_SUCCESS; }
    AAX_Result SetPackageVersion(uint32_t v) override { version = v; return AAX_SUCCESS; }
    AAX_IPropertyMap *NewPropertyMap() override { return &properties; }
    AAX_Result SetProperties(AAX_IPropertyMap *) override { return AAX_SUCCESS; }
    AAX_Result GetHostVersion(uint32_t *p) const override { if (p) *p = 0; return AAX_SUCCESS; }
    AAX_IDescriptionHost *DescriptionHost() override { return nullptr; }
    const AAX_IDescriptionHost *DescriptionHost() const override { return nullptr; }
    IACFDefinition *HostDefinition() const override { return nullptr; }
};

struct MidiNode final : AAX_IMIDINode {
    std::vector<AAX_CMidiPacket> in, posted;
    AAX_CMidiStream stream{};
    AAX_CMidiStream *GetNodeBuffer() override {
        stream.mBufferSize = uint32_t(in.size());
        stream.mBuffer = in.empty() ? nullptr : in.data();
        return &stream;
    }
    AAX_Result PostMIDIPacket(AAX_CMidiPacket *packet) override { posted.push_back(*packet); return AAX_SUCCESS; }
    AAX_ITransport *GetTransport() override { return nullptr; }
};

// Answers IID_IAAXEffectParametersV1 for the GUI with the given data model and
// forwards everything else to the controller host.
class GuiHost final : public IACFUnknown {
public:
    TransportControllerHost &controller;
    AAX_IEffectParameters *model;
    // The test owns the model; hold a reference so the GUI's Release cannot delete it.
    GuiHost(TransportControllerHost &c, AAX_IEffectParameters *m) : controller(c), model(m) { model->AddRef(); }
    ACFRESULT ACFMETHODCALLTYPE QueryInterface(const acfIID &iid, void **value) override {
        if (!value) return ACF_E_INVALIDARG;
        if (!std::memcmp(&iid, &IID_IAAXEffectParametersV1, sizeof(iid))) {
            *value = model;
            model->AddRef();
            return ACF_OK;
        }
        return controller.QueryInterface(iid, value);
    }
    acfUInt32 ACFMETHODCALLTYPE AddRef() override { return 1; }
    acfUInt32 ACFMETHODCALLTYPE Release() override { return 1; }
};

class ViewContainer final : public AAX_IACFViewContainer {
public:
    int32_t type;
    void *pointer;
    ViewContainer(int32_t t, void *p) : type(t), pointer(p) {}
    ACFRESULT ACFMETHODCALLTYPE QueryInterface(const acfIID &iid, void **value) override {
        if (!value) return ACF_E_INVALIDARG;
        *value = nullptr;
        if (!std::memcmp(&iid, &IID_IAAXViewContainerV1, sizeof(iid))) {
            *value = static_cast<AAX_IACFViewContainer *>(this);
            return ACF_OK;
        }
        return ACF_E_NOINTERFACE;
    }
    acfUInt32 ACFMETHODCALLTYPE AddRef() override { return 1; }
    acfUInt32 ACFMETHODCALLTYPE Release() override { return 1; }
    int32_t GetType() override { return type; }
    void *GetPtr() override { return pointer; }
    AAX_Result GetModifiers(uint32_t *m) override { if (m) *m = 0; return AAX_SUCCESS; }
    AAX_Result SetViewSize(AAX_Point &) override { return AAX_SUCCESS; }
    AAX_Result HandleParameterMouseDown(AAX_CParamID, uint32_t) override { return AAX_ERROR_UNIMPLEMENTED; }
    AAX_Result HandleParameterMouseDrag(AAX_CParamID, uint32_t) override { return AAX_ERROR_UNIMPLEMENTED; }
    AAX_Result HandleParameterMouseUp(AAX_CParamID, uint32_t) override { return AAX_ERROR_UNIMPLEMENTED; }
};

inline AAX_EStemFormat stem(int32_t value) { return AAX_EStemFormat(value); }

// One algorithm instance: context fields laid out as the component registered them.
// Like Pro Tools, the host appends one mono key channel after the main inputs
// when a side-chain field is registered, and the auxiliary output stems' channels
// after the main outputs. `outputs` holds main then auxiliary channels.
struct Context {
    // Host buffers hold every block size the tests run, including the
    // oversized block the adapter must silence: it writes all `frames` samples.
    static constexpr int32_t kCapacity = 2048;
    std::array<void *, 8> fields{};
    void *owner = nullptr;
    int32_t frames = 0;
    // Side-chain index the host reports: the key's input channel, or 0 when no
    // key is assigned (see connectKey).
    int32_t sideChainIndex = 0;
    uint32_t mainInputs = 0;
    std::vector<int32_t> privateStorage = std::vector<int32_t>(8, 0);
    std::vector<std::vector<float>> inputs, outputs;
    std::vector<float *> inputPointers, outputPointers;
    MidiNode midiIn, midiOut;
    const Component &component;
    Context(const Component &c, uint32_t inputChannels, uint32_t outputChannels) : component(c), mainInputs(inputChannels) {
        inputs.assign(inputChannels + (c.sideChain >= 0 ? 1 : 0), std::vector<float>(kCapacity));
        outputs.assign(outputChannels + c.auxChannels(), std::vector<float>(kCapacity));
        for (auto &b : inputs) inputPointers.push_back(b.data());
        for (auto &b : outputs) outputPointers.push_back(b.data());
        fields[size_t(c.privateField)] = &owner;
        fields[size_t(c.frames)] = &frames;
        fields[size_t(c.audioOut)] = outputPointers.data();
        if (c.audioIn >= 0) fields[size_t(c.audioIn)] = inputPointers.data();
        if (c.sideChain >= 0) fields[size_t(c.sideChain)] = &sideChainIndex;
        for (const auto &field : c.privateFields) fields[size_t(field.first)] = &privateStorage[size_t(field.first)];
        const int32_t mi = c.fieldIndex(AAX_eMIDINodeType_LocalInput), mo = c.fieldIndex(AAX_eMIDINodeType_LocalOutput);
        if (mi >= 0) fields[size_t(mi)] = static_cast<AAX_IMIDINode *>(&midiIn);
        if (mo >= 0) fields[size_t(mo)] = static_cast<AAX_IMIDINode *>(&midiOut);
    }
    void connectKey(bool connected) { sideChainIndex = connected ? int32_t(mainInputs) : 0; }
    std::vector<float> &key() { return inputs.at(mainInputs); }
    void run(int32_t count) {
        require(count <= kCapacity, "test block exceeds the fake host's buffers");
        frames = count;
        void *instances[] = {fields.data()};
        component.process(instances, instances + 1);
    }
};

} // namespace fake
