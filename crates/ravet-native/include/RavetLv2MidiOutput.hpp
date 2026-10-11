#pragma once
#include "RavetNativeMidiOutput.hpp"
#include "lv2/atom/atom.h"
#include <array>
#include <cstring>
#include <limits>

namespace ravet::native {
struct Lv2MidiOutputTypes {
    uint32_t chunk=0,sequence=0,midi=0,frameTime=0;
    bool valid()const noexcept{return chunk&&sequence&&midi&&frameTime&&chunk!=sequence&&chunk!=midi&&chunk!=frameTime&&sequence!=midi&&sequence!=frameTime&&midi!=frameTime;}
};
struct Lv2ProtectedRange { uintptr_t first=0,last=0; };
// External-SDK, serialized fixed-storage output staging. CONTROL supplies cached
// URIDs; AUDIO never maps/query/allocates. Caller provides live 8-byte aligned
// Atom allocation covering header + declared body capacity every run, plus
// complete known protected ranges. Scalar/range checks do not prove allocations.
// Stage all canonical native records before copying any host event prefix.
// Failed stage/admission invalidates readiness; stale storage is diagnostic only.
// No SysEx/UMP/identity/expression or implicit scoped-release policy.
template<size_t Capacity=1024> class Lv2MidiOutput final {
    static_assert(Capacity>0&&Capacity<=1024);
public:
    static constexpr size_t maximumBodyBytes=sizeof(LV2_Atom_Sequence_Body)+Capacity*(sizeof(LV2_Atom_Event)+8);
    static constexpr size_t maximumBytes=sizeof(LV2_Atom)+maximumBodyBytes;
    enum class Error:uint32_t{None,Types,Admission,InsufficientCapacity,Records,NotStaged};
    struct Diagnostic {Error error=Error::None;uint32_t field=0,staged=0,published=0;};
    class Admission {
        friend class Lv2MidiOutput;
        const Lv2MidiOutput* owner_=nullptr;uint64_t generation_=0;
        const Lv2MidiOutput* rangeOwner_=nullptr;uint64_t rangeGeneration_=0;
        void* pointer_=nullptr;uintptr_t knownFirst_=0,knownLast_=0;uint32_t capacity_=0;bool empty_=false,full_=false,consumed_=false;
    public:
        Admission()=default;Admission(const Admission&)=delete;Admission& operator=(const Admission&)=delete;
        bool canEmpty()const noexcept{return owner_&&owner_->current(*this)&&empty_;}
        bool complete()const noexcept{return owner_&&owner_->current(*this)&&full_;}
        uintptr_t first()const noexcept{return rangeOwner_&&rangeOwner_->knownLive_&&rangeOwner_->generation_==rangeGeneration_?knownFirst_:0;}
        uintptr_t last()const noexcept{return rangeOwner_&&rangeOwner_->knownLive_&&rangeOwner_->generation_==rangeGeneration_?knownLast_:0;}
    };
    explicit Lv2MidiOutput(Lv2MidiOutputTypes types={}):types_(types){}
    Lv2MidiOutput(const Lv2MidiOutput&)=delete;Lv2MidiOutput& operator=(const Lv2MidiOutput&)=delete;
    bool valid()const noexcept{return types_.valid();}
    // Does not write output. An undersized but otherwise independently admitted
    // Chunk may permit only a complete 16-byte empty Sequence. Wrong type,
    // overflow, alignment or any known alias permits no output write at all.
    bool admit(void* output,const Lv2ProtectedRange* protectedRanges,uint32_t count,Admission& admission){
        ready_=false;admissionLive_=false;knownLive_=false;diagnostic_={};diagnostic_.error=Error::Admission;
        if(!valid()){diagnostic_.error=Error::Types;return false;}
        if(generation_==UINT64_MAX)return false;++generation_;
        const auto first=reinterpret_cast<uintptr_t>(output);
        if(!first||first%8||sizeof(LV2_Atom)>UINTPTR_MAX-first||count>2048||(count&&!protectedRanges))return false;
        const auto rangesFirst=reinterpret_cast<uintptr_t>(protectedRanges),rangesBytes=sizeof(Lv2ProtectedRange)*size_t(count);
        if(count&&(rangesFirst%alignof(Lv2ProtectedRange)||rangesBytes>UINTPTR_MAX-rangesFirst))return false;
        auto protectedSpan=[&](uintptr_t last){
            const auto self=reinterpret_cast<uintptr_t>(this),token=reinterpret_cast<uintptr_t>(&admission);
            if((first<self+sizeof(*this)&&self<last)||(first<token+sizeof(admission)&&token<last)||(count&&first<rangesFirst+rangesBytes&&rangesFirst<last))return true;
            for(uint32_t i=0;i<count;++i){const auto& range=protectedRanges[i];if(range.last<range.first||(first<range.last&&range.first<last))return true;}return false;
        };
        if(protectedSpan(first+sizeof(LV2_Atom)))return false;
        knownLive_=true;admission.rangeOwner_=this;admission.rangeGeneration_=generation_;admission.owner_=nullptr;admission.pointer_=nullptr;admission.knownFirst_=first;admission.knownLast_=first+sizeof(LV2_Atom);admission.empty_=false;admission.full_=false;admission.consumed_=false;
        LV2_Atom header{};std::memcpy(&header,output,sizeof(header));
        if(uint64_t(sizeof(header))+header.size>UINTPTR_MAX-first)return false;
        admission.knownLast_=first+sizeof(header)+header.size;
        if(header.type!=types_.chunk||protectedSpan(admission.knownLast_))return false;
        admissionLive_=true;admission.owner_=this;admission.generation_=generation_;admission.pointer_=output;admission.capacity_=header.size;
        admission.empty_=header.size>=sizeof(LV2_Atom_Sequence_Body);
        admission.full_=header.size>=maximumBodyBytes;
        diagnostic_.error=admission.full_?Error::None:Error::InsufficientCapacity;return admission.full_;
    }
    // Begin a new owner block even when its metadata cannot be admitted. Prior
    // stage/diagnostic/publication state is not replayed after early rejection.
    void beginBlock()noexcept{ready_=false;admissionLive_=false;knownLive_=false;diagnostic_={};diagnostic_.error=Error::Admission;}
    bool stage(const RavetNativeEventV1* events,uint32_t count,uint32_t frames){
        ready_=false;diagnostic_.staged=diagnostic_.published=0;diagnostic_.error=Error::Records;
        if(!valid()||frames>65536||count>Capacity||(count&&!events))return false;
        const auto first=reinterpret_cast<uintptr_t>(events);const auto bytes=sizeof(*events)*size_t(count);const auto self=reinterpret_cast<uintptr_t>(this);
        if(count&&(first%alignof(RavetNativeEventV1)||bytes>UINTPTR_MAX-first||(first<self+sizeof(*this)&&self<first+bytes)))return false;
        LV2_Atom_Sequence sequence{};sequence.atom.type=types_.sequence;sequence.atom.size=sizeof(sequence.body);sequence.body.unit=types_.frameTime;
        std::memcpy(candidate_.data(),&sequence,sizeof(sequence));uint32_t previous=0;size_t cursor=sizeof(sequence);
        for(uint32_t i=0;i<count;++i){NativeMidiMessage message{};diagnostic_.field=i;
            std::array<uint8_t,8> valueBytes{};const auto* foreign=reinterpret_cast<const volatile uint8_t*>(&events[i].value);for(size_t byte=0;byte<8;++byte)valueBytes[byte]=foreign[byte];uint64_t valueBits=0;std::memcpy(&valueBits,valueBytes.data(),8);const uint64_t observed=valueBits;
            if((events[i].kind==RAVET_NATIVE_NOTE_ON||events[i].kind==RAVET_NATIVE_NOTE_OFF)&&((observed&UINT64_C(0x7ff0000000000000))==UINT64_C(0x7ff0000000000000)))return false;
            if(events[i].kind==RAVET_NATIVE_MIDI&&observed)return false;
            if(events[i].offset<previous||!nativeMidiOutput(events[i],frames,message))return false;previous=events[i].offset;
            LV2_Atom_Event event{};event.time.frames=message.offset;event.body.type=types_.midi;event.body.size=message.size;
            std::memcpy(candidate_.data()+cursor,&event,sizeof(event));std::memset(candidate_.data()+cursor+sizeof(event),0,8);std::memcpy(candidate_.data()+cursor+sizeof(event),message.bytes.data(),message.size);cursor+=sizeof(event)+8;
        }
        sequence.atom.size=uint32_t(cursor-sizeof(LV2_Atom));std::memcpy(candidate_.data(),&sequence,sizeof(sequence));
        std::memcpy(bytes_.data(),candidate_.data(),cursor);used_=uint32_t(cursor);count_=count;ready_=true;diagnostic_.error=Error::None;diagnostic_.staged=count;return true;
    }
    // Every publication consumes readiness/admission once. Borrowed host memory
    // must stay unchanged/live since admit. No callback/partial append/replay.
    bool publish(Admission& admission){
        const bool staged=ready_;ready_=false;
        if(!staged||!current(admission)||!admission.full_||used_>sizeof(LV2_Atom)+admission.capacity_){diagnostic_.error=Error::NotStaged;return false;}
        admission.consumed_=true;std::memcpy(admission.pointer_,bytes_.data(),used_);diagnostic_.published=count_;return true;
    }
    bool empty(Admission& admission){
        ready_=false;if(!current(admission)||!admission.empty_)return false;
        admission.consumed_=true;LV2_Atom_Sequence sequence{};sequence.atom.type=types_.sequence;sequence.atom.size=sizeof(sequence.body);sequence.body.unit=types_.frameTime;std::memcpy(admission.pointer_,&sequence,sizeof(sequence));diagnostic_.published=0;return true;
    }
    const Diagnostic& diagnosticsStopped()const noexcept{return diagnostic_;}
    bool readyStopped()const noexcept{return ready_;}
    uint32_t countStopped()const noexcept{return count_;}
    const uint8_t* bytesStopped()const noexcept{return bytes_.data();}
private:
    bool current(const Admission& a)const noexcept{return admissionLive_&&a.owner_==this&&a.generation_==generation_&&!a.consumed_;}
    Lv2MidiOutputTypes types_{};uint64_t generation_=0;bool ready_=false,admissionLive_=false,knownLive_=false;uint32_t used_=sizeof(LV2_Atom_Sequence),count_=0;Diagnostic diagnostic_{};
    alignas(8)std::array<uint8_t,maximumBytes> candidate_{},bytes_{};
};
} // namespace ravet::native
