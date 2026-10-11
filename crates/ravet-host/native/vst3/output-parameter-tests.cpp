#define RAVET_VST3_EVENT_LIST_TESTS
#include "BoundedEventLists.hpp"
#include <cstdlib>
#include <iostream>
#include <memory>
#include <new>
#include <stdexcept>
#include <vector>
using namespace Steinberg;
using namespace Steinberg::Vst;
using namespace ravet::vst3::processing_draft;
using Point=ravet::vst3::parameters_draft::Point;
namespace ravet::vst3::processing_draft {
struct EventListTestAccess {
    static auto& listRefs(EventBatch& batch){return batch.parameters_.refs;}
    static auto& queueRefs(EventBatch& batch){return batch.parameterQueues_[0].refs;}
    static auto* points(EventBatch& batch){return batch.parameterPoints_.data();}
};
}
namespace {
thread_local bool measured=false;
thread_local size_t allocations=0,deallocations=0;
void require(bool good,const char* message){if(!good){std::cerr<<"FAIL "<<message<<'\n';throw std::runtime_error(message);}}
std::unique_ptr<EventPool> pool(){auto p=std::make_unique<EventPool>();
    const uint32_t ids[]{0,7,UINT32_MAX-1};require(p->configureParameters(ids,3),"copied actual controller IDs");return p;}
struct Cycle {
    EventBatch* batch=nullptr;
    explicit Cycle(EventPool& owner,uint32_t frames=65536){EventBuses buses{};
        require(owner.prepare(frames,nullptr,0,nullptr,0,buses,buses,batch)==EventAdmission::Ok,"whole process batch admission");}
    ~Cycle(){if(batch&&!batch->frozen())batch->freeze();}
    auto& list(){return batch->outputParameters();}
    IParamValueQueue* queue(uint32_t id=7){int32 index=-1;auto* q=list().addParameterData(id,index);
        require(q&&index>=0,"actual SDK writable parameter queue");return q;}
    uint32_t freeze(){return batch->freeze();}
};
void copiedCatalogueAndInterleavedSdkDispatch(){
    auto owner=pool();Cycle c(*owner);auto& list=c.list();int32 index=-1;
    uint32_t zero=0,seven=7,last=UINT32_MAX-1;
    auto* a=list.addParameterData(last,index);require(a&&index==0,"full unsigned actual ID");
    auto* b=list.addParameterData(zero,index);require(b&&index==1,"zero actual ID");
    auto* empty=list.addParameterData(seven,index);require(empty&&index==2,"empty queues retained in creation order");
    require(list.addParameterData(last,index)==a&&index==0&&list.getParameterCount()==3,"same ID returns existing queue");
    require(a->addPoint(100,.125,index)==kResultOk&&index==0,"first a point");
    require(b->addPoint(3,.75,index)==kResultOk&&index==0,"independent queue timing may move backwards globally");
    require(a->addPoint(120,.625,index)==kResultOk&&index==1,"a linked second point after interleaving");
    require(b->addPoint(4,0,index)==kResultOk&&index==1,"b linked second point");
    require(a->getParameterId()==last&&b->getParameterId()==0&&empty->getPointCount()==0,"actual queue identity/count");
    require(list.getParameterData(0)==a&&list.getParameterData(1)==b&&list.getParameterData(2)==empty,"SDK first-creation order");
    for(int32 i=0;i<2;++i){int32 offset=-1;double value=-1;
        require(a->getPoint(i,offset,value)==kResultOk&&offset==(i?120:100)&&value==(i?.625:.125),"a independent linked-ledger retrieval");
        require(b->getPoint(i,offset,value)==kResultOk&&offset==(i?4:3)&&value==(i?0:.75),"b independent linked-ledger retrieval");}
    require(c.freeze()==0,"interleaved SDK completion");
    uint32_t ids[4]{99,99,99,99},queues=99,count=99;Point points[5]{};points[4]={99,99,99};
    require(c.batch->copyOutputParameters(ids,4,points,5,queues,count)&&queues==3&&count==4,"whole queue/point publication");
    require(ids[0]==last&&ids[1]==0&&ids[2]==7&&ids[3]==99,"empty queues and caller tail preserve creation order");
    const Point wanted[]{{last,100,.125},{0,3,.75},{last,120,.625},{0,4,0}};
    for(size_t i=0;i<4;++i)require(points[i].id==wanted[i].id&&points[i].offset==wanted[i].offset&&points[i].value==wanted[i].value,"exact insertion ledger without sorting or merging");
    require(points[4].id==99&&points[4].value==99,"point tail unchanged");
    index=99;require(!list.getParameterCount()&&a->getParameterId()==kNoParamId&&
        a->addPoint(121,.5,index)==kInvalidArgument&&index==99&&c.batch->failureKind()==0,"frozen SDK operations preserve completed output/diagnostic");
}
void catalogueAdmissionAndPreservation(){
    auto owner=pool();const uint32_t duplicate[]{0,7,0},invalid[]{0,7,kNoParamId};
    require(!owner->configureParameters(duplicate,3)&&!owner->configureParameters(invalid,3)&&
        !owner->configureParameters(nullptr,1),"whole malformed catalogue refusal");
    std::array<uint32_t,1024> ids{};for(size_t i=0;i<ids.size();++i)ids[i]=uint32_t(i);
    bool worker=true;std::thread thread([&]{worker=owner->configureParameters(ids.data(),ids.size());});thread.join();
    require(!worker,"wrong CONTROL owner cannot replace catalogue");
    {Cycle c(*owner);require(!owner->configureParameters(ids.data(),ids.size()),"active process excludes catalogue edit");
        auto* q=c.queue(UINT32_MAX-1);require(q->getParameterId()==UINT32_MAX-1,"original catalogue survived all rejections");
        q->addRef();require(c.freeze()==0&&owner->observedRetention(),"individual queue marks retained batch");
        require(!owner->configureParameters(ids.data(),ids.size()),"retained queue forbids replacement");
        require(q->release()==1,"foreign release is nondestructive");}
    require(owner->configureParameters(ids.data(),ids.size()),"exact1024 copied IDs after release");
    ids.fill(kNoParamId);Cycle c(*owner);require(c.queue(1023)->getParameterId()==1023,"catalogue owns bytes independently of caller");
    int32 index=55;const uint32_t unknown=1024;
    require(!c.list().addParameterData(unknown,index)&&index==55&&c.freeze()==1,"unknown actual ID faults before queue mutation");
}
void malformedPointsAndProtectedOutputs(){
    auto owner=pool();
    const auto badPoint=[&](int32 offset,double value,uint32_t frames){Cycle c(*owner,frames);auto* q=c.queue();int32 index=88;
        const auto status=q->addPoint(offset,value,index);const auto failure=c.freeze();
        require(status==kInvalidArgument&&index==88&&failure==1,"malformed point leaves count/index unchanged and faults whole process");};
    badPoint(-1,.5,8);badPoint(8,.5,8);badPoint(1,.5,0);badPoint(0,-.125,8);badPoint(0,1.125,8);
    uint64_t nanBits=UINT64_C(0x7ff8000000000001),infinityBits=UINT64_C(0x7ff0000000000000);double nan=0,infinity=0;
    std::memcpy(&nan,&nanBits,8);std::memcpy(&infinity,&infinityBits,8);badPoint(0,nan,8);badPoint(0,infinity,8);
    for(const int32 next:{5,4}){Cycle c(*owner);auto* q=c.queue();int32 index=-1;
        require(q->addPoint(5,.5,index)==kResultOk,"ordered initial point");index=77;
        const auto status=q->addPoint(next,.75,index);require(status==kInvalidArgument&&index==77&&c.freeze()==1,"duplicate/backwards same-ID offset rejected");}
    {Cycle c(*owner);auto* q=c.queue();int32 index=0;require(q->addPoint(0,.5,index)==kResultOk,"get point initial");
        int32 offset=99;double value=99;const auto status=q->getPoint(1,offset,value);
        require(status==kInvalidArgument&&offset==99&&value==99&&c.freeze()==1,"invalid index preserves both scalar outputs");}
    {Cycle c(*owner);auto* q=c.queue();int32 index=0;require(q->addPoint(0,.5,index)==kResultOk,"protected output initial");
        auto* point=EventListTestAccess::points(*c.batch);const auto before=point[0];
        const auto status=q->getPoint(0,reinterpret_cast<int32&>(point[0].offset),point[0].value);
        require(status==kInvalidArgument&&point[0].offset==before.offset&&point[0].value==before.value&&c.freeze()==1,"SDK cannot write its own or other batch storage");}
    {Cycle c(*owner);uint32_t id=7;int32& alias=reinterpret_cast<int32&>(id);
        require(!c.list().addParameterData(id,alias)&&id==7&&c.freeze()==1,"ID input/index output alias refused before publication");}
    {Cycle c(*owner);auto* q=c.queue();auto* point=EventListTestAccess::points(*c.batch);
        const auto status=q->addPoint(0,.5,reinterpret_cast<int32&>(point[0].offset));
        require(status==kInvalidArgument&&c.freeze()==1,"protected addPoint index refused");}
    {Cycle c(*owner);auto* q=c.queue();int32 index=99;uint32_t id=7;
        std::thread thread([&]{require(q->addPoint(0,.5,index)==kInvalidArgument&&
            !c.list().addParameterData(id,index),"wrong-thread mutable SDK refusal");});thread.join();
        require(index==99&&c.freeze()==2,"wrong thread diagnostic with no index write");}
}
void actualQueueAndPointBounds(){
    auto owner=pool();std::array<uint32_t,65> ids{};for(size_t i=0;i<ids.size();++i)ids[i]=uint32_t(i);
    require(owner->configureParameters(ids.data(),ids.size()),"capacity catalogue");
    {Cycle c(*owner);int32 index=-1;for(uint32_t id=0;id<64;++id)
        require(c.list().addParameterData(id,index)&&index==int32(id),"all64 actual SDK output queues");
        uint32_t id=64;index=99;require(!c.list().addParameterData(id,index)&&index==99&&c.freeze()==4,"queue65 faults with no partial append");}
    {Cycle c(*owner);auto* q=c.queue(0);int32 index=-1;
        for(int32 i=0;i<2048;++i)require(q->addPoint(i,double(i)/2048,index)==kResultOk&&index==i,"all2048 actual SDK points");
        for(int32 i=0;i<2048;++i){int32 offset=-1;double value=-1;
            require(q->getPoint(i,offset,value)==kResultOk&&offset==i&&value==double(i)/2048,"every linked maximum point retrieved");}
        index=99;require(q->addPoint(2048,1,index)==kInternalError&&index==99&&c.freeze()==4,"point2049 faults without index/point publication");}
    {Cycle c(*owner);auto* a=c.queue(0);auto* b=c.queue(1);int32 index=-1;
        for(int32 i=0;i<2048;++i)require((i%2?a:b)->addPoint(i,.5,index)==kResultOk,"whole block capacity shared across IDs");
        index=99;require(b->addPoint(2048,.5,index)==kInternalError&&index==99&&c.freeze()==4,"per-queue storage does not multiply point cap");}
}
void queryRetentionAndSixteenBatchExhaustion(){
    auto owner=pool();std::array<IParamValueQueue*,16> held{};std::array<EventBatch*,16> batches{};
    for(size_t i=0;i<held.size();++i){Cycle c(*owner);auto* q=c.queue();int32 index=0;
        require(q->addPoint(0,double(i)/16,index)==kResultOk,"retained copied point");
        void* queried=nullptr;require(q->queryInterface(IParamValueQueue::iid.toTUID(),&queried)==kResultOk&&queried==q,"actual queue QueryInterface retain");
        held[i]=q;batches[i]=c.batch;require(c.freeze()==0,"retained queue freeze");
        for(size_t j=0;j<i;++j)require(batches[j]!=batches[i],"retained individual queue prevents complete batch reuse");}
    EventBuses profile{};EventBatch* result=reinterpret_cast<EventBatch*>(uintptr_t{0x1234});
    require(owner->prepare(0,nullptr,0,nullptr,0,profile,profile,result)==EventAdmission::Capacity&&
        result==reinterpret_cast<EventBatch*>(uintptr_t{0x1234}),"sixteen retained individual queues exhaust pool without overwrite");
    for(size_t i=0;i<held.size();++i){uint32_t ids[1]{99},queues=99,count=99;Point points[1]{};
        require(batches[i]->copyOutputParameters(ids,1,points,1,queues,count)&&queues==1&&count==1&&points[0].value==double(i)/16,"all sixteen retained ledgers remain immutable");}
    std::thread worker([&]{for(auto* q:held)require(q->release()==1,"worker final foreign release never deletes queue/batch");});worker.join();
    require(!owner->hasForeignReferences()&&owner->observedRetention(),"retention observation remains sticky after release");
    {Cycle c(*owner,0);auto* q=c.queue();int32 index=99;
        require(q->addPoint(0,1,index)==kResultOk&&index==0,"zero-frame and reused queue reinitialized");
        void* result=nullptr;require(c.list().queryInterface(IParameterChanges::iid.toTUID(),&result)==kResultOk&&result==&c.list(),"actual list QueryInterface");
        require(c.freeze()==0&&owner->hasForeignReferences(),"list retention alone retains complete batch");
        require(c.list().release()==1&&!owner->hasForeignReferences(),"list foreign release");}
    {Cycle c(*owner);auto* q=c.queue();auto& refs=EventListTestAccess::queueRefs(*c.batch);refs.store(UINT32_MAX);
        void* result=reinterpret_cast<void*>(uintptr_t{0x1234});const auto status=q->queryInterface(IParamValueQueue::iid.toTUID(),&result);
        refs.store(1);require(status==kInternalError&&!result&&c.freeze()==1,"reference saturation is a protocol fault without wrap");}
    {Cycle c(*owner);require(c.queue()->release()==1&&c.freeze()==1,"foreign release cannot destroy permanent host reference");}
}
void wholePublicationAndHeapFreeCycles(){
    auto owner=pool();Cycle c(*owner);int32 index=0;require(c.queue()->addPoint(5,.75,index)==kResultOk&&c.freeze()==0,"publication initial");
    uint32_t ids[2]{99,99},queues=99,count=99;Point points[2]{{99,99,99},{99,99,99}};
    require(!c.batch->copyOutputParameters(ids,0,points,2,queues,count)&&ids[0]==99&&points[0].value==99&&queues==99&&count==99,"queue capacity whole refusal");
    require(!c.batch->copyOutputParameters(ids,2,points,0,queues,count)&&ids[0]==99&&points[0].value==99&&queues==99&&count==99,"point capacity whole refusal");
    require(!c.batch->copyOutputParameters(ids,2,points,2,queues,queues)&&queues==99,"overlapping count destinations refuse whole publication");
    require(!c.batch->copyOutputParameters(ids,2,points,2,ids[0],count)&&ids[0]==99,"count/queue-ID output overlap refuses");
    require(!c.batch->copyOutputParameters(reinterpret_cast<uint32_t*>(points),2,points,2,queues,count)&&points[0].value==99,"queue-ID/point output overlap refuses");
    require(!c.batch->copyOutputParameters(ids,2,EventListTestAccess::points(*c.batch),2,queues,count)&&queues==99,"publication cannot overwrite protected pool");
    bool copied=true;std::thread thread([&]{copied=c.batch->copyOutputParameters(ids,2,points,2,queues,count);});thread.join();
    require(!copied&&ids[0]==99&&points[0].value==99&&queues==99&&count==99,"wrong-origin publication preserves all destinations");
    require(c.batch->copyOutputParameters(ids,2,points,2,queues,count)&&ids[0]==7&&points[0].value==.75&&queues==1&&count==1,"successful whole publication");
    bool all=true;allocations=deallocations=0;measured=true;
    for(uint32_t i=0;i<10000;++i){EventBuses profile{};EventBatch* batch=nullptr;uint32_t id=7;
        all&=owner->prepare(i%65,nullptr,0,nullptr,0,profile,profile,batch)==EventAdmission::Ok;
        if(!batch)break;
        auto* q=batch->outputParameters().addParameterData(id,index);all&=q!=nullptr;
        if(q){all&=q->addPoint(0,.5,index)==kResultOk;int32 offset=99;double value=99;
            all&=q->getPoint(0,offset,value)==kResultOk&&offset==0&&value==.5;}
        all&=batch->freeze()==0;
        all&=batch->copyOutputParameters(ids,2,points,2,queues,count)&&queues==1&&count==1;
    }
    measured=false;require(all&&allocations==0&&deallocations==0,"10000variable/zero-frame actual SDK output queue calls allocate/deallocate no ordinary C++ heap");
    std::cout<<"OBSERVED sizeof(EventPool)="<<sizeof(EventPool)<<";10000phases ordinaryCppHeap=0/0\n";
}
void combinedEventParameterPublication(){
    auto owner=pool();EventBuses buses{};buses[0]=16;EventBatch* batch=nullptr;
    require(owner->prepare(8,nullptr,0,nullptr,0,buses,buses,batch)==EventAdmission::Ok,"combined process admission");
    Event event{};event.type=Event::kDataEvent;event.sampleOffset=3;
    const uint8_t sysex[]{0xf0,0xf8,0x41,0xf7};event.data={4,DataEvent::kMidiSysEx,sysex};
    int32 index=99;const uint32_t id=7;auto* queue=batch->outputParameters().addParameterData(id,index);
    require(batch->output().addEvent(event)==kResultOk&&queue&&queue->addPoint(4,.625,index)==kResultOk&&batch->freeze()==0,"both actual SDK output directions complete");
    Record records[2]{};records[0].type=99;records[1].type=98;uint8_t payload[8];std::memset(payload,99,sizeof(payload));
    uint32_t ids[2]{99,98};Point points[2]{{99,99,99},{98,98,98}};
    RavetVst3ProcessingOutputs output{sizeof(output),0,records,2,99,payload,8,99,ids,2,99,points,2,99};
    const auto intact=[&]{return records[0].type==99&&payload[0]==99&&ids[0]==99&&points[0].value==99;};
    const auto refused=[&](RavetVst3ProcessingOutputs candidate){
        const auto before=candidate;
        require(!batch->copyOutputCombined(candidate)&&std::memcmp(&candidate,&before,sizeof(candidate))==0&&intact(),"combined late refusal preserves all outputs and all counts");};
    auto bad=output;bad.parameter_point_capacity=0;refused(bad);
    bad=output;bad.parameter_queue_capacity=0;refused(bad);
    bad=output;bad.payload_capacity=3;refused(bad);
    bad=output;bad.event_capacity=0;refused(bad);
    bad=output;bad.parameter_points=reinterpret_cast<Point*>(records);refused(bad);
    bad=output;bad.parameter_ids=reinterpret_cast<uint32_t*>(payload);refused(bad);
    bad=output;bad.payload=reinterpret_cast<uint8_t*>(records);refused(bad);
    bad=output;bad.parameter_ids=&bad.event_count;
    const auto before=bad;
    require(!batch->copyOutputCombined(bad)&&std::memcmp(&bad,&before,sizeof(bad))==0&&intact(),"count field cannot overlap an output buffer");
    bad=output;bad.reserved=1;refused(bad);
    bad=output;bad.struct_size--;refused(bad);
    bool success=true;std::thread thread([&]{success=batch->copyOutputCombined(output);});thread.join();
    require(!success&&intact()&&output.event_count==99&&output.parameter_point_count==99,"combined wrong-origin refusal preserves all destinations");
    require(batch->copyOutputCombined(output)&&output.event_count==1&&output.payload_used==4&&
        output.parameter_queue_count==1&&output.parameter_point_count==1,"one successful publication commits every output/count");
    require(records[0].type==Event::kDataEvent&&records[0].offset==3&&records[0].payload_bytes==4&&
        std::memcmp(payload,sysex,4)==0&&ids[0]==7&&points[0].id==7&&points[0].offset==4&&points[0].value==.625,"both independent event/payload/parameter output oracles");
    require(records[1].type==98&&payload[4]==99&&ids[1]==98&&points[1].value==98,"all four capacity suffixes unchanged");
}
}
void* operator new(std::size_t size){if(measured)++allocations;if(void* p=std::malloc(size?size:1))return p;throw std::bad_alloc();}
void* operator new[](std::size_t size){return ::operator new(size);}
void operator delete(void* pointer)noexcept{if(pointer&&measured)++deallocations;std::free(pointer);}
void operator delete[](void* pointer)noexcept{::operator delete(pointer);}
int main(){try{
    copiedCatalogueAndInterleavedSdkDispatch();std::cout<<"PASS copied catalogue/interleaved SDK queue dispatch\n";
    catalogueAdmissionAndPreservation();std::cout<<"PASS whole CONTROL catalogue admission/preservation\n";
    malformedPointsAndProtectedOutputs();std::cout<<"PASS malformed scalar/timing/protected/thread refusal\n";
    actualQueueAndPointBounds();std::cout<<"PASS actual64queue/2048whole-block point bounds\n";
    queryRetentionAndSixteenBatchExhaustion();std::cout<<"PASS actual query/refcounts/16retained individual queue batches\n";
    wholePublicationAndHeapFreeCycles();std::cout<<"PASS whole publication/10000zero ordinary heap phases\n";
    combinedEventParameterPublication();std::cout<<"PASS atomic combined event/payload/parameter/count publication\n";
    return 0;
}catch(const std::exception& error){std::cerr<<error.what()<<'\n';return 1;}}
