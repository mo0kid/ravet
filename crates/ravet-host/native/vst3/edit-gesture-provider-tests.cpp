// Edit-gesture companion through the exported C tables with a substituted
// module loader (decision 0276): legacy handler behaviour without enable,
// argument/alias/thread refusals, injected gestures from a fixture view and
// controller (including reentry from view and parameter CONTROL callbacks),
// restartComponent flag policy, bounded overflow that refuses rather than
// drops, drain while an AUDIO capability is live, and edits during retirement.
#include "RavetVst3EditGestures.h"
#include "RavetVst3Editor.h"
#include "RavetVst3Parameters.h"
#include "editor-view-fixture.hpp"
#include "public.sdk/source/vst/hosting/module.h"
#include "pluginterfaces/vst/ivstaudioprocessor.h"
#include "pluginterfaces/vst/ivstcomponent.h"
#include <cmath>
#include <filesystem>
#include <iostream>
#include <limits>
#include <map>
#include <thread>
#include <unistd.h>

using namespace editor_fixture;
namespace {
struct Plugin {
    Script view;                     // scripted IPlugView from the shared editor fixture
    IComponentHandler* handler = nullptr;
    std::function<void()> onSet;     // runs inside controller setParamNormalized
    bool editOnClear = false;        // performEdit from inside setComponentHandler(nullptr)
    tresult clearEdit = kResultOk;
    std::vector<std::string> log;
};
Plugin* active = nullptr;
class Component final : public IComponent, public IAudioProcessor {
public:
    explicit Component(Plugin& p) : p_(p) {}
    tresult PLUGIN_API queryInterface(const TUID id, void** output) override {
        *output = nullptr;
        if (FUnknownPrivate::iidEqual(id, FUnknown::iid) || FUnknownPrivate::iidEqual(id, IComponent::iid) ||
            FUnknownPrivate::iidEqual(id, IPluginBase::iid)) *output = static_cast<IComponent*>(this);
        else if (FUnknownPrivate::iidEqual(id, IAudioProcessor::iid)) *output = static_cast<IAudioProcessor*>(this);
        else return kNoInterface;
        addRef();
        return kResultOk;
    }
    uint32 PLUGIN_API addRef() override { return ++refs_; }
    uint32 PLUGIN_API release() override { const auto n = --refs_; if (!n) delete this; return n; }
    tresult PLUGIN_API initialize(FUnknown*) override { return kResultOk; }
    tresult PLUGIN_API terminate() override { p_.log.emplace_back("component-terminate"); return kResultOk; }
    tresult PLUGIN_API getControllerClassId(TUID id) override { std::memset(id, 0, 16); id[0] = 2; return kResultOk; }
    tresult PLUGIN_API setIoMode(IoMode) override { return kNotImplemented; }
    int32 PLUGIN_API getBusCount(MediaType type, BusDirection) override { return type == kAudio ? 1 : 0; }
    tresult PLUGIN_API getBusInfo(MediaType type, BusDirection direction, int32, BusInfo& info) override {
        info = {}; info.mediaType = type; info.direction = direction; info.channelCount = 2; info.busType = kMain;
        info.flags = BusInfo::kDefaultActive; info.name[0] = 'm'; return kResultOk;
    }
    tresult PLUGIN_API getRoutingInfo(RoutingInfo&, RoutingInfo&) override { return kNotImplemented; }
    tresult PLUGIN_API activateBus(MediaType, BusDirection, int32, TBool) override { return kResultOk; }
    tresult PLUGIN_API setActive(TBool) override { return kResultOk; }
    tresult PLUGIN_API setState(IBStream*) override { return kResultOk; }
    tresult PLUGIN_API getState(IBStream*) override { return kResultOk; }
    tresult PLUGIN_API setBusArrangements(SpeakerArrangement*, int32, SpeakerArrangement*, int32) override { return kResultOk; }
    tresult PLUGIN_API getBusArrangement(BusDirection, int32, SpeakerArrangement& out) override { out = SpeakerArr::kStereo; return kResultOk; }
    tresult PLUGIN_API canProcessSampleSize(int32 size) override { return size == kSample32 ? kResultOk : kResultFalse; }
    uint32 PLUGIN_API getLatencySamples() override { return 0; }
    tresult PLUGIN_API setupProcessing(ProcessSetup&) override { return kResultOk; }
    tresult PLUGIN_API setProcessing(TBool) override { return kResultOk; }
    tresult PLUGIN_API process(ProcessData&) override { return kResultOk; }
    uint32 PLUGIN_API getTailSamples() override { return 0; }
private:
    Plugin& p_;
    uint32 refs_ = 1;
};
// Parameters: 1 automatable, 2 writable, 7 read-only, 9 hidden read-only, and
// writable 100..199 for the open-gesture limit.
const std::vector<std::pair<ParamID, int32>> parameters = [] {
    std::vector<std::pair<ParamID, int32>> list{{1, ParameterInfo::kCanAutomate}, {2, 0},
        {7, ParameterInfo::kIsReadOnly}, {9, ParameterInfo::kIsReadOnly | ParameterInfo::kIsHidden}};
    for (ParamID id = 100; id < 200; ++id) list.emplace_back(id, ParameterInfo::kCanAutomate);
    return list;
}();
class Controller final : public IEditController {
public:
    explicit Controller(Plugin& p) : p_(p) {}
    tresult PLUGIN_API queryInterface(const TUID id, void** output) override {
        *output = nullptr;
        if (FUnknownPrivate::iidEqual(id, FUnknown::iid) || FUnknownPrivate::iidEqual(id, IEditController::iid) ||
            FUnknownPrivate::iidEqual(id, IPluginBase::iid)) {
            *output = static_cast<IEditController*>(this); addRef(); return kResultOk;
        }
        return kNoInterface;
    }
    uint32 PLUGIN_API addRef() override { return ++refs_; }
    uint32 PLUGIN_API release() override { const auto n = --refs_; if (!n) delete this; return n; }
    tresult PLUGIN_API initialize(FUnknown*) override { return kResultOk; }
    tresult PLUGIN_API terminate() override { p_.log.emplace_back("controller-terminate"); return kResultOk; }
    tresult PLUGIN_API setComponentState(IBStream*) override { return kResultOk; }
    tresult PLUGIN_API setState(IBStream*) override { return kResultOk; }
    tresult PLUGIN_API getState(IBStream*) override { return kResultOk; }
    int32 PLUGIN_API getParameterCount() override { return int32(parameters.size()); }
    tresult PLUGIN_API getParameterInfo(int32 index, ParameterInfo& info) override {
        if (index < 0 || index >= int32(parameters.size())) return kInvalidArgument;
        info = {}; info.id = parameters[size_t(index)].first; info.flags = parameters[size_t(index)].second;
        info.title[0] = 'p'; info.defaultNormalizedValue = 0.5; return kResultOk;
    }
    tresult PLUGIN_API getParamStringByValue(ParamID, ParamValue, String128) override { return kResultFalse; }
    tresult PLUGIN_API getParamValueByString(ParamID, TChar*, ParamValue&) override { return kResultFalse; }
    ParamValue PLUGIN_API normalizedParamToPlain(ParamID, ParamValue value) override { return value; }
    ParamValue PLUGIN_API plainParamToNormalized(ParamID, ParamValue value) override { return value; }
    ParamValue PLUGIN_API getParamNormalized(ParamID) override { return 0.5; }
    tresult PLUGIN_API setParamNormalized(ParamID, ParamValue) override { if (p_.onSet) p_.onSet(); return kResultOk; }
    tresult PLUGIN_API setComponentHandler(IComponentHandler* handler) override {
        if (handler) { handler->addRef(); p_.handler = handler; p_.log.emplace_back("handler"); return kResultOk; }
        if (p_.handler) {
            if (p_.editOnClear) p_.clearEdit = p_.handler->performEdit(1, 0.75);
            p_.handler->release(); p_.handler = nullptr;
        }
        p_.log.emplace_back("handler-clear");
        return kResultOk;
    }
    IPlugView* PLUGIN_API createView(FIDString name) override {
        require(pthread_main_np() && name && std::strcmp(name, ViewType::kEditor) == 0);
        return static_cast<IPlugView*>(new View(p_.view));
    }
private:
    Plugin& p_;
    uint32 refs_ = 1;
};
struct Factory final : IPluginFactory3 {
    Plugin* plugin = nullptr;
    uint32_t references = 1;
    tresult PLUGIN_API queryInterface(const TUID id, void** output) override {
        *output = nullptr;
        if (FUnknownPrivate::iidEqual(id, FUnknown::iid) || FUnknownPrivate::iidEqual(id, IPluginFactory::iid) ||
            FUnknownPrivate::iidEqual(id, IPluginFactory2::iid) || FUnknownPrivate::iidEqual(id, IPluginFactory3::iid)) {
            *output = static_cast<IPluginFactory3*>(this); addRef(); return kResultOk;
        }
        return kNoInterface;
    }
    uint32 PLUGIN_API addRef() override { return ++references; }
    uint32 PLUGIN_API release() override { return --references; }
    tresult PLUGIN_API getFactoryInfo(PFactoryInfo* out) override { *out = {}; std::strcpy(out->vendor, "RAVET"); return kResultOk; }
    int32 PLUGIN_API countClasses() override { return 2; }
    tresult PLUGIN_API getClassInfo(int32 index, PClassInfo* out) override {
        *out = {}; out->cid[0] = char(index + 1); out->cardinality = 1;
        std::strcpy(out->category, index ? "Component Controller Class" : "Audio Module Class");
        std::strcpy(out->name, "Gesture fixture"); return kResultOk;
    }
    tresult PLUGIN_API getClassInfo2(int32, PClassInfo2*) override { return kNotImplemented; }
    tresult PLUGIN_API getClassInfoUnicode(int32 index, PClassInfoW* out) override {
        *out = {}; out->cid[0] = char(index + 1); out->cardinality = 1;
        std::strcpy(out->category, index ? "Component Controller Class" : "Audio Module Class");
        out->name[0] = 'g'; return kResultOk;
    }
    tresult PLUGIN_API createInstance(FIDString id, FIDString, void** output) override {
        require(plugin);
        if (id[0] == 2) *output = static_cast<IEditController*>(new Controller(*plugin));
        else *output = static_cast<IComponent*>(new Component(*plugin));
        return kResultOk;
    }
    tresult PLUGIN_API setHostContext(FUnknown*) override { return kNotImplemented; }
};
std::map<std::string, Factory*> modules;
class FixtureModule final : public VST3::Hosting::Module {
    Factory& owner_;
public:
    explicit FixtureModule(Factory& owner) : owner_(owner) { owner_.addRef(); factory = VST3::Hosting::PluginFactory(owned<IPluginFactory>(&owner_)); }
    ~FixtureModule() noexcept override { factory = VST3::Hosting::PluginFactory(nullptr); if (active) active->log.emplace_back("module-exit"); }
    bool load(const std::string&, std::string&) override { return true; }
};
} // namespace
namespace VST3::Hosting {
Module::Ptr Module::create(const std::string& path, std::string&) {
    return std::make_shared<FixtureModule>(*modules.at(std::filesystem::canonical(path).string()));
}
}
namespace {
const RavetVst3ProviderV2* audio = nullptr;
const RavetVst3ParameterProviderV1* params = nullptr;
const RavetVst3EditorProviderV1* editors = nullptr;
const RavetVst3EditGestureProviderV1* gestures = nullptr;
RavetVst3EditGestureState state(RavetVst3Instance* child) {
    RavetVst3EditGestureState s{}; s.struct_size = sizeof(s);
    require(gestures->state(child, &s) == RAVET_VST3_OK && s.struct_size == sizeof(s));
    return s;
}
std::vector<RavetVst3EditGesture> drain(RavetVst3Instance* child, uint32_t capacity = RAVET_VST3_EDIT_GESTURE_CAPACITY) {
    std::vector<RavetVst3EditGesture> out(capacity); uint32_t count = 77;
    require(gestures->drain(child, out.data(), capacity, &count) == RAVET_VST3_OK && count <= capacity);
    out.resize(count); return out;
}
void expect(const RavetVst3EditGesture& g, uint32_t kind, uint32_t id, double value, uint64_t sequence, uint32_t flags = 0) {
    if (g.kind != kind || g.id != id || g.value != value || g.sequence != sequence || g.flags != flags)
        std::cerr << "gesture kind " << g.kind << " id " << g.id << " value " << g.value << " seq " << g.sequence << " flags " << g.flags << '\n';
    require(g.struct_size == sizeof(g) && g.kind == kind && g.id == id && g.value == value && g.sequence == sequence && g.flags == flags);
}
struct Fixture {
    Plugin plugin;
    Factory factory;
    std::string path;
    RavetVst3Module* parent = nullptr;
    RavetVst3Instance* child = nullptr;
    Fixture(const std::filesystem::path& root, const std::string& name, bool refresh = true, bool enable = true) {
        factory.plugin = &plugin;
        const auto directory = root / (name + ".vst3");
        std::filesystem::create_directory(directory);
        path = directory.string();
        modules[std::filesystem::canonical(directory).string()] = &factory;
        active = &plugin;
        int32_t native = 0;
        require(audio->open(reinterpret_cast<const uint8_t*>(path.data()), uint32_t(path.size()), &parent, &native) == RAVET_VST3_OK);
        require(audio->create(parent, 0, &child, &native) == RAVET_VST3_OK && plugin.handler);
        if (refresh) {
            uint32_t count = 0; RavetVst3ParameterReport report{}; report.struct_size = sizeof(report);
            require(params->refresh(child, &count, &report) == RAVET_VST3_OK && count == parameters.size());
        }
        if (enable) require(gestures->enable(child) == RAVET_VST3_OK);
    }
    void close() {
        int32_t native = 0;
        require(audio->destroy(child, &native) == RAVET_VST3_OK); child = nullptr;
        require(audio->close(parent, &native) == RAVET_VST3_OK); parent = nullptr;
        active = nullptr;
    }
};
void legacy(const std::filesystem::path& root) {
    Fixture f(root, "legacy", true, false);
    auto* h = f.plugin.handler;
    // Without enable the original component handler contract is unchanged.
    require(h->beginEdit(1) == kNotImplemented && h->performEdit(1, 0.5) == kNotImplemented && h->endEdit(1) == kNotImplemented);
    require(h->restartComponent(kIoChanged | kLatencyChanged) == kResultOk && h->restartComponent(0) == kInvalidArgument);
    uint32_t requests = 0;
    require(audio->restart_requests(f.child, &requests) == RAVET_VST3_OK && requests == uint32_t(kIoChanged | kLatencyChanged));
    std::array<RavetVst3EditGesture, 4> out{}; uint32_t count = 9;
    require(gestures->drain(f.child, out.data(), 4, &count) == RAVET_VST3_LIFECYCLE && count == 9);
    const auto s = state(f.child);
    require(s.flags == 0 && s.capacity == RAVET_VST3_EDIT_GESTURE_CAPACITY && s.pending == 0 && s.accepted == 0);
    f.close();
}
void arguments(const std::filesystem::path& root) {
    require(gestures->struct_size == sizeof(RavetVst3EditGestureProviderV1) && gestures->version == 1 && gestures->instance_origin == audio);
    Fixture f(root, "arguments", false, false);
    require(gestures->enable(nullptr) == RAVET_VST3_INVALID);
    require(gestures->enable(f.child) == RAVET_VST3_LIFECYCLE); // catalogue not refreshed
    uint32_t count = 0; RavetVst3ParameterReport report{}; report.struct_size = sizeof(report);
    require(params->refresh(f.child, &count, &report) == RAVET_VST3_OK);
    std::thread([&] { require(gestures->enable(f.child) == RAVET_VST3_THREAD); }).join();
    require(gestures->enable(f.child) == RAVET_VST3_OK);
    require(gestures->enable(f.child) == RAVET_VST3_OK); // re-enable refreshes the snapshot
    std::array<RavetVst3EditGesture, 4> out{}; uint32_t n = 5;
    require(gestures->drain(nullptr, out.data(), 4, &n) == RAVET_VST3_INVALID);
    require(gestures->drain(f.child, nullptr, 4, &n) == RAVET_VST3_INVALID);
    require(gestures->drain(f.child, out.data(), 4, nullptr) == RAVET_VST3_INVALID);
    require(gestures->drain(f.child, out.data(), 0, &n) == RAVET_VST3_INVALID);
    require(gestures->drain(f.child, out.data(), RAVET_VST3_EDIT_GESTURE_CAPACITY + 1, &n) == RAVET_VST3_INVALID);
    // Aliased count/output and owner overlap refuse before any copy.
    require(gestures->drain(f.child, out.data(), 4, reinterpret_cast<uint32_t*>(&out[1])) == RAVET_VST3_INVALID);
    require(gestures->drain(f.child, reinterpret_cast<RavetVst3EditGesture*>(f.child), 1, &n) == RAVET_VST3_INVALID);
    require(gestures->drain(f.child, reinterpret_cast<RavetVst3EditGesture*>(f.plugin.handler), 1, &n) == RAVET_VST3_INVALID);
    require(n == 5);
    std::thread([&] { uint32_t local = 0; std::array<RavetVst3EditGesture, 1> o{}; RavetVst3EditGestureState s{}; s.struct_size = sizeof(s);
        require(gestures->drain(f.child, o.data(), 1, &local) == RAVET_VST3_THREAD);
        require(gestures->state(f.child, &s) == RAVET_VST3_THREAD); }).join();
    RavetVst3EditGestureState wrong{}; wrong.struct_size = 3;
    require(gestures->state(f.child, &wrong) == RAVET_VST3_INVALID && gestures->state(nullptr, &wrong) == RAVET_VST3_INVALID);
    require(gestures->drain(f.child, out.data(), 4, &n) == RAVET_VST3_OK && n == 0);
    f.close();
}
void capture(const std::filesystem::path& root) {
    Fixture f(root, "capture");
    auto* h = f.plugin.handler;
    require(h->beginEdit(1) == kResultOk && h->performEdit(1, 0.25) == kResultOk && h->performEdit(1, 0.5) == kResultOk && h->endEdit(1) == kResultOk);
    require(h->performEdit(2, 1.0) == kResultOk && h->performEdit(2, 0.0) == kResultOk);
    // Unknown, read-only and hidden ids; out-of-range and non-finite values.
    require(h->beginEdit(42) == kInvalidArgument && h->performEdit(7, 0.5) == kInvalidArgument && h->endEdit(9) == kInvalidArgument);
    // Non-finite values from bit patterns, so the fast-math profile keeps them.
    auto bits = [](uint64_t pattern) { double value = 0; std::memcpy(&value, &pattern, sizeof(value)); return value; };
    for (double bad : {bits(UINT64_C(0x7ff8000000000000)), bits(UINT64_C(0x7ff0000000000000)), -0.1, 1.5})
        require(h->performEdit(1, bad) == kInvalidArgument);
    std::thread([&] { require(h->performEdit(1, 0.3) == kResultFalse && h->beginEdit(1) == kResultFalse); }).join();
    auto s = state(f.child);
    require(s.flags == 1 && s.pending == 6 && s.accepted == 6 && s.unknown == 3 && s.invalid == 4 && s.off_thread == 2 && s.overflowed == 0);
    const auto first = drain(f.child, 3);
    require(first.size() == 3);
    expect(first[0], RAVET_VST3_EDIT_BEGIN, 1, 0, 0); expect(first[1], RAVET_VST3_EDIT_PERFORM, 1, 0.25, 1); expect(first[2], RAVET_VST3_EDIT_PERFORM, 1, 0.5, 2);
    const auto rest = drain(f.child);
    require(rest.size() == 3);
    expect(rest[0], RAVET_VST3_EDIT_END, 1, 0, 3); expect(rest[1], RAVET_VST3_EDIT_PERFORM, 2, 1.0, 4); expect(rest[2], RAVET_VST3_EDIT_PERFORM, 2, 0.0, 5);
    s = state(f.child);
    require(s.pending == 0 && s.drained == 6 && drain(f.child).empty());
    f.close();
}
void restarts(const std::filesystem::path& root) {
    Fixture f(root, "restart");
    auto* h = f.plugin.handler;
    require(h->restartComponent(kParamValuesChanged) == kResultOk && h->restartComponent(kLatencyChanged) == kResultOk);
    require(h->restartComponent(kParamTitlesChanged | kParamValuesChanged) == kResultOk);
    for (int32 refused : {int32(kReloadComponent), int32(kIoChanged), int32(kMidiCCAssignmentChanged), int32(kNoteExpressionChanged),
                          int32(kIoTitlesChanged), int32(kPrefetchableSupportChanged), int32(kRoutingInfoChanged),
                          int32(kKeyswitchChanged), int32(kParamIDMappingChanged), int32(kParamValuesChanged | kIoChanged)})
        require(h->restartComponent(refused) == kResultFalse);
    require(h->restartComponent(0) == kInvalidArgument && h->restartComponent(0x1000) == kInvalidArgument);
    std::thread([&] { require(h->restartComponent(kParamValuesChanged) == kResultFalse); }).join();
    uint32_t requests = 0;
    require(audio->restart_requests(f.child, &requests) == RAVET_VST3_OK && requests == uint32_t(kParamValuesChanged | kLatencyChanged | kParamTitlesChanged));
    const auto s = state(f.child);
    require(s.restart_accepted == 3 && s.restart_refused == 10 && s.invalid == 2 && s.off_thread == 1 && s.accepted == 0);
    const auto records = drain(f.child);
    require(records.size() == 13);
    expect(records[0], RAVET_VST3_EDIT_RESTART, 0, 0, 0, kParamValuesChanged);
    expect(records[1], RAVET_VST3_EDIT_RESTART, 0, 0, 1, kLatencyChanged);
    expect(records[2], RAVET_VST3_EDIT_RESTART, 0, 0, 2, kParamTitlesChanged | kParamValuesChanged);
    expect(records[3], RAVET_VST3_EDIT_RESTART_REFUSED, 0, 0, 3, kReloadComponent);
    expect(records[12], RAVET_VST3_EDIT_RESTART_REFUSED, 0, 0, 12, kParamValuesChanged | kIoChanged);
    f.close();
}
void overflow(const std::filesystem::path& root) {
    Fixture f(root, "overflow");
    auto* h = f.plugin.handler;
    for (uint32_t i = 0; i < RAVET_VST3_EDIT_GESTURE_CAPACITY; ++i) require(h->performEdit(1, double(i) / 1024.) == kResultOk);
    // A full ring refuses; nothing accepted is overwritten.
    require(h->performEdit(1, 0.9) == kResultFalse && h->beginEdit(2) == kResultFalse);
    require(h->restartComponent(kParamValuesChanged) == kResultFalse && h->restartComponent(kIoChanged) == kResultFalse);
    auto s = state(f.child);
    require(s.pending == RAVET_VST3_EDIT_GESTURE_CAPACITY && s.accepted == RAVET_VST3_EDIT_GESTURE_CAPACITY && s.overflowed == 4 && s.restart_refused == 1);
    const auto first = drain(f.child, 10);
    for (uint32_t i = 0; i < 10; ++i) expect(first[i], RAVET_VST3_EDIT_PERFORM, 1, double(i) / 1024., i);
    // A begin reserves its end: 10 free slots admit the begin plus 8 values.
    require(h->beginEdit(2) == kResultOk);
    for (uint32_t i = 0; i < 8; ++i) require(h->performEdit(2, 0.5) == kResultOk);
    require(h->performEdit(2, 0.5) == kResultFalse && h->beginEdit(1) == kResultFalse);
    s = state(f.child);
    require(s.pending == RAVET_VST3_EDIT_GESTURE_CAPACITY - 1 && s.open == 1 && s.overflowed == 6);
    // The end of the accepted begin still records in its reserved slot.
    require(h->endEdit(2) == kResultOk && h->endEdit(2) == kResultFalse);
    const auto all = drain(f.child);
    require(all.size() == RAVET_VST3_EDIT_GESTURE_CAPACITY);
    for (uint32_t i = 0; i < all.size(); ++i) require(all[i].sequence == 10 + i); // contiguous, no gaps
    expect(all[all.size() - 10], RAVET_VST3_EDIT_BEGIN, 2, 0, 10 + RAVET_VST3_EDIT_GESTURE_CAPACITY - 10);
    expect(all.back(), RAVET_VST3_EDIT_END, 2, 0, 10 + RAVET_VST3_EDIT_GESTURE_CAPACITY - 1);
    s = state(f.child);
    require(s.overflowed == 6 && s.unpaired == 1 && s.open == 0 && s.drained == 10 + RAVET_VST3_EDIT_GESTURE_CAPACITY && s.pending == 0);
    // 1000 fill/drain cycles stay bounded and contiguous.
    uint64_t next = 10 + RAVET_VST3_EDIT_GESTURE_CAPACITY;
    for (int cycle = 0; cycle < 1000; ++cycle) {
        require(h->beginEdit(1) == kResultOk && h->performEdit(1, 0.5) == kResultOk && h->endEdit(1) == kResultOk);
        const auto batch = drain(f.child, 2);
        require(batch.size() == 2 && batch[0].sequence == next && batch[1].sequence == next + 1);
        const auto tail = drain(f.child, 4);
        require(tail.size() == 1 && tail[0].sequence == next + 2);
        next += 3;
    }
    f.close();
}
// Begin/end pairing under refusal: a full ring mid-gesture never refuses the
// end of an accepted begin, refused begins never record, and drained records
// are always paired.
void pairing(const std::filesystem::path& root) {
    Fixture f(root, "pairing");
    auto* h = f.plugin.handler;
    require(h->beginEdit(1) == kResultOk);
    uint32_t values = 0;
    while (h->performEdit(1, 0.25) == kResultOk) ++values;
    require(values == RAVET_VST3_EDIT_GESTURE_CAPACITY - 2); // begin + reserved end
    // Full mid-gesture: no new begin, but the open gesture still ends.
    require(h->beginEdit(2) == kResultFalse && h->endEdit(2) == kResultFalse);
    require(h->endEdit(1) == kResultOk);
    auto s = state(f.child);
    require(s.pending == RAVET_VST3_EDIT_GESTURE_CAPACITY && s.open == 0 && s.unpaired == 1 && s.overflowed == 2);
    auto records = drain(f.child);
    expect(records.front(), RAVET_VST3_EDIT_BEGIN, 1, 0, 0);
    expect(records.back(), RAVET_VST3_EDIT_END, 1, 0, RAVET_VST3_EDIT_GESTURE_CAPACITY - 1);
    // A duplicate begin of an open id is unpaired; the gesture stays single.
    require(h->beginEdit(1) == kResultOk && h->beginEdit(1) == kResultFalse && h->endEdit(1) == kResultOk);
    require(h->endEdit(1) == kResultFalse);
    records = drain(f.child);
    require(records.size() == 2 && records[0].kind == RAVET_VST3_EDIT_BEGIN && records[1].kind == RAVET_VST3_EDIT_END);
    // With one unreserved slot left, a begin (needing two) is refused while a
    // value and the open gesture's end are still admitted.
    require(h->beginEdit(2) == kResultOk);
    while (state(f.child).pending + state(f.child).open < RAVET_VST3_EDIT_GESTURE_CAPACITY - 1)
        require(h->performEdit(2, 0.5) == kResultOk);
    require(h->beginEdit(1) == kResultFalse && h->performEdit(2, 1.0) == kResultOk && h->endEdit(2) == kResultOk);
    records = drain(f.child);
    require(records.front().kind == RAVET_VST3_EDIT_BEGIN && records.back().kind == RAVET_VST3_EDIT_END);
    // At most 64 open gestures; ending one admits another.
    for (ParamID id = 100; id < 164; ++id) require(h->beginEdit(id) == kResultOk);
    require(state(f.child).open == RAVET_VST3_EDIT_OPEN_GESTURES && h->beginEdit(164) == kResultFalse);
    require(h->endEdit(100) == kResultOk && h->beginEdit(164) == kResultOk);
    for (ParamID id = 101; id < 165; ++id) require(h->endEdit(id) == kResultOk);
    records = drain(f.child);
    // Every drained record set is balanced per id.
    std::map<uint32_t, int> depth;
    for (const auto& r : records) {
        if (r.kind == RAVET_VST3_EDIT_BEGIN) require(++depth[r.id] == 1);
        if (r.kind == RAVET_VST3_EDIT_END) require(--depth[r.id] == 0);
    }
    for (const auto& [id, d] : depth) require(d == 0);
    s = state(f.child);
    require(s.open == 0 && s.pending == 0 && s.unpaired == 3);
    f.close();
}
// N3/N4 (R3): closing the editor with gestures open records a synthesised END
// per open gesture; a late endEdit is then unpaired. kNoParamId is refused.
void editorCloseSynthesizesEnd(const std::filesystem::path& root) {
    Fixture f(root, "synthesize");
    auto* h = f.plugin.handler;
    require(h->performEdit(kNoParamId, 0.5) == kInvalidArgument && h->beginEdit(kNoParamId) == kInvalidArgument);
    RavetVst3Editor* editor = nullptr; RavetVst3EditorSize initial{sizeof(initial), 0, 0, 0};
    RavetVst3EditorReport r{}; r.struct_size = sizeof(r);
    require(editors->create(f.child, RAVET_VST3_EDITOR_PLATFORM_NSVIEW, &editor, &initial, &r) == RAVET_VST3_OK);
    require(h->beginEdit(1) == kResultOk && h->beginEdit(2) == kResultOk && h->performEdit(2, 0.5) == kResultOk);
    require(state(f.child).open == 2);
    require(editors->destroy(editor, &r) == RAVET_VST3_OK);
    auto s = state(f.child);
    require(s.open == 0 && s.unknown == 2);
    const auto records = drain(f.child);
    require(records.size() == 5);
    expect(records[0], RAVET_VST3_EDIT_BEGIN, 1, 0, 0);
    expect(records[3], RAVET_VST3_EDIT_END, 1, 0, 3, RAVET_VST3_EDIT_SYNTHESIZED);
    expect(records[4], RAVET_VST3_EDIT_END, 2, 0, 4, RAVET_VST3_EDIT_SYNTHESIZED);
    require(h->endEdit(1) == kResultFalse && state(f.child).unpaired == 1);
    // A closed editor with no open gesture records nothing extra.
    editor = nullptr;
    require(editors->create(f.child, RAVET_VST3_EDITOR_PLATFORM_NSVIEW, &editor, &initial, &r) == RAVET_VST3_OK);
    require(editors->destroy(editor, &r) == RAVET_VST3_OK && drain(f.child).empty());
    f.close();
}
void reentry(const std::filesystem::path& root) {
    Fixture f(root, "reentry");
    auto* h = f.plugin.handler;
    std::vector<tresult> edits; std::vector<int32_t> nested;
    // Gestures from inside host calls into the view (editor busy flag held).
    f.plugin.view.hook = [&](const std::string& name) {
        if (name == "attached") {
            edits.push_back(h->beginEdit(1)); edits.push_back(h->performEdit(1, 0.125)); edits.push_back(h->endEdit(1));
            std::array<RavetVst3EditGesture, 2> out{}; uint32_t count = 0; RavetVst3EditGestureState s{}; s.struct_size = sizeof(s);
            nested.push_back(gestures->drain(f.child, out.data(), 2, &count));   // busy: refused
            nested.push_back(gestures->state(f.child, &s));                      // copied read stays available
            nested.push_back(gestures->enable(f.child));                         // CONTROL: refused
        }
        if (name == "onSize") edits.push_back(h->restartComponent(kParamValuesChanged));
        if (name == "removed") edits.push_back(h->performEdit(2, 0.625));
    };
    RavetVst3Editor* editor = nullptr; RavetVst3EditorSize initial{sizeof(initial), 0, 0, 0};
    RavetVst3EditorReport r{}; r.struct_size = sizeof(r);
    require(editors->create(f.child, RAVET_VST3_EDITOR_PLATFORM_NSVIEW, &editor, &initial, &r) == RAVET_VST3_OK && editor);
    int parentStorage = 0; RavetVst3EditorSize applied{sizeof(applied), 0, 0, 0};
    require(editors->attach(editor, &parentStorage, &applied, &r) == RAVET_VST3_OK);
    RavetVst3EditorSize proposed{sizeof(proposed), 640, 480, 0};
    require(editors->resize(editor, &proposed, &applied, &r) == RAVET_VST3_OK);
    // A plug-in UI event: an idle frame resize also runs onSize under the gate.
    require(f.plugin.view.current->frame(ViewRect(0, 0, 700, 500)) == kResultTrue);
    // The child cannot retire while the view lives; edits stay accepted.
    int32_t native = 0;
    require(audio->destroy(f.child, &native) == RAVET_VST3_LIFECYCLE);
    require(editors->destroy(editor, &r) == RAVET_VST3_OK);
    require(edits == (std::vector<tresult>{kResultOk, kResultOk, kResultOk, kResultOk, kResultOk, kResultOk}));
    require(nested == (std::vector<int32_t>{RAVET_VST3_LIFECYCLE, RAVET_VST3_OK, RAVET_VST3_LIFECYCLE}));
    // Parameter CONTROL call into the controller that echoes a performEdit.
    f.plugin.view.hook = nullptr;
    f.plugin.onSet = [&] { edits.push_back(h->performEdit(1, 1.0)); };
    double value = 0; RavetVst3ParameterReport report{}; report.struct_size = sizeof(report);
    require(params->value(f.child, 1, RAVET_VST3_PARAMETER_EDIT, 0.25, &value, &report) == RAVET_VST3_OK && edits.back() == kResultOk);
    f.plugin.onSet = nullptr;
    const auto records = drain(f.child);
    require(records.size() == 7);
    expect(records[0], RAVET_VST3_EDIT_BEGIN, 1, 0, 0); expect(records[1], RAVET_VST3_EDIT_PERFORM, 1, 0.125, 1);
    expect(records[2], RAVET_VST3_EDIT_END, 1, 0, 2);
    expect(records[3], RAVET_VST3_EDIT_RESTART, 0, 0, 3, kParamValuesChanged);
    expect(records[4], RAVET_VST3_EDIT_RESTART, 0, 0, 4, kParamValuesChanged);
    expect(records[5], RAVET_VST3_EDIT_PERFORM, 2, 0.625, 5);
    expect(records[6], RAVET_VST3_EDIT_PERFORM, 1, 1.0, 6);
    f.close();
}
void duringAudio(const std::filesystem::path& root) {
    Fixture f(root, "audio");
    auto* h = f.plugin.handler;
    int32_t native = 0;
    RavetVst3AudioProfile profile{sizeof(profile), 64, 2, 2, 48000};
    require(audio->prepare(f.child, &profile, &native) == RAVET_VST3_OK);
    require(audio->start(f.child, &native) == RAVET_VST3_OK);
    RavetVst3Audio* lease = nullptr;
    require(audio->begin_audio(f.child, &lease, &native) == RAVET_VST3_OK && lease);
    // CONTROL enable refuses while a lease is live; drain and state stay usable.
    require(gestures->enable(f.child) == RAVET_VST3_LIFECYCLE);
    for (int cycle = 0; cycle < 100; ++cycle) {
        require(h->beginEdit(1) == kResultOk && h->performEdit(1, 0.5) == kResultOk && h->endEdit(1) == kResultOk);
        require(drain(f.child).size() == 3);
    }
    std::thread worker([&] { require(h->performEdit(1, 0.5) == kResultFalse); }); // a processor-thread edit
    worker.join();
    const auto s = state(f.child);
    require(s.accepted == 300 && s.drained == 300 && s.off_thread == 1);
    require(audio->end_audio(lease, &native) == RAVET_VST3_OK);
    require(audio->stop(f.child, &native) == RAVET_VST3_OK);
    f.close();
}
void retirement(const std::filesystem::path& root) {
    Fixture f(root, "retire");
    f.plugin.editOnClear = true;
    require(f.plugin.handler->performEdit(1, 0.5) == kResultOk);
    int32_t native = 0;
    require(audio->destroy(f.child, &native) == RAVET_VST3_OK); f.child = nullptr;
    // The handler retires before setComponentHandler(nullptr): a late edit refuses.
    require(f.plugin.clearEdit == kResultFalse && !f.plugin.handler);
    require(audio->close(f.parent, &native) == RAVET_VST3_OK); f.parent = nullptr;
    const auto& log = f.plugin.log;
    auto at = [&](const std::string& name) { const auto i = std::find(log.begin(), log.end(), name); require(i != log.end()); return i - log.begin(); };
    require(at("handler-clear") < at("controller-terminate") && at("controller-terminate") < at("component-terminate") &&
            at("component-terminate") < at("module-exit"));
    active = nullptr;
}
} // namespace
int main() {
    try {
        require(pthread_main_np());
        audio = ravet_vst3_provider_v2(); params = ravet_vst3_parameter_provider_v1();
        editors = ravet_vst3_editor_provider_v1(); gestures = ravet_vst3_edit_gesture_provider_v1();
        char directory[] = "/tmp/ravet-vst3-gesture-XXXXXX";
        require(mkdtemp(directory));
        const std::filesystem::path root(directory);
        legacy(root); arguments(root); capture(root); restarts(root); overflow(root); pairing(root); editorCloseSynthesizesEnd(root); reentry(root); duringAudio(root); retirement(root);
        std::filesystem::remove_all(root);
        std::cout << "PASS edit-gesture provider: legacy handler unchanged without enable, exact origin, argument/alias/"
                     "thread refusals, ordered capture with id/value/thread validation, restart flag policy, bounded "
                     "overflow refusal with contiguous sequences over 1000 cycles, begin/end pairing with reserved end slots "
                     "under a full ring and a 64-gesture limit, synthesised ends at editor close, kNoParamId refusal, "
                     "reentry from view and parameter "
                     "callbacks, drain during live AUDIO, and refusal after handler retirement\n";
    } catch (const std::exception& error) {
        std::cerr << error.what() << '\n';
        return 1;
    }
}
