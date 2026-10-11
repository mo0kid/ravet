#include "RavetVst3Provider.h"
#include "public.sdk/source/vst/hosting/module.h"
#include "pluginterfaces/vst/ivsthostapplication.h"
#include <algorithm>
#include <cstring>
#include <iostream>
#include <stdexcept>
#include <string>
#include <vector>
// Authored protocol fixture: real public SDK interfaces and provider, controlled
// pointer-valid factory and Module::create substitute. SDK loader implementation
// and actual modules are tested separately; this is not native code isolation.
using namespace Steinberg;
namespace {
void require(bool value) { if (!value) throw std::runtime_error("context contract failed"); }
struct State {
    int mode = 0, refs = 1, installed = 0, cleared = 0, probes = 0, entries = 0, exits = 0;
    FUnknown* retained = nullptr;
    FUnknown* borrowedHost = nullptr;
    bool moduleLive = false;
    std::vector<std::string> order;
    void probe() {
        if (!borrowedHost) return;
        void* object = nullptr;
        require(borrowedHost->queryInterface(Vst::IHostApplication::iid, &object) == kResultOk && object);
        static_cast<Vst::IHostApplication*>(object)->release();
        ++probes;
    }
};
State* state = nullptr;
struct Factory final : IPluginFactory3 {
    tresult PLUGIN_API queryInterface(const TUID iid, void** object) override {
        *object = nullptr;
        const bool version3 = FUnknownPrivate::iidEqual(iid, IPluginFactory3::iid);
        const bool version2 = FUnknownPrivate::iidEqual(iid, IPluginFactory2::iid);
        if (state->mode == 4 && (version3 || version2)) return kNoInterface;
        if (version3 || version2 || FUnknownPrivate::iidEqual(iid, IPluginFactory::iid) || FUnknownPrivate::iidEqual(iid, FUnknown::iid)) {
            *object = static_cast<IPluginFactory3*>(this); addRef();
            if (state->mode == 5 && version2) return kInternalError;
            if (state->mode == 8 && version2) throw std::runtime_error("query after retained factory output");
            return kResultOk;
        }
        return kNoInterface;
    }
    uint32 PLUGIN_API addRef() override { return ++state->refs; }
    uint32 PLUGIN_API release() override {
        if (state->moduleLive) { state->probe(); state->order.push_back("factory-release"); }
        return --state->refs;
    }
    int32 PLUGIN_API countClasses() override { return 1; }
    tresult PLUGIN_API getFactoryInfo(PFactoryInfo* info) override { *info = {}; std::strcpy(info->vendor,"RAVET"); return kResultOk; }
    tresult PLUGIN_API getClassInfo(int32, PClassInfo* info) override {
        *info = {}; info->cid[0] = 1; info->cardinality = 1;
        std::strcpy(info->category,"Audio Module Class"); std::strcpy(info->name,"Context fixture");
        return kResultOk;
    }
    tresult PLUGIN_API getClassInfo2(int32, PClassInfo2*) override { return kNotImplemented; }
    tresult PLUGIN_API getClassInfoUnicode(int32, PClassInfoW* info) override {
        *info = {}; info->cid[0] = 1; info->cardinality = 1; std::strcpy(info->category,"Audio Module Class");
        if (state->mode != 3) info->name[0] = 'x';
        return kResultOk;
    }
    tresult PLUGIN_API createInstance(FIDString, FIDString, void** object) override { *object = nullptr; return kNotImplemented; }
    tresult PLUGIN_API setHostContext(FUnknown* host) override {
        if (host) {
            state->retained = host; host->addRef(); state->borrowedHost = host;
            ++state->installed; state->probe(); state->order.push_back("install");
            if (state->mode == 6) throw std::runtime_error("fixture context installation");
            return state->mode == 1 ? kInternalError : kNotImplemented;
        }
        ++state->cleared; state->probe(); state->order.push_back("clear");
        if (state->retained) { state->retained->release(); state->retained = nullptr; }
        if (state->mode == 7) throw std::runtime_error("fixture context clearing");
        return state->mode == 2 ? kInternalError : kNotImplemented;
    }
};
Factory fixtureFactory;
struct FakeModule final : VST3::Hosting::Module {
    FakeModule() { ++state->entries; state->moduleLive = true; fixtureFactory.addRef(); factory = VST3::Hosting::PluginFactory(owned<IPluginFactory>(&fixtureFactory)); }
    ~FakeModule() noexcept override {
        factory = VST3::Hosting::PluginFactory(nullptr);
        try { state->probe(); state->order.push_back("module-exit"); }
        catch (...) { std::terminate(); }
        ++state->exits; state->moduleLive = false;
    }
    bool load(const std::string&, std::string&) override { return true; }
};
}
namespace VST3::Hosting {
Module::Ptr Module::create(const std::string&, std::string&) { return std::make_shared<FakeModule>(); }
}
int main() {
    try {
        const auto* provider = ravet_vst3_provider_v1();
        const std::string path = "explicit-context-fixture.vst3";
        for (int mode = 0; mode <= 8; ++mode) {
            State observation; observation.mode = mode; state = &observation;
            RavetVst3Module* owner = nullptr; int32_t native = 123;
            const auto status = provider->open(reinterpret_cast<const uint8_t*>(path.data()), path.size(), &owner, &native);
            if (mode == 1 || mode == 3 || mode == 5 || mode == 6 || mode == 8) {
                require(!owner && status == (mode == 3 ? RAVET_VST3_METADATA : RAVET_VST3_NATIVE));
                if (mode != 3) {
                    if (native != kInternalError) std::cerr << "mode " << mode << " unexpected native result " << native << '\n';
                    require(native == kInternalError);
                }
            } else {
                require(status == RAVET_VST3_OK && owner);
                RavetVst3Module* duplicate = nullptr; int32_t duplicateNative = 123;
                require(provider->open(reinterpret_cast<const uint8_t*>(path.data()), path.size(), &duplicate, &duplicateNative) == RAVET_VST3_CAPACITY);
                require(!duplicate && observation.entries == 1 && observation.exits == 0 && observation.moduleLive);
                RavetVst3FactoryRecord metadata{}; metadata.struct_size = sizeof(metadata);
                require(provider->factory(owner,&metadata) == RAVET_VST3_OK);
                require(metadata.host_context_result == (mode == 4 ? kNoInterface : kNotImplemented));
                const auto closed = provider->close(owner,&native);
                require(closed == (mode == 2 || mode == 7 ? RAVET_VST3_RETIRED_NATIVE : RAVET_VST3_OK));
                require(native == (mode == 2 || mode == 7 ? kInternalError : mode == 4 ? kResultOk : kNotImplemented));
            }
            if (observation.refs != 1) std::cerr << "mode " << mode << " retained factory refs " << observation.refs << " expected1\n";
            require(observation.refs == 1 && !observation.moduleLive && !observation.retained);
            require(observation.entries == 1 && observation.exits == 1);
            require(observation.installed == (mode == 4 || mode == 5 || mode == 8 ? 0 : 1));
            require(observation.cleared == observation.installed);
            require(!observation.order.empty() && observation.order.back() == "module-exit");
            if (observation.installed) {
                const auto clear = std::find(observation.order.begin(),observation.order.end(),"clear");
                const auto exit = std::find(observation.order.begin(),observation.order.end(),"module-exit");
                require(clear != observation.order.end() && clear < exit && observation.probes >= 4);
            }
        }
        state = nullptr;
        std::cout << "PASS provider context optional/failure outcomes, exact cleanup and host lifetime through module termination\n";
    } catch (const std::exception& error) { std::cerr << error.what() << '\n'; return 1; }
}
