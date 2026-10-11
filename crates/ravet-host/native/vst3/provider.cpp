#include "RavetVst3Provider.h"
#include "RavetVst3Catalogue.hpp"
#include "RavetVst3Instance.hpp"
#include "RavetVst3State.h"
#include "RavetVst3Parameters.h"
#include "RavetVst3Editor.h"
#include "RavetVst3EditorOwner.hpp"
#include "ProcessingPoolKeepers.hpp"
#include "public.sdk/source/vst/hosting/module.h"
#include "public.sdk/source/vst/hosting/hostclasses.h"
#include <memory>
#include "RavetVst3CoreRegistry.hpp"
#include <thread>
#include <filesystem>
#include <initializer_list>

namespace {
struct NativeCore;
void publishProtection(NativeCore&,const RavetVst3Module*) noexcept;
using Registry = ravet::vst3::draft::CoreRegistry<NativeCore>;
// Deliberate process-lifetime bookkeeping. Future non-discardable instance pins
// survive ordinary facade retirement; this registry has no static destructor.
Registry& registry() { static auto* instance = new Registry; return *instance; }
struct NativeCore:std::enable_shared_from_this<NativeCore> {
    const std::string key;
    explicit NativeCore(std::string canonicalPath):key(std::move(canonicalPath)){}
    const std::string& identityKey()const noexcept{return key;}
    Steinberg::IPtr<Steinberg::Vst::HostApplication> host = Steinberg::owned(new Steinberg::Vst::HostApplication);
    VST3::Hosting::Module::Ptr module;
    Steinberg::IPtr<Steinberg::IPluginFactory3> factory3;
    ravet::vst3::Catalogue catalogue;
    ravet::vst3::hosted_audio::ScratchBudget scratchBudget;
    ravet::vst3::parameters_draft::ImmutableCoreProtection immutableProtection;
    ravet::vst3::parameters_draft::LiveCoreProtection liveProtection;
    std::array<RavetVst3Instance*,256> childRegistry{};
    std::array<RavetVst3Audio*,256> audioRegistry{};
    const RavetVst3Module* activeFacade=nullptr; // Numeric extent only; never read facade fields from escaped SDK.
    static void refreshState(void* opaque)noexcept {
        auto& origin=*static_cast<NativeCore*>(opaque);
        publishProtection(origin,origin.activeFacade);
    }
    static void pinHandler(void* opaque){
        auto& origin=*static_cast<NativeCore*>(opaque);
        if(registry().pin(origin.reservation,origin.shared_from_this())!=Registry::Result::Accepted)std::terminate();
    }
    ravet::vst3::draft::ControllerHandlerKeepers handlerKeepers{&immutableProtection,&liveProtection,this,&NativeCore::refreshState,&NativeCore::pinHandler};
    bool contextAttempted = false;
    Steinberg::tresult contextResult = Steinberg::kNoInterface;
    Steinberg::IPluginFactory* reservation = nullptr;
    std::unique_ptr<ravet::vst3::state_draft::StateStreamKeepers> stateKeepers;
    std::unique_ptr<ravet::vst3::parameters_draft::ParameterPoolKeepers> parameterKeepers;
    std::unique_ptr<ravet::vst3::processing_draft::ProcessingPoolKeepers<NativeCore>> processingKeepers;
    auto& processingPools() {
        if(!processingKeepers)processingKeepers=std::make_unique<ravet::vst3::processing_draft::ProcessingPoolKeepers<NativeCore>>(*this,&immutableProtection,&liveProtection);
        return *processingKeepers;
    }
    auto& streams() {
        if(!stateKeepers)stateKeepers=std::make_unique<ravet::vst3::state_draft::StateStreamKeepers>(&immutableProtection,&liveProtection,this,&NativeCore::refreshState);
        return *stateKeepers;
    }
    auto& parameterPools() {
        if(!parameterKeepers)parameterKeepers=std::make_unique<ravet::vst3::parameters_draft::ParameterPoolKeepers>(&immutableProtection,&liveProtection);
        return *parameterKeepers;
    }
    Steinberg::tresult retire(bool* retained = nullptr) noexcept {
        bool pinned = false;
        if (reservation && registry().beginRetire(reservation, this, pinned) != Registry::Result::Accepted)
            std::terminate(); // private owner/reservation invariant
        if (retained) *retained = pinned;
        if (pinned) {
            if (registry().finishRetire(reservation, this) != Registry::Result::Accepted) std::terminate();
            return Steinberg::kResultOk; // v2 must report explicit process-pin outcome
        }
        Steinberg::tresult result = Steinberg::kResultOk;
        if (factory3 && contextAttempted) {
            contextAttempted = false;
            try { result = factory3->setHostContext(nullptr); }
            catch (...) { result = Steinberg::kInternalError; }
        }
        // Identity remains reserved throughout foreign release and moduleExit.
        // Host survives both, including failed setters retaining a borrowed host.
        factory3 = nullptr;
        stateKeepers.reset(); // ordinary core has no foreign references; CONTROL
        parameterKeepers.reset();
        processingKeepers.reset();
        module.reset();
        if (reservation) {
            if (registry().finishRetire(reservation, this) != Registry::Result::Accepted) std::terminate();
            reservation = nullptr;
        }
        return result;
    }
    ~NativeCore() { (void)retire(); }
};
}
struct RavetVst3Module {
    std::thread::id thread = std::this_thread::get_id();
    std::shared_ptr<NativeCore> core;
    uint32_t children = 0;
    bool version2 = false;
    bool controlBusy = false;
    uint32_t audioChildren = 0; // MAIN only; protects shared keeper geometry
};
struct RavetVst3Instance {
    RavetVst3Module* parent = nullptr;
    std::unique_ptr<ravet::vst3::draft::Instance> owner;
    RavetVst3Editor* editor = nullptr; // MAIN only; at most one live view per child
};
struct RavetVst3Editor {
    RavetVst3Instance* instance = nullptr;
    std::unique_ptr<ravet::vst3::editor_draft::Owner> owner;
};
struct RavetVst3Audio { RavetVst3Instance* instance = nullptr; };
namespace {
// Exact maximum: core + facade + 256*(wrapper + 41 owned ranges) +
// 256 audio wrappers + (state keeper + 16*(object + payload)) +
// (parameter keeper + 16 pools) + (processing keeper + 16 pools) +
// 256 keeper-owned handler ranges (attached duplicates counted) = 11333.
static constexpr size_t MaximumLiveRanges=256+2+256*(1+ravet::vst3::draft::Instance::MaximumOwnedRanges)+256+33+17+17;
static_assert(MaximumLiveRanges==11333);
static_assert(MaximumLiveRanges<=ravet::vst3::parameters_draft::LiveCoreProtection::MaximumRanges);
void publishProtection(NativeCore& core,const RavetVst3Module* facade) noexcept {
    auto& protection=core.liveProtection;protection.clear();core.activeFacade=facade;
    auto append=[&](const void* pointer,size_t bytes,size_t alignment){return protection.append(pointer,bytes,1,alignment);};
    bool valid=append(&core,sizeof(core),alignof(NativeCore));
    if(facade)valid=valid&&append(facade,sizeof(*facade),alignof(RavetVst3Module));
    for(auto* child:core.childRegistry)if(child)valid=valid&&append(child,sizeof(*child),alignof(RavetVst3Instance))&&child->owner->appendOwnedRanges(append);
    for(auto* audio:core.audioRegistry)if(audio)valid=valid&&append(audio,sizeof(*audio),alignof(RavetVst3Audio));
    if(core.stateKeepers)valid=valid&&core.stateKeepers->appendProtectionRanges(append);
    if(core.parameterKeepers)valid=valid&&core.parameterKeepers->appendProtectionRanges(append);
    if(core.processingKeepers)valid=valid&&core.processingKeepers->appendProtectionRanges(append);
    valid=valid&&core.handlerKeepers.appendProtectionRanges(append);
    if(!valid)std::terminate(); // fixed registry bounds imply capacity; arithmetic is owned storage
}
template<class Pointer> bool registryInsert(std::array<Pointer*,256>& entries,Pointer* pointer)noexcept {
    for(auto& entry:entries)if(!entry){entry=pointer;return true;}return false;
}
template<class Pointer> void registryRemove(std::array<Pointer*,256>& entries,Pointer* pointer)noexcept {
    for(auto& entry:entries)if(entry==pointer){entry=nullptr;return;}std::terminate();
}
bool facadeExternal(const RavetVst3Module& parent,const void* p,size_t bytes,size_t alignment)noexcept {
    ravet::vst3::parameters_draft::Range value{};
    return ravet::vst3::parameters_draft::range(p,bytes,1,alignment,value)&&
        parent.core->immutableProtection.allows(value)&&parent.core->liveProtection.allows(value);
}
template<class Interface> Steinberg::IPtr<Interface> query(Steinberg::IPluginFactory& factory) {
    Interface* object = nullptr;
    Steinberg::tresult result;
    try { result = factory.queryInterface(Interface::iid, reinterpret_cast<void**>(&object)); }
    catch (...) { result = Steinberg::kInternalError; }
    if (result == Steinberg::kNoInterface && !object) return nullptr;
    auto retained = Steinberg::owned(object);
    ravet::vst3::check(result);
    if (!object) throw ravet::vst3::Failure{ravet::vst3::Error::Metadata};
    return retained;
}
template<std::size_t N> void copy(char (&destination)[N], const std::string& source) {
    if (source.size() >= N) throw ravet::vst3::Failure{ravet::vst3::Error::Metadata};
    std::memcpy(destination, source.data(), source.size());
}
int32_t openCommon(const uint8_t* bytes, uint32_t length, RavetVst3Module** output, int32_t* native, bool version2) noexcept {
    if (version2 && !pthread_main_np()) return RAVET_VST3_THREAD;
    if (!bytes || !output || !native || *output || length == 0 || length > 4096) return RAVET_VST3_INVALID;
    try {
        std::string path(reinterpret_cast<const char*>(bytes), length);
        if (path.find('\0') != std::string::npos || !ravet::vst3::utf8(path)) return RAVET_VST3_INVALID;
        std::error_code pathError;
        const auto canonicalPath = std::filesystem::canonical(path,pathError);
        const std::string key = pathError ? path : canonicalPath.string();
        if (key.empty() || key.size()>4096 || !ravet::vst3::utf8(key)) return RAVET_VST3_INVALID;
        auto candidate = std::make_unique<RavetVst3Module>();
        candidate->version2 = version2;
        std::shared_ptr<NativeCore> known;
        const auto lookup = registry().reopenKey(key,version2,known);
        if (lookup == Registry::Result::Accepted) {
            candidate->core = std::move(known);
            if(!candidate->core->liveProtection.beginControl()){
                bool pinned=false;
                if(registry().beginRetire(candidate->core->reservation,candidate->core,pinned)!=Registry::Result::Accepted||!pinned||
                   registry().finishRetire(candidate->core->reservation,candidate->core)!=Registry::Result::Accepted)std::terminate();
                return RAVET_VST3_LIFECYCLE;
            }
            publishProtection(*candidate->core,candidate.get());candidate->core->liveProtection.endControl();
            *native = Steinberg::kResultOk; *output = candidate.release(); return RAVET_VST3_OK;
        }
        if (lookup != Registry::Result::NotFound) throw ravet::vst3::Failure{ravet::vst3::Error::Capacity};
        candidate->core = std::make_shared<NativeCore>(key);
        auto& core = *candidate->core;
        std::string diagnostic;
        core.module = VST3::Hosting::Module::create(path, diagnostic);
        if (!core.module) { *native = Steinberg::kInternalError; return RAVET_VST3_NATIVE; }
        const auto& factory = core.module->getFactory().get();
        if (!factory) return RAVET_VST3_METADATA;
        // Modules may return the same process-global singleton factory. Two
        // independent contexts would overwrite/clear each other's live context.
        // Reserve before foreign setters and retain through native termination.
        if (registry().reserve(factory, candidate->core) != Registry::Result::Accepted)
            throw ravet::vst3::Failure{ravet::vst3::Error::Capacity};
        core.reservation = factory;
        // Retain interfaces only while their containing module remains loaded.
        core.factory3 = query<Steinberg::IPluginFactory3>(*factory);
        auto factory2 = query<Steinberg::IPluginFactory2>(*factory);
        if (core.factory3) {
            core.contextAttempted = true;
            core.contextResult = core.factory3->setHostContext(core.host);
            // The SDK's macOS CPluginFactory invokes its registered callbacks
            // and then returns kNotImplemented. Context delivery is optional;
            // preserve this exact status rather than claiming successful setup.
            if (core.contextResult != Steinberg::kNotImplemented)
                ravet::vst3::check(core.contextResult);
        }
        core.catalogue = ravet::vst3::inspect(*factory, factory2, core.factory3);
        if(!core.immutableProtection.seal(&core,sizeof(core),alignof(NativeCore),core.key,core.catalogue))
            throw ravet::vst3::Failure{ravet::vst3::Error::Capacity};
        if(!core.liveProtection.beginControl())return RAVET_VST3_LIFECYCLE;
        publishProtection(core,candidate.get());core.liveProtection.endControl();
        *native = Steinberg::kResultOk;
        *output = candidate.release();
        return RAVET_VST3_OK;
    } catch (const ravet::vst3::Failure& failure) {
        *native = failure.native;
        switch (failure.error) {
            case ravet::vst3::Error::Capacity: return RAVET_VST3_CAPACITY;
            case ravet::vst3::Error::Metadata: return RAVET_VST3_METADATA;
            case ravet::vst3::Error::Native: return RAVET_VST3_NATIVE;
        }
    } catch (const std::bad_alloc&) { return RAVET_VST3_ALLOCATION; }
    catch (...) { *native = Steinberg::kInternalError; return RAVET_VST3_NATIVE; }
    return RAVET_VST3_NATIVE;
}
int32_t open(const uint8_t* b,uint32_t n,RavetVst3Module** o,int32_t* e) noexcept {return openCommon(b,n,o,e,false);}
int32_t openV2(const uint8_t* b,uint32_t n,RavetVst3Module** o,int32_t* e) noexcept {return openCommon(b,n,o,e,true);}
int32_t factory(const RavetVst3Module* owner, RavetVst3FactoryRecord* output) noexcept {
    if (!owner || !output) return RAVET_VST3_INVALID;
    if (owner->thread != std::this_thread::get_id()) return RAVET_VST3_THREAD;
    ravet::vst3::parameters_draft::LiveCoreProtection::Read admission(&owner->core->liveProtection);
    ravet::vst3::parameters_draft::Range requested{};
    if(!admission.admitted(&owner->core->liveProtection))return RAVET_VST3_LIFECYCLE;
    if(!ravet::vst3::parameters_draft::range(output,1,sizeof(*output),alignof(std::remove_reference_t<decltype(*output)>),requested)||
       !owner->core->liveProtection.allows(requested)||!owner->core->immutableProtection.allows(requested)||output->struct_size!=sizeof(*output))return RAVET_VST3_INVALID;
    try {
        RavetVst3FactoryRecord candidate{};
        candidate.struct_size = sizeof(candidate); candidate.flags = owner->core->catalogue.flags;
        candidate.class_count = static_cast<uint32_t>(owner->core->catalogue.classes.size());
        candidate.host_context_result = owner->core->contextResult;
        copy(candidate.vendor, owner->core->catalogue.vendor); copy(candidate.url, owner->core->catalogue.url); copy(candidate.email, owner->core->catalogue.email);
        *output = candidate;
        return RAVET_VST3_OK;
    } catch (...) { return RAVET_VST3_METADATA; }
}
int32_t classInfo(const RavetVst3Module* owner, uint32_t index, RavetVst3ClassRecord* output) noexcept {
    if (!owner || !output) return RAVET_VST3_INVALID;
    if (owner->thread != std::this_thread::get_id()) return RAVET_VST3_THREAD;
    if (index >= owner->core->catalogue.classes.size()) return RAVET_VST3_INVALID;
    ravet::vst3::parameters_draft::LiveCoreProtection::Read admission(&owner->core->liveProtection);
    ravet::vst3::parameters_draft::Range requested{};
    if(!admission.admitted(&owner->core->liveProtection))return RAVET_VST3_LIFECYCLE;
    if(!ravet::vst3::parameters_draft::range(output,1,sizeof(*output),alignof(std::remove_reference_t<decltype(*output)>),requested)||
       !owner->core->liveProtection.allows(requested)||!owner->core->immutableProtection.allows(requested)||output->struct_size!=sizeof(*output))return RAVET_VST3_INVALID;
    try {
        const auto& item = owner->core->catalogue.classes[index];
        RavetVst3ClassRecord candidate{};
        candidate.struct_size = sizeof(candidate); std::memcpy(candidate.id, item.id.data(), item.id.size());
        candidate.cardinality = item.cardinality; candidate.class_flags = item.flags; candidate.vendor_from_factory = item.vendorFromFactory;
        copy(candidate.category, item.category); copy(candidate.name, item.name); copy(candidate.vendor, item.vendor);
        copy(candidate.version, item.version); copy(candidate.sdk_version, item.sdkVersion); copy(candidate.subcategories, item.subcategories);
        *output = candidate;
        return RAVET_VST3_OK;
    } catch (...) { return RAVET_VST3_METADATA; }
}
int32_t close(RavetVst3Module* owner, int32_t* native) noexcept {
    if (!owner || !native || owner->version2) return RAVET_VST3_INVALID;
    if (owner->thread != std::this_thread::get_id()) return RAVET_VST3_THREAD;
    if(!owner->core->liveProtection.beginControl())return RAVET_VST3_LIFECYCLE;
    if(!facadeExternal(*owner,native,sizeof(*native),alignof(int32_t))){owner->core->liveProtection.endControl();return RAVET_VST3_INVALID;}
    const auto result = owner->core->retire();
    publishProtection(*owner->core,nullptr);owner->core->liveProtection.endControl();
    delete owner;
    *native = result;
    return result == Steinberg::kResultOk || result == Steinberg::kNotImplemented ? RAVET_VST3_OK : RAVET_VST3_RETIRED_NATIVE;
}
bool control(const RavetVst3Module* parent) noexcept {
    return parent && parent->thread==std::this_thread::get_id() && pthread_main_np();
}
void retainProcessingCore(RavetVst3Module& parent) noexcept {
    const auto& keepers=parent.core->parameterKeepers;
    if(keepers&&keepers->observedRetention()){
        if(registry().pin(parent.core->reservation,parent.core)!=Registry::Result::Accepted)std::terminate();
    }
    const auto& processing=parent.core->processingKeepers;
    if(processing) {
        try {
            processing->pinBeforeForeignTeardown(*parent.core,[&](NativeCore& origin){
                if(&origin!=parent.core.get()||registry().pin(origin.reservation,parent.core)!=Registry::Result::Accepted)
                    std::terminate();
            });
        } catch (...) {std::terminate();} // private exact CONTROL/core invariant
    }
}
struct ControlScope {
    RavetVst3Module& parent;bool acquired;
    explicit ControlScope(RavetVst3Module& p,bool audioBoundary=false):parent(p),
        acquired(!p.controlBusy&&(audioBoundary||!p.audioChildren)&&p.core->liveProtection.beginControl()){
        if(acquired){
            parent.controlBusy=true;
            // Sticky retention pins the actual image before ANY subsequent
            // foreign CONTROL call, including stop, reprepare and sibling create.
            retainProcessingCore(parent);
            parent.core->handlerKeepers.reclaim();
        }
    }
    ~ControlScope(){if(acquired){publishProtection(*parent.core,&parent);parent.controlBusy=false;parent.core->liveProtection.endControl();}}
};
template<class Call> int32_t invoke(int32_t* native,Call call) noexcept {
    if(!native)return RAVET_VST3_INVALID;
    try {call();*native=Steinberg::kResultOk;return RAVET_VST3_OK;}
    catch(const ravet::vst3::draft::InstanceFailure& e){
        *native=e.origin==ravet::vst3::draft::FailureOrigin::Thrown?Steinberg::kInternalError:e.native;
        using ravet::vst3::draft::Stage;
        return e.stage==Stage::Thread?RAVET_VST3_THREAD:e.stage==Stage::Lifecycle?RAVET_VST3_LIFECYCLE:RAVET_VST3_NATIVE;
    }catch(const std::bad_alloc&){return RAVET_VST3_ALLOCATION;}
    catch(...){*native=Steinberg::kInternalError;return RAVET_VST3_NATIVE;}
}
int32_t closeV2(RavetVst3Module* parent,int32_t* native) noexcept {
    if(!parent||!native||!parent->version2)return RAVET_VST3_INVALID;
    if(!control(parent))return RAVET_VST3_THREAD;
    if(parent->controlBusy)return RAVET_VST3_LIFECYCLE;
    if(parent->children)return RAVET_VST3_CHILDREN;
    if(!parent->core->liveProtection.beginControl())return RAVET_VST3_LIFECYCLE;
    if(!facadeExternal(*parent,native,sizeof(*native),alignof(int32_t))){parent->core->liveProtection.endControl();return RAVET_VST3_INVALID;}
    parent->controlBusy=true; // stays guarded through foreign cleanup and deletion
    retainProcessingCore(*parent);
    bool pinned=false;const auto result=parent->core->retire(&pinned);
    publishProtection(*parent->core,nullptr);parent->core->liveProtection.endControl();
    delete parent;*native=result;
    if(pinned)return RAVET_VST3_PROCESS_PINNED;
    return result==Steinberg::kResultOk||result==Steinberg::kNotImplemented?RAVET_VST3_OK:RAVET_VST3_RETIRED_NATIVE;
}
int32_t createInstance(RavetVst3Module* parent,uint32_t index,RavetVst3Instance** output,int32_t* native) noexcept {
    if(!parent||!parent->version2||!output||*output||!native)return RAVET_VST3_INVALID;
    if(!control(parent))return RAVET_VST3_THREAD;
    ControlScope scope(*parent);if(!scope.acquired)return RAVET_VST3_LIFECYCLE;
    if(parent->children>=256||!parent->core->handlerKeepers.available()||std::find(parent->core->childRegistry.begin(),parent->core->childRegistry.end(),nullptr)==parent->core->childRegistry.end())return RAVET_VST3_CAPACITY;
    const auto& catalogue=parent->core->catalogue;
    if(index>=catalogue.classes.size()||catalogue.classes[index].category!="Audio Module Class")return RAVET_VST3_INVALID;
    if(!facadeExternal(*parent,output,sizeof(*output),alignof(RavetVst3Instance*))||
       !facadeExternal(*parent,native,sizeof(*native),alignof(int32_t)))return RAVET_VST3_INVALID;
    using Owner=ravet::vst3::draft::Instance;Owner::Range a{},z{};
    (void)Owner::range(output,1,sizeof(*output),alignof(RavetVst3Instance*),a);(void)Owner::range(native,1,sizeof(*native),alignof(int32_t),z);
    if(Owner::overlaps(a,z))return RAVET_VST3_INVALID;
    return invoke(native,[&]{
        auto child=std::make_unique<RavetVst3Instance>();child->parent=parent;
        // The pin takes effect BEFORE the first foreign create, including failed
        // or throwing creates. No later failure may unload this factory/image.
        if(catalogue.flags&Steinberg::PFactoryInfo::kComponentNonDiscardable){
            if(registry().pin(parent->core->reservation,parent->core)!=Registry::Result::Accepted)
                throw ravet::vst3::draft::InstanceFailure{ravet::vst3::draft::Stage::Lifecycle,Steinberg::kInternalError};
        }
        const auto& factory=parent->core->module->getFactory().get();
        child->owner=ravet::vst3::draft::Instance::allocate(catalogue,index,parent->core,parent->core->handlerKeepers,&parent->core->scratchBudget);
        if(!registryInsert(parent->core->childRegistry,child.get()))std::terminate();
        publishProtection(*parent->core,parent);
        try{child->owner->initialize(*factory,*parent->core->host,catalogue,index);}
        catch(...){(void)child->owner->retire();child->owner.reset();registryRemove(parent->core->childRegistry,child.get());throw;}
        ++parent->children;*output=child.release();
    });
}
int32_t prepareInstance(RavetVst3Instance* child,const RavetVst3AudioProfile* p,int32_t* native) noexcept {
    if(!child||!p||p->struct_size!=sizeof(*p)||!native)return RAVET_VST3_INVALID;
    if(!control(child->parent))return RAVET_VST3_THREAD;
    ControlScope scope(*child->parent);if(!scope.acquired)return RAVET_VST3_LIFECYCLE;
    if(!facadeExternal(*child->parent,native,sizeof(*native),alignof(int32_t)))return RAVET_VST3_INVALID;
    if(!facadeExternal(*child->parent,p,sizeof(*p),alignof(RavetVst3AudioProfile)))return RAVET_VST3_INVALID;
    return invoke(native,[&]{child->owner->prepare({p->sample_rate,p->maximum_frames,p->input_channels,p->output_channels});});
}
int32_t startInstance(RavetVst3Instance* child,int32_t* native) noexcept {
    if(!child||!native)return RAVET_VST3_INVALID;
    if(!control(child->parent))return RAVET_VST3_THREAD;
    ControlScope scope(*child->parent);if(!scope.acquired)return RAVET_VST3_LIFECYCLE;
    if(!facadeExternal(*child->parent,native,sizeof(*native),alignof(int32_t)))return RAVET_VST3_INVALID;
    return invoke(native,[&]{child->owner->start();});
}
int32_t beginAudio(RavetVst3Instance* child,RavetVst3Audio** output,int32_t* native) noexcept {
    if(!child||!output||*output||!native)return RAVET_VST3_INVALID;
    if(!control(child->parent))return RAVET_VST3_THREAD;
    ControlScope scope(*child->parent,true);if(!scope.acquired)return RAVET_VST3_LIFECYCLE;
    if(!facadeExternal(*child->parent,native,sizeof(*native),alignof(int32_t)))return RAVET_VST3_INVALID;
    if(!facadeExternal(*child->parent,output,sizeof(*output),alignof(RavetVst3Audio*)))return RAVET_VST3_INVALID;
    return invoke(native,[&]{auto capability=std::make_unique<RavetVst3Audio>();capability->instance=child;
        if(std::find(child->parent->core->audioRegistry.begin(),child->parent->core->audioRegistry.end(),nullptr)==child->parent->core->audioRegistry.end())
            throw std::length_error("audio capability registry capacity");
        child->owner->beginAudio();if(!registryInsert(child->parent->core->audioRegistry,capability.get()))std::terminate();
        ++child->parent->audioChildren;*output=capability.release();});
}
bool stateExternal(const RavetVst3Instance&,const void*,size_t,size_t,bool);
int32_t processAudio(RavetVst3Audio* audio,const RavetVst3AudioBlock* block,int32_t* native) noexcept {
    if(!audio||!block||!native)return RAVET_VST3_INVALID;
    auto& core=*audio->instance->parent->core;
    ravet::vst3::parameters_draft::LiveCoreProtection::Read admission(&core.liveProtection);
    if(!admission.admitted(&core.liveProtection))return RAVET_VST3_LIFECYCLE;
    const auto& child=*audio->instance;
    if(!stateExternal(child,block,sizeof(*block),alignof(RavetVst3AudioBlock),true)||
       !stateExternal(child,native,sizeof(*native),alignof(int32_t),true)||block->input_channels>2||block->output_channels>2||
       !stateExternal(child,block->inputs,block->input_channels*sizeof(RavetVst3InputChannel),alignof(RavetVst3InputChannel),true)||
       !stateExternal(child,block->outputs,block->output_channels*sizeof(RavetVst3OutputChannel),alignof(RavetVst3OutputChannel),true))return RAVET_VST3_INVALID;
    for(uint32_t i=0;i<block->input_channels;++i)if(!stateExternal(child,block->inputs[i].samples,size_t(block->inputs[i].capacity)*sizeof(float),alignof(float),true))return RAVET_VST3_INVALID;
    for(uint32_t i=0;i<block->output_channels;++i)if(!stateExternal(child,block->outputs[i].samples,size_t(block->outputs[i].capacity)*sizeof(float),alignof(float),true))return RAVET_VST3_INVALID;
    auto& instance=*audio->instance->owner;
    if(!instance.validateBlock(*block,native,sizeof(*native)))return RAVET_VST3_INVALID;
    using Owner=ravet::vst3::draft::Instance;
    Owner::Range handle{},childRange{},diagnostic{};
    if(!Owner::range(audio,1,sizeof(*audio),alignof(RavetVst3Audio),handle)||
       !Owner::range(audio->instance,1,sizeof(*audio->instance),alignof(RavetVst3Instance),childRange)||
       !Owner::range(native,1,sizeof(*native),alignof(int32_t),diagnostic))return RAVET_VST3_INVALID;
    if(Owner::overlaps(diagnostic,handle)||Owner::overlaps(diagnostic,childRange))return RAVET_VST3_INVALID;
    for(uint32_t ch=0;ch<block->output_channels;++ch){Owner::Range out{};
        (void)Owner::range(block->outputs[ch].samples,block->frames,sizeof(float),alignof(float),out);
        if(Owner::overlaps(out,handle)||Owner::overlaps(out,childRange))return RAVET_VST3_INVALID;
    }
    const auto result=instance.processBlock(*block,native,sizeof(*native));*native=result;
    return result==Steinberg::kResultOk?RAVET_VST3_OK:RAVET_VST3_NATIVE;
}
int32_t endAudio(RavetVst3Audio* audio,int32_t* native) noexcept {
    if(!audio||!native)return RAVET_VST3_INVALID;
    if(!control(audio->instance->parent))return RAVET_VST3_THREAD;
    ControlScope scope(*audio->instance->parent,true);if(!scope.acquired)return RAVET_VST3_LIFECYCLE;
    if(!facadeExternal(*audio->instance->parent,native,sizeof(*native),alignof(int32_t)))return RAVET_VST3_INVALID;
    retainProcessingCore(*audio->instance->parent);
    const auto status=invoke(native,[&]{audio->instance->owner->endAudio();});
    if(status==RAVET_VST3_OK){if(!audio->instance->parent->audioChildren)std::terminate();--audio->instance->parent->audioChildren;registryRemove(audio->instance->parent->core->audioRegistry,audio);delete audio;}
    return status;
}
int32_t stopInstance(RavetVst3Instance* child,int32_t* native) noexcept {
    if(!child||!native)return RAVET_VST3_INVALID;
    if(!control(child->parent))return RAVET_VST3_THREAD;
    ControlScope scope(*child->parent);if(!scope.acquired)return RAVET_VST3_LIFECYCLE;
    if(!facadeExternal(*child->parent,native,sizeof(*native),alignof(int32_t)))return RAVET_VST3_INVALID;
    return invoke(native,[&]{const auto result=child->owner->stop();if(result.native!=Steinberg::kResultOk||result.origin==ravet::vst3::draft::FailureOrigin::Thrown)throw result;});
}
int32_t destroyInstance(RavetVst3Instance* child,int32_t* native) noexcept {
    if(!child||!native)return RAVET_VST3_INVALID;
    if(!control(child->parent))return RAVET_VST3_THREAD;
    ControlScope scope(*child->parent);if(!scope.acquired)return RAVET_VST3_LIFECYCLE;
    if(!facadeExternal(*child->parent,native,sizeof(*native),alignof(int32_t)))return RAVET_VST3_INVALID;
    if(child->owner->attached())return RAVET_VST3_LIFECYCLE;
    if(child->editor)return RAVET_VST3_LIFECYCLE; // view must be released before controller retirement
    retainProcessingCore(*child->parent);
    const auto result=child->owner->retire();
    if(child->parent->core->parameterKeepers){
        child->owner->detachParameters(*child->parent->core->parameterKeepers);
        child->parent->core->parameterKeepers->reclaim();
    }
    if(child->parent->core->processingKeepers){
        auto& core=*child->parent->core;
        child->owner->detachProcessing(*core.processingKeepers,core);
        core.processingKeepers->reclaim(core,[&](NativeCore& origin){
            if(registry().pin(origin.reservation,child->parent->core)!=Registry::Result::Accepted)std::terminate();
        });
    }
    --child->parent->children;child->owner.reset();registryRemove(child->parent->core->childRegistry,child);delete child;*native=result.returned?result.native:result.origin==ravet::vst3::draft::FailureOrigin::Thrown?Steinberg::kInternalError:result.native;
    return result.native==Steinberg::kResultOk&&result.origin!=ravet::vst3::draft::FailureOrigin::Thrown?RAVET_VST3_OK:RAVET_VST3_RETIRED_NATIVE;
}
int32_t restartRequests(const RavetVst3Instance* child,uint32_t* out) noexcept {
    if(!child||!out)return RAVET_VST3_INVALID;
    if(!control(child->parent))return RAVET_VST3_THREAD;
    auto& live=child->parent->core->liveProtection;
    ravet::vst3::parameters_draft::LiveCoreProtection::Read admission(&live);
    if(!admission.admitted(&live))return RAVET_VST3_LIFECYCLE;
    if(!facadeExternal(*child->parent,out,sizeof(*out),alignof(uint32_t)))return RAVET_VST3_INVALID;
    *out=child->owner->restartRequests();return RAVET_VST3_OK;
}
// No caller range may overwrite provider/parent/core/scratch/keeper ownership.
// Caller liveness remains a trust requirement; these are arithmetic/alias checks.
bool stateExternal(const RavetVst3Instance& child,const void* data,size_t bytes,size_t alignment,bool audio=false) {
    using Owner=ravet::vst3::draft::Instance;
    Owner::Range requested{},object{};
    if(!Owner::range(data,bytes,1,alignment,requested)||!child.owner->stateExternal(data,bytes,alignment))return false;
    if(!Owner::range(&child,1,sizeof(child),alignof(RavetVst3Instance),object)||Owner::overlaps(requested,object))return false;
    if(!Owner::range(child.parent,1,sizeof(*child.parent),alignof(RavetVst3Module),object)||Owner::overlaps(requested,object))return false;
    if(!Owner::range(child.parent->core.get(),1,sizeof(NativeCore),alignof(NativeCore),object)||Owner::overlaps(requested,object))return false;
    for(auto* sibling:child.parent->core->childRegistry)if(sibling){
        if(!Owner::range(sibling,1,sizeof(*sibling),alignof(RavetVst3Instance),object)||Owner::overlaps(requested,object))return false;
        bool allowed=true;
        auto check=[&](const void* p,size_t n,size_t a){Owner::Range r{};return Owner::range(p,n,1,a,r)&&!Owner::overlaps(requested,r);};
        allowed=sibling->owner->appendOwnedRanges(check);if(!allowed)return false;
    }
    for(auto* capability:child.parent->core->audioRegistry)if(capability&&
        (!Owner::range(capability,1,sizeof(*capability),alignof(RavetVst3Audio),object)||Owner::overlaps(requested,object)))return false;
    if(!child.parent->core->immutableProtection.externalRange(data,bytes,alignment))return false;
    const auto& keepers=child.parent->core->stateKeepers;
    if(!child.parent->core->handlerKeepers.externalRange(data,bytes,alignment))return false;
    if(keepers&&!(audio?keepers->borrowedExternalRange(data,bytes,alignment):keepers->externalRange(data,bytes,alignment)))return false;
    const auto& parameterKeepers=child.parent->core->parameterKeepers;
    if(parameterKeepers&&!parameterKeepers->externalRange(data,bytes,alignment))return false;
    const auto& processingKeepers=child.parent->core->processingKeepers;
    return !processingKeepers||processingKeepers->externalRange(data,bytes,alignment);
}
bool stateRanges(const RavetVst3Instance& child,const void* record,const void* component,size_t componentBytes,
        const void* controller,size_t controllerBytes,const void* report,bool capture) {
    using Owner=ravet::vst3::draft::Instance;
    Owner::Range records{},components{},controllers{},reports{};
    if(!stateExternal(child,record,sizeof(RavetVst3StateRecord),alignof(RavetVst3StateRecord))||
       !stateExternal(child,report,sizeof(RavetVst3StateReport),alignof(RavetVst3StateReport))||
       !stateExternal(child,component,componentBytes,1)||!stateExternal(child,controller,controllerBytes,1))return false;
    (void)Owner::range(record,1,sizeof(RavetVst3StateRecord),alignof(RavetVst3StateRecord),records);
    (void)Owner::range(report,1,sizeof(RavetVst3StateReport),alignof(RavetVst3StateReport),reports);
    (void)Owner::range(component,componentBytes,1,1,components);(void)Owner::range(controller,controllerBytes,1,1,controllers);
    if(Owner::overlaps(records,reports)||Owner::overlaps(components,reports)||Owner::overlaps(controllers,reports)||
       Owner::overlaps(components,records)||Owner::overlaps(controllers,records))return false;
    return !capture||!Owner::overlaps(components,controllers);
}
// This scope is established while exact parent CONTROL reservation is held.
// reserve()/pin() never execute foreign code while the registry mutex is held.
struct StateRetention {
    RavetVst3Module& parent;
    explicit StateRetention(RavetVst3Module& p):parent(p) {
        if(!registry().activeReservation(parent.core->reservation,parent.core))std::terminate();
    }
    ~StateRetention() {
        auto& core=*parent.core;
        if(core.stateKeepers&&core.stateKeepers->hasForeignReferences()) {
            if(registry().pin(core.reservation,parent.core)!=Registry::Result::Accepted)std::terminate();
        }
        if(core.stateKeepers)core.stateKeepers->reclaim();
    }
};
int32_t captureState(RavetVst3Instance* child,RavetVst3StateRecord* record,uint8_t* component,uint32_t componentCapacity,
        uint8_t* controller,uint32_t controllerCapacity,RavetVst3StateReport* report) noexcept {
    if(!child||!record||!report)return RAVET_VST3_INVALID;
    if(!control(child->parent))return RAVET_VST3_THREAD;
    ControlScope scope(*child->parent);if(!scope.acquired)return RAVET_VST3_LIFECYCLE;
    using Stream=ravet::vst3::state_draft::BoundedStateStream;
    if(!componentCapacity||componentCapacity>Stream::MaximumBytes||!controllerCapacity||controllerCapacity>Stream::MaximumBytes||
       !stateRanges(*child,record,component,componentCapacity,controller,controllerCapacity,report,true))return RAVET_VST3_INVALID;
    if(record->struct_size!=sizeof(*record)||report->struct_size!=sizeof(*report))return RAVET_VST3_INVALID;
    try {
        if(!child->owner->stateAllowed())return RAVET_VST3_LIFECYCLE;
        StateRetention retained(*child->parent);
        auto staged=ravet::vst3::state_draft::StateTransaction::initial();
        const bool success=child->owner->captureState(child->parent->core->streams(),component,componentCapacity,
            controller,controllerCapacity,*record,staged);
        *report=staged;return success?RAVET_VST3_OK:RAVET_VST3_NATIVE;
    }catch(const std::length_error&){return RAVET_VST3_CAPACITY;}
    catch(const std::bad_alloc&){return RAVET_VST3_ALLOCATION;}
    catch(...){return RAVET_VST3_NATIVE;}
}
int32_t restoreState(RavetVst3Instance* child,const RavetVst3StateRecord* record,const uint8_t* component,
        const uint8_t* controller,RavetVst3StateReport* report) noexcept {
    if(!child||!record||!report)return RAVET_VST3_INVALID;
    if(!control(child->parent))return RAVET_VST3_THREAD;
    ControlScope scope(*child->parent);if(!scope.acquired)return RAVET_VST3_LIFECYCLE;
    // Validate metadata itself before reading its sizes, then all source extents.
    if(!stateExternal(*child,record,sizeof(*record),alignof(RavetVst3StateRecord))||
       !stateExternal(*child,report,sizeof(*report),alignof(RavetVst3StateReport)))return RAVET_VST3_INVALID;
    if(record->struct_size!=sizeof(*record)||report->struct_size!=sizeof(*report))return RAVET_VST3_INVALID;
    try {
        if(!child->owner->stateAllowed())return RAVET_VST3_LIFECYCLE;
        if(!child->owner->stateIdentity(*record)||!stateRanges(*child,record,component,record->component_size,
            controller,record->controller_size,report,false))return RAVET_VST3_INVALID;
        StateRetention retained(*child->parent);
        auto staged=ravet::vst3::state_draft::StateTransaction::initial();
        const bool success=child->owner->restoreState(child->parent->core->streams(),*record,component,controller,staged);
        *report=staged;return success?RAVET_VST3_OK:RAVET_VST3_NATIVE;
    }catch(const std::length_error&){return RAVET_VST3_CAPACITY;}
    catch(const std::bad_alloc&){return RAVET_VST3_ALLOCATION;}
    catch(...){return RAVET_VST3_NATIVE;}
}
const RavetVst3ProviderV2 providerV2{sizeof(RavetVst3ProviderV2),2,openV2,factory,classInfo,closeV2,
    createInstance,prepareInstance,startInstance,beginAudio,processAudio,endAudio,stopInstance,destroyInstance,restartRequests};
const RavetVst3Provider provider{sizeof(RavetVst3Provider), 1, open, factory, classInfo, close};
const RavetVst3StateProviderV1 stateProvider{sizeof(RavetVst3StateProviderV1),1,&providerV2,captureState,restoreState};

bool parameterRanges(const RavetVst3Instance& child,const void* output,size_t bytes,size_t alignment,
        const RavetVst3ParameterReport* report) {
    using Owner=ravet::vst3::draft::Instance;
    Owner::Range a{},b{};
    return report&&stateExternal(child,output,bytes,alignment)&&
        stateExternal(child,report,sizeof(*report),alignof(RavetVst3ParameterReport))&&
        Owner::range(output,bytes,1,alignment,a)&&Owner::range(report,1,sizeof(*report),alignof(RavetVst3ParameterReport),b)&&
        !Owner::overlaps(a,b)&&report->struct_size==sizeof(*report);
}
template<class Call> int32_t parameterInvoke(RavetVst3Instance& child,RavetVst3ParameterReport& report,Call call,uint32_t stage=0) {
    using namespace ravet::vst3::parameters_draft;
    RavetVst3ParameterReport staged{sizeof(report),stage,0,0,0,Steinberg::kResultOk};
    int32_t status=RAVET_VST3_OK;
    try{call();}
    catch(const ParameterFailure& e){
        staged.stage=uint32_t(e.stage);staged.failure=16+uint32_t(e.kind);staged.index=e.index;staged.native_result=e.kind==ParameterErrorKind::Native?e.native:0;
        if(e.kind==ParameterErrorKind::Native)staged.flags=RAVET_VST3_PARAMETER_ATTEMPTED|RAVET_VST3_PARAMETER_RETURNED;
        if(e.kind==ParameterErrorKind::Thrown)staged.flags=RAVET_VST3_PARAMETER_ATTEMPTED|RAVET_VST3_PARAMETER_THROWN;
        status=e.kind==ParameterErrorKind::Capacity?RAVET_VST3_CAPACITY:
            e.kind==ParameterErrorKind::Native||e.kind==ParameterErrorKind::Thrown?RAVET_VST3_NATIVE:RAVET_VST3_METADATA;
    }
    catch(const ravet::vst3::draft::InstanceFailure& e){
        using Stage=ravet::vst3::draft::Stage;
        if(e.stage==Stage::Thread)return RAVET_VST3_THREAD;
        if(e.stage==Stage::Lifecycle)return RAVET_VST3_LIFECYCLE;
        if(e.stage==Stage::Profile)return RAVET_VST3_INVALID;
        staged.stage=100+uint32_t(e.stage);
        staged.failure=e.origin==ravet::vst3::draft::FailureOrigin::Thrown?RAVET_VST3_PARAMETER_FAILURE_THROWN:
            e.returned?RAVET_VST3_PARAMETER_FAILURE_NATIVE:RAVET_VST3_PARAMETER_FAILURE_METADATA;
        if(e.returned){staged.flags=RAVET_VST3_PARAMETER_ATTEMPTED|RAVET_VST3_PARAMETER_RETURNED;staged.native_result=e.native;}
        if(e.origin==ravet::vst3::draft::FailureOrigin::Thrown)staged.flags=RAVET_VST3_PARAMETER_ATTEMPTED|RAVET_VST3_PARAMETER_THROWN;
        status=e.origin==ravet::vst3::draft::FailureOrigin::Local&&!e.returned?RAVET_VST3_METADATA:RAVET_VST3_NATIVE;
    }
    catch(const std::length_error&){return RAVET_VST3_CAPACITY;}
    catch(const std::bad_alloc&){return RAVET_VST3_ALLOCATION;}
    catch(...){staged.failure=RAVET_VST3_PARAMETER_FAILURE_METADATA;staged.native_result=0;status=RAVET_VST3_METADATA;}
    if(!child.owner->parametersCompatible())staged.flags|=RAVET_VST3_PARAMETER_INVALIDATED;
    report=staged;return status;
}
int32_t refreshParameters(RavetVst3Instance* child,uint32_t* count,RavetVst3ParameterReport* report) noexcept {
    if(!child||!count||!report)return RAVET_VST3_INVALID;
    if(!control(child->parent))return RAVET_VST3_THREAD;
    ControlScope scope(*child->parent);if(!scope.acquired)return RAVET_VST3_LIFECYCLE;
    if(!parameterRanges(*child,count,sizeof(*count),alignof(uint32_t),report))return RAVET_VST3_INVALID;
    return parameterInvoke(*child,*report,[&]{child->owner->refreshParameters();*count=uint32_t(child->owner->parameterCatalogue().parameters().size());});
}
int32_t parameterInfo(const RavetVst3Instance* child,uint32_t index,RavetVst3ParameterRecord* output) noexcept {
    if(!child||!output)return RAVET_VST3_INVALID;
    if(!control(child->parent))return RAVET_VST3_THREAD;
    ControlScope scope(*child->parent);if(!scope.acquired)return RAVET_VST3_LIFECYCLE;
    if(!stateExternal(*child,output,sizeof(*output),alignof(RavetVst3ParameterRecord))||output->struct_size!=sizeof(*output))return RAVET_VST3_INVALID;
    try{
        const auto& parameters=child->owner->parameterCatalogue().parameters();if(index>=parameters.size())return RAVET_VST3_INVALID;
        const auto& p=parameters[index];RavetVst3ParameterRecord staged{};staged.struct_size=sizeof(staged);
        staged.id=p.id;staged.flags=p.flags;staged.steps=p.steps;staged.unit=p.unit;staged.default_normalized=p.defaultNormalized;
        std::memcpy(staged.title,p.title.data(),p.title.size());std::memcpy(staged.short_title,p.shortTitle.data(),p.shortTitle.size());std::memcpy(staged.units,p.units.data(),p.units.size());
        *output=staged;return RAVET_VST3_OK;
    }catch(...){return RAVET_VST3_LIFECYCLE;}
}
int32_t parameterValue(RavetVst3Instance* child,uint32_t id,uint32_t operation,double input,double* output,RavetVst3ParameterReport* report) noexcept {
    if(!child||!output||!report||operation>RAVET_VST3_PARAMETER_EDIT)return RAVET_VST3_INVALID;
    if(!control(child->parent))return RAVET_VST3_THREAD;
    ControlScope scope(*child->parent);if(!scope.acquired)return RAVET_VST3_LIFECYCLE;
    if(!parameterRanges(*child,output,sizeof(*output),alignof(double),report))return RAVET_VST3_INVALID;
    return parameterInvoke(*child,*report,[&]{const auto value=child->owner->parameterValue(id,operation,input);*output=value;});
}
int32_t formatParameter(RavetVst3Instance* child,uint32_t id,double value,uint8_t* output,uint32_t capacity,uint32_t* length,RavetVst3ParameterReport* report) noexcept {
    if(!child||!output||!length||!report||!capacity||capacity>512)return RAVET_VST3_INVALID;
    if(!control(child->parent))return RAVET_VST3_THREAD;
    ControlScope scope(*child->parent);if(!scope.acquired)return RAVET_VST3_LIFECYCLE;
    using Owner=ravet::vst3::draft::Instance;Owner::Range bytes{},size{};
    if(!parameterRanges(*child,output,capacity,1,report)||!parameterRanges(*child,length,sizeof(*length),alignof(uint32_t),report)||
       !Owner::range(output,capacity,1,1,bytes)||!Owner::range(length,1,sizeof(*length),alignof(uint32_t),size)||Owner::overlaps(bytes,size))return RAVET_VST3_INVALID;
    return parameterInvoke(*child,*report,[&]{
        auto text=child->owner->formatParameter(id,value);if(text.size()>=capacity)throw std::length_error("parameter text capacity");
        std::memcpy(output,text.data(),text.size());output[text.size()]=0;*length=uint32_t(text.size());
    });
}
bool parameterUtf16(const uint8_t* bytes,uint32_t count,Steinberg::Vst::String128& output) {
    if(count>384||(!bytes&&count))return false;
    const std::string input(count?reinterpret_cast<const char*>(bytes):"",count);
    if(input.find('\0')!=std::string::npos||!ravet::vst3::utf8(input))return false;
    size_t p=0,n=0;while(p<count){
        uint32_t c=bytes[p++],extra=0;
        if(c>=0xf0){c&=7;extra=3;}else if(c>=0xe0){c&=15;extra=2;}else if(c>=0xc0){c&=31;extra=1;}
        while(extra--){c=(c<<6)|(bytes[p++]&63);}
        if(c<=0xffff){if(n>=127)return false;output[n++]=Steinberg::Vst::TChar(c);}
        else{if(n>=126)return false;c-=0x10000;output[n++]=Steinberg::Vst::TChar(0xd800+(c>>10));output[n++]=Steinberg::Vst::TChar(0xdc00+(c&1023));}
    }
    output[n]=0;return true;
}
int32_t parseParameter(RavetVst3Instance* child,uint32_t id,const uint8_t* text,uint32_t length,double* output,RavetVst3ParameterReport* report) noexcept {
    if(!child||!output||!report||length>384||(!text&&length))return RAVET_VST3_INVALID;
    if(!control(child->parent))return RAVET_VST3_THREAD;
    ControlScope scope(*child->parent);if(!scope.acquired)return RAVET_VST3_LIFECYCLE;
    using Owner=ravet::vst3::draft::Instance;Owner::Range source{},out{},diagnostic{};
    if(!parameterRanges(*child,output,sizeof(*output),alignof(double),report)||!stateExternal(*child,text,length,1)||
       !Owner::range(text,length,1,1,source)||!Owner::range(output,1,sizeof(*output),alignof(double),out)||
       !Owner::range(report,1,sizeof(*report),alignof(RavetVst3ParameterReport),diagnostic)||
       Owner::overlaps(source,out)||Owner::overlaps(source,diagnostic))return RAVET_VST3_INVALID;
    return parameterInvoke(*child,*report,[&]{
        Steinberg::Vst::String128 native{};if(!parameterUtf16(text,length,native))throw ravet::vst3::parameters_draft::ParameterFailure{
            ravet::vst3::parameters_draft::ParameterErrorKind::Metadata,ravet::vst3::parameters_draft::ParameterStage::Parse,id};
        const auto value=child->owner->parseParameter(id,native);*output=value;
    });
}
int32_t prepareParameters(RavetVst3Instance* child,const RavetVst3AudioProfile* profile,RavetVst3ParameterReport* report) noexcept {
    if(!child||!profile||!report)return RAVET_VST3_INVALID;
    if(!control(child->parent))return RAVET_VST3_THREAD;
    ControlScope scope(*child->parent);if(!scope.acquired)return RAVET_VST3_LIFECYCLE;
    if(!parameterRanges(*child,profile,sizeof(*profile),alignof(RavetVst3AudioProfile),report)||profile->struct_size!=sizeof(*profile))return RAVET_VST3_INVALID;
    return parameterInvoke(*child,*report,[&]{child->owner->prepareParameters({profile->sample_rate,profile->maximum_frames,profile->input_channels,profile->output_channels},child->parent->core->parameterPools());},RAVET_VST3_PARAMETER_STAGE_PREPARE);
}
int32_t processParameters(RavetVst3Audio* audio,const RavetVst3AudioBlock* block,const RavetVst3ParameterPoint* points,uint32_t count,RavetVst3ParameterReport* report) noexcept {
    if(!audio||!block||!report||count>2048)return RAVET_VST3_INVALID;
    const auto& child=*audio->instance;
    auto& live=child.parent->core->liveProtection;
    ravet::vst3::parameters_draft::LiveCoreProtection::Read admission(&live);
    if(!admission.admitted(&live))return RAVET_VST3_LIFECYCLE;
    // Joined exclusive AUDIO contract; no CONTROL registry calls in this path.
    if(!stateExternal(child,report,sizeof(*report),alignof(RavetVst3ParameterReport),true)||report->struct_size!=sizeof(*report)||
       !stateExternal(child,points,count*sizeof(*points),alignof(RavetVst3ParameterPoint),true)||
       !stateExternal(child,block,sizeof(*block),alignof(RavetVst3AudioBlock),true))return RAVET_VST3_INVALID;
    using Owner=ravet::vst3::draft::Instance;Owner::Range handle{},diagnostic{},source{};
    if(!Owner::range(audio,1,sizeof(*audio),alignof(RavetVst3Audio),handle)||
       !Owner::range(report,1,sizeof(*report),alignof(RavetVst3ParameterReport),diagnostic)||
       !Owner::range(points,count,sizeof(*points),alignof(RavetVst3ParameterPoint),source)||
       Owner::overlaps(handle,diagnostic)||Owner::overlaps(handle,source))return RAVET_VST3_INVALID;
    if(block->input_channels>2||block->output_channels>2||
       !stateExternal(child,block->inputs,block->input_channels*sizeof(RavetVst3InputChannel),alignof(RavetVst3InputChannel),true)||
       !stateExternal(child,block->outputs,block->output_channels*sizeof(RavetVst3OutputChannel),alignof(RavetVst3OutputChannel),true))return RAVET_VST3_INVALID;
    for(uint32_t i=0;i<block->input_channels;++i)if(!stateExternal(child,block->inputs[i].samples,size_t(block->inputs[i].capacity)*sizeof(float),alignof(float),true))return RAVET_VST3_INVALID;
    for(uint32_t i=0;i<block->output_channels;++i)if(!stateExternal(child,block->outputs[i].samples,size_t(block->outputs[i].capacity)*sizeof(float),alignof(float),true))return RAVET_VST3_INVALID;
    if(!child.owner->parametersCompatible())return RAVET_VST3_LIFECYCLE;
    if(!child.owner->validateBlock(*block,report,sizeof(*report)))return RAVET_VST3_INVALID;
    for(uint32_t ch=0;ch<block->output_channels;++ch){Owner::Range out{};
        (void)Owner::range(block->outputs[ch].samples,block->frames,sizeof(float),alignof(float),out);
        if(Owner::overlaps(out,handle)||!stateExternal(child,block->outputs[ch].samples,block->frames*sizeof(float),alignof(float),true))return RAVET_VST3_INVALID;
    }
    return child.owner->processParameterBlock(*block,points,count,*report);
}
const RavetVst3ParameterProviderV1 parameterProvider{sizeof(RavetVst3ParameterProviderV1),1,&providerV2,
    refreshParameters,parameterInfo,parameterValue,formatParameter,parseParameter,prepareParameters,processParameters};
// Stopped processing metadata operations share the module's existing CONTROL
// exclusion, including every sibling AUDIO capability and retained-core pin.
bool processingControlRanges(const RavetVst3Instance& child,const void* input,size_t inputBytes,size_t inputAlignment,
        const void* output,size_t outputBytes,size_t outputAlignment,const RavetVst3ProcessingReport* report) {
    using namespace ravet::vst3::parameters_draft;
    Range a{},b{},c{};
    return report&&stateExternal(child,input,inputBytes,inputAlignment)&&stateExternal(child,output,outputBytes,outputAlignment)&&
        stateExternal(child,report,sizeof(*report),alignof(RavetVst3ProcessingReport))&&
        range(input,inputBytes,1,inputAlignment,a)&&range(output,outputBytes,1,outputAlignment,b)&&
        range(report,1,sizeof(*report),alignof(RavetVst3ProcessingReport),c)&&
        !overlaps(a,b)&&!overlaps(a,c)&&!overlaps(b,c)&&report->struct_size==sizeof(*report);
}
template<class Call> int32_t processingInvoke(RavetVst3Instance& child,RavetVst3ProcessingReport& report,uint32_t stage,Call call) noexcept {
    using namespace ravet::vst3::processing_draft;
    RavetVst3ProcessingReport staged{};staged.struct_size=sizeof(staged);staged.stage=stage;staged.index=UINT32_MAX;
    int32_t status=RAVET_VST3_OK;
    try{call();}
    catch(const CatalogueFailure& e){
        staged.stage=16+uint32_t(e.stage);staged.failure=16+uint32_t(e.kind);
        if(e.returned){staged.flags=RAVET_VST3_PROCESSING_ATTEMPTED|RAVET_VST3_PROCESSING_RETURNED;staged.native_result=e.native;}
        if(e.kind==CatalogueError::Thrown)staged.flags=RAVET_VST3_PROCESSING_ATTEMPTED|RAVET_VST3_PROCESSING_THROWN;
        status=e.kind==CatalogueError::Invalid?RAVET_VST3_METADATA:RAVET_VST3_NATIVE;
    }
    catch(const ravet::vst3::draft::InstanceFailure& e){
        using Stage=ravet::vst3::draft::Stage;
        if(e.stage==Stage::Thread)return RAVET_VST3_THREAD;
        if(e.stage==Stage::Lifecycle)return RAVET_VST3_LIFECYCLE;
        if(e.stage==Stage::Profile)return RAVET_VST3_INVALID;
        staged.stage=100+uint32_t(e.stage);
        staged.failure=e.origin==ravet::vst3::draft::FailureOrigin::Thrown?RAVET_VST3_PROCESSING_FAILURE_THROWN:
            e.returned?RAVET_VST3_PROCESSING_FAILURE_NATIVE:16+uint32_t(CatalogueError::Invalid);
        if(e.returned){staged.flags=RAVET_VST3_PROCESSING_ATTEMPTED|RAVET_VST3_PROCESSING_RETURNED;staged.native_result=e.native;}
        if(e.origin==ravet::vst3::draft::FailureOrigin::Thrown)staged.flags=RAVET_VST3_PROCESSING_ATTEMPTED|RAVET_VST3_PROCESSING_THROWN;
        status=e.origin==ravet::vst3::draft::FailureOrigin::Local&&!e.returned?RAVET_VST3_METADATA:RAVET_VST3_NATIVE;
    }
    catch(const std::length_error&){return RAVET_VST3_CAPACITY;}
    catch(const std::bad_alloc&){return RAVET_VST3_ALLOCATION;}
    catch(const std::logic_error&){return RAVET_VST3_LIFECYCLE;}
    catch(...){staged.failure=16+uint32_t(CatalogueError::Invalid);status=RAVET_VST3_METADATA;}
    if(!child.owner->processingCompatible())staged.flags|=RAVET_VST3_PROCESSING_INVALIDATED;
    report=staged;return status;
}
int32_t refreshProcessing(RavetVst3Instance* child,const RavetVst3ProcessingSelection* selection,
        RavetVst3ProcessingInfo* info,RavetVst3ProcessingReport* report) noexcept {
    if(!child||!selection||!info||!report)return RAVET_VST3_INVALID;
    if(!control(child->parent))return RAVET_VST3_THREAD;
    ControlScope scope(*child->parent);if(!scope.acquired)return RAVET_VST3_LIFECYCLE;
    if(!processingControlRanges(*child,selection,sizeof(*selection),alignof(RavetVst3ProcessingSelection),
        info,sizeof(*info),alignof(RavetVst3ProcessingInfo),report)||selection->struct_size!=sizeof(*selection)||
        selection->reserved||selection->input_mask>0xffff||selection->output_mask>0xffff||info->struct_size!=sizeof(*info))return RAVET_VST3_INVALID;
    return processingInvoke(*child,*report,RAVET_VST3_PROCESSING_STAGE_REFRESH,[&]{
        child->owner->refreshProcessing(uint16_t(selection->input_mask),uint16_t(selection->output_mask));
        const auto& catalogue=child->owner->processingCatalogue();
        RavetVst3ProcessingInfo staged{};staged.struct_size=sizeof(staged);
        staged.input_bus_count=catalogue.inputCount();staged.output_bus_count=catalogue.outputCount();
        staged.selected_input_mask=selection->input_mask;staged.selected_output_mask=selection->output_mask;
        staged.flags=(catalogue.hasMidiMapping()?RAVET_VST3_PROCESSING_MAPPING:0)|
            (catalogue.hasContextRequirements()?RAVET_VST3_PROCESSING_CONTEXT_REQUIREMENTS:0);
        staged.context_requirements=catalogue.contextRequirements();*info=staged;
    });
}
int32_t processingBusInfo(const RavetVst3Instance* child,uint32_t output,uint32_t index,RavetVst3EventBusRecord* info) noexcept {
    if(!child||!info||output>1)return RAVET_VST3_INVALID;
    if(!control(child->parent))return RAVET_VST3_THREAD;
    ControlScope scope(*child->parent);if(!scope.acquired)return RAVET_VST3_LIFECYCLE;
    if(!stateExternal(*child,info,sizeof(*info),alignof(RavetVst3EventBusRecord))||info->struct_size!=sizeof(*info))return RAVET_VST3_INVALID;
    try{const auto& bus=child->owner->processingCatalogue().bus(output!=0,index);
        RavetVst3EventBusRecord staged{};staged.struct_size=sizeof(staged);staged.index=bus.index;
        staged.channels=bus.channels;staged.bus_type=bus.type;staged.flags=bus.flags;copy(staged.name,bus.name);*info=staged;
        return RAVET_VST3_OK;
    }catch(const ravet::vst3::processing_draft::CatalogueFailure&){return RAVET_VST3_INVALID;}
    catch(...){return RAVET_VST3_LIFECYCLE;}
}
int32_t processingMidiAssignment(const RavetVst3Instance* child,uint32_t bus,uint32_t channel,uint32_t controller,uint32_t* id) noexcept {
    if(!child||!id)return RAVET_VST3_INVALID;
    if(!control(child->parent))return RAVET_VST3_THREAD;
    ControlScope scope(*child->parent);if(!scope.acquired)return RAVET_VST3_LIFECYCLE;
    if(!stateExternal(*child,id,sizeof(*id),alignof(uint32_t)))return RAVET_VST3_INVALID;
    try{*id=child->owner->processingCatalogue().assignment(bus,channel,controller);return RAVET_VST3_OK;}
    catch(const ravet::vst3::processing_draft::CatalogueFailure&){return RAVET_VST3_INVALID;}
    catch(...){return RAVET_VST3_LIFECYCLE;}
}
int32_t prepareProcessing(RavetVst3Instance* child,const RavetVst3AudioProfile* profile,RavetVst3ProcessingReport* report) noexcept {
    if(!child||!profile||!report)return RAVET_VST3_INVALID;
    if(!control(child->parent))return RAVET_VST3_THREAD;
    ControlScope scope(*child->parent);if(!scope.acquired)return RAVET_VST3_LIFECYCLE;
    if(!processingControlRanges(*child,profile,sizeof(*profile),alignof(RavetVst3AudioProfile),nullptr,0,1,report)||
        profile->struct_size!=sizeof(*profile))return RAVET_VST3_INVALID;
    return processingInvoke(*child,*report,RAVET_VST3_PROCESSING_STAGE_PREPARE,[&]{
        auto& core=*child->parent->core;child->owner->prepareProcessing(
            {profile->sample_rate,profile->maximum_frames,profile->input_channels,profile->output_channels},
            core.parameterPools(),core.processingPools(),core);
    });
}
int32_t processComplete(RavetVst3Audio* audio,const RavetVst3AudioBlock* block,
        const RavetVst3ParameterPoint* points,uint32_t pointCount,const RavetVst3EventRecord* events,uint32_t eventCount,
        const uint8_t* payload,uint32_t payloadBytes,const RavetVst3ContextRecord* context,
        RavetVst3ProcessingOutputs* outputs,RavetVst3ProcessingReport* report) noexcept {
    if(!audio||!audio->instance||!block||!outputs||!report)return RAVET_VST3_INVALID;
    using namespace ravet::vst3::processing_draft;using namespace ravet::vst3::parameters_draft;
    const auto& child=*audio->instance;auto& core=*child.parent->core;
    LiveCoreProtection::Read admission(&core.liveProtection);
    if(!admission.admitted(&core.liveProtection))return RAVET_VST3_LIFECYCLE;
    ProcessingBoundary boundary;Range handle{};
    if(!range(audio,1,sizeof(*audio),alignof(RavetVst3Audio),handle)||
       !boundary.admit(*block,points,pointCount,events,eventCount,payload,payloadBytes,context,*outputs,*report,
        [&](const void* p,size_t bytes,size_t alignment){Range r{};return stateExternal(child,p,bytes,alignment,true)&&
            range(p,bytes,1,alignment,r)&&!overlaps(r,handle);}))return RAVET_VST3_INVALID;
    // Pool ownership stays fixed while module AUDIO children are outstanding.
    // Copy numeric extents only, without retaining/releasing interfaces on AUDIO.
    ProcessingBoundary protection;
    auto append=[&](const void* p,size_t bytes,size_t alignment){Range r{};
        return range(p,bytes,1,alignment,r)&&protection.append(r);};
    if(!append(audio,sizeof(*audio),alignof(RavetVst3Audio))||!append(&child,sizeof(child),alignof(RavetVst3Instance))||
       !append(child.parent,sizeof(*child.parent),alignof(RavetVst3Module))||!append(&core,sizeof(core),alignof(NativeCore))||
       (core.parameterKeepers&&!core.parameterKeepers->appendProtectionRanges(append))||
       (core.processingKeepers&&!core.processingKeepers->appendProtectionRanges(append))||
       (core.stateKeepers&&!core.stateKeepers->appendProtectionRanges(append)))return RAVET_VST3_CAPACITY;
    return child.owner->processCompleteBlock(*block,points,pointCount,events,eventCount,payload,payloadBytes,context,*outputs,*report,
        protection.ranges(),protection.count());
}
const RavetVst3ProcessingProviderV1 processingProvider{sizeof(RavetVst3ProcessingProviderV1),1,&providerV2,
    refreshProcessing,processingBusInfo,processingMidiAssignment,prepareProcessing,processComplete};
bool hostedControlRanges(const RavetVst3Instance& child,const void* input,size_t inputBytes,size_t inputAlignment,
        const void* output,size_t outputBytes,size_t outputAlignment,const RavetVst3HostedAudioReport* report) {
    using namespace ravet::vst3::parameters_draft;Range a{},b{},c{};
    return report&&stateExternal(child,input,inputBytes,inputAlignment)&&stateExternal(child,output,outputBytes,outputAlignment)&&
        stateExternal(child,report,sizeof(*report),alignof(RavetVst3HostedAudioReport))&&
        range(input,inputBytes,1,inputAlignment,a)&&range(output,outputBytes,1,outputAlignment,b)&&
        range(report,1,sizeof(*report),alignof(RavetVst3HostedAudioReport),c)&&!overlaps(a,b)&&!overlaps(a,c)&&!overlaps(b,c)&&report->struct_size==sizeof(*report);
}
template<class Call> int32_t hostedInvoke(RavetVst3Instance& child,RavetVst3HostedAudioReport& report,
        ravet::vst3::draft::Stage stage,Call call)noexcept {
    using namespace ravet::vst3::draft;
    RavetVst3HostedAudioReport staged{};staged.struct_size=sizeof(staged);staged.stage=100+uint32_t(stage);staged.index=UINT32_MAX;
    const bool previouslyPrepared=child.owner->prepared();int32_t status=RAVET_VST3_OK;
    try{call();}
    catch(const InstanceFailure& e) {
        staged.stage=100+uint32_t(e.stage);
        if(e.origin==FailureOrigin::Thrown) {staged.failure=3;staged.flags=RAVET_VST3_HOSTED_ATTEMPTED|RAVET_VST3_HOSTED_THROWN;status=RAVET_VST3_NATIVE;}
        else if(e.returned) {staged.failure=2;staged.flags=RAVET_VST3_HOSTED_ATTEMPTED|RAVET_VST3_HOSTED_RETURNED;staged.native_result=e.native;status=RAVET_VST3_NATIVE;}
        else {staged.failure=e.stage==Stage::Buses||e.stage==Stage::Arrangement?6u:1u;
            status=e.stage==Stage::Thread?RAVET_VST3_THREAD:e.stage==Stage::Lifecycle?RAVET_VST3_LIFECYCLE:staged.failure==6?RAVET_VST3_METADATA:RAVET_VST3_INVALID;}
    }
    catch(const std::length_error&) {staged.failure=4;status=RAVET_VST3_CAPACITY;}
    catch(const std::bad_alloc&) {staged.failure=5;status=RAVET_VST3_ALLOCATION;}
    catch(const std::invalid_argument&) {staged.failure=1;status=RAVET_VST3_INVALID;}
    catch(const std::logic_error&) {staged.failure=1;status=RAVET_VST3_LIFECYCLE;}
    catch(...) {staged.failure=6;status=RAVET_VST3_METADATA;}
    if(previouslyPrepared&&!child.owner->prepared())staged.flags|=RAVET_VST3_HOSTED_INVALIDATED;
    report=staged;return status;
}
int32_t refreshHostedAudio(RavetVst3Instance* child,RavetVst3HostedAudioInfo* output,RavetVst3HostedAudioReport* report)noexcept {
    if(!child||!output||!report)return RAVET_VST3_INVALID;
    if(!control(child->parent))return RAVET_VST3_THREAD;
    ControlScope scope(*child->parent);if(!scope.acquired)return RAVET_VST3_LIFECYCLE;
    if(!hostedControlRanges(*child,nullptr,0,1,output,sizeof(*output),alignof(RavetVst3HostedAudioInfo),report)||output->struct_size!=sizeof(*output))return RAVET_VST3_INVALID;
    return hostedInvoke(*child,*report,ravet::vst3::draft::Stage::Buses,[&] {
        child->owner->refreshHostedAudio();const auto& catalogue=child->owner->hostedAudioCatalogue();
        const RavetVst3HostedAudioInfo staged{sizeof(*output),catalogue.inputCount,catalogue.outputCount,0};*output=staged;
    });
}
int32_t hostedAudioBus(const RavetVst3Instance* child,uint32_t direction,uint32_t index,RavetVst3HostedAudioBus* output,RavetVst3HostedAudioReport* report)noexcept {
    if(!child||!output||!report||direction>1)return RAVET_VST3_INVALID;
    if(!control(child->parent))return RAVET_VST3_THREAD;
    ControlScope scope(*child->parent);if(!scope.acquired)return RAVET_VST3_LIFECYCLE;
    if(!hostedControlRanges(*child,nullptr,0,1,output,sizeof(*output),alignof(RavetVst3HostedAudioBus),report)||output->struct_size!=sizeof(*output))return RAVET_VST3_INVALID;
    return hostedInvoke(*const_cast<RavetVst3Instance*>(child),*report,ravet::vst3::draft::Stage::Buses,[&] {
        const auto& catalogue=child->owner->hostedAudioCatalogue();if(index>=catalogue.count(bool(direction)))throw ravet::vst3::draft::InstanceFailure{ravet::vst3::draft::Stage::Profile,0};
        *output=catalogue.bus(bool(direction),index);
    });
}
int32_t prepareHostedAudio(RavetVst3Instance* child,const RavetVst3HostedAudioProfile* requested,RavetVst3HostedAudioReport* report)noexcept {
    if(!child||!requested||!report)return RAVET_VST3_INVALID;
    if(!control(child->parent))return RAVET_VST3_THREAD;
    ControlScope scope(*child->parent);if(!scope.acquired)return RAVET_VST3_LIFECYCLE;
    if(!hostedControlRanges(*child,requested,sizeof(*requested),alignof(RavetVst3HostedAudioProfile),nullptr,0,1,report))return RAVET_VST3_INVALID;
    const auto copied=*requested;
    return hostedInvoke(*child,*report,ravet::vst3::draft::Stage::Profile,[&] {
        (void)child->owner->preflightHostedAudio(copied); // before lazy keeper allocation
        child->owner->prepareHostedAudio(copied,child->parent->core->parameterPools(),child->parent->core->processingPools(),*child->parent->core);
    });
}
int32_t hostedTiming(const RavetVst3Instance* child,RavetVst3HostedAudioTiming* output,RavetVst3HostedAudioReport* report)noexcept {
    if(!child||!output||!report)return RAVET_VST3_INVALID;
    if(!control(child->parent))return RAVET_VST3_THREAD;
    ControlScope scope(*child->parent);if(!scope.acquired)return RAVET_VST3_LIFECYCLE;
    if(!hostedControlRanges(*child,nullptr,0,1,output,sizeof(*output),alignof(RavetVst3HostedAudioTiming),report)||output->struct_size!=sizeof(*output))return RAVET_VST3_INVALID;
    return hostedInvoke(*const_cast<RavetVst3Instance*>(child),*report,ravet::vst3::draft::Stage::Latency,[&]{*output=child->owner->hostedTiming();});
}
template<class Sample,class Block,class Boundary> int32_t processHosted(RavetVst3Audio* audio,const Block* block,
        const RavetVst3ParameterPoint* points,uint32_t pointCount,const RavetVst3EventRecord* events,uint32_t eventCount,
        const uint8_t* payload,uint32_t payloadBytes,const RavetVst3ContextRecord* context,
        RavetVst3ProcessingOutputs* outputs,RavetVst3ProcessingReport* report)noexcept {
    if(!audio||!audio->instance||!block||!outputs||!report)return RAVET_VST3_INVALID;
    using namespace ravet::vst3::processing_draft;using namespace ravet::vst3::parameters_draft;
    const auto& child=*audio->instance;auto& core=*child.parent->core;
    LiveCoreProtection::Read admission(&core.liveProtection);
    if(!admission.admitted(&core.liveProtection))return RAVET_VST3_LIFECYCLE;
    Boundary boundary;Range handle{};
    if(!range(audio,1,sizeof(*audio),alignof(RavetVst3Audio),handle)||
       !boundary.admit(*block,points,pointCount,events,eventCount,payload,payloadBytes,context,*outputs,*report,
        [&](const void* p,size_t bytes,size_t alignment){Range r{};return stateExternal(child,p,bytes,alignment,true)&&range(p,bytes,1,alignment,r)&&!overlaps(r,handle);}))return RAVET_VST3_INVALID;
    Boundary protection;
    auto append=[&](const void* p,size_t bytes,size_t alignment){Range r{};return range(p,bytes,1,alignment,r)&&protection.append(r);};
    if(!append(audio,sizeof(*audio),alignof(RavetVst3Audio))||!append(&child,sizeof(child),alignof(RavetVst3Instance))||
       !append(child.parent,sizeof(*child.parent),alignof(RavetVst3Module))||!append(&core,sizeof(core),alignof(NativeCore))||
       (core.parameterKeepers&&!core.parameterKeepers->appendProtectionRanges(append))||
       (core.processingKeepers&&!core.processingKeepers->appendProtectionRanges(append))||
       (core.stateKeepers&&!core.stateKeepers->appendProtectionRanges(append)))return RAVET_VST3_CAPACITY;
    return child.owner->processHostedBlock<Sample,Block,Boundary>(*block,points,pointCount,events,eventCount,payload,payloadBytes,context,*outputs,*report,protection.ranges(),protection.count());
}
const RavetVst3HostedAudioProviderV1 hostedAudioProvider{sizeof(RavetVst3HostedAudioProviderV1),1,&providerV2,&processingProvider,
    refreshHostedAudio,hostedAudioBus,prepareHostedAudio,hostedTiming,
    processHosted<float,RavetVst3AudioBlock,ravet::vst3::processing_draft::HostedBoundary32>,
    processHosted<double,RavetVst3HostedAudioBlock64,ravet::vst3::processing_draft::HostedBoundary64>};

// Editor companion. Entries run on MAIN, hold the module's controlBusy flag
// (so reentrant editor/instance CONTROL calls from view callbacks are refused)
// but never take the live-core CONTROL admission, so a sibling AUDIO scope can
// stay active. Caller records are checked against the editor/child/module/core
// objects and each other; caller liveness remains trust.
struct EditorScope {
    RavetVst3Module* parent=nullptr;int32_t status=RAVET_VST3_OK;bool held=false;
    explicit EditorScope(const RavetVst3Instance& child)noexcept:parent(child.parent) {
        if(!control(parent)){status=RAVET_VST3_THREAD;return;}
        if(parent->controlBusy){status=RAVET_VST3_EDITOR_REENTRY;return;}
        parent->controlBusy=true;held=true;
    }
    ~EditorScope(){if(held)parent->controlBusy=false;}
    EditorScope(const EditorScope&)=delete;EditorScope& operator=(const EditorScope&)=delete;
};
struct EditorRecord {const void* pointer;size_t bytes,alignment;};
bool editorRecords(const RavetVst3Instance& child,const RavetVst3Editor* editor,const EditorRecord* records,size_t recordCount)noexcept {
    using Owner=ravet::vst3::draft::Instance;
    std::array<Owner::Range,5> owned{};size_t count=0;
    if(!Owner::range(&child,1,sizeof(child),alignof(RavetVst3Instance),owned[count++])||
       !Owner::range(child.parent,1,sizeof(*child.parent),alignof(RavetVst3Module),owned[count++])||
       !Owner::range(child.parent->core.get(),1,sizeof(NativeCore),alignof(NativeCore),owned[count++]))return false;
    if(editor){
        if(!Owner::range(editor,1,sizeof(*editor),alignof(RavetVst3Editor),owned[count++]))return false;
        if(editor->owner&&!Owner::range(editor->owner.get(),1,sizeof(ravet::vst3::editor_draft::Owner),alignof(ravet::vst3::editor_draft::Owner),owned[count++]))return false;
    }
    std::array<Owner::Range,4> requested{};size_t used=0;
    for(size_t index=0;index<recordCount;++index){
        const auto& record=records[index];
        if(used==requested.size()||!Owner::range(record.pointer,1,record.bytes,record.alignment,requested[used]))return false;
        for(size_t i=0;i<count;++i)if(Owner::overlaps(requested[used],owned[i]))return false;
        for(size_t i=0;i<used;++i)if(Owner::overlaps(requested[used],requested[i]))return false;
        ++used;
    }
    return true;
}
bool editorRecords(const RavetVst3Instance& child,const RavetVst3Editor* editor,std::initializer_list<EditorRecord> records)noexcept {
    return editorRecords(child,editor,records.begin(),records.size());
}
void editorReport(RavetVst3EditorReport& report,const ravet::vst3::editor_draft::Outcome& outcome)noexcept {
    RavetVst3EditorReport staged{};staged.struct_size=sizeof(staged);staged.stage=uint32_t(outcome.stage);
    staged.failure=uint32_t(outcome.failure);staged.flags=outcome.flags;
    staged.native_result=outcome.failure==ravet::vst3::editor_draft::Failure::Returned?outcome.native:0;
    report=staged;
}
void editorSize(RavetVst3EditorSize& output,ravet::vst3::editor_draft::Extent extent)noexcept {
    const RavetVst3EditorSize staged{sizeof(staged),extent.width,extent.height,0};output=staged;
}
// Held around an idle plug-in frame resize so nested CONTROL entries from that
// onSize are refused exactly as inside an editor entry.
bool editorGate(void* context,bool enter)noexcept {
    auto* parent=static_cast<RavetVst3Module*>(context);
    if(!enter){parent->controlBusy=false;return true;}
    if(!control(parent)||parent->controlBusy)return false;
    parent->controlBusy=true;return true;
}
bool editorValid(const RavetVst3Editor* editor)noexcept {
    return editor&&editor->instance&&editor->instance->editor==editor&&editor->owner;
}
int32_t createEditor(RavetVst3Instance* child,uint32_t platform,RavetVst3Editor** output,RavetVst3EditorSize* initial,RavetVst3EditorReport* report)noexcept {
    using namespace ravet::vst3::editor_draft;
    if(!child||!child->parent||!child->owner||!output||!initial||!report)return RAVET_VST3_INVALID;
    EditorScope scope(*child);if(scope.status)return scope.status;
    if(!editorRecords(*child,nullptr,{{output,sizeof(*output),alignof(RavetVst3Editor*)},{initial,sizeof(*initial),alignof(RavetVst3EditorSize)},
        {report,sizeof(*report),alignof(RavetVst3EditorReport)}})||*output||initial->struct_size!=sizeof(*initial)||report->struct_size!=sizeof(*report))
        return RAVET_VST3_INVALID;
    if(platform!=RAVET_VST3_EDITOR_PLATFORM_NSVIEW){editorReport(*report,local(Stage::Admission,Failure::Local,RAVET_VST3_INVALID));return RAVET_VST3_INVALID;}
    if(child->editor){editorReport(*report,local(Stage::Admission,Failure::Reentry,RAVET_VST3_LIFECYCLE));return RAVET_VST3_LIFECYCLE;}
    auto* controller=child->owner->editController();
    if(!controller){const auto outcome=local(Stage::Admission,Failure::Unavailable,RAVET_VST3_EDITOR_UNAVAILABLE);editorReport(*report,outcome);return outcome.status;}
    std::unique_ptr<RavetVst3Editor> wrapper;
    try{wrapper=std::make_unique<RavetVst3Editor>();}
    catch(...){editorReport(*report,local(Stage::Admission,Failure::Allocation,RAVET_VST3_ALLOCATION));return RAVET_VST3_ALLOCATION;}
    Extent extent;Outcome outcome;
    auto owner=Owner::create(*controller,extent,outcome);
    editorReport(*report,outcome);
    if(!owner)return outcome.status==RAVET_VST3_OK?RAVET_VST3_NATIVE:outcome.status;
    owner->gate(editorGate,child->parent);
    wrapper->instance=child;wrapper->owner=std::move(owner);child->editor=wrapper.get();
    editorSize(*initial,extent);*output=wrapper.release();
    return RAVET_VST3_OK;
}
template<class Operation> int32_t editorInvoke(RavetVst3Editor* editor,RavetVst3EditorReport* report,
        std::initializer_list<EditorRecord> outputs,Operation operation)noexcept {
    if(!editorValid(editor)||!report)return RAVET_VST3_INVALID;
    EditorScope scope(*editor->instance);if(scope.status)return scope.status;
    std::array<EditorRecord,3> records{};size_t count=0;
    records[count++]={report,sizeof(*report),alignof(RavetVst3EditorReport)};
    for(const auto& record:outputs)if(count<records.size())records[count++]=record;
    if(outputs.size()>2||!editorRecords(*editor->instance,editor,records.data(),count)||report->struct_size!=sizeof(*report))return RAVET_VST3_INVALID;
    const auto outcome=operation(*editor->owner);
    editorReport(*report,outcome);
    return outcome.status;
}
int32_t attachEditor(RavetVst3Editor* editor,void* parent,RavetVst3EditorSize* applied,RavetVst3EditorReport* report)noexcept {
    if(!applied||applied->struct_size!=sizeof(*applied))return RAVET_VST3_INVALID;
    return editorInvoke(editor,report,{{applied,sizeof(*applied),alignof(RavetVst3EditorSize)}},[&](ravet::vst3::editor_draft::Owner& owner){
        ravet::vst3::editor_draft::Extent extent=owner.current();const auto outcome=owner.attach(parent,extent);
        if(outcome.ok())editorSize(*applied,extent);return outcome;});
}
int32_t editorGetSize(RavetVst3Editor* editor,RavetVst3EditorSize* output,RavetVst3EditorReport* report)noexcept {
    if(!output||output->struct_size!=sizeof(*output))return RAVET_VST3_INVALID;
    return editorInvoke(editor,report,{{output,sizeof(*output),alignof(RavetVst3EditorSize)}},[&](ravet::vst3::editor_draft::Owner& owner){
        ravet::vst3::editor_draft::Extent extent;const auto outcome=owner.size(extent);
        if(outcome.ok())editorSize(*output,extent);return outcome;});
}
int32_t resizeEditor(RavetVst3Editor* editor,const RavetVst3EditorSize* proposed,RavetVst3EditorSize* applied,RavetVst3EditorReport* report)noexcept {
    if(!proposed||!applied||proposed->struct_size!=sizeof(*proposed)||proposed->reserved||applied->struct_size!=sizeof(*applied))return RAVET_VST3_INVALID;
    return editorInvoke(editor,report,{{proposed,sizeof(*proposed),alignof(RavetVst3EditorSize)},{applied,sizeof(*applied),alignof(RavetVst3EditorSize)}},
        [&](ravet::vst3::editor_draft::Owner& owner){
            const ravet::vst3::editor_draft::Extent copied{proposed->width,proposed->height};
            ravet::vst3::editor_draft::Extent extent;const auto outcome=owner.resize(copied,extent);
            if(outcome.ok())editorSize(*applied,extent);return outcome;});
}
int32_t scaleEditor(RavetVst3Editor* editor,float factor,RavetVst3EditorSize* applied,RavetVst3EditorReport* report)noexcept {
    if(!applied||applied->struct_size!=sizeof(*applied))return RAVET_VST3_INVALID;
    return editorInvoke(editor,report,{{applied,sizeof(*applied),alignof(RavetVst3EditorSize)}},[&](ravet::vst3::editor_draft::Owner& owner){
        ravet::vst3::editor_draft::Extent extent=owner.current();const auto outcome=owner.scale(factor,extent);
        if(outcome.ok())editorSize(*applied,extent);return outcome;});
}
int32_t detachEditor(RavetVst3Editor* editor,RavetVst3EditorReport* report)noexcept {
    return editorInvoke(editor,report,{},[](ravet::vst3::editor_draft::Owner& owner){return owner.detach();});
}
int32_t editorState(const RavetVst3Editor* editor,RavetVst3EditorState* output)noexcept {
    if(!editorValid(editor)||!output)return RAVET_VST3_INVALID;
    if(!control(editor->instance->parent))return RAVET_VST3_THREAD;
    if(!editorRecords(*editor->instance,editor,{{output,sizeof(*output),alignof(RavetVst3EditorState)}})||output->struct_size!=sizeof(*output))return RAVET_VST3_INVALID;
    *output=editor->owner->state();return RAVET_VST3_OK;
}
int32_t takeEditorRequest(RavetVst3Editor* editor,RavetVst3EditorSize* output)noexcept {
    if(!editorValid(editor)||!output)return RAVET_VST3_INVALID;
    if(!control(editor->instance->parent))return RAVET_VST3_THREAD;
    if(!editorRecords(*editor->instance,editor,{{output,sizeof(*output),alignof(RavetVst3EditorSize)}})||output->struct_size!=sizeof(*output))return RAVET_VST3_INVALID;
    ravet::vst3::editor_draft::Extent extent;
    if(!editor->owner->takeRequest(extent))return RAVET_VST3_EDITOR_EMPTY;
    editorSize(*output,extent);return RAVET_VST3_OK;
}
int32_t destroyEditor(RavetVst3Editor* editor,RavetVst3EditorReport* report)noexcept {
    if(!editorValid(editor)||!report)return RAVET_VST3_INVALID;
    auto* child=editor->instance;
    {
        EditorScope scope(*child);if(scope.status)return scope.status;
        if(!editorRecords(*child,editor,{{report,sizeof(*report),alignof(RavetVst3EditorReport)}})||report->struct_size!=sizeof(*report))return RAVET_VST3_INVALID;
        if(editor->owner->busy())return RAVET_VST3_EDITOR_REENTRY;
        const auto outcome=editor->owner->retire();
        editorReport(*report,outcome);
        child->editor=nullptr;editor->owner.reset();delete editor;
        return outcome.ok()?RAVET_VST3_OK:RAVET_VST3_RETIRED_NATIVE;
    }
}
const RavetVst3EditorProviderV1 editorProvider{sizeof(RavetVst3EditorProviderV1),1,&providerV2,
    createEditor,attachEditor,editorGetSize,resizeEditor,scaleEditor,detachEditor,editorState,takeEditorRequest,destroyEditor};

}
extern "C" const RavetVst3Provider* ravet_vst3_provider_v1() { return &provider; }

extern "C" const RavetVst3ProviderV2* ravet_vst3_provider_v2() {return &providerV2;}
extern "C" const RavetVst3StateProviderV1* ravet_vst3_state_provider_v1() {return &stateProvider;}
extern "C" const RavetVst3ParameterProviderV1* ravet_vst3_parameter_provider_v1() {return &parameterProvider;}

extern "C" const RavetVst3ProcessingProviderV1* ravet_vst3_processing_provider_v1() {return &processingProvider;}

extern "C" const RavetVst3HostedAudioProviderV1* ravet_vst3_hosted_audio_provider_v1() {return &hostedAudioProvider;}

extern "C" const RavetVst3EditorProviderV1* ravet_vst3_editor_provider_v1() {return &editorProvider;}
