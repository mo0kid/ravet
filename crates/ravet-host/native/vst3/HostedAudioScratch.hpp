#ifndef RAVET_VST3_HOSTED_AUDIO_SCRATCH_HPP
#define RAVET_VST3_HOSTED_AUDIO_SCRATCH_HPP
#include "HostedAudioProfile.hpp"
#include <memory>
#include <stdexcept>
#include <utility>
#include <type_traits>
namespace ravet::vst3::hosted_audio {
// MAIN-only containing-core accounting of requested typed sample extents.
// Allocator headers/foreign heap and fixed metadata are excluded, explicitly.
// Legacy preparation can retain its original admission; a new hosted profile
// admits only when ALL existing and candidate owned scratch fits MaximumBytes.
class ScratchBudget {
    size_t bytes_=0;
public:
    static constexpr size_t MaximumBytes=64u*1024u*1024u;
    bool canAdmit(size_t bytes,bool bounded=true)const noexcept {
        return bytes<=std::numeric_limits<size_t>::max()-bytes_&&
            (!bounded||(bytes_<=MaximumBytes&&bytes<=MaximumBytes-bytes_));
    }
    bool admit(size_t bytes,bool bounded=true)noexcept {
        if(!canAdmit(bytes,bounded))return false;bytes_+=bytes;return true;
    }
    void release(size_t bytes)noexcept {if(bytes>bytes_)std::terminate();bytes_-=bytes;}
    size_t bytes()const noexcept{return bytes_;}
};
// Two precisely sized typed arrays, no channel-vector capacity ambiguity.
// Current + pending values are inline Instance members, at most four extents.
// No method/destructor on AUDIO may construct, destroy or replace this owner.
class Scratch {
    ScratchBudget* budget_=nullptr;
    size_t inputSamples_=0,outputSamples_=0,bytes_=0;
    uint32_t bits_=0,frames_=0;
    std::unique_ptr<float[]> input32_,output32_;
    std::unique_ptr<double[]> input64_,output64_;
    void moveFrom(Scratch&& other)noexcept {
        budget_=std::exchange(other.budget_,nullptr);inputSamples_=std::exchange(other.inputSamples_,0);
        outputSamples_=std::exchange(other.outputSamples_,0);bytes_=std::exchange(other.bytes_,0);
        bits_=std::exchange(other.bits_,0);frames_=std::exchange(other.frames_,0);
        input32_=std::move(other.input32_);output32_=std::move(other.output32_);
        input64_=std::move(other.input64_);output64_=std::move(other.output64_);
    }
public:
    Scratch()=default;
    Scratch(const Scratch&)=delete;Scratch& operator=(const Scratch&)=delete;
    Scratch(Scratch&& other)noexcept{moveFrom(std::move(other));}
    Scratch& operator=(Scratch&& other)noexcept{if(this!=&other){clear();moveFrom(std::move(other));}return *this;}
    ~Scratch(){clear();}
    void clear()noexcept {
        input32_.reset();output32_.reset();input64_.reset();output64_.reset();
        if(budget_)budget_->release(bytes_);
        budget_=nullptr;inputSamples_=outputSamples_=bytes_=0;bits_=frames_=0;
    }
    // Strong failure policy: preserve this current owner while constructing
    // and charging a separate complete candidate. Zero-channel sides allocate
    // nothing; backing typed samples are value-initialized before publication.
    void prepare(ScratchBudget& budget,uint32_t bits,uint32_t frames,uint32_t inputs,uint32_t outputs,bool bounded=true) {
        if((bits!=32&&bits!=64)||frames<1||frames>65536||inputs>64||outputs>64||(!inputs&&!outputs))
            throw std::invalid_argument("hosted audio scratch profile");
        Scratch candidate;
        candidate.bits_=bits;candidate.frames_=frames;
        candidate.inputSamples_=size_t(inputs)*frames;candidate.outputSamples_=size_t(outputs)*frames;
        const size_t samples=candidate.inputSamples_+candidate.outputSamples_;
        const size_t width=bits/8;
        if(samples>std::numeric_limits<size_t>::max()/width)throw std::length_error("hosted audio scratch arithmetic");
        candidate.bytes_=samples*width;
        if(!budget.admit(candidate.bytes_,bounded))throw std::length_error("hosted audio containing-core scratch capacity");
        candidate.budget_=&budget;
        if(bits==32){
            if(candidate.inputSamples_)candidate.input32_=std::make_unique<float[]>(candidate.inputSamples_);
            if(candidate.outputSamples_)candidate.output32_=std::make_unique<float[]>(candidate.outputSamples_);
        }else{
            if(candidate.inputSamples_)candidate.input64_=std::make_unique<double[]>(candidate.inputSamples_);
            if(candidate.outputSamples_)candidate.output64_=std::make_unique<double[]>(candidate.outputSamples_);
        }
        *this=std::move(candidate);
    }
    uint32_t bits()const noexcept{return bits_;}
    uint32_t frames()const noexcept{return frames_;}
    size_t bytes()const noexcept{return bytes_;}
    template<class S> S* input(uint32_t channel)noexcept {
        const auto offset=size_t(channel)*frames_;
        if(offset>=inputSamples_)return nullptr;
        if constexpr(std::is_same_v<S,float>)return bits_==32?input32_.get()+offset:nullptr;
        else if constexpr(std::is_same_v<S,double>)return bits_==64?input64_.get()+offset:nullptr;
        else return nullptr;
    }
    template<class S> S* output(uint32_t channel)noexcept {
        const auto offset=size_t(channel)*frames_;
        if(offset>=outputSamples_)return nullptr;
        if constexpr(std::is_same_v<S,float>)return bits_==32?output32_.get()+offset:nullptr;
        else if constexpr(std::is_same_v<S,double>)return bits_==64?output64_.get()+offset:nullptr;
        else return nullptr;
    }
    template<class Append> bool appendRanges(Append append)const noexcept {
        if(!bytes_)return true;
        if(bits_==32)return (!inputSamples_||append(input32_.get(),inputSamples_*sizeof(float),alignof(float)))&&
            (!outputSamples_||append(output32_.get(),outputSamples_*sizeof(float),alignof(float)));
        if(bits_==64)return (!inputSamples_||append(input64_.get(),inputSamples_*sizeof(double),alignof(double)))&&
            (!outputSamples_||append(output64_.get(),outputSamples_*sizeof(double),alignof(double)));
        return false;
    }
    bool external(const void* pointer,size_t bytes,size_t alignment)const noexcept {
        using parameters_draft::Range;using parameters_draft::range;using parameters_draft::overlaps;
        Range requested{};if(!range(pointer,bytes,1,alignment,requested))return false;
        return appendRanges([&](const void* p,size_t count,size_t align){Range owned{};return range(p,count,1,align,owned)&&!overlaps(requested,owned);});
    }
};
}
#endif
