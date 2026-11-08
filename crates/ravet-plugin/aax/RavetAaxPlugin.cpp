// Generic AAX adapter over the RAVET plug-in table. See RavetAaxPlugin.hpp.
#include "RavetAaxPlugin.hpp"
#include "RavetAaxShortMidiInput.hpp"
#include "RavetAaxTransport.hpp"
#include "RavetNativeMidiOutput.hpp"
#include "AAX_CParameter.h"
#include "AAX_CString.h"
#include "AAX_ICollection.h"
#include "AAX_IComponentDescriptor.h"
#include "AAX_IController.h"
#include "AAX_IEffectDescriptor.h"
#include "AAX_IMIDINode.h"
#include "AAX_IPropertyMap.h"
#include "AAX_IViewContainer.h"
#include "AAX_ITaperDelegate.h"
#include "AAX_IDisplayDelegate.h"
#include "AAX_Properties.h"
#include <algorithm>
#include <climits>
#include <cstdio>
#include <cstring>
#include <map>
#include <mutex>
#include <set>
#include <tuple>

namespace ravet::aax::plugin {
namespace {

std::atomic<const RavetPluginEntryV1 *> overrideTable{nullptr};
// Output stems the host accepted per described effect, keyed by
// (manufacturer, product, AAX type ID); see acceptedOutputStems(). MAIN only
// (describe and EffectInit), guarded anyway since hosts differ.
using StemKey = std::tuple<uint32_t, uint32_t, uint32_t>;
std::mutex stemLock;
std::map<StemKey, uint32_t> &stemRecords() {
    static std::map<StemKey, uint32_t> records;
    return records;
}
StemKey stemKey(const RavetPluginProductV1 &product, const RavetPluginLayoutV1 &layout) {
    return StemKey{product.aax_manufacturer, product.aax_product, layout.aax_type_id};
}
void recordOutputStems(const RavetPluginProductV1 &product, const RavetPluginLayoutV1 &layout, uint32_t count) {
    std::lock_guard<std::mutex> guard(stemLock);
    stemRecords()[stemKey(product, layout)] = count;
}
// Stem names handed to the host stay alive for the library's lifetime.
const char *keepName(const std::string &name) {
    static std::mutex lock;
    static std::set<std::string> names;
    std::lock_guard<std::mutex> guard(lock);
    return names.insert(name).first->c_str();
}

// The AAX Library forwards host calls to these methods without catching, so
// every method that allocates (parameter objects, ID strings, state buffers,
// text) runs through this guard: no C++ exception may unwind into the host.
// The algorithm callback does not allocate and is not wrapped.
template <class R, class F> R guarded(R failure, F &&body) noexcept {
    try {
        return body();
    } catch (...) {
        return failure;
    }
}

// The auxiliary bus records as RavetPlugin.h defines them: at most 8; each
// named (nonblank, at most 64 bytes, unique) with 0..64 inputs and outputs,
// not both 0.
bool validAuxiliaries(const RavetPluginProductV1 &product) {
    if (product.auxiliary_count > kMaximumAuxiliaries || (product.auxiliary_count && !product.auxiliaries)) return false;
    for (uint32_t i = 0; i < product.auxiliary_count; ++i) {
        const RavetPluginAuxiliaryBusV1 &bus = product.auxiliaries[i];
        if (!bus.name || (!bus.inputs && !bus.outputs) || bus.inputs > kMaximumAuxiliaryChannels ||
            bus.outputs > kMaximumAuxiliaryChannels)
            return false;
        size_t length = 0;
        bool blank = true;
        for (; length <= 64 && bus.name[length]; ++length)
            blank = blank && (bus.name[length] == ' ' || (bus.name[length] >= '\t' && bus.name[length] <= '\r'));
        if (length == 0 || length > 64 || blank) return false;
        for (uint32_t j = 0; j < i; ++j)
            if (!std::strcmp(product.auxiliaries[j].name, bus.name)) return false;
    }
    return true;
}

bool valid(const RavetPluginEntryV1 *table) {
    return table && table->version == RAVET_PLUGIN_VERSION_1 &&
           table->byte_size >= sizeof(RavetPluginEntryV1) && table->product &&
           table->product->layout_count && table->product->layouts && validAuxiliaries(*table->product);
}

// Physical ↔ normalized mapping through the table (scale, steps, clamping).
class Taper final : public AAX_ITaperDelegate<double> {
    const RavetPluginEntryV1 *table_;
    uint32_t index_;
    double minimum_, maximum_;
public:
    Taper(const RavetPluginEntryV1 *table, uint32_t index, double minimum, double maximum)
        : table_(table), index_(index), minimum_(minimum), maximum_(maximum) {}
    Taper *Clone() const AAX_OVERRIDE { return new Taper(*this); }
    double GetMaximumValue() const AAX_OVERRIDE { return maximum_; }
    double GetMinimumValue() const AAX_OVERRIDE { return minimum_; }
    double ConstrainRealValue(double value) const AAX_OVERRIDE { return NormalizedToReal(RealToNormalized(value)); }
    double NormalizedToReal(double normalized) const AAX_OVERRIDE {
        double physical = minimum_;
        return table_->denormalize(index_, normalized, &physical) == 0 ? physical : minimum_;
    }
    double RealToNormalized(double physical) const AAX_OVERRIDE {
        double normalized = 0.;
        return table_->normalize(index_, physical, &normalized) == 0 ? normalized : 0.;
    }
};

// Text through the product's own formatting and unit.
class Display final : public AAX_IDisplayDelegate<double> {
    const RavetPluginEntryV1 *table_;
    uint32_t index_;
public:
    Display(const RavetPluginEntryV1 *table, uint32_t index) : table_(table), index_(index) {}
    Display *Clone() const AAX_OVERRIDE { return new Display(*this); }
    bool ValueToString(double value, AAX_CString *text) const AAX_OVERRIDE { return ValueToString(value, 0, text); }
    bool ValueToString(double value, int32_t maximum, AAX_CString *text) const AAX_OVERRIDE {
        return guarded(false, [&] { return format(value, maximum, text); });
    }
    bool format(double value, int32_t maximum, AAX_CString *text) const {
        if (!text) return false;
        char buffer[256] = {};
        if (table_->value_to_text(index_, value, buffer, sizeof(buffer)) != 0) return false;
        std::string out(buffer);
        if (maximum > 0 && out.size() > size_t(maximum)) {
            size_t cut = size_t(maximum);
            while (cut > 0 && (uint8_t(out[cut]) & 0xc0) == 0x80) --cut; // keep UTF-8 whole
            out.resize(cut);
        }
        text->Set(out.c_str());
        return true;
    }
    bool StringToValue(const AAX_CString &text, double *value) const AAX_OVERRIDE {
        return value && table_->text_to_value(index_, text.Get(), value) == 0;
    }
};

uint32_t packedVersion(const char *version) {
    unsigned major = 0, minor = 0, patch = 0;
    if (!version || std::sscanf(version, "%u.%u.%u", &major, &minor, &patch) < 1) return 0;
    return (std::min(major, 0xffffu) << 16) | (std::min(minor, 0xffu) << 8) | std::min(patch, 0xffu);
}

uint32_t category(uint32_t value) {
    switch (value) {
    case RAVET_PLUGIN_INSTRUMENT: return AAX_ePlugInCategory_SWGenerators;
    case RAVET_PLUGIN_MIDI_EFFECT: return AAX_EPlugInCategory_MIDIEffect;
    default: return AAX_EPlugInCategory_Effect;
    }
}

uint32_t channels(AAX_EStemFormat format) {
    return format == AAX_eStemFormat_None ? 0 : AAX_STEM_FORMAT_CHANNEL_COUNT(format);
}

void silence(float *const *outputs, uint32_t count, int32_t frames) {
    if (!outputs || frames <= 0) return;
    for (uint32_t c = 0; c < count; ++c)
        if (outputs[c]) std::memset(outputs[c], 0, sizeof(float) * size_t(frames));
}

// Tolerant fallback when the strict short-MIDI preflight rejects a stream (for
// example a SysEx packet): keeps the complete short messages, in order.
uint32_t tolerantMidi(const AAX_CMidiStream &stream, uint32_t frames, RavetNativeEventV1 *out) {
    uint32_t count = 0, previous = 0;
    for (uint32_t i = 0; i < stream.mBufferSize && i < kMaximumEvents && stream.mBuffer; ++i) {
        const AAX_CMidiPacket &packet = stream.mBuffer[i];
        const uint32_t offset = frames ? std::min<uint32_t>(packet.mTimestamp, frames - 1) : 0;
        const uint8_t status = packet.mData[0];
        uint32_t length = 0;
        if (status >= 0x80 && status < 0xf0) length = (status & 0xe0) == 0xc0 ? 2 : 3;
        if (!length || packet.mLength != length || offset < previous) continue;
        bool data = true;
        for (uint32_t b = 1; b < length; ++b) data = data && packet.mData[b] < 0x80;
        if (!data) continue;
        RavetNativeEventV1 event{};
        event.offset = offset;
        event.kind = RAVET_NATIVE_MIDI;
        event.detail = length;
        for (uint32_t b = 0; b < length; ++b) event.words[0] |= uint32_t(packet.mData[b]) << (8 * b);
        out[count++] = event;
        previous = offset;
    }
    return count;
}

} // namespace

const RavetPluginEntryV1 *entry() {
    if (auto *table = overrideTable.load(std::memory_order_acquire)) return table;
    static const RavetPluginEntryV1 *const table = ravet_plugin_v1();
    return valid(table) ? table : nullptr;
}

void setEntryForTesting(const RavetPluginEntryV1 *table) { overrideTable.store(table, std::memory_order_release); }

std::string parameterId(const RavetPluginEntryV1 &table, uint32_t index) {
    if (table.product->bypass_index >= 0 && uint32_t(table.product->bypass_index) == index) return cDefaultMasterBypassID;
    RavetPluginParameterV1 parameter{};
    if (table.parameter(index, &parameter) != 0) return {};
    char text[16];
    std::snprintf(text, sizeof(text), "p%X", parameter.id);
    return text;
}

bool stemFormat(uint32_t count, AAX_EStemFormat &out) {
    switch (count) {
    case 0: out = AAX_eStemFormat_None; return true;
    case 1: out = AAX_eStemFormat_Mono; return true;
    case 2: out = AAX_eStemFormat_Stereo; return true;
    default: return false;
    }
}

uint32_t acceptedOutputStems(const RavetPluginProductV1 &product, const RavetPluginLayoutV1 &layout) {
    std::lock_guard<std::mutex> guard(stemLock);
    const auto found = stemRecords().find(stemKey(product, layout));
    return found == stemRecords().end() ? 0 : found->second;
}

std::string effectId(const RavetPluginProductV1 &product, const RavetPluginLayoutV1 &layout) {
    char suffix[32];
    std::snprintf(suffix, sizeof(suffix), ".aax.%ux%u", layout.inputs, layout.outputs);
    return std::string(product.plugin_id) + suffix;
}

std::string stemName(const char *bus, AAX_EStemFormat stem) {
    std::string name = bus ? bus : "";
    std::string lower = name;
    for (char &c : lower)
        if (c >= 'A' && c <= 'Z') c = char(c - 'A' + 'a');
    const char *suffix = stem == AAX_eStemFormat_Mono ? " Mono" : stem == AAX_eStemFormat_Stereo ? " Stereo" : "";
    if (lower.find("mono") != std::string::npos || lower.find("stereo") != std::string::npos) suffix = "";
    const size_t room = kMaximumStemName - std::strlen(suffix);
    if (name.size() > room) {
        size_t cut = room;
        while (cut > 0 && (uint8_t(name[cut]) & 0xc0) == 0x80) --cut; // keep UTF-8 whole
        name.resize(cut);
    }
    return name + suffix;
}

BusPlan busPlan(const RavetPluginEntryV1 &table, const RavetPluginLayoutV1 &layout, uint32_t outputStems) {
    const RavetPluginProductV1 &product = *table.product;
    BusPlan plan{};
    plan.count = std::min(product.auxiliary_count, kMaximumAuxiliaries);
    plan.hostOutputs = layout.outputs;
    plan.hostInputs = layout.inputs;
    uint32_t stems = 0;
    for (uint32_t i = 0; i < plan.count; ++i) {
        const RavetPluginAuxiliaryBusV1 &bus = product.auxiliaries[i];
        AuxiliaryPlan &route = plan.buses[i];
        route.inputs = bus.inputs;
        route.outputs = bus.outputs;
        if (bus.inputs && !bus.outputs && !plan.sideChain) {
            route.route = AuxiliaryRoute::SideChain;
            plan.sideChain = true;
        } else if (!bus.inputs && bus.outputs && stems < outputStems && stemFormat(bus.outputs, route.stem)) {
            // Pro Tools supports only mono and stereo auxiliary output stems.
            route.route = AuxiliaryRoute::Output;
            route.hostChannel = plan.hostOutputs;
            plan.hostOutputs += bus.outputs;
            ++stems;
        } else {
            route.stem = AAX_eStemFormat_None;
        }
    }
    // AAX reports the key as an index into the main input array; give layouts
    // without a main input one, so a connected key's index is never 0.
    if (plan.sideChain && !layout.inputs) plan.hostInputs = layout.outputs;
    return plan;
}

FieldLayout fieldLayout(const RavetPluginEntryV1 &table, const RavetPluginLayoutV1 &layout) {
    FieldLayout fields{};
    int32_t next = 3; // 0 owner, 1 frames, 2 outputs (main, then auxiliary stems)
    const BusPlan plan = busPlan(table, layout);
    if (plan.hostInputs) fields.inputs = next++;
    if (table.product->note_input) fields.midiInput = next++;
    if (table.product->note_output) fields.midiOutput = next++;
    if (plan.sideChain) fields.sideChain = next++;
    fields.count = uint32_t(next);
    return fields;
}

// ---------------------------------------------------------------- data model

Parameters::Parameters(const RavetPluginEntryV1 *table) : table_(table), owner_(new Owner) {
    owner_->table = table;
    owner_->instance = table ? table->create() : nullptr;
}

Parameters::~Parameters() {
    if (owner_ && owner_->instance) {
        owner_->ready.store(false, std::memory_order_release);
        if (prepared_) table_->deactivate(owner_->instance);
        table_->destroy(owner_->instance);
        owner_->instance = nullptr;
    }
}

AAX_IEffectParameters *AAX_CALLBACK Parameters::create() {
    try {
        return new Parameters(entry());
    } catch (...) {
        return nullptr;
    }
}

AAX_Result Parameters::EffectInit() {
    return guarded<AAX_Result>(AAX_ERROR_UNKNOWN_EXCEPTION, [&] { return initialize(); });
}

AAX_Result Parameters::initialize() {
    if (!table_ || !owner_->instance) return AAX_ERROR_NOT_INITIALIZED;
    const RavetPluginProductV1 &product = *table_->product;

    // Parameters, in table order. Hidden parameters stay out of the host's view
    // (they still travel in the state envelope); the bypass role never hides.
    ids_.assign(product.parameter_count, std::string());
    readOnly_.assign(product.parameter_count, 0.);
    for (uint32_t index = 0; index < product.parameter_count; ++index) {
        RavetPluginParameterV1 info{};
        if (table_->parameter(index, &info) != 0) return AAX_ERROR_INVALID_PARAMETER_INDEX;
        const bool bypass = product.bypass_index >= 0 && uint32_t(product.bypass_index) == index;
        if ((info.flags & RAVET_PLUGIN_PARAMETER_HIDDEN) && !bypass) continue;
        const std::string id = parameterId(*table_, index);
        const bool readOnly = (info.flags & RAVET_PLUGIN_PARAMETER_READ_ONLY) != 0;
        const bool automatable = (info.flags & RAVET_PLUGIN_PARAMETER_AUTOMATABLE) && !readOnly;
        auto *parameter = new AAX_CParameter<double>(id.c_str(), AAX_CString(info.name ? info.name : id.c_str()),
                                                     info.default_value, Taper(table_, index, info.minimum, info.maximum),
                                                     Display(table_, index), automatable);
        if (bypass || info.steps) {
            parameter->SetNumberOfSteps(bypass && !info.steps ? 2 : info.steps + 1);
            parameter->SetType(AAX_eParameterType_Discrete);
        }
        mParameterManager.AddParameter(parameter);
        // A state loaded before EffectInit may already differ from the default.
        double normalized = 0.;
        const double current = table_->get_value(owner_->instance, index);
        if (current != info.default_value && table_->normalize(index, current, &normalized) == 0)
            parameter->UpdateNormalizedValue(normalized);
        ids_[index] = id;
        indices_[id] = index;
        readOnly_[index] = table_->get_value(owner_->instance, index);
    }

    // Layout from the host's stems; prepare the processor for it.
    AAX_CSampleRate rate = 0;
    AAX_EStemFormat input = AAX_eStemFormat_None, output = AAX_eStemFormat_None;
    AAX_IController *controller = Controller();
    if (!controller) return AAX_ERROR_NOT_INITIALIZED;
    AAX_Result status = controller->GetSampleRate(&rate);
    if (status == AAX_SUCCESS) status = controller->GetInputStemFormat(&input);
    if (status == AAX_SUCCESS) status = controller->GetOutputStemFormat(&output);
    if (status != AAX_SUCCESS) return status;
    if (!(rate > 0.)) return AAX_ERROR_INVALID_ARGUMENT;
    // The layout whose declared stems the host reports. A side-chain gives a
    // 0-input layout a main input stem, which can make two layouts' stems
    // equal (0x2 and 2x2): the host's effect ID tells them apart, then the
    // first match in table order.
    const auto declares = [&](const RavetPluginLayoutV1 &candidate) {
        return busPlan(*table_, candidate).hostInputs == channels(input) && candidate.outputs == channels(output);
    };
    const RavetPluginLayoutV1 *layout = nullptr;
    AAX_CString effect;
    if (controller->GetEffectID(&effect) == AAX_SUCCESS)
        for (uint32_t i = 0; i < product.layout_count && !layout; ++i)
            if (effectId(product, product.layouts[i]) == effect.Get() && declares(product.layouts[i]))
                layout = &product.layouts[i];
    for (uint32_t i = 0; i < product.layout_count && !layout; ++i)
        if (declares(product.layouts[i])) layout = &product.layouts[i];
    if (!layout) return AAX_ERROR_INVALID_ARGUMENT;
    owner_->fields = fieldLayout(*table_, *layout);
    owner_->inputs = layout->inputs;
    owner_->outputs = layout->outputs;
    owner_->transport = Transport();
    prepareBuses(*layout);

    RavetNativeProcessorConfigV1 config{};
    config.version = 1;
    config.maximum_frames = kMaximumFrames;
    config.maximum_events = kMaximumEvents;
    config.maximum_output_events = product.note_output ? kMaximumEvents : 0;
    config.bus_count = 1 + owner_->plan.count;
    config.sample_rate = double(rate);
    config.buses[0] = {layout->inputs, layout->outputs};
    for (uint32_t i = 0; i < owner_->plan.count; ++i)
        config.buses[1 + i] = {owner_->plan.buses[i].inputs, owner_->plan.buses[i].outputs};
    if (table_->prepare(owner_->instance, &config) != 0) return AAX_ERROR_INVALID_ARGUMENT;
    prepared_ = true;

    RavetNativeProcessorTimingV1 timing{};
    if (table_->timing(owner_->instance, &timing) == 0) {
        owner_->latency.store(timing.latency_samples, std::memory_order_relaxed);
        reportedLatency_ = int32_t(std::min<uint32_t>(timing.latency_samples, INT32_MAX));
        controller->SetSignalLatency(reportedLatency_);
    }
    owner_->ready.store(true, std::memory_order_release);
    return AAX_SUCCESS;
}

void Parameters::prepareBuses(const RavetPluginLayoutV1 &layout) {
    Owner &owner = *owner_;
    owner.plan = busPlan(*table_, layout, acceptedOutputStems(*table_->product, layout));
    uint32_t inputs = 0, outputs = 0;
    for (uint32_t i = 0; i < owner.plan.count; ++i) {
        inputs += owner.plan.buses[i].inputs;
        outputs += owner.plan.buses[i].outputs;
    }
    owner.silent.assign(kMaximumFrames, 0.f);
    owner.scratch.assign(size_t(outputs) * kMaximumFrames, 0.f);
    // Silent inputs and scratch outputs until the callback routes host buffers.
    owner.auxiliaryInputs.assign(inputs, owner.silent.data());
    owner.auxiliaryOutputs.assign(outputs, nullptr);
    for (uint32_t c = 0; c < outputs; ++c) owner.auxiliaryOutputs[c] = owner.scratch.data() + size_t(c) * kMaximumFrames;
    uint32_t input = 0, output = 0;
    for (uint32_t i = 0; i < owner.plan.count; ++i) {
        const AuxiliaryPlan &route = owner.plan.buses[i];
        owner.buses[1 + i] = {route.inputs, route.outputs,
                              route.inputs ? static_cast<const void *>(owner.auxiliaryInputs.data() + input) : nullptr,
                              route.outputs ? static_cast<const void *>(owner.auxiliaryOutputs.data() + output) : nullptr};
        input += route.inputs;
        output += route.outputs;
    }
}

AAX_Result Parameters::Uninitialize() {
    if (owner_) owner_->ready.store(false, std::memory_order_release);
    if (prepared_ && owner_->instance) {
        table_->deactivate(owner_->instance);
        prepared_ = false;
    }
    return AAX_CEffectParameters::Uninitialize();
}

AAX_Result Parameters::UpdateParameterNormalizedValue(AAX_CParamID id, double value, AAX_EUpdateSource source) {
    // The lookup builds a std::string from the host's ID.
    return guarded<AAX_Result>(AAX_ERROR_UNKNOWN_EXCEPTION, [&] { return update(id, value, source); });
}

AAX_Result Parameters::update(AAX_CParamID id, double value, AAX_EUpdateSource source) {
    const AAX_Result status = AAX_CEffectParameters::UpdateParameterNormalizedValue(id, value, source);
    if (status != AAX_SUCCESS || !id) return status;
    const auto found = indices_.find(id);
    if (found == indices_.end() || !owner_->instance) return status;
    double physical = 0.;
    if (table_->denormalize(found->second, value, &physical) == 0)
        table_->set_value(owner_->instance, found->second, physical);
    return status;
}

AAX_Result Parameters::ResetFieldData(AAX_CFieldIndex field, void *data, uint32_t size) const {
    if (field == 0) {
        if (!data || size != sizeof(Owner *)) return AAX_ERROR_INVALID_ARGUMENT;
        Owner *owner = owner_.get();
        std::memcpy(data, &owner, sizeof(owner));
        return AAX_SUCCESS;
    }
    return AAX_CEffectParameters::ResetFieldData(field, data, size);
}

void Parameters::reportEdits() {
    if (!owner_->instance) return;
    std::array<RavetPluginEditV1, 64> edits{};
    for (;;) {
        const uint32_t count = table_->take_edits(owner_->instance, edits.data(), uint32_t(edits.size()));
        for (uint32_t i = 0; i < count; ++i) {
            const RavetPluginEditV1 &edit = edits[i];
            if (edit.index >= ids_.size() || ids_[edit.index].empty()) continue;
            const char *id = ids_[edit.index].c_str();
            if (edit.kind == RAVET_PLUGIN_EDIT_BEGIN) {
                TouchParameter(id);
            } else if (edit.kind == RAVET_PLUGIN_EDIT_END) {
                ReleaseParameter(id);
            } else if (edit.kind == RAVET_PLUGIN_EDIT_VALUE) {
                double normalized = 0.;
                if (table_->normalize(edit.index, edit.value, &normalized) == 0)
                    SetParameterNormalizedValue(id, normalized);
            }
        }
        if (count < edits.size()) break;
    }
}

void Parameters::reportLatency() {
    if (!prepared_) return;
    if (table_->latency_changed(owner_->instance))
        owner_->timingRequested.store(true, std::memory_order_release);
    if (owner_->timingPublished.exchange(false, std::memory_order_acq_rel)) {
        const int32_t latency = int32_t(std::min<uint32_t>(owner_->latency.load(std::memory_order_acquire), INT32_MAX));
        if (latency != reportedLatency_ && Controller() && Controller()->SetSignalLatency(latency) == AAX_SUCCESS)
            reportedLatency_ = latency;
    }
}

void Parameters::reflectValues() {
    // Bring the host's parameter objects in line with the table's cache, e.g.
    // after a state load. The host echoes each through UpdateParameterNormalizedValue.
    for (uint32_t index = 0; index < ids_.size(); ++index) {
        if (ids_[index].empty()) continue;
        AAX_IParameter *parameter = mParameterManager.GetParameterByID(ids_[index].c_str());
        double normalized = 0.;
        const double value = table_->get_value(owner_->instance, index);
        readOnly_[index] = value;
        if (parameter && table_->normalize(index, value, &normalized) == 0 &&
            normalized != parameter->GetNormalizedValue())
            parameter->SetNormalizedValue(normalized);
    }
}

AAX_Result Parameters::TimerWakeup() {
    return guarded<AAX_Result>(AAX_ERROR_UNKNOWN_EXCEPTION, [&] { return wake(); });
}

AAX_Result Parameters::wake() {
    reportEdits();
    reportLatency();
    // Read-only parameters (meters and similar) follow the table.
    for (uint32_t index = 0; index < ids_.size(); ++index) {
        if (ids_[index].empty()) continue;
        RavetPluginParameterV1 info{};
        if (table_->parameter(index, &info) != 0 || !(info.flags & RAVET_PLUGIN_PARAMETER_READ_ONLY)) continue;
        const double value = table_->get_value(owner_->instance, index);
        if (value == readOnly_[index]) continue;
        readOnly_[index] = value;
        double normalized = 0.;
        if (AAX_IParameter *parameter = mParameterManager.GetParameterByID(ids_[index].c_str()))
            if (table_->normalize(index, value, &normalized) == 0) parameter->SetNormalizedValue(normalized);
    }
    return AAX_CEffectParameters::TimerWakeup();
}

AAX_Result Parameters::NotificationReceived(AAX_CTypeID type, const void *data, uint32_t size) {
    if (type == AAX_eNotificationEvent_SignalLatencyChanged && Controller()) {
        int32_t latency = 0;
        if (Controller()->GetSignalLatency(&latency) == AAX_SUCCESS) reportedLatency_ = latency;
    }
    return AAX_CEffectParameters::NotificationReceived(type, data, size);
}

AAX_Result Parameters::GetMasterBypassParameter(AAX_IString *id) const {
    if (!id) return AAX_ERROR_NULL_ARGUMENT;
    id->Set(table_ && table_->product->bypass_index >= 0 ? cDefaultMasterBypassID : "");
    return AAX_SUCCESS;
}

// ---------------------------------------------------------------- state

bool Parameters::saveState(std::vector<uint8_t> &out) const {
    if (!owner_->instance) return false;
    uint32_t length = 0;
    if (table_->save_state(owner_->instance, nullptr, 0, &length) != 0 || length > uint32_t(INT32_MAX)) return false;
    out.resize(length);
    if (table_->save_state(owner_->instance, out.data(), length, &length) != 0 || length > out.size()) return false;
    out.resize(length);
    return true;
}

AAX_Result Parameters::GetNumberOfChunks(int32_t *count) const {
    if (!count) return AAX_ERROR_NULL_ARGUMENT;
    *count = 1;
    return AAX_SUCCESS;
}

AAX_Result Parameters::GetChunkIDFromIndex(int32_t index, AAX_CTypeID *id) const {
    if (!id) return AAX_ERROR_NULL_ARGUMENT;
    if (index != 0) return AAX_ERROR_INVALID_CHUNK_INDEX;
    *id = kStateChunkId;
    return AAX_SUCCESS;
}

AAX_Result Parameters::GetChunkSize(AAX_CTypeID id, uint32_t *size) const {
    if (!size) return AAX_ERROR_NULL_ARGUMENT;
    if (id != kStateChunkId) return AAX_ERROR_INVALID_CHUNK_ID;
    // The host allocates this size, then calls GetChunk: keep the bytes so both agree.
    return guarded<AAX_Result>(AAX_ERROR_UNKNOWN_EXCEPTION, [&] {
        if (!saveState(chunk_)) return AAX_ERROR_INVALID_ARGUMENT;
        *size = uint32_t(chunk_.size());
        chunkCapacity_ = *size;
        return AAX_SUCCESS;
    });
}

AAX_Result Parameters::GetChunk(AAX_CTypeID id, AAX_SPlugInChunk *chunk) const {
    if (!chunk) return AAX_ERROR_NULL_ARGUMENT;
    if (id != kStateChunkId) return AAX_ERROR_INVALID_CHUNK_ID;
    return guarded<AAX_Result>(AAX_ERROR_UNKNOWN_EXCEPTION, [&] { return writeChunk(chunk); });
}

AAX_Result Parameters::writeChunk(AAX_SPlugInChunk *chunk) const {
    std::vector<uint8_t> data;
    if (!chunk_.empty()) data.swap(chunk_);
    else if (!saveState(data)) return AAX_ERROR_INVALID_ARGUMENT;
    // fData was allocated by the host from the last GetChunkSize. Without a
    // cached chunk (GetChunk repeated, or never sized), never write more
    // than that: the envelope length is fixed per product today, but a
    // future product extra state may change it between the two calls.
    if (data.size() > chunkCapacity_) return AAX_ERROR_INCORRECT_CHUNK_SIZE;
    chunk->fSize = int32_t(data.size());
    chunk->fVersion = 1;
    std::memcpy(chunk->fData, data.data(), data.size());
    return AAX_SUCCESS;
}

AAX_Result Parameters::SetChunk(AAX_CTypeID id, const AAX_SPlugInChunk *chunk) {
    if (!chunk) return AAX_ERROR_NULL_ARGUMENT;
    if (id != kStateChunkId) return AAX_ERROR_INVALID_CHUNK_ID;
    if (chunk->fSize < 0 || !owner_->instance) return AAX_ERROR_INCORRECT_CHUNK_SIZE;
    if (table_->load_state(owner_->instance, reinterpret_cast<const uint8_t *>(chunk->fData), uint32_t(chunk->fSize)) != 0)
        return AAX_ERROR_INCORRECT_CHUNK_SIZE;
    chunk_.clear();
    // The state is applied; reflecting it calls into the AAX Library.
    guarded(0, [&] { reflectValues(); return 0; });
    ++mNumPlugInChanges;
    return AAX_SUCCESS;
}

AAX_Result Parameters::CompareActiveChunk(const AAX_SPlugInChunk *chunk, AAX_CBoolean *equal) const {
    if (!chunk || !equal) return AAX_ERROR_NULL_ARGUMENT;
    if (chunk->fChunkID != kStateChunkId) {
        *equal = true; // not ours: nothing to compare
        return AAX_SUCCESS;
    }
    return guarded<AAX_Result>(AAX_ERROR_UNKNOWN_EXCEPTION, [&] {
        std::vector<uint8_t> current;
        if (!saveState(current)) return AAX_ERROR_INVALID_ARGUMENT;
        *equal = chunk->fSize >= 0 && size_t(chunk->fSize) == current.size() &&
                 std::memcmp(chunk->fData, current.data(), current.size()) == 0;
        return AAX_SUCCESS;
    });
}

// ---------------------------------------------------------------- processing

// AUDIO. No allocation or exception path: fixed Owner arrays, and the field
// pointers are those registered for this layout in describe(). Output silence
// uses the host's frame count, which is also the length of its buffers.
void AAX_CALLBACK Parameters::process(void *const instances[], const void *end) {
    for (void *const *cursor = instances; cursor < static_cast<void *const *>(end); ++cursor) {
        void *const *fields = static_cast<void *const *>(*cursor);
        if (!fields || !fields[0]) continue;
        Owner *owner = *static_cast<Owner *const *>(fields[0]);
        if (!owner) continue;
        const int32_t frames = fields[1] ? *static_cast<const int32_t *>(fields[1]) : 0;
        float *const *outputs = static_cast<float *const *>(fields[2]);
        if (!owner->ready.load(std::memory_order_acquire) || frames < 0 || uint32_t(frames) > kMaximumFrames) {
            silence(outputs, owner->plan.hostOutputs, frames);
            continue;
        }
        const RavetPluginEntryV1 *table = owner->table;
        const FieldLayout &layout = owner->fields;
        const BusPlan &plan = owner->plan;

        // Between blocks: answer a latency query from MAIN.
        if (owner->timingRequested.exchange(false, std::memory_order_acq_rel)) {
            RavetNativeProcessorTimingV1 timing{};
            if (table->timing(owner->instance, &timing) == 0) {
                owner->latency.store(timing.latency_samples, std::memory_order_release);
                owner->timingPublished.store(true, std::memory_order_release);
            }
        }

        uint32_t count = 0;
        if (layout.midiInput >= 0) {
            auto *node = static_cast<AAX_IMIDINode *>(fields[layout.midiInput]);
            if (const AAX_CMidiStream *stream = node ? node->GetNodeBuffer() : nullptr) {
                if (!AaxShortMidiInput::convert(stream, uint32_t(frames), owner->input.data(), kMaximumEvents, count)) {
                    owner->midiRejected.fetch_add(1, std::memory_order_relaxed);
                    count = tolerantMidi(*stream, uint32_t(frames), owner->input.data());
                }
            }
        }

        AaxTransportSnapshot snapshot;
        const RavetNativeTransportV1 *transport = nullptr;
        if (owner->transport && aaxTransport(owner->transport, snapshot)) transport = &snapshot.transport();

        float *const *inputs = layout.inputs >= 0 ? static_cast<float *const *>(fields[layout.inputs]) : nullptr;
        // A 0-input layout's declared main input (side-chain products) is not
        // passed on: the processor's main bus keeps 0 inputs.
        owner->buses[0] = {owner->inputs, owner->outputs, owner->inputs ? inputs : nullptr, outputs};
        if (plan.count) {
            // The key is the host input channel right after the declared main
            // inputs (AddSideChainIn reports its index). Any other index,
            // including 0 when no key is assigned, means no key: silence.
            const float *key = owner->silent.data();
            if (layout.sideChain >= 0 && inputs && fields[layout.sideChain]) {
                const int32_t index = *static_cast<const int32_t *>(fields[layout.sideChain]);
                if (index > 0 && uint32_t(index) == plan.hostInputs && inputs[index]) key = inputs[index];
            }
            uint32_t input = 0, output = 0;
            for (uint32_t i = 0; i < plan.count; ++i) {
                const AuxiliaryPlan &route = plan.buses[i];
                if (route.route == AuxiliaryRoute::SideChain)
                    for (uint32_t c = 0; c < route.inputs; ++c) owner->auxiliaryInputs[input + c] = key;
                if (route.route == AuxiliaryRoute::Output)
                    for (uint32_t c = 0; c < route.outputs; ++c) {
                        float *host = outputs ? outputs[route.hostChannel + c] : nullptr;
                        owner->auxiliaryOutputs[output + c] =
                            host ? host : owner->scratch.data() + size_t(output + c) * kMaximumFrames;
                    }
                input += route.inputs;
                output += route.outputs;
            }
        }
        RavetNativeProcessorResultV1 result{};
        const uint32_t capacity = layout.midiOutput >= 0 ? kMaximumEvents : 0;
        const int32_t status = table->process(owner->instance, owner->buses.data(), 1 + plan.count, uint32_t(frames),
                                              count ? owner->input.data() : nullptr, count, transport,
                                              capacity ? owner->output.data() : nullptr, capacity, &result);
        owner->lastStatus.store(status, std::memory_order_relaxed);
        if (status != 0 || result.status == 2) {
            silence(outputs, plan.hostOutputs, frames);
            continue;
        }
        if (layout.midiOutput >= 0 && result.event_count) {
            auto *node = static_cast<AAX_IMIDINode *>(fields[layout.midiOutput]);
            for (uint32_t i = 0; node && i < result.event_count && i < capacity; ++i) {
                native::NativeMidiMessage message{};
                if (!native::nativeMidiOutput(owner->output[i], uint32_t(frames), message)) {
                    owner->midiDropped.fetch_add(1, std::memory_order_relaxed);
                    continue;
                }
                AAX_CMidiPacket packet{};
                packet.mTimestamp = message.offset;
                packet.mLength = message.size;
                for (uint32_t b = 0; b < message.size; ++b) packet.mData[b] = message.bytes[b];
                packet.mIsImmediate = false;
                if (node->PostMIDIPacket(&packet) != AAX_SUCCESS)
                    owner->midiDropped.fetch_add(1, std::memory_order_relaxed);
            }
        }
    }
}

// ---------------------------------------------------------------- editor

AAX_IEffectGUI *AAX_CALLBACK Gui::create() {
    try {
        return new Gui;
    } catch (...) {
        return nullptr;
    }
}

Parameters *Gui::model() const { return dynamic_cast<Parameters *>(const_cast<Gui *>(this)->GetEffectParameters()); }

AAX_Result Gui::GetViewSize(AAX_Point *size) const {
    if (!size) return AAX_ERROR_NULL_ARGUMENT;
    const RavetPluginEntryV1 *table = entry();
    if (!table) return AAX_ERROR_NOT_INITIALIZED;
    size->horz = float(table->product->editor_width);
    size->vert = float(table->product->editor_height);
    return AAX_SUCCESS;
}

void Gui::CreateViewContainer() {
    AAX_IViewContainer *view = GetViewContainer();
    Parameters *parameters = model();
    if (!view || !parameters || !parameters->instance() || open_) return;
    const RavetPluginEntryV1 *table = parameters->table();
    uint32_t kind = 0;
    switch (view->GetType()) {
    case AAX_eViewContainer_Type_NSView: kind = RAVET_PLUGIN_VIEW_COCOA; break;
    case AAX_eViewContainer_Type_HWND: kind = RAVET_PLUGIN_VIEW_WIN32; break;
    default: return;
    }
    void *parent = view->GetPtr();
    if (!parent || !table->product->editor_width) return;
    open_ = table->editor_open(parameters->instance(), kind, parent, table->product->editor_width,
                               table->product->editor_height) == 0;
}

void Gui::DeleteViewContainer() {
    if (!open_) return;
    if (Parameters *parameters = model())
        if (parameters->instance()) parameters->table()->editor_close(parameters->instance());
    open_ = false;
}

AAX_Result Gui::TimerWakeup() {
    // reportEdits calls back into the AAX Library's parameter manager.
    return guarded<AAX_Result>(AAX_ERROR_UNKNOWN_EXCEPTION, [&] {
        if (Parameters *parameters = model()) {
            if (open_ && parameters->instance()) parameters->table()->editor_idle(parameters->instance());
            parameters->reportEdits();
        }
        return AAX_SUCCESS;
    });
}

// ---------------------------------------------------------------- descriptors

AAX_Result describe(AAX_ICollection &collection, const RavetPluginEntryV1 &table) {
    const RavetPluginProductV1 &product = *table.product;
    AAX_Result status = collection.SetManufacturerName(product.vendor);
    if (status == AAX_SUCCESS) status = collection.AddPackageName(product.name);
    if (status == AAX_SUCCESS) status = collection.SetPackageVersion(packedVersion(product.version));
    if (status != AAX_SUCCESS) return status;
    if (!validAuxiliaries(product)) return AAX_ERROR_INVALID_ARGUMENT;

    uint32_t registered = 0;
    for (uint32_t i = 0; i < product.layout_count; ++i) {
        const RavetPluginLayoutV1 &layout = product.layouts[i];
        AAX_EStemFormat input, output;
        // AAX has no stem for other channel counts without channel roles; skip them.
        if (!stemFormat(layout.inputs, input) || !stemFormat(layout.outputs, output) || !layout.outputs) continue;
        const BusPlan plan = busPlan(table, layout);
        stemFormat(plan.hostInputs, input); // the declared main input for side-chain products
        AAX_IEffectDescriptor *descriptor = collection.NewDescriptor();
        if (!descriptor) return AAX_ERROR_NULL_OBJECT;
        status = descriptor->AddName(product.name);
        if (status == AAX_SUCCESS) status = descriptor->AddCategory(category(product.category));
        if (status == AAX_SUCCESS)
            status = descriptor->AddProcPtr(reinterpret_cast<void *>(&Parameters::create), kAAX_ProcPtrID_Create_EffectParameters);
        if (status == AAX_SUCCESS && product.editor_width && product.editor_height)
            status = descriptor->AddProcPtr(reinterpret_cast<void *>(&Gui::create), kAAX_ProcPtrID_Create_EffectGUI);
        if (status != AAX_SUCCESS) return status;
        if (AAX_IPropertyMap *effect = descriptor->NewPropertyMap()) {
            // save_state/load_state are MAIN-thread calls in RavetPlugin.h.
            effect->AddProperty(AAX_eProperty_RequiresChunkCallsOnMainThread, 1);
            effect->AddProperty(AAX_eProperty_UsesTransport, 1);
            status = descriptor->SetProperties(effect);
            if (status != AAX_SUCCESS) return status;
        }

        AAX_IComponentDescriptor *component = descriptor->NewComponentDescriptor();
        if (!component) return AAX_ERROR_NULL_OBJECT;
        const FieldLayout fields = fieldLayout(table, layout);
        status = component->AddPrivateData(0, sizeof(Owner *));
        if (status == AAX_SUCCESS) status = component->AddAudioBufferLength(1);
        if (status == AAX_SUCCESS) status = component->AddAudioOut(2);
        if (status == AAX_SUCCESS && fields.inputs >= 0) status = component->AddAudioIn(fields.inputs);
        if (status == AAX_SUCCESS && fields.midiInput >= 0)
            status = component->AddMIDINode(fields.midiInput, AAX_eMIDINodeType_LocalInput, "MIDI In", 0xffff);
        if (status == AAX_SUCCESS && fields.midiOutput >= 0)
            status = component->AddMIDINode(fields.midiOutput, AAX_eMIDINodeType_LocalOutput, "MIDI Out", 0xffff);
        if (status != AAX_SUCCESS) return status;
        // The side-chain index field. A host that refuses it gets a zeroed
        // private int32 in its place instead (fields may not have gaps); index 0
        // reads as "no key", so the bus stays silent there.
        bool sideChain = false;
        if (fields.sideChain >= 0) {
            sideChain = component->AddSideChainIn(fields.sideChain) == AAX_SUCCESS;
            if (!sideChain) status = component->AddPrivateData(fields.sideChain, sizeof(int32_t));
            if (status != AAX_SUCCESS) return status;
        }
        // A host without auxiliary output stems refuses AddAuxOutputStem and
        // will not provide their buffers. Stop at the first refusal (later
        // stems would shift channels) and record what this effect accepted;
        // instances route only those (the rest become Unrouted).
        uint32_t stems = 0;
        for (uint32_t b = 0; b < plan.count; ++b) {
            const AuxiliaryPlan &route = plan.buses[b];
            if (route.route != AuxiliaryRoute::Output) continue;
            // The field index argument is deprecated; stems follow the main outputs.
            const char *name = keepName(stemName(product.auxiliaries[b].name, route.stem));
            if (component->AddAuxOutputStem(2, route.stem, name) != AAX_SUCCESS) break;
            ++stems;
        }
        AAX_IPropertyMap *properties = component->NewPropertyMap();
        if (!properties) return AAX_ERROR_NULL_OBJECT;
        const std::pair<AAX_EProperty, AAX_CPropertyValue> records[] = {
            {AAX_eProperty_ManufacturerID, AAX_CPropertyValue(product.aax_manufacturer)},
            {AAX_eProperty_ProductID, AAX_CPropertyValue(product.aax_product)},
            {AAX_eProperty_PlugInID_Native, AAX_CPropertyValue(layout.aax_type_id)},
            {AAX_eProperty_InputStemFormat, AAX_CPropertyValue(input)},
            {AAX_eProperty_OutputStemFormat, AAX_CPropertyValue(output)},
            {AAX_eProperty_CanBypass, product.bypass_index >= 0 ? 1 : 0},
            {AAX_eProperty_LatencyContribution, 0},
        };
        for (const auto &record : records) {
            status = properties->AddProperty(record.first, record.second);
            if (status != AAX_SUCCESS) return status;
        }
        if (sideChain) {
            status = properties->AddProperty(AAX_eProperty_SupportsSideChainInput, 1);
            if (status != AAX_SUCCESS) return status;
        }
        // Auxiliary output stems belong to one instance's channel set, not to
        // each mono instance of a multi-mono insert.
        if (product.note_input || product.note_output || stems) {
            status = properties->AddProperty(AAX_eProperty_Constraint_MultiMonoSupport, 0);
            if (status != AAX_SUCCESS) return status;
        }
        status = component->AddProcessProc_Native(reinterpret_cast<AAX_CProcessProc>(&Parameters::process), properties);
        if (status == AAX_SUCCESS) status = descriptor->AddComponent(component);
        if (status != AAX_SUCCESS) return status;

        status = collection.AddEffect(effectId(product, layout).c_str(), descriptor);
        if (status != AAX_SUCCESS) return status;
        recordOutputStems(product, layout, stems);
        ++registered;
    }
    return registered ? AAX_SUCCESS : AAX_ERROR_INVALID_ARGUMENT;
}

} // namespace ravet::aax::plugin

// Called by the AAX Library's ACFRegisterPlugin.
#if defined(_WIN32)
__declspec(dllexport)
#else
__attribute__((visibility("default")))
#endif
AAX_Result GetEffectDescriptions(AAX_ICollection *collection) {
    if (!collection) return AAX_ERROR_NULL_ARGUMENT;
    const RavetPluginEntryV1 *table = ravet::aax::plugin::entry();
    if (!table) return AAX_ERROR_NULL_OBJECT;
    try {
        return ravet::aax::plugin::describe(*collection, *table);
    } catch (...) {
        return AAX_ERROR_UNKNOWN_EXCEPTION;
    }
}
