#define RAVET_VST3_PARAMETER_CHANGES_TESTS
#include "BoundedParameterChanges.hpp"
#include <algorithm>
#include <cstdlib>
#include <cstring>
#include <iostream>
#include <memory>
#include <new>
#include <stdexcept>
#include <vector>
using namespace Steinberg;
using namespace Steinberg::Vst;
using namespace ravet::vst3::parameters_draft;
namespace ravet::vst3::parameters_draft {
struct ParameterChangesTestAccess {
    static auto& refs(Batch& batch){return batch.refs_;}
};
}
namespace {
thread_local bool measured=false;
thread_local size_t allocations=0,deallocations=0;
void require(bool condition,const char* message){if(!condition)throw std::runtime_error(message);}
double fromBits(uint64_t raw){volatile uint64_t runtime=raw;raw=runtime;double value=0;std::memcpy(&value,&raw,sizeof(value));return value;}
std::unique_ptr<Pool> pool(){return std::make_unique<Pool>();}
void groupingAndImmutableInput() {
    auto owner=pool();Batch* batch=nullptr;
    const std::array<Point,6> points{{{19,0,.2},{4,0,.3},{19,2,.5},{8,2,1},{4,3,.7},{19,7,.9}}};
    require(owner->prepare(8,points.data(),points.size(),batch)==Admission::Ok,"prepare grouping");
    IParameterChanges& sdk=*batch;
    require(sdk.getParameterCount()==3,"three SDK queues");
    const std::array<uint32_t,3> ids{19,4,8};const std::array<int32,3> counts{3,2,1};
    for(int32 q=0;q<3;++q){
        auto* queue=sdk.getParameterData(q);require(queue&&queue->getParameterId()==ids[q]&&queue->getPointCount()==counts[q],"first encounter queue identities");
        int32 n=0;
        for(const auto& point:points)if(point.id==ids[q]){
            int32 offset=-9;double value=-10;
            require(queue->getPoint(n++,offset,value)==kResultOk&&offset==int32(point.offset)&&value==point.value,"exact independent packed point oracle");
        }
    }
    require(batch->freeze()==0&&!owner->hasForeignReferences(),"complete freeze");
    require(sdk.getParameterCount()==0&&!sdk.getParameterData(0)&&batch->failureKind()==0,"frozen SDK reads don't alter completed diagnostic");
    require(owner->prepare(0,nullptr,0,batch)==Admission::Ok&&batch->getParameterCount()==0,"zero block empty batch");
    require(batch->freeze()==0,"empty freeze");
}
void wholeAdmissionRejections() {
    auto owner=pool();Batch* batch=nullptr;
    const std::array<Point,2> good{{{1,0,0},{1,7,1}}};
    require(owner->prepare(8,good.data(),good.size(),batch)==Admission::Ok,"first complete admission");
    auto* original=batch;Batch* output=reinterpret_cast<Batch*>(uintptr_t{0x1234});
    require(owner->prepare(8,good.data(),good.size(),output)==Admission::Busy&&output==reinterpret_cast<Batch*>(uintptr_t{0x1234}),"nested valid process rejected before touching pool");
    auto invalid=good;invalid[1].value=fromBits(UINT64_C(0x7ff8000000000001));
    require(owner->prepare(8,invalid.data(),invalid.size(),output)==Admission::Invalid,"late NaN whole preflight");
    require(original->getParameterCount()==1&&original->getParameterData(0)->getPointCount()==2&&original->freeze()==0,"rejected nested call preserves active batch");
    for(auto value:{-0.1,1.1,fromBits(UINT64_C(0x7ff0000000000000)),fromBits(UINT64_C(0xfff0000000000000))}){
        invalid=good;invalid[1].value=value;
        require(owner->prepare(8,invalid.data(),invalid.size(),output)==Admission::Invalid,"late normalized range");
    }
    invalid=good;invalid[1].id=kNoParamId;
    require(owner->prepare(8,invalid.data(),invalid.size(),output)==Admission::Invalid,"sentinel id");
    invalid=good;invalid[1].offset=8;
    require(owner->prepare(8,invalid.data(),invalid.size(),output)==Admission::Invalid,"offset at nonempty boundary");
    invalid=good;invalid[1].offset=0;
    require(owner->prepare(8,invalid.data(),invalid.size(),output)==Admission::Invalid,"same-ID duplicate offset rejected");
    invalid[1].id=2;
    require(owner->prepare(0,invalid.data(),invalid.size(),batch)==Admission::Ok,"different-ID sameoffset at zero frame");
    require(batch->freeze()==0,"zero-frame explicit points");
    invalid=good;invalid[0].offset=5;invalid[1].offset=3;invalid[1].id=2;
    require(owner->prepare(8,invalid.data(),invalid.size(),output)==Admission::Invalid,"global decreasing offsets");
    require(owner->prepare(8,reinterpret_cast<const Point*>(UINTPTR_MAX-7),2,output)==Admission::Invalid,"arithmetic overflowing source");
    require(owner->prepare(8,nullptr,1,output)==Admission::Invalid,"null nonempty source");
    require(owner->prepare(8,reinterpret_cast<const Point*>(owner.get()),1,output)==Admission::Invalid,"own source storage");
    require(owner->prepare(8,good.data(),2049,output)==Admission::Capacity,"point cap before source reads");
    std::array<Point,65> many{};for(size_t i=0;i<many.size();++i)many[i]={uint32_t(i),0,0};
    require(owner->prepare(8,many.data(),many.size(),output)==Admission::Capacity,"65 unique IDs");
    auto& aliasedOutput=*reinterpret_cast<Batch**>(owner.get());
    require(owner->prepare(8,good.data(),good.size(),aliasedOutput)==Admission::Invalid,"own output publication excluded");
    require(output==reinterpret_cast<Batch*>(uintptr_t{0x1234})&&!owner->observedRetention(),"every failure preserves caller handle and retention");
}
void maximumPackedBatch() {
    auto owner=pool();Batch* batch=nullptr;std::array<Point,2048> points{};
    for(size_t i=0;i<points.size();++i)points[i]={uint32_t(i%64),uint32_t(i),double(i%17)/16.};
    require(owner->prepare(2048,points.data(),points.size(),batch)==Admission::Ok&&batch->getParameterCount()==64,"maximum complete point/group admission");
    for(int32 q=0;q<64;++q){
        auto* queue=batch->getParameterData(q);require(queue->getParameterId()==uint32_t(q)&&queue->getPointCount()==32,"maximum group ID/count");
        for(int32 p=0;p<32;++p){
            int32 offset=-1;double value=-1;const auto index=size_t(q)+64*size_t(p);
            require(queue->getPoint(p,offset,value)==kResultOk&&offset==int32(index)&&value==double(index%17)/16.,"maximum independent packed oracle");
        }
    }
    require(batch->freeze()==0,"maximum completion");
}
void outputRangesAndMutationFaults() {
    auto owner=pool();Batch* batch=nullptr;Point point{3,0,.5};
    require(owner->prepare(1,&point,1,batch)==Admission::Ok,"range setup");
    auto* queue=batch->getParameterData(0);int32 offset=-44;double value=-88;
    require(queue->getPoint(-1,offset,value)==kInvalidArgument&&offset==-44&&value==-88,"invalid index output atomicity");
    require(batch->freeze()==1,"SDK swallowed protocol fault separate from returned process status");
    require(owner->prepare(1,&point,1,batch)==Admission::Ok,"reuse clear sticky phase");queue=batch->getParameterData(0);
    require(queue->getPoint(0,*reinterpret_cast<int32*>(owner.get()),value)==kInvalidArgument&&value==-88,"queue output excludes whole pool");
    require(batch->freeze()==1,"protected output fault");
    require(owner->prepare(1,&point,1,batch)==Admission::Ok,"overlap setup");queue=batch->getParameterData(0);
    value=-88;
    require(queue->getPoint(0,*reinterpret_cast<int32*>(&value),value)==kInvalidArgument&&value==-88,"overlapping SDK scalar outputs unchanged");
    require(batch->freeze()==1,"overlap protocol fault");
    require(owner->prepare(1,&point,1,batch)==Admission::Ok,"mutation setup");queue=batch->getParameterData(0);
    offset=-44;require(queue->addPoint(0,1,offset)==kInvalidArgument&&offset==-44,"immutable SDK queue input");
    int32 index=-11;ParamID id=5;
    require(!batch->addParameterData(id,index)&&index==-11&&batch->freeze()==3,"immutable SDK parent input +first sticky fault");
}
void queryOwnershipAndFrozenQueue() {
    auto owner=pool();Batch* batch=nullptr;Point point{8,0,.5};
    require(owner->prepare(1,&point,1,batch)==Admission::Ok,"QI setup");
    auto* queue=batch->getParameterData(0);void* result=reinterpret_cast<void*>(uintptr_t{0x1234});
    require(queue->queryInterface(nullptr,&result)==kInvalidArgument&&result==reinterpret_cast<void*>(uintptr_t{0x1234}),"null selector unchanged");
    require(queue->queryInterface(IParamValueQueue::iid,reinterpret_cast<void**>(owner.get()))==kInvalidArgument,"QI whole pool output protection");
    require(queue->queryInterface(reinterpret_cast<const char*>(owner.get()),&result)==kInvalidArgument,"QI whole pool selector protection");
    require(queue->queryInterface(IParamValueQueue::iid,reinterpret_cast<void**>(UINTPTR_MAX-3))==kInvalidArgument,"QI arithmetic overflow");
    alignas(void*) std::array<uint8_t,32> alias{};std::memcpy(alias.data(),IParamValueQueue::iid,sizeof(TUID));const auto before=alias;
    require(queue->queryInterface(reinterpret_cast<const char*>(alias.data()),reinterpret_cast<void**>(alias.data()))==kInvalidArgument&&alias==before,"QI selector/output overlap");
    TUID unknown{};std::memset(unknown,0x77,sizeof(unknown));
    require(queue->queryInterface(unknown,&result)==kNoInterface&&!result,"unknown QI clears external output");
    require(queue->queryInterface(IParameterChanges::iid,&result)==kNoInterface&&!result,"child cannot QI parent");
    require(queue->queryInterface(IParamValueQueue::iid,&result)==kResultOk&&result==queue,"retained child QI");
    require(batch->freeze()==1&&owner->observedRetention()&&owner->hasForeignReferences(),"individual child retention +swallowed malformed QI detected");
    int32 offset=-33;double value=-66;
    std::thread worker([&]{
        require(queue->getParameterId()==kNoParamId&&queue->getPointCount()==0,"frozen worker counts/identity unavailable");
        require(queue->getPoint(0,offset,value)==kInvalidArgument&&offset==-33&&value==-66,"frozen worker outputs unchanged");
        void* second=nullptr;require(queue->queryInterface(FUnknown::iid,&second)==kResultOk&&second,"frozen worker QI remains available");
        require(static_cast<IParamValueQueue*>(second)->release()==2&&queue->release()==1,"worker release never deletes payload");
    });worker.join();
    require(!owner->hasForeignReferences()&&batch->failureKind()==1&&owner->observedRetention(),"completed fault immutable +process retention sticky");
}
void referenceLimits() {
    auto owner=pool();Batch* batch=nullptr;Point point{3,0,.8};
    require(owner->prepare(1,&point,1,batch)==Admission::Ok,"reference floor setup");
    require(batch->release()==1&&batch->freeze()==1&&!owner->hasForeignReferences(),"invalid borrowed release preserves host reference");
    require(owner->prepare(1,&point,1,batch)==Admission::Ok,"saturation setup");
    // Injection models the impractical maximum owner count, not 4 billion actual owners.
    auto& refs=ParameterChangesTestAccess::refs(*batch);refs.store(UINT32_MAX-1);
    void* result=nullptr;
    require(batch->queryInterface(IParameterChanges::iid,&result)==kResultOk&&result==batch&&refs.load()==UINT32_MAX,"last representable QI lease");
    result=reinterpret_cast<void*>(uintptr_t{0x1234});
    require(batch->queryInterface(IParameterChanges::iid,&result)==kInternalError&&!result&&refs.load()==UINT32_MAX,"saturated QI refuses without fabricated lease");
    require(batch->addRef()==UINT32_MAX&&refs.load()==UINT32_MAX,"saturated AddRef cannot wrap");
    refs.store(1);require(batch->freeze()==1,"saturation first fault coherent");
    require(owner->prepare(1,&point,1,batch)==Admission::Ok,"concurrent final QI setup");refs.store(UINT32_MAX-1);
    std::atomic<bool> go{false};std::array<tresult,2> status{};std::array<void*,2> outputs{};
    std::thread a([&]{while(!go.load(std::memory_order_acquire))std::this_thread::yield();status[0]=batch->queryInterface(IParameterChanges::iid,&outputs[0]);});
    std::thread b([&]{while(!go.load(std::memory_order_acquire))std::this_thread::yield();status[1]=batch->queryInterface(IParameterChanges::iid,&outputs[1]);});
    go.store(true,std::memory_order_release);a.join();b.join();
    require((status[0]==kResultOk)+(status[1]==kResultOk)==1&&
        (status[0]==kInternalError)+(status[1]==kInternalError)==1&&
        (outputs[0]==batch)+(outputs[1]==batch)==1&&refs.load()==UINT32_MAX,"concurrent QI exactly one available lease");
    refs.store(1);require(batch->freeze()==1,"concurrent saturation fault");
}
void retainedExhaustionAndReclaim() {
    auto owner=pool();std::array<IParamValueQueue*,16> retained{};Batch* batch=nullptr;Point point{1,0,1};
    for(auto& queue:retained){
        require(owner->prepare(1,&point,1,batch)==Admission::Ok,"next reserved slot");queue=batch->getParameterData(0);queue->addRef();require(batch->freeze()==0,"retained valid freeze");
    }
    auto* previous=batch;
    require(owner->prepare(1,&point,1,batch)==Admission::Capacity&&batch==previous,"16 individually retained child slots exhausted before publication");
    std::thread worker([&]{for(auto* queue:retained)require(queue->release()==1,"all worker payload release retains host");});worker.join();
    require(owner->prepare(1,&point,1,batch)==Admission::Ok,"all-host slot reusable after foreign release");
    require(batch->addRef()==2&&batch->freeze()==0,"parent AddRef retention");
    require(owner->hasForeignReferences()&&batch->release()==1&&!owner->hasForeignReferences(),"parent reference included in slot proof");
}
void packedFreezeAndHeap() {
    auto owner=pool();Batch* batch=nullptr;Point point{3,0,.8};
    for(size_t i=0;i<1000;++i){
        require(owner->prepare(1,&point,1,batch)==Admission::Ok,"race admit");auto* queue=batch->getParameterData(0);queue->addRef();
        std::atomic<bool> go{false};
        std::thread worker([&]{while(!go.load(std::memory_order_acquire))std::this_thread::yield();int32 offset=-1;double value=-2;require(queue->getPoint(0,offset,value)==kInvalidArgument&&offset==-1&&value==-2,"wrong-thread race output immutable");queue->release();});
        go.store(true,std::memory_order_release);const auto failure=batch->freeze();
        require(failure==0||failure==2,"complete packed phase fault");worker.join();require(batch->failureKind()==failure,"no postfreeze fault publication");
    }
    allocations=deallocations=0;measured=true;
    bool valid=true;
    for(size_t i=0;i<10000;++i){
        valid&=owner->prepare(1,&point,1,batch)==Admission::Ok;int32 offset=-1;double value=-1;
        auto* queue=batch->getParameterData(0);valid&=queue&&queue->getPoint(0,offset,value)==kResultOk&&offset==0&&value==.8;
        valid&=batch->freeze()==0;
    }
    measured=false;
    require(valid&&allocations==0&&deallocations==0,"10000 phases no regular C++new/delete");
}
}
void* operator new(size_t bytes){if(measured)++allocations;if(void* p=std::malloc(bytes?bytes:1))return p;throw std::bad_alloc();}
void* operator new[](size_t bytes){return ::operator new(bytes);}
void operator delete(void* p)noexcept{if(measured&&p)++deallocations;std::free(p);}
void operator delete[](void* p)noexcept{::operator delete(p);}
int main(){try{
    groupingAndImmutableInput();wholeAdmissionRejections();maximumPackedBatch();outputRangesAndMutationFaults();
    queryOwnershipAndFrozenQueue();retainedExhaustionAndReclaim();referenceLimits();packedFreezeAndHeap();
    std::cout<<"PASS actual SDK immutable parameter changes:8 groups,2048 points/64 queues,16 retained slots,1000 freeze races,10000 heap-free phases\n";
    return 0;
}catch(const std::exception& error){measured=false;std::cerr<<error.what()<<'\n';return 1;}}
