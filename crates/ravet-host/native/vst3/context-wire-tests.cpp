#include "ContextWire.hpp"
#include <array>
#include <cstring>
#include <iostream>
#include <stdexcept>
using namespace ravet::vst3::processing_draft;
namespace {
void require(bool good,const char* message){if(!good)throw std::runtime_error(message);}
Context complete(){Context r{};r.struct_size=sizeof(r);r.state=ContextFlags;r.sample_rate=96000;
    r.project_time_samples=INT64_MIN+1;r.system_time=INT64_MAX;r.continuous_time_samples=INT64_MIN;
    r.project_time_music=-123.25;r.bar_position_music=-128.5;r.cycle_start_music=-64.125;r.cycle_end_music=1024.375;
    r.tempo=137.125;r.time_signature_numerator=17;r.time_signature_denominator=32;
    r.chord_key=255;r.chord_root=254;r.chord_mask=0xfff;r.smpte_offset_subframes=INT32_MIN;
    r.frames_per_second=30;r.frame_rate_flags=3;r.samples_to_next_clock=INT32_MIN;return r;}
bool exact(const Context& r,const NativeContext& n){const auto s=r.state;
    return n.state==s&&n.sampleRate==r.sample_rate&&n.projectTimeSamples==r.project_time_samples&&
        n.systemTime==((s&RAVET_VST3_CONTEXT_SYSTEM_TIME)?r.system_time:0)&&
        n.continousTimeSamples==((s&RAVET_VST3_CONTEXT_CONTINUOUS)?r.continuous_time_samples:0)&&
        n.projectTimeMusic==((s&RAVET_VST3_CONTEXT_MUSIC)?r.project_time_music:0)&&
        n.barPositionMusic==((s&RAVET_VST3_CONTEXT_BAR)?r.bar_position_music:0)&&
        n.cycleStartMusic==((s&RAVET_VST3_CONTEXT_CYCLE)?r.cycle_start_music:0)&&
        n.cycleEndMusic==((s&RAVET_VST3_CONTEXT_CYCLE)?r.cycle_end_music:0)&&
        n.tempo==((s&RAVET_VST3_CONTEXT_TEMPO)?r.tempo:0)&&
        n.timeSigNumerator==((s&RAVET_VST3_CONTEXT_TIME_SIGNATURE)?r.time_signature_numerator:0)&&
        n.timeSigDenominator==((s&RAVET_VST3_CONTEXT_TIME_SIGNATURE)?r.time_signature_denominator:0)&&
        n.chord.keyNote==((s&RAVET_VST3_CONTEXT_CHORD)?r.chord_key:0)&&
        n.chord.rootNote==((s&RAVET_VST3_CONTEXT_CHORD)?r.chord_root:0)&&
        uint32_t(n.chord.chordMask)==((s&RAVET_VST3_CONTEXT_CHORD)?r.chord_mask:0)&&
        n.smpteOffsetSubframes==((s&RAVET_VST3_CONTEXT_SMPTE)?r.smpte_offset_subframes:0)&&
        n.frameRate.framesPerSecond==((s&RAVET_VST3_CONTEXT_SMPTE)?r.frames_per_second:0)&&
        n.frameRate.flags==((s&RAVET_VST3_CONTEXT_SMPTE)?r.frame_rate_flags:0)&&
        n.samplesToNextClock==((s&RAVET_VST3_CONTEXT_CLOCK)?r.samples_to_next_clock:0);
}
void everyIndependentPresenceCombination(){
    const std::array<uint32_t,13> bits{{RAVET_VST3_CONTEXT_PLAYING,RAVET_VST3_CONTEXT_CYCLE_ACTIVE,
        RAVET_VST3_CONTEXT_RECORDING,RAVET_VST3_CONTEXT_SYSTEM_TIME,RAVET_VST3_CONTEXT_MUSIC,
        RAVET_VST3_CONTEXT_TEMPO,RAVET_VST3_CONTEXT_BAR,RAVET_VST3_CONTEXT_CYCLE,
        RAVET_VST3_CONTEXT_TIME_SIGNATURE,RAVET_VST3_CONTEXT_SMPTE,RAVET_VST3_CONTEXT_CLOCK,
        RAVET_VST3_CONTEXT_CONTINUOUS,RAVET_VST3_CONTEXT_CHORD}};
    for(uint32_t mask=0;mask<(1u<<bits.size());++mask){auto r=complete();r.state=0;
        for(size_t i=0;i<bits.size();++i)if(mask&(1u<<i))r.state|=bits[i];
        NativeContext output{};
        require(sdkContext(r,96000,output)&&exact(r,output),"every actual SDK field/presence bit copied independently without synthetic defaults");}
}
void fullSignedUnitsAndNoInventedWhitelist(){
    auto r=complete();NativeContext output{};
    for(const int64_t signedValue:{INT64_MIN,int64_t(-1),int64_t(0),INT64_MAX}){
        r.project_time_samples=signedValue;r.continuous_time_samples=signedValue;r.system_time=signedValue;
        require(sdkContext(r,96000,output)&&exact(r,output),"sample positions and nanoseconds retain exact signed64 domain");}
    for(const int32_t signedValue:{INT32_MIN,int32_t(-17),int32_t(0),INT32_MAX}){
        r.smpte_offset_subframes=signedValue;r.samples_to_next_clock=signedValue;
        require(sdkContext(r,96000,output)&&exact(r,output),"negative clock and1/80frame SMPTE retain exact signed32 units");}
    for(const uint32_t fps:{1u,24u,25u,30u,48u,50u,60u,120u,UINT32_MAX})for(uint32_t flags=0;flags<4;++flags){
        r.frames_per_second=fps;r.frame_rate_flags=flags;
        require(sdkContext(r,96000,output)&&exact(r,output),"FPS examples are not an exhaustive whitelist; actual flags preserved");}
    r.time_signature_numerator=INT32_MAX;r.time_signature_denominator=3;
    r.cycle_start_music=r.cycle_end_music=-.5;
    require(sdkContext(r,96000,output)&&exact(r,output),"positive signature denominator need not be power of two; equal loop endpoints carrier allowed");
}
void wholeBadContextPreservesDestination(){
    const auto bad=[](Context r,double rate=96000){NativeContext output;std::memset(&output,0xa5,sizeof(output));
        std::array<uint8_t,sizeof(output)> before{};std::memcpy(before.data(),&output,sizeof(output));
        require(!sdkContext(r,rate,output)&&std::memcmp(before.data(),&output,sizeof(output))==0,"whole malformed context refusal without destination write");};
    auto r=complete();r.struct_size--;bad(r);r=complete();r.reserved=1;bad(r);
    for(uint32_t bit=0;bit<32;++bit)if(!(ContextFlags&(1u<<bit))){r=complete();r.state|=1u<<bit;bad(r);}
    r=complete();r.sample_rate=48000;bad(r);r=complete();r.sample_rate=0;bad(r);r=complete();r.sample_rate=-96000;bad(r);
    uint64_t nanBits=UINT64_C(0x7ff8000000000001),infBits=UINT64_C(0x7ff0000000000000);double nan=0,inf=0;
    std::memcpy(&nan,&nanBits,8);std::memcpy(&inf,&infBits,8);
    for(const double value:{nan,inf,-inf}){
        r=complete();r.sample_rate=value;bad(r);bad(complete(),value);
        for(auto field:{&Context::project_time_music,&Context::bar_position_music,
                       &Context::cycle_start_music,&Context::cycle_end_music,&Context::tempo}){
            r=complete();r.*field=value;bad(r);}
    }
    r=complete();r.cycle_start_music=r.cycle_end_music+1;bad(r);
    for(const double tempo:{0.,-1.}){r=complete();r.tempo=tempo;bad(r);}
    for(const int32_t value:{0,-1})for(auto field:{&Context::time_signature_numerator,&Context::time_signature_denominator}){r=complete();r.*field=value;bad(r);}
    for(auto field:{&Context::chord_key,&Context::chord_root}){r=complete();r.*field=256;bad(r);}
    r=complete();r.chord_mask=0x1000;bad(r);r=complete();r.chord_mask=0xffff;bad(r);
    r=complete();r.frames_per_second=0;bad(r);r=complete();r.frame_rate_flags=4;bad(r);
    // Unavailable scalars are not read/validated as offered data.
    r=complete();r.state=RAVET_VST3_CONTEXT_CYCLE_ACTIVE|RAVET_VST3_CONTEXT_RECORDING;
    r.project_time_music=r.bar_position_music=r.cycle_start_music=r.cycle_end_music=r.tempo=nan;
    r.time_signature_numerator=r.time_signature_denominator=-1;r.chord_key=r.chord_root=r.chord_mask=UINT32_MAX;
    r.frames_per_second=0;r.frame_rate_flags=UINT32_MAX;
    NativeContext output{};require(sdkContext(r,96000,output)&&exact(r,output),"absent poison storage ignored; transport bits do not invent presence");
}
void actualRequirementsAndWholeCallThroughput(){
    for(uint32_t flags=0;flags<2048;++flags)require(validateContextRequirements(flags),"all actual SDK requirement-bit combinations accepted");
    for(uint32_t bit=11;bit<32;++bit)require(!validateContextRequirements(1u<<bit),"unknown SDK requirement rejected");
    auto r=complete();NativeContext output{};
    for(uint32_t i=0;i<10000;++i){r.project_time_samples=int64_t(i)-5000;r.samples_to_next_clock=-int32_t(i);
        require(sdkContext(r,96000,output)&&exact(r,output),"10000owned block-start contexts preserve exact signed observations");}
}
}
int main(){try{
    everyIndependentPresenceCombination();std::cout<<"PASS8192 actual SDK independent presence combinations\n";
    fullSignedUnitsAndNoInventedWhitelist();std::cout<<"PASS full signed nanosecond/sample/clock/SMPTE and frame/signature domains\n";
    wholeBadContextPreservesDestination();std::cout<<"PASS whole malformed context/absence/destination preservation\n";
    actualRequirementsAndWholeCallThroughput();std::cout<<"PASS2048 request combinations/10000owned context constructions\n";
    return 0;
}catch(const std::exception& error){std::cerr<<error.what()<<'\n';return 1;}}
