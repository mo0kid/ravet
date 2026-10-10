#include "RavetVst3Transport.hpp"
#include <array>
#include <cstdio>
#include <cstdlib>
#include <cstring>
#include <limits>
#include <new>
namespace { bool measuring=false;size_t news=0,deletes=0; }
void *operator new(size_t bytes){if(measuring)++news;if(auto *p=std::malloc(bytes?bytes:1))return p;throw std::bad_alloc();}
void operator delete(void *p)noexcept{if(p&&measuring)++deletes;std::free(p);}
void operator delete(void *p,size_t)noexcept{::operator delete(p);}
using C=Steinberg::Vst::ProcessContext;
static bool check(bool okay,const char *message){if(!okay)std::fprintf(stderr,"VST3 transport: %s\n",message);return okay;}
void malformed(double &output,uint64_t bits){volatile uint64_t source=bits;const uint64_t copy=source;std::memcpy(&output,&copy,sizeof(copy));}
uint64_t bits(const double &value){uint64_t output=0;std::memcpy(&output,&value,sizeof(output));return output;}
RavetNativeTransportV1 sentinel(){RavetNativeTransportV1 value{};value.present=999;value.flags=99;value.sample_position=77;value.tempo=91.;return value;}
C context(){C value{};value.sampleRate=48000.;value.projectTimeSamples=std::numeric_limits<int64_t>::min();value.projectTimeMusic=-3.75;value.tempo=123.5;value.timeSigNumerator=7;value.timeSigDenominator=8;value.cycleStartMusic=-8.;value.cycleEndMusic=8.;return value;}
bool availability(){
    const std::array<uint32_t,10> flags{C::kPlaying,C::kRecording,C::kCycleActive,C::kProjectTimeMusicValid,C::kTempoValid,C::kTimeSigValid,C::kCycleValid,C::kBarPositionValid,C::kSystemTimeValid,UINT32_C(0x80000000)};
    for(uint32_t mask=0;mask<1024;++mask){auto c=context();for(size_t index=0;index<flags.size();++index)if(mask&(1u<<index))c.state|=flags[index];
        malformed(c.barPositionMusic,UINT64_C(0x7ff8000000000001));c.systemTime=std::numeric_limits<int64_t>::max();
        RavetNativeTransportV1 mapped=sentinel();if(!check(ravet::vst3::nativeTransport(&c,mapped),"all supported/unused availability combinations"))return false;
        uint32_t present=RAVET_TRANSPORT_PLAYING|RAVET_TRANSPORT_SAMPLES|RAVET_TRANSPORT_DETAILS;
        if(c.state&C::kProjectTimeMusicValid)present|=RAVET_TRANSPORT_BEATS;
        if(c.state&C::kTempoValid)present|=RAVET_TRANSPORT_TEMPO;
        if(c.state&C::kTimeSigValid)present|=RAVET_TRANSPORT_SIGNATURE;
        if((c.state&C::kCycleActive)&&(c.state&C::kCycleValid)&&(c.state&C::kProjectTimeMusicValid))present|=RAVET_TRANSPORT_BEAT_LOOP;
        uint32_t state=0;if(c.state&C::kPlaying)state|=RAVET_TRANSPORT_FLAG_PLAYING;if(c.state&C::kRecording)state|=RAVET_TRANSPORT_FLAG_RECORDING;if(c.state&C::kCycleActive)state|=RAVET_TRANSPORT_FLAG_LOOP_ACTIVE;
        if(!check(mapped.present==present&&mapped.flags==state&&mapped.sample_position==c.projectTimeSamples&&mapped.reserved==0,"presence/state/exact minimum i64"))return false;
        if(!check(mapped.beat_position==((present&RAVET_TRANSPORT_BEATS)?-3.75:0.)&&mapped.tempo==((present&RAVET_TRANSPORT_TEMPO)?123.5:0.)&&mapped.signature_numerator==((present&RAVET_TRANSPORT_SIGNATURE)?7u:0u)&&mapped.signature_denominator==((present&RAVET_TRANSPORT_SIGNATURE)?8u:0u)&&mapped.beat_loop_start==((present&RAVET_TRANSPORT_BEAT_LOOP)?-8.:0.)&&mapped.beat_loop_end==((present&RAVET_TRANSPORT_BEAT_LOOP)?8.:0.),"gated values/loop omission"))return false;
        if(!check(mapped.bar_start==0.&&mapped.bar_number==0&&mapped.tempo_increment==0.&&mapped.sample_loop_start==0&&mapped.sample_loop_end==0,"no fabricated unmappable fields"))return false;
    }
    auto c=context();c.state=C::kProjectTimeMusicValid|C::kTimeSigValid;c.projectTimeSamples=std::numeric_limits<int64_t>::max();c.timeSigNumerator=c.timeSigDenominator=65535;malformed(c.projectTimeMusic,UINT64_C(0x8000000000000000));
    RavetNativeTransportV1 mapped{};if(!check(ravet::vst3::nativeTransport(&c,mapped)&&mapped.sample_position==std::numeric_limits<int64_t>::max()&&mapped.signature_numerator==65535&&mapped.signature_denominator==65535&&bits(mapped.beat_position)==UINT64_C(0x8000000000000000),"maximum exact i64/u16/used signed zero"))return false;
    mapped=sentinel();if(!check(ravet::vst3::nativeTransport(nullptr,mapped)&&mapped.present==0&&mapped.flags==0&&bits(mapped.beat_position)==0,"null means unknown canonical snapshot"))return false;
    c=context();malformed(c.projectTimeMusic,UINT64_C(0x7ff8000000000001));malformed(c.tempo,UINT64_C(0x7ff0000000000000));malformed(c.cycleStartMusic,UINT64_C(0x7ff8000000000001));c.timeSigNumerator=c.timeSigDenominator=-1;
    return check(ravet::vst3::nativeTransport(&c,mapped)&&mapped.present==(RAVET_TRANSPORT_PLAYING|RAVET_TRANSPORT_SAMPLES|RAVET_TRANSPORT_DETAILS),"unavailable malformed fields ignored");
}
bool malformedContexts(){
    for(unsigned index=0;index<19;++index){auto c=context();c.state=C::kProjectTimeMusicValid|C::kTempoValid|C::kTimeSigValid|C::kCycleActive|C::kCycleValid;
        switch(index){
            case 0:malformed(c.sampleRate,UINT64_C(0x7ff8000000000001));break;
            case 1:malformed(c.sampleRate,UINT64_C(0x7ff0000000000000));break;
            case 2:c.sampleRate=0.;break;case 3:c.sampleRate=-1.;break;
            case 4:malformed(c.projectTimeMusic,UINT64_C(0x7ff8000000000001));break;
            case 5:malformed(c.projectTimeMusic,UINT64_C(0x7ff0000000000000));break;
            case 6:malformed(c.tempo,UINT64_C(0x7ff8000000000001));break;
            case 7:malformed(c.tempo,UINT64_C(0x7ff0000000000000));break;
            case 8:c.tempo=0.;break;case 9:c.tempo=-1.;break;
            case 10:c.timeSigNumerator=0;break;case 11:c.timeSigDenominator=-1;break;
            case 12:c.timeSigNumerator=65536;break;case 13:c.timeSigDenominator=65536;break;
            case 14:malformed(c.cycleStartMusic,UINT64_C(0x7ff8000000000001));break;
            case 15:malformed(c.cycleEndMusic,UINT64_C(0x7ff0000000000000));break;
            case 16:c.cycleEndMusic=c.cycleStartMusic;break;
            case 17:c.cycleEndMusic=c.cycleStartMusic-1.;break;
            case 18:c.state&=~C::kProjectTimeMusicValid;c.cycleEndMusic=c.cycleStartMusic;break;
        }
        const auto previous=sentinel();auto output=previous;
        if(!check(!ravet::vst3::nativeTransport(&c,output)&&std::memcmp(&output,&previous,sizeof(output))==0,"malformed context whole rejection/output preservation"))return false;
    }
    auto c=context();c.state=C::kCycleValid;c.cycleStartMusic=c.cycleEndMusic=0.;RavetNativeTransportV1 mapped{};
    if(!check(ravet::vst3::nativeTransport(&c,mapped)&&!(mapped.present&RAVET_TRANSPORT_BEAT_LOOP),"inactive zero extent is omitted"))return false;
    c=context();c.state=C::kCycleValid|C::kCycleActive;
    return check(ravet::vst3::nativeTransport(&c,mapped)&&(mapped.flags&RAVET_TRANSPORT_FLAG_LOOP_ACTIVE)&&!(mapped.present&RAVET_TRANSPORT_BEAT_LOOP),"positionless active bounds omitted with known loop state");
}
bool heap(){auto c=context();c.state=C::kPlaying|C::kProjectTimeMusicValid|C::kTempoValid|C::kTimeSigValid|C::kCycleActive|C::kCycleValid;RavetNativeTransportV1 output{};measuring=true;bool okay=true;for(unsigned index=0;index<10000;++index){c.projectTimeSamples+=1;okay=okay&&ravet::vst3::nativeTransport(&c,output)&&output.sample_position==c.projectTimeSamples;}measuring=false;return check(okay&&news==0&&deletes==0,"10000 complete mappings zero C++ new-delete");}
int main(){if(!availability()||!malformedContexts()||!heap())return 1;std::puts("VST3 transport 1024 availability profiles, exact extremes/signed zero, canonical null/omissions, malformed whole rejection and 10000 zero-C++heap mappings passed");return 0;}
