#include "provider.cpp"
#include "RavetVst3State.h"
#include "processing-instance-fixture.hpp"
#include "public.sdk/source/vst/hosting/module.h"
#include "pluginterfaces/vst/ivsthostapplication.h"
#include <filesystem>
#include <map>
#include <unistd.h>
namespace {
struct RetainedFactory final:IPluginFactory3 {
    Observation state;
    Factory authored{state};
    uint32_t references=1,entries=0,exits=0,installed=0,cleared=0;
    FUnknown* host=nullptr;
    FUnknown* borrowedHost=nullptr;
    void probe(){require(borrowedHost);void* result=nullptr;require(borrowedHost->queryInterface(IHostApplication::iid,&result)==kResultOk&&result);static_cast<IHostApplication*>(result)->release();}
    tresult PLUGIN_API queryInterface(const TUID id,void** output)override {
        *output=nullptr;
        if(FUnknownPrivate::iidEqual(id,FUnknown::iid)||FUnknownPrivate::iidEqual(id,IPluginFactory::iid)||
           FUnknownPrivate::iidEqual(id,IPluginFactory2::iid)||FUnknownPrivate::iidEqual(id,IPluginFactory3::iid)) {
            *output=static_cast<IPluginFactory3*>(this);addRef();return kResultOk;
        }return kNoInterface;
    }
    uint32 PLUGIN_API addRef()override{return ++references;}
    uint32 PLUGIN_API release()override{return --references;}
    tresult PLUGIN_API getFactoryInfo(PFactoryInfo* out)override{*out={};std::strcpy(out->vendor,"RAVET");return kResultOk;}
    int32 PLUGIN_API countClasses()override{return 2;}
    tresult PLUGIN_API getClassInfo(int32 index,PClassInfo* out)override {
        *out={};out->cid[0]=char(index+1);out->cardinality=1;
        std::strcpy(out->category,index?"Component Controller Class":"Audio Module Class");std::strcpy(out->name,"State stream retention");return kResultOk;
    }
    tresult PLUGIN_API getClassInfo2(int32,PClassInfo2*)override{return kNotImplemented;}
    tresult PLUGIN_API getClassInfoUnicode(int32 index,PClassInfoW* out)override {
        *out={};out->cid[0]=char(index+1);out->cardinality=1;
        std::strcpy(out->category,index?"Component Controller Class":"Audio Module Class");out->name[0]='s';return kResultOk;
    }
    Observation* nextObservation=nullptr;
    tresult PLUGIN_API createInstance(FIDString id,FIDString iid,void** output)override{
        if(nextObservation){Factory chosen(*nextObservation);return chosen.createInstance(id,iid,output);}
        return authored.createInstance(id,iid,output);
    }
    tresult PLUGIN_API setHostContext(FUnknown* value)override {
        if(value){require(!host);host=value;borrowedHost=value;host->addRef();++installed;probe();}
        else{require(host);probe();host->release();host=nullptr;++cleared;}
        return kNotImplemented;
    }
};
std::map<std::string,RetainedFactory*> modules;
class RetainedModule final:public VST3::Hosting::Module {
    RetainedFactory& owner;
public:
    explicit RetainedModule(RetainedFactory& from):owner(from){++owner.entries;owner.addRef();factory=VST3::Hosting::PluginFactory(owned<IPluginFactory>(&owner));}
    ~RetainedModule()noexcept override{owner.probe();factory=VST3::Hosting::PluginFactory(nullptr);++owner.exits;}
    bool load(const std::string&,std::string&)override{return true;}
};
}
namespace VST3::Hosting {
Module::Ptr Module::create(const std::string& path,std::string&){return std::make_shared<RetainedModule>(*modules.at(std::filesystem::canonical(path).string()));}
}
namespace {
using namespace ravet::vst3::processing_draft;
thread_local bool measured=false;
thread_local size_t allocations=0,deallocations=0;
struct Fixture {
    const RavetVst3ProviderV2* api=ravet_vst3_provider_v2();
    const RavetVst3ProcessingProviderV1* processing=ravet_vst3_processing_provider_v1();
    const RavetVst3ParameterProviderV1* parameters=ravet_vst3_parameter_provider_v1();
    RetainedFactory* factory=new RetainedFactory;
    RavetVst3Module* module=nullptr;RavetVst3Instance* child=nullptr;RavetVst3Audio* audio=nullptr;
    std::filesystem::path path;
    int32_t native=999;
    explicit Fixture(std::function<void(RavetVst3Module&,ControllerHandler&)> hook={}){
        require(metadata().classes[0].id[0]==1);
        char directory[]="/tmp/ravet-vst3-paired-provider-XXXXXX";require(mkdtemp(directory));path=directory;
        modules[std::filesystem::canonical(path).string()]=factory;factory->state.handlerCall=[this,hook](IComponentHandler* value){if(value&&hook)hook(*module,*static_cast<ControllerHandler*>(value));};
        auto text=path.string();require(api->open(reinterpret_cast<const uint8_t*>(text.data()),uint32_t(text.size()),&module,&native)==RAVET_VST3_OK);
        require(api->create(module,0,&child,&native)==RAVET_VST3_OK);
    }
    void prepare(){
        uint32_t count=999;RavetVst3ParameterReport parameterReport{};parameterReport.struct_size=sizeof(parameterReport);
        require(parameters->refresh(child,&count,&parameterReport)==RAVET_VST3_OK);
        RavetVst3ProcessingSelection selection{sizeof(selection),2,1,0};RavetVst3ProcessingInfo info{};info.struct_size=sizeof(info);
        RavetVst3ProcessingReport report{};report.struct_size=sizeof(report);
        require(processing->refresh(child,&selection,&info,&report)==RAVET_VST3_OK);
        require(info.input_bus_count==2&&info.output_bus_count==2&&info.selected_input_mask==2&&info.selected_output_mask==1);
        RavetVst3AudioProfile profile{sizeof(profile),16,1,1,48000};
        require(processing->prepare(child,&profile,&report)==RAVET_VST3_OK);
        require(api->start(child,&native)==RAVET_VST3_OK&&api->begin_audio(child,&audio,&native)==RAVET_VST3_OK);
    }
    ~Fixture(){
        if(audio)require(api->end_audio(audio,&native)==RAVET_VST3_OK);
        if(child)require(api->destroy(child,&native)==RAVET_VST3_OK);
        const auto result=module?api->close(module,&native):RAVET_VST3_PROCESS_PINNED;require(result==RAVET_VST3_OK||result==RAVET_VST3_PROCESS_PINNED);
        if(result==RAVET_VST3_OK)delete factory; // Pinned core borrows process-lifetime fixture factory.
        modules.erase(std::filesystem::canonical(path).string());std::filesystem::remove_all(path);
    }
};
void healthyEarlyAndAliases(){
    ControllerHandler* handler=nullptr;
    Fixture f([&](RavetVst3Module& module,ControllerHandler& installed){
        handler=&installed;require(module.children==0&&module.core->handlerKeepers.count()==1);
        auto* provisional=*std::find_if(module.core->childRegistry.begin(),module.core->childRegistry.end(),[](auto* value){return value;});
        require(provisional&&provisional->owner&&module.core->liveProtection.currentControl());
        void* output=reinterpret_cast<void*>(uintptr_t(0x1110));const auto refs=installed.references();
        require(installed.queryInterface(IComponentHandler::iid,&output)==kResultOk&&output==&installed&&installed.references()==refs+1);
        require(installed.release()==refs);
        const auto reject=[&](const void* p,size_t bytes){
            if(bytes<sizeof(void*)||reinterpret_cast<uintptr_t>(p)%alignof(void*))return;
            std::array<uint8_t,sizeof(void*)> old{};std::memcpy(old.data(),p,old.size());
            require(installed.queryInterface(IComponentHandler::iid,reinterpret_cast<void**>(const_cast<void*>(p)))==kInvalidArgument);
            require(std::memcmp(old.data(),p,old.size())==0&&installed.references()==refs);
        };
        reject(provisional,sizeof(*provisional));reject(provisional->owner.get(),sizeof(Instance));reject(&module,sizeof(module));
        reject(module.core.get(),sizeof(NativeCore));reject(&module.core->handlerKeepers,sizeof(module.core->handlerKeepers));reject(&installed,sizeof(installed));
        reject(module.core->catalogue.classes.data(),module.core->catalogue.classes.capacity()*sizeof(Class));
        output=reinterpret_cast<void*>(uintptr_t(0x2220));require(installed.queryInterface(reinterpret_cast<const char*>(&installed),&output)==kInvalidArgument&&output==reinterpret_cast<void*>(uintptr_t(0x2220))&&installed.references()==refs);
        alignas(void*) std::array<char,16> overlap{};std::memcpy(overlap.data(),IComponentHandler::iid,16);const auto before=overlap;
        require(installed.queryInterface(overlap.data(),reinterpret_cast<void**>(overlap.data()))==kInvalidArgument&&overlap==before&&installed.references()==refs);
        bool rejected=false;std::thread worker([&]{void* out=reinterpret_cast<void*>(uintptr_t(0x3330));rejected=installed.queryInterface(IComponentHandler::iid,&out)==kInvalidArgument&&out==reinterpret_cast<void*>(uintptr_t(0x3330));});worker.join();require(rejected&&installed.references()==refs);
    });
    require(handler&&!f.module->core->handlerKeepers.observedRetention());
    f.factory->state.handlerCall={};f.prepare();require(f.api->end_audio(f.audio,&f.native)==RAVET_VST3_OK);f.audio=nullptr;
    require(f.api->stop(f.child,&f.native)==RAVET_VST3_OK);
    // Every full scratch/catalogue capacity is refused, including suffixes.
    const auto refs=handler->references();
    require(f.child->owner->appendOwnedRanges([&](const void* p,size_t bytes,size_t alignment){
        (void)alignment;if(bytes<sizeof(void*)||reinterpret_cast<uintptr_t>(p)%alignof(void*))return true;
        std::array<uint8_t,sizeof(void*)> old{};std::memcpy(old.data(),p,old.size());
        require(handler->queryInterface(IComponentHandler::iid,reinterpret_cast<void**>(const_cast<void*>(p)))==kInvalidArgument&&std::memcmp(old.data(),p,old.size())==0&&handler->references()==refs);return true;
    }));
    RavetVst3Instance* sibling=nullptr;require(f.api->create(f.module,0,&sibling,&f.native)==RAVET_VST3_OK);
    for(const auto* pointer:{static_cast<const void*>(sibling),static_cast<const void*>(sibling->owner.get())}){
        std::array<uint8_t,sizeof(void*)> before{};std::memcpy(before.data(),pointer,before.size());
        require(handler->queryInterface(IComponentHandler::iid,reinterpret_cast<void**>(const_cast<void*>(pointer)))==kInvalidArgument&&std::memcmp(before.data(),pointer,before.size())==0&&handler->references()==refs);
    }
    require(f.api->destroy(sibling,&f.native)==RAVET_VST3_OK);
    void* output=nullptr;require(handler->queryInterface(IComponentHandler::iid,&output)==kResultOk&&handler->release()==refs);
}
void provisionalFailureAndRecovery(){
    Fixture f;f.factory->state.handlerCall={};
    for(int phase=0;phase<4;++phase){
        Observation state;ControllerHandler* captured=nullptr;
        state.throwCreate=phase==0;state.fail=phase==1?"handler":"";state.throwCall=phase==2?"handler":"";
        state.controlCall=[&](const char* call){if(std::strcmp(call,"create")==0)require(std::count_if(f.module->core->childRegistry.begin(),f.module->core->childRegistry.end(),[](auto* p){return p;})==2);};
        state.handlerCall=[&](IComponentHandler* raw){if(raw){captured=static_cast<ControllerHandler*>(raw);captured->addRef();void* out=nullptr;require(captured->queryInterface(IComponentHandler::iid,&out)==kResultOk);static_cast<IComponentHandler*>(out)->release();}};
        f.factory->nextObservation=&state;RavetVst3Instance* child=nullptr;int32_t native=999;
        const auto result=f.api->create(f.module,0,&child,&native);
        if(phase<3){require(result==RAVET_VST3_NATIVE&&!child&&f.module->children==1&&state.destroyed==1&&std::count_if(f.module->core->childRegistry.begin(),f.module->core->childRegistry.end(),[](auto* p){return p;})==1);}
        else{require(result==RAVET_VST3_OK&&child&&f.module->children==2);require(f.api->destroy(child,&native)==RAVET_VST3_OK);}
        if(captured){require(captured->retired()&&captured->restartComponent(kIoChanged)==kInvalidArgument&&f.module->core->handlerKeepers.observedRetention());
            bool released=false;std::thread worker([&]{released=captured->release()==1;});worker.join();require(released);}
        f.factory->nextObservation=nullptr;
        {ControlScope scope(*f.module);require(scope.acquired&&f.module->core->handlerKeepers.count()==1);}
    }
}
void retainedRetiredCoreReopenAndWorkerRelease(){
    ControllerHandler* retained=nullptr;Fixture f([&](RavetVst3Module&,ControllerHandler& handler){retained=&handler;handler.addRef();});
    f.factory->state.handlerCall={};const auto core=f.module->core;
    require(f.api->destroy(f.child,&f.native)==RAVET_VST3_OK);f.child=nullptr;
    require(retained&&retained->references()==2&&retained->retired()&&retained->restartComponent(kIoChanged)==kInvalidArgument&&core->handlerKeepers.observedRetention());
    require(f.api->close(f.module,&f.native)==RAVET_VST3_PROCESS_PINNED);f.module=nullptr;require(f.factory->exits==0);
    void* output=nullptr;require(retained->queryInterface(IComponentHandler::iid,&output)==kResultOk&&output==retained&&retained->release()==2);
    auto path=f.path.string();require(f.api->open(reinterpret_cast<const uint8_t*>(path.data()),uint32_t(path.size()),&f.module,&f.native)==RAVET_VST3_OK&&f.module->core.get()==core.get()&&f.factory->entries==1);
    f.factory->state.handlerCall=[&](IComponentHandler* raw){if(raw){void* out=reinterpret_cast<void*>(uintptr_t(0x4440));require(retained->queryInterface(IComponentHandler::iid,&out)==kInvalidArgument&&out==reinterpret_cast<void*>(uintptr_t(0x4440))&&retained->references()==2);}};
    require(f.api->create(f.module,0,&f.child,&f.native)==RAVET_VST3_OK);f.factory->state.handlerCall={};
    size_t frees=999;bool released=false;
    std::thread worker([&]{allocations=deallocations=0;measured=true;released=retained->release()==1;measured=false;frees=deallocations;});worker.join();
    require(released&&frees==0&&core->handlerKeepers.count()==2&&f.factory->exits==0);
    {ControlScope scope(*f.module);require(scope.acquired&&core->handlerKeepers.count()==1);}
}
void keeperCapacityActualProvider(){
    std::vector<ControllerHandler*> retained;retained.reserve(256);
    Fixture f([&](RavetVst3Module&,ControllerHandler& handler){handler.addRef();retained.push_back(&handler);});
    for(size_t i=0;i<256;++i){
        require(f.api->destroy(f.child,&f.native)==RAVET_VST3_OK);f.child=nullptr;
        if(i<255)require(f.api->create(f.module,0,&f.child,&f.native)==RAVET_VST3_OK);
    }
    require(retained.size()==256&&f.module->children==0&&f.module->core->handlerKeepers.count()==256);
    const auto creates=f.factory->state.count("create");f.native=777;
    require(f.api->create(f.module,0,&f.child,&f.native)==RAVET_VST3_CAPACITY&&!f.child&&f.native==777&&f.factory->state.count("create")==creates);
    bool released=true;std::thread worker([&]{for(auto* handler:retained)released=released&&handler->release()==1;});worker.join();require(released);
    f.factory->state.handlerCall={};require(f.api->create(f.module,0,&f.child,&f.native)==RAVET_VST3_OK&&f.module->core->handlerKeepers.count()==1);
    require(MaximumLiveRanges==11333&&MaximumLiveRanges<parameters_draft::LiveCoreProtection::MaximumRanges);
}
void concurrentQueryPublication(){
    ControllerHandler* handler=nullptr;Fixture f([&](RavetVst3Module&,ControllerHandler& value){handler=&value;value.addRef();});
    f.factory->state.handlerCall={};const auto refs=handler->references();std::atomic<bool> start{false};size_t accepted=0,rejectedQueries=0;
    std::thread worker([&]{while(!start.load(std::memory_order_acquire))std::this_thread::yield();
        for(size_t i=0;i<10000;++i){void* output=reinterpret_cast<void*>(uintptr_t(0x8880));const auto result=handler->queryInterface(IComponentHandler::iid,&output);
            if(result==kResultOk){require(output==handler);handler->release();++accepted;}
            else{require(result==kInvalidArgument&&output==reinterpret_cast<void*>(uintptr_t(0x8880)));++rejectedQueries;}
        }
    });start.store(true,std::memory_order_release);size_t acquired=0,refused=0;
    for(size_t i=0;i<2000;++i){ControlScope scope(*f.module);if(scope.acquired)++acquired;else ++refused;}
    worker.join();require(accepted+rejectedQueries==10000&&acquired+refused==2000&&handler->references()==refs);
    require(handler->release()==refs-1);
}
void unclassifiedVsAttributedReports(){
    Fixture f;uint32_t count=999;RavetVst3ParameterReport parameter{};parameter.struct_size=sizeof(parameter);
    require(f.parameters->refresh(f.child,&count,&parameter)==RAVET_VST3_OK);
    RavetVst3ProcessingSelection selection{sizeof(selection),2,1,0};RavetVst3ProcessingInfo info{};info.struct_size=sizeof(info);RavetVst3ProcessingReport processing{};processing.struct_size=sizeof(processing);
    require(f.processing->refresh(f.child,&selection,&info,&processing)==RAVET_VST3_OK);
    for(bool invalidated:{false,true}){
        if(invalidated){f.child->owner->refreshParameters();require(!f.child->owner->processingCompatible());}
        parameter.native_result=4242;parameter.flags=RAVET_VST3_PARAMETER_RETURNED;
        require(parameterInvoke(*f.child,parameter,[]{throw 123;},4)==RAVET_VST3_METADATA&&parameter.stage==4&&parameter.failure==RAVET_VST3_PARAMETER_FAILURE_METADATA&&parameter.native_result==0&&parameter.flags==0);
        processing.native_result=4242;processing.flags=RAVET_VST3_PROCESSING_RETURNED;
        require(processingInvoke(*f.child,processing,3,[]{throw 123;})==RAVET_VST3_METADATA&&processing.stage==3&&processing.failure==16&&processing.native_result==0&&processing.flags==(invalidated?uint32_t(RAVET_VST3_PROCESSING_INVALIDATED):0));
        require(parameterInvoke(*f.child,parameter,[]{sdkCall(Stage::Setup,[]{throw 123;return kResultOk;});},4)==RAVET_VST3_NATIVE&&parameter.stage==108&&parameter.failure==RAVET_VST3_PARAMETER_FAILURE_THROWN&&parameter.native_result==0&&(parameter.flags&7)==5);
        require(processingInvoke(*f.child,processing,3,[]{sdkAccepted(Stage::Setup,[]{return rejected;});})==RAVET_VST3_NATIVE&&processing.stage==108&&processing.native_result==rejected&&(processing.flags&7)==3);
    }
}
}
void* operator new(std::size_t n){if(measured)++allocations;if(void* p=std::malloc(n))return p;throw std::bad_alloc();}
void operator delete(void* p)noexcept{if(measured&&p)++deallocations;std::free(p);}
void* operator new[](std::size_t n){return ::operator new(n);}
void operator delete[](void* p)noexcept{::operator delete(p);}
int main(){try{
    healthyEarlyAndAliases();std::cout<<"PASS1 early whole-child publication/healthy CONTROL QI/self keeper sibling class selector aliases\n";
    provisionalFailureAndRecovery();std::cout<<"PASS2 failed create/setup retained handler retirement/refusal/retry\n";
    retainedRetiredCoreReopenAndWorkerRelease();std::cout<<"PASS3 child/facade retirement exact pinned reopen/stale refusal/worker no C++free/MAIN reclaim\n";
    keeperCapacityActualProvider();std::cout<<"PASS4 actual256 retired handler owners/257 refusal before foreign callback/release recovery/exact11333 bound\n";
    concurrentQueryPublication();std::cout<<"PASS5 bounded10000 real QI writes versus2000 CONTROL publications with balanced references\n";
    unclassifiedVsAttributedReports();std::cout<<"PASS6 unclassified metadata versus attributed SDK throw/return and measured invalidation\n";
}catch(const std::exception& e){std::cerr<<e.what()<<'\n';return 1;}}
