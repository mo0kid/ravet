#import <AVFoundation/AVFoundation.h>
#import <AudioToolbox/AudioToolbox.h>
#import <Foundation/Foundation.h>
#include "RavetAuHost.h"
#include <array>
#include <atomic>
#include <cmath>
#include <cstring>
#include <limits>
#include <memory>
#include <thread>
#include <vector>

namespace {
using Diagnostic=RavetAuHostDiagnosticV1;
#ifdef RAVET_AUH_TESTING
using Completion=void (^)(AUAudioUnit*,NSError*);
using InstantiateHook=void (*)(AudioComponentDescription,AudioComponentInstantiationOptions,Completion);
InstantiateHook instantiateHook=nullptr;
#endif
struct Span {uintptr_t start=0,end=0;};
bool span(const void* p,size_t n,size_t a,Span& s) noexcept {
 if(!n){s={};return true;} const auto v=reinterpret_cast<uintptr_t>(p);
 if(!p||v%a||n>UINTPTR_MAX-v)return false;s={v,v+n};return true;
}
bool overlaps(Span a,Span b) noexcept{return a.start<a.end&&b.start<b.end&&a.start<b.end&&b.start<a.end;}
Span region(const void* p,size_t n) noexcept {Span s;span(p,n,1,s);return s;}
void clear(Diagnostic& d) noexcept {d={};d.byte_size=sizeof(d);}
void os(Diagnostic& d,OSStatus s) noexcept {++d.returned;d.has_os_status=1;d.os_status=s;}
void error(Diagnostic& d,NSError* e) {
 if(!e)return;d.has_error=1;d.error_code=static_cast<int64_t>(e.code);
 NSData* bytes=[e.domain dataUsingEncoding:NSUTF8StringEncoding];
 const auto n=bytes.length;if(n>sizeof(d.domain)){d.domain_length=UINT32_MAX;return;}
 d.domain_length=static_cast<uint32_t>(n);if(n)std::memcpy(d.domain,bytes.bytes,n);
}
struct Catalogue {std::vector<RavetAuHostComponentV1> components;bool retiring=false;};
struct Instance;
struct Request {
 AudioComponentDescription description{};uint32_t mode=0;
 std::atomic<bool> delivered{false};bool ready=false,cancelled=false;
 __strong AUAudioUnit* unit=nil;__strong NSError* failure=nil;
};
struct Instance {
 __strong AUAudioUnit* unit=nil;__strong AURenderBlock render=nil;
 __strong AURenderPullInputBlock pull=nil;
 RavetAuHostInstanceInfoV1 info{};RavetAuHostProfileV1 profile{};
 std::vector<float> input,output,pulled;
 std::atomic<bool> processing{false};std::atomic<bool> rendering{false};
 bool prepared=false,audio=false,faulted=false,resourcesAttempted=false,retiring=false,retirementFailed=false;uint32_t frames=0;
 std::array<Span,10> activeProtected{};
 double sampleTime=0;std::thread::id worker;
};
static_assert(std::atomic<bool>::is_always_lock_free,"prepared admission must be lock-free");
struct Core {
 std::array<std::unique_ptr<Catalogue>,256> catalogues{};
 std::array<std::shared_ptr<Request>,16> requests{};
 std::array<std::unique_ptr<Instance>,256> instances{};
 uint32_t control=0,audio=0,reservedInstances=0;
};
// Captured foreign async callbacks outlive facade cancellation. The bounded
// registry/provider is intentionally process-lifetime, never a stack teardown.
Core& core(){static Core* c=new Core;return *c;}
bool instanceRange(const Instance& i,Span s) noexcept {
 if(overlaps(s,region(&i,sizeof(i))))return false;
 for(const auto* v:{&i.input,&i.output,&i.pulled})
  if(overlaps(s,region(v->data(),v->size()*sizeof(float))))return false;
 return true;
}
bool external(Span s) noexcept {
 auto& c=core();if(overlaps(s,region(&c,sizeof(c))))return false;
 for(auto& p:c.catalogues)if(p&&(overlaps(s,region(p.get(),sizeof(*p)))||
  overlaps(s,region(p->components.data(),p->components.size()*sizeof(RavetAuHostComponentV1)))))return false;
 for(auto& p:c.requests)if(p&&overlaps(s,region(p.get(),sizeof(*p))))return false;
 for(auto& p:c.instances)if(p&&!instanceRange(*p,s))return false;
 return true;
}
struct Control {
 bool admitted=false;int32_t status=0;
 explicit Control(bool audioBoundary=false){
  if(!NSThread.isMainThread){status=RAVET_AUH_WRONG_THREAD;return;}
  auto& c=core();if(c.control||(!audioBoundary&&c.audio)){status=RAVET_AUH_BUSY;return;}
  ++c.control;admitted=true;
 }
 ~Control(){if(admitted)--core().control;}
};
bool diagReady(Diagnostic* d) noexcept {
 Span s;return span(d,sizeof(*d),alignof(Diagnostic),s)&&external(s);
}
template<class F> int32_t controlCall(Diagnostic* d,F&& f,bool boundary=false) noexcept {
 Control guard(boundary);if(!guard.admitted)return guard.status;
 if(!diagReady(d))return RAVET_AUH_INVALID;clear(*d);
 try {@try{return f(*d);}@catch(NSException*){d->exception=1;return RAVET_AUH_EXCEPTION;}}
 catch(...){d->exception=1;return RAVET_AUH_EXCEPTION;}
}
template<class T> bool caller(T* p,Diagnostic& d) noexcept {
 Span s;return span(p,sizeof(T),alignof(T),s)&&external(s)&&!overlaps(s,region(&d,sizeof(d)));
}
Catalogue* findCatalogue(const void* p){for(auto& x:core().catalogues)if(x.get()==p&&!x->retiring)return x.get();return nullptr;}
Instance* findInstance(const void* p){for(auto& x:core().instances)if(x.get()==p&&!x->retiring)return x.get();return nullptr;}
std::shared_ptr<Request>* findRequest(void* p){for(auto& x:core().requests)if(x.get()==p)return &x;return nullptr;}
bool reapCancelled(Diagnostic* diagnostic=nullptr) noexcept {
 if(core().audio)return true;
 auto& c=core();++c.control;
 struct Exit {~Exit(){--core().control;}} exit;
 bool okay=true;
 for(auto& r:c.requests)if(r&&r->cancelled&&r->ready){r->unit=nil;r->failure=nil;r.reset();--c.reservedInstances;}
 for(auto& p:c.catalogues)if(p&&p->retiring)p.reset();
 for(auto& i:c.instances)if(i&&i->retiring&&!i->retirementFailed){
  try {@try {
   if(i->resourcesAttempted){if(diagnostic)++diagnostic->attempted;[i->unit deallocateRenderResources];if(diagnostic)++diagnostic->returned;}
   i->resourcesAttempted=false;i->render=nil;i->pull=nil;i->unit=nil;i.reset();
  }@catch(NSException*){i->retirementFailed=true;if(diagnostic)diagnostic->exception=1;okay=false;}}
  catch(...){i->retirementFailed=true;if(diagnostic)diagnostic->exception=1;okay=false;}
 }
 return okay;
}
int32_t catalogue(const RavetAuHostFilterV1* filter,void** owner,uint32_t* count,Diagnostic* d){
 return controlCall(d,[&](Diagnostic& q)->int32_t{
  reapCancelled();if(!caller(filter,q)||!caller(owner,q)||!caller(count,q)||overlaps(region(owner,sizeof(*owner)),region(count,sizeof(*count)))||
   filter->byte_size!=sizeof(*filter)||filter->reserved||!filter->type||!filter->maximum_components||filter->maximum_components>256)return RAVET_AUH_INVALID;
  const auto meta=region(filter,sizeof(*filter));if(overlaps(meta,region(owner,sizeof(*owner)))||overlaps(meta,region(count,sizeof(*count))))return RAVET_AUH_INVALID;
  auto slot=core().catalogues.end();for(auto it=core().catalogues.begin();it!=core().catalogues.end();++it)if(!*it){slot=it;break;}
  if(slot==core().catalogues.end())return RAVET_AUH_CAPACITY;
  auto result=std::make_unique<Catalogue>();AudioComponentDescription desc{filter->type,filter->subtype,filter->manufacturer,0,0};
  AudioComponent component=nullptr;
  while((component=AudioComponentFindNext(component,&desc))){
   AudioComponentDescription found{};++q.attempted;auto status=AudioComponentGetDescription(component,&found);os(q,status);if(status)return RAVET_AUH_FOREIGN;
   if(!(found.componentFlags&kAudioComponentFlag_IsV3AudioUnit))continue;
   if(result->components.size()==filter->maximum_components)return RAVET_AUH_CAPACITY;
   for(auto& c:result->components)if(c.type==found.componentType&&c.subtype==found.componentSubType&&c.manufacturer==found.componentManufacturer)return RAVET_AUH_INVALID;
   RavetAuHostComponentV1 c{};c.byte_size=sizeof(c);c.type=found.componentType;c.subtype=found.componentSubType;c.manufacturer=found.componentManufacturer;c.flags=found.componentFlags;
   ++q.attempted;status=AudioComponentGetVersion(component,&c.version);os(q,status);if(status)return RAVET_AUH_FOREIGN;
   CFStringRef name=nullptr;++q.attempted;status=AudioComponentCopyName(component,&name);os(q,status);
   if(status||!name){if(name)CFRelease(name);return RAVET_AUH_FOREIGN;}
   NSString* owned=CFBridgingRelease(name);NSData* bytes=[owned dataUsingEncoding:NSUTF8StringEncoding];
   if(!bytes||bytes.length>sizeof(c.name))return RAVET_AUH_INVALID;
   c.name_length=static_cast<uint32_t>(bytes.length);if(c.name_length)std::memcpy(c.name,bytes.bytes,c.name_length);result->components.push_back(c);
  }
  *count=static_cast<uint32_t>(result->components.size());*owner=result.get();*slot=std::move(result);return RAVET_AUH_OK;
 });
}
int32_t component(const void* owner,uint32_t index,RavetAuHostComponentV1* out,Diagnostic* d){
 return controlCall(d,[&](Diagnostic& q)->int32_t{auto* c=findCatalogue(owner);if(!c||!caller(out,q)||index>=c->components.size())return RAVET_AUH_INVALID;*out=c->components[index];return RAVET_AUH_OK;});
}
int32_t closeCatalogue(void* owner,Diagnostic* d){
 return controlCall(d,[&](Diagnostic& q)->int32_t{auto* c=findCatalogue(owner);if(!c)return RAVET_AUH_INVALID;
  c->retiring=true;q.consumed=1;return reapCancelled(&q)?RAVET_AUH_OK:RAVET_AUH_EXCEPTION;},true);
}
int32_t createAsync(void* owner,uint32_t index,uint32_t mode,void** out,Diagnostic* d){
 return controlCall(d,[&](Diagnostic& q)->int32_t{
  reapCancelled();auto* c=findCatalogue(owner);if(!c||!caller(out,q)||index>=c->components.size()||mode>RAVET_AUH_OUT_OF_PROCESS)return RAVET_AUH_INVALID;
  size_t occupied=core().reservedInstances;for(auto& i:core().instances)occupied+=bool(i);if(occupied>=256)return RAVET_AUH_CAPACITY;
  auto slot=core().requests.end();for(auto it=core().requests.begin();it!=core().requests.end();++it)if(!*it){slot=it;break;}if(slot==core().requests.end())return RAVET_AUH_CAPACITY;
  auto r=std::make_shared<Request>();auto& item=c->components[index];r->description={item.type,item.subtype,item.manufacturer,0,0};r->mode=mode;
  *slot=r;++core().reservedInstances;
  AudioComponentInstantiationOptions options=0;
  if(mode==RAVET_AUH_IN_PROCESS)options=kAudioComponentInstantiation_LoadInProcess;
  if(mode==RAVET_AUH_OUT_OF_PROCESS)options=kAudioComponentInstantiation_LoadOutOfProcess;
  ++q.attempted;
  try {@try {
   auto completion=^(AUAudioUnit* unit,NSError* failure){
    if(r->delivered.exchange(true,std::memory_order_acq_rel))return;
    dispatch_async(dispatch_get_main_queue(), ^{
     r->unit=unit;r->failure=failure;r->ready=true;
     // Main delivery may coincide with an unrelated admitted audio child. Its
     // foreign retirement is deferred to the next admitted control boundary.
     reapCancelled();
    });
   };
#ifdef RAVET_AUH_TESTING
   if(instantiateHook)instantiateHook(r->description,options,completion);else
#endif
   [AUAudioUnit instantiateWithComponentDescription:r->description options:options completionHandler:completion];
  } @catch(NSException*){r->cancelled=true;throw;}}
  catch(...){r->cancelled=true;throw;}
  // Returning from async initiation is not a completed NSError result.
  *out=r.get();return RAVET_AUH_OK;
 });
}
int32_t cancel(void* request,Diagnostic* d){
 return controlCall(d,[&](Diagnostic& q)->int32_t{auto* r=findRequest(request);if(!r||(*r)->cancelled)return RAVET_AUH_INVALID;(*r)->cancelled=true;q.consumed=1;reapCancelled();return RAVET_AUH_OK;},true);
}
int32_t poll(void* request,void** out,RavetAuHostInstanceInfoV1* info,Diagnostic* d){
 return controlCall(d,[&](Diagnostic& q)->int32_t{
  auto* slot=findRequest(request);if(!slot||(*slot)->cancelled||!caller(out,q)||!caller(info,q)||overlaps(region(out,sizeof(*out)),region(info,sizeof(*info))))return RAVET_AUH_INVALID;
  auto r=*slot;if(!r->ready)return RAVET_AUH_PENDING;
  struct Retire {std::shared_ptr<Request>* slot;Diagnostic& q;~Retire(){(*slot)->unit=nil;(*slot)->failure=nil;slot->reset();--core().reservedInstances;q.consumed=1;}} retire{slot,q};
  ++q.attempted;++q.returned;error(q,r->failure);
  auto consume=[]{};
  if(r->failure||!r->unit||q.domain_length==UINT32_MAX){consume();return RAVET_AUH_FOREIGN;}
  auto result=std::make_unique<Instance>();result->unit=r->unit;
  auto desc=result->unit.componentDescription;bool inProcess=result->unit.isLoadedInProcess;
  if(desc.componentType!=r->description.componentType||desc.componentSubType!=r->description.componentSubType||desc.componentManufacturer!=r->description.componentManufacturer){consume();return RAVET_AUH_INVALID;}
  if((r->mode==RAVET_AUH_IN_PROCESS&&!inProcess)||(r->mode==RAVET_AUH_OUT_OF_PROCESS&&inProcess)){consume();return RAVET_AUH_MODE;}
  result->info={sizeof(RavetAuHostInstanceInfoV1),desc.componentType,desc.componentSubType,desc.componentManufacturer,r->mode,uint32_t(inProcess),{0,0}};
  auto destination=core().instances.end();for(auto it=core().instances.begin();it!=core().instances.end();++it)if(!*it){destination=it;break;}
  if(destination==core().instances.end()){consume();return RAVET_AUH_CAPACITY;}
  *out=result.get();*info=result->info;*destination=std::move(result);consume();return RAVET_AUH_OK;
 });
}
int32_t pull(Instance* i,const AudioTimeStamp* time,AUAudioFrameCount frames,NSInteger bus,AudioBufferList* buffers) noexcept {
 // Trusted plug-in callback must supply the complete live ABL for its count.
 // Replacing its pointers avoids writing plug-in samples or losing captured
 // source data through an in-place first pull.
 Span ts,bs;
 if(!i->rendering.load(std::memory_order_acquire)||i->worker!=std::this_thread::get_id()||bus||frames!=i->frames||
  !span(time,sizeof(*time),alignof(AudioTimeStamp),ts)||!span(buffers,offsetof(AudioBufferList,mBuffers),alignof(AudioBufferList),bs)||
  !instanceRange(*i,ts)||!instanceRange(*i,bs))return kAudioUnitErr_InvalidPropertyValue;
 if(!(time->mFlags&kAudioTimeStampSampleTimeValid)||time->mSampleTime!=i->sampleTime||buffers->mNumberBuffers!=i->profile.input_channels||
  !span(buffers,offsetof(AudioBufferList,mBuffers)+buffers->mNumberBuffers*sizeof(AudioBuffer),alignof(AudioBufferList),bs)||
  !instanceRange(*i,bs)||overlaps(ts,bs))return kAudioUnitErr_InvalidPropertyValue;
 for(auto protectedSpan:i->activeProtected)if(overlaps(bs,protectedSpan))return kAudioUnitErr_InvalidPropertyValue;
 for(uint32_t c=0;c<i->profile.input_channels;++c)if(buffers->mBuffers[c].mNumberChannels!=1)return kAudioUnitErr_FormatNotSupported;
 for(uint32_t c=0;c<i->profile.input_channels;++c){auto* p=i->pulled.data()+c*i->profile.maximum_frames;std::memcpy(p,i->input.data()+c*i->profile.maximum_frames,frames*sizeof(float));
  buffers->mBuffers[c].mData=p;buffers->mBuffers[c].mDataByteSize=frames*sizeof(float);}
 return noErr;
}
int32_t prepare(void* handle,const RavetAuHostProfileV1* profile,Diagnostic* d){
 return controlCall(d,[&](Diagnostic& q)->int32_t{
  auto* i=findInstance(handle);if(!i||!caller(profile,q)||profile->byte_size!=sizeof(*profile)||!profile->maximum_frames||profile->maximum_frames>65536||
   !profile->output_channels||profile->output_channels>2||profile->input_channels>2||(profile->input_channels&&profile->input_channels!=profile->output_channels)||
   !std::isfinite(profile->sample_rate)||profile->sample_rate<=0||profile->sample_rate>768000)return RAVET_AUH_INVALID;
  if(i->faulted)return RAVET_AUH_FAULTED;if(i->prepared)return RAVET_AUH_BUSY;
  std::vector<float> input(profile->maximum_frames*profile->input_channels),output(profile->maximum_frames*profile->output_channels),pulled(input.size());
  // Geometry/resource failures may mutate the foreign unit: poison after the
  // first foreign preparation attempt, rather than claiming a rolled-back unit.
  i->faulted=true;
  if(i->unit.outputBusses.count!=1||i->unit.inputBusses.count!=(profile->input_channels?1:0))return RAVET_AUH_INVALID;
  AVAudioFormat* format=[[AVAudioFormat alloc] initStandardFormatWithSampleRate:profile->sample_rate channels:profile->output_channels];if(!format)return RAVET_AUH_INVALID;
  NSError* failure=nil;++q.attempted;BOOL accepted=[i->unit.outputBusses[0] setFormat:format error:&failure];++q.returned;error(q,failure);if(!accepted||failure)return RAVET_AUH_FOREIGN;
  if(profile->input_channels){failure=nil;++q.attempted;accepted=[i->unit.inputBusses[0] setFormat:format error:&failure];++q.returned;error(q,failure);if(!accepted||failure)return RAVET_AUH_FOREIGN;}
  i->unit.maximumFramesToRender=profile->maximum_frames;
  __strong AURenderBlock render=i->unit.renderBlock;if(!render)return RAVET_AUH_INVALID;
  Instance* borrowed=i;__strong AURenderPullInputBlock pullBlock=^OSStatus(AudioUnitRenderActionFlags*,const AudioTimeStamp* t,AUAudioFrameCount n,NSInteger b,AudioBufferList* buffers){return pull(borrowed,t,n,b,buffers);};
  failure=nil;i->resourcesAttempted=true;++q.attempted;accepted=[i->unit allocateRenderResourcesAndReturnError:&failure];++q.returned;error(q,failure);
  if(!accepted||failure){[i->unit deallocateRenderResources];i->resourcesAttempted=false;return RAVET_AUH_FOREIGN;}
  i->input=std::move(input);i->output=std::move(output);i->pulled=std::move(pulled);i->profile=*profile;i->render=render;i->pull=pullBlock;i->prepared=true;i->faulted=false;return RAVET_AUH_OK;
 });
}
int32_t begin(void* handle,void** token,Diagnostic* d){
 return controlCall(d,[&](Diagnostic& q)->int32_t{auto* i=findInstance(handle);if(!i||!caller(token,q))return RAVET_AUH_INVALID;if(i->faulted)return RAVET_AUH_FAULTED;
  if(!i->prepared||i->audio)return RAVET_AUH_BUSY;i->audio=true;++core().audio;*token=i;return RAVET_AUH_OK;},true);
}
struct Processing {Instance* i;~Processing(){i->rendering.store(false,std::memory_order_release);i->processing.store(false,std::memory_order_release);}};
int32_t process(void* token,const RavetAuHostBlockV1* block,Diagnostic* d) noexcept {
 // Opaque AUDIO token is a genuine live admitted instance. Never registry-scan
 // or inspect another child's CONTROL-owned geometry on the callback thread.
 auto* i=static_cast<Instance*>(token);if(!i)return RAVET_AUH_INVALID;
 bool expected=false;if(!i->processing.compare_exchange_strong(expected,true,std::memory_order_acquire))return RAVET_AUH_BUSY;
 Processing guard{i};Span ds,meta;if(!span(d,sizeof(*d),alignof(Diagnostic),ds)||!span(block,sizeof(*block),alignof(RavetAuHostBlockV1),meta)||overlaps(ds,meta)||
  !instanceRange(*i,ds)||!instanceRange(*i,meta)||overlaps(ds,region(&core(),sizeof(Core)))||overlaps(meta,region(&core(),sizeof(Core))))return RAVET_AUH_INVALID;
 clear(*d);if(!i->audio||!i->prepared)return RAVET_AUH_BUSY;if(i->faulted)return RAVET_AUH_FAULTED;
 if(block->byte_size!=sizeof(*block)||block->input_channels!=i->profile.input_channels||block->output_channels!=i->profile.output_channels||block->frames>i->profile.maximum_frames)return RAVET_AUH_INVALID;
 Span ins,outs;if(!span(block->inputs,block->input_channels*sizeof(RavetAuHostInputV1),alignof(RavetAuHostInputV1),ins)||
  !span(block->outputs,block->output_channels*sizeof(RavetAuHostOutputV1),alignof(RavetAuHostOutputV1),outs)||!instanceRange(*i,ins)||!instanceRange(*i,outs)||
  overlaps(ins,outs)||overlaps(ins,meta)||overlaps(outs,meta)||overlaps(ins,ds)||overlaps(outs,ds)||overlaps(ins,region(&core(),sizeof(Core)))||overlaps(outs,region(&core(),sizeof(Core))))return RAVET_AUH_INVALID;
 std::array<Span,2> inputs{},outputs{};
 for(uint32_t c=0;c<block->input_channels;++c){auto& ch=block->inputs[c];if(ch.reserved||ch.capacity<block->frames||!span(ch.samples,block->frames*sizeof(float),alignof(float),inputs[c])||
  !instanceRange(*i,inputs[c])||overlaps(inputs[c],ds)||overlaps(inputs[c],meta)||overlaps(inputs[c],ins)||overlaps(inputs[c],outs)||overlaps(inputs[c],region(&core(),sizeof(Core))))return RAVET_AUH_INVALID;}
 for(uint32_t c=0;c<block->output_channels;++c){auto& ch=block->outputs[c];if(ch.reserved||ch.capacity<block->frames||!span(ch.samples,block->frames*sizeof(float),alignof(float),outputs[c])||
  !instanceRange(*i,outputs[c])||overlaps(outputs[c],ds)||overlaps(outputs[c],meta)||overlaps(outputs[c],ins)||overlaps(outputs[c],outs)||overlaps(outputs[c],region(&core(),sizeof(Core))))return RAVET_AUH_INVALID;
  for(uint32_t j=0;j<c;++j)if(overlaps(outputs[c],outputs[j]))return RAVET_AUH_INVALID;}
 for(uint32_t c=0;c<block->input_channels;++c)if(block->frames)std::memcpy(i->input.data()+c*i->profile.maximum_frames,block->inputs[c].samples,block->frames*sizeof(float));
 struct {UInt32 count;AudioBuffer buffers[2];} storage{block->output_channels,{}};
 for(uint32_t c=0;c<block->output_channels;++c)storage.buffers[c]={1,static_cast<UInt32>(block->frames*sizeof(float)),i->output.data()+c*i->profile.maximum_frames};
 AudioTimeStamp time{};time.mSampleTime=static_cast<double>(block->sample_position);time.mFlags=kAudioTimeStampSampleTimeValid;AudioUnitRenderActionFlags flags=0;
 i->activeProtected={ds,meta,ins,outs,inputs[0],inputs[1],outputs[0],outputs[1],region(&core(),sizeof(Core)),{}};
 i->frames=block->frames;i->sampleTime=time.mSampleTime;i->worker=std::this_thread::get_id();i->rendering.store(true,std::memory_order_release);
 auto silence=[&]{for(uint32_t c=0;c<block->output_channels;++c)if(block->frames)std::memset(block->outputs[c].samples,0,block->frames*sizeof(float));};
 try {@try {
  ++d->attempted;OSStatus result=i->render(&flags,&time,block->frames,0,reinterpret_cast<AudioBufferList*>(&storage),block->input_channels?i->pull:static_cast<AURenderPullInputBlock>(nil));os(*d,result);
  if(result){i->faulted=true;silence();return RAVET_AUH_FOREIGN;}
  if(storage.count!=block->output_channels){i->faulted=true;silence();return RAVET_AUH_INVALID;}
  for(uint32_t c=0;c<block->output_channels;++c)if(storage.buffers[c].mNumberChannels!=1||storage.buffers[c].mData!=i->output.data()+c*i->profile.maximum_frames||storage.buffers[c].mDataByteSize<block->frames*sizeof(float)){i->faulted=true;silence();return RAVET_AUH_INVALID;}
  for(uint32_t c=0;c<block->output_channels;++c)if(block->frames)std::memcpy(block->outputs[c].samples,i->output.data()+c*i->profile.maximum_frames,block->frames*sizeof(float));return RAVET_AUH_OK;
 }@catch(NSException*){d->exception=1;i->faulted=true;silence();return RAVET_AUH_EXCEPTION;}}
 catch(...){d->exception=1;i->faulted=true;silence();return RAVET_AUH_EXCEPTION;}
}
int32_t end(void* token,Diagnostic* d){
 return controlCall(d,[&](Diagnostic& q)->int32_t{auto* i=findInstance(token);if(!i||!i->audio)return RAVET_AUH_INVALID;if(i->processing.load(std::memory_order_acquire))return RAVET_AUH_BUSY;
  i->audio=false;--core().audio;return reapCancelled(&q)?RAVET_AUH_OK:RAVET_AUH_EXCEPTION;},true);
}
int32_t stop(void* handle,Diagnostic* d){
 return controlCall(d,[&](Diagnostic& q)->int32_t{auto* i=findInstance(handle);if(!i)return RAVET_AUH_INVALID;
  if(i->resourcesAttempted){const bool faulted=i->faulted;i->faulted=true;++q.attempted;[i->unit deallocateRenderResources];++q.returned;i->resourcesAttempted=false;i->prepared=false;i->render=nil;i->pull=nil;i->faulted=faulted;}
  return i->faulted?RAVET_AUH_FAULTED:RAVET_AUH_OK;
 });
}
int32_t destroy(void* handle,Diagnostic* d){
 return controlCall(d,[&](Diagnostic& q)->int32_t{auto* i=findInstance(handle);if(!i)return RAVET_AUH_INVALID;
  if(i->audio)return RAVET_AUH_BUSY;i->retiring=true;q.consumed=1;
  return reapCancelled(&q)?RAVET_AUH_OK:RAVET_AUH_EXCEPTION;},true);
}
const RavetAuHostEntryV1 entry{sizeof(RavetAuHostEntryV1),1,catalogue,component,closeCatalogue,createAsync,poll,cancel,prepare,begin,process,end,stop,destroy};
}
extern "C" const RavetAuHostEntryV1* ravet_au_host_v1(){return &entry;}

#ifdef RAVET_AUH_TESTING
// Explicit fixture-only public surface; absent from the maintained production
// provider. No fixture accesses Core/Instance private layout or foreign SDK data.
extern "C" int32_t ravet_auh_test_set_instantiate_hook(InstantiateHook hook){
 Control c;if(!c.admitted)return c.status;instantiateHook=hook;return 0;
}
extern "C" void ravet_auh_test_counts(uint32_t* pending,uint32_t* instances,uint32_t* reserved){
 if(!NSThread.isMainThread)return;*pending=0;*instances=0;
 for(auto& r:core().requests)*pending+=bool(r);for(auto& i:core().instances)*instances+=bool(i);
 *reserved=core().reservedInstances;
}
#endif
