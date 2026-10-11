#include "RavetLv2Processor.hpp"
#include "RavetNoteRouting.h"
#include <memory>
#include <thread>
#include "RavetLv2NoteInput.hpp"
#include "lv2/time/time.h"
#include "lv2/core/lv2.h"
#include "lv2/midi/midi.h"
#include "lv2/state/state.h"
#include "lv2/urid/urid.h"
#include <array>
#include <atomic>
#include <cmath>
#include <cstdlib>
#include <cstring>
#include <dlfcn.h>
#include <iostream>
#include <limits>
#include <new>
#include <stdexcept>
static std::atomic<uint64_t> allocations{0}, deletions{0};
void *operator new(size_t size) { ++allocations; if (auto *p = std::malloc(size ? size : 1)) return p; throw std::bad_alloc(); }
void *operator new[](size_t size) { return ::operator new(size); }
void operator delete(void *p) noexcept { ++deletions; std::free(p); }
void operator delete[](void *p) noexcept { ::operator delete(p); }
void operator delete(void *p, size_t) noexcept { ::operator delete(p); }
void operator delete[](void *p, size_t) noexcept { ::operator delete(p); }
void require(bool okay, const char *message) { if (!okay) throw std::runtime_error(message); }
constexpr uint32_t sequenceType = 3, midiType = 4, frameTimeType = 5;
struct Position {
  std::array<uint8_t,512> bytes{};uint32_t size=sizeof(LV2_Atom_Object_Body);
  Position(float speed=1.f,int64_t frame=INT64_MAX,float numerator=7.f){
    const LV2_Atom_Object_Body body{0,7};std::memcpy(bytes.data(),&body,sizeof(body));
    property(12,8,frame);property(13,9,-1.5);property(14,10,120.f);property(15,10,speed);property(16,10,numerator);property(17,11,int32_t(8));
  }
  template<class T>void property(uint32_t key,uint32_t type,T value){const LV2_Atom_Property_Body header{key,0,{sizeof(T),type}};std::memcpy(bytes.data()+size,&header,sizeof(header));std::memcpy(bytes.data()+size+sizeof(header),&value,sizeof(value));size+=sizeof(header)+((sizeof(T)+7)&~uint32_t(7));}
};
struct alignas(8) Sequence {
  std::array<uint8_t, 65544> bytes{};
  Sequence() { clear(); }
  void clear() { LV2_Atom_Sequence s{}; s.atom = {sizeof(s.body), sequenceType}; std::memcpy(bytes.data(), &s, sizeof(s)); }
  LV2_Atom_Sequence *get() { return reinterpret_cast<LV2_Atom_Sequence *>(bytes.data()); }
  void position(int64_t time,const Position &position){LV2_Atom_Event event{};event.time.frames=time;event.body={position.size,6};const auto cursor=sizeof(LV2_Atom)+get()->atom.size;const auto padded=(position.size+7)&~uint32_t(7);require(cursor+sizeof(event)+padded<=bytes.size(),"fixture Position capacity");std::memcpy(bytes.data()+cursor,&event,sizeof(event));std::memcpy(bytes.data()+cursor+sizeof(event),position.bytes.data(),position.size);get()->atom.size+=sizeof(event)+padded;}
  void add(int64_t time, std::initializer_list<uint8_t> data, uint32_t type = midiType) {
    LV2_Atom_Event event{}; event.time.frames = time; event.body = {uint32_t(data.size()), type};
    const auto cursor = sizeof(LV2_Atom) + get()->atom.size;
    const auto padded = (data.size() + 7) & ~size_t(7);
    require(cursor + sizeof(event) + padded <= bytes.size(), "fixture sequence capacity");
    std::memcpy(bytes.data() + cursor, &event, sizeof(event));
    size_t i = cursor + sizeof(event); for (auto byte : data) bytes[i++] = byte;
    get()->atom.size += uint32_t(sizeof(event) + padded);
  }
};
uint32_t map(void *handle, const char *uri) {
  ++*static_cast<uint32_t *>(handle);
  if (!std::strcmp(uri, "urn:ravet:note-routing:state")) return 1;
  if (!std::strcmp(uri, LV2_ATOM__Chunk)) return 2;
  if (!std::strcmp(uri, LV2_ATOM__Sequence)) return sequenceType;
  if (!std::strcmp(uri, LV2_MIDI__MidiEvent)) return midiType;
  if (!std::strcmp(uri, LV2_ATOM__frameTime)) return frameTimeType;
  const std::array<const char *,12> uris{LV2_ATOM__Object,LV2_TIME__Position,LV2_ATOM__Long,LV2_ATOM__Double,LV2_ATOM__Float,LV2_ATOM__Int,LV2_TIME__frame,LV2_TIME__beat,LV2_TIME__beatsPerMinute,LV2_TIME__speed,LV2_TIME__beatsPerBar,LV2_TIME__beatUnit};
  for(uint32_t i=0;i<uris.size();++i)if(!std::strcmp(uri,uris[i]))return i+6;
  return 0;
}
uint32_t mapWithoutPosition(void *handle,const char *uri){return !std::strcmp(uri,LV2_TIME__Position)?0:map(handle,uri);}
struct Property { std::array<uint8_t, 24> bytes{}; size_t size = 0; uint32_t type = 2, flags = 3; bool present = false; };
LV2_State_Status store(void *handle, uint32_t key, const void *bytes, size_t size, uint32_t type, uint32_t flags) {
  auto &p = *static_cast<Property *>(handle);
  if (key != 1 || size != p.bytes.size()) return LV2_STATE_ERR_UNKNOWN;
  std::memcpy(p.bytes.data(), bytes, size); p.size = size; p.type = type; p.flags = flags; p.present = true; return LV2_STATE_SUCCESS;
}
uint32_t retrieves=0;
const void *retrieve(void *handle, uint32_t key, size_t *size, uint32_t *type, uint32_t *flags) {
  ++retrieves;auto &p = *static_cast<Property *>(handle); if (!p.present || key != 1) return nullptr;
  *size = p.size; *type = p.type; *flags = p.flags; return p.bytes.data();
}
uint32_t word(const Property &p, uint32_t offset) { return uint32_t(p.bytes[offset]) | uint32_t(p.bytes[offset+1])<<8 | uint32_t(p.bytes[offset+2])<<16 | uint32_t(p.bytes[offset+3])<<24; }
float level(const Property &p) { const auto bits = word(p, 20); float value; std::memcpy(&value, &bits, 4); return value; }
void level(Property &p, float value) { uint32_t bits; std::memcpy(&bits, &value, 4); for (uint32_t i=0;i<4;++i) p.bytes[20+i] = uint8_t(bits >> (8*i)); }

struct alignas(8) Output {
  std::array<uint8_t,24592+32> bytes{};
  void clear(uint32_t capacity=24584,uint32_t type=2){bytes.fill(0xa5);const LV2_Atom atom{capacity,type};std::memcpy(bytes.data(),&atom,8);}
  LV2_Atom_Sequence* get(){return reinterpret_cast<LV2_Atom_Sequence*>(bytes.data());}
  void check(uint32_t count){require(get()->atom.type==3&&get()->body.unit==5&&get()->body.pad==0&&get()->atom.size==8+24*count,"complete sequence shape");for(size_t i=8+get()->atom.size;i<bytes.size();++i)require(bytes[i]==0xa5,"output backing tails");}
  void event(uint32_t index,int64_t offset,std::initializer_list<uint8_t> expected){const auto cursor=16+24*index;LV2_Atom_Event e{};std::memcpy(&e,bytes.data()+cursor,16);require(e.time.frames==offset&&e.body.type==4&&e.body.size==expected.size(),"timed event header");size_t i=0;for(auto value:expected)require(bytes[cursor+16+i++]==value,"independent event payload");for(;i<8;++i)require(!bytes[cursor+16+i],"zero event padding");}
};

using Instance=ravet::native::NativeLv2Processor<1,2,true,true,1024,1024>;
const RavetNativeProcessorEntryV1* realCore=nullptr;const RavetNativeStateEntryV1* realState=nullptr;
RavetNativeProcessorEntryV1 core{};RavetNativeParameterEntryV1 schema{};
const RavetNativeProcessorEntryV1* coreEntry(){return &core;}
const RavetNativeParameterEntryV1* schemaEntry(){return &schema;}
Instance* activeInstance=nullptr;void* nativeOwner=nullptr;uint32_t processes=0,applies=0,mode=0;uint32_t observedCapacity=0;RavetNativeTransportV1 observedTransport{};bool reentry=false;Output* activeOutput=nullptr;std::array<float,65>* activeAudio=nullptr;Property* activeRecall=nullptr;
int32_t apply(void* owner,const void* publication,RavetNativeStateApplyV1* report){++applies;if(mode==6)return -811;return realState->apply(owner,publication,report);}
int32_t process(void* owner,const RavetNativeProcessorBusV1* buses,uint32_t busesCount,uint32_t frames,const RavetNativeEventV1* events,uint32_t count,const RavetNativeTransportV1* transport,RavetNativeEventV1* output,uint32_t capacity,RavetNativeProcessorResultV1* report){
 ++processes;nativeOwner=owner;observedCapacity=capacity;observedTransport=transport?*transport:RavetNativeTransportV1{};
 if(reentry){reentry=false;const auto bytes=activeOutput->bytes;const auto samples=*activeAudio;const auto pc=processes,ac=applies,reads=retrieves;Instance::OutputDiagnostic diagnostic;require(!activeInstance->outputDiagnosticsStopped(diagnostic),"live diagnostics rejected");require(activeInstance->restore(retrieve,activeRecall)==LV2_STATE_ERR_UNKNOWN,"live restore rejected before getter");activeInstance->connect(0,nullptr);activeInstance->deactivate();activeInstance->activate();activeInstance->run(frames);std::thread concurrent([&]{activeInstance->run(frames);});concurrent.join();require(processes==pc&&applies==ac&&retrieves==reads&&bytes==activeOutput->bytes&&samples==*activeAudio,"nested run/lifecycle/connect cannot touch active output or DSP");}
 if(mode==5)return 2;if(mode==1)return -717;if(mode==2)throw std::runtime_error("violating callback throw");
 const auto status=realCore->process(owner,buses,busesCount,frames,events,count,transport,output,capacity,report);
 if(!status&&mode==3&&report->event_count){output[report->event_count-1].kind=999;}
 if(!status&&mode==4)report->event_count=1025;
 return status;
}
int32_t reflect(void* owner,const void* publication,RavetNativeStateRevisionV1* revision){if(mode==7)return -812;return realState->reflect(owner,publication,revision);}
uint64_t history(void* cache){RavetNativeTelemetryValueV1 value{};require(!ravet_note_routing_telemetry_v1(32)->snapshot(nativeOwner,cache,&value,1)&&value.id==1,"authoritative native history snapshot");return value.samples;}
int main(){try{
 realCore=ravet_note_routing_processor_v1(32);realState=ravet_note_routing_state_v1(32);core=*realCore;core.process=process;schema=*ravet_note_routing_parameters_v1(32);schema.processor_entry=coreEntry;auto cache=*ravet_note_routing_cached_parameters_v1(32);cache.parameters_entry=schemaEntry;auto state=*realState;state.parameters_entry=schemaEntry;state.apply=apply;state.reflect=reflect;
 const auto* telemetry=ravet_note_routing_telemetry_v1(32);auto* meter=telemetry->create_cache();require(meter,"real stopped cache");
 uint32_t calls=0;LV2_URID_Map mapper{&calls,map};LV2_Feature feature{LV2_URID__map,&mapper};const LV2_Feature* features[]{&feature,nullptr};
 for(uint32_t failure=0;failure<8;++failure){mode=0;auto instance=std::make_unique<Instance>(&core,&cache,&state,1,48000.,"urn:ravet:note-routing:state",features);require(instance->valid(),"coherent forwarding fixture");activeInstance=instance.get();Sequence input;Output output;std::array<float,65> audio{};audio.fill(99);float gain=1;instance->connect(0,audio.data());instance->connect(1,&gain);instance->connect(2,input.get());instance->connect(3,output.get());instance->activate();output.clear();input.position(0,Position{});input.add(3,{0x90,60,127});instance->run(32);output.check(1);require(history(meter)==32&&observedCapacity==1024,"full prepared output budget and real native frame history");require(observedTransport.present&&observedTransport.sample_position==INT64_MAX&&observedTransport.tempo==120.&&observedTransport.signature_numerator==7&&observedTransport.signature_denominator==8,"exact transport reaches real native alongside output budget");
 const auto beforeBadProcess=processes,beforeBadApply=applies;volatile uint32_t nanSeed=0x7fc00001;uint32_t nanWord=nanSeed;std::memcpy(&gain,&nanWord,4);output.clear();instance->run(32);output.check(0);uint32_t gainWord=0;std::memcpy(&gainWord,&gain,4);require(gainWord==nanWord&&history(meter)==32&&processes==beforeBadProcess&&applies==beforeBadApply,"integer-seeded malformed control rejects before native history/state");gain=1;
 Property recall;require(!instance->save(store,&recall),"save authoritative Level");level(recall,.5f);require(!instance->restore(retrieve,&recall),"queue distinct recall");gain=.75f;input.add(31,{0x90,61,128});output.clear();const auto pc=processes,ac=applies;instance->run(32);output.check(0);require(history(meter)==32&&processes==pc&&applies==ac,"late malformed block preserves every native history and pending dequeue");
 gain=1;input.clear();input.add(0,{0x90,60,127});input.add(31,{0x80,60,64});output.clear();activeOutput=&output;activeAudio=&audio;activeRecall=&recall;reentry=true;instance->run(32);output.check(2);output.event(0,0,{0x90,60,64});output.event(1,31,{0x80,60,64});require(history(meter)==64,"outer callback alone advances history");
 output.clear();input.clear();input.add(0,{0x90,60,127});mode=failure;instance->run(32);Instance::OutputDiagnostic d;require(instance->outputDiagnosticsStopped(d),"stopped diagnostics");
 if(failure==0){require(d.error==Instance::OutputError::None&&d.nativeReturned&&!d.nativeStatus&&d.published==1&&history(meter)==96,"successful status/history");}
 if(failure==1){output.check(0);require(d.error==Instance::OutputError::Native&&d.nativeReturned&&d.nativeStatus==-717&&instance->lastNativeOutputStatus()==-717&&history(meter)==64,"exact returned failure with no DSP history advance");mode=0;output.clear();instance->run(32);require(history(meter)==96,"nonpoison rejection recovers");}
 if(failure==2){output.check(0);require(d.error==Instance::OutputError::Exception&&d.exception&&d.nativeAttempted&&!d.nativeReturned&&instance->lastNativeOutputStatus()==0&&history(meter)==64,"throw preserves last actually returned status");mode=0;output.clear();instance->run(32);output.check(0);require(history(meter)==64,"exception poison refuses later DSP");}
 if(failure==3||failure==4){output.check(0);require(d.nativeReturned&&!d.nativeStatus&&history(meter)==96&&d.controlsConsumed&&d.error==(failure==3?Instance::OutputError::Stage:Instance::OutputError::Report),"late native corruption before any output publication; DSP history not rolled back");mode=0;output.clear();instance->run(32);output.check(0);require(history(meter)==96,"local corrupt report/event poison no stale replay");}
 if(failure==5){output.check(0);require(d.nativeReturned&&d.nativeStatus==2&&history(meter)==64,"original native poison status");mode=0;output.clear();instance->activate();instance->run(32);output.check(0);require(history(meter)==64,"native poison refuses reset/replay");}
 if(failure==6){output.check(0);require(d.applyReturned&&d.applyStatus==-811&&!d.nativeAttempted&&history(meter)==64,"original apply refusal before dispatch");mode=0;output.clear();instance->run(32);output.check(1);require(history(meter)==96,"apply rejection recovers");}
 if(failure==7){output.check(0);require(d.reflectReturned&&d.reflectStatus==-812&&d.nativeReturned&&!d.nativeStatus&&history(meter)==96,"original reflect refusal after DSP, before publication");mode=0;output.clear();instance->run(32);output.check(1);require(history(meter)==128,"reflect refusal consumes block but never replays staged prior bytes");}
 instance->deactivate();
 }
 telemetry->destroy_cache(meter);std::cout<<"LV2 output owner: authoritative history/state/reentry/capacity and exact refusal/throw/corruption diagnostics PASS\n";
 }catch(const std::exception& e){std::cerr<<e.what()<<'\n';return 1;}}
