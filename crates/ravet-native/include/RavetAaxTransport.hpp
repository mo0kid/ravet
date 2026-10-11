#pragma once
#include "AAX_ITransport.h"
#include "RavetNativeTransport.h"
#include "RavetNativeScalars.hpp"
#include <array>
#include <cstring>

namespace ravet::aax {
enum class AaxTransportGetter:uint32_t {Playing,Tempo,Meter,Ticks,Resolution,Loop,Samples,Count};
enum class AaxTransportFault:uint32_t {None,InvalidHostData,InvalidProjection,ForeignException};
struct AaxTransportResult {
    AaxTransportFault fault=AaxTransportFault::None;
    AaxTransportGetter getter=AaxTransportGetter::Count;
    uint32_t attempted=0,returned=0;
    std::array<AAX_Result,7> statuses{};
    explicit operator bool()const noexcept{return fault==AaxTransportFault::None;}
};
// Output-last capture of individually queried fields, not a coherent host tuple.
// Original statuses are meaningful ONLY for returned getters; failed collection
// Result distinguishes attempted throws from returned SDK statuses. DETAILS has no
// per-flag availability; unknown recording/discontinuity/pre-roll are zero/default.
// Loop availability describes configured playback loop, not guaranteed execution.
class AaxTransportSnapshot {
    RavetNativeTransportV1 transport_{};
    uint32_t attempted_=0,returned_=0,available_=0,ticksPerQuarter_=0;
    std::array<AAX_Result,7> statuses_{};
    int64_t ticks_=0,loopStart_=0,loopEnd_=0;
    bool configuredLoop_=false;
public:
    const RavetNativeTransportV1 &transport()const noexcept{return transport_;}
    uint32_t attempted()const noexcept{return attempted_;}
    uint32_t returned()const noexcept{return returned_;}
    uint32_t available()const noexcept{return available_;}
    AAX_Result status(AaxTransportGetter getter)const noexcept{const auto i=uint32_t(getter);return i<statuses_.size()?statuses_[i]:0;}
    int64_t ticks()const noexcept{return ticks_;}
    uint32_t ticksPerQuarter()const noexcept{return ticksPerQuarter_;}
    int64_t loopStartTicks()const noexcept{return loopStart_;}
    int64_t loopEndTicks()const noexcept{return loopEnd_;}
    bool configuredLoop()const noexcept{return configuredLoop_;}
    friend AaxTransportResult aaxTransport(const AAX_ITransport *,AaxTransportSnapshot &);
};
// Call only with initialized model-owned versioned public proxy, in serialized
// native ProcessProc. Never borrow unversioned IMIDINode::GetTransport(). All
// foreign lifetime/thread/write obligations remain explicit; stop/join retirement.
// Getter errors affect only their own availability; throws or successful malformed
// tuples reject whole projection and leave output untouched. Foreign getter side
// effects cannot be rolled back. Helpers use fixed stack and allocate no heap.
// Initialized bool/i64 defaults cannot detect every successful unwritten field;
// successful typed SDK getters must honor output writes/valid representations.
inline AaxTransportResult aaxTransport(const AAX_ITransport *host,AaxTransportSnapshot &output){
    using native::scalar::finite;
    AaxTransportSnapshot candidate{};AaxTransportResult result{};
    if(!host){output=candidate;return result;}
    auto call=[&](AaxTransportGetter getter,auto query){
        const auto i=uint32_t(getter);result.attempted|=1u<<i;candidate.attempted_|=1u<<i;
        try{const auto status=query();result.returned|=1u<<i;candidate.returned_|=1u<<i;result.statuses[i]=candidate.statuses_[i]=status;if(status==AAX_SUCCESS)candidate.available_|=1u<<i;return true;}
        catch(...){result.fault=AaxTransportFault::ForeignException;result.getter=getter;return false;}
    };
    auto known=[&](AaxTransportGetter getter){return (candidate.available_&(1u<<uint32_t(getter)))!=0;};
    auto fail=[&](AaxTransportFault fault,AaxTransportGetter getter){result.fault=fault;result.getter=getter;return result;};
    bool playing=false;double tempo;const uint64_t nan=UINT64_C(0x7ff8000000000000);std::memcpy(&tempo,&nan,sizeof(tempo));
    int32_t numerator=0,denominator=0;int64_t samples=0;
    if(!call(AaxTransportGetter::Playing,[&]{return host->IsTransportPlaying(&playing);}))return result;
    if(!call(AaxTransportGetter::Tempo,[&]{return host->GetCurrentTempo(&tempo);}))return result;
    if(!call(AaxTransportGetter::Meter,[&]{return host->GetCurrentMeter(&numerator,&denominator);}))return result;
    if(!call(AaxTransportGetter::Ticks,[&]{return host->GetCurrentTickPosition(&candidate.ticks_);}))return result;
    if(!call(AaxTransportGetter::Resolution,[&]{return host->GetTicksPerQuarter(&candidate.ticksPerQuarter_);}))return result;
    if(!call(AaxTransportGetter::Loop,[&]{return host->GetCurrentLoopPosition(&candidate.configuredLoop_,&candidate.loopStart_,&candidate.loopEnd_);}))return result;
    // SDK native sample location is defined during playback only. Do not invent
    // stopped selection position or call it while playing is unknown/false.
    if(known(AaxTransportGetter::Playing)&&playing&& !call(AaxTransportGetter::Samples,[&]{return host->GetCurrentNativeSampleLocation(&samples);}))return result;
    auto &t=candidate.transport_;
    if(known(AaxTransportGetter::Playing)){t.present|=RAVET_TRANSPORT_PLAYING;if(playing)t.flags|=RAVET_TRANSPORT_FLAG_PLAYING;}
    if(known(AaxTransportGetter::Samples)){t.present|=RAVET_TRANSPORT_SAMPLES;t.sample_position=samples;}
    if(known(AaxTransportGetter::Tempo)){if(!finite(tempo)||tempo<=0.)return fail(AaxTransportFault::InvalidHostData,AaxTransportGetter::Tempo);t.present|=RAVET_TRANSPORT_TEMPO;t.tempo=tempo;}
    if(known(AaxTransportGetter::Meter)){
        if(numerator<1||numerator>65535||denominator<1||denominator>65535)return fail(AaxTransportFault::InvalidHostData,AaxTransportGetter::Meter);
        t.present|=RAVET_TRANSPORT_DETAILS|RAVET_TRANSPORT_SIGNATURE;t.signature_numerator=uint32_t(numerator);t.signature_denominator=uint32_t(denominator);
    }
    if(known(AaxTransportGetter::Resolution)&&!candidate.ticksPerQuarter_)return fail(AaxTransportFault::InvalidHostData,AaxTransportGetter::Resolution);
    if(known(AaxTransportGetter::Ticks)&&known(AaxTransportGetter::Resolution)){
        // Raw signed-i64 sidecar remains exact; native PPQ is explicitly rounded
        // f64 conversion/division over the FULL i64 domain, not tick quantization.
        const auto beat=double(candidate.ticks_)/double(candidate.ticksPerQuarter_);
        if(!finite(beat))return fail(AaxTransportFault::InvalidProjection,AaxTransportGetter::Ticks);
        t.present|=RAVET_TRANSPORT_BEATS;t.beat_position=beat;
    }
    if(known(AaxTransportGetter::Loop)){
        t.present|=RAVET_TRANSPORT_DETAILS;
        if(candidate.configuredLoop_){
            if(candidate.loopStart_>=candidate.loopEnd_)return fail(AaxTransportFault::InvalidHostData,AaxTransportGetter::Loop);
            t.flags|=RAVET_TRANSPORT_FLAG_LOOP_ACTIVE;
            if(t.present&RAVET_TRANSPORT_BEATS){
                const auto start=double(candidate.loopStart_)/double(candidate.ticksPerQuarter_),end=double(candidate.loopEnd_)/double(candidate.ticksPerQuarter_);
                if(!finite(start)||!finite(end)||start>=end)return fail(AaxTransportFault::InvalidProjection,AaxTransportGetter::Loop);
                t.present|=RAVET_TRANSPORT_BEAT_LOOP;t.beat_loop_start=start;t.beat_loop_end=end;
            }
        }
    }
    output=candidate;return result;
}
} // namespace ravet::aax
