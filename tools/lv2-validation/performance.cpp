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
  if (!std::strcmp(uri, "urn:ravet:performance-sine:state")) return 1;
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
void normalizer() {
  Sequence sequence; std::array<RavetNativeEventV1, 1024> output{}; uint32_t count = 999;
  sequence.add(0, {0x90, 69, 1}); sequence.add(1, {0x82, 69, 3}); sequence.add(1, {0x90, 70, 0});
  require(ravet::native::Lv2NoteInput::convert(sequence.get(), 3, 4, 5, 2, output.data(), 1024, count) && count == 3, "normalizer timed notes");
  require(output[0].kind == RAVET_NATIVE_NOTE_ON && output[0].channel == 0 && output[0].key == 69 && output[0].value == double(1.f/127.f), "note velocity is exact widened MIDI fraction");
  require(output[1].kind == RAVET_NATIVE_NOTE_OFF_SCOPED && output[1].scope == 3 && output[1].channel == 2 && output[1].value == double(3.f/127.f), "off channel/key scope and release velocity");
  require(output[2].kind == RAVET_NATIVE_NOTE_OFF_SCOPED && output[2].value == 0., "MIDI zero velocity off convention");
  for (uint32_t malformed = 0; malformed < 14; ++malformed) {
    sequence.clear(); sequence.add(0, {0x90, 69, 127});
    if (malformed == 0) sequence.add(0, {0x90, 69, 128});
    if (malformed == 1) sequence.add(0, {0x90, 69});
    if (malformed == 2) sequence.add(0, {69, 127});
    if (malformed == 3) sequence.add(0, {0xf0, 1, 0xf7});
    if (malformed == 4) sequence.add(0, {0xf7});
    if (malformed == 5) sequence.add(0, {0xf9});
    if (malformed == 6) sequence.add(0, {0x90, 69, 127, 1});
    if (malformed == 7) sequence.add(-1, {0x90, 69, 127});
    if (malformed == 8) sequence.add(2, {0x90, 69, 127});
    if (malformed == 9) { sequence.clear(); sequence.add(1, {0x90, 69, 127}); sequence.add(0, {0x80,69,0}); }
    if (malformed == 10) sequence.get()->body.unit = 999;
    if (malformed == 11) sequence.get()->atom.size -= 1;
    if (malformed == 12) sequence.get()->atom.size = 65537;
    if (malformed == 13) sequence.get()->body.pad = 1;
    output[0].kind = 999; count = 999;
    require(!ravet::native::Lv2NoteInput::convert(sequence.get(), 3, 4, 5, 2, output.data(), 1024, count) && output[0].kind == 999 && count == 999, "late malformed sequence conversion atomic");
  }
  sequence.clear();
  for (auto message : {std::initializer_list<uint8_t>{0xb0,1,127}, {0xc0,1}, {0xd0,1}, {0xe0,1,127}, {0xa0,69,127}, {0xf1,1}, {0xf2,1,2}, {0xf3,1}, {0xf6}, {0xf8}, {0xfa}, {0xfb}, {0xfc}, {0xfe}, {0xff}}) sequence.add(0, message);
  sequence.add(0,{1,2,3,4},999);
  require(ravet::native::Lv2NoteInput::convert(sequence.get(),3,4,5,0,nullptr,0,count) && count == 0, "defined unsupported short MIDI and unknown atoms explicitly ignored");
  sequence.clear(); sequence.get()->body.unit = frameTimeType;
  for (uint32_t i=0;i<1024;++i) sequence.add(0,{0x90,uint8_t(i%128),127});
  require(ravet::native::Lv2NoteInput::convert(sequence.get(),3,4,5,0,output.data(),1024,count) && count==1024, "maximum raw and generated note batch");
  require(!ravet::native::Lv2NoteInput::convert(sequence.get(),3,4,5,0,output.data(),1023,count), "whole output budget enforced");
  sequence.add(0,{0xf8}); require(!ravet::native::Lv2NoteInput::convert(sequence.get(),3,4,5,0,output.data(),1024,count), "raw event bound includes ignored messages");
}
int main(int argc, char **argv) {
  try {
    normalizer(); require(argc == 2, "binary argument");
    auto *library = dlopen(argv[1], RTLD_NOW | RTLD_LOCAL); require(library, "actual performance module");
    auto descriptor = reinterpret_cast<const LV2_Descriptor *(*)(uint32_t)>(dlsym(library,"lv2_descriptor")); require(descriptor && !descriptor(2), "bounded public descriptors");
    const LV2_Feature *none[] = {nullptr}; uint32_t mapCalls = 0; LV2_URID_Map mapper{&mapCalls,map}; LV2_Feature feature{LV2_URID__map,&mapper}; const LV2_Feature *features[] = {&feature,nullptr};
    for (uint32_t index=0;index<2;++index) {
      const auto *d=descriptor(index); const auto channels=index+1;
      require(!d->instantiate(d,48000.,"",none) && !d->instantiate(d,0.,"",features), "instrument requires MIDI map and valid rate");
      LV2_URID_Map incompleteMapper{&mapCalls,mapWithoutPosition};LV2_Feature incompleteFeature{LV2_URID__map,&incompleteMapper};const LV2_Feature *incompleteFeatures[]{&incompleteFeature,nullptr};require(!d->instantiate(d,48000.,"",incompleteFeatures),"transport profile requires all cached time URIDs");
      auto instance=d->instantiate(d,48000.,"",features), reference=d->instantiate(d,48000.,"",features); require(instance && reference,"native instrument instances");
      std::array<float,4097> left{},right{},expected{},expectedRight{}; left.fill(99.f);right.fill(99.f);expected.fill(99.f);expectedRight.fill(99.f);
      float gain=.2f, referenceGain=.2f; Sequence input,referenceInput;
      d->connect_port(instance,0,left.data());d->connect_port(reference,0,expected.data());
      if(channels==2) {d->connect_port(instance,1,right.data());d->connect_port(reference,1,expectedRight.data());}
      d->connect_port(instance,channels,&gain);d->connect_port(reference,channels,&referenceGain);
      d->connect_port(instance,channels+1,input.get());d->connect_port(reference,channels+1,referenceInput.get());
      d->activate(instance);d->activate(reference);
      const Position knownPosition;input.position(0,knownPosition);referenceInput.position(0,knownPosition);
      input.add(3,{0x90,69,64});referenceInput.add(3,{0x90,69,64});
      d->run(instance,300);d->run(reference,128);referenceInput.clear();
      d->connect_port(reference,0,expected.data()+128);if(channels==2)d->connect_port(reference,1,expectedRight.data()+128);d->run(reference,172);
      for(uint32_t i=0;i<300;++i) {
        require(left[i]==expected[i],"actual sample-exact block partition"); if(channels==2)require(right[i]==left[i] && expectedRight[i]==left[i],"native stereo center duplicates mono");
        if(i<3)require(left[i]==0.f,"sample-offset leading silence");
        if(i>=3 && i<243) {
          const auto n=i-3; const auto analytic=std::sin(6.283185307179586*440./48000.*n)*(n+1)/240.*double(.2f)*double(64.f/127.f)/16.;
          require(std::abs(double(left[i])-analytic)<1e-8,"native MIDI velocity/frequency/attack analytic samples");
        }
      }
      require(left[300]==99.f && right[300]==99.f,"active prefix tails");
      // Late malformed data and a level change must preserve running DSP history.
      input.clear();input.add(0,{0x90,69,127});input.add(10,{0x90,69,128});gain=.75f;d->run(instance,64);
      for(uint32_t i=0;i<64;++i)require(left[i]==0.f,"whole malformed input fault silence");
      gain=.2f;input.clear();d->run(instance,64);
      d->connect_port(reference,0,expected.data());if(channels==2)d->connect_port(reference,1,expectedRight.data());d->run(reference,64);
      for(uint32_t i=0;i<64;++i)require(left[i]==expected[i],"malformed sequence preserved exact voice/phase history");
      input.clear();input.add(0,{0x90,69,127});input.position(10,Position(.5f));gain=.75f;left.fill(99.f);right.fill(99.f);d->run(instance,64);
      for(uint32_t i=0;i<64;++i)require(left[i]==0.f,"late unsupported time speed rejects full note/control block");require(left[64]==99.f&&right[64]==99.f,"time rejection preserves backing tails");
      input.clear();gain=.2f;input.position(0,Position(1.f,INT64_MIN));d->run(instance,64);d->run(reference,64);
      for(uint32_t i=0;i<64;++i)require(left[i]==expected[i],"malformed time preserved exact native phase and control history");
      const auto *state=static_cast<const LV2_State_Interface *>(d->extension_data(LV2_STATE__interface)); require(state,"state interface");
      Property saved; require(!state->save(instance,store,&saved,0,features),"complete instrument state save");
      require(saved.size==24 && !std::memcmp(saved.bytes.data(),"RAVETPS\0",8) && word(saved,8)==1 && word(saved,12)==1 && word(saved,16)==1 && level(saved)==.2f,"instrument canonical product/schema level");
      auto restored=saved;level(restored,.75f);require(!state->restore(instance,retrieve,&restored,0,features),"instrument level restore");
      input.add(0,{0xf0,1,0xf7});gain=.5f;d->run(instance,64);require(!state->save(instance,store,&saved,0,features)&&level(saved)==.75f,"malformed MIDI preserves pending recall");
      input.clear();gain=.2f;d->run(instance,0);require(!state->save(instance,store,&saved,0,features)&&level(saved)==.75f,"valid zero-frame accepts retained recall");
      auto timeRecall=saved;level(timeRecall,.6f);require(!state->restore(instance,retrieve,&timeRecall,0,features),"pending complete recall before malformed time");
      input.clear();input.position(0,Position(.5f));gain=.5f;d->run(instance,64);require(!state->save(instance,store,&saved,0,features)&&level(saved)==.6f,"malformed time preserves pending complete-state publication");
      input.clear();input.position(0,knownPosition);gain=.2f;d->run(instance,0);require(!state->save(instance,store,&saved,0,features)&&level(saved)==.6f,"valid time zero-frame flush accepts retained recall");
      level(timeRecall,.75f);require(!state->restore(instance,retrieve,&timeRecall,0,features),"restore original fixture Level");d->run(instance,0);input.clear();
      d->run(instance,64);d->run(reference,64);
      for(uint32_t i=0;i<64;++i)require(std::abs(double(left[i])-double(expected[i])*.75/double(.2f))<1e-8,"level recall preserves existing voice phase/history");
      gain=.2f; // Unchanged connected value leaves recalled state until changed.
      d->activate(instance);input.clear();input.add(0,{0x90,69,127});input.add(0,{0x90,69,0});d->run(instance,64);
      for(uint32_t i=0;i<64;++i)require(left[i]==0.f,"same-frame MIDI zero-velocity release order");
      d->activate(instance);input.clear();input.add(0,{0x90,69,0});input.add(0,{0x90,69,127});d->run(instance,64);require(std::abs(left[63])>0.f,"same-frame reverse order starts voice");
      input.clear();input.add(0,{0x80,69,0});d->run(instance,2401);require(left[2399]==0.f && left[2400]==0.f,"release ends without Atom output notifications");
      d->activate(instance);input.clear();input.add(0,{0x91,69,127});input.add(0,{0x92,69,127});d->run(instance,64);
      input.clear();input.add(0,{0x81,69,0});d->run(instance,2500);
      require(std::abs(left[2499])>0.f,"channel/key note release preserves different MIDI channel");
      input.clear();input.add(0,{0x82,69,0});d->run(instance,2401);require(left[2400]==0.f,"matching remaining channel releases");
      // Shared native event capacity includes changed global controls.
      d->activate(instance);input.clear();for(uint32_t i=0;i<1024;++i)input.add(0,{0x90,uint8_t(i%128),127});gain=.5f;d->run(instance,0);
      require(!state->save(instance,store,&saved,0,features)&&level(saved)==.75f,"1024 notes plus changed control reject atomically");
      gain=.2f;d->run(instance,0);input.clear();d->run(instance,64);require(std::abs(left[63])>0.f,"1024 notes accepted when controls unchanged");
      const auto beforeNew=allocations.load(), beforeDelete=deletions.load(); const auto beforeMaps=mapCalls;
      for(uint32_t cycle=0;cycle<1000;++cycle) {
        d->activate(instance);input.clear();input.position(0,knownPosition);input.add(3,{0x9f,127,127});d->run(instance,64);require(std::abs(left[63])>0.f,"bounded repeated note render");
        input.clear();input.add(0,{0x8f,127,64});d->run(instance,2401);require(left[2400]==0.f,"bounded repeated release completion");
      }
      require(allocations.load()==beforeNew && deletions.load()==beforeDelete && mapCalls==beforeMaps,"2000 prepared instrument cycles no C++ heap or mapper calls");
      d->deactivate(instance);d->cleanup(instance);d->deactivate(reference);d->cleanup(reference);
    }
    dlclose(library);std::cout<<"LV2 native MIDI/time instruments: bounded atomic normalizer, timed analytic mono/stereo audio, phase rollback, state, release/order/capacity and 2000 C++ allocation-free prepared cycles passed\n";
  } catch(const std::exception &error) {std::cerr<<error.what()<<'\n';return 1;}
}
