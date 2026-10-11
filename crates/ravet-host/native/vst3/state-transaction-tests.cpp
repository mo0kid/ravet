#include "provider.cpp"
#include "state-fixture.hpp"
#include <sstream>
namespace {
struct FixtureObjects {
    Observation state;
    Factory factory{state};
};
// Controlled factories have the same lifetime as their reserved core. Pinned
// registry identities cannot be reused stack addresses in successive fixtures.
struct FixtureCore:NativeCore {
    std::shared_ptr<FixtureObjects> objects;
    FixtureCore(std::string key,std::shared_ptr<FixtureObjects> fixtures):NativeCore(std::move(key)),objects(std::move(fixtures)){}
};
struct OwnedFixture {
    std::shared_ptr<FixtureObjects> objects=std::make_shared<FixtureObjects>();
    Observation& state=objects->state;
    Factory& factory=objects->factory;
    RavetVst3Module* parent=nullptr;
    RavetVst3Instance* child=nullptr;
    explicit OwnedFixture(int controller=0) {
        static uint64_t serial=0;
        state.controllerMode=controller;
        auto core=std::make_shared<FixtureCore>("state-controlled-"+std::to_string(++serial),objects);
        core->catalogue=metadata();
        require(registry().reserve(&factory,core)==Registry::Result::Accepted);core->reservation=&factory;
        parent=new RavetVst3Module;parent->version2=true;parent->core=core;
        child=new RavetVst3Instance;child->parent=parent;
        require(core->immutableProtection.seal(core.get(),sizeof(NativeCore),alignof(NativeCore),core->key,core->catalogue));
        require(core->liveProtection.beginControl());
        child->owner=Instance::allocate(core->catalogue,0,core,core->handlerKeepers);
        require(registryInsert(core->childRegistry,child));publishProtection(*core,parent);
        child->owner->initialize(factory,*core->host,core->catalogue,0);parent->children=1;
        publishProtection(*core,parent);core->liveProtection.endControl();
    }
    int capture(RavetVst3StateRecord& record,std::array<uint8_t,8>& component,std::array<uint8_t,8>& controller,
            RavetVst3StateReport& report,uint32_t capacity=8) {
        state.captured=0;
        return stateProvider.capture(child,&record,component.data(),capacity,controller.data(),capacity,&report);
    }
    int restore(const RavetVst3StateRecord& record,const std::array<uint8_t,8>& component,
            const std::array<uint8_t,8>& controller,RavetVst3StateReport& report) {
        state.restored=0;return stateProvider.restore(child,&record,component.data(),controller.data(),&report);
    }
    void releaseForeign() {
        auto streams=std::move(state.retained);
        std::thread worker([&]{for(auto* stream:streams)require(stream->release()==1);});worker.join();
    }
    ~OwnedFixture() {
        releaseForeign();int32_t result=777;
        if(child)require(providerV2.destroy(child,&result)==RAVET_VST3_OK);
        if(parent){const auto status=providerV2.close(parent,&result);require(status==RAVET_VST3_OK||status==RAVET_VST3_PROCESS_PINNED);}
    }
};
RavetVst3StateRecord record() {RavetVst3StateRecord r{};r.struct_size=sizeof(r);return r;}
RavetVst3StateReport report() {RavetVst3StateReport r{};r.struct_size=sizeof(r);r.native_result=777;return r;}
void lifecycleAndRoundTrip() {
    for(int mode:{0,1,2})for(bool optional:{false,true}) {
        OwnedFixture f(mode);f.state.unsupported=optional;
        auto r=record();auto diagnostic=report();std::array<uint8_t,8> component,controller;component.fill(99);controller.fill(88);
        require(f.capture(r,component,controller,diagnostic)==RAVET_VST3_OK);
        require(r.component_size==4&&r.class_id[0]==1);
        require(r.controller_mode==uint32_t(mode==2?0:optional?1:2)&&r.controller_size==uint32_t(mode==2||optional?0:4));
        require(diagnostic.controller_capture_result==(mode==2?kNoInterface:optional?kNotImplemented:kResultOk));
        require(std::equal(f.state.component.begin(),f.state.component.end(),component.begin())&&component[4]==99);
        require(controller[4]==88);
        f.state.component.fill(0);f.state.controller.fill(0);f.state.mirrored.fill(0);f.state.stateStages.clear();
        require(f.restore(r,component,controller,diagnostic)==RAVET_VST3_OK);
        const std::vector<uint32_t> expected=mode==2?std::vector<uint32_t>{3}:optional?std::vector<uint32_t>{3,4}:std::vector<uint32_t>{3,4,5};
        require(f.state.stateStages==expected&&f.state.component[0]==11);
        if(mode!=2)require(f.state.mirrored[0]==11);
        if(mode!=2&&!optional)require(f.state.controller[0]==21);
        require(diagnostic.flags==(RAVET_VST3_STATE_MUTATING|RAVET_VST3_STATE_PREPARE_REQUIRED|RAVET_VST3_STATE_ATTEMPTED|RAVET_VST3_STATE_RETURNED));
        int32_t native=777;require(providerV2.start(f.child,&native)==RAVET_VST3_LIFECYCLE);
        RavetVst3AudioProfile p{sizeof(p),16,1,1,48000};require(providerV2.prepare(f.child,&p,&native)==RAVET_VST3_OK);
        require(providerV2.start(f.child,&native)==RAVET_VST3_OK);const auto before=f.state.stateStages;
        auto prior=diagnostic;
        require(f.capture(r,component,controller,diagnostic)==RAVET_VST3_LIFECYCLE&&f.state.stateStages==before&&std::memcmp(&prior,&diagnostic,sizeof(prior))==0);
        RavetVst3Audio* audio=nullptr;require(providerV2.begin_audio(f.child,&audio,&native)==RAVET_VST3_OK);
        require(f.restore(r,component,controller,diagnostic)==RAVET_VST3_LIFECYCLE);
        require(providerV2.end_audio(audio,&native)==RAVET_VST3_OK&&providerV2.stop(f.child,&native)==RAVET_VST3_OK);
        // Success invalidates an already prepared stopped profile as well.
        require(f.restore(r,component,controller,diagnostic)==RAVET_VST3_OK);
        require(providerV2.start(f.child,&native)==RAVET_VST3_LIFECYCLE);
    }
    OwnedFixture empty;empty.state.empty=true;auto r=record();auto d=report();std::array<uint8_t,8> c{},e{};
    require(empty.capture(r,c,e,d)==RAVET_VST3_OK&&r.component_size==0&&r.controller_mode==2&&r.controller_size==0);
    require(empty.restore(r,c,e,d)==RAVET_VST3_OK); // zero state is distinct from unsupported
}
void failedTransactions() {
    for(uint32_t stage=1;stage<=5;++stage)for(bool throwing:{false,true})for(bool swallowed:{false,true}) {
        OwnedFixture f(1);auto r=record();auto d=report();std::array<uint8_t,8> c,e;c.fill(99);e.fill(88);
        if(stage>2)require(f.capture(r,c,e,d)==RAVET_VST3_OK);
        const auto oldRecord=r;const auto oldC=c,oldE=e;
        f.state.failState=stage;f.state.throwState=throwing;f.state.swallow=swallowed;f.state.retaining=true;f.state.stateStages.clear();
        const auto result=stage<=2?f.capture(r,c,e,d,4):f.restore(r,c,e,d);
        require(result==RAVET_VST3_NATIVE&&d.stage==stage&&(d.flags&RAVET_VST3_STATE_ATTEMPTED));
        require(d.native_result==(throwing?0:swallowed?kResultOk:rejected));
        require(d.stream_failure==(swallowed?(stage<=2?4u:3u):0u));
        require(bool(d.flags&RAVET_VST3_STATE_THROWN)==throwing);
        require(bool(d.flags&RAVET_VST3_STATE_RETURNED)==!throwing);
        require(bool(d.flags&RAVET_VST3_STATE_POISONED)==(stage>2));
        if(stage<=2)require(c==oldC&&e==oldE&&std::memcmp(&r,&oldRecord,sizeof(r))==0);
        else {
            require(f.state.component[0]==11); // partial commit is not rolled back
            int32_t native=777;RavetVst3AudioProfile p{sizeof(p),16,1,1,48000};
            require(providerV2.prepare(f.child,&p,&native)==RAVET_VST3_LIFECYCLE);
            require(f.capture(r,c,e,d)==RAVET_VST3_LIFECYCLE);
            require(providerV2.stop(f.child,&native)==RAVET_VST3_OK);
        }
        for(auto* stream:f.state.retained){int64 cursor=777;require(stream->tell(&cursor)==kInvalidArgument&&cursor==777);}
        require(f.parent->core->stateKeepers->hasForeignReferences());
        f.releaseForeign();require(!f.parent->core->stateKeepers->hasForeignReferences());
    }
}
void preflightAndReentry() {
    OwnedFixture f(1);auto r=record();auto d=report();std::array<uint8_t,8> c{},e{};
    require(f.capture(r,c,e,d)==RAVET_VST3_OK);
    auto unchanged=d;const auto before=f.state.stateStages;auto wrong=r;wrong.class_id[0]=7;
    require(f.restore(wrong,c,e,d)==RAVET_VST3_INVALID);
    wrong=r;wrong.controller_mode=0;wrong.controller_size=0;require(f.restore(wrong,c,e,d)==RAVET_VST3_INVALID);
    wrong=r;wrong.component_size=8u*1024u*1024u+1;require(f.restore(wrong,c,e,d)==RAVET_VST3_INVALID);
    require(stateProvider.capture(f.child,&r,c.data(),8,c.data()+1,7,&d)==RAVET_VST3_INVALID);
    require(stateProvider.capture(f.child,&r,reinterpret_cast<uint8_t*>(&r),8,e.data(),8,&d)==RAVET_VST3_INVALID);
    require(stateProvider.restore(f.child,&r,c.data(),e.data(),reinterpret_cast<RavetVst3StateReport*>(c.data()))==RAVET_VST3_INVALID);
    require(stateProvider.capture(f.child,&r,c.data(),0,e.data(),8,&d)==RAVET_VST3_INVALID);
    require(stateProvider.capture(f.child,&r,reinterpret_cast<uint8_t*>(f.child),8,e.data(),8,&d)==RAVET_VST3_INVALID);
    require(f.state.stateStages==before&&std::memcmp(&d,&unchanged,sizeof(d))==0);
    bool wrongThread=false;std::thread worker([&]{wrongThread=f.capture(r,c,e,d)==RAVET_VST3_THREAD;});worker.join();require(wrongThread);
    require(f.state.stateStages==before&&std::memcmp(&d,&unchanged,sizeof(d))==0);
    int reentries=0;f.state.reentry=[&]{
        const auto prior=d;auto temp=r;
        require(stateProvider.restore(f.child,&temp,c.data(),e.data(),&d)==RAVET_VST3_LIFECYCLE);
        require(std::memcmp(&d,&prior,sizeof(d))==0);int32_t native=777;
        require(providerV2.destroy(f.child,&native)==RAVET_VST3_LIFECYCLE&&native==777);++reentries;
    };
    require(f.capture(r,c,e,d)==RAVET_VST3_OK&&reentries==2);f.state.reentry={};
    // Immutable restore source streams may alias one another when lengths agree.
    require(stateProvider.restore(f.child,&r,c.data(),c.data(),&d)==RAVET_VST3_OK);
}
void retainedBudgetAndReopen() {
    OwnedFixture f(1);auto r=record();auto d=report();std::array<uint8_t,8> c{},e{};f.state.retaining=true;
    for(int i=0;i<8;++i)require(f.capture(r,c,e,d)==RAVET_VST3_OK);
    const auto before=f.state.stateStages;const auto old=d;
    require(f.restore(r,c,e,d)==RAVET_VST3_CAPACITY&&f.state.stateStages==before&&std::memcmp(&old,&d,sizeof(d))==0);
    require(f.capture(r,c,e,d)==RAVET_VST3_CAPACITY&&f.state.stateStages==before);
    auto core=f.parent->core;const auto key=core->key;int32_t native=777;
    require(providerV2.destroy(f.child,&native)==RAVET_VST3_OK);f.child=nullptr;
    require(providerV2.close(f.parent,&native)==RAVET_VST3_PROCESS_PINNED);f.parent=nullptr;
    for(auto* stream:f.state.retained){int64 value=777;require(stream->tell(&value)==kInvalidArgument&&value==777);}
    RavetVst3Module* reopened=nullptr;
    require(providerV2.open(reinterpret_cast<const uint8_t*>(key.data()),uint32_t(key.size()),&reopened,&native)==RAVET_VST3_OK);
    require(reopened->core.get()==core.get()&&core->stateKeepers->retainedBytes()==128);
    f.releaseForeign();core->stateKeepers->reclaim();require(core->stateKeepers->retainedBytes()==0);
    require(providerV2.close(reopened,&native)==RAVET_VST3_PROCESS_PINNED);
}
void originalReturnedThenThrownStateStatus() {
    OwnedFixture f;f.state.failState=1;auto r=record();auto d=report();std::array<uint8_t,8> c{},e{};
    require(f.capture(r,c,e,d)==RAVET_VST3_NATIVE&&d.native_result==rejected&&(d.flags&RAVET_VST3_STATE_RETURNED));
    f.state.throwState=true;
    require(f.capture(r,c,e,d)==RAVET_VST3_NATIVE&&d.native_result==0&&d.stage==1&&
        (d.flags&RAVET_VST3_STATE_THROWN)&&!(d.flags&RAVET_VST3_STATE_RETURNED));
}
void siblingStreamCallbacks() {
    OwnedFixture f;
    auto sibling=std::make_unique<RavetVst3Instance>();sibling->parent=f.parent;
    {ControlScope gate(*f.parent);require(gate.acquired);
        sibling->owner=Instance::allocate(f.parent->core->catalogue,0,f.parent->core,f.parent->core->handlerKeepers);
        require(registryInsert(f.parent->core->childRegistry,sibling.get()));publishProtection(*f.parent->core,f.parent);
        sibling->owner->initialize(f.factory,*f.parent->core->host,f.parent->core->catalogue,0);++f.parent->children;
    }
    auto* target=sibling->owner.get();
    std::array<uint8_t,32> preserved{};std::memcpy(preserved.data(),target,preserved.size());
    for(int mode=0;mode<8;++mode){
        f.state.streamProbe=[&](uint32_t stage,IBStream* stream){if(stage!=1)return;
            uint8_t byte=99;int32 count=-1;int64 position=-1;
            const auto refs=static_cast<ravet::vst3::state_draft::BoundedStateStream*>(stream)->references();
            if(mode==0)require(stream->read(target,1,&count)==kInvalidArgument&&count==-1);
            if(mode==1)require(stream->write(target,1,&count)==kInvalidArgument&&count==-1);
            if(mode==2)require(stream->read(&byte,1,reinterpret_cast<int32*>(target))==kInvalidArgument&&byte==99);
            if(mode==3)require(stream->write(&byte,1,reinterpret_cast<int32*>(target))==kInvalidArgument);
            if(mode==4)require(stream->seek(1,IBStream::kIBSeekSet,reinterpret_cast<int64*>(target))==kInvalidArgument);
            if(mode==5)require(stream->tell(reinterpret_cast<int64*>(target))==kInvalidArgument);
            if(mode==6)require(stream->queryInterface(IBStream::iid,reinterpret_cast<void**>(target))==kInvalidArgument);
            if(mode==7){void* output=reinterpret_cast<void*>(uintptr_t(123));require(stream->queryInterface(reinterpret_cast<const char*>(target),&output)==kInvalidArgument&&output==reinterpret_cast<void*>(uintptr_t(123)));}
            require(std::memcmp(preserved.data(),target,preserved.size())==0);
            require(static_cast<ravet::vst3::state_draft::BoundedStateStream*>(stream)->references()==refs);
            require(stream->tell(&position)==kResultOk&&position==0); // failed seek/data/count effects did not move cursor
            void* output=nullptr;require(stream->queryInterface(IBStream::iid,&output)==kResultOk&&output==stream);
            require(static_cast<IBStream*>(output)->release()==refs);
            // Active off-thread QI cannot enter original CONTROL view.
            std::thread foreign([&]{void* out=reinterpret_cast<void*>(uintptr_t(456));require(stream->queryInterface(IBStream::iid,&out)==kInvalidArgument&&out==reinterpret_cast<void*>(uintptr_t(456)));});foreign.join();
        };
        auto r=record();auto d=report();std::array<uint8_t,8> c{},e{};const auto before=r;
        const auto result=f.capture(r,c,e,d);
        require(result==(mode<6?RAVET_VST3_NATIVE:RAVET_VST3_OK));
        if(mode<6)require(d.stream_failure==1&&std::memcmp(&r,&before,sizeof(r))==0);
        f.state.streamProbe={};
        require(f.capture(r,c,e,d)==RAVET_VST3_OK&&r.component_size==4&&c[0]==11);
    }
    // Capacity belongs to every newly reserved stream before its callback.
    IBStream* escaped=nullptr;size_t observedRanges=0;
    f.state.streamProbe=[&](uint32_t stage,IBStream* stream){if(stage!=1)return;
        auto* native=static_cast<ravet::vst3::state_draft::BoundedStateStream*>(stream);
        require(!f.parent->core->liveProtection.allows({reinterpret_cast<uintptr_t>(native->bytes()),reinterpret_cast<uintptr_t>(native->bytes())+native->capacity()}));
        observedRanges=f.parent->core->liveProtection.count();escaped=stream;stream->addRef();
    };
    auto r=record();auto d=report();std::array<uint8_t,8> c{},e{};require(f.capture(r,c,e,d)==RAVET_VST3_OK&&observedRanges<=MaximumLiveRanges&&escaped);
    f.state.streamProbe={};
    std::thread frozen([&]{void* out=nullptr;require(escaped->queryInterface(IBStream::iid,&out)==kResultOk&&out==escaped);require(static_cast<IBStream*>(out)->release()==2);});frozen.join();
    {ControlScope gate(*f.parent);require(gate.acquired);void* out=reinterpret_cast<void*>(uintptr_t(789));require(escaped->queryInterface(IBStream::iid,&out)==kInvalidArgument&&out==reinterpret_cast<void*>(uintptr_t(789)));}
    require(escaped->release()==1);
    int32_t native=999;require(providerV2.destroy(sibling.release(),&native)==RAVET_VST3_OK);
}
}
int main(){try{originalReturnedThenThrownStateStatus();siblingStreamCallbacks();std::cout<<"PASS actual same-phase state sibling source/destination/count/seek/tell/QI protection, cursor/refcount preservation, retry and off-thread/frozen admission\n";
    require(stateProvider.provider==ravet_vst3_provider_v2()&&stateProvider.version==1&&stateProvider.struct_size==sizeof(stateProvider));
    lifecycleAndRoundTrip();failedTransactions();preflightAndReentry();retainedBudgetAndReopen();
    std::cout<<"PASS component/controller state transaction: combined/separate/absent/unsupported/zero, independent cursors, stopped preflight, first/late native/throw/swallowed faults, poison/reprepare, immutable output publication, metadata/identity/thread/reentry, retained frozen streams/budget/process-pin reopen\n";
}catch(const std::exception& error){std::cerr<<error.what()<<'\n';return 1;}}
