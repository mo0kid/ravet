// Public synthetic binary ABI contracts; no DAW/private assets.
#include "RavetGainMeters.h"
#include <array>
#include <cmath>
#include <cstring>
#include <iostream>
#include <limits>
#include <stdexcept>
#include <type_traits>
static_assert(sizeof(RavetGainMeterReading)==32 && sizeof(RavetGainMeters)==128);
static_assert(offsetof(RavetGainMeterReading,samples)==16);
static_assert(offsetof(RavetGainMeters,output)==64);
void check(bool ok,const char* reason) { if(!ok) throw std::runtime_error(reason); }
void close(double a,double b,double tolerance) { check(std::abs(a-b)<=tolerance*std::max(1.,std::abs(b)),"meter value"); }
template<class Sample,class Handle,class Prepare,class Restore,class Process,class Read,class Reset,class Destroy>
unsigned validate(Handle* handle,Prepare prepare,Restore restore,Process process,Read read,Reset reset,Destroy destroy) {
  check(handle!=nullptr,"create"); unsigned blocks=0;
  RavetGainMeters meters{};
  check(read(handle,&meters)==0 && meters.input[0].samples==0,"new meters");
  RavetGainMeters sentinel{};sentinel.input[0].peak=99.;meters=sentinel;
  check(read(nullptr,&meters)==1 && std::memcmp(&meters,&sentinel,sizeof meters)==0,"null atomic");
  check(read(handle,nullptr)==1,"null output");
  for(double rate:{44100.,48000.,96000.}) for(unsigned channels:{1u,2u}) {
    check(prepare(handle,rate,channels,4)==0,"prepare");check(restore(handle,0.5f,0)==0,"restore");
    std::array<Sample,4> left{{Sample(0.125),Sample(-0.25),Sample(0.5),Sample(-1.)}},right{{Sample(1.),Sample(-0.5),Sample(0.25),Sample(-0.125)}};
    const Sample* inputs[]{left.data(),right.data()};std::array<Sample,4> l{},r{};Sample* outputs[]{l.data(),r.data()};
    float gain=0.;uint32_t bypass=0;
    check(process(handle,inputs,outputs,channels,4,nullptr,0,&gain,&bypass)==0,"process");++blocks;
    check(read(handle,&meters)==0,"read");double rms=std::sqrt((1.+0.25+0.0625+0.015625)/4.);
    double tolerance=std::is_same_v<Sample,double>?1e-14:1e-7;
    for(unsigned c=0;c<channels;++c) {close(meters.input[c].peak,1.,tolerance);close(meters.input[c].rms,rms,tolerance);close(meters.output[c].peak,0.5,tolerance);close(meters.output[c].rms,rms*0.5,tolerance);
      check(meters.input[c].samples==4 && meters.output[c].samples==4 && meters.input[c].nonfinite==0 && meters.input[c].reserved==0 && meters.output[c].reserved==0,"counts/flags");}
    if(channels==1) check(meters.input[1].samples==0 && meters.output[1].peak==0.,"mono zeros");
    auto retained=meters;
    check(process(handle,nullptr,nullptr,channels,0,nullptr,0,&gain,&bypass)==0,"empty");++blocks;
    check(read(handle,&meters)==0 && std::memcmp(&meters,&retained,sizeof meters)==0,"empty retained");
    check(process(handle,inputs,outputs,channels,4,nullptr,1025,&gain,&bypass)==1,"rejected");
    check(read(handle,&meters)==0 && std::memcmp(&meters,&retained,sizeof meters)==0,"fault retained");
    check(restore(handle,1.,1)==0,"bypass restore");left[0]=std::numeric_limits<Sample>::quiet_NaN();left[1]=std::numeric_limits<Sample>::infinity();left[2]=Sample(0.5);left[3]=Sample(0.);
    check(process(handle,inputs,outputs,channels,4,nullptr,0,&gain,&bypass)==0,"nonfinite");++blocks;
    check(read(handle,&meters)==0 && meters.input[0].nonfinite==1 && meters.output[0].nonfinite==1 && meters.input[0].samples==4,"nonfinite diagnostics");
    close(meters.input[0].peak,0.5,tolerance);close(meters.input[0].rms,0.25,tolerance);
    reset(handle);check(read(handle,&meters)==0 && meters.input[0].samples==0 && meters.output[0].nonfinite==0,"reset");
  }
  destroy(handle);return blocks;
}
int main() {try {
  auto blocks=validate<float>(ravet_gain_create(),ravet_gain_prepare,ravet_gain_restore,ravet_gain_process,ravet_gain_meters,ravet_gain_reset,ravet_gain_destroy);
  blocks+=validate<double>(ravet_gain64_create(),ravet_gain64_prepare,ravet_gain64_restore,ravet_gain64_process,ravet_gain64_meters,ravet_gain64_reset,ravet_gain64_destroy);
  auto* h=ravet_gain64_create();check(ravet_gain64_prepare(h,48000.,1,4)==0 && ravet_gain64_restore(h,1.,0)==0,"large prepare");
  double exact=std::nextafter(1.,2.);std::array<double,4> x{{exact,exact,exact,exact}},y{};const double* input[]{x.data()};double* output[]{y.data()};float gain=0.;uint32_t bypass=0;RavetGainMeters meters{};
  check(ravet_gain64_process(h,input,output,1,4,nullptr,0,&gain,&bypass)==0 && ravet_gain64_meters(h,&meters)==0 && meters.input[0].peak==exact && meters.input[0].rms==exact,"native f64 levels");++blocks;
  x.fill(1e200);check(ravet_gain64_process(h,input,output,1,4,nullptr,0,&gain,&bypass)==0 && ravet_gain64_meters(h,&meters)==0 && meters.input[0].rms==1e200,"large f64 RMS");++blocks;ravet_gain64_destroy(h);
  std::cout<<"Native meter layouts/f32/f64/counts/diagnostics/retention: "<<blocks<<" blocks passed\n";
} catch(const std::exception& e) {std::cerr<<e.what()<<"\n";return 1;} }
