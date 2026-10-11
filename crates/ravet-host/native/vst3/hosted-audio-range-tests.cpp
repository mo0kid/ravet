#include "ProcessingBoundary.hpp"
#include "BoundedEventLists.hpp"
#include <iostream>
#include <memory>
#include <stdexcept>
namespace p=ravet::vst3::parameters_draft;
namespace e=ravet::vst3::processing_draft;
using namespace Steinberg;
using namespace Steinberg::Vst;
static void check(bool value){if(!value)throw std::runtime_error("hosted rich range contract");}
template<class S,class Block,class Input,class Output,class Boundary> void fullRangeAdmission() {
    std::array<std::array<S,4>,64> samplesIn{},samplesOut{};
    std::array<Input,64> inputs{};std::array<Output,64> outputs{};
    for(size_t i=0;i<64;++i){inputs[i]={samplesIn[i].data(),4,0};outputs[i]={samplesOut[i].data(),4,0};samplesOut[i].fill(S(.75));}
    Block block{sizeof(block),2,64,64,inputs.data(),outputs.data()};
    p::Point point{7,0,.5};e::Record event{};event.struct_size=sizeof(event);event.type=Event::kNoteOnEvent;event.pitch=69;event.note_id=1;event.velocity=.5;
    e::Record outEvent{};uint8_t payload[4]{},outPayload[4]{};uint32_t id=7;p::Point outPoint{};
    RavetVst3ContextRecord context{};RavetVst3ProcessingReport report{};report.struct_size=sizeof(report);
    RavetVst3ProcessingOutputs target{};target.struct_size=sizeof(target);target.events=&outEvent;target.event_capacity=1;
    target.payload=outPayload;target.payload_capacity=4;target.parameter_ids=&id;target.parameter_queue_capacity=1;target.parameter_points=&outPoint;target.parameter_point_capacity=1;
    Boundary boundary;
    check(boundary.admit(block,&point,1,&event,1,payload,4,&context,target,report,[](const void*,size_t,size_t){return true;}));
    check(boundary.count()==e::HostedCallerRanges&&boundary.count()>128);
    const auto callerCount=boundary.count();std::array<uint64_t,113> ownerBytes{};
    for(size_t i=0;i<ownerBytes.size();++i){p::Range r{};check(p::range(&ownerBytes[i],1,sizeof(uint64_t),alignof(uint64_t),r));check(boundary.appendOwner(r,callerCount));}
    check(boundary.count()==e::HostedMaximumRanges);
    auto parameters=std::make_unique<p::Pool>();auto events=std::make_unique<e::EventPool>();p::Pool::Plan pp;e::EventPool::Plan ep;e::EventBuses buses{};buses[0]=16;
    check(parameters->preflight(2,&point,1,pp,boundary.ranges(),129)==p::Admission::Capacity);
    check(events->preflight(2,&event,1,payload,4,buses,buses,ep,boundary.ranges(),129)==e::EventAdmission::Capacity);
    check(parameters->preflight(2,&point,1,pp,boundary.ranges(),boundary.count(),256)==p::Admission::Ok);
    check(events->preflight(2,&event,1,payload,4,buses,buses,ep,boundary.ranges(),boundary.count(),256)==e::EventAdmission::Ok);
    p::Pool::Reservation pr;e::EventPool::Reservation er;
    check(parameters->reserve(pr)==p::Admission::Ok&&events->reserve(er)==e::EventAdmission::Ok);
    auto& pb=parameters->activate(pp,pr);auto& eb=events->activate(ep,er);
    // Last copied owner extent must participate in escaped SDK destination guards.
    auto* last=reinterpret_cast<void**>(&ownerBytes.back());ownerBytes.back()=UINT64_C(0x7777777777777777);
    check(pb.queryInterface(FUnknown::iid,last)==kInvalidArgument&&ownerBytes.back()==UINT64_C(0x7777777777777777));
    check(eb.input().queryInterface(FUnknown::iid,last)==kInvalidArgument&&ownerBytes.back()==UINT64_C(0x7777777777777777));
    const auto parameterFault=pb.freeze(),eventFault=eb.freeze();check(parameterFault==1&&eventFault==1);
    Boundary collision;check(collision.admit(block,&point,1,&event,1,payload,4,&context,target,report,[](const void*,size_t,size_t){return true;}));
    for(size_t i=0;i<112;++i){p::Range r{};check(p::range(&ownerBytes[i],1,sizeof(uint64_t),alignof(uint64_t),r));check(collision.appendOwner(r,callerCount));}
    p::Range late{};check(p::range(samplesOut.back().data()+3,1,sizeof(S),alignof(S),late));
    unsigned dspCalls=0;if(collision.appendOwner(late,callerCount))++dspCalls;
    check(dspCalls==0&&collision.count()==253&&parameters->idle()&&events->idle());
    for(const auto& ch:samplesOut)for(auto value:ch)check(value==S(.75));
    // A late final-channel metadata alias is also rejected before admission publication.
    outputs.back().samples=reinterpret_cast<S*>(&report);outputs.back().capacity=4;
    Boundary invalid;check(!invalid.admit(block,&point,1,&event,1,payload,4,&context,target,report,[](const void*,size_t,size_t){return true;})&&invalid.count()==0);
}
int main(){try{
    fullRangeAdmission<float,RavetVst3AudioBlock,RavetVst3InputChannel,RavetVst3OutputChannel,e::HostedBoundary32>();
    fullRangeAdmission<double,RavetVst3HostedAudioBlock64,RavetVst3HostedInputChannel64,RavetVst3HostedOutputChannel64,e::HostedBoundary64>();
    std::cout<<"2 hosted 254-range rich boundary groups passed\n";
}catch(const std::exception& e){std::cerr<<e.what()<<'\n';return 1;}}
