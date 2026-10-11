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
const void *retrieve(void *handle, uint32_t key, size_t *size, uint32_t *type, uint32_t *flags) {
  auto &p = *static_cast<Property *>(handle); if (!p.present || key != 1) return nullptr;
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
int main(int argc,char** argv){try{
 require(argc==2,"module argument");auto* library=dlopen(argv[1],RTLD_NOW|RTLD_LOCAL);require(library,"actual NoteRouting module");auto descriptor=reinterpret_cast<const LV2_Descriptor*(*)(uint32_t)>(dlsym(library,"lv2_descriptor"));require(descriptor&&!descriptor(2),"exact descriptor count");
 uint32_t calls=0;LV2_URID_Map mapper{&calls,map};LV2_Feature feature{LV2_URID__map,&mapper};const LV2_Feature* features[]{&feature,nullptr};const LV2_Feature* none[]{nullptr};
 for(uint32_t channels=1;channels<=2;++channels){auto* d=descriptor(channels-1);require(!d->instantiate(d,48000.,"",none),"mandatory cached URIDs");auto instance=d->instantiate(d,48000.,"",features);require(instance,"real native instance");
 std::array<float,1025> left{},right{};left.fill(99);right.fill(99);float gain=1;Sequence input;Output output;output.clear();d->connect_port(instance,0,left.data());if(channels==2)d->connect_port(instance,1,right.data());d->connect_port(instance,channels,&gain);d->connect_port(instance,channels+1,input.get());d->connect_port(instance,channels+2,output.get());d->activate(instance);
 input.add(3,{0x92,69,127});input.add(3,{0x82,69,64});input.add(31,{0x9f,127,1});d->run(instance,64);output.check(3);output.event(0,3,{0x92,69,127});output.event(1,3,{0x82,69,64});output.event(2,31,{0x9f,127,1});for(uint32_t i=0;i<64;++i)require(left[i]==0&&(! (channels==2)||right[i]==0),"authoritative silent output");require(left[64]==99&&right[64]==99,"audio tails");
 const auto* state=static_cast<const LV2_State_Interface*>(d->extension_data(LV2_STATE__interface));require(state,"complete state interface");Property saved;require(!state->save(instance,store,&saved,0,features)&&level(saved)==1,"default state");auto recall=saved;level(recall,.5f);require(!state->restore(instance,retrieve,&recall,0,features),"pending Level recall");
 // Late malformed input must not consume recall or changed controls.
 gain=.75f;input.clear();input.add(0,{0x90,60,127});input.add(1,{0x90,61,128});output.clear();d->run(instance,64);output.check(0);require(!state->save(instance,store,&saved,0,features)&&level(saved)==.5f,"late input preserves queued state");
 gain=1;input.clear();input.add(0,{0x90,60,127});output.clear();d->run(instance,0);output.check(1);output.event(0,0,{0x90,60,64});require(!state->save(instance,store,&saved,0,features)&&level(saved)==.5f,"zero-frame recall routes without frame advance");
 // Full configured output capacity required, even empty prefix; safe empty is16 bytes only.
 gain=.75f;input.clear();output.clear(8);d->run(instance,32);output.check(0);require(!state->save(instance,store,&saved,0,features)&&level(saved)==.5f,"undersized output before scalar consumption");output.clear();input.add(0,{0x90,61,127});gain=1;d->run(instance,32);output.check(1);output.event(0,0,{0x90,61,64});
 output.clear(7);const auto unchanged=output.bytes;d->run(instance,32);require(output.bytes==unchanged,"unsafe empty body untouched");output.clear(24584,3);const auto wrong=output.bytes;d->run(instance,32);require(output.bytes==wrong,"wrong output type untouched");
 // Writable audio sharing input/output atoms or control must not overwrite metadata.
 input.clear();input.add(0,{0x90,62,127});output.clear();const auto incoming=input.bytes;d->connect_port(instance,0,input.get());d->run(instance,32);require(input.bytes==incoming,"audio/input alias preserved");output.check(0);d->connect_port(instance,0,left.data());d->connect_port(instance,channels+2,input.get());d->run(instance,32);require(input.bytes==incoming,"output/input alias preserved");d->connect_port(instance,channels+2,output.get());
 if(channels==2){output.clear();d->connect_port(instance,1,input.bytes.data()+16);d->run(instance,32);require(input.bytes==incoming,"late second-channel input-body alias preserved");output.check(0);d->connect_port(instance,1,left.data()+1);output.clear();d->run(instance,32);output.check(0);require(!state->save(instance,store,&saved,0,features)&&level(saved)==.5f,"late partial output overlap before state/DSP");d->connect_port(instance,1,right.data());}
 left.fill(99);right.fill(99);input.clear();output.clear();d->run(instance,1025);output.check(0);for(uint32_t i=0;i<1024;++i)require(left[i]==0&&(!(channels==2)||right[i]==0),"excess-frame supported prefix silence");require(left[1024]==99&&right[1024]==99,"excess-frame backing-tail preserved");
 // Changed Level +1024notes exceed shared native input budget atomically.
 gain=.8f;input.clear();for(uint32_t i=0;i<1024;++i)input.add(0,{0x90,uint8_t(i%128),127});output.clear();d->run(instance,0);output.check(0);require(!state->save(instance,store,&saved,0,features)&&level(saved)==.5f,"late combined capacity retains Level");gain=1;output.clear();d->run(instance,0);output.check(1024);for(uint32_t i=0;i<1024;++i)output.event(i,0,{0x90,uint8_t(i%128),64});
 input.clear();const auto beforeNew=allocations.load(),beforeDelete=deletions.load();const auto beforeMaps=calls;for(uint32_t i=0;i<1000;++i){output.clear();input.clear();input.add(1,{0x90,69,127});d->run(instance,32);output.check(1);output.event(0,1,{0x90,69,64});}require(allocations==beforeNew&&deletions==beforeDelete&&calls==beforeMaps,"prepared output cycles no C++ heap/URID calls");
 d->deactivate(instance);output.clear();input.clear();d->run(instance,32);output.check(0);d->cleanup(instance);
 }
 dlclose(library);std::cout<<"LV2 generated output: actual canonical mono/stereo timing/order/recall/capacity/aliases/tails/max1024 and2000 allocation-free cycles PASS\n";
 }catch(const std::exception& e){std::cerr<<e.what()<<'\n';return 1;}}
