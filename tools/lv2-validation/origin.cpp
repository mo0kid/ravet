#include "RavetLv2Processor.hpp"
#include <array>
#include <iostream>
#include <stdexcept>
namespace {
void require(bool value,const char *message){if(!value)throw std::runtime_error(message);}
struct Counts{uint32_t core=0,cache=0,state=0,coreDrop=0,cacheDrop=0,stateDrop=0;};
Counts counts{};
const RavetNativeProcessorEntryV1 *realCore;
const RavetNativeCachedParameterEntryV1 *realCache;
const RavetNativeStateEntryV1 *realState;
RavetNativeProcessorEntryV1 core{};
RavetNativeParameterEntryV1 parameters{},copy{};
RavetNativeCachedParameterEntryV1 cache{};
RavetNativeStateEntryV1 state{};
const RavetNativeProcessorEntryV1 *coreEntry(){return &core;}
const RavetNativeParameterEntryV1 *parameterEntry(){return &parameters;}
const RavetNativeParameterEntryV1 *copyEntry(){return &copy;}
const RavetNativeParameterEntryV1 *nullEntry(){return nullptr;}
const RavetNativeParameterEntryV1 *throwEntry(){throw std::runtime_error("violating metadata query");}
const RavetNativeProcessorEntryV1 *throwCore(){throw std::runtime_error("violating core query");}
void *createCore(){++counts.core;return realCore->create();}
void dropCore(void *owner){++counts.coreDrop;realCore->destroy(owner);}
void *createCache(){++counts.cache;return realCache->create();}
void dropCache(void *owner){++counts.cacheDrop;realCache->destroy(owner);}
void *createState(const uint8_t *bytes,uint32_t count){++counts.state;return realState->create(bytes,count);}
void dropState(void *owner){++counts.stateDrop;realState->destroy(owner);}
using Instance=ravet::native::NativeLv2Processor<2>;
void invalid(const RavetNativeProcessorEntryV1 *p,const RavetNativeCachedParameterEntryV1 *c,const RavetNativeStateEntryV1 *s,const char *message){
 counts={};float sentinel=99.f;const LV2_Feature *features[]{nullptr};
 {Instance instance(p,c,s,1,48000.,"urn:ravet:gain:state",features);require(!instance.valid(),message);instance.connect(0,&sentinel);instance.connect(1,&sentinel);instance.activate();instance.run(1);instance.deactivate();}
 require(sentinel==99.f,"invalid helper callback storage remains unchanged");
 require(!counts.core&&!counts.cache&&!counts.state&&!counts.coreDrop&&!counts.cacheDrop&&!counts.stateDrop,"origin preflight invokes no creators or retirements");
}
void valid(const RavetNativeProcessorEntryV1 *p,const RavetNativeCachedParameterEntryV1 *c,const RavetNativeStateEntryV1 *s){
 counts={};const LV2_Feature *features[]{nullptr};std::array<float,65> input{},output{};input.fill(.25f);output.fill(99.f);float gain=1.f,bypass=1.f;
 {Instance instance(p,c,s,1,48000.,"urn:ravet:gain:state",features);require(instance.valid(),"exact coherent core/schema origins acquire");instance.connect(0,input.data());instance.connect(1,output.data());instance.connect(2,&gain);instance.connect(3,&bypass);instance.activate();instance.run(64);require(output[0]==.25f&&output[63]==.25f&&output[64]==99.f,"actual native Gain processing and backing tail");instance.deactivate();}
 require(counts.core==1&&counts.cache==1&&counts.state==1&&counts.coreDrop==1&&counts.cacheDrop==1&&counts.stateDrop==1,"valid coherent resources retire exactly once");
}
void cases(){
 realCore=ravet_native_processor_v1(32);realCache=ravet_native_cached_parameters_v1(32);realState=ravet_native_state_v1(32);
 require(realCore&&realCache&&realState&&realCache->parameters_entry()->processor_entry()==realCore&&realState->parameters_entry()==realCache->parameters_entry(),"actual canonical exported native origins");
 core=*realCore;core.create=createCore;core.destroy=dropCore;
 parameters=*realCache->parameters_entry();parameters.processor_entry=coreEntry;copy=parameters;
 cache=*realCache;cache.parameters_entry=parameterEntry;cache.create=createCache;cache.destroy=dropCache;
 state=*realState;state.parameters_entry=parameterEntry;state.create=createState;state.destroy=dropState;
 valid(&core,&cache,&state);
 auto copiedCore=core;invalid(&copiedCore,&cache,&state,"byte-identical copied core refuses");
 auto copiedCache=cache;copiedCache.parameters_entry=copyEntry;invalid(&core,&copiedCache,&state,"copied cache schema refuses despite identical metadata/callbacks");
 auto copiedState=state;copiedState.parameters_entry=copyEntry;invalid(&core,&cache,&copiedState,"copied state schema refuses despite identical metadata/callbacks");
 auto badCache=cache;badCache.parameters_entry=nullEntry;invalid(&core,&badCache,&state,"null cache schema refuses");
 auto badState=state;badState.parameters_entry=nullEntry;invalid(&core,&cache,&badState,"null state schema refuses");
 badCache.parameters_entry=throwEntry;invalid(&core,&badCache,&state,"throwing cache metadata refuses before creators");
 badState.parameters_entry=throwEntry;invalid(&core,&cache,&badState,"throwing state metadata refuses before creators");
 parameters.processor_entry=throwCore;invalid(&core,&cache,&state,"throwing core metadata refuses before creators");parameters.processor_entry=coreEntry;
 // Existing ABI exposes core/schema provenance, not cache/state top-level
 // address oracles. A copied top-level view retaining real origins is valid.
 auto sameCache=cache;auto sameState=state;valid(&core,&sameCache,&sameState);
 {const LV2_Feature *features[]{nullptr};Instance actual(realCore,realCache,realState,1,48000.,"urn:ravet:gain:state",features);require(actual.valid(),"genuine exported core/schema cache/state remain supported");}
}
}
int main(){try{cases();std::cout<<"PASS LV2 exact core/shared schema origins, creator0 copied/null/throw refusal, coherent balanced/native Gain consumers\n";return 0;}catch(const std::exception &error){std::cerr<<error.what()<<'\n';return 1;}}
