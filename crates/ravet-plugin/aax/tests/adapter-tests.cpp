// Behaviour tests of the generic AAX adapter against the mock table
// (mock/RavetPluginMock.cpp), through the public SDK interfaces with a synthetic
// host. Not Avid's validator and not Pro Tools.
#include "FakeHost.hpp"
#include "RavetAaxPlugin.hpp"
#include "AAX_CString.h"
#include <cmath>
#include <cstring>
#include <iostream>

extern "C" {
void ravet_mock_push_edit(void *, uint32_t, uint32_t, double);
uint32_t ravet_mock_blocks(void *);
uint32_t ravet_mock_last_transport(void *);
double ravet_mock_last_tempo(void *);
void *ravet_mock_editor_parent(void *);
uint32_t ravet_mock_editor_kind(void *);
uint32_t ravet_mock_editor_idle(void *);
}
AAX_Result GetEffectDescriptions(AAX_ICollection *);

using fake::require;
using namespace ravet::aax::plugin;

namespace {
enum : uint32_t { Gain, Mode, Bypass, Peak, Calibration };
const char *gainId = "p47414E00", *modeId = "p4D4F4445", *peakId = "p5045414B";

double normalized(uint32_t index, double physical) {
    double n = 0;
    require(entry()->normalize(index, physical, &n) == 0, "normalize");
    return n;
}

struct Instance {
    TransportControllerHost host;
    std::unique_ptr<AAX_IEffectParameters> model;
    Parameters *parameters = nullptr;
    std::unique_ptr<fake::Context> context;
    Instance(fake::Descriptor &descriptor, const std::string &id, AAX_EStemFormat stem) {
        host.effect = id;
        host.input = host.output = stem;
        model.reset(descriptor.newParameters());
        parameters = dynamic_cast<Parameters *>(model.get());
        require(parameters != nullptr, "factory returns the adapter's data model");
        require(model->Initialize(host.unknown()) == AAX_SUCCESS, "Initialize " + id);
        const uint32_t channels = AAX_STEM_FORMAT_CHANNEL_COUNT(stem);
        context.reset(new fake::Context(descriptor.component, channels, channels));
        require(model->ResetFieldData(0, &context->owner, sizeof(void *)) == AAX_SUCCESS && context->owner == parameters->owner(),
                "ResetFieldData publishes the owner");
    }
    ~Instance() {
        if (model) model->Uninitialize();
    }
    void *instance() const { return parameters->instance(); }
    void fill(float value) {
        for (auto &channel : context->inputs) std::fill(channel.begin(), channel.end(), value);
    }
    void set(const char *id, double value) {
        require(model->UpdateParameterNormalizedValue(id, value, AAX_eUpdateSource_Unspecified) == AAX_SUCCESS, std::string("update ") + id);
    }
};

void descriptors(fake::Collection &collection) {
    require(collection.manufacturer == "RAVET" && collection.package == "RAVET Mock" && collection.version == 0x000100, "package identity");
    require(collection.effects.size() == 2, "one effect per layout");
    require(collection.effects[0].first == "org.ravet.mock.aax.1x1" && collection.effects[1].first == "org.ravet.mock.aax.2x2", "effect IDs");
    for (size_t i = 0; i < 2; ++i) {
        auto &d = *collection.effects[i].second;
        auto &c = d.component;
        const auto &p = *c.properties;
        require(d.name == "RAVET Mock" && d.category == AAX_EPlugInCategory_Effect, "name and category");
        require(d.createGui != nullptr, "editor registered when the product has one");
        require(d.effect.at(AAX_eProperty_RequiresChunkCallsOnMainThread) == 1 && d.effect.at(AAX_eProperty_UsesTransport) == 1, "effect properties");
        require(p.at(AAX_eProperty_ManufacturerID) == 0x52617674 && p.at(AAX_eProperty_ProductID) == 0x526d6f63, "manufacturer/product IDs");
        require(p.at(AAX_eProperty_PlugInID_Native) == (i ? 0x526d5332 : 0x526d4d31), "type ID per layout");
        const auto stem = i ? AAX_eStemFormat_Stereo : AAX_eStemFormat_Mono;
        require(p.at(AAX_eProperty_InputStemFormat) == stem && p.at(AAX_eProperty_OutputStemFormat) == stem, "stems");
        require(p.at(AAX_eProperty_CanBypass) == 1 && p.at(AAX_eProperty_Constraint_MultiMonoSupport) == 0, "bypass and multi-mono");
        require(c.privateField == 0 && c.frames == 1 && c.audioOut == 2 && c.audioIn == 3, "audio fields");
        require(c.fieldIndex(AAX_eMIDINodeType_LocalInput) == 4 && c.fieldIndex(AAX_eMIDINodeType_LocalOutput) == 5, "MIDI fields");
    }
}

void parameters(Instance &mono) {
    auto &host = mono.host;
    require(host.registered.size() == 4 && host.registered.count(cDefaultMasterBypassID) && host.registered.count(gainId) &&
                host.registered.count(modeId) && host.registered.count(peakId),
            "visible parameters registered; hidden one omitted");
    AAX_CString bypass, text;
    require(mono.model->GetMasterBypassParameter(&bypass) == AAX_SUCCESS && std::string(bypass.Get()) == cDefaultMasterBypassID, "master bypass");
    int32_t steps = 0;
    AAX_EParameterType type{};
    require(mono.model->GetParameterNumberOfSteps(modeId, &steps) == AAX_SUCCESS && steps == 3, "stepped mode has 3 values");
    require(mono.model->GetParameterType(modeId, &type) == AAX_SUCCESS && type == AAX_eParameterType_Discrete, "stepped mode is discrete");
    require(mono.model->GetParameterNumberOfSteps(cDefaultMasterBypassID, &steps) == AAX_SUCCESS && steps == 2, "bypass has 2 values");
    AAX_CBoolean automatable = true;
    require(mono.model->GetParameterIsAutomatable(peakId, &automatable) == AAX_SUCCESS && !automatable, "read-only is not automatable");
    require(mono.model->GetParameterStringFromValue(modeId, 0.5, &text, 32) == AAX_SUCCESS && std::string(text.Get()) == "Warm", "value_to_text");
    require(mono.model->GetParameterStringFromValue(gainId, normalized(Gain, -6), &text, 32) == AAX_SUCCESS && std::string(text.Get()) == "-6.00 dB", "gain text");
    require(mono.model->GetParameterStringFromValue(gainId, normalized(Gain, -6), &text, 3) == AAX_SUCCESS && std::string(text.Get()) == "-6.", "text truncated to host length");
    double value = 0;
    require(mono.model->GetParameterValueFromString(modeId, &value, AAX_CString("Hot")) == AAX_SUCCESS && value == 1.0, "text_to_value");
    double defaultValue = -1;
    require(mono.model->GetParameterDefaultNormalizedValue(gainId, &defaultValue) == AAX_SUCCESS && std::fabs(defaultValue - normalized(Gain, 0)) < 1e-9, "default");
    std::cout << "parameters: ok\n";
}

void processing(Instance &mono) {
    auto *table = entry();
    mono.set(gainId, normalized(Gain, -6));
    require(std::fabs(table->get_value(mono.instance(), Gain) + 6.) < 1e-9, "host change reaches the table cache at once");
    mono.fill(1.f);
    mono.context->run(256);
    require(std::fabs(mono.context->outputs[0][0] - 0.501187f) < 1e-4 && std::fabs(mono.context->outputs[0][255] - 0.501187f) < 1e-4, "gain applied");
    require(ravet_mock_blocks(mono.instance()) == 1, "one table block");
    require(ravet_mock_last_transport(mono.instance()) & RAVET_TRANSPORT_TEMPO, "transport delivered");
    require(ravet_mock_last_tempo(mono.instance()) == 120., "tempo from AAX transport");
    mono.set(cDefaultMasterBypassID, 1.);
    mono.context->run(64);
    require(mono.context->outputs[0][0] == 1.f, "host bypass maps to the bypass parameter");
    mono.set(cDefaultMasterBypassID, 0.);
    // Oversized and malformed blocks are silenced without calling the table.
    mono.context->outputs[0][0] = 9.f;
    mono.context->run(2048);
    require(mono.context->outputs[0][0] == 0.f && ravet_mock_blocks(mono.instance()) == 2, "frames above 1024 are silenced");
    mono.context->run(0);
    require(ravet_mock_blocks(mono.instance()) == 3, "empty block reaches the table");
    std::cout << "processing: ok\n";
}

void midi(Instance &mono) {
    auto &in = mono.context->midiIn.in;
    in.clear();
    AAX_CMidiPacket on{};
    on.mTimestamp = 3; on.mLength = 3; on.mData[0] = 0x91; on.mData[1] = 60; on.mData[2] = 100;
    AAX_CMidiPacket bend{};
    bend.mTimestamp = 10; bend.mLength = 3; bend.mData[0] = 0xe0; bend.mData[1] = 0; bend.mData[2] = 64;
    in = {on, bend};
    mono.context->midiOut.posted.clear();
    mono.context->run(128);
    auto &out = mono.context->midiOut.posted;
    require(out.size() == 2 && out[0].mTimestamp == 3 && out[0].mData[0] == 0x91 && out[0].mData[2] == 100 && out[1].mData[0] == 0xe0,
            "MIDI in reaches the table and its MIDI out reaches the output node");
    // A SysEx packet makes the strict preflight reject the stream; the
    // tolerant fallback keeps the short messages.
    AAX_CMidiPacket sysex{};
    sysex.mTimestamp = 5; sysex.mLength = 1; sysex.mData[0] = 0xf0;
    in = {on, sysex, bend};
    out.clear();
    mono.context->run(128);
    require(out.size() == 2 && mono.parameters->owner()->midiRejected.load() == 1, "SysEx skipped, short messages kept");
    in.clear();
    std::cout << "midi: ok\n";
}

void latency(Instance &mono) {
    auto &host = mono.host;
    const uint32_t initial = host.latencyUpdates;
    require(initial == 1, "latency reported after prepare");
    mono.set(modeId, 1.); // mode 2 → 128 samples in the mock
    mono.context->run(64);   // the processor's latency changes
    mono.model->TimerWakeup(); // MAIN sees latency_changed, asks AUDIO to read it
    require(host.latencyUpdates == initial, "not reported before AUDIO reads timing");
    mono.context->run(64);   // AUDIO reads timing() between blocks
    mono.model->TimerWakeup(); // MAIN reports it
    require(host.latencyUpdates == initial + 1, "latency change reported to the host");
    require(mono.parameters->owner()->latency.load() == 128, "new latency value");
    mono.model->TimerWakeup();
    require(host.latencyUpdates == initial + 1, "reported once");
    mono.set(modeId, 0.);
    std::cout << "latency: ok\n";
}

void edits(Instance &mono) {
    auto &host = mono.host;
    const uint32_t touches = host.touches, releases = host.releases, requests = host.requests;
    ravet_mock_push_edit(mono.instance(), RAVET_PLUGIN_EDIT_BEGIN, Gain, 0);
    ravet_mock_push_edit(mono.instance(), RAVET_PLUGIN_EDIT_VALUE, Gain, -12);
    ravet_mock_push_edit(mono.instance(), RAVET_PLUGIN_EDIT_END, Gain, 0);
    ravet_mock_push_edit(mono.instance(), RAVET_PLUGIN_EDIT_VALUE, Calibration, 0.5); // hidden: not reported
    mono.model->TimerWakeup();
    require(host.touches > touches && host.releases > releases && host.requests == requests + 1, "editor gesture reported as touch/set/release");
    std::cout << "editor edits: ok\n";
}

void state(Instance &mono) {
    auto *table = entry();
    mono.set(gainId, normalized(Gain, -9));
    mono.set(modeId, 0.5);
    uint32_t size = 0;
    require(mono.model->GetChunkSize(kStateChunkId, &size) == AAX_SUCCESS, "chunk size");
    std::vector<uint8_t> storage(sizeof(AAX_SPlugInChunk) + size);
    auto *chunk = reinterpret_cast<AAX_SPlugInChunk *>(storage.data());
    chunk->fChunkID = kStateChunkId;
    require(mono.model->GetChunk(kStateChunkId, chunk) == AAX_SUCCESS && uint32_t(chunk->fSize) == size, "chunk");
    std::vector<uint8_t> direct(size);
    uint32_t length = 0;
    require(table->save_state(mono.instance(), direct.data(), size, &length) == 0 && length == size &&
                std::memcmp(direct.data(), chunk->fData, size) == 0,
            "chunk carries save_state bytes unchanged");
    mono.set(gainId, normalized(Gain, 0));
    mono.set(modeId, 0.);
    AAX_CBoolean equal = true;
    require(mono.model->CompareActiveChunk(chunk, &equal) == AAX_SUCCESS && !equal, "compare sees the change");
    const uint32_t requests = mono.host.requests;
    require(mono.model->SetChunk(kStateChunkId, chunk) == AAX_SUCCESS, "SetChunk");
    require(std::fabs(table->get_value(mono.instance(), Gain) + 9) < 1e-9 && table->get_value(mono.instance(), Mode) == 1., "load_state applied");
    require(mono.host.requests > requests, "host parameter objects follow the loaded state");
    require(mono.model->SetChunk(0x41424344, chunk) == AAX_ERROR_INVALID_CHUNK_ID, "foreign chunk ID rejected");
    std::cout << "state: ok\n";
}

void readOnly(Instance &mono) {
    mono.set(gainId, normalized(Gain, 0));
    mono.fill(0.5f);
    mono.context->run(64);
    const uint32_t requests = mono.host.requests;
    mono.model->TimerWakeup();
    require(mono.host.requests == requests + 1, "read-only value reflected to the host");
    std::cout << "read-only reflection: ok\n";
}

void editor(Instance &mono, fake::Descriptor &descriptor) {
    using Create = AAX_IEffectGUI *(AAX_CALLBACK *)();
    std::unique_ptr<AAX_IEffectGUI> gui(reinterpret_cast<Create>(descriptor.createGui)());
    fake::GuiHost guiHost(mono.host, mono.model.get());
    require(gui->Initialize(&guiHost) == AAX_SUCCESS, "GUI Initialize");
    AAX_Point size(0, 0);
    require(gui->GetViewSize(&size) == AAX_SUCCESS && size.horz == 480 && size.vert == 320, "view size from the product");
    int parentStorage = 0;
#if defined(_WIN32)
    fake::ViewContainer view(AAX_eViewContainer_Type_HWND, &parentStorage);
    const uint32_t kind = RAVET_PLUGIN_VIEW_WIN32;
#else
    fake::ViewContainer view(AAX_eViewContainer_Type_NSView, &parentStorage);
    const uint32_t kind = RAVET_PLUGIN_VIEW_COCOA;
#endif
    require(gui->SetViewContainer(&view) == AAX_SUCCESS, "SetViewContainer");
    require(ravet_mock_editor_parent(mono.instance()) == &parentStorage && ravet_mock_editor_kind(mono.instance()) == kind, "editor_open with the host view");
    gui->TimerWakeup();
    gui->TimerWakeup();
    require(ravet_mock_editor_idle(mono.instance()) == 2, "editor_idle from the GUI timer");
    require(gui->SetViewContainer(nullptr) == AAX_SUCCESS && ravet_mock_editor_parent(mono.instance()) == nullptr, "editor_close");
    require(gui->Uninitialize() == AAX_SUCCESS, "GUI Uninitialize");
    std::cout << "editor: ok\n";
}

void stereo(fake::Descriptor &descriptor) {
    Instance stereo(descriptor, "org.ravet.mock.aax.2x2", AAX_eStemFormat_Stereo);
    stereo.fill(0.25f);
    stereo.context->run(512);
    require(stereo.context->outputs[0][511] == 0.25f && stereo.context->outputs[1][511] == 0.25f, "stereo unity");
    TransportControllerHost host;
    host.input = AAX_eStemFormat_Stereo;
    host.output = AAX_eStemFormat_Mono;
    std::unique_ptr<AAX_IEffectParameters> bad(descriptor.newParameters());
    require(bad->Initialize(host.unknown()) != AAX_SUCCESS, "undeclared layout rejected");
    bad->Uninitialize();
    std::cout << "stereo and layout rejection: ok\n";
}
} // namespace

int main() {
    try {
        fake::Collection collection;
        require(GetEffectDescriptions(&collection) == AAX_SUCCESS, "GetEffectDescriptions");
        descriptors(collection);
        std::cout << "descriptors: ok\n";
        {
            Instance mono(*collection.effects[0].second, collection.effects[0].first, AAX_eStemFormat_Mono);
            parameters(mono);
            processing(mono);
            midi(mono);
            latency(mono);
            edits(mono);
            state(mono);
            readOnly(mono);
            editor(mono, *collection.effects[0].second);
        }
        stereo(*collection.effects[1].second);
        std::cout << "PASS\n";
        return 0;
    } catch (const std::exception &error) {
        std::cerr << "FAIL: " << error.what() << "\n";
        return 1;
    }
}
