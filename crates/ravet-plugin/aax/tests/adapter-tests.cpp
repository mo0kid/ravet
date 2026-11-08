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
const RavetPluginEntryV1 *ravet_mock_auxiliary_v1(void);
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
    Instance(fake::Descriptor &descriptor, const std::string &id, AAX_EStemFormat stem) : Instance(descriptor, id, stem, stem) {}
    Instance(fake::Descriptor &descriptor, const std::string &id, AAX_EStemFormat input, AAX_EStemFormat output) {
        host.effect = id;
        host.input = input;
        host.output = output;
        model.reset(descriptor.newParameters());
        parameters = dynamic_cast<Parameters *>(model.get());
        require(parameters != nullptr, "factory returns the adapter's data model");
        require(model->Initialize(host.unknown()) == AAX_SUCCESS, "Initialize " + id);
        context.reset(new fake::Context(descriptor.component, fake::stemChannels(input), fake::stemChannels(output)));
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
        // Products without auxiliary buses keep the layout they always had.
        require(c.sideChain < 0 && c.aux.empty() && c.privateFields.empty() && c.fieldCount() == 6, "no auxiliary fields");
        require(!p.values.count(AAX_eProperty_SupportsSideChainInput), "no side-chain property");
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
    // A repeated GetChunk (no cached bytes) still fits the size the host allocated.
    require(mono.model->GetChunk(kStateChunkId, chunk) == AAX_SUCCESS && uint32_t(chunk->fSize) == size, "repeated chunk");
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
    {
        // GetChunk without a preceding GetChunkSize: the host buffer size is
        // unknown, so nothing is written.
        std::vector<uint8_t> storage(sizeof(AAX_SPlugInChunk) + 4096, 0xee);
        auto *chunk = reinterpret_cast<AAX_SPlugInChunk *>(storage.data());
        chunk->fChunkID = kStateChunkId;
        chunk->fSize = 0;
        require(stereo.model->GetChunk(kStateChunkId, chunk) == AAX_ERROR_INCORRECT_CHUNK_SIZE && chunk->fSize == 0 &&
                    uint8_t(chunk->fData[0]) == 0xee, "unsized GetChunk refused");
        uint32_t size = 0;
        require(stereo.model->GetChunkSize(kStateChunkId, &size) == AAX_SUCCESS && size <= 4096, "chunk size after refusal");
        require(stereo.model->GetChunk(kStateChunkId, chunk) == AAX_SUCCESS && uint32_t(chunk->fSize) == size, "sized GetChunk");
    }
    TransportControllerHost host;
    host.input = AAX_eStemFormat_Stereo;
    host.output = AAX_eStemFormat_Mono;
    std::unique_ptr<AAX_IEffectParameters> bad(descriptor.newParameters());
    require(bad->Initialize(host.unknown()) != AAX_SUCCESS, "undeclared layout rejected");
    bad->Uninitialize();
    std::cout << "stereo and layout rejection: ok\n";
}
// RAVET Mock Aux (see the mock): Key → side-chain, Monitor and Direct → aux
// output stems, the rest unrouted.
struct TableCopy {
    RavetPluginProductV1 product;
    RavetPluginEntryV1 table;
    explicit TableCopy(const RavetPluginEntryV1 &source) : product(*source.product), table(source) { table.product = &product; }
};

void stemNames() {
    require(stemName("Monitor", AAX_eStemFormat_Stereo) == "Monitor Stereo" && stemName("Direct", AAX_eStemFormat_Mono) == "Direct Mono",
            "stem format appended");
    require(stemName("Stereo Bus", AAX_eStemFormat_Stereo) == "Stereo Bus" && stemName("FX MONO", AAX_eStemFormat_Mono) == "FX MONO" &&
                stemName("left (mono)", AAX_eStemFormat_Stereo) == "left (mono)",
            "names that already say mono/stereo are kept (case-insensitive)");
    const std::string longest(64, 'a');
    const std::string cut = stemName(longest.c_str(), AAX_eStemFormat_Stereo);
    require(cut.size() == kMaximumStemName && cut == std::string(57, 'a') + " Stereo", "long names cut to fit the suffix");
    std::string accents;
    for (int i = 0; i < 32; ++i) accents += "\xc3\xa9"; // 32 x U+00E9, 64 bytes
    const std::string utf8 = stemName(accents.c_str(), AAX_eStemFormat_Stereo);
    require(utf8.size() == 63 && utf8.substr(0, 56) == accents.substr(0, 56) && utf8.substr(56) == " Stereo", "cut at a UTF-8 boundary");
    std::cout << "stem names: ok\n";
}

void auxiliary() {
    const RavetPluginEntryV1 &aux = *ravet_mock_auxiliary_v1();
    setEntryForTesting(&aux);
    fake::Collection collection;
    require(GetEffectDescriptions(&collection) == AAX_SUCCESS, "describe the auxiliary product");
    require(collection.effects.size() == 2 && collection.effects[0].first == "org.ravet.mock.aux.aax.2x2" &&
                collection.effects[1].first == "org.ravet.mock.aux.aax.0x2",
            "effect IDs keep the RAVET layout counts");
    auto &stereo = *collection.effects[0].second, &instrument = *collection.effects[1].second;
    for (auto *d : {&stereo, &instrument}) {
        auto &c = d->component;
        const auto &p = *c.properties;
        require(c.aux.size() == 2 && c.aux[0].stem == AAX_eStemFormat_Stereo && c.aux[0].name == "Monitor Stereo" &&
                    c.aux[1].stem == AAX_eStemFormat_Mono && c.aux[1].name == "Direct Mono",
                "output-only mono/stereo buses become named aux output stems, in order");
        require(p.at(AAX_eProperty_Constraint_MultiMonoSupport) == 0, "aux stems disable multi-mono");
        // Both layouts declare a stereo main input: 0x2 gets one for the key.
        require(p.at(AAX_eProperty_InputStemFormat) == AAX_eStemFormat_Stereo && p.at(AAX_eProperty_OutputStemFormat) == AAX_eStemFormat_Stereo,
                "declared stems");
        require(c.audioOut == 2 && c.audioIn == 3 && c.sideChain == 4 && c.fieldCount() == 5, "side-chain field after the inputs");
        require(p.at(AAX_eProperty_SupportsSideChainInput) == 1, "side-chain property");
    }
    require(stereo.component.properties->at(AAX_eProperty_PlugInID_Native) == 0x52615332 &&
                instrument.component.properties->at(AAX_eProperty_PlugInID_Native) == 0x52614932,
            "type IDs unchanged by the declared input");
    {
        Instance fx(stereo, "org.ravet.mock.aux.aax.2x2", AAX_eStemFormat_Stereo);
        auto &context = *fx.context;
        const auto &plan = fx.parameters->owner()->plan;
        require(fx.parameters->owner()->inputs == 2, "effect ID selects 2x2");
        require(plan.count == 6 && plan.sideChain && plan.hostInputs == 2 && plan.hostOutputs == 5, "plan");
        require(plan.buses[0].route == AuxiliaryRoute::SideChain && plan.buses[1].route == AuxiliaryRoute::Output &&
                    plan.buses[2].route == AuxiliaryRoute::Unrouted && plan.buses[3].route == AuxiliaryRoute::Unrouted &&
                    plan.buses[4].route == AuxiliaryRoute::Unrouted && plan.buses[5].route == AuxiliaryRoute::Output &&
                    plan.buses[1].hostChannel == 2 && plan.buses[5].hostChannel == 4,
                "routes");
        require(context.inputs.size() == 3 && context.outputs.size() == 5, "host appends key and stem channels");
        for (uint32_t c = 0; c < 2; ++c) std::fill(context.inputs[c].begin(), context.inputs[c].end(), 0.25f);
        std::fill(context.key().begin(), context.key().end(), 0.75f);
        context.connectKey(true);
        context.run(128);
        require(context.outputs[0][127] == 0.25f && context.outputs[1][127] == 0.25f, "main bus unchanged");
        require(context.outputs[2][0] == 0.75f && context.outputs[3][127] == 0.75f, "mono key feeds every side-chain channel");
        require(context.outputs[4][0] == 0.5f && context.outputs[4][127] == 0.5f,
                "unrouted input buses are silent; mono aux stem written");
        // No key assigned (index 0) or an index that is not the key's: silence.
        context.connectKey(false);
        context.run(128);
        require(context.outputs[2][0] == 0.f && context.outputs[3][127] == 0.f, "no key → silent side-chain");
        context.sideChainIndex = 1;
        context.run(64);
        require(context.outputs[2][0] == 0.f, "main input channel is never taken as the key");
        // Oversized blocks silence the aux stems too.
        context.outputs[4][0] = 9.f;
        context.run(2048);
        require(context.outputs[4][0] == 0.f && context.outputs[2][2047] == 0.f, "oversized block silences stems");
    }
    {
        // The instrument layout: a declared (ignored) stereo main input, key at index 2.
        Instance synth(instrument, "org.ravet.mock.aux.aax.0x2", AAX_eStemFormat_Stereo, AAX_eStemFormat_Stereo);
        auto &context = *synth.context;
        const auto &plan = synth.parameters->owner()->plan;
        require(synth.parameters->owner()->inputs == 0 && plan.sideChain && plan.hostInputs == 2 && plan.hostOutputs == 5,
                "effect ID selects 0x2 despite equal stems");
        require(context.inputs.size() == 3 && context.outputs.size() == 5, "0x2 host buffers");
        for (uint32_t c = 0; c < 2; ++c) std::fill(context.inputs[c].begin(), context.inputs[c].end(), 0.9f);
        std::fill(context.key().begin(), context.key().end(), 0.75f);
        context.connectKey(true);
        require(context.sideChainIndex == 2, "key after the declared main input");
        context.run(256);
        require(context.outputs[0][0] == 0.f && context.outputs[1][255] == 0.f, "declared main input ignored");
        require(context.outputs[2][0] == 0.75f && context.outputs[3][255] == 0.75f && context.outputs[4][255] == 0.5f,
                "0x2: key reaches the side-chain bus, stems routed");
        context.connectKey(false);
        context.run(64);
        require(context.outputs[2][0] == 0.f, "0x2: no key → silence");
    }
    {
        // Without a matching effect ID the first layout with those stems is used.
        TransportControllerHost host;
        host.effect = "unknown";
        host.input = host.output = AAX_eStemFormat_Stereo;
        std::unique_ptr<AAX_IEffectParameters> model(instrument.newParameters());
        require(model->Initialize(host.unknown()) == AAX_SUCCESS &&
                    dynamic_cast<Parameters *>(model.get())->owner()->inputs == 2,
                "stem fallback picks the first matching layout");
        model->Uninitialize();
    }
    {
        // Aux outputs without a side-chain: 0-input layouts keep a None input
        // stem and today's fields; type IDs unchanged.
        TableCopy copy(aux);
        const RavetPluginAuxiliaryBusV1 outputsOnly[] = {{"Monitor", 0, 2}, {"Direct", 0, 1}};
        copy.product.auxiliaries = outputsOnly;
        copy.product.auxiliary_count = 2;
        setEntryForTesting(&copy.table);
        fake::Collection plain;
        require(GetEffectDescriptions(&plain) == AAX_SUCCESS && plain.effects.size() == 2, "describe outputs-only product");
        auto &c = plain.effects[1].second->component;
        require(c.properties->at(AAX_eProperty_InputStemFormat) == AAX_eStemFormat_None && c.audioIn < 0 && c.sideChain < 0 &&
                    c.fieldCount() == 3 && !c.properties->values.count(AAX_eProperty_SupportsSideChainInput) &&
                    c.properties->at(AAX_eProperty_PlugInID_Native) == 0x52614932 && c.aux.size() == 2,
                "no side-chain: 0x2 stems and fields unchanged");
        auto &c2 = plain.effects[0].second->component;
        require(c2.audioIn == 3 && c2.sideChain < 0 && c2.fieldCount() == 4, "no side-chain: 2x2 fields unchanged");
        // (Description only: the mock's instances prepare against its own bus list.)
        setEntryForTesting(&aux);
    }
    {
        // Per-layout refusal: the host takes both stems for 2x2 but only the
        // first for 0x2; each layout's instances route what it accepted.
        fake::Collection partial;
        partial.auxLimits = {SIZE_MAX, 1};
        require(GetEffectDescriptions(&partial) == AAX_SUCCESS && partial.effects.size() == 2, "describe with a partial refusal");
        require(partial.effects[0].second->component.aux.size() == 2 && partial.effects[1].second->component.aux.size() == 1, "stems per layout");
        require(acceptedOutputStems(*aux.product, aux.product->layouts[0]) == 2 &&
                    acceptedOutputStems(*aux.product, aux.product->layouts[1]) == 1,
                "accepted stems recorded per type ID");
        Instance fx(*partial.effects[0].second, partial.effects[0].first, AAX_eStemFormat_Stereo);
        require(fx.parameters->owner()->plan.hostOutputs == 5 && fx.context->outputs.size() == 5, "2x2 keeps both stems");
        Instance synth(*partial.effects[1].second, partial.effects[1].first, AAX_eStemFormat_Stereo, AAX_eStemFormat_Stereo);
        const auto &plan = synth.parameters->owner()->plan;
        require(plan.hostOutputs == 4 && plan.buses[1].route == AuxiliaryRoute::Output && plan.buses[5].route == AuxiliaryRoute::Unrouted &&
                    synth.context->outputs.size() == 4,
                "0x2 routes only the accepted stem");
        std::fill(synth.context->key().begin(), synth.context->key().end(), 0.75f);
        synth.context->connectKey(true);
        synth.context->run(64);
        require(synth.context->outputs[2][63] == 0.75f && synth.context->outputs[3][0] == 0.75f, "accepted stem written");
    }
    {
        // A host without side-chain or aux stem support: the description still
        // succeeds, the key field becomes zeroed private data, every bus is unrouted.
        fake::Collection bare;
        bare.refuseSideChain = bare.refuseAuxOutputs = true;
        require(GetEffectDescriptions(&bare) == AAX_SUCCESS && bare.effects.size() == 2, "describe without aux support");
        auto &d = *bare.effects[0].second;
        require(d.component.sideChain < 0 && d.component.aux.empty() && d.component.privateFields.count(4) &&
                    d.component.fieldCount() == 5 && !d.component.properties->values.count(AAX_eProperty_SupportsSideChainInput),
                "refused side-chain keeps the field slot");
        require(acceptedOutputStems(*aux.product, aux.product->layouts[0]) == 0, "refused stems recorded");
        Instance fx(d, bare.effects[0].first, AAX_eStemFormat_Stereo);
        const auto &plan = fx.parameters->owner()->plan;
        require(plan.hostOutputs == 2 && plan.buses[1].route == AuxiliaryRoute::Unrouted && plan.buses[5].route == AuxiliaryRoute::Unrouted,
                "refused stems are unrouted");
        require(fx.context->outputs.size() == 2 && fx.context->inputs.size() == 2, "no extra host buffers");
        for (auto &channel : fx.context->inputs) std::fill(channel.begin(), channel.end(), 0.5f);
        fx.context->run(128);
        require(fx.context->outputs[0][127] == 0.5f && ravet_mock_blocks(fx.instance()) == 1, "processes with every bus");
    }
    {
        // Never described: another product identity (no stem record) assumes
        // no stems, even when the host does provide their buffers.
        TableCopy copy(aux);
        copy.product.aax_product = 0x526d6e64; // Rmnd
        require(acceptedOutputStems(copy.product, copy.product.layouts[0]) == 0, "no record for an undescribed effect");
        setEntryForTesting(&copy.table);
        Instance fx(stereo, "org.ravet.mock.aux.aax.2x2", AAX_eStemFormat_Stereo);
        auto &context = *fx.context;
        require(fx.parameters->owner()->plan.hostOutputs == 2 && context.outputs.size() == 5, "undescribed: stems unrouted");
        for (size_t c = 2; c < 5; ++c) std::fill(context.outputs[c].begin(), context.outputs[c].end(), 9.f);
        std::fill(context.key().begin(), context.key().end(), 0.75f);
        context.connectKey(true);
        context.run(128);
        require(context.outputs[2][0] == 9.f && context.outputs[3][127] == 9.f && context.outputs[4][127] == 9.f,
                "undescribed: host stem buffers never written");
        context.run(2048); // oversized: silence covers only the main outputs here
        require(context.outputs[2][0] == 9.f && context.outputs[0][0] == 0.f, "undescribed: silence stays in the main outputs");
        setEntryForTesting(&aux);
    }
    {
        // Malformed auxiliary records are refused before anything is registered.
        const RavetPluginEntryV1 &good = *ravet_mock_auxiliary_v1();
        const RavetPluginAuxiliaryBusV1 cases[][2] = {
            {{"A", 0, 0}, {"B", 1, 0}},  {{"A", 65, 0}, {"B", 1, 0}}, {{"A", 0, 65}, {"B", 1, 0}},
            {{"A", 1, 0}, {"A", 0, 1}},  {{" \t", 1, 0}, {"B", 1, 0}}, {{"", 1, 0}, {"B", 1, 0}},
            {{nullptr, 1, 0}, {"B", 1, 0}},
            {{"01234567890123456789012345678901234567890123456789012345678901234", 1, 0}, {"B", 1, 0}},
        };
        for (const auto &records : cases) {
            RavetPluginProductV1 product = *good.product;
            product.auxiliary_count = 2;
            product.auxiliaries = records;
            RavetPluginEntryV1 table = good;
            table.product = &product;
            setEntryForTesting(&table);
            fake::Collection rejected;
            require(GetEffectDescriptions(&rejected) == AAX_ERROR_INVALID_ARGUMENT && rejected.effects.empty(), "malformed auxiliary bus refused");
        }
        RavetPluginProductV1 product = *good.product;
        RavetPluginEntryV1 table = good;
        table.product = &product;
        setEntryForTesting(&table);
        product.auxiliaries = nullptr;
        fake::Collection missing;
        require(GetEffectDescriptions(&missing) == AAX_ERROR_INVALID_ARGUMENT, "missing auxiliary array refused");
        product.auxiliaries = good.product->auxiliaries;
        product.auxiliary_count = 9;
        fake::Collection many;
        require(GetEffectDescriptions(&many) == AAX_ERROR_INVALID_ARGUMENT, "more than 8 auxiliary buses refused");
        // A 64-byte name is the longest allowed.
        const RavetPluginAuxiliaryBusV1 longest[] = {{"0123456789012345678901234567890123456789012345678901234567890123", 1, 0}};
        product.auxiliary_count = 1;
        product.auxiliaries = longest;
        fake::Collection accepted;
        require(GetEffectDescriptions(&accepted) == AAX_SUCCESS, "64-byte name accepted");
        setEntryForTesting(ravet_mock_auxiliary_v1());
    }
    setEntryForTesting(nullptr);
    std::cout << "auxiliary buses: ok\n";
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
        stemNames();
        auxiliary();
        std::cout << "PASS\n";
        return 0;
    } catch (const std::exception &error) {
        std::cerr << "FAIL: " << error.what() << "\n";
        return 1;
    }
}
