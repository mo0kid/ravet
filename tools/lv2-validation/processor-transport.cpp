#include "RavetLv2Processor.hpp"
#include "RavetPerformance.h"
#include <iostream>
#include <stdexcept>
namespace {
void require(bool value,const char *message){if(!value)throw std::runtime_error(message);}
const RavetNativeProcessorEntryV1 *originCore;
const RavetNativeStateEntryV1 *originState;
RavetNativeProcessorEntryV1 forwarded;
RavetNativeParameterEntryV1 parameters;
uint32_t processCalls=0,applyCalls=0,mapCalls=0,lastChanged=0;
bool lastNull=true;double lastParameter=-1.;RavetNativeTransportV1 lastTransport{};
const RavetNativeProcessorEntryV1 *coreEntry(){return &forwarded;}
const RavetNativeParameterEntryV1 *parametersEntry(){return &parameters;}
int32_t process(void *owner,const RavetNativeProcessorBusV1 *buses,uint32_t busCount,uint32_t frames,
    const RavetNativeEventV1 *events,uint32_t count,const RavetNativeTransportV1 *transport,
    RavetNativeEventV1 *output,uint32_t capacity,RavetNativeProcessorResultV1 *result){
    ++processCalls;lastNull=!transport;if(transport)lastTransport=*transport;lastParameter=-1.;
    for(uint32_t i=0;i<count;++i)if(events[i].kind==RAVET_NATIVE_PARAMETER_VALUE)lastParameter=events[i].value;
    return originCore->process(owner,buses,busCount,frames,events,count,transport,output,capacity,result);
}
int32_t apply(void *owner,const void *publication,RavetNativeStateApplyV1 *output){
    ++applyCalls;const auto result=originState->apply(owner,publication,output);if(!result)lastChanged=output->changed;return result;
}
uint32_t map(void *,const char *uri){
    ++mapCalls;const std::array<const char *,17> uris{LV2_ATOM__Sequence,LV2_ATOM__frameTime,LV2_ATOM__Object,LV2_TIME__Position,
        LV2_ATOM__Long,LV2_ATOM__Double,LV2_ATOM__Float,LV2_ATOM__Int,LV2_TIME__frame,LV2_TIME__beat,LV2_TIME__beatsPerMinute,
        LV2_TIME__speed,LV2_TIME__beatsPerBar,LV2_TIME__beatUnit,LV2_MIDI__MidiEvent,LV2_ATOM__Chunk,"urn:ravet:performance-sine:state"};
    for(uint32_t i=0;i<uris.size();++i)if(!std::strcmp(uri,uris[i]))return i+1;return 0;
}
uint32_t midiOnlyMap(void *user,const char *uri){const auto id=map(user,uri);return id<=2||id>=15?id:0;}
struct Position {
    std::array<uint8_t,256> bytes{};uint32_t size=sizeof(LV2_Atom_Object_Body);
    Position(float speed=1.f){const LV2_Atom_Object_Body body{0,4};std::memcpy(bytes.data(),&body,sizeof(body));property(9,5,int64_t(INT64_MAX));property(10,6,0x1.0000000000001p52);property(11,7,123.456f);property(12,7,speed);property(13,7,7.f);property(14,8,int32_t(8));}
    template<class T>void property(uint32_t key,uint32_t type,T value){const LV2_Atom_Property_Body property{key,0,{sizeof(T),type}};std::memcpy(bytes.data()+size,&property,sizeof(property));std::memcpy(bytes.data()+size+sizeof(property),&value,sizeof(value));size+=sizeof(property)+((sizeof(T)+7)&~uint32_t(7));}
};
struct Sequence {
    alignas(8)std::array<uint8_t,512> bytes{};uint32_t size=sizeof(LV2_Atom_Sequence);
    Sequence(){LV2_Atom_Sequence header{{sizeof(header.body),1},{0,0}};std::memcpy(bytes.data(),&header,sizeof(header));}
    LV2_Atom_Sequence *get(){return reinterpret_cast<LV2_Atom_Sequence *>(bytes.data());}
    void append(int64_t offset,uint32_t type,const uint8_t *body,uint32_t count){const LV2_Atom_Event event{{offset},{count,type}};std::memcpy(bytes.data()+size,&event,sizeof(event));if(count)std::memcpy(bytes.data()+size+sizeof(event),body,count);size+=sizeof(event)+((count+7)&~uint32_t(7));get()->atom.size=size-sizeof(LV2_Atom);}
    void position(const Position &position){append(0,3,position.bytes.data(),position.size);}
};
struct Saved{std::array<uint8_t,24> bytes{};};
LV2_State_Status store(void *handle,uint32_t key,const void *bytes,size_t count,uint32_t type,uint32_t flags){if(key!=17||count!=24||type!=16||flags!=3)return LV2_STATE_ERR_UNKNOWN;std::memcpy(static_cast<Saved *>(handle)->bytes.data(),bytes,count);return LV2_STATE_SUCCESS;}
const void *retrieve(void *handle,uint32_t key,size_t *size,uint32_t *type,uint32_t *flags){if(key!=17)return nullptr;*size=24;*type=16;*flags=3;return static_cast<Saved *>(handle)->bytes.data();}
float level(const Saved &saved){uint32_t bits=0;for(uint32_t i=0;i<4;++i)bits|=uint32_t(saved.bytes[20+i])<<(8*i);float value;std::memcpy(&value,&bits,4);return value;}
void level(Saved &saved,float value){uint32_t bits;std::memcpy(&bits,&value,4);for(uint32_t i=0;i<4;++i)saved.bytes[20+i]=uint8_t(bits>>(8*i));}
void cases(){
    originCore=ravet_performance_processor_v1(32);originState=ravet_performance_state_v1(32);forwarded=*originCore;forwarded.process=process;
    parameters=*ravet_performance_cached_parameters_v1(32)->parameters_entry();parameters.processor_entry=coreEntry;
    auto cache=*ravet_performance_cached_parameters_v1(32);cache.parameters_entry=parametersEntry;
    auto state=*originState;state.parameters_entry=parametersEntry;state.apply=apply;
    LV2_URID_Map mapper{nullptr,map};LV2_Feature feature{LV2_URID__map,&mapper};const LV2_Feature *features[]{&feature,nullptr};
    std::array<float,65> samples{};float gain=.2f;Sequence input;input.position(Position{});
    {
        ravet::native::NativeLv2Processor<1,2,true,true> instance(&forwarded,&cache,&state,1,48000.,"urn:ravet:performance-sine:state",features);
        require(instance.valid(),"controlled forwarded native transport profile");instance.connect(0,samples.data());instance.connect(1,&gain);instance.connect(2,input.get());instance.activate();
        const auto maps=mapCalls;instance.run(0);
        require(processCalls==1&&applyCalls==1&&!lastNull&&lastTransport.sample_position==INT64_MAX&&lastTransport.beat_position==0x1.0000000000001p52&&lastTransport.tempo==double(123.456f)&&lastTransport.signature_numerator==7&&lastTransport.signature_denominator==8,"exact collected native context reaches the real processor through generic helper");
        Saved saved;require(!instance.save(store,&saved),"save complete publication");level(saved,.6f);require(!instance.restore(retrieve,&saved),"pending complete publication before bad time");
        input=Sequence{};input.position(Position{});const Position late(.5f);input.append(10,3,late.bytes.data(),late.size);gain=.75f;samples.fill(99.f);const auto calls=processCalls,applies=applyCalls;instance.run(64);
        require(processCalls==calls&&applyCalls==applies&&samples[0]==0.f&&samples[63]==0.f&&samples[64]==99.f,"full time preflight before any state apply/DSP, preserving supported tails");
        input=Sequence{};input.position(Position{});instance.run(0);
        require(processCalls==calls+1&&applyCalls==applies+1&&lastChanged==1&&lastParameter==.75,"valid recovery applies retained complete recall and still delivers control rejected earlier");
        require(!instance.save(store,&saved)&&level(saved)==.75f,"state/control recovery remains coherent");
        input=Sequence{};instance.run(0);require(lastNull&&mapCalls==maps,"missing Position reaches DSP as null unknown with no runtime map calls");instance.deactivate();
    }
    {
        LV2_URID_Map midiMapper{nullptr,midiOnlyMap};LV2_Feature midiFeature{LV2_URID__map,&midiMapper};const LV2_Feature *midiFeatures[]{&midiFeature,nullptr};
        ravet::native::NativeLv2Processor<1,2,true> instance(&forwarded,&cache,&state,1,48000.,"urn:ravet:performance-sine:state",midiFeatures);
        require(instance.valid(),"unchanged MIDI-only template needs no time URIDs");gain=.2f;input=Sequence{};input.position(Position(.5f));const uint8_t note[]{0x90,69,127};input.append(3,15,note,3);instance.connect(0,samples.data());instance.connect(1,&gain);instance.connect(2,input.get());instance.activate();const auto calls=processCalls;instance.run(64);
        require(processCalls==calls+1&&lastNull&&std::abs(samples[63])>0.f,"default MIDI-only profile still ignores opaque time bodies and delivers native notes");instance.deactivate();
    }
}
}
int main(){try{cases();std::cout<<"PASS LV2 generic native transport delivery and state preflight/default-profile contracts\n";return 0;}catch(const std::exception &error){std::cerr<<error.what()<<'\n';return 1;}}
