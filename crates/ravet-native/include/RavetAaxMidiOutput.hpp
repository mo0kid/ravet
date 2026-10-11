#pragma once
#include "AAX_IMIDINode.h"
#include "RavetNativeMidiOutput.hpp"
#include <array>
#include <cstdint>
#include <cstring>

namespace ravet::aax {
// One exclusively owned AUDIO stage. Host packet pointers are synchronous input
// borrows, not retained references. Fixed bytes remain stable through callbacks.
// stage/deliver reentry rejects without touching active storage/readiness/results.
// Whole-list validation precedes the first post; accepted posts cannot roll back.
template<size_t MaximumPackets=1024>
class AaxMidiOutput final {
    static_assert(MaximumPackets>0&&MaximumPackets<=1024);
    std::array<AAX_CMidiPacket,MaximumPackets> packets_{};
    uint32_t count_=0;
    bool ready_=false, delivering_=false;
    static bool subset(const native::NativeMidiMessage& message) {
        const auto status=message.bytes[0]&0xf0;
        return status==0x80||status==0x90||status==0xa0||status==0xc0||
               status==0xd0||status==0xe0||(status==0xb0&&message.bytes[1]==0);
    }
public:
    enum class StageCode:uint32_t { Success,Invalid,Busy };
    struct StageResult {StageCode code=StageCode::Invalid;uint32_t count=0;explicit operator bool()const{return code==StageCode::Success;}};
    enum class DeliveryCode:uint32_t { Success,NotReady,InvalidNode,Refused,Exception,Busy };
    // hasStatus means returned>0; status is the LAST ACTUALLY RETURNED SDK
    // result. After a later throw it may be a preceding success0, never the
    // unreturned latest attempt. Exception + attempted>returned distinguishes it.
    struct DeliveryResult {
        DeliveryCode code=DeliveryCode::NotReady;
        uint32_t attempted=0,returned=0,posted=0;
        AAX_Result status=AAX_SUCCESS;
        bool hasStatus=false;
        explicit operator bool()const{return code==DeliveryCode::Success;}
    };
    // Serialized caller. Invalidates a completed batch, but cannot invalidate a
    // packet currently borrowed by a callback. Lifecycle also uses this guard.
    bool invalidate(){if(delivering_)return false;ready_=false;return true;}
    StageResult stage(const RavetNativeEventV1* records,uint32_t count,uint32_t frames) {
        if(delivering_)return {StageCode::Busy,0};
        ready_=false;
        if(count>MaximumPackets||frames>1024)return {};
        const auto first=reinterpret_cast<uintptr_t>(records),own=reinterpret_cast<uintptr_t>(this);
        const size_t bytes=size_t(count)*sizeof(*records);
        if(count&&(!records||first%alignof(RavetNativeEventV1)||bytes>UINTPTR_MAX-first||
           (first<own+sizeof(*this)&&own<first+bytes)))return {};
        std::array<AAX_CMidiPacket,MaximumPackets> candidate{};
        uint32_t previous=0;
        for(uint32_t i=0;i<count;++i) {
            RavetNativeEventV1 input{};std::memcpy(&input,records+i,sizeof(input));
            native::NativeMidiMessage message{};
            if((i&&input.offset<previous)||!native::nativeMidiOutput(input,frames,message)||!subset(message))return {};
            previous=input.offset;
            auto& packet=candidate[i];packet.mTimestamp=message.offset;packet.mLength=message.size;
            for(uint32_t b=0;b<message.size;++b)packet.mData[b]=message.bytes[b];
            packet.mIsImmediate=0;
        }
        packets_=candidate;count_=count;ready_=true;return {StageCode::Success,count};
    }
    DeliveryResult deliver(AAX_IMIDINode* node) {
        if(delivering_)return {DeliveryCode::Busy,0,0,0,AAX_SUCCESS,false};
        if(!ready_)return {};
        ready_=false;
        if(!node)return {DeliveryCode::InvalidNode,0,0,0,AAX_SUCCESS,false};
        delivering_=true;
        struct Guard {bool& active;~Guard(){active=false;}} guard{delivering_};
        DeliveryResult result{};result.code=DeliveryCode::Success;
        for(uint32_t i=0;i<count_;++i) {
            ++result.attempted;
            try {
                const auto status=node->PostMIDIPacket(&packets_[i]);
                ++result.returned;result.hasStatus=true;result.status=status;
                if(status!=AAX_SUCCESS){result.code=DeliveryCode::Refused;return result;}
                ++result.posted;
            } catch(...) {result.code=DeliveryCode::Exception;return result;}
        }
        return result;
    }
};
} // namespace ravet::aax
