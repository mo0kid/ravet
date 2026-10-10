#include "RavetVst3Audio.hpp"
#include <cstdio>
#include <cstdlib>
#include <cstring>
#include <limits>
#include <new>
namespace {bool measuring=false;size_t news=0,deletes=0;}
void *operator new(size_t bytes){if(measuring)++news;if(auto *p=std::malloc(bytes?bytes:1))return p;throw std::bad_alloc();}
void operator delete(void *p)noexcept{if(p&&measuring)++deletes;std::free(p);}
void operator delete(void *p,size_t)noexcept{::operator delete(p);}
using namespace Steinberg;using namespace Steinberg::Vst;using ravet::vst3::NativeAudioBusMap;
bool check(bool okay,const char *message){if(!okay)std::fprintf(stderr,"VST3 native audio: %s\n",message);return okay;}
RavetNativeProcessorConfigV1 config(){RavetNativeProcessorConfigV1 c{};c.version=1;c.maximum_frames=8;c.maximum_events=c.maximum_output_events=1024;c.bus_count=4;c.sample_rate=48000.;c.buses[0]={0,2};c.buses[1]={1,0};c.buses[2]={0,1};c.buses[3]={2,2};return c;}
template<class S> bool profile(){
    const uint32_t bits=sizeof(S)*8;NativeAudioBusMap map;
    if(!check(!map.valid()&&map.configure(config(),bits)&&map.inputBusCount()==2&&map.outputBusCount()==3&&map.busCount()==4,"instrument/sidechain/output-only/bidirectional compact directions"))return false;
    std::array<std::array<S,10>,8> samples{};for(auto &plane:samples)plane.fill(S(.125));
    S *mainOut[2]{samples[0].data(),samples[1].data()},*side[1]{samples[2].data()},*auxOut[1]{samples[3].data()},*mixedIn[2]{samples[4].data(),samples[5].data()},*mixedOut[2]{samples[6].data(),samples[7].data()};
    AudioBusBuffers inputs[2]{},outputs[3]{};inputs[0].numChannels=1;inputs[1].numChannels=2;outputs[0].numChannels=2;outputs[1].numChannels=1;outputs[2].numChannels=2;
    if constexpr(sizeof(S)==4){inputs[0].channelBuffers32=side;inputs[1].channelBuffers32=mixedIn;outputs[0].channelBuffers32=mainOut;outputs[1].channelBuffers32=auxOut;outputs[2].channelBuffers32=mixedOut;}
    else{inputs[0].channelBuffers64=side;inputs[1].channelBuffers64=mixedIn;outputs[0].channelBuffers64=mainOut;outputs[1].channelBuffers64=auxOut;outputs[2].channelBuffers64=mixedOut;}
    ProcessData data{};data.symbolicSampleSize=bits==32?kSample32:kSample64;data.numSamples=8;data.numInputs=2;data.numOutputs=3;data.inputs=inputs;data.outputs=outputs;
    std::array<RavetNativeProcessorBusV1,9> out{};const auto beforeSamples=samples;
    measuring=true;bool passed=true;for(unsigned cycle=0;cycle<10000;++cycle){if(!map.map(data,out)||out[0].input_channels!=0||out[0].output_channels!=2||out[0].inputs||out[0].outputs!=mainOut||out[1].inputs!=side||out[1].outputs||out[2].inputs||out[2].outputs!=auxOut||out[3].inputs!=mixedIn||out[3].outputs!=mixedOut||out[4].input_channels||out[8].outputs){passed=false;break;}}
    measuring=false;if(!check(passed&&news==0&&deletes==0&&samples==beforeSamples,"10000 compact native-precision mappings with zero C++ heap and no sample/tail changes"))return false;
    side[0]=mainOut[1];mixedIn[0]=auxOut[0];if(!check(map.map(data,out),"inputs may alias outputs across buses"))return false;side[0]=samples[2].data();mixedIn[0]=samples[4].data();
    for(unsigned test=0;test<13;++test){auto bad=data;S *original=mainOut[0];const auto originalChannels=outputs[2].numChannels;
        if(test==0)bad.numInputs=1;if(test==1)bad.numOutputs=2;if(test==2)bad.inputs=nullptr;if(test==3)bad.outputs=nullptr;if(test==4)outputs[2].numChannels=1;if(test==5)bad.symbolicSampleSize=bits==32?kSample64:kSample32;if(test==6)bad.numSamples=9;if(test==7)bad.numSamples=-1;if(test==8)mainOut[0]=nullptr;if(test==9)mainOut[0]=mainOut[1]+1;if(test==10)mainOut[0]=reinterpret_cast<S *>(uintptr_t(samples[0].data())+1);if(test==11)mainOut[0]=reinterpret_cast<S *>(std::numeric_limits<uintptr_t>::max()-(sizeof(S)-1));if(test==12)bad.numInputs=-1;
        out[0].input_channels=999;out[8].output_channels=77;const auto before=out;
        if(!check(!map.map(bad,out)&&std::memcmp(out.data(),before.data(),sizeof(out))==0,"complete shape/pointer/alias/overflow failures preserve native destination"))return false;
        mainOut[0]=original;outputs[2].numChannels=originalChannels;
    }
    // Fault cleanup clears only supported live prefixes, preserves tails and
    // avoids touching null/misaligned sample pointers in other channels.
    mainOut[0]=reinterpret_cast<S *>(uintptr_t(samples[0].data())+1);mainOut[1]=samples[1].data();mixedOut[1]=nullptr;
    map.silenceSupportedOutputs(data);
    if(!check(samples[0]==beforeSamples[0]&&samples[1][0]==S(0)&&samples[1][7]==S(0)&&samples[1][8]==S(.125)&&samples[1][9]==S(.125)&&outputs[0].silenceFlags==2&&outputs[2].silenceFlags==1,"fault cleanup skips invalid pointers, clears valid prefixes/tails and reports exact bits"))return false;
    mainOut[0]=samples[0].data();mixedOut[1]=samples[7].data();
    data.numSamples=0;for(auto &bus:inputs){bus.channelBuffers32=nullptr;bus.silenceFlags=UINT64_MAX;}for(auto &bus:outputs){bus.channelBuffers32=nullptr;bus.silenceFlags=UINT64_MAX;}
    if(!check(map.map(data,out)&&out[0].outputs==nullptr&&out[1].inputs==nullptr&&outputs[0].silenceFlags==UINT64_MAX,"exact zero-frame shape ignores sample arrays/flags"))return false;
    data.numInputs=data.numOutputs=0;data.inputs=data.outputs=nullptr;
    if(!check(map.map(data,out)&&out[3].input_channels==2&&out[3].output_channels==2&&out[3].inputs==nullptr,"completely omitted zero-frame audio retains native shape"))return false;
    data.numInputs=1;return check(!map.map(data,out),"partially omitted zero-frame direction rejects");
}
bool metadata(){NativeAudioBusMap map;auto c=config();if(!map.configure(c,32))return false;
    for(unsigned test=0;test<13;++test){auto bad=c;uint32_t bits=32;if(test==0)bad.version=2;if(test==1)bad.reserved=1;if(test==2)bad.bus_count=0;if(test==3)bad.bus_count=10;if(test==4)bad.maximum_frames=0;if(test==5)bad.maximum_frames=65537;if(test==6)bad.maximum_events=1025;if(test==7)bad.maximum_output_events=1025;if(test==8)bad.buses[1]={0,0};if(test==9)bad.buses[0].outputs=65;if(test==10)bad.buses[4].inputs=1;if(test==11){volatile uint64_t encoded=UINT64_C(0x7ff8000000000001);const uint64_t copy=encoded;std::memcpy(&bad.sample_rate,&copy,sizeof(copy));}if(test==12)bits=16;
        if(!check(!map.configure(bad,bits)&&map.valid()&&map.sampleBits()==32&&map.busCount()==4,"whole malformed config rejects and retains previous profile"))return false;
    }
    c.sample_rate=0.;return check(!map.configure(c,32),"finite positive rate required");
}
template<class S>bool maximum(){NativeAudioBusMap map;auto c=config();c.bus_count=9;for(auto &bus:c.buses)bus={64,64};if(!map.configure(c,sizeof(S)*8))return false;
    std::array<std::array<S,8>,576> samples{};std::array<std::array<S *,64>,9> pointers{};std::array<AudioBusBuffers,9> inputs{},outputs{};
    for(size_t bus=0;bus<9;++bus){inputs[bus].numChannels=outputs[bus].numChannels=64;for(size_t channel=0;channel<64;++channel)pointers[bus][channel]=samples[bus*64+channel].data();if constexpr(sizeof(S)==4)inputs[bus].channelBuffers32=outputs[bus].channelBuffers32=pointers[bus].data();else inputs[bus].channelBuffers64=outputs[bus].channelBuffers64=pointers[bus].data();}
    ProcessData data{};data.symbolicSampleSize=sizeof(S)==4?kSample32:kSample64;data.numSamples=8;data.numInputs=data.numOutputs=9;data.inputs=inputs.data();data.outputs=outputs.data();std::array<RavetNativeProcessorBusV1,9> mapped{};
    measuring=true;bool passed=true;for(unsigned cycle=0;cycle<1000;++cycle)if(!map.map(data,mapped)){passed=false;break;}measuring=false;
    return check(passed&&news==0&&deletes==0&&mapped[8].input_channels==64&&mapped[8].output_channels==64,"1000 maximum 576-channel alias-safe maps with zero C++ heap");
}
bool mismatchCleanup(){
    for(uint32_t preparedBits:{32u,64u}){
        NativeAudioBusMap map;auto c=config();c.bus_count=1;c.buses[0]={0,1};for(size_t index=1;index<9;++index)c.buses[index]={0,0};if(!map.configure(c,preparedBits))return false;
        std::array<float,10> narrow{};std::array<double,10> wide{};narrow.fill(99.f);wide.fill(1e200);float *np[]{narrow.data()};double *wp[]{wide.data()};AudioBusBuffers output{};output.numChannels=1;
        ProcessData data{};data.numOutputs=1;data.outputs=&output;data.numSamples=8;data.symbolicSampleSize=preparedBits==32?kSample64:kSample32;
        if(preparedBits==32)output.channelBuffers64=wp;else output.channelBuffers32=np;
        std::array<RavetNativeProcessorBusV1,9> mapped{};
        if(!check(!map.map(data,mapped),"mismatched process precision still rejects"))return false;
        map.silenceSupportedOutputs(data);
        if(!check((preparedBits==32?wide[0]==0.&&wide[7]==0.&&wide[8]==1e200:narrow[0]==0.f&&narrow[7]==0.f&&narrow[8]==99.f)&&output.silenceFlags==1,"fault cleanup uses declared SDK precision independently of prepared precision"))return false;
    }
    return true;
}
int main(){return mismatchCleanup()&&metadata()&&profile<float>()&&profile<double>()&&maximum<float>()&&maximum<double>()?0:1;}
