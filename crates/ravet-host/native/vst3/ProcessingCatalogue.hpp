#ifndef RAVET_VST3_PROCESSING_CATALOGUE_HPP
#define RAVET_VST3_PROCESSING_CATALOGUE_HPP
#include "ContextWire.hpp"
#include "EventWire.hpp"
#include "RavetVst3Catalogue.hpp"
#include "pluginterfaces/vst/ivstcomponent.h"
#include "pluginterfaces/vst/ivsteditcontroller.h"
#include "pluginterfaces/vst/ivstmidicontrollers.h"
#include <algorithm>
#include <pthread.h>
#include <string>

namespace ravet::vst3::processing_draft {
enum class CatalogueStage { Thread, Parameters, BusCount, BusInfo, Selection, Mapping, ContextRequirements };
enum class CatalogueError { Invalid, Native, Thrown };
struct CatalogueFailure {
    CatalogueError kind=CatalogueError::Invalid;
    CatalogueStage stage=CatalogueStage::Parameters;
    Steinberg::tresult native=0;
    // Meaningful only for a failing SDK tresult-returning method, never for a
    // throw, malformed count/metadata or host-owned selection admission.
    bool returned=false;
};
template<class Call> auto catalogueCall(CatalogueStage stage,Call call)->decltype(call()) {
    try{return call();}catch(...){throw CatalogueFailure{CatalogueError::Thrown,stage,0,false};}
}
inline void catalogueAccepted(Steinberg::tresult status,CatalogueStage stage) {
    if(status!=Steinberg::kResultOk)throw CatalogueFailure{CatalogueError::Native,stage,status,true};
}
struct CopiedEventBus {
    uint32_t index=0,channels=0,type=0,flags=0;
    std::string name;
};
// CONTROL-only copied snapshot. No raw SDK interface/name/mapping buffer survives
// inspection. Selection is explicit and no bus is activated by discovery.
class ProcessingCatalogue {
    static constexpr uint32_t Unassigned=Steinberg::Vst::kNoParamId;
    std::array<CopiedEventBus,16> input_{},output_{};
    std::array<uint32_t,16*16*130> assignments_{};
    uint32_t inputs_=0,outputs_=0,requirements_=0;
    uint16_t selectedInput_=0,selectedOutput_=0;
    bool mapping_=false,contextRequirements_=false;
    static_assert(Steinberg::Vst::kCountCtrlNumber==130);
    static size_t at(uint32_t bus,uint32_t channel,uint32_t controller) {
        return (size_t(bus)*16+channel)*130+controller;
    }
public:
    static constexpr size_t MaximumBuses=16,MaximumChannels=16,MaximumControllers=130;
    ProcessingCatalogue(){assignments_.fill(Unassigned);}
    static ProcessingCatalogue inspect(Steinberg::Vst::IComponent& component,
            Steinberg::Vst::IMidiMapping* mapping,
            Steinberg::Vst::IProcessContextRequirements* context,
            const uint32_t* writableIds,size_t idCount,uint16_t selectedInput,uint16_t selectedOutput) {
        using namespace Steinberg;
        using namespace Steinberg::Vst;
        if(!pthread_main_np())throw CatalogueFailure{CatalogueError::Invalid,CatalogueStage::Thread};
        parameters_draft::Range ids{};
        if(idCount>1024||!parameters_draft::range(writableIds,idCount,sizeof(uint32_t),alignof(uint32_t),ids))
            throw CatalogueFailure{CatalogueError::Invalid,CatalogueStage::Parameters};
        for(size_t i=0;i<idCount;++i)if(writableIds[i]==Unassigned||
            std::find(writableIds,writableIds+i,writableIds[i])!=writableIds+i)
            throw CatalogueFailure{CatalogueError::Invalid,CatalogueStage::Parameters};
        ProcessingCatalogue candidate;
        candidate.selectedInput_=selectedInput;candidate.selectedOutput_=selectedOutput;
        for(auto direction:{kInput,kOutput}) {
            const auto count=catalogueCall(CatalogueStage::BusCount,[&]{return component.getBusCount(kEvent,direction);});
            if(count<0||count>16)throw CatalogueFailure{CatalogueError::Invalid,CatalogueStage::BusCount};
            const uint32_t allowed=count==16?0xffffu:(1u<<uint32_t(count))-1u;
            const auto selected=direction==kInput?selectedInput:selectedOutput;
            if(uint32_t(selected)&~allowed)throw CatalogueFailure{CatalogueError::Invalid,CatalogueStage::Selection};
            auto& entries=direction==kInput?candidate.input_:candidate.output_;
            (direction==kInput?candidate.inputs_:candidate.outputs_)=uint32_t(count);
            for(int32 index=0;index<count;++index) {
                BusInfo info{};
                catalogueAccepted(catalogueCall(CatalogueStage::BusInfo,[&]{return component.getBusInfo(kEvent,direction,index,info);}),CatalogueStage::BusInfo);
                if(info.mediaType!=kEvent||info.direction!=direction||info.channelCount<1||info.channelCount>16||
                   (info.busType!=kMain&&info.busType!=kAux)||
                   (info.flags&~uint32_t(BusInfo::kDefaultActive)))
                    throw CatalogueFailure{CatalogueError::Invalid,CatalogueStage::BusInfo};
                std::string name;
                try{name=ravet::vst3::text(info.name,true);}
                catch(const ravet::vst3::Failure&){throw CatalogueFailure{CatalogueError::Invalid,CatalogueStage::BusInfo};}
                entries[uint32_t(index)]={uint32_t(index),uint32_t(info.channelCount),uint32_t(info.busType),info.flags,std::move(name)};
            }
        }
        candidate.contextRequirements_=context!=nullptr;
        if(context) {
            candidate.requirements_=catalogueCall(CatalogueStage::ContextRequirements,[&]{return context->getProcessContextRequirements();});
            if(!validateContextRequirements(candidate.requirements_))
                throw CatalogueFailure{CatalogueError::Invalid,CatalogueStage::ContextRequirements};
        }
        candidate.mapping_=mapping!=nullptr;
        if(mapping)for(uint32_t bus=0;bus<candidate.inputs_;++bus)if(selectedInput&(1u<<bus))
            for(uint32_t channel=0;channel<candidate.input_[bus].channels;++channel)
                for(uint32_t controller=0;controller<MaximumControllers;++controller) {
                    ParamID id=Unassigned;
                    const auto status=catalogueCall(CatalogueStage::Mapping,[&]{return mapping->getMidiControllerAssignment(
                        int32(bus),int16(channel),CtrlNumber(controller),id);});
                    if(status==kResultFalse)continue; // Its output argument has no valid assignment.
                    catalogueAccepted(status,CatalogueStage::Mapping);
                    if(id==Unassigned||!idCount||std::find(writableIds,writableIds+idCount,id)==writableIds+idCount)
                        throw CatalogueFailure{CatalogueError::Invalid,CatalogueStage::Mapping};
                    candidate.assignments_[at(bus,channel,controller)]=id;
                }
        return candidate;
    }
    // Pure immutable prepared metadata snapshot, usable on AUDIO. The outer
    // exclusive session forbids concurrent CONTROL refresh; ranges copy numbers
    // rather than borrowing name objects into retained SDK interfaces.
    size_t protectionRanges(std::array<parameters_draft::Range,33>& result)const noexcept {
        using parameters_draft::range;
        std::array<parameters_draft::Range,33> candidate{};size_t count=0;
        if(!range(this,1,sizeof(*this),alignof(ProcessingCatalogue),candidate[count++]))return 0;
        for(bool output:{false,true}){
            const auto total=output?outputs_:inputs_;
            if(total>16)return 0;
            for(uint32_t i=0;i<total;++i){const auto& name=(output?output_:input_)[i].name;
                if(name.capacity()==std::numeric_limits<size_t>::max()||
                   !range(name.data(),name.capacity()+1,1,1,candidate[count++]))return 0;
            }
        }
        result=candidate;return count;
    }
    bool externalRange(const void* pointer,size_t bytes,size_t alignment)const noexcept {
        parameters_draft::Range requested{};
        if(!parameters_draft::range(pointer,bytes,1,alignment,requested))return false;
        std::array<parameters_draft::Range,33> owned{};const auto count=protectionRanges(owned);
        if(!count)return false;
        for(size_t i=0;i<count;++i)if(parameters_draft::overlaps(requested,owned[i]))return false;
        return true;
    }
    uint32_t inputCount()const{return inputs_;}
    uint32_t outputCount()const{return outputs_;}
    const CopiedEventBus& bus(bool output,uint32_t index)const {
        if(index>=(output?outputs_:inputs_))throw CatalogueFailure{CatalogueError::Invalid,CatalogueStage::Selection};
        return (output?output_:input_)[index];
    }
    EventBuses selectedBuses(bool output)const {
        EventBuses result{};const auto count=output?outputs_:inputs_;
        const auto mask=output?selectedOutput_:selectedInput_;
        for(uint32_t i=0;i<count;++i)if(mask&(1u<<i))result[i]=(output?output_:input_)[i].channels;
        return result;
    }
    bool hasMidiMapping()const{return mapping_;}
    bool hasContextRequirements()const{return contextRequirements_;}
    uint32_t contextRequirements()const{return requirements_;}
    uint32_t assignment(uint32_t bus,uint32_t channel,uint32_t controller)const {
        if(bus>=inputs_||channel>=input_[bus].channels||controller>=MaximumControllers||
           !(selectedInput_&(1u<<bus)))throw CatalogueFailure{CatalogueError::Invalid,CatalogueStage::Selection};
        return assignments_[at(bus,channel,controller)];
    }
};
}
#endif
