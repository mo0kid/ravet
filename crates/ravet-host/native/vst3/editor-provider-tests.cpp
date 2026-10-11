// Editor companion through the exported C tables with a substituted module
// loader: exact origin, argument/alias refusals, one editor per child, child and
// module retirement ordering, provider reentry from view callbacks, report
// mapping and editor operations while an AUDIO capability is active.
#include "RavetVst3Editor.h"
#include "editor-view-fixture.hpp"
#include "public.sdk/source/vst/hosting/module.h"
#include "pluginterfaces/vst/ivstaudioprocessor.h"
#include "pluginterfaces/vst/ivstcomponent.h"
#include <filesystem>
#include <iostream>
#include <map>
#include <thread>
#include <unistd.h>

using namespace editor_fixture;
namespace {
Script* active = nullptr;
class Component final : public IComponent, public IAudioProcessor {
public:
    explicit Component(Script& s) : s_(s) {}
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
    uint32 PLUGIN_API release() override { const auto n = --refs_; if (!n) { s_.log.emplace_back("component-destroyed"); delete this; } return n; }
    tresult PLUGIN_API initialize(FUnknown*) override { s_.log.emplace_back("component-initialize"); return kResultOk; }
    tresult PLUGIN_API terminate() override { s_.log.emplace_back("component-terminate"); return kResultOk; }
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
    Script& s_;
    uint32 refs_ = 1;
};
struct Factory final : IPluginFactory3 {
    Script* script = nullptr;
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
        std::strcpy(out->name, "Editor fixture"); return kResultOk;
    }
    tresult PLUGIN_API getClassInfo2(int32, PClassInfo2*) override { return kNotImplemented; }
    tresult PLUGIN_API getClassInfoUnicode(int32 index, PClassInfoW* out) override {
        *out = {}; out->cid[0] = char(index + 1); out->cardinality = 1;
        std::strcpy(out->category, index ? "Component Controller Class" : "Audio Module Class");
        out->name[0] = 'e'; return kResultOk;
    }
    tresult PLUGIN_API createInstance(FIDString id, FIDString, void** output) override {
        require(script);
        if (id[0] == 2) *output = static_cast<IEditController*>(new Controller(*script));
        else *output = static_cast<IComponent*>(new Component(*script));
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
const RavetVst3EditorProviderV1* editors = nullptr;
RavetVst3EditorReport report() { RavetVst3EditorReport r{}; r.struct_size = sizeof(r); r.stage = 77; return r; }
RavetVst3EditorSize extent(uint32_t w = 0, uint32_t h = 0) { return RavetVst3EditorSize{sizeof(RavetVst3EditorSize), w, h, 0}; }
void expectReport(const RavetVst3EditorReport& r, uint32_t stage, uint32_t failure, int32_t native = 0) {
    if (r.stage != stage || r.failure != failure || r.native_result != native)
        std::cerr << "report stage " << r.stage << " failure " << r.failure << " native " << r.native_result << '\n';
    require(r.struct_size == sizeof(r) && r.stage == stage && r.failure == failure && r.native_result == native && !r.reserved);
    require((failure == 2) == bool(r.flags & RAVET_VST3_EDITOR_RETURNED) && (failure == 3) == bool(r.flags & RAVET_VST3_EDITOR_THROWN));
}
struct Fixture {
    Script script;
    Factory factory;
    std::string path;
    RavetVst3Module* parent = nullptr;
    RavetVst3Instance* child = nullptr;
    explicit Fixture(const std::filesystem::path& root, const std::string& name) {
        factory.script = &script;
        const auto directory = root / (name + ".vst3");
        std::filesystem::create_directory(directory);
        path = directory.string();
        modules[std::filesystem::canonical(directory).string()] = &factory;
        active = &script;
        int32_t native = 0;
        require(audio->open(reinterpret_cast<const uint8_t*>(path.data()), uint32_t(path.size()), &parent, &native) == RAVET_VST3_OK);
        require(audio->create(parent, 0, &child, &native) == RAVET_VST3_OK);
    }
    RavetVst3Editor* open() {
        RavetVst3Editor* editor = nullptr; auto initial = extent(); auto r = report();
        require(editors->create(child, RAVET_VST3_EDITOR_PLATFORM_NSVIEW, &editor, &initial, &r) == RAVET_VST3_OK && editor);
        require(initial.width == 400 && initial.height == 300 && !initial.reserved);
        expectReport(r, 204, 0);
        return editor;
    }
    void close() {
        int32_t native = 0;
        require(audio->destroy(child, &native) == RAVET_VST3_OK); child = nullptr;
        const auto closed = audio->close(parent, &native);
        if (closed != RAVET_VST3_OK) std::cerr << "close status " << closed << " native " << native << '\n';
        require(closed == RAVET_VST3_OK); parent = nullptr;
        require(script.live == 0);
        active = nullptr;
    }
};
int parentStorage = 0;
void* const nativeParent = &parentStorage;

void tableAndArguments(const std::filesystem::path& root) {
    require(editors->struct_size == sizeof(RavetVst3EditorProviderV1) && editors->version == 1 && editors->instance_origin == audio);
    Fixture f(root, "arguments");
    RavetVst3Editor* editor = nullptr; auto initial = extent(); auto r = report();
    require(editors->create(nullptr, 1, &editor, &initial, &r) == RAVET_VST3_INVALID);
    require(editors->create(f.child, 1, nullptr, &initial, &r) == RAVET_VST3_INVALID);
    require(editors->create(f.child, 1, &editor, nullptr, &r) == RAVET_VST3_INVALID);
    require(editors->create(f.child, 1, &editor, &initial, nullptr) == RAVET_VST3_INVALID);
    auto wrong = report(); wrong.struct_size = 3;
    require(editors->create(f.child, 1, &editor, &initial, &wrong) == RAVET_VST3_INVALID && wrong.stage == 77);
    auto badSize = extent(); badSize.struct_size = 4;
    require(editors->create(f.child, 1, &editor, &badSize, &r) == RAVET_VST3_INVALID);
    // Aliased caller records and owned-object overlap refuse before any foreign call.
    require(editors->create(f.child, 1, &editor, reinterpret_cast<RavetVst3EditorSize*>(&r), &r) == RAVET_VST3_INVALID);
    require(editors->create(f.child, 1, reinterpret_cast<RavetVst3Editor**>(f.child), &initial, &r) == RAVET_VST3_INVALID);
    RavetVst3Editor* sentinel = reinterpret_cast<RavetVst3Editor*>(&parentStorage);
    require(editors->create(f.child, 1, &sentinel, &initial, &r) == RAVET_VST3_INVALID);
    for (uint32_t platform : {0u, 2u, 0xffffffffu}) {
        r = report();
        require(editors->create(f.child, platform, &editor, &initial, &r) == RAVET_VST3_INVALID && !editor);
        expectReport(r, 200, 1);
    }
    require(f.script.count("createView") == 0);
    std::thread worker([&] { auto local = report(); auto size = extent(); RavetVst3Editor* out = nullptr;
        require(editors->create(f.child, 1, &out, &size, &local) == RAVET_VST3_THREAD && !out); });
    worker.join();
    editor = f.open();
    RavetVst3Editor* second = nullptr; r = report();
    require(editors->create(f.child, 1, &second, &initial, &r) == RAVET_VST3_LIFECYCLE && !second);
    expectReport(r, 200, 9);
    require(f.script.count("createView") == 1);
    // Entry argument checks on a live editor.
    auto applied = extent();
    require(editors->attach(nullptr, nativeParent, &applied, &r) == RAVET_VST3_INVALID);
    require(editors->attach(editor, nativeParent, nullptr, &r) == RAVET_VST3_INVALID);
    require(editors->attach(editor, nativeParent, reinterpret_cast<RavetVst3EditorSize*>(editor), &r) == RAVET_VST3_INVALID);
    auto proposed = extent(500, 400); proposed.reserved = 1;
    require(editors->resize(editor, &proposed, &applied, &r) == RAVET_VST3_INVALID);
    proposed.reserved = 0;
    require(editors->resize(editor, &proposed, &proposed, &r) == RAVET_VST3_INVALID);
    RavetVst3EditorState state{}; state.struct_size = 1;
    require(editors->state(editor, &state) == RAVET_VST3_INVALID);
    std::thread off([&] { auto local = report(); auto size = extent(); RavetVst3EditorState s{}; s.struct_size = sizeof(s);
        require(editors->size(editor, &size, &local) == RAVET_VST3_THREAD);
        require(editors->state(editor, &s) == RAVET_VST3_THREAD);
        require(editors->take_request(editor, &size) == RAVET_VST3_THREAD);
        require(editors->destroy(editor, &local) == RAVET_VST3_THREAD); });
    off.join();
    r = report();
    require(editors->destroy(editor, &r) == RAVET_VST3_OK);
    expectReport(r, 212, 0);
    f.close();
}
void lifecycleAndOrdering(const std::filesystem::path& root) {
    Fixture f(root, "ordering");
    auto* editor = f.open();
    auto r = report(); auto applied = extent();
    f.script.resizeInAttached = ViewRect(0, 0, 500, 350);
    require(editors->attach(editor, nativeParent, &applied, &r) == RAVET_VST3_OK);
    expectReport(r, 205, 0);
    require(r.flags == (RAVET_VST3_EDITOR_STATE | RAVET_VST3_EDITOR_ATTACHED | RAVET_VST3_EDITOR_DEFERRED) && applied.width == 500 && applied.height == 350);
    require(f.script.lastParent == nativeParent);
    auto request = extent();
    require(editors->take_request(editor, &request) == RAVET_VST3_OK && request.width == 500 && request.height == 350);
    require(editors->take_request(editor, &request) == RAVET_VST3_EDITOR_EMPTY);
    auto proposed = extent(100, 100);
    require(editors->resize(editor, &proposed, &applied, &r) == RAVET_VST3_OK && applied.width == 200 && applied.height == 150);
    expectReport(r, 208, 0);
    require(editors->size(editor, &request, &r) == RAVET_VST3_OK && request.width == 200);
    require(editors->set_scale(editor, 2.f, &applied, &r) == RAVET_VST3_EDITOR_UNSUPPORTED);
    expectReport(r, 209, 8);
    require(f.script.current->frame(ViewRect(0, 0, 640, 480)) == kResultTrue);
    RavetVst3EditorState state{}; state.struct_size = sizeof(state);
    require(editors->state(editor, &state) == RAVET_VST3_OK && state.flags == (1u | 2u | 8u) && state.width == 640 && state.height == 480);
    require(state.frame_calls == 2 && state.frame_applied == 2 && state.frame_refused == 0);
    // The child, and through it the module, cannot retire while the view lives.
    int32_t native = 0;
    require(audio->destroy(f.child, &native) == RAVET_VST3_LIFECYCLE);
    require(audio->close(f.parent, &native) == RAVET_VST3_CHILDREN);
    require(f.script.count("controller-terminate") == 0 && f.script.live == 1);
    f.script.log.clear();
    r = report();
    require(editors->destroy(editor, &r) == RAVET_VST3_OK);
    expectReport(r, 212, 0);
    require(f.script.log == (std::vector<std::string>{"removed", "clearFrame", "view-destroyed"}));
    require(audio->destroy(f.child, &native) == RAVET_VST3_OK); f.child = nullptr;
    require(audio->close(f.parent, &native) == RAVET_VST3_OK); f.parent = nullptr;
    // Complete order: view, handler, controller, component, module.
    const auto& log = f.script.log;
    require(f.script.index("view-destroyed") < f.script.index("handler-clear"));
    require(f.script.index("handler-clear") < f.script.index("controller-terminate"));
    require(f.script.index("controller-terminate") < f.script.index("component-terminate"));
    require(f.script.index("component-terminate") < f.script.index("module-exit"));
    require(log.back() == "module-exit" && f.script.live == 0);
    active = nullptr;
}
void reportsAndFaults(const std::filesystem::path& root) {
    {   Fixture f(root, "unavailable"); f.script.create = Script::Create::Null;
        RavetVst3Editor* editor = nullptr; auto initial = extent(); auto r = report();
        require(editors->create(f.child, 1, &editor, &initial, &r) == RAVET_VST3_EDITOR_UNAVAILABLE && !editor);
        expectReport(r, 201, 7); require(initial.width == 0);
        f.script.create = Script::Create::Throw; r = report();
        require(editors->create(f.child, 1, &editor, &initial, &r) == RAVET_VST3_NATIVE && !editor);
        expectReport(r, 201, 3);
        f.script.create = Script::Create::View; f.script.platform = false; r = report();
        require(editors->create(f.child, 1, &editor, &initial, &r) == RAVET_VST3_EDITOR_UNSUPPORTED && !editor);
        expectReport(r, 202, 8);
        f.script.platform = true; f.script.size = ViewRect(0, 0, 0, 0); r = report();
        require(editors->create(f.child, 1, &editor, &initial, &r) == RAVET_VST3_METADATA && !editor);
        expectReport(r, 204, 6);
        f.script.size = ViewRect(0, 0, 400, 300); f.script.setFrameResult = rejected; r = report();
        require(editors->create(f.child, 1, &editor, &initial, &r) == RAVET_VST3_NATIVE && !editor);
        expectReport(r, 203, 2, rejected);
        f.script.setFrameResult = kResultOk;
        require(f.script.live == 0);
        editor = f.open();
        f.script.attachedResult = rejected; r = report(); auto applied = extent();
        require(editors->attach(editor, nativeParent, &applied, &r) == RAVET_VST3_NATIVE);
        expectReport(r, 205, 2, rejected); require(!(r.flags & RAVET_VST3_EDITOR_ATTACHED));
        f.script.attachedResult = kResultOk;
        require(editors->attach(editor, nativeParent, &applied, &r) == RAVET_VST3_OK);
        require(editors->attach(editor, nativeParent, &applied, &r) == RAVET_VST3_LIFECYCLE);
        expectReport(r, 205, 9);
        // A refused second attach reports the true (attached) state.
        require((r.flags & (RAVET_VST3_EDITOR_STATE | RAVET_VST3_EDITOR_ATTACHED)) == (RAVET_VST3_EDITOR_STATE | RAVET_VST3_EDITOR_ATTACHED));
        f.script.removedResult = rejected;
        require(editors->detach(editor, &r) == RAVET_VST3_NATIVE);
        expectReport(r, 210, 2, rejected); require(r.flags & RAVET_VST3_EDITOR_ATTACHED);
        f.script.throwRemoved = true; f.script.removedResult = kResultOk;
        r = report();
        // Destroy consumes even when the attached view throws during removal.
        require(editors->destroy(editor, &r) == RAVET_VST3_RETIRED_NATIVE);
        expectReport(r, 210, 3);
        require(f.script.live == 0);
        f.close(); }
}
void providerReentry(const std::filesystem::path& root) {
    Fixture f(root, "reentry");
    auto* editor = f.open();
    std::vector<int32_t> nested;
    f.script.hook = [&](const std::string& name) {
        if (name != "onSize") return;
        auto r = report(); auto size = extent(); int32_t native = 0;
        nested.push_back(editors->size(editor, &size, &r));
        nested.push_back(editors->destroy(editor, &r));
        nested.push_back(editors->detach(editor, &r));
        nested.push_back(audio->destroy(f.child, &native));
        RavetVst3AudioProfile profile{sizeof(profile), 64, 2, 2, 48000};
        nested.push_back(audio->prepare(f.child, &profile, &native));
        RavetVst3Instance* sibling = nullptr;
        nested.push_back(audio->create(f.parent, 0, &sibling, &native));
        RavetVst3EditorState state{}; state.struct_size = sizeof(state);
        nested.push_back(editors->state(editor, &state)); // pure copied read stays available
    };
    auto r = report(); auto proposed = extent(640, 480); auto applied = extent();
    require(editors->attach(editor, nativeParent, &applied, &r) == RAVET_VST3_OK);
    require(editors->resize(editor, &proposed, &applied, &r) == RAVET_VST3_OK && applied.width == 640);
    require(nested == (std::vector<int32_t>{RAVET_VST3_EDITOR_REENTRY, RAVET_VST3_EDITOR_REENTRY, RAVET_VST3_EDITOR_REENTRY,
                                           RAVET_VST3_LIFECYCLE, RAVET_VST3_LIFECYCLE, RAVET_VST3_LIFECYCLE, RAVET_VST3_OK}));
    // A plug-in UI event: frame resize applies onSize in the same call stack, and
    // nested provider calls from that onSize are refused the same way.
    nested.clear();
    require(f.script.current->frame(ViewRect(0, 0, 700, 500)) == kResultTrue);
    require(nested == (std::vector<int32_t>{RAVET_VST3_EDITOR_REENTRY, RAVET_VST3_EDITOR_REENTRY, RAVET_VST3_EDITOR_REENTRY,
                                           RAVET_VST3_LIFECYCLE, RAVET_VST3_LIFECYCLE, RAVET_VST3_LIFECYCLE, RAVET_VST3_OK}));
    // The idle resize still applied once; its nested calls were refused by the gate.
    RavetVst3EditorState state{}; state.struct_size = sizeof(state);
    require(editors->state(editor, &state) == RAVET_VST3_OK && state.width == 700 && state.frame_applied == 1);
    f.script.hook = nullptr;
    require(editors->destroy(editor, &r) == RAVET_VST3_OK);
    f.close();
}
void duringAudio(const std::filesystem::path& root) {
    Fixture f(root, "audio");
    int32_t native = 0;
    RavetVst3AudioProfile profile{sizeof(profile), 64, 2, 2, 48000};
    require(audio->prepare(f.child, &profile, &native) == RAVET_VST3_OK);
    require(audio->start(f.child, &native) == RAVET_VST3_OK);
    RavetVst3Audio* lease = nullptr;
    require(audio->begin_audio(f.child, &lease, &native) == RAVET_VST3_OK && lease);
    // Editor entries do not take the live-core CONTROL admission.
    for (int cycle = 0; cycle < 3; ++cycle) {
        f.script.size = ViewRect(0, 0, 400, 300);
        auto* editor = f.open();
        auto r = report(); auto applied = extent(); auto proposed = extent(800, 600);
        require(editors->attach(editor, nativeParent, &applied, &r) == RAVET_VST3_OK);
        require(editors->resize(editor, &proposed, &applied, &r) == RAVET_VST3_OK && applied.width == 800);
        require(editors->detach(editor, &r) == RAVET_VST3_OK);
        require(audio->destroy(f.child, &native) == RAVET_VST3_LIFECYCLE);
        require(editors->destroy(editor, &r) == RAVET_VST3_OK);
    }
    require(audio->end_audio(lease, &native) == RAVET_VST3_OK);
    require(audio->stop(f.child, &native) == RAVET_VST3_OK);
    f.close();
}
} // namespace
int main() {
    try {
        require(pthread_main_np());
        audio = ravet_vst3_provider_v2(); editors = ravet_vst3_editor_provider_v1();
        char directory[] = "/tmp/ravet-vst3-editor-XXXXXX";
        require(mkdtemp(directory));
        const std::filesystem::path root(directory);
        tableAndArguments(root); lifecycleAndOrdering(root); reportsAndFaults(root); providerReentry(root); duringAudio(root);
        std::filesystem::remove_all(root);
        std::cout << "PASS editor provider: exact origin, argument/alias/thread refusals, one editor per child, child "
                     "LIFECYCLE and module CHILDREN while open, view->handler->controller->component->module order, "
                     "report mapping, provider reentry refusal from view callbacks and editors during active AUDIO\n";
    } catch (const std::exception& error) {
        std::cerr << error.what() << '\n';
        return 1;
    }
}
