#include "RavetAuParameters.hpp"
#include "RavetAuState.hpp"
#include "RavetPerformance.h"
#include <iostream>
#include <stdexcept>
namespace {
void require(bool value,const char *text){if(!value)throw std::runtime_error(text);}
void contracts(){const auto cache=ravet_performance_cached_parameters_v1(64);const auto state=ravet_performance_state_v1(64);AudioUnitParameterInfo retained{};
    ravet::au::NativeAuParameters<1> defaultRejected(cache),wrong32(cache,32),unknown(cache,128);require(!defaultRejected.valid()&&!wrong32.valid()&&!unknown.valid(),"default32/explicit mismatch/unknown native precision rejects");
    {ravet::au::NativeAuParameters<1> schema(cache,64);require(schema.valid()&&schema.count()==1&&schema.parametersEntry()->sample_bits==64,"explicit exact64 native schema adoption");
        require(schema.info(1,retained)&&retained.minValue==0.f&&retained.maxValue==1.f&&retained.defaultValue==.2f&&CFEqual(retained.cfNameString,CFSTR("Level")),"native64 provenance still normalized AU f32 metadata");
        float value=77.f;
        for(uint32_t pattern:{UINT32_C(0x7fc00001),UINT32_C(0x7f800000),UINT32_C(0xff800000)}){volatile uint32_t source=pattern;const uint32_t bits=source;float foreign;std::memcpy(&foreign,&bits,4);require(!schema.control(1,foreign,0,value)&&value==77.f,"actual native64 provider rejects foreign nonfinite controls without output mutation");}
        require(schema.control(1,.375f,65535,value)&&value==.375f,"native64 cached control keeps f32 values");CFStringRef text=nullptr;require(schema.format(1,value,text)&&CFEqual(text,CFSTR("0.375")),"native64 physical f32 text");CFRelease(text);require(schema.parse(1,CFSTR(" +0.375 \n"),value)&&value==.375f,"native64 complete f32 parser");
        ravet::au::NativeAuState validator(state,schema.parametersEntry(),64);require(validator.valid(),"explicit paired64 complete codec");ravet::au::NativeAuState defaultState(state,schema.parametersEntry()),mixed(state,ravet_performance_parameters_v1(32),64),unknownState(state,schema.parametersEntry(),128);require(!defaultState.valid()&&!mixed.valid()&&!unknownState.valid(),"default32/state parameter mismatch/unknown precision rejects");
        auto copied=*schema.parametersEntry();ravet::au::NativeAuState copiedOrigin(state,&copied,64);require(!copiedOrigin.valid(),"explicit64 state origin remains exact");
        auto publication=state->create(nullptr,0);std::array<uint8_t,24> bytes{};RavetNativeStateRevisionV1 revision{};require(publication&&!state->save_state(publication,bytes.data(),bytes.size(),&revision),"actual64 complete state source");state->destroy(publication);auto data=CFDataCreate(nullptr,bytes.data(),bytes.size());require(validator.validateData(data),"actual64 complete state canonical acceptance");CFRelease(data);bytes[16]=2;data=CFDataCreate(nullptr,bytes.data(),bytes.size());require(!validator.validateData(data),"explicit64 complete malformed stable ID rejected");CFRelease(data);
    }
    require(CFEqual(retained.cfNameString,CFSTR("Level")),"retained64 CF name survives helper retirement");CFRelease(retained.cfNameString);if(retained.unitName)CFRelease(retained.unitName);
    const auto cache32=ravet_performance_cached_parameters_v1(32);ravet::au::NativeAuParameters<1> unchanged(cache32);require(unchanged.valid(),"existing default32 constructor remains valid");ravet::au::NativeAuState state32(ravet_performance_state_v1(32),unchanged.parametersEntry());require(state32.valid(),"existing default32 codec remains valid");
}
}
int main(){try{contracts();std::cout<<"PASS AU explicit native32/64 provenance with f32 schema/control/complete codec contracts\n";return 0;}catch(const std::exception &error){std::cerr<<error.what()<<'\n';return 1;}}
