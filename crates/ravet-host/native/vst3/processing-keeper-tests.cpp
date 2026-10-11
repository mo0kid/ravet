#include "ProcessingPoolKeepers.hpp"
#include "RavetVst3CoreRegistry.hpp"
#include <iostream>
#include <memory>
#include <stdexcept>
using namespace ravet::vst3::processing_draft;
using namespace Steinberg;
using namespace Steinberg::Vst;
namespace {
void require(bool good,const char* message){if(!good){std::cerr<<"FAIL "<<message<<'\n';throw std::runtime_error(message);}}
struct Observation {size_t coreDestructions=0,pins=0,foreignTeardowns=0;};
struct Core {
    const std::string key;
    Observation& observation;
    ProcessingPoolKeepers<Core> pools;
    Core(std::string k,Observation& o):key(std::move(k)),observation(o),pools(*this){}
    const std::string& identityKey()const{return key;}
    ~Core(){++observation.coreDestructions;}
};
template<class F>bool refused(F&& f){try{f();return false;}catch(const std::logic_error&){return true;}}
EventBatch& prepare(EventPool& pool){EventBuses buses{};EventBatch* batch=nullptr;
    require(pool.prepare(32,nullptr,0,nullptr,0,buses,buses,batch)==EventAdmission::Ok,"keeper actual process batch");return *batch;}
void actualObjectByteCapAndReclaim(){
    Observation observation;Core core("bounded",observation);using Keepers=ProcessingPoolKeepers<Core>;
    constexpr size_t maximum=(Keepers::MaximumBytes-sizeof(Keepers))/sizeof(EventPool);
    static_assert(maximum==6&&maximum<Keepers::MaximumPools);
    std::array<EventPool*,maximum> pools{};
    for(size_t i=0;i<maximum;++i){pools[i]=&core.pools.attach(core);
        require(core.pools.count(core)==i+1&&core.pools.bytes(core)==sizeof(Keepers)+(i+1)*sizeof(EventPool),"actual complete pool bytes, including keeper");}
    bool capacity=false;try{core.pools.attach(core);}catch(const std::length_error&){capacity=true;}
    require(capacity&&core.pools.count(core)==maximum&&core.pools.bytes(core)<=Keepers::MaximumBytes,"64MiB forbids seventh complete pool without partial attachment");
    size_t pins=0;auto pin=[&](Core& origin){require(&origin==&core,"exact core pin origin");++pins;};
    for(auto* pool:pools)core.pools.detach(core,*pool);
    core.pools.reclaim(core,pin);
    require(!pins&&core.pools.count(core)==0&&core.pools.bytes(core)==sizeof(Keepers),"unretained detached CONTROL reclamation");
    auto& again=core.pools.attach(core);core.pools.detach(core,again);core.pools.reclaim(core,pin);
    require(!pins&&core.pools.count(core)==0,"unretained pool budget reusable");
    std::cout<<"OBSERVED six pools="<<sizeof(Keepers)+maximum*sizeof(EventPool)<<"bytes;cap="<<Keepers::MaximumBytes<<"\n";
}
void ownerIdentityActiveDetachAndProtectedRanges(){
    Observation observation;Core core("one",observation),other("two",observation);
    auto& pool=core.pools.attach(core);auto& batch=prepare(pool);
    require(refused([&]{core.pools.detach(core,pool);}),"active process detach rejected");
    require(refused([&]{core.pools.attach(other);})&&refused([&]{core.pools.count(other);}),"wrong native core origin rejected");
    bool threadRefusal=false;std::thread worker([&]{threadRefusal=refused([&]{core.pools.attach(core);});});worker.join();
    require(threadRefusal&&core.pools.count(core)==1,"wrong CONTROL thread cannot allocate/attach");
    uint32_t outside=0;
    require(!core.pools.externalRange(&core.pools,sizeof(core.pools),alignof(decltype(core.pools)))&&
        !core.pools.externalRange(&pool,sizeof(pool),alignof(EventPool))&&
        !core.pools.externalRange(&batch.outputParameters(),sizeof(void*),alignof(void*))&&
        core.pools.externalRange(&outside,sizeof(outside),alignof(uint32_t)),"whole keeper and all pool metadata protected from caller writes");
    require(batch.freeze()==0,"active original process preserved");core.pools.detach(core,pool);
    require(refused([&]{core.pools.detach(core,pool);}),"double detach rejected");
    auto& alien=other.pools.attach(other);
    require(refused([&]{core.pools.detach(core,alien);}),"foreign pool identity rejected");
    core.pools.reclaim(core,[](Core&){throw std::runtime_error("unexpected pin");});
    other.pools.detach(other,alien);other.pools.reclaim(other,[](Core&){throw std::runtime_error("unexpected pin");});
}
void retainedListWorkerReleaseAndRefusedPin(){
    Observation observation;Core core("retained",observation);auto& pool=core.pools.attach(core);
    const uint32_t id=7;require(pool.configureParameters(&id,1),"retained catalogue");auto& batch=prepare(pool);
    int32 index=-1;auto* queue=batch.outputParameters().addParameterData(id,index);
    require(queue&&queue->addPoint(3,.625,index)==kResultOk,"actual retained output parameter point");
    queue->addRef();batch.output().addRef();require(batch.freeze()==0,"two interface retentions observed");
    core.pools.detach(core,pool);
    bool threw=false;try{core.pools.reclaim(core,[&](Core& origin){require(&origin==&core,"refused exact pin origin");throw std::runtime_error("pin refused");});}
    catch(const std::runtime_error&){threw=true;}
    require(threw&&core.pools.count(core)==1&&core.pools.observedRetention(core),"refused pin prevents pool destruction/reclamation");
    size_t pins=0;auto pin=[&](Core& origin){require(&origin==&core,"retained exact origin");++pins;};
    core.pools.reclaim(core,pin);require(pins==1&&core.pools.count(core)==1,"live queue/list keep complete payload and point pool");
    std::thread worker([&]{queue->release();batch.output().release();});worker.join();
    core.pools.reclaim(core,pin);require(pins==2&&core.pools.count(core)==0,"final worker release never reclaims; explicit CONTROL does");
    core.pools.pinBeforeForeignTeardown(core,pin);
    require(pins==3&&core.pools.observedRetention(core),"retention remains sticky after pool reclamation before foreign teardown");
}
void exactRegistryPinReopenAndImageLifetime(){
    // Controlled core resource observation and the actual reusable reservation
    // registry; no SDK module or production image-loading claim from this test.
    using Registry=ravet::vst3::draft::CoreRegistry<Core,2>;
    auto* registry=new Registry;auto* observation=new Observation;
    auto owner=std::make_shared<Core>("canonical-fixture",*observation);
    int identity=0;const void* factory=&identity;
    require(registry->reserve(factory,owner)==Registry::Result::Accepted,"actual native core reservation");
    auto& pool=owner->pools.attach(*owner);auto& batch=prepare(pool);batch.input().addRef();
    require(batch.freeze()==0,"retained input SDK list");
    auto pin=[&](Core& origin){require(&origin==owner.get(),"exact shared core passed to pin");
        require(registry->pin(factory,owner)==Registry::Result::Accepted,"actual registry pin before teardown");++observation->pins;};
    owner->pools.pinBeforeForeignTeardown(*owner,pin);
    bool pinned=false;require(registry->beginRetire(factory,owner,pinned)==Registry::Result::Accepted&&pinned,"retirement observes pin already installed");
    // A real provider skips native factory/context/image teardown when pinned.
    if(!pinned)++observation->foreignTeardowns;
    owner->pools.detach(*owner,pool);
    require(registry->finishRetire(factory,owner)==Registry::Result::Accepted,"consuming facade retirement");
    std::weak_ptr<Core> retained=owner;owner.reset();
    require(!retained.expired()&&observation->coreDestructions==0&&observation->foreignTeardowns==0,"exact core resource survives last ordinary facade");
    std::shared_ptr<Core> wrong;
    require(registry->reopenKey("other",true,wrong)==Registry::Result::NotFound&&!wrong,"wrong path cannot borrow retained core");
    require(registry->reopenKey("canonical-fixture",false,wrong)==Registry::Result::Busy&&!wrong,"v1 reopen excludes pinned image");
    require(registry->reopenKey("canonical-fixture",true,owner)==Registry::Result::Accepted&&owner.get()==retained.lock().get(),"v2 exact core reopen without new resource");
    std::thread worker([&]{batch.input().release();});worker.join();
    owner->pools.reclaim(*owner,pin);
    require(owner->pools.count(*owner)==0&&observation->pins==2&&observation->coreDestructions==0,"reopened CONTROL reclaims released pool but never unpins core");
    require(registry->beginRetire(factory,owner,pinned)==Registry::Result::Accepted&&pinned&&
        registry->finishRetire(factory,owner)==Registry::Result::Accepted,"second consuming close remains pinned");owner.reset();
    require(!retained.expired()&&observation->coreDestructions==0,"no implicit image unpin after foreign release");
    // Deliberate process-lifetime registry and observation, matching production.
}
}
int main(){try{
    actualObjectByteCapAndReclaim();std::cout<<"PASS actual object64MiB budget/CONTROL reclamation\n";
    ownerIdentityActiveDetachAndProtectedRanges();std::cout<<"PASS exact owner/thread/detach/protected pool ranges\n";
    retainedListWorkerReleaseAndRefusedPin();std::cout<<"PASS retained output queue/event/worker/refused pin/sticky evidence\n";
    exactRegistryPinReopenAndImageLifetime();std::cout<<"PASS actual registry pin-before-retire/exact reopen/controlled core lifetime\n";
    return 0;
}catch(const std::exception& error){std::cerr<<error.what()<<'\n';return 1;}}
