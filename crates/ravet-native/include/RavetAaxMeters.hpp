#pragma once
#include "RavetNativeClient.hpp"
#include "RavetAaxPrecision.hpp"
#include "AAX.h"
#include "AAX_Enums.h"
#include <array>
#include <cstring>

namespace ravet::aax {
// Explicit linear host-tap projection; hints are metadata, never inferred dB.
enum class AaxMeterProjection:uint32_t { UnitClamp=0, FiniteFloat=1 };
struct AaxMeterBinding {
    AAX_CTypeID meterId=0;
    uint32_t telemetryId=0;
    const char* name=nullptr; // CONTROL borrow: terminated UTF8 <=255 bytes.
    AAX_EMeterType type=AAX_eMeterType_Other;
    AAX_EMeterOrientation orientation=AAX_eMeterOrientation_Default;
    AAX_EMeterBallisticType ballistics=AAX_eMeterBallisticType_NoDecay;
    AaxMeterProjection projection=AaxMeterProjection::UnitClamp;
};
struct AaxMeterDiagnostics {
    enum class Error:uint32_t { None, Binding, Metadata, Snapshot, Projection, Exception, Preflight };
    Error error=Error::Binding;
    bool cacheAttempted=false,cacheReturned=false,infoAttempted=false,infoReturned=false,
        cacheRetireAttempted=false,cacheRetireReturned=false,snapshotAttempted=false,snapshotReturned=false,published=false;
    int32_t infoStatus=1,snapshotStatus=1;
    uint32_t field=0;
};
// CONTROL prepares complete immutable binding metadata, using one temporary
// exact telemetry cache before schema/DSP/SDK owners. AUDIO stages a full native
// snapshot and all projected words before ANY tap write. No AUDIO allocation,
// cache/query/retirement or automatic host registration. Caller retains exact
// tables/library and serializes CONTROL preparation and exclusive AUDIO use.
// Result codes and exceptions stay separate; foreign ABI is nonthrowing.
template<size_t Count> class AaxPreparedMeters final {
    static_assert(Count>0&&Count<=64);
    std::array<AaxMeterBinding,Count> bindings_{};
    std::array<AAX_CTypeID,Count> meterIds_{};
    std::array<std::array<char,256>,Count> names_{};
    std::array<uint32_t,1024> ids_{};
    std::array<uint32_t,Count> indices_{};
    std::array<RavetNativeTelemetryInfoV1,Count> selectedInfo_{};
    std::array<RavetNativeTelemetryValueV1,1024> values_{};
    std::array<uint32_t,Count> words_{};
    uint32_t nativeCount_=0;
    bool valid_=false;
    AaxMeterDiagnostics diagnostic_{};
    AaxMeterDiagnostics candidateDiagnostic_{};
    static uint64_t bits(const double& value)noexcept{uint64_t result=0;std::memcpy(&result,&value,8);volatile uint64_t observed=result;return observed;}
    static bool finite(uint64_t value)noexcept{return (value&UINT64_C(0x7ff0000000000000))!=UINT64_C(0x7ff0000000000000);}
    static bool utf8(const uint8_t* data,uint32_t count)noexcept{
        for(uint32_t i=0;i<count;){const auto lead=data[i++];if(!lead)return false;if(lead<0x80)continue;
            uint32_t tail=0,code=0,minimum=0;
            if(lead>=0xc2&&lead<=0xdf){tail=1;code=lead&31;minimum=0x80;}
            else if(lead>=0xe0&&lead<=0xef){tail=2;code=lead&15;minimum=0x800;}
            else if(lead>=0xf0&&lead<=0xf4){tail=3;code=lead&7;minimum=0x10000;}
            else return false;
            if(tail>count-i)return false;
            for(uint32_t j=0;j<tail;++j){const auto next=data[i++];if((next&0xc0)!=0x80)return false;code=(code<<6)|(next&63);}
            if(code<minimum||code>0x10ffff||(code>=0xd800&&code<=0xdfff))return false;
        }return true;
    }
    bool inspect(const RavetNativeProcessorEntryV1* core,const RavetNativeTelemetryEntryV1* table){
        if(!core||core->version!=1||core->byte_size!=sizeof(*core)||core->reserved||
           (core->sample_bits!=32&&core->sample_bits!=64)||!core->create||!core->destroy||!core->prepare||!core->reset||!core->deactivate||!core->process||!core->timing||
           !table||table->version!=1||table->byte_size!=sizeof(*table)||table->sample_bits!=core->sample_bits||!table->telemetry_count||table->telemetry_count>1024||
           !table->processor_entry||!table->create_cache||!table->destroy_cache||!table->info||!table->snapshot)return false;
        if(table->processor_entry()!=core)return false;
        diagnostic_.cacheAttempted=true;void* cache=table->create_cache();diagnostic_.cacheReturned=true;if(!cache)return false;
        bool okay=true;
        try{
            nativeCount_=table->telemetry_count;
            for(uint32_t index=0;index<nativeCount_;++index){
                RavetNativeTelemetryInfoV1 info{};diagnostic_.field=index;diagnostic_.infoAttempted=true;diagnostic_.infoReturned=false;
                diagnostic_.infoStatus=table->info(cache,index,&info);diagnostic_.infoReturned=true;
                if(diagnostic_.infoStatus||info.reserved||info.name_length>255||info.unit_length>63||!utf8(info.name,info.name_length)||!utf8(info.unit,info.unit_length)||
                   !finite(bits(info.display_minimum))||!finite(bits(info.display_maximum))||info.display_minimum>info.display_maximum){okay=false;break;}
                for(uint32_t j=0;j<index;++j)if(ids_[j]==info.id)okay=false;
                if(!okay)break;ids_[index]=info.id;
                for(uint32_t j=0;j<Count;++j)if(bindings_[j].telemetryId==info.id){indices_[j]=index;selectedInfo_[j]=info;}
            }
            if(okay)for(auto index:indices_)if(index==UINT32_MAX)okay=false;
        }catch(...){diagnostic_.error=AaxMeterDiagnostics::Error::Exception;okay=false;}
        diagnostic_.cacheRetireAttempted=true;
        try{table->destroy_cache(cache);diagnostic_.cacheRetireReturned=true;}
        catch(...){diagnostic_.error=AaxMeterDiagnostics::Error::Exception;okay=false;}
        return okay;
    }
public:
    // Metadata pointers are borrowed by SDK descriptors; keep this owner at one
    // stable address through every borrower rather than copying internal names.
    AaxPreparedMeters(AaxPreparedMeters&&)=delete;
    AaxPreparedMeters& operator=(AaxPreparedMeters&&)=delete;
    AaxPreparedMeters(const RavetNativeProcessorEntryV1* core,const RavetNativeTelemetryEntryV1* table,const AaxMeterBinding* bindings){
        const auto first=reinterpret_cast<uintptr_t>(bindings);const auto bytes=sizeof(AaxMeterBinding)*Count;
        const auto coreFirst=reinterpret_cast<uintptr_t>(core),tableFirst=reinterpret_cast<uintptr_t>(table);
        if(!first||first%alignof(AaxMeterBinding)||bytes>UINTPTR_MAX-first)return;
        auto intersects=[&](uintptr_t a,size_t size){return a&&size<=UINTPTR_MAX-a&&first<a+size&&a<first+bytes;};
        if(intersects(coreFirst,sizeof(RavetNativeProcessorEntryV1))||intersects(tableFirst,sizeof(RavetNativeTelemetryEntryV1)))return;
        for(uint32_t i=0;i<Count;++i){
            const auto& item=bindings[i];diagnostic_.field=i;
            if(!item.name||uint32_t(item.type)>5||uint32_t(item.orientation)>3||uint32_t(item.ballistics)>1||uint32_t(item.projection)>1)return;
            uint32_t length=0;while(length<256&&item.name[length])++length;
            if(length==256||!utf8(reinterpret_cast<const uint8_t*>(item.name),length))return;
            for(uint32_t j=0;j<i;++j)if(bindings[j].meterId==item.meterId)return;
            bindings_[i]=item;meterIds_[i]=item.meterId;std::memcpy(names_[i].data(),item.name,length);bindings_[i].name=names_[i].data();
        }
        indices_.fill(UINT32_MAX);diagnostic_.error=AaxMeterDiagnostics::Error::Metadata;
        try{valid_=inspect(core,table);}catch(...){diagnostic_.error=AaxMeterDiagnostics::Error::Exception;}
        if(valid_)diagnostic_.error=AaxMeterDiagnostics::Error::None;
    }
    AaxPreparedMeters(const AaxPreparedMeters&)=delete;
    AaxPreparedMeters& operator=(const AaxPreparedMeters&)=delete;
    bool valid()const noexcept{return valid_;}
    const AAX_CTypeID* meterIds()const noexcept{return valid_?meterIds_.data():nullptr;}
    const AaxMeterBinding* binding(uint32_t index)const noexcept{return valid_&&index<Count?&bindings_[index]:nullptr;}
    const AaxMeterDiagnostics& diagnosticsStopped()const noexcept{return diagnostic_;}
    void beginAudio()noexcept{diagnostic_={};diagnostic_.error=AaxMeterDiagnostics::Error::Preflight;}
    const AaxMeterDiagnostics& candidateDiagnosticsStopped()const noexcept{return candidateDiagnostic_;}
    bool compatibleStopped(const RavetNativeProcessorEntryV1* core,const RavetNativeTelemetryEntryV1* table){
        AaxPreparedMeters candidate(core,table,bindings_.data());candidateDiagnostic_=candidate.diagnostic_;if(!candidate.valid_||candidate.nativeCount_!=nativeCount_)return false;
        for(uint32_t i=0;i<nativeCount_;++i)if(candidate.ids_[i]!=ids_[i])return false;
        if(candidate.indices_!=indices_)return false;
        for(uint32_t i=0;i<Count;++i){const auto& a=selectedInfo_[i];const auto& b=candidate.selectedInfo_[i];if(a.id!=b.id||a.name_length!=b.name_length||a.unit_length!=b.unit_length||bits(a.display_minimum)!=bits(b.display_minimum)||bits(a.display_maximum)!=bits(b.display_maximum)||std::memcmp(a.name,b.name,a.name_length)||std::memcmp(a.unit,b.unit,a.unit_length))return false;}
        return true;
    }
    // The whole tap pointer/range admission belongs to the owner before DSP.
    // A data-dependent projection fault is AFTER DSP and preserves all old taps;
    // this function never invents audio rollback or changes native process status.
    bool publish(native::NativeProcessorClient& owner,float* taps){
        diagnostic_={};diagnostic_.error=AaxMeterDiagnostics::Error::Snapshot;
        if(!valid_||!taps||owner.telemetryCount()!=nativeCount_)return false;
        diagnostic_.snapshotAttempted=true;
        try{diagnostic_.snapshotStatus=owner.snapshotTelemetry(values_.data(),nativeCount_);diagnostic_.snapshotReturned=true;}
        catch(...){diagnostic_.error=AaxMeterDiagnostics::Error::Exception;return false;}
        if(diagnostic_.snapshotStatus)return false;
        for(uint32_t i=0;i<nativeCount_;++i)if(values_[i].id!=ids_[i]||(values_[i].flags&~uint32_t(RAVET_TELEMETRY_NONFINITE_SAMPLES))||!finite(bits(values_[i].value))){diagnostic_.field=i;diagnostic_.error=AaxMeterDiagnostics::Error::Projection;return false;}
        for(uint32_t i=0;i<Count;++i){
            uint64_t value=bits(values_[indices_[i]].value);
            if(bindings_[i].projection==AaxMeterProjection::UnitClamp){if((value>>63)&&(value&UINT64_C(0x7fffffffffffffff)))value=0;else if(!(value>>63)&&value>UINT64_C(0x3ff0000000000000))value=UINT64_C(0x3ff0000000000000);}
            if(!AaxIeeePrecision::narrow(value,words_[i])){diagnostic_.error=AaxMeterDiagnostics::Error::Projection;diagnostic_.field=i;return false;}
        }
        for(uint32_t i=0;i<Count;++i)std::memcpy(taps+i,words_.data()+i,4);
        diagnostic_.published=true;diagnostic_.error=AaxMeterDiagnostics::Error::None;return true;
    }
};
template<> class AaxPreparedMeters<0> final {
    bool valid_=true;
public:
    AaxPreparedMeters(const RavetNativeProcessorEntryV1*,const RavetNativeTelemetryEntryV1*,const AaxMeterBinding* bindings):valid_(!bindings){}
    bool valid()const noexcept{return valid_;}
};
} // namespace ravet::aax
