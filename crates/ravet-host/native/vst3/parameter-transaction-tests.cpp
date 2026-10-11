#include "provider.cpp"
#include "parameter-fixture.hpp"
#include <sstream>
#include <atomic>
namespace {
struct FixtureObjects {Observation state;Factory factory{state};};
struct FixtureCore:NativeCore {
    std::shared_ptr<FixtureObjects> objects;
    FixtureCore(std::string key,std::shared_ptr<FixtureObjects> f):NativeCore(std::move(key)),objects(std::move(f)){}
};
struct OwnedFixture {
    std::shared_ptr<FixtureObjects> objects=std::make_shared<FixtureObjects>();Observation& state=objects->state;
    RavetVst3Module* parent=nullptr;RavetVst3Instance* child=nullptr;RavetVst3Audio* audio=nullptr;
    std::array<float,10> input{},output{};RavetVst3InputChannel in{input.data(),10,0};RavetVst3OutputChannel out{output.data(),10,0};
    RavetVst3AudioBlock block{sizeof(block),8,1,1,&in,&out};
    std::string key;
    OwnedFixture(){
        static uint64_t serial=0;key="parameter-controlled-"+std::to_string(++serial);
        auto core=std::make_shared<FixtureCore>(key,objects);core->catalogue=metadata();
        require(registry().reserve(&objects->factory,core)==Registry::Result::Accepted);core->reservation=&objects->factory;
        parent=new RavetVst3Module;parent->version2=true;parent->core=core;
        child=new RavetVst3Instance;child->parent=parent;
        require(core->immutableProtection.seal(core.get(),sizeof(NativeCore),alignof(NativeCore),core->key,core->catalogue));
        require(core->liveProtection.beginControl());
        child->owner=Instance::allocate(core->catalogue,0,core,core->handlerKeepers);
        require(registryInsert(core->childRegistry,child));publishProtection(*core,parent);
        child->owner->initialize(objects->factory,*core->host,core->catalogue,0);parent->children=1;
        publishProtection(*core,parent);core->liveProtection.endControl();
        output.fill(77);
    }
    RavetVst3ParameterReport report(){RavetVst3ParameterReport r{};r.struct_size=sizeof(r);r.native_result=999;return r;}
    void refresh(){uint32_t count=999;auto r=report();require(parameterProvider.refresh(child,&count,&r)==RAVET_VST3_OK&&count==2);}
    void start(){refresh();RavetVst3AudioProfile p{sizeof(p),8,1,1,48000};auto r=report();int32_t n=999;
        require(parameterProvider.prepare(child,&p,&r)==RAVET_VST3_OK&&providerV2.start(child,&n)==RAVET_VST3_OK&&providerV2.begin_audio(child,&audio,&n)==RAVET_VST3_OK);
    }
    int process(const RavetVst3ParameterPoint* points,uint32_t count,RavetVst3ParameterReport& r){return parameterProvider.process(audio,&block,points,count,&r);}
    void join(){int32_t n=999;require(providerV2.end_audio(audio,&n)==RAVET_VST3_OK);audio=nullptr;require(providerV2.stop(child,&n)==RAVET_VST3_OK);}
    void releaseForeign(){auto count=state.retainedCount;state.retainedCount=0;std::thread worker([&]{for(size_t i=0;i<count;++i)require(state.retained[i]->release()==1);});worker.join();}
    void destroy(){int32_t n=999;require(providerV2.destroy(child,&n)==RAVET_VST3_OK);child=nullptr;}
    int close(){int32_t n=999;const auto result=providerV2.close(parent,&n);parent=nullptr;return result;}
    ~OwnedFixture(){if(audio)join();releaseForeign();if(child)destroy();if(parent){const auto result=close();require(result==RAVET_VST3_OK||result==RAVET_VST3_PROCESS_PINNED);}}
};
void stoppedMetadataAndRefusal(){
    OwnedFixture f;f.refresh();auto r=f.report();double value=999;
    for(uint64_t bits:{UINT64_C(0x7ff8000000000001),UINT64_C(0x7ff0000000000000),UINT64_C(0xfff0000000000000)}){
        volatile uint64_t observed=bits;uint64_t copied=observed;double rate=0;std::memcpy(&rate,&copied,sizeof(rate));
        RavetVst3AudioProfile invalid{sizeof(invalid),8,1,1,rate};const auto calls=f.state.calls.size();auto before=r;
        require(parameterProvider.prepare(f.child,&invalid,&r)==RAVET_VST3_LIFECYCLE&&f.state.calls.size()==calls&&std::memcmp(&r,&before,sizeof(r))==0);
    }
    RavetVst3ParameterRecord old{};old.struct_size=sizeof(old);require(parameterProvider.info(f.child,0,&old)==RAVET_VST3_OK&&old.id==1);
    f.state.badInfo=true;uint32_t count=777;require(parameterProvider.refresh(f.child,&count,&r)==RAVET_VST3_METADATA&&count==777&&(r.flags&RAVET_VST3_PARAMETER_INVALIDATED));
    RavetVst3ParameterRecord preserved{};preserved.struct_size=sizeof(preserved);require(parameterProvider.info(f.child,0,&preserved)==RAVET_VST3_OK&&std::memcmp(&old,&preserved,sizeof(old))==0);
    RavetVst3AudioProfile p{sizeof(p),8,1,1,48000};const auto calls=f.state.calls.size();auto prior=r;
    require(parameterProvider.prepare(f.child,&p,&r)==RAVET_VST3_LIFECYCLE&&f.state.calls.size()==calls&&std::memcmp(&r,&prior,sizeof(r))==0);
    f.state.badInfo=false;f.refresh();
    for(bool throws:{false,true}){
        f.state.refuseEdit=!throws;f.state.throwEdit=throws;
        require(parameterProvider.value(f.child,1,RAVET_VST3_PARAMETER_EDIT,throws?.7:.6,&value,&r)==RAVET_VST3_NATIVE&&value==999);
        require(f.state.controllerValue==(throws?.7:.6)&&r.native_result==(throws?0:rejected)&&
            (r.flags&RAVET_VST3_PARAMETER_INVALIDATED)&&(r.flags&(throws?RAVET_VST3_PARAMETER_THROWN:RAVET_VST3_PARAMETER_RETURNED)));
        prior=r;require(parameterProvider.prepare(f.child,&p,&r)==RAVET_VST3_LIFECYCLE&&std::memcmp(&r,&prior,sizeof(r))==0);
        f.state.refuseEdit=f.state.throwEdit=false;f.refresh();
    }
    require(parameterProvider.value(f.child,2,RAVET_VST3_PARAMETER_EDIT,.8,&value,&r)==RAVET_VST3_METADATA&&f.state.controllerValue==.7&&value==999);
}
void immutableWholeBlockAndReentry(){
    OwnedFixture f;f.start();auto r=f.report();const auto prior=r;const RavetVst3ParameterPoint p{1,0,.8};
    f.state.duringProcess=[&]{auto nested=f.report();auto before=nested;
        require(f.process(&p,1,nested)==RAVET_VST3_LIFECYCLE&&std::memcmp(&nested,&before,sizeof(before))==0);
    };
    bool pass=false;std::thread worker([&]{
        const RavetVst3ParameterPoint readOnly{2,0,.8};require(f.process(&readOnly,1,r)==RAVET_VST3_INVALID&&f.state.processes==0&&f.output[0]==77&&std::memcmp(&r,&prior,sizeof(r))==0);
        auto* pool=f.child->parent->core->parameterKeepers.get();
        const auto* old=f.block.outputs;f.block.outputs=reinterpret_cast<const RavetVst3OutputChannel*>(pool);
        require(f.process(&p,1,r)==RAVET_VST3_INVALID&&f.state.processes==0);f.block.outputs=old;
        require(f.process(&p,1,r)==RAVET_VST3_OK&&f.state.processes==1&&f.output[0]==.8f&&f.output[8]==77);pass=true;
    });worker.join();require(pass);f.state.duringProcess={};f.join();
}
void retainedQueuesCapacityAndExactPin(){
    OwnedFixture f;f.start();f.state.retainQueues=true;const RavetVst3ParameterPoint p{1,0,.4};auto r=f.report();
    std::thread worker([&]{for(uint32_t i=0;i<16;++i)require(f.process(&p,1,r)==RAVET_VST3_OK);auto prior=r;f.output.fill(77);
        require(f.process(&p,1,r)==RAVET_VST3_CAPACITY&&f.state.processes==16&&f.output[0]==77&&std::memcmp(&r,&prior,sizeof(r))==0);
    });worker.join();require(f.state.retainedCount==16);f.join();
    const auto core=f.parent->core;require(core->parameterKeepers->observedRetention());
    f.destroy();require(core->parameterKeepers->count()==1&&f.state.destroyed==1);require(f.close()==RAVET_VST3_PROCESS_PINNED);
    std::shared_ptr<NativeCore> reopened;require(registry().reopenKey(f.key,true,reopened)==Registry::Result::Accepted&&reopened.get()==core.get());
    f.releaseForeign();require(reopened->parameterKeepers->observedRetention());reopened->parameterKeepers->reclaim();require(reopened->parameterKeepers->count()==0);
    bool pinned=false;require(reopened->retire(&pinned)==kResultOk&&pinned);
}
void originalProcessRefusalExceptionAndProtocolFault(){
    for(uint32_t kind=0;kind<3;++kind){OwnedFixture f;f.start();auto r=f.report();const RavetVst3ParameterPoint p{1,0,.4};
        f.state.protocolFault=kind==0;f.state.throwProcess=kind==1;if(kind==2)f.state.fail="process";
        std::thread worker([&]{require(f.process(&p,1,r)==RAVET_VST3_NATIVE&&f.state.processes==1&&f.output[0]==0&&f.output[8]==77);
            require(r.native_result==(kind==0?kResultOk:kind==1?kInternalError:rejected)&&r.failure==(kind==0?1u:0u));
            require(r.flags&(kind==1?RAVET_VST3_PARAMETER_THROWN:RAVET_VST3_PARAMETER_RETURNED));
            auto before=r;f.output.fill(77);require(f.process(&p,1,r)==RAVET_VST3_LIFECYCLE&&f.output[0]==77&&std::memcmp(&r,&before,sizeof(r))==0);
        });worker.join();f.join();}
}
void sharedCoreAudioExcludesControlUntilEveryJoin(){
    OwnedFixture f;f.refresh();auto secondObjects=std::make_shared<FixtureObjects>();
    auto second=std::make_unique<RavetVst3Instance>();second->parent=f.parent;
    require(f.parent->core->liveProtection.beginControl());
    second->owner=Instance::allocate(f.parent->core->catalogue,0,f.parent->core,f.parent->core->handlerKeepers);
    require(registryInsert(f.parent->core->childRegistry,second.get()));publishProtection(*f.parent->core,f.parent);
    second->owner->initialize(secondObjects->factory,*f.parent->core->host,f.parent->core->catalogue,0);
    ++f.parent->children;publishProtection(*f.parent->core,f.parent);f.parent->core->liveProtection.endControl();
    auto r=f.report();uint32_t count=999;int32_t native=999;
    require(parameterProvider.refresh(second.get(),&count,&r)==RAVET_VST3_OK&&count==2);
    RavetVst3AudioProfile profile{sizeof(profile),8,1,1,48000};
    require(parameterProvider.prepare(f.child,&profile,&r)==RAVET_VST3_OK);
    require(parameterProvider.prepare(second.get(),&profile,&r)==RAVET_VST3_OK);
    require(providerV2.start(f.child,&native)==RAVET_VST3_OK&&providerV2.start(second.get(),&native)==RAVET_VST3_OK);
    require(providerV2.begin_audio(f.child,&f.audio,&native)==RAVET_VST3_OK);
    require(f.parent->audioChildren==1);
    const auto secondCalls=secondObjects->state.calls.size();const auto before=r;
    require(parameterProvider.refresh(second.get(),&count,&r)==RAVET_VST3_LIFECYCLE&&std::memcmp(&r,&before,sizeof(r))==0);
    require(parameterProvider.prepare(second.get(),&profile,&r)==RAVET_VST3_LIFECYCLE&&secondObjects->state.calls.size()==secondCalls);
    RavetVst3Audio* otherAudio=nullptr;
    require(providerV2.begin_audio(second.get(),&otherAudio,&native)==RAVET_VST3_OK&&f.parent->audioChildren==2);
    std::array<float,10> otherInput{},otherOutput{};otherOutput.fill(77);
    RavetVst3InputChannel in{otherInput.data(),10,0};RavetVst3OutputChannel out{otherOutput.data(),10,0};
    RavetVst3AudioBlock block{sizeof(block),8,1,1,&in,&out};
    const RavetVst3ParameterPoint point{1,0,.4};std::atomic<bool> ready{false},proceed{false};
    std::thread worker([&]{
        int32_t attemptedEnd=999;require(providerV2.end_audio(f.audio,&attemptedEnd)==RAVET_VST3_THREAD&&attemptedEnd==999);
        auto report=f.report();require(f.process(&point,1,report)==RAVET_VST3_OK);
        ready.store(true,std::memory_order_release);
        while(!proceed.load(std::memory_order_acquire)){}
        for(uint32_t i=0;i<10000;++i){
            require(f.process(&point,1,report)==RAVET_VST3_OK);
            require(parameterProvider.process(otherAudio,&block,&point,1,&report)==RAVET_VST3_OK);
        }
    });
    while(!ready.load(std::memory_order_acquire)){}
    const auto firstCalls=f.state.calls.size();const auto pools=f.parent->core->parameterKeepers->count();
    double value=999;native=999;
    require(parameterProvider.value(second.get(),1,RAVET_VST3_PARAMETER_EDIT,.8,&value,&r)==RAVET_VST3_LIFECYCLE&&value==999);
    require(providerV2.stop(second.get(),&native)==RAVET_VST3_LIFECYCLE&&native==999);
    require(providerV2.destroy(second.get(),&native)==RAVET_VST3_LIFECYCLE&&native==999);
    RavetVst3Instance* newChild=nullptr;
    require(providerV2.create(f.parent,0,&newChild,&native)==RAVET_VST3_LIFECYCLE&&!newChild&&native==999);
    require(f.state.calls.size()==firstCalls&&secondObjects->state.calls.size()==secondCalls&&f.parent->core->parameterKeepers->count()==pools);
    proceed.store(true,std::memory_order_release);worker.join();
    require(f.state.processes==10001&&secondObjects->state.processes==10000&&otherOutput[0]==.4f&&otherOutput[8]==77);
    require(providerV2.end_audio(f.audio,&native)==RAVET_VST3_OK);f.audio=nullptr;
    require(f.parent->audioChildren==1);native=999;
    require(providerV2.stop(f.child,&native)==RAVET_VST3_LIFECYCLE&&native==999);
    require(providerV2.destroy(f.child,&native)==RAVET_VST3_LIFECYCLE&&native==999);
    require(providerV2.end_audio(otherAudio,&native)==RAVET_VST3_OK&&f.parent->audioChildren==0);
    require(providerV2.stop(f.child,&native)==RAVET_VST3_OK&&providerV2.stop(second.get(),&native)==RAVET_VST3_OK);
    require(parameterProvider.value(second.get(),1,RAVET_VST3_PARAMETER_EDIT,.8,&value,&r)==RAVET_VST3_OK&&value==.8);
    require(providerV2.destroy(second.release(),&native)==RAVET_VST3_OK&&f.parent->children==1);
}
void preparationDistinguishesReturnedRefusalOwnedMetadataAndException(){
    for(uint32_t kind=0;kind<3;++kind){
        OwnedFixture f;f.refresh();auto r=f.report();RavetVst3AudioProfile profile{sizeof(profile),8,1,1,48000};
        if(kind==0)f.state.fail="setup";else if(kind==1)f.state.fail="bus-count";else f.state.throwSetup=true;
        require(parameterProvider.prepare(f.child,&profile,&r)==(kind==1?RAVET_VST3_METADATA:RAVET_VST3_NATIVE));
        if(kind==0)require(r.stage==RAVET_VST3_PARAMETER_STAGE_SETUP&&r.native_result==rejected&&r.flags==(RAVET_VST3_PARAMETER_ATTEMPTED|RAVET_VST3_PARAMETER_RETURNED));
        if(kind==1)require(r.stage==RAVET_VST3_PARAMETER_STAGE_BUSES&&r.native_result==0&&r.failure==RAVET_VST3_PARAMETER_FAILURE_METADATA&&r.flags==0);
        if(kind==2)require(r.stage==RAVET_VST3_PARAMETER_STAGE_SETUP&&r.native_result==0&&r.failure==RAVET_VST3_PARAMETER_FAILURE_THROWN&&r.flags==(RAVET_VST3_PARAMETER_ATTEMPTED|RAVET_VST3_PARAMETER_THROWN));
        int32_t native=999;RavetVst3Audio* audio=nullptr;
        require(providerV2.start(f.child,&native)==RAVET_VST3_LIFECYCLE);
        require(providerV2.begin_audio(f.child,&audio,&native)==RAVET_VST3_LIFECYCLE&&!audio&&f.parent->audioChildren==0);
    }
}
}
int main(){try{
    stoppedMetadataAndRefusal();immutableWholeBlockAndReentry();retainedQueuesCapacityAndExactPin();originalProcessRefusalExceptionAndProtocolFault();sharedCoreAudioExcludesControlUntilEveryJoin();preparationDistinguishesReturnedRefusalOwnedMetadataAndException();
    std::cout<<"PASS actual initialized parameter transactions:6 groups, preserved metadata/explicit reconciliation, mutating refusal/throw, read-only/known-owner metadata/reentry preflight,16 retained child queues/frozen exact-core pin/reopen, original native0 protocol-fault/native-refusal/exception diagnostics and silence, two shared-core audio owners defer all CONTROL mutations until both joined, prepare distinguishes real refusal from owned metadata and exceptions\n";
}catch(const std::exception& e){std::cerr<<e.what()<<'\n';return 1;}}
