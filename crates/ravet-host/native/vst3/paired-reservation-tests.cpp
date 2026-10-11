#define RAVET_VST3_PAIRED_RESERVATION_TESTS
#include "BoundedEventLists.hpp"
#include <iostream>
#include <memory>
#include <stdexcept>
using namespace Steinberg;
using namespace Steinberg::Vst;
namespace p=ravet::vst3::parameters_draft;
namespace e=ravet::vst3::processing_draft;
namespace ravet::vst3::parameters_draft {
struct PairedReservationAccess {
    static const void* batches(const Pool& pool){return &pool.batches_;}
    static size_t bytes(const Pool& pool){return sizeof(pool.batches_);}
};
}
namespace ravet::vst3::processing_draft {
struct PairedReservationAccess {
    static const void* batches(const EventPool& pool){return &pool.batches_;}
    static size_t bytes(const EventPool& pool){return sizeof(pool.batches_);}
};
}
namespace {
void require(bool valid,const char* message){if(!valid)throw std::runtime_error(message);}
struct Snapshot {
    std::unique_ptr<uint8_t[]> bytes;size_t count;
    Snapshot(const void* source,size_t n):bytes(new uint8_t[n]),count(n){std::memcpy(bytes.get(),source,n);}
    bool equal(const void* source)const{return !std::memcmp(bytes.get(),source,count);}
};
e::Record record(){e::Record r{};r.struct_size=sizeof(r);r.type=Event::kNoteOnEvent;r.channel=1;r.pitch=69;r.note_id=123;r.velocity=.75f;return r;}
void cancellationCopiesNothingAndBothReservationsAdmitBeforeActivation(){
    auto events=std::make_unique<e::EventPool>();auto parameters=std::make_unique<p::Pool>();
    e::EventBuses buses{};buses[0]=16;auto input=record();const p::Point point{7,0,.625};
    e::EventPool::Plan ep;p::Pool::Plan pp;
    require(events->preflight(8,&input,1,nullptr,0,buses,buses,ep)==e::EventAdmission::Ok,"event pure plan");
    require(parameters->preflight(8,&point,1,pp)==p::Admission::Ok,"parameter pure plan");
    Snapshot es(e::PairedReservationAccess::batches(*events),e::PairedReservationAccess::bytes(*events));
    Snapshot ps(p::PairedReservationAccess::batches(*parameters),p::PairedReservationAccess::bytes(*parameters));
    {
        e::EventPool::Reservation er;p::Pool::Reservation pr;
        require(events->reserve(er)==e::EventAdmission::Ok&&parameters->reserve(pr)==p::Admission::Ok,"reserve both storage owners");
        require(!events->idle()&&!parameters->idle()&&es.equal(e::PairedReservationAccess::batches(*events))&&ps.equal(p::PairedReservationAccess::batches(*parameters)),"reserved batches byte-identical and not copied or activated");
        e::EventPool::Reservation nested;p::Pool::Reservation nestedP;
        require(events->reserve(nested)==e::EventAdmission::Busy&&parameters->reserve(nestedP)==p::Admission::Busy,"reentry before storage mutation");
    }
    require(events->idle()&&parameters->idle()&&es.equal(e::PairedReservationAccess::batches(*events))&&ps.equal(p::PairedReservationAccess::batches(*parameters)),"cancel restores admission only and copies nothing");
    {
        e::EventPool::Reservation er;p::Pool::Reservation pr;
        require(events->reserve(er)==e::EventAdmission::Ok&&parameters->reserve(pr)==p::Admission::Ok,"both leases admitted");
        auto& eb=events->activate(ep,er);auto& pb=parameters->activate(pp,pr);
        Event found{};require(eb.input().getEvent(0,found)==kResultOk&&found.noteOn.noteId==123,"event copied after both admissions");
        auto* queue=pb.getParameterData(0);int32 offset=-1;double value=-1;
        require(queue&&queue->getPoint(0,offset,value)==kResultOk&&offset==0&&value==.625,"parameter copied after both admissions");
        require(eb.freeze()==0&&pb.freeze()==0,"balanced paired freeze");
    }
    require(events->idle()&&parameters->idle(),"activation transferred each admission to batch freeze");
}
void exhaustedSecondPoolPreservesFirstReservationAndPlanFailures(){
    auto events=std::make_unique<e::EventPool>();auto parameters=std::make_unique<p::Pool>();
    e::EventBuses buses{};buses[0]=16;const auto input=record();const p::Point point{7,0,.625};
    std::array<p::Batch*,16> retained{};
    for(auto& batch:retained){require(parameters->prepare(8,&point,1,batch)==p::Admission::Ok,"fill retained second pool");batch->addRef();require(batch->freeze()==0,"retain whole queue batch");}
    Snapshot before(e::PairedReservationAccess::batches(*events),e::PairedReservationAccess::bytes(*events));
    {
        e::EventPool::Reservation er;p::Pool::Reservation pr;
        require(events->reserve(er)==e::EventAdmission::Ok&&parameters->reserve(pr)==p::Admission::Capacity,"second pool retained capacity refused");
    }
    require(events->idle()&&parameters->idle()&&before.equal(e::PairedReservationAccess::batches(*events)),"first pool exact unchanged after second capacity refusal");
    std::thread worker([&]{for(auto* batch:retained)require(batch->release()==1,"worker release host floor");});worker.join();
    e::EventPool::Plan ep;p::Pool::Plan pp;auto bad=input;bad.offset=8;
    require(events->preflight(8,&bad,1,nullptr,0,buses,buses,ep)==e::EventAdmission::Invalid&&events->idle(),"late event bad offset pure refusal");
    const p::Point duplicate[]{{7,0,.1},{7,0,.2}};
    require(parameters->preflight(8,duplicate,2,pp)==p::Admission::Invalid&&parameters->idle(),"whole conflicting parameter preflight");
    require(events->preflight(8,&input,1,nullptr,0,buses,buses,ep)==e::EventAdmission::Ok&&parameters->preflight(8,&point,1,pp)==p::Admission::Ok,"valid continuation");
    e::EventPool::Reservation er;p::Pool::Reservation pr;
    require(events->reserve(er)==e::EventAdmission::Ok&&parameters->reserve(pr)==p::Admission::Ok,"final worker releases permit paired reservation");
    auto& eb=events->activate(ep,er);auto& pb=parameters->activate(pp,pr);require(eb.freeze()==0&&pb.freeze()==0,"final balanced activation");
}
void immutableProtectionBlocksCoreSiblingAndBorrowedWrites(){
    auto events=std::make_unique<e::EventPool>();auto parameters=std::make_unique<p::Pool>();auto sibling=std::make_unique<e::EventPool>();
    std::array<uint64_t,32> core{};core.fill(UINT64_C(0x7171717171717171));
    Event callerEvent{};callerEvent.type=Event::kNoteOnEvent;
    int32 borrowedOffset=777;double borrowedValue=.125;void* borrowedInterface=reinterpret_cast<void*>(uintptr_t(77));
    p::Range ranges[5]{};
    require(p::range(core.data(),core.size(),sizeof(uint64_t),alignof(uint64_t),ranges[0])&&p::range(sibling.get(),1,sizeof(*sibling),alignof(e::EventPool),ranges[1])&&
        p::range(&callerEvent,1,sizeof(callerEvent),alignof(Event),ranges[2])&&p::range(&borrowedOffset,1,sizeof(borrowedOffset),alignof(int32),ranges[3])&&p::range(&borrowedValue,1,sizeof(borrowedValue),alignof(double),ranges[4]),"complete protection ranges");
    e::EventBuses buses{};buses[0]=16;auto input=record();const uint32 id=7;const p::Point point{id,0,.625};require(events->configureParameters(&id,1),"output known IDs");
    for(int kind=0;kind<6;++kind){
        e::EventPool::Plan ep;p::Pool::Plan pp;
        require(events->preflight(8,&input,1,nullptr,0,buses,buses,ep,ranges,5)==e::EventAdmission::Ok&&parameters->preflight(8,&point,1,pp,ranges,5)==p::Admission::Ok,"copied immutable protection plans");
        e::EventPool::Reservation er;p::Pool::Reservation pr;require(events->reserve(er)==e::EventAdmission::Ok&&parameters->reserve(pr)==p::Admission::Ok,"protected paired leases");
        auto& eb=events->activate(ep,er);auto& pb=parameters->activate(pp,pr);
        if(kind==0)require(eb.input().getEvent(0,callerEvent)==kInvalidArgument,"SDK result cannot overwrite borrowed event record");
        if(kind==1)require(eb.input().queryInterface(FUnknown::iid,reinterpret_cast<void**>(core.data()))==kInvalidArgument,"QI cannot overwrite native core");
        if(kind==2)require(eb.output().addEvent(callerEvent)==kInvalidArgument,"SDK event header cannot borrow caller metadata");
        if(kind==3){auto* q=pb.getParameterData(0);require(q&&q->getPoint(0,borrowedOffset,borrowedValue)==kInvalidArgument,"input queue cannot write borrowed caller outputs");}
        if(kind==4)require(eb.output().queryInterface(FUnknown::iid,reinterpret_cast<void**>(sibling.get()))==kInvalidArgument,"QI cannot overwrite sibling processing pool");
        if(kind==5){int32 index=-1;auto* q=eb.outputParameters().addParameterData(id,index);require(q&&q->addPoint(0,.5,borrowedOffset)==kInvalidArgument,"output queue cannot write borrowed diagnostic");}
        const auto ef=eb.freeze(),pf=pb.freeze();require((kind==3?pf:ef)==1,"original owned protocol failure recorded");
        require(core[0]==UINT64_C(0x7171717171717171)&&callerEvent.type==Event::kNoteOnEvent&&borrowedOffset==777&&borrowedValue==.125&&borrowedInterface==reinterpret_cast<void*>(uintptr_t(77)),"all protected bytes preserved");
    }
}
}
int main(){try{
 cancellationCopiesNothingAndBothReservationsAdmitBeforeActivation();std::cout<<"PASS two-phase pure preflight/paired reservations/byte-exact cancellation/no mutation before both leases\n";
 exhaustedSecondPoolPreservesFirstReservationAndPlanFailures();std::cout<<"PASS second retained-pool exhaustion preserves first storage, whole plan failures and worker-release continuation\n";
 immutableProtectionBlocksCoreSiblingAndBorrowedWrites();std::cout<<"PASS immutable SDK argument protection for core/sibling/original caller spans and original protocol faults\n";
}catch(const std::exception& ex){std::cerr<<ex.what()<<'\n';return 1;}}
