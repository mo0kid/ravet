#include "RavetLv2Transport.hpp"
#include "RavetPerformance.h"
#include <cstdlib>
#include <iostream>
#include <new>
#include <stdexcept>
#include <type_traits>
namespace allocation {bool track=false;size_t created=0,retired=0;}
void *operator new(size_t bytes){if(allocation::track)++allocation::created;if(auto result=std::malloc(bytes?bytes:1))return result;throw std::bad_alloc();}
void *operator new[](size_t bytes){return ::operator new(bytes);}
void operator delete(void *pointer) noexcept{if(pointer&&allocation::track)++allocation::retired;std::free(pointer);}
void operator delete[](void *pointer) noexcept{::operator delete(pointer);}
void operator delete(void *pointer,size_t) noexcept{::operator delete(pointer);}
void operator delete[](void *pointer,size_t) noexcept{::operator delete(pointer);}
namespace {
using ravet::native::Lv2TimePosition;using ravet::native::Lv2TimePositionTypes;
void require(bool okay,const char *message){if(!okay)throw std::runtime_error(message);}
constexpr Lv2TimePositionTypes types{1,2,3,4,5,6,7,8,9,10,11,12,13,14};
struct Object {
    alignas(8) std::array<uint8_t,4096> bytes{};uint32_t size=sizeof(LV2_Atom_Object_Body);
    Object(){LV2_Atom_Object_Body body{0,types.position};std::memcpy(bytes.data(),&body,sizeof(body));}
    template<class T> uint32_t property(uint32_t key,uint32_t type,T value,uint32_t context=0){
        const uint32_t at=size;const LV2_Atom_Property_Body header{key,context,{sizeof(T),type}};
        std::memcpy(bytes.data()+size,&header,sizeof(header));std::memcpy(bytes.data()+size+sizeof(header),&value,sizeof(value));
        size+=sizeof(header)+((sizeof(T)+7)&~uint32_t(7));return at;
    }
};
struct Sequence {
    alignas(8) std::array<uint8_t,65544> bytes{};uint32_t size=sizeof(LV2_Atom_Sequence);
    Sequence(){LV2_Atom_Sequence header{{sizeof(header.body),types.sequence},{0,0}};std::memcpy(bytes.data(),&header,sizeof(header));}
    LV2_Atom_Sequence *get(){return reinterpret_cast<LV2_Atom_Sequence *>(bytes.data());}
    uint32_t append(int64_t offset,uint32_t type,const uint8_t *body,uint32_t count){
        const uint32_t at=size;const LV2_Atom_Event event{{offset},{count,type}};
        std::memcpy(bytes.data()+size,&event,sizeof(event));if(count)std::memcpy(bytes.data()+size+sizeof(event),body,count);
        size+=sizeof(event)+((count+7)&~uint32_t(7));get()->atom.size=size-sizeof(LV2_Atom);return at;
    }
    uint32_t position(int64_t offset,const Object &object){return append(offset,types.object,object.bytes.data(),object.size);}
};
Object complete(int64_t sample=INT64_MAX){Object object;object.property(types.frame,types.longType,sample);object.property(types.beat,types.doubleType,0x1.0000000000001p52);object.property(types.tempo,types.floatType,123.456f);object.property(types.speed,types.floatType,1.f);object.property(types.numerator,types.floatType,65535.f);object.property(types.denominator,types.intType,int32_t(65535));return object;}
template<class T> void foreignNaN(T &value){using Bits=std::conditional_t<sizeof(T)==4,uint32_t,uint64_t>;volatile Bits foreignBits=sizeof(T)==4?Bits(UINT32_C(0x7fc00000)):Bits(UINT64_C(0x7ff8000000000000));Bits bits=foreignBits;std::memcpy(&value,&bits,sizeof(value));}
void nativeCanonical(){
    const auto *entry=ravet_performance_processor_v1(32);require(entry,"actual native performance entry");auto *processor=entry->create();require(processor,"actual native create");
    RavetNativeProcessorConfigV1 config{};config.version=1;config.maximum_frames=64;config.maximum_events=1024;config.bus_count=1;config.sample_rate=48000.;config.buses[0]={0,1};require(!entry->prepare(processor,&config),"actual native zero-frame preparation");
    const RavetNativeProcessorBusV1 bus{0,1,nullptr,nullptr};auto accepted=[&](const RavetNativeTransportV1 &transport){RavetNativeProcessorResultV1 result{};return entry->process(processor,&bus,1,0,nullptr,0,&transport,nullptr,0,&result)==0&&result.status==0&&!result.event_count;};
    Sequence full;full.position(0,complete());RavetNativeTransportV1 snapshot{};require(Lv2TimePosition::collect(full.get(),types,0,snapshot)&&accepted(snapshot),"native codec accepts complete collected LV2 snapshot");
    Object signature;signature.property(types.numerator,types.floatType,7.f);signature.property(types.denominator,types.intType,int32_t(8));Sequence signatureOnly;signatureOnly.position(0,signature);require(Lv2TimePosition::collect(signatureOnly.get(),types,0,snapshot)&&snapshot.present==(RAVET_TRANSPORT_DETAILS|RAVET_TRANSPORT_SIGNATURE)&&accepted(snapshot),"native codec accepts signature-only LV2 DETAILS placeholders");
    auto invalid=snapshot;invalid.present&=~RAVET_TRANSPORT_DETAILS;require(!accepted(invalid),"native codec rejects signature dependency corruption");
    Object music;music.property(types.beat,types.doubleType,-1.5);music.property(types.tempo,types.floatType,120.f);Sequence musicOnly;musicOnly.position(0,music);require(Lv2TimePosition::collect(musicOnly.get(),types,0,snapshot)&&snapshot.present==(RAVET_TRANSPORT_BEATS|RAVET_TRANSPORT_TEMPO)&&accepted(snapshot),"native codec accepts independent LV2 music-only snapshot");
    Object sample;sample.property(types.frame,types.longType,int64_t(INT64_MIN));Sequence sampleOnly;sampleOnly.position(0,sample);require(Lv2TimePosition::collect(sampleOnly.get(),types,0,snapshot)&&accepted(snapshot),"native codec accepts exact independent minimum i64 LV2 sample");
    invalid=snapshot;invalid.flags=RAVET_TRANSPORT_FLAG_RECORDING;require(!accepted(invalid),"native codec rejects unavailable-details flags");
    Sequence empty;require(Lv2TimePosition::collect(empty.get(),types,0,snapshot)&&accepted(snapshot),"native codec accepts wholly unknown LV2 snapshot");
    require(!entry->deactivate(processor),"native validation owner stop");entry->destroy(processor);
}
void cases(){
    Sequence sequence;auto full=complete();sequence.position(0,full);RavetNativeTransportV1 output{};
    require(types.valid()&&Lv2TimePosition::collect(sequence.get(),types,64,output),"complete exact encoded block-start Position");
    require(output.sample_position==INT64_MAX&&output.beat_position==0x1.0000000000001p52&&output.tempo==double(123.456f)&&output.signature_numerator==65535&&output.signature_denominator==65535,"Long/Double exactness, Float widening and bounded signature");
    require(output.present==(RAVET_TRANSPORT_PLAYING|RAVET_TRANSPORT_SAMPLES|RAVET_TRANSPORT_BEATS|RAVET_TRANSPORT_TEMPO|RAVET_TRANSPORT_DETAILS|RAVET_TRANSPORT_SIGNATURE)&&output.flags==RAVET_TRANSPORT_FLAG_PLAYING,"canonical signature DETAILS carries only unknown/default boolean placeholders");
    const auto retained=output;
    auto reject=[&](Sequence &bad,const char *message){require(!Lv2TimePosition::collect(bad.get(),types,64,output)&&!std::memcmp(&output,&retained,sizeof(output)),message);};
    Sequence duplicate;duplicate.position(0,full);duplicate.position(0,full);reject(duplicate,"duplicate offset0 Position rejects atomically");
    for(float speed:{-.5f,-1.f,.5f,2.f}){Object bad;bad.property(types.speed,types.floatType,speed);Sequence test;test.position(0,full);test.position(3,bad);reject(test,"unsupported later speed rejects rather than stopped");}
    for(uint32_t key:{types.beat,types.tempo,types.speed,types.numerator}){Object bad;float value;foreignNaN(value);if(key==types.beat){double beat;foreignNaN(beat);bad.property(key,types.doubleType,beat);}else bad.property(key,types.floatType,value);Sequence test;test.position(0,bad);reject(test,"foreign nonfinite supported scalar rejects atomically");}
    for(float numerator:{0.f,-1.f,3.5f,65536.f}){Object bad;bad.property(types.numerator,types.floatType,numerator);Sequence test;test.position(0,bad);reject(test,"nonintegral/out-of-range signature numerator rejects before cast");}
    for(int32_t denominator:{0,-1,65536,INT32_MAX}){Object bad;bad.property(types.denominator,types.intType,denominator);Sequence test;test.position(0,bad);reject(test,"invalid signature denominator rejects before cast");}
    Object bad=complete();bad.property(types.speed,types.floatType,0.f);Sequence duplicateKey;duplicateKey.position(0,bad);reject(duplicateKey,"duplicate supported property key rejects");
    Object unknown;unknown.property(100,types.intType,int32_t(1));unknown.property(100,types.longType,int64_t(2));Sequence unknownDuplicate;unknownDuplicate.position(0,unknown);reject(unknownDuplicate,"duplicate opaque property key rejects");
    for(uint32_t key:{types.frame,types.beat,types.tempo,types.speed,types.numerator,types.denominator}){Object wrong;wrong.property(key,types.intType,int32_t(1),1);Sequence test;test.position(0,wrong);reject(test,"supported nondefault context rejects");}
    Object wrongFrame;wrongFrame.property(types.frame,types.doubleType,1.);Sequence wrongType;wrongType.position(0,wrongFrame);reject(wrongType,"Double frame never narrows into Long coordinate");
    Object wrongBeat;wrongBeat.property(types.beat,types.floatType,1.f);Sequence wrongBeatSequence;wrongBeatSequence.position(0,wrongBeat);reject(wrongBeatSequence,"Float beat is not silently widened from another encoding");
    Object wrongTempo;wrongTempo.property(types.tempo,types.doubleType,120.);Sequence wrongTempoSequence;wrongTempoSequence.position(0,wrongTempo);reject(wrongTempoSequence,"Double BPM is outside selected exact Float encoding");
    Object wrongSize;wrongSize.property(types.frame,types.longType,uint32_t(1));Sequence wrongSizeSequence;wrongSizeSequence.position(0,wrongSize);reject(wrongSizeSequence,"known scalar body byte size preflight");
    Object overflow;const auto at=overflow.property(types.frame,types.longType,int64_t(1));LV2_Atom_Property_Body oversized{types.frame,0,{UINT32_MAX,types.longType}};std::memcpy(overflow.bytes.data()+at,&oversized,sizeof(oversized));Sequence overflowSequence;overflowSequence.position(0,overflow);reject(overflowSequence,"large declared property extent does not overflow or read out of bounds");
    Object truncated=complete();--truncated.size;Sequence truncatedSequence;truncatedSequence.position(0,truncated);reject(truncatedSequence,"incomplete final property padding rejects");
    Sequence truncatedEvent;truncatedEvent.position(0,full);--truncatedEvent.get()->atom.size;reject(truncatedEvent,"incomplete event padding rejects");
    Sequence tooLarge;tooLarge.get()->atom.size=UINT32_MAX;reject(tooLarge,"declared sequence bound checked before walk");
    Sequence unit;unit.position(0,full);unit.get()->body.unit=100;reject(unit,"unsupported beat-time sequence unit rejects");unit.get()->body.unit=0;unit.get()->body.pad=1;reject(unit,"reserved sequence pad rejects");
    Sequence unordered;unordered.position(3,full);unordered.position(2,full);reject(unordered,"raw source timestamps must be ordered");
    Sequence negative;negative.position(-1,full);reject(negative,"negative event timestamp rejects");
    Sequence outside;outside.position(64,full);reject(outside,"event offset beyond active slice rejects");
    require(!Lv2TimePosition::collect(reinterpret_cast<LV2_Atom_Sequence *>(sequence.bytes.data()+1),types,64,output)&&!std::memcmp(&output,&retained,sizeof(output)),"unaligned sequence rejects before read/publication");
    auto invalidTypes=types;invalidTypes.floatType=types.longType;require(!Lv2TimePosition::collect(sequence.get(),invalidTypes,64,output),"aliased cached URIDs reject");invalidTypes=types;invalidTypes.object=0;require(!invalidTypes.valid(),"missing cached URID rejects");
    Object many;for(uint32_t key=100;key<229;++key)many.property(key,types.intType,int32_t(1));Sequence tooMany;tooMany.position(0,many);reject(tooMany,"129 unique properties exceed bound");
    Sequence tooManyEvents;for(uint32_t i=0;i<1025;++i)tooManyEvents.append(0,100,nullptr,0);reject(tooManyEvents,"1025 raw events exceed bound");
    Object partial;partial.property(types.frame,types.longType,int64_t(INT64_MIN));partial.property(types.numerator,types.floatType,7.f);Sequence partialSequence;partialSequence.position(0,partial);require(Lv2TimePosition::collect(partialSequence.get(),types,64,output)&&output.present==RAVET_TRANSPORT_SAMPLES&&output.sample_position==INT64_MIN&&!output.signature_numerator&&!output.signature_denominator,"exact i64 minimum and valid incomplete signature stays unknown");
    Object stopped;stopped.property(types.speed,types.floatType,-0.f);stopped.property(types.beat,types.doubleType,-0.);Sequence stoppedSequence;stoppedSequence.position(0,stopped);require(Lv2TimePosition::collect(stoppedSequence.get(),types,64,output)&&output.present==(RAVET_TRANSPORT_PLAYING|RAVET_TRANSPORT_BEATS)&&!output.flags,"explicit zero speed means known stopped");uint64_t beatBits;std::memcpy(&beatBits,&output.beat_position,sizeof(beatBits));require(beatBits==UINT64_C(0x8000000000000000),"used Double signed zero survives");
    Object later=complete(99);Sequence laterSequence;laterSequence.position(0,full);laterSequence.position(3,later);require(Lv2TimePosition::collect(laterSequence.get(),types,64,output)&&output.sample_position==INT64_MAX,"later validated Position does not replace/extrapolate block-start context");
    Sequence laterOnly;laterOnly.position(3,full);require(Lv2TimePosition::collect(laterOnly.get(),types,64,output)&&!output.present&&!output.flags,"later-only context remains unknown rather than stopped");
    Sequence empty;require(Lv2TimePosition::collect(empty.get(),types,0,output)&&!output.present,"empty zero-frame context unknown");require(Lv2TimePosition::collect(sequence.get(),types,0,output)&&output.sample_position==INT64_MAX,"offset0 exact context on zero-frame flush");require(!Lv2TimePosition::collect(laterOnly.get(),types,0,output),"zero-frame later offset rejects");
    Object maximum;for(uint32_t key=100;key<228;++key)maximum.property(key,types.intType,int32_t(1),100);Sequence maximumSequence;maximumSequence.position(0,maximum);require(Lv2TimePosition::collect(maximumSequence.get(),types,64,output)&&!output.present,"128 unique opaque bounded property values accepted without invented meaning");
    Sequence maximumEvents;for(uint32_t i=0;i<1024;++i)maximumEvents.append(0,100,nullptr,0);require(Lv2TimePosition::collect(maximumEvents.get(),types,64,output),"1024 raw bounded opaque events accepted");
    bool accepted=true;allocation::created=allocation::retired=0;allocation::track=true;
    for(uint32_t cycle=0;cycle<2000;++cycle){accepted=Lv2TimePosition::collect(laterSequence.get(),types,64,output)&&accepted;accepted=!Lv2TimePosition::collect(duplicateKey.get(),types,64,output)&&accepted;}
    allocation::track=false;require(accepted&&!allocation::created&&!allocation::retired,"2000 accepted and late-rejected collections have no ordinary C++ heap actions");
}
}
int main(){try{nativeCanonical();cases();std::cout<<"PASS LV2 exact block-start time Position collection contracts\n";return 0;}catch(const std::exception &error){std::cerr<<error.what()<<'\n';return 1;}}
