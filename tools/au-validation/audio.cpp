#include "RavetAuAudio.hpp"
#include "RavetPerformance.h"
#include <cmath>
#include <cstdlib>
#include <iostream>
#include <new>
#include <stdexcept>
namespace allocation {
bool track=false;
size_t created=0,retired=0;
}
void *operator new(std::size_t bytes) {
    if(allocation::track)++allocation::created;
    if(auto result=std::malloc(bytes?bytes:1))return result;
    throw std::bad_alloc();
}
void *operator new[](std::size_t bytes) {return ::operator new(bytes);}
void operator delete(void *pointer) noexcept {if(pointer&&allocation::track)++allocation::retired;std::free(pointer);}
void operator delete[](void *pointer) noexcept {::operator delete(pointer);}
void operator delete(void *pointer,std::size_t) noexcept {::operator delete(pointer);}
void operator delete[](void *pointer,std::size_t) noexcept {::operator delete(pointer);}
namespace {
void require(bool value, const char *message) { if (!value) throw std::runtime_error(message); }
template <size_t N> struct Buffers {
    UInt32 count = N;
    std::array<AudioBuffer, N> channels{};
    AudioBufferList *list() { return reinterpret_cast<AudioBufferList *>(this); }
};
RavetNativeProcessorConfigV1 config(uint32_t count = 1, uint32_t frames = 64) {
    RavetNativeProcessorConfigV1 result{};
    result.version = 1; result.bus_count = count; result.maximum_frames = frames;
    result.maximum_events = 1024; result.sample_rate = 48000.; return result;
}
float *output(const ravet::au::NativeAuAudioBlock &block, uint32_t bus, uint32_t channel) {
    return static_cast<float *const *>(block.buses()[bus].outputs)[channel];
}
const float *input(const ravet::au::NativeAuAudioBlock &block, uint32_t bus, uint32_t channel) {
    return static_cast<const float *const *>(block.buses()[bus].inputs)[channel];
}
void mappingCases() {
    static_assert(offsetof(Buffers<2>, channels) == offsetof(AudioBufferList, mBuffers));
    std::array<std::array<float, 66>, 8> samples{};
    for (auto &channel : samples) channel.fill(99.f);
    Buffers<2> mainInput{2, {{{1,256,samples[0].data()},{1,256,samples[1].data()}}}};
    Buffers<1> detector{1, {{{1,256,samples[2].data()}}}};
    Buffers<1> mixedInput{1, {{{1,256,samples[3].data()}}}};
    Buffers<2> mainOutput{2, {{{1,256,samples[4].data()},{1,256,samples[5].data()}}}};
    Buffers<1> auxiliaryOutput{1, {{{1,256,samples[6].data()}}}};
    Buffers<1> mixedOutput{1, {{{1,256,samples[7].data()}}}};
    const AudioBufferList *inputs[]{mainInput.list(),detector.list(),mixedInput.list()};
    AudioBufferList *outputs[]{mainOutput.list(),auxiliaryOutput.list(),mixedOutput.list()};
    auto layout = config(4); layout.buses[0] = {2,2}; layout.buses[1] = {1,0}; layout.buses[2] = {0,1}; layout.buses[3] = {1,1};
    ravet::au::NativeAuAudioBusMap mapper; ravet::au::NativeAuAudioBlock block;
    require(mapper.configure(layout) && mapper.inputBusCount()==3 && mapper.outputBusCount()==3, "independent compact AU element counts");
    require(mapper.map(inputs,3,outputs,3,64,block) && block.busCount()==4 && block.frames()==64, "complete main/aux mapping");
    require(input(block,0,0)==samples[0].data() && output(block,0,1)==samples[5].data(), "main planar order");
    require(input(block,1,0)==samples[2].data() && !block.buses()[1].outputs, "input-only detector retains native order");
    require(!block.buses()[2].inputs && output(block,2,0)==samples[6].data(), "output-only auxiliary retains native order");
    require(input(block,3,0)==samples[3].data() && output(block,3,0)==samples[7].data(), "late mixed bus uses compact directional indices");
    auto retained = block; require(retained.buses()[0].outputs!=block.buses()[0].outputs && output(retained,3,0)==samples[7].data(), "mapped-block copy owns retargeted arrays");
    auto reject = [&](const char *message) {
        const auto views = block.buses(); const auto last = output(block,3,0);
        require(!mapper.map(inputs,3,outputs,3,64,block) && block.buses()==views && output(block,3,0)==last && block.frames()==64, message);
        for (const auto &channel : samples) require(channel[0]==99.f && channel[64]==99.f, "mapping faults never mutate samples/tails");
    };
    mixedOutput.channels[0].mData=samples[4].data();reject("late full output alias rejects before publication");
    mixedOutput.channels[0].mData=samples[4].data()+1;reject("late partial output overlap rejects before publication");
    mixedOutput.channels[0].mData=reinterpret_cast<void *>(uintptr_t(samples[7].data())+1);reject("late unaligned output rejects");
    mixedOutput.channels[0].mData=reinterpret_cast<void *>(std::numeric_limits<uintptr_t>::max()-3);reject("late output address overflow rejects");
    mixedOutput.channels[0].mData=samples[7].data();mixedOutput.channels[0].mDataByteSize=255;reject("late undersized output byte capacity rejects");mixedOutput.channels[0].mDataByteSize=256;
    mainOutput.count=1;mainOutput.channels[0].mNumberChannels=2;reject("single interleaved multichannel buffer rejects");mainOutput.count=2;mainOutput.channels[0].mNumberChannels=1;
    mixedInput.channels[0].mData=nullptr;reject("late disconnected input rejects");mixedInput.channels[0].mData=samples[3].data();
    require(!mapper.map(inputs,2,outputs,3,64,block) && output(block,3,0)==samples[7].data(), "wrong compact input count retains mapping");
    require(!mapper.map(inputs,3,outputs,4,64,block), "bus-index counts are not guessed as compact indices");
    require(!mapper.map(inputs,3,outputs,3,65,block) && block.frames()==64, "oversized active slice retains mapping");
    detector.channels[0].mData=samples[5].data();require(mapper.map(inputs,3,outputs,3,64,block) && input(block,1,0)==output(block,0,1), "input may alias cross-bus output");detector.channels[0].mData=samples[2].data();
    auto invalid=layout;invalid.buses[8]={1,0};require(!mapper.configure(invalid)&&mapper.busCount()==4,"unused native layout rejects without configuration mutation");
    invalid=layout;invalid.maximum_frames=0;require(!mapper.configure(invalid)&&mapper.maximumFrames()==64,"invalid prepared capacity retains profile");
    invalid=layout;invalid.buses[1]={0,0};require(!mapper.configure(invalid),"empty active native bus rejects");
    require(mapper.map(nullptr,0,nullptr,0,0,block) && block.busCount()==4 && !block.buses()[0].inputs && !block.buses()[0].outputs,"explicit zero-frame omission retains declared native layout");
    mixedOutput.channels[0].mData=nullptr;mixedOutput.channels[0].mDataByteSize=0;
    require(mapper.map(inputs,3,outputs,3,0,block) && !block.buses()[3].outputs,"exact zero-frame lists allow null samples");
    mixedOutput.channels[0].mNumberChannels=2;require(!mapper.map(inputs,3,outputs,3,0,block),"zero-frame exact lists still validate planar channel shape");
}
void maximumCases() {
    auto layout=config(9,1);for(auto &bus:layout.buses)bus={0,64};
    std::array<std::array<float,2>,576> samples{};std::array<Buffers<64>,9> storage{};std::array<AudioBufferList *,9> outputs{};
    for(uint32_t bus=0;bus<9;++bus){outputs[bus]=storage[bus].list();for(uint32_t channel=0;channel<64;++channel){samples[bus*64+channel].fill(99.f);storage[bus].channels[channel]={1,4,samples[bus*64+channel].data()};}}
    ravet::au::NativeAuAudioBusMap mapper;ravet::au::NativeAuAudioBlock block;
    require(mapper.configure(layout)&&mapper.map(nullptr,0,outputs.data(),9,1,block),"maximum nine buses/576 output channels bounded mapping");
    require(output(block,8,63)==samples[575].data(),"maximum last-channel source order");
    storage[8].channels[63].mData=samples[0].data();require(!mapper.map(nullptr,0,outputs.data(),9,1,block)&&output(block,8,63)==samples[575].data(),"maximum late overlap leaves complete prior map");
    bool okay=true;allocation::created=allocation::retired=0;allocation::track=true;
    for(uint32_t cycle=0;cycle<2000;++cycle){
        storage[8].channels[63].mData=samples[575].data();okay=mapper.map(nullptr,0,outputs.data(),9,1,block)&&okay;
        storage[8].channels[63].mData=samples[0].data();okay=!mapper.map(nullptr,0,outputs.data(),9,1,block)&&okay;
        okay=output(block,8,63)==samples[575].data()&&okay;
    }
    allocation::track=false;
    require(okay&&allocation::created==0&&allocation::retired==0,"2000 maximum accepted/late-fault map cycles use no C++ heap actions");
}
void actualGain() {
    auto layout=config();layout.buses[0]={2,2};
    const auto core=ravet_native_processor_v1(32);auto owner=core->create();require(owner&&!core->prepare(owner,&layout),"actual native Gain preparation");
    std::array<float,65> left{},right{},outLeft{},outRight{};left.fill(.25f);right.fill(-.5f);outLeft.fill(99.f);outRight.fill(99.f);
    Buffers<2> inputLists{2,{{{1,256,left.data()},{1,256,right.data()}}}},outputLists{2,{{{1,256,outLeft.data()},{1,256,outRight.data()}}}};
    const AudioBufferList *inputs[]{inputLists.list()};AudioBufferList *outputs[]{outputLists.list()};
    ravet::au::NativeAuAudioBusMap mapper;ravet::au::NativeAuAudioBlock block;require(mapper.configure(layout)&&mapper.map(inputs,1,outputs,1,64,block),"actual Gain AU/native pointer binding");
    RavetNativeProcessorResultV1 result{};require(!core->process(owner,block.buses(),1,64,nullptr,0,nullptr,nullptr,0,&result)&&result.status<2,"actual Gain process mapped AU buffers");
    require(outLeft[0]==.25f&&outRight[63]==-.5f&&outLeft[64]==99.f&&outRight[64]==99.f,"actual Gain audio and backing tails");
    outputLists.channels[0].mData=left.data();outputLists.channels[1].mData=right.data();require(mapper.map(inputs,1,outputs,1,64,block),"actual in-place AU/native binding");
    require(!core->process(owner,block.buses(),1,64,nullptr,0,nullptr,nullptr,0,&result)&&left[63]==.25f&&right[63]==-.5f,"actual Gain in-place rendering");
    core->deactivate(owner);core->destroy(owner);
}
void actualPerformance() {
    auto layout=config(1,512);layout.buses[0]={0,2};const auto core=ravet_performance_processor_v1(32);
    auto owner=core->create();require(owner&&!core->prepare(owner,&layout),"actual Performance native preparation");
    std::array<float,513> left{},right{};left.fill(99.f);right.fill(99.f);Buffers<2> outputLists{2,{{{1,2048,left.data()},{1,2048,right.data()}}}};AudioBufferList *outputs[]{outputLists.list()};
    ravet::au::NativeAuAudioBusMap mapper;ravet::au::NativeAuAudioBlock block;RavetNativeProcessorResultV1 result{};
    require(mapper.configure(layout)&&mapper.map(nullptr,0,nullptr,0,0,block)&&!core->process(owner,block.buses(),1,0,nullptr,0,nullptr,nullptr,0,&result),"actual zero-frame omitted native arrays");
    require(mapper.map(nullptr,0,outputs,1,300,block),"actual instrument AU/native mapping");
    RavetNativeEventV1 note{};note.offset=3;note.kind=RAVET_NATIVE_NOTE_ON;note.channel=1;note.key=69;note.value=double(64.f/127.f);
    require(!core->process(owner,block.buses(),1,300,&note,1,nullptr,nullptr,0,&result)&&result.status<2,"actual mapped instrument process");
    for(uint32_t i=0;i<200;++i){const auto n=i>=3?i-3:0;const auto expected=i<3?0.:std::sin(6.283185307179586*440./48000.*n)*(n+1)/240.*double(.2f)*double(64.f/127.f)/16.;require(std::abs(double(left[i])-expected)<1e-8&&left[i]==right[i],"actual mapped analytic timing/frequency/velocity/attack");}
    require(left[300]==99.f&&right[300]==99.f,"mapped instrument active-prefix tails");core->deactivate(owner);core->destroy(owner);
}
}
int main() {
    try {mappingCases();maximumCases();actualGain();actualPerformance();std::cout<<"Prepared AU f32 main/aux mapping, complete faults/retention, actual native Gain/Performance bindings passed\n";}
    catch(const std::exception &error){std::cerr<<error.what()<<'\n';return 1;}
}
