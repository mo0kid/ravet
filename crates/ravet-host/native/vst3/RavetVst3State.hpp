#ifndef RAVET_VST3_STATE_HPP
#define RAVET_VST3_STATE_HPP
#include "RavetVst3State.h"
#include "BoundedStateStream.hpp"
#include "pluginterfaces/vst/ivstcomponent.h"
#include "pluginterfaces/vst/ivsteditcontroller.h"

namespace ravet::vst3::state_draft {
// Internal synchronous transactions. Instance/provider supply whole-range,
// identity/lifecycle and registry admission before these foreign SDK calls.
class StateTransaction {
    struct FreezeBatch {
        StateStreamKeepers::Batch streams;
        ~FreezeBatch(){for(auto* stream:streams)if(stream&&!stream->frozen())stream->freeze();}
    };
    template<class Call> static bool call(BoundedStateStream& stream, uint32_t stage,
            Call operation, RavetVst3StateReport& report, bool optional=false) {
        stream.refreshProtection(); // Whole current stream/sibling capacities, before SDK attempt.
        report.stage=stage;report.flags|=RAVET_VST3_STATE_ATTEMPTED;report.flags&=~uint32_t(RAVET_VST3_STATE_RETURNED);
        try {report.native_result=operation(static_cast<Steinberg::IBStream*>(&stream));report.flags|=RAVET_VST3_STATE_RETURNED;}
        catch (...) {report.native_result=0;report.flags|=RAVET_VST3_STATE_THROWN;}
        stream.freeze();report.stream_failure=stream.failureKind();
        return !report.stream_failure&&!(report.flags&RAVET_VST3_STATE_THROWN)&&
            (report.native_result==Steinberg::kResultOk||(optional&&report.native_result==Steinberg::kNotImplemented));
    }
public:
    static RavetVst3StateReport initial() {
        RavetVst3StateReport result{};result.struct_size=sizeof(result);
        result.controller_capture_result=Steinberg::kNoInterface;return result;
    }
    static bool capture(Steinberg::Vst::IComponent& component,
            Steinberg::Vst::IEditController* controller, const uint8_t* classId,
            StateStreamKeepers& keepers, uint8_t* componentBytes,size_t componentCapacity,
            uint8_t* controllerBytes,size_t controllerCapacity,
            RavetVst3StateRecord& record,RavetVst3StateReport& report) {
        StateStreamKeepers::Spec specs[2]={{componentCapacity,true,nullptr,0},{controllerCapacity,true,nullptr,0}};
        FreezeBatch batch{keepers.reserve(specs,controller?2:1)};
        report=initial();
        if(!call(*batch.streams[0],RAVET_VST3_COMPONENT_CAPTURE,
                [&](Steinberg::IBStream* stream){return component.getState(stream);},report))return false;
        uint32_t mode=RAVET_VST3_CONTROLLER_ABSENT;
        if(controller) {
            const bool success=call(*batch.streams[1],RAVET_VST3_CONTROLLER_CAPTURE,
                [&](Steinberg::IBStream* stream){return controller->getState(stream);},report,true);
            report.controller_capture_result=report.native_result;
            if(!success)return false;
            mode=report.native_result==Steinberg::kNotImplemented?RAVET_VST3_CONTROLLER_UNSUPPORTED:RAVET_VST3_CONTROLLER_STATE;
        }
        RavetVst3StateRecord staged{};staged.struct_size=sizeof(staged);staged.controller_mode=mode;
        std::memcpy(staged.class_id,classId,16);
        staged.component_size=uint32_t(batch.streams[0]->size());
        staged.controller_size=mode==RAVET_VST3_CONTROLLER_STATE?uint32_t(batch.streams[1]->size()):0;
        if(staged.component_size)std::memcpy(componentBytes,batch.streams[0]->bytes(),staged.component_size);
        if(staged.controller_size)std::memcpy(controllerBytes,batch.streams[1]->bytes(),staged.controller_size);
        record=staged;report.stage=RAVET_VST3_STATE_NONE;report.native_result=Steinberg::kResultOk;return true;
    }
    static bool restore(Steinberg::Vst::IComponent& component,
            Steinberg::Vst::IEditController* controller,StateStreamKeepers& keepers,
            const RavetVst3StateRecord& record,const uint8_t* componentBytes,
            const uint8_t* controllerBytes,RavetVst3StateReport& report) {
        const size_t componentCapacity=std::max(size_t(record.component_size),size_t(1));
        const size_t controllerCapacity=std::max(size_t(record.controller_size),size_t(1));
        StateStreamKeepers::Spec specs[3]={{componentCapacity,false,componentBytes,record.component_size},
            {componentCapacity,false,componentBytes,record.component_size},
            {controllerCapacity,false,controllerBytes,record.controller_size}};
        const size_t count=controller?(record.controller_mode==RAVET_VST3_CONTROLLER_STATE?3:2):1;
        FreezeBatch batch{keepers.reserve(specs,count)};
        report=initial();report.flags=RAVET_VST3_STATE_MUTATING;
        report.controller_capture_result=record.controller_mode==RAVET_VST3_CONTROLLER_ABSENT?Steinberg::kNoInterface:
            record.controller_mode==RAVET_VST3_CONTROLLER_UNSUPPORTED?Steinberg::kNotImplemented:Steinberg::kResultOk;
        if(!call(*batch.streams[0],RAVET_VST3_COMPONENT_RESTORE,
                [&](Steinberg::IBStream* stream){return component.setState(stream);},report))return false;
        if(controller&&!call(*batch.streams[1],RAVET_VST3_CONTROLLER_COMPONENT_RESTORE,
                [&](Steinberg::IBStream* stream){return controller->setComponentState(stream);},report))return false;
        if(controller&&record.controller_mode==RAVET_VST3_CONTROLLER_STATE&&
                !call(*batch.streams[2],RAVET_VST3_CONTROLLER_RESTORE,
                [&](Steinberg::IBStream* stream){return controller->setState(stream);},report))return false;
        report.stage=RAVET_VST3_STATE_NONE;report.flags|=RAVET_VST3_STATE_PREPARE_REQUIRED;
        report.native_result=Steinberg::kResultOk;return true;
    }
};
}
#endif
