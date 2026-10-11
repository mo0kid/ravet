#include "RavetVst3Parameters.h"
#include "RavetVst3State.h"
#include <array>
#include <algorithm>
#include <cmath>
#include <cstring>
#include <iostream>
#include <stdexcept>
#include <thread>
#include <string>
#include <cstdlib>
#include <new>
namespace {thread_local bool measured=false;thread_local uint64_t allocations=0,deallocations=0;}
void* operator new(std::size_t bytes){if(measured)++allocations;if(auto* p=std::malloc(bytes?bytes:1))return p;throw std::bad_alloc();}
void* operator new[](std::size_t n){return ::operator new(n);}
void operator delete(void* p)noexcept{if(p&&measured)++deallocations;std::free(p);}
void operator delete[](void* p)noexcept{::operator delete(p);}
void operator delete(void* p,std::size_t)noexcept{::operator delete(p);}
void operator delete[](void* p,std::size_t)noexcept{::operator delete(p);}
static void require(bool x){if(!x)throw std::runtime_error("actual parameter owner/render contract");}
static RavetVst3ParameterReport diagnostic(){RavetVst3ParameterReport r{};r.struct_size=sizeof(r);r.native_result=777;return r;}
struct LinearOracle {
    float current=1,target=1;double start=1;uint32_t elapsed=240;bool bypass=false;
    void event(const RavetVst3ParameterPoint& p){
        if(p.id==2){bypass=p.value>=.5;return;}
        const float next=float(p.value*4);if(next==target)return;
        target=next;start=current;elapsed=0;
    }
    float next(){
        if(elapsed<240){++elapsed;current=elapsed==240?target:float(start+(double(target)-start)*double(elapsed)/240.);}
        return current;
    }
};
int main(int argc,char** argv){try{
    require(argc==3);const auto* api=ravet_vst3_provider_v2();const auto* parameters=ravet_vst3_parameter_provider_v1();
    require(parameters&&parameters->version==1&&parameters->struct_size==sizeof(*parameters)&&parameters->provider==api);
    for(uint32_t module=1;module<3;++module){
        const std::string path=argv[module];RavetVst3Module* parent=nullptr;int32_t native=999;
        require(api->open(reinterpret_cast<const uint8_t*>(path.data()),uint32_t(path.size()),&parent,&native)==RAVET_VST3_OK);
        RavetVst3FactoryRecord factory{};factory.struct_size=sizeof(factory);require(api->factory(parent,&factory)==RAVET_VST3_OK);
        uint32_t index=0;for(;index<factory.class_count;++index){RavetVst3ClassRecord c{};c.struct_size=sizeof(c);require(api->class_info(parent,index,&c)==RAVET_VST3_OK);if(std::string(c.category)=="Audio Module Class")break;}require(index<factory.class_count);
        for(uint32_t channels:{1u,2u}){
            RavetVst3Instance* child=nullptr;require(api->create(parent,index,&child,&native)==RAVET_VST3_OK);
            auto report=diagnostic();uint32_t count=999;require(parameters->refresh(child,&count,&report)==RAVET_VST3_OK&&count>=1&&count<=1024);
            bool level=false,bypass=false;for(uint32_t i=0;i<count;++i){RavetVst3ParameterRecord p{};p.struct_size=sizeof(p);require(parameters->info(child,i,&p)==RAVET_VST3_OK&&p.title[0]);level|=p.id==1;bypass|=p.id==2;}
            require(level&&(module!=1||bypass));double value=-99;
            RavetVst3StateRecord saved{};saved.struct_size=sizeof(saved);RavetVst3StateReport stateReport{};stateReport.struct_size=sizeof(stateReport);
            std::array<uint8_t,128> componentState{},controllerState{};
            require(ravet_vst3_state_provider_v1()->capture(child,&saved,componentState.data(),componentState.size(),controllerState.data(),controllerState.size(),&stateReport)==RAVET_VST3_OK);
            require(parameters->value(child,1,RAVET_VST3_PARAMETER_READ,0,&value,&report)==RAVET_VST3_OK);
            if(module==1){
                require(value==.25);require(parameters->value(child,1,RAVET_VST3_PARAMETER_EDIT,.75,&value,&report)==RAVET_VST3_OK&&value==.75);
                require(parameters->value(child,1,RAVET_VST3_PARAMETER_PLAIN,.75,&value,&report)==RAVET_VST3_OK&&value==3);
                require(parameters->value(child,1,RAVET_VST3_PARAMETER_NORMALIZE,2,&value,&report)==RAVET_VST3_OK&&value==.5);
                std::array<uint8_t,512> text{};uint32_t length=999;
                require(parameters->format(child,1,.5,text.data(),text.size(),&length,&report)==RAVET_VST3_OK&&length>0&&length<text.size());
                require(parameters->parse(child,1,text.data(),length,&value,&report)==RAVET_VST3_OK&&std::abs(value-.5)<1e-6);
            }
            RavetVst3AudioProfile profile{sizeof(profile),64,module==1?channels:0u,channels,48000.};
            require(parameters->prepare(child,&profile,&report)==RAVET_VST3_OK);require(api->start(child,&native)==RAVET_VST3_OK);
            RavetVst3Audio* audio=nullptr;require(api->begin_audio(child,&audio,&native)==RAVET_VST3_OK);
            count=555;auto prior=report;require(parameters->refresh(child,&count,&report)==RAVET_VST3_LIFECYCLE&&count==555&&std::memcmp(&report,&prior,sizeof(report))==0);
            std::array<std::array<float,70>,2> input{},output{};std::array<RavetVst3InputChannel,2> in{};std::array<RavetVst3OutputChannel,2> out{};
            for(uint32_t ch=0;ch<channels;++ch){for(uint32_t j=0;j<70;++j)input[ch][j]=float(j+1+ch)/128.f;output[ch].fill(77);in[ch]={input[ch].data(),70,0};out[ch]={output[ch].data(),70,0};}
            RavetVst3AudioBlock block{sizeof(block),64,profile.input_channels,channels,profile.input_channels?in.data():nullptr,out.data()};
            bool pass=false;std::thread worker([&]{
                measured=true;allocations=deallocations=0;
                auto r=diagnostic();auto old=r;
                const std::array<RavetVst3ParameterPoint,2> late={{{1,0,.5},{0xffffffffu,7,.3}}};
                require(parameters->process(audio,&block,late.data(),late.size(),&r)==RAVET_VST3_INVALID&&output[0][0]==77&&std::memcmp(&r,&old,sizeof(r))==0);
                const std::array<RavetVst3ParameterPoint,2> duplicate={{{1,0,.5},{1,0,.3}}};
                require(parameters->process(audio,&block,duplicate.data(),duplicate.size(),&r)==RAVET_VST3_INVALID&&output[0][0]==77&&std::memcmp(&r,&old,sizeof(r))==0);
                require(parameters->process(audio,&block,nullptr,0,reinterpret_cast<RavetVst3ParameterReport*>(output[0].data()))==RAVET_VST3_INVALID&&output[0][0]==77);
                require(parameters->process(audio,&block,nullptr,0,&r)==RAVET_VST3_OK&&r.native_result==0);
                // Actual controller edit above did not become ProcessData input.
                for(uint32_t ch=0;ch<channels;++ch)for(uint32_t j=0;j<64;++j)require(output[ch][j]==(module==1?input[ch][j]:0));
                const std::array<RavetVst3ParameterPoint,5> points={{{1,0,.5},{1,3,.125},{2,5,1},{2,12,0},{1,16,.25}}};
                const RavetVst3ParameterPoint performancePoint{1,0,.3};
                LinearOracle oracle;
                for(uint32_t n=0;n<10000;++n){
                    block.frames=n?std::array<uint32_t,6>{0,1,7,32,64,13}[n%6]:64;
                    const auto* p=n==0?(module==1?points.data():&performancePoint):nullptr;
                    const uint32_t used=p?(module==1?points.size():1):0;
                    output[0].fill(77);output[1].fill(77);
                    require(parameters->process(audio,&block,p,used,&r)==RAVET_VST3_OK&&r.native_result==0&&r.failure==0);
                    size_t event=0;for(uint32_t j=0;j<block.frames;++j){
                        while(module==1&&event<used&&points[event].offset==j)oracle.event(points[event++]);
                        const float gain=oracle.next();
                        for(uint32_t ch=0;ch<channels;++ch){const float expected=module==1?input[ch][j]*(oracle.bypass?1:gain):0;require(std::abs(output[ch][j]-expected)<1e-6f);}
                    }
                    for(uint32_t ch=0;ch<channels;++ch)for(uint32_t j=block.frames;j<70;++j)require(output[ch][j]==77);
                }
                measured=false;require(allocations==0&&deallocations==0);pass=true;
            });worker.join();require(pass);require(api->end_audio(audio,&native)==RAVET_VST3_OK&&api->stop(child,&native)==RAVET_VST3_OK);
            // Controller snapshot is explicit and still independent of AUDIO.
            if(module==1)require(parameters->value(child,1,RAVET_VST3_PARAMETER_READ,0,&value,&report)==RAVET_VST3_OK&&value==.75);
            else{
                require(ravet_vst3_state_provider_v1()->capture(child,&saved,componentState.data(),componentState.size(),controllerState.data(),controllerState.size(),&stateReport)==RAVET_VST3_OK&&saved.component_size==24);
                uint32_t bits=0;for(uint32_t i=0;i<4;++i)bits|=uint32_t(componentState[20+i])<<(8*i);float levelValue=0;std::memcpy(&levelValue,&bits,4);
                require(levelValue==float(.3));
            }
            require(api->destroy(child,&native)==RAVET_VST3_OK);
        }
        require(api->close(parent,&native)==RAVET_VST3_OK);
    }
    std::cout<<"PASS actual Gain/Performance parameter companion:4 mono/stereo profiles, owned metadata/control/text, controller/DSP separation, independent timed smoothing/bypass oracle,10000 variable/zero blocks/profile with zero C++ new/delete, whole rejection and tails\n";
}catch(const std::exception& e){std::cerr<<e.what()<<'\n';return 1;}}
