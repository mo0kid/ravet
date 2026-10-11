#include "ControllerHost.hpp"
#include "PerformanceParameters.hpp"
#include <cmath>
#include <iostream>
#include <stdexcept>
void require(bool okay,const char *message){if(!okay)throw std::runtime_error(message);}
struct Node final:AAX_IMIDINode {
 std::array<AAX_CMidiPacket,1024> packets{};AAX_CMidiStream stream{0,packets.data()};uint32_t reads=0;bool absent=false,throws=false;
 AAX_CMidiStream *GetNodeBuffer()override{++reads;if(throws)throw std::runtime_error("synthetic node refusal");return absent?nullptr:&stream;}
 AAX_Result PostMIDIPacket(AAX_CMidiPacket*)override{return AAX_ERROR_UNIMPLEMENTED;}
 AAX_ITransport *GetTransport()override{throw std::runtime_error("unversioned transport must not be requested");}
 void note(uint32_t at,uint8_t status,uint8_t key,uint8_t velocity){packets[stream.mBufferSize++]={at,3,{status,key,velocity,0},0};}
};
using Model=RavetPerformanceAaxParameters;
int main(){try{
 for(uint32_t channels:{1u,2u}){
  ControllerHost host;host.input=AAX_eStemFormat_None;host.output=channels==1?AAX_eStemFormat_Mono:AAX_eStemFormat_Stereo;host.effect=channels==1?"org.ravet.performance.mono":"org.ravet.performance.stereo";
  std::unique_ptr<Model> model(createPerformanceAax(channels));require(model->Initialize(host.unknown())==AAX_SUCCESS&&model->nativeStatus()==0&&host.registered.size()==1,"real SDK instrument None input and exact Level-only schema Initialize");
  AAX_CString bypass("sentinel");require(model->GetMasterBypassParameter(&bypass)==AAX_ERROR_INVALID_PARAMETER_ID&&bypass=="sentinel","instrument has no fabricated native bypass");
  Model::Owner *owner=nullptr;require(model->ResetFieldData(0,&owner,sizeof(owner))==AAX_SUCCESS&&owner,"real SDK instrument ready owner publication");
  std::array<std::array<float,1026>,2> samples{};for(auto &ch:samples)ch.fill(77.f);float *out[]={samples[0].data(),samples[1].data()};Node node;int32_t frames=240;Model::Context context{&owner,&frames,out,&node};Model::Context *contexts[]={&context};node.note(3,0x90,69,64);Model::process(contexts,contexts+1);
  require(model->lastProcessStatus()==0&&node.reads==1,"one real public node buffer query per actual algorithm callback");
  for(uint32_t i=0;i<240;++i){const auto n=i<3?0:i-3;const auto analytic=i<3?0.:std::sin(6.283185307179586*440./48000.*n)*(n+1)/240.*double(.2f)*double(64.f/127.f)/16.;require(std::abs(double(samples[0][i])-analytic)<1e-8,"SDK model independent timed note analytic native audio");if(channels==2)require(samples[0][i]==samples[1][i],"SDK model centered stereo");}require(samples[0][240]==77.f,"SDK model active tail");
  context.notes=nullptr;frames=64;Model::process(contexts,contexts+1);require(model->lastProcessStatus()==1&&samples[0][0]==0.f&&owner->ready(),"null node fails closed locally without invalidating valid native owner");context.notes=&node;node.absent=true;Model::process(contexts,contexts+1);require(model->lastProcessStatus()==1,"null stream malformed");node.absent=false;node.throws=true;Model::process(contexts,contexts+1);require(model->lastProcessStatus()==1,"throwing node records local rejection and silence");node.throws=false;node.stream.mBufferSize=0;
  require(model->UpdateParameterNormalizedValue("p1",.75,AAX_eUpdateSource_Unspecified)==AAX_SUCCESS,"real acknowledged SDK Level intent");bool nested=false;host.onCurrent=[&]{nested=true;require(model->Uninitialize()==AAX_ERROR_INVALID_ARGUMENT,"instrument SDK notification cannot retire live borrower");};require(model->UpdateParameterNormalizedValue("p1",.5,AAX_eUpdateSource_Unspecified)==AAX_SUCCESS&&nested,"instrument guarded foreign acknowledgement");host.onCurrent={};
  Model::process(contexts,contexts+1);require(model->lastProcessStatus()==0,"valid empty stream processes running native voices and queued Level");
  require(model->resetStopped()==AAX_SUCCESS&&model->reprepareStopped()==AAX_SUCCESS&&model->Uninitialize()==AAX_SUCCESS&&host.references==1&&host.registered.empty(),"instrument checked stopped reset/reprepare/borrower retirement");
 }
 for(uint32_t channels:{1u,2u}){ControllerHost host;host.effect=channels==1?"org.ravet.performance.mono":"org.ravet.performance.stereo";host.input=host.output=channels==1?AAX_eStemFormat_Mono:AAX_eStemFormat_Stereo;std::unique_ptr<Model> model(createPerformanceAax(channels));require(model->Initialize(host.unknown())==AAX_ERROR_INVALID_ARGUMENT&&host.registered.empty(),"instrument exact None stem rejects effect-shaped host before object publication");require(model->Uninitialize()==AAX_SUCCESS,"failed instrument setup retires proxies");}
 std::cout<<"PASS real SDK native AAX instrument None/mono/stereo Initialize, Level-only schema, timed MIDI node callback, rejected/null/throwing node silence and checked retirement\n";return 0;
 }catch(const std::exception &e){std::cerr<<e.what()<<'\n';return 1;}}
