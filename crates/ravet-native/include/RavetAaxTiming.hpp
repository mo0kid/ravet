#pragma once
#include "RavetNativeProcessor.h"
#include "RavetNativeScalars.hpp"
#include "AAX_IController.h"
#include "AAX_Errors.h"
#include "AAX_IPropertyMap.h"
#include "AAX_Properties.h"
#include "AAX_Enums.h"
#include <atomic>
#include <cstdint>
#include <cstring>
#include <limits>

namespace ravet::aax {
// Explicit CONTROL timing policy; SDK latency requests alone never authorize DSP
// changes. Notification intake allocates nothing and calls no SDK/native getter.
// Caller serializes CONTROL reconciliation before next AUDIO after a delivered ACK.
// Native tails are diagnostic only: this profile invents no AAX tail property.
struct AaxNativeTiming {
    RavetNativeProcessorTimingV1 value{};
    int32_t latency=0;
    static bool canonical(const RavetNativeProcessorTimingV1 &v,AaxNativeTiming &out){
        if(v.latency_samples>uint32_t(INT32_MAX)||v.tail_kind>2||(v.tail_kind!=1&&v.tail_samples))return false;
        AaxNativeTiming next{v,int32_t(v.latency_samples)};out=next;return true;
    }
    static AAX_Result declareInitial(AAX_IPropertyMap *properties,uint32_t samples){
        if(!properties||samples>uint32_t(INT32_MAX))return AAX_ERROR_INVALID_ARGUMENT;
        return properties->AddProperty(AAX_eProperty_LatencyContribution,int32_t(samples));
    }
};
class AaxTimingMailbox final {
    // 24-bit saturating ACK sequence, compensation2bits, ACK eligibility, exhausted.
    static constexpr uint32_t Count=(1u<<24)-1u,CompShift=24,CompMask=3u<<CompShift,Eligible=1u<<26,Exhausted=1u<<27;
    std::atomic<uint32_t> word_{0};
    static_assert(std::atomic<uint32_t>::is_always_lock_free);
public:
    enum class Compensation:uint32_t{Unknown=0,Disabled=1,Enabled=2};
    struct Snapshot{uint32_t sequence=0;Compensation compensation=Compensation::Unknown;bool eligible=false,exhausted=false;};
    Snapshot snapshot()const{const auto w=word_.load(std::memory_order_acquire);return {w&Count,Compensation((w&CompMask)>>CompShift),bool(w&Eligible),bool(w&Exhausted)};}
    AAX_Result intake(AAX_CTypeID type,const void *data,uint32_t size){
        bool ack=false;Compensation comp=Compensation::Unknown;
        if(type==AAX_eNotificationEvent_SignalLatencyChanged){if(data||size)return AAX_ERROR_INVALID_ARGUMENT;ack=true;}
        else if(type==AAX_eNotificationEvent_DelayCompensationState){
            const auto first=reinterpret_cast<uintptr_t>(data);if(!first||size!=sizeof(int32_t)||first>UINTPTR_MAX-sizeof(int32_t))return AAX_ERROR_INVALID_ARGUMENT;
            int32_t v=-1;std::memcpy(&v,data,sizeof(v));if(v!=0&&v!=1)return AAX_ERROR_INVALID_ARGUMENT;comp=v?Compensation::Enabled:Compensation::Disabled;
        }else return AAX_ERROR_UNIMPLEMENTED;
        auto old=word_.load(std::memory_order_acquire);
        for(;;){auto next=old;if(old&Exhausted)return AAX_ERROR_INVALID_ARGUMENT;
            if(ack){if((old&Count)==Count){next=old|Exhausted;}else{next=(old&~Count)|((old&Count)+1);if((old&CompMask)==(uint32_t(Compensation::Disabled)<<CompShift))next&=~Eligible;else next|=Eligible;}}
            else{next=(old&~CompMask)|(uint32_t(comp)<<CompShift);if(comp==Compensation::Disabled)next&=~Eligible;}
            if(word_.compare_exchange_weak(old,next,std::memory_order_acq_rel,std::memory_order_acquire))return (next&Exhausted)?AAX_ERROR_INVALID_ARGUMENT:AAX_SUCCESS;
        }
    }
};
// Diagnostics copy only after AUDIO stop/join. Each returned flag validates its
// original SDK/native result; a throw is an attempted/unreturned operation.
struct AaxTimingDiagnostic {
    enum class Phase:uint32_t{Initial,Candidate,Request,Pending,Published,Adopted,Faulted};
    Phase phase=Phase::Initial;
    uint32_t generation=0,ackSequence=0;
    AaxTimingMailbox::Snapshot notices{};
    int32_t currentLatency=0,requestedLatency=0,observedLatency=0;
    bool nativeAttempted=false,nativeReturned=false,requestAttempted=false,requestReturned=false,getterAttempted=false,getterReturned=false;
    int32_t nativeStatus=0,requestStatus=0,getterStatus=0;
    RavetNativeProcessorTimingV1 current{},candidate{};
};
} // namespace ravet::aax
