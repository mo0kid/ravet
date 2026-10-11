#include "provider.cpp"
#include "RavetVst3State.h"
#include "processing-module-fixture.hpp"
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
    tresult PLUGIN_API createInstance(FIDString id,FIDString iid,void** output)override{return authored.createInstance(id,iid,output);}
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
int main(){try{
    using namespace ravet::vst3::processing_draft;
    char directory[]="/tmp/ravet-vst3-processing-retain-XXXXXX";
    require(mkdtemp(directory));const std::filesystem::path root(directory);
    auto* audio=ravet_vst3_provider_v2();
    for(int mode=0;mode<6;++mode) {
        auto* factory=new RetainedFactory;
        const auto path=root/(std::to_string(mode)+".vst3");std::filesystem::create_directory(path);
        const auto alias=root/("alias-"+std::to_string(mode)+".vst3");std::filesystem::create_directory_symlink(path,alias);
        const auto name=path.string(),other=alias.string();
        modules[std::filesystem::canonical(path).string()]=factory;
        int32_t native=777;RavetVst3Module* parent=nullptr;RavetVst3Instance* child=nullptr;
        require(audio->open(reinterpret_cast<const uint8_t*>(name.data()),uint32_t(name.size()),&parent,&native)==RAVET_VST3_OK);
        require(audio->create(parent,0,&child,&native)==RAVET_VST3_OK);
        auto* core=parent->core.get();std::weak_ptr<NativeCore> lifetime=parent->core;
        auto& keepers=core->processingPools();auto& pool=keepers.attach(*core);
        const uint32_t id=7;require(pool.configureParameters(&id,1));
        require(!stateExternal(*child,&keepers,sizeof(keepers),alignof(decltype(keepers))));
        require(!stateExternal(*child,&pool,sizeof(pool),alignof(EventPool)));
        EventBatch* batch=nullptr;FUnknown* retained=nullptr;
        std::thread worker([&]{
            EventBuses buses{};require(pool.prepare(32,nullptr,0,nullptr,0,buses,buses,batch)==EventAdmission::Ok);
            int32 index=-1;auto* queue=batch->outputParameters().addParameterData(id,index);
            require(queue&&queue->addPoint(3,.625,index)==kResultOk);
            if(mode==1||mode==5)retained=&batch->input();
            else if(mode==2)retained=&batch->output();
            else if(mode==3)retained=&batch->outputParameters();
            else if(mode==4)retained=queue;
            if(retained)require(retained->addRef()==2);
            require(batch->freeze()==0);
            if(mode==5){require(retained->release()==1);retained=nullptr;}
        });worker.join();
        require(pool.idle()&&pool.observedRetention()==bool(mode));
        const auto before=lifetime.use_count();size_t cleanupCalls=0;
        factory->state.controlCall=[&](const char*){
            ++cleanupCalls;
            // Observes the actual production registry's additional strong pin
            // before any component-handler/terminate cleanup callback.
            require(lifetime.use_count()==before+(mode?1:0));
            require(registry().activeReservation(core->reservation,parent->core));
            factory->probe();
        };
        require(audio->destroy(child,&native)==RAVET_VST3_OK);child=nullptr;
        factory->state.controlCall={};require(cleanupCalls>=2&&factory->state.destroyed==1);
        keepers.detach(*core,pool);
        // Centralized pin occurred before destruction; reclaim repeats the same
        // exact origin policy and refuses any live foreign-reference teardown.
        keepers.reclaim(*core,[&](NativeCore& origin){
            require(&origin==core&&registry().pin(origin.reservation,parent->core)==Registry::Result::Accepted);
        });
        require(keepers.count(*core)==(retained?1u:0u));
        require(audio->close(parent,&native)==(mode?RAVET_VST3_PROCESS_PINNED:RAVET_VST3_OK));parent=nullptr;
        if(!mode) {
            require(lifetime.expired()&&factory->entries==1&&factory->exits==1&&factory->cleared==1&&factory->references==1);
            delete factory;
        } else {
            require(!lifetime.expired()&&factory->entries==1&&factory->exits==0&&factory->cleared==0&&factory->installed==1);
            factory->probe();
            require(ravet_vst3_provider_v1()->open(reinterpret_cast<const uint8_t*>(other.data()),uint32_t(other.size()),&parent,&native)==RAVET_VST3_CAPACITY&&!parent);
            if(retained){
                std::thread release([&]{void* queried=nullptr;
                    require(retained->queryInterface(FUnknown::iid,&queried)==kResultOk&&queried);
                    require(static_cast<FUnknown*>(queried)->release()==2&&retained->release()==1);
                });release.join();retained=nullptr;
            }
            require(audio->open(reinterpret_cast<const uint8_t*>(other.data()),uint32_t(other.size()),&parent,&native)==RAVET_VST3_OK);
            require(parent->core.get()==core&&factory->entries==1&&factory->installed==1);
            keepers.reclaim(*core,[&](NativeCore& origin){
                require(&origin==core&&registry().pin(origin.reservation,parent->core)==Registry::Result::Accepted);
            });
            require(keepers.count(*core)==0&&keepers.observedRetention(*core));
            require(audio->create(parent,0,&child,&native)==RAVET_VST3_OK);
            require(audio->destroy(child,&native)==RAVET_VST3_OK&&audio->close(parent,&native)==RAVET_VST3_PROCESS_PINNED);
            require(factory->entries==1&&factory->exits==0&&factory->cleared==0);factory->probe();
        }
    }
    std::filesystem::remove_all(root);
    std::cout<<"PASS actual provider core/process-pool ownership and alias exclusion, pin before foreign component cleanup, live input/output/list/individual-queue and released-before-cleanup retention, balanced ordinary module/context exit, worker final release, same exact core canonical reopen and CONTROL reclaim\n";
}catch(const std::exception& error){std::cerr<<error.what()<<'\n';return 1;}}
