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
    explicit Fixture(bool controller=true){
        require(metadata().classes[0].id[0]==1);
        char directory[]="/tmp/ravet-vst3-paired-provider-XXXXXX";require(mkdtemp(directory));path=directory;
        modules[std::filesystem::canonical(path).string()]=factory;factory->state.hasController=controller;
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
        require(api->destroy(child,&native)==RAVET_VST3_OK);
        const auto result=api->close(module,&native);require(result==RAVET_VST3_OK||result==RAVET_VST3_PROCESS_PINNED);
        if(result==RAVET_VST3_OK)delete factory; // Pinned core borrows process-lifetime fixture factory.
        modules.erase(std::filesystem::canonical(path).string());std::filesystem::remove_all(path);
    }
};
struct Block {
    std::array<float,20> input{},output{};
    RavetVst3InputChannel in{input.data(),uint32_t(input.size()),0};
    RavetVst3OutputChannel out{output.data(),uint32_t(output.size()),0};
    RavetVst3AudioBlock block{sizeof(block),16,1,1,&in,&out};
    std::array<Record,12> events{};std::array<uint8_t,96> payload{};
    std::array<uint32_t,4> ids{};std::array<RavetVst3ParameterPoint,8> points{};
    RavetVst3ProcessingOutputs outputs{sizeof(outputs),0,events.data(),uint32_t(events.size()),777,
        payload.data(),uint32_t(payload.size()),778,ids.data(),uint32_t(ids.size()),779,
        points.data(),uint32_t(points.size()),780};
    RavetVst3ProcessingReport report{};
    Block(){input.fill(.75f);output.fill(9);payload.fill(0xa5);ids.fill(0xcccccccc);report.struct_size=sizeof(report);report.index=888;}
    int32_t call(Fixture& f,const RavetVst3ParameterPoint* parameters=nullptr,uint32_t parameterCount=0,
            const Record* inputEvents=nullptr,uint32_t eventCount=0,const uint8_t* bytes=nullptr,uint32_t byteCount=0,
            const RavetVst3ContextRecord* context=nullptr){
        return f.processing->process(f.audio,&block,parameters,parameterCount,inputEvents,eventCount,bytes,byteCount,context,&outputs,&report);
    }
};

void siblingCompleteRanges() {
    Fixture f;
    Observation siblingState; siblingState.longEventNames=true;
    f.factory->nextObservation=&siblingState;
    RavetVst3Instance* sibling=nullptr;
    require(f.api->create(f.module,0,&sibling,&f.native)==RAVET_VST3_OK);
    f.factory->nextObservation=nullptr;
    uint32_t count=0;RavetVst3ParameterReport pr{};pr.struct_size=sizeof(pr);
    require(f.parameters->refresh(sibling,&count,&pr)==RAVET_VST3_OK);
    RavetVst3ProcessingSelection selection{sizeof(selection),2,1,0};RavetVst3ProcessingInfo info{};info.struct_size=sizeof(info);
    RavetVst3ProcessingReport r{};r.struct_size=sizeof(r);
    require(f.processing->refresh(sibling,&selection,&info,&r)==RAVET_VST3_OK);
    RavetVst3AudioProfile profile{sizeof(profile),32,1,1,48000};
    require(f.processing->prepare(sibling,&profile,&r)==RAVET_VST3_OK);
    std::array<ravet::vst3::parameters_draft::Range,64> owned{};size_t used=0;
    auto collect=[&](const void* p,size_t n,size_t a){auto& x=owned[used++];return ravet::vst3::parameters_draft::range(p,n,1,a,x);};
    require(collect(sibling,sizeof(*sibling),alignof(RavetVst3Instance))&&sibling->owner->appendOwnedRanges(collect));
    require(used<=42&&f.module->core->liveProtection.count()<MaximumLiveRanges);
    f.prepare();Block b;const auto original=b.outputs;const auto originalReport=b.report;
    std::thread worker([&]{
        for(size_t i=0;i<used;++i)if(owned[i].end>owned[i].start){
            auto* p=reinterpret_cast<uint8_t*>(owned[i].start);const auto first=*p;
            b.outputs.payload=p;b.outputs.payload_capacity=1;
            require(b.call(f)==RAVET_VST3_INVALID&&*p==first&&f.factory->state.processes==0);
            require(std::memcmp(&b.report,&originalReport,sizeof(originalReport))==0);
        }
        b.outputs=original;
        f.factory->state.processing=[&](ProcessData& data){
            for(size_t i=0;i<used;++i)if(owned[i].end-owned[i].start>=sizeof(void*)&&owned[i].start%alignof(void*)==0){
                auto* p=reinterpret_cast<void**>(owned[i].start);std::array<uint8_t,sizeof(void*)> before{};std::memcpy(before.data(),p,before.size());
                require(data.inputEvents->queryInterface(IEventList::iid,p)==kInvalidArgument);
                require(std::memcmp(before.data(),p,before.size())==0);
            }
        };
        // Illegal SDK aliases mark protocol failure, no foreign pointer write.
        require(b.call(f)==RAVET_VST3_NATIVE&&b.report.output_event_failure==1);
    });worker.join();
    require(f.api->end_audio(f.audio,&f.native)==RAVET_VST3_OK);f.audio=nullptr;
    require(f.api->destroy(sibling,&f.native)==RAVET_VST3_OK);
    require(std::all_of(f.module->core->childRegistry.begin(),f.module->core->childRegistry.end(),[&](auto* c){return !c||c==f.child;}));
}
void readerControlAndStaleClassification() {
    using P=ravet::vst3::parameters_draft::LiveCoreProtection;
    Fixture f;auto& live=f.module->core->liveProtection;
    // Exact lifetime read reservation excludes all CONTROL native attempts.
    {
        P::Read read(&live);require(read.admitted(&live));auto before=f.factory->state.calls.size();
        uint32_t count=777;RavetVst3ParameterReport r{};r.struct_size=sizeof(r);const auto saved=r;
        require(f.parameters->refresh(f.child,&count,&r)==RAVET_VST3_LIFECYCLE&&count==777&&std::memcmp(&r,&saved,sizeof(r))==0&&f.factory->state.calls.size()==before);
        require(f.api->destroy(f.child,&f.native)==RAVET_VST3_LIFECYCLE);
    }
    {ControlScope control(*f.module);require(control.acquired);P::Read read(&live);require(!read.admitted(&live));}
    require(live.beginControl());
    live.clear();alignas(void*)std::array<uint8_t,64> bytes{};
    require(live.append(bytes.data(),bytes.size(),1,alignof(void*)));
    ravet::vst3::parameters_draft::Range requested{},old{};
    require(ravet::vst3::parameters_draft::range(bytes.data(),8,1,alignof(void*),requested));old=requested;
    require(live.classify(requested,&old,1)==P::Alias::LiveOwned);
    live.clear();require(live.classify(requested,&old,1)==P::Alias::CopiedNumeric);
    // A same numeric address becomes live again: stale classification never weakens live protection.
    require(live.append(bytes.data(),bytes.size(),1,alignof(void*))&&live.classify(requested,&old,1)==P::Alias::LiveOwned);
    publishProtection(*f.module->core,f.module);live.endControl();
}
void retainedQueryDuringControl() {
    Fixture f;f.prepare();Block b;IEventList* escaped=nullptr;
    f.factory->state.processing=[&](ProcessData& d){escaped=d.inputEvents;require(escaped->addRef()==2);};
    std::thread worker([&]{require(b.call(f)==RAVET_VST3_OK);});worker.join();require(escaped);
    void* value=nullptr;require(escaped->queryInterface(IEventList::iid,&value)==kResultOk&&value==escaped);static_cast<IEventList*>(value)->release();
    require(f.api->end_audio(f.audio,&f.native)==RAVET_VST3_OK);f.audio=nullptr;
    size_t refused=0;
    f.factory->state.controlCall=[&](const char*){void* output=reinterpret_cast<void*>(uintptr_t(0x123));require(escaped->queryInterface(IEventList::iid,&output)==kInvalidArgument&&output==reinterpret_cast<void*>(uintptr_t(0x123)));++refused;};
    require(f.api->stop(f.child,&f.native)==RAVET_VST3_OK&&refused>=2);
    f.factory->state.controlCall={};require(escaped->release()==1);
}
void fixedCapacityAndConcurrentAdmission() {
    using P=ravet::vst3::parameters_draft::LiveCoreProtection;
    P protection;require(protection.beginControl());alignas(void*)uint64_t storage=42;
    for(size_t i=0;i<P::MaximumRanges;++i)require(protection.append(&storage,1,sizeof(storage),alignof(uint64_t)));
    require(protection.count()==P::MaximumRanges&&!protection.append(&storage,1,sizeof(storage),alignof(uint64_t)));
    protection.endControl();
    std::atomic<bool> held{false},finish{false};
    std::thread reader([&]{P::Read guard(&protection);require(guard.admitted(&protection));held.store(true,std::memory_order_release);
        while(!finish.load(std::memory_order_acquire))std::this_thread::yield();
        // This write occurs inside the same guard that excludes publication.
        storage=43;
    });
    while(!held.load(std::memory_order_acquire))std::this_thread::yield();
    require(!protection.beginControl());finish.store(true,std::memory_order_release);reader.join();
    require(storage==43&&protection.beginControl());protection.clear();protection.endControl();
    // The provider registry itself has exact, non-allocating slot capacity.
    std::array<RavetVst3Instance*,256> slots{};RavetVst3Instance object;
    for(size_t i=0;i<slots.size();++i)require(registryInsert(slots,&object));
    const auto saved=slots;require(!registryInsert(slots,&object)&&slots==saved);
    for(size_t i=0;i<slots.size();++i)registryRemove(slots,&object);
    require(std::all_of(slots.begin(),slots.end(),[](auto* p){return !p;}));
}
void actualChildCapacity() {
    Fixture f(false);std::array<RavetVst3Instance*,255> siblings{};
    for(auto& child:siblings)require(f.api->create(f.module,0,&child,&f.native)==RAVET_VST3_OK);
    require(f.module->children==256&&f.module->core->liveProtection.count()<=MaximumLiveRanges);
    const auto oldCalls=f.factory->state.calls.size();RavetVst3Instance* refused=nullptr;f.native=999;
    require(f.api->create(f.module,0,&refused,&f.native)==RAVET_VST3_CAPACITY&&refused==nullptr&&f.native==999&&f.factory->state.calls.size()==oldCalls&&f.module->children==256);
    for(auto* child:siblings)require(f.api->destroy(child,&f.native)==RAVET_VST3_OK);
    require(f.module->children==1);
}
void copiedNumericActualSdkRefusal() {
    Fixture f;using P=ravet::vst3::parameters_draft::LiveCoreProtection;
    auto& live=f.module->core->liveProtection;
    auto storage=std::make_unique<EventPool>(&f.module->core->immutableProtection,&live);
    auto& pool=*storage;
    alignas(void*)std::array<uint8_t,64> recycled{};
    ravet::vst3::parameters_draft::Range old{};require(ravet::vst3::parameters_draft::range(recycled.data(),recycled.size(),1,alignof(void*),old));
    std::thread worker([&]{
        P::Read guard(&live);require(guard.admitted(&live));EventBuses buses{};EventPool::Plan plan;
        require(pool.preflight(16,nullptr,0,nullptr,0,buses,buses,plan,&old,1)==EventAdmission::Ok);
        EventPool::Reservation reservation;require(pool.reserve(reservation)==EventAdmission::Ok);auto* batch=&pool.activate(plan,reservation);require(batch);
        require(batch->input().addRef()==2&&batch->freeze()==0);
        // Reuse a still-live caller buffer at exactly the prior numeric address.
        recycled.fill(0xa5);void** destination=reinterpret_cast<void**>(recycled.data());
        require(live.classify(old,&old,1)==P::Alias::CopiedNumeric);
        require(batch->input().queryInterface(IEventList::iid,destination)==kInvalidArgument);
        require(std::all_of(recycled.begin(),recycled.end(),[](auto v){return v==0xa5;}));
        require(batch->input().release()==1);
    });worker.join();
}
void provenance() {
    for(int which=0;which<3;++which){
        Fixture f;uint32_t count=0;RavetVst3ParameterReport p{};p.struct_size=sizeof(p);
        require(f.parameters->refresh(f.child,&count,&p)==RAVET_VST3_OK);
        RavetVst3ProcessingSelection selection{sizeof(selection),2,1,0};RavetVst3ProcessingInfo info{};info.struct_size=sizeof(info);
        RavetVst3ProcessingReport r{};r.struct_size=sizeof(r);
        require(f.processing->refresh(f.child,&selection,&info,&r)==RAVET_VST3_OK);
        if(which==0)f.factory->state.fail="setup";
        if(which==1)f.factory->state.throwSetup=true;
        if(which==2)f.factory->state.fail="bus-count";
        RavetVst3AudioProfile profile{sizeof(profile),16,1,1,48000};
        const auto result=f.processing->prepare(f.child,&profile,&r);
        require(result==(which==2?RAVET_VST3_METADATA:RAVET_VST3_NATIVE));
        require(r.stage==uint32_t(which==2?105:108));
        require(r.failure==uint32_t(which==2?16:which==1?2:1));
        require((r.flags&7)==uint32_t(which==2?0:which==1?5:3)&&r.native_result==(which==0?rejected:0));
        require(!(r.flags&RAVET_VST3_PROCESSING_HISTORY_MAY_HAVE_ADVANCED));
    }
    for(const auto& operation:{std::string("bus-count"),std::string("bus-info"),std::string("precision"),std::string("arrangement"),std::string("setup"),std::string("input-bus"),std::string("output-bus")}){
        Fixture f;uint32_t count=0;RavetVst3ParameterReport p{};p.struct_size=sizeof(p);require(f.parameters->refresh(f.child,&count,&p)==RAVET_VST3_OK);
        f.factory->state.throwCall=operation;
        RavetVst3AudioProfile profile{sizeof(profile),16,1,1,48000};
        const auto result=f.parameters->prepare(f.child,&profile,&p);
        // Names match actual authored SDK call observations, not a manufactured report.
        if(operation=="bus-count"||f.factory->state.count(operation.c_str())){
            require(result==RAVET_VST3_NATIVE&&(p.flags&7)==5&&p.native_result==0&&p.failure==RAVET_VST3_PARAMETER_FAILURE_THROWN);
            const auto stage=operation=="bus-count"||operation=="bus-info"?105:operation=="precision"?107:operation=="arrangement"?106:operation=="setup"?108:109;
            require(p.stage==uint32_t(stage));
        }else require(false);
        f.factory->state.throwCall.clear();
    }
    for(bool throws:{false,true}){
        Fixture f;f.prepare();require(f.api->end_audio(f.audio,&f.native)==RAVET_VST3_OK);f.audio=nullptr;
        if(throws)f.factory->state.throwCall="stop";else f.factory->state.fail="stop";
        // Cleanup retains actual original status rather than reconstructing returned provenance.
        auto failure=f.child->owner->stop();require(failure.returned==!throws&&failure.origin==(throws?FailureOrigin::Thrown:FailureOrigin::Returned)&&failure.stage==Stage::Stop&&failure.native==(throws?0:rejected));
        f.factory->state.throwCall.clear();f.factory->state.fail.clear();
    }
}
}
void* operator new(std::size_t n){if(measured)++allocations;if(auto* p=std::malloc(n?n:1))return p;throw std::bad_alloc();}
void operator delete(void* p)noexcept{if(measured&&p)++deallocations;std::free(p);}
void* operator new[](std::size_t n){return ::operator new(n);}
void operator delete[](void* p)noexcept{::operator delete(p);}
int main(){try {
    siblingCompleteRanges();std::cout<<"PASS actual sibling wrapper/Instance/handler/scratch capacity/catalogue/name whole range and SDK QI alias rejection\n";
    readerControlAndStaleClassification();std::cout<<"PASS CONTROL/read exclusion and private live-vs-stale numeric classification\n";
    retainedQueryDuringControl();std::cout<<"PASS actual frozen retained QI CONTROL reentry preserves destination and balances references\n";
    fixedCapacityAndConcurrentAdmission();actualChildCapacity();copiedNumericActualSdkRefusal();std::cout<<"PASS exact fixed range/registry capacities, concurrent check-to-write exclusion and actual SDK stale numeric reuse refusal\n";
    provenance();std::cout<<"PASS actual returned4242 / thrown / local metadata original-stage CONTROL provenance\n";
    return 0;
}catch(const std::exception& e){std::cerr<<e.what()<<'\n';return 1;}}
