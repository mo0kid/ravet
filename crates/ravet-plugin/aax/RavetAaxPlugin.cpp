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

namespace ravet::aax::plugin {
namespace {

std::atomic<const RavetPluginEntryV1 *> overrideTable{nullptr};

bool valid(const RavetPluginEntryV1 *table) {
    return table && table->version == RAVET_PLUGIN_VERSION_1 &&
           table->byte_size >= sizeof(RavetPluginEntryV1) && table->product &&
           table->product->layout_count && table->product->layouts;
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

FieldLayout fieldLayout(const RavetPluginEntryV1 &table, const RavetPluginLayoutV1 &layout) {
    FieldLayout fields{};
    int32_t next = 3; // 0 owner, 1 frames, 2 outputs
    if (layout.inputs) fields.inputs = next++;
    if (table.product->note_input) fields.midiInput = next++;
    if (table.product->note_output) fields.midiOutput = next++;
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
    const RavetPluginLayoutV1 *layout = nullptr;
    for (uint32_t i = 0; i < product.layout_count && !layout; ++i)
        if (product.layouts[i].inputs == channels(input) && product.layouts[i].outputs == channels(output))
            layout = &product.layouts[i];
    if (!layout) return AAX_ERROR_INVALID_ARGUMENT;
    owner_->fields = fieldLayout(*table_, *layout);
    owner_->inputs = layout->inputs;
    owner_->outputs = layout->outputs;
    owner_->transport = Transport();

    RavetNativeProcessorConfigV1 config{};
    config.version = 1;
    config.maximum_frames = kMaximumFrames;
    config.maximum_events = kMaximumEvents;
    config.maximum_output_events = product.note_output ? kMaximumEvents : 0;
    config.bus_count = 1;
    config.sample_rate = double(rate);
    config.buses[0] = {layout->inputs, layout->outputs};
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

AAX_Result Parameters::Uninitialize() {
    if (owner_) owner_->ready.store(false, std::memory_order_release);
    if (prepared_ && owner_->instance) {
        table_->deactivate(owner_->instance);
        prepared_ = false;
    }
    return AAX_CEffectParameters::Uninitialize();
}

AAX_Result Parameters::UpdateParameterNormalizedValue(AAX_CParamID id, double value, AAX_EUpdateSource source) {
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
    if (!saveState(chunk_)) return AAX_ERROR_INVALID_ARGUMENT;
    *size = uint32_t(chunk_.size());
    return AAX_SUCCESS;
}

AAX_Result Parameters::GetChunk(AAX_CTypeID id, AAX_SPlugInChunk *chunk) const {
    if (!chunk) return AAX_ERROR_NULL_ARGUMENT;
    if (id != kStateChunkId) return AAX_ERROR_INVALID_CHUNK_ID;
    std::vector<uint8_t> data;
    if (!chunk_.empty()) data.swap(chunk_);
    else if (!saveState(data)) return AAX_ERROR_INVALID_ARGUMENT;
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
    reflectValues();
    ++mNumPlugInChanges;
    return AAX_SUCCESS;
}

AAX_Result Parameters::CompareActiveChunk(const AAX_SPlugInChunk *chunk, AAX_CBoolean *equal) const {
    if (!chunk || !equal) return AAX_ERROR_NULL_ARGUMENT;
    if (chunk->fChunkID != kStateChunkId) {
        *equal = true; // not ours: nothing to compare
        return AAX_SUCCESS;
    }
    std::vector<uint8_t> current;
    if (!saveState(current)) return AAX_ERROR_INVALID_ARGUMENT;
    *equal = chunk->fSize >= 0 && size_t(chunk->fSize) == current.size() &&
             std::memcmp(chunk->fData, current.data(), current.size()) == 0;
    return AAX_SUCCESS;
}

// ---------------------------------------------------------------- processing

void AAX_CALLBACK Parameters::process(void *const instances[], const void *end) {
    for (void *const *cursor = instances; cursor < static_cast<void *const *>(end); ++cursor) {
        void *const *fields = static_cast<void *const *>(*cursor);
        if (!fields || !fields[0]) continue;
        Owner *owner = *static_cast<Owner *const *>(fields[0]);
        if (!owner) continue;
        const int32_t frames = fields[1] ? *static_cast<const int32_t *>(fields[1]) : 0;
        float *const *outputs = static_cast<float *const *>(fields[2]);
        if (!owner->ready.load(std::memory_order_acquire) || frames < 0 || uint32_t(frames) > kMaximumFrames) {
            silence(outputs, owner->outputs, frames);
            continue;
        }
        const RavetPluginEntryV1 *table = owner->table;
        const FieldLayout &layout = owner->fields;

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
        RavetNativeProcessorBusV1 bus{owner->inputs, owner->outputs, inputs, outputs};
        RavetNativeProcessorResultV1 result{};
        const uint32_t capacity = layout.midiOutput >= 0 ? kMaximumEvents : 0;
        const int32_t status = table->process(owner->instance, &bus, 1, uint32_t(frames),
                                              count ? owner->input.data() : nullptr, count, transport,
                                              capacity ? owner->output.data() : nullptr, capacity, &result);
        owner->lastStatus.store(status, std::memory_order_relaxed);
        if (status != 0 || result.status == 2) {
            silence(outputs, owner->outputs, frames);
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
    if (Parameters *parameters = model()) {
        if (open_ && parameters->instance()) parameters->table()->editor_idle(parameters->instance());
        parameters->reportEdits();
    }
    return AAX_SUCCESS;
}

// ---------------------------------------------------------------- descriptors

AAX_Result describe(AAX_ICollection &collection, const RavetPluginEntryV1 &table) {
    const RavetPluginProductV1 &product = *table.product;
    AAX_Result status = collection.SetManufacturerName(product.vendor);
    if (status == AAX_SUCCESS) status = collection.AddPackageName(product.name);
    if (status == AAX_SUCCESS) status = collection.SetPackageVersion(packedVersion(product.version));
    if (status != AAX_SUCCESS) return status;

    uint32_t registered = 0;
    for (uint32_t i = 0; i < product.layout_count; ++i) {
        const RavetPluginLayoutV1 &layout = product.layouts[i];
        AAX_EStemFormat input, output;
        // AAX has no stem for other channel counts without channel roles; skip them.
        if (!stemFormat(layout.inputs, input) || !stemFormat(layout.outputs, output) || !layout.outputs) continue;
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
        if (product.note_input || product.note_output) {
            status = properties->AddProperty(AAX_eProperty_Constraint_MultiMonoSupport, 0);
            if (status != AAX_SUCCESS) return status;
        }
        status = component->AddProcessProc_Native(reinterpret_cast<AAX_CProcessProc>(&Parameters::process), properties);
        if (status == AAX_SUCCESS) status = descriptor->AddComponent(component);
        if (status != AAX_SUCCESS) return status;

        char suffix[32];
        std::snprintf(suffix, sizeof(suffix), ".aax.%ux%u", layout.inputs, layout.outputs);
        const std::string effectId = std::string(product.plugin_id) + suffix;
        status = collection.AddEffect(effectId.c_str(), descriptor);
        if (status != AAX_SUCCESS) return status;
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
