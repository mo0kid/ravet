#include "RavetVst3State.h"
#include "state-fixture.hpp"
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
    char directory[]="/tmp/ravet-vst3-state-retain-XXXXXX";require(mkdtemp(directory));const std::filesystem::path root(directory);
    auto* audio=ravet_vst3_provider_v2();auto* state=ravet_vst3_state_provider_v1();require(state->provider==audio);
    for(int mode=0;mode<4;++mode) {
        // Pinned fixture module/factory/state/host follow process lifetime.
        auto* factory=new RetainedFactory;factory->state.controllerMode=1;
        const auto path=root/(std::to_string(mode)+".vst3");std::filesystem::create_directory(path);
        const auto alias=root/("alias-"+std::to_string(mode)+".vst3");std::filesystem::create_directory_symlink(path,alias);
        const auto name=path.string(),other=alias.string();modules[std::filesystem::canonical(path).string()]=factory;
        int32_t native=777;RavetVst3Module* parent=nullptr;RavetVst3Instance* child=nullptr;
        require(audio->open(reinterpret_cast<const uint8_t*>(name.data()),uint32_t(name.size()),&parent,&native)==RAVET_VST3_OK);
        require(audio->create(parent,0,&child,&native)==RAVET_VST3_OK);
        factory->state.retaining=mode!=0;factory->state.failState=mode>=2?1:0;factory->state.throwState=mode==3;
        RavetVst3StateRecord record{};record.struct_size=sizeof(record);RavetVst3StateReport report{};report.struct_size=sizeof(report);
        std::array<uint8_t,8> component{},controller{};
        require(state->capture(child,&record,component.data(),8,controller.data(),8,&report)==(mode>=2?RAVET_VST3_NATIVE:RAVET_VST3_OK));
        if(mode>=2)require(report.stage==1&&report.native_result==(mode==3?0:rejected));
        require(audio->destroy(child,&native)==RAVET_VST3_OK);child=nullptr;
        require(audio->close(parent,&native)==(mode?RAVET_VST3_PROCESS_PINNED:RAVET_VST3_OK));parent=nullptr;
        if(!mode) {
            require(factory->entries==1&&factory->exits==1&&factory->cleared==1&&!factory->host&&factory->references==1);
            delete factory;
        } else {
            require(factory->entries==1&&factory->exits==0&&factory->cleared==0&&factory->installed==1&&factory->references==3);factory->probe();
            for(auto* stream:factory->state.retained){int64 value=777;require(stream->tell(&value)==kInvalidArgument&&value==777);}
            require(ravet_vst3_provider_v1()->open(reinterpret_cast<const uint8_t*>(other.data()),uint32_t(other.size()),&parent,&native)==RAVET_VST3_CAPACITY&&!parent);
            auto references=std::move(factory->state.retained);std::thread release([&]{for(auto* stream:references)require(stream->release()==1);});release.join();
            require(audio->open(reinterpret_cast<const uint8_t*>(other.data()),uint32_t(other.size()),&parent,&native)==RAVET_VST3_OK);
            require(factory->entries==1&&factory->installed==1&&factory->exits==0);
            factory->state.retaining=false;factory->state.failState=0;factory->state.throwState=false;
            require(audio->create(parent,0,&child,&native)==RAVET_VST3_OK);
            require(state->capture(child,&record,component.data(),8,controller.data(),8,&report)==RAVET_VST3_OK);
            require(audio->destroy(child,&native)==RAVET_VST3_OK&&audio->close(parent,&native)==RAVET_VST3_PROCESS_PINNED);
            require(factory->entries==1&&factory->installed==1&&factory->exits==0&&factory->cleared==0);factory->probe();
        }
    }
    std::filesystem::remove_all(root);
    std::cout<<"PASS ordinary module teardown and state-retention forced process pin after successful/rejected/thrown getter, live host/context/module before foreign worker release, canonical same-context reopen/no repeat entry, v1 exclusion and continued state IO\n";
}catch(const std::exception& error){std::cerr<<error.what()<<'\n';return 1;}}
