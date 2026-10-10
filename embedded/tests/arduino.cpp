#include "../arduino/src/RavetEmbedded.h"
#include <cassert>
#include <limits>
using namespace ravet::embedded;
int main() {
    StereoAdapter<Gain, 64> audio;
    assert(audio.prepare({48000, 64, 8}));
    float samples[] = {1,-1,1,-1,1,-1,99,99};
    ParameterEvent events[] = {{0,0,2}, {1,0,3}, {1,0,4}};
    assert(audio.process(samples,8,samples,8,3,events,3) == Status::Continue);
    assert(samples[0]==2 && samples[2]==4 && samples[4]==4 && samples[6]==99);
    const ParameterEvent bad[] = {{0,0,1}, {2,0,5}};
    assert(audio.process(samples,8,samples,8,3,bad,2) == Status::Fault);
    assert(audio.processor().gain()==4 && samples[0]==0 && samples[5]==0 && samples[6]==99);
    ParameterEvent zero{0,0,0.5f};
    assert(audio.process(nullptr,0,nullptr,0,0,&zero,1)==Status::Continue);
    assert(audio.processor().gain()==0.5f);
    assert(audio.process(samples,1,samples,8,2)==Status::Fault);
    assert(samples[4]==0 && samples[6]==99);
    assert(audio.process(nullptr,0,nullptr,0,SIZE_MAX)==Status::Fault);
    ParameterEvent unordered[] = {{1,0,2}, {0,0,3}};
    assert(audio.process(samples,8,samples,8,3,unordered,2)==Status::Fault);
    assert(audio.processor().gain()==0.5f);
    ParameterEvent nan{0,0,std::numeric_limits<float>::quiet_NaN()};
    assert(audio.process(samples,8,samples,8,3,&nan,1)==Status::Fault);
    assert(!audio.prepare({48000,65,8}));
    assert(audio.process(samples,8,samples,8,3)==Status::Fault);
    assert(audio.prepare({48000,64,8}));
    audio.reset();
    assert(audio.processor().gain()==0.5f);
}
