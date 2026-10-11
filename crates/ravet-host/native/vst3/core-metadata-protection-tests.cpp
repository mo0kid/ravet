#include "ProcessingPoolKeepers.hpp"
#include "ParameterPoolKeepers.hpp"
#include "RavetVst3CoreRegistry.hpp"
#include <cstdlib>
#include <iostream>
#include <new>
using namespace Steinberg;
using namespace Steinberg::Vst;
using namespace ravet::vst3::parameters_draft;
using namespace ravet::vst3::processing_draft;
namespace {thread_local bool measured=false;thread_local size_t allocations=0,deallocations=0;}
void* operator new(size_t n){if(measured)++allocations;if(void* p=std::malloc(n?n:1))return p;throw std::bad_alloc();}
void* operator new[](size_t n){return ::operator new(n);}
void operator delete(void* p)noexcept{if(p&&measured)++deallocations;std::free(p);}
void operator delete[](void* p)noexcept{::operator delete(p);}
void operator delete(void* p,size_t)noexcept{::operator delete(p);}
void operator delete[](void* p,size_t)noexcept{::operator delete(p);}
namespace {
void require(bool good,const char* why){if(!good){std::cerr<<"FAIL "<<why<<'\n';throw std::runtime_error(why);}}
struct Core {
    const std::string key=std::string(4096,'k');
    ravet::vst3::Catalogue catalogue;
    ImmutableCoreProtection protection;
    Event event{};int32 offset=77,index=99;double value=88;void* queryResult=nullptr;
    std::unique_ptr<ProcessingPoolKeepers<Core>> processing;
    std::unique_ptr<ParameterPoolKeepers> parameters;
    size_t* destroyed=nullptr;
    const std::string& identityKey()const{return key;}
    explicit Core(size_t count=256,size_t* observation=nullptr):destroyed(observation){
        catalogue.vendor=std::string(100,'v');catalogue.url="u";catalogue.email=std::string(120,'e');
        catalogue.classes.reserve(count);
        for(size_t i=0;i<count;++i){ravet::vst3::Class item;
            item.category="Audio Module Class";item.name=std::string(300,char('a'+i%26));
            item.vendor=std::string(100,'v');item.version="1";item.sdkVersion=std::string(90,'s');
            item.subcategories=std::string(120,'c');catalogue.classes.push_back(std::move(item));}
        if(count<=256){require(protection.seal(this,sizeof(*this),alignof(Core),key,catalogue),"seal exact containing core metadata");
            processing=std::make_unique<ProcessingPoolKeepers<Core>>(*this,&protection);
            parameters=std::make_unique<ParameterPoolKeepers>(&protection);}
    }
    ~Core(){if(destroyed)++*destroyed;}
};
Record note(){Record r{};r.struct_size=sizeof(r);r.type=Event::kNoteOnEvent;r.pitch=69;r.velocity=.5f;return r;}
EventBatch& events(EventPool& pool){EventBuses buses{};buses[0]=1;auto n=note();EventBatch* result=nullptr;
    require(pool.prepare(8,&n,1,nullptr,0,buses,buses,result)==EventAdmission::Ok,"actual event batch");return *result;}
Batch& parameters(Pool& pool){Point p{7,2,.625};Batch* result=nullptr;
    require(pool.prepare(8,&p,1,result)==Admission::Ok,"actual parameter batch");return *result;}
void fixedSnapshotAndWholeMetadata(){
    Core core;require(core.protection.count()==1544&&core.protection.count()<=ImmutableCoreProtection::MaximumRanges,"all 256x6 string ranges plus core/key/catalogue/vector/factory/snapshot");
    require(!core.protection.seal(&core,sizeof(core),alignof(Core),core.key,core.catalogue)&&core.protection.count()==1544,"one immutable seal preserves original snapshot");
    const auto check=[&](const std::string& text){
        require(!core.protection.externalRange(text.data(),text.capacity()+1,1),"whole inline/heap string capacity including terminator protected");
        require(!core.protection.externalRange(text.data()+text.size(),1,1),"each string terminator protected");
    };
    check(core.key);check(core.catalogue.vendor);check(core.catalogue.url);check(core.catalogue.email);
    for(const auto& item:core.catalogue.classes){check(item.category);check(item.name);check(item.vendor);check(item.version);check(item.sdkVersion);check(item.subcategories);}
    require(!core.protection.externalRange(core.catalogue.classes.data(),core.catalogue.classes.capacity()*sizeof(ravet::vst3::Class),alignof(ravet::vst3::Class)),"whole allocated class vector capacity");
    require(!core.protection.externalRange(&core.protection,sizeof(core.protection),alignof(ImmutableCoreProtection)),"snapshot's own numeric ownership storage");
    int outside=0;require(core.protection.externalRange(&outside,sizeof(outside),alignof(int))&&core.protection.externalRange(nullptr,0,1),"external values and canonical empty spans admitted");
    Range r{};require(!range(&outside,1,0,1,r)&&!range(&outside,1,1,0,r)&&!range(&outside,SIZE_MAX,2,1,r),"zero divisor/alignment and multiplication overflow rejected");
    Core tooMany(257);require(!tooMany.protection.seal(&tooMany,sizeof(tooMany),alignof(Core),tooMany.key,tooMany.catalogue)&&!tooMany.protection.sealed()&&!tooMany.protection.count(),"capacity refusal leaves unpublished snapshot unchanged");
    ImmutableCoreProtection unsealed;bool failed=false;try{Pool invalid(&unsealed);}catch(const std::logic_error&){failed=true;}require(failed,"unsealed borrowing refused before SDK storage becomes visible");
}
void actualSdkMetadataAliasesAndHealthyRetry(){
    Core core;auto& ep=core.processing->attach(core);auto& pp=core.parameters->attach();
    const uint32 id=7;require(ep.configureParameters(&id,1),"output parameter catalogue");
    auto& eb=events(ep);auto& pb=parameters(pp);auto* inputQueue=pb.getParameterData(0);
    require(inputQueue!=nullptr,"actual input queue");
    require(eb.input().getEvent(0,core.event)==kInvalidArgument&&core.event.type==0,"whole core destination denied before event write");
    require(eb.freeze()==1,"event alias fault independently retained");
    require(inputQueue->getPoint(0,core.offset,core.value)==kInvalidArgument&&core.offset==77&&core.value==88,"input SDK point destinations cannot overwrite core");
    require(pb.freeze()==1,"input point alias fault independently retained");
    for(const auto& item:core.catalogue.classes){
        for(const auto* text:{&item.category,&item.name,&item.vendor,&item.version,&item.sdkVersion,&item.subcategories}){
            const auto original=*text;
            const auto address=reinterpret_cast<uintptr_t>(text->data());
            const auto aligned=(address+alignof(void*)-1)&~uintptr_t(alignof(void*)-1);
            require(aligned%alignof(void*)==0&&aligned+sizeof(void*)<=address+text->capacity()+1,"SDK metadata destination is aligned and fits owned allocation; not merely alignment refusal");
            auto** destination=reinterpret_cast<void**>(aligned);
            require(eb.input().queryInterface(IEventList::iid,destination)==kInvalidArgument&&
                pb.queryInterface(IParameterChanges::iid,destination)==kInvalidArgument&&*text==original,"frozen actual SDK QI protects every inline/heap class metadata allocation");
        }
    }
    auto& fresh=events(ep);Event copied{};require(fresh.input().getEvent(0,copied)==kResultOk&&copied.noteOn.pitch==69,"healthy event retry after refusal");
    int32 index=-1;auto* outputQueue=fresh.outputParameters().addParameterData(id,index);require(outputQueue&&index==0,"healthy output queue");
    require(outputQueue->addPoint(3,.25,core.index)==kInvalidArgument&&core.index==99,"output SDK index cannot overwrite core");require(fresh.freeze()==1,"output destination failure recorded");
    auto& healthy=parameters(pp);auto* queue=healthy.getParameterData(0);int32 offset=-1;double value=-1;
    require(queue&&queue->getPoint(0,offset,value)==kResultOk&&offset==2&&value==.625&&healthy.freeze()==0,"healthy input retry exact point");
    // Host preflight rejects protected metadata before interpreting it as points/events.
    Pool::Plan p;EventPool::Plan e;EventBuses buses{};
    require(pp.preflight(8,reinterpret_cast<const Point*>(core.catalogue.classes.data()),1,p)==Admission::Invalid,"parameter source core alias rejected before read");
    require(ep.preflight(8,reinterpret_cast<const Record*>(core.catalogue.classes.data()),1,nullptr,0,buses,buses,e)==EventAdmission::Invalid,"event source core alias rejected before read");
    require(!core.processing->externalRange(core.key.data(),1,1)&&!core.parameters->externalRange(core.catalogue.classes.back().name.data(),1,1),"keeper caller boundary shares immutable protection");
    core.processing->detach(core,ep);core.parameters->detach(pp);
    core.processing->reclaim(core,[](Core&){throw std::runtime_error("unretained unexpected pin");});core.parameters->reclaim();
}
void retainedSdkReferencesAfterFacadeRetirement(){
    using Registry=ravet::vst3::draft::CoreRegistry<Core,2>;
    auto* registry=new Registry;auto* destructions=new size_t(0);auto core=std::make_shared<Core>(256,destructions);
    int identity=0;require(registry->reserve(&identity,core)==Registry::Result::Accepted,"exact core reservation");
    auto& ep=core->processing->attach(*core);auto& pp=core->parameters->attach();auto& eb=events(ep);auto& pb=parameters(pp);
    auto* eq=&eb.input();auto* pq=pb.getParameterData(0);eq->addRef();pq->addRef();require(eb.freeze()==0&&pb.freeze()==0,"actual event/parameter escaped references");
    core->processing->pinBeforeForeignTeardown(*core,[&](Core& origin){require(&origin==core.get()&&registry->pin(&identity,core)==Registry::Result::Accepted,"exact process pin before teardown");});
    require(core->parameters->observedRetention(),"input parameter retention also observed");
    core->processing->detach(*core,ep);core->parameters->detach(pp);
    bool pinned=false;require(registry->beginRetire(&identity,core,pinned)==Registry::Result::Accepted&&pinned&&registry->finishRetire(&identity,core)==Registry::Result::Accepted,"ordinary facade retirement retains containing core");
    Core* origin=core.get();std::weak_ptr<Core> weak=core;core.reset();require(!weak.expired()&&!*destructions,"original facade gone; core snapshot/catalogue/pools live");
    bool passed=false;std::thread worker([&]{allocations=deallocations=0;measured=true;
        void* interface=nullptr;
        const bool refused=eq->queryInterface(IEventList::iid,&origin->queryResult)==kInvalidArgument&&origin->queryResult==nullptr;
        const bool permitted=eq->queryInterface(IEventList::iid,&interface)==kResultOk&&interface==eq;
        if(interface)static_cast<IEventList*>(interface)->release();
        int32 offset=55;double value=66;const bool frozen=pq->getPoint(0,offset,value)==kInvalidArgument&&offset==55&&value==66;
        const bool released=eq->release()==1&&pq->release()==1;
        measured=false;passed=refused&&permitted&&frozen&&released&&!allocations&&!deallocations;
    });worker.join();require(passed&&!*destructions,"retained frozen SDK QI after facade retirement reads live core numeric snapshot with zero new/delete");
    require(registry->reopenKey(origin->key,true,core)==Registry::Result::Accepted&&core.get()==origin,"exact retained core reopened");
    core->processing->reclaim(*core,[&](Core& c){require(&c==origin&&registry->pin(&identity,core)==Registry::Result::Accepted,"sticky pin before released pool reclaim");});core->parameters->reclaim();
    require(core->processing->count(*core)==0&&core->parameters->count()==0&&!*destructions,"CONTROL reclaims pools; never unpins containing core");
    require(registry->beginRetire(&identity,core,pinned)==Registry::Result::Accepted&&pinned&&registry->finishRetire(&identity,core)==Registry::Result::Accepted,"reopened facade retires with same pin");core.reset();
    // Deliberate process-lifetime controlled registry. This is not a loaded
    // plug-in, DAW, native image, device or real-time deadline qualification.
}
}
int main(){try{
    fixedSnapshotAndWholeMetadata();std::cout<<"PASS fixed1544 metadata ranges/capacity/immutable publication\n";
    actualSdkMetadataAliasesAndHealthyRetry();std::cout<<"PASS actual SDK event/input/output/QI whole metadata aliases and healthy retries\n";
    retainedSdkReferencesAfterFacadeRetirement();std::cout<<"PASS exact core pin/facade retirement/retained SDK numeric snapshot/zero worker new-delete\n";
    return 0;
}catch(const std::exception& e){std::cerr<<e.what()<<'\n';return 1;}}
