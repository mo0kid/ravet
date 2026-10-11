#import "RavetAuHost.h"
#import <AVFoundation/AVFoundation.h>
#import <AudioToolbox/AUAudioUnitImplementation.h>
#include <array>
#include <atomic>
#include <cstdio>
#include <cstdlib>
#include <cstring>
#include <thread>
using Completion=void (^)(AUAudioUnit*,NSError*);
using Hook=void (*)(AudioComponentDescription,AudioComponentInstantiationOptions,Completion);
extern "C" int32_t ravet_auh_test_set_instantiate_hook(Hook);
extern "C" void ravet_auh_test_counts(uint32_t*,uint32_t*,uint32_t*);
static void check(bool c,const char* m){if(!c){std::fprintf(stderr,"FAIL %s\n",m);std::exit(1);}}
static std::atomic<uint32_t> destroyed{0},rendered{0};
static uint32_t failureKind=0;
static std::array<Completion,16> callbacks{};
static uint32_t callbackCount=0;
static AudioComponentDescription description{kAudioUnitType_Effect,'WhF3','Ravt',0,0};
@interface FixtureUnit : AUAudioUnit
@end
@implementation FixtureUnit {
 AUAudioUnitBusArray* _inputs;AUAudioUnitBusArray* _outputs;
}
- (instancetype)initWithComponentDescription:(AudioComponentDescription)d options:(AudioComponentInstantiationOptions)o error:(NSError**)e {
 self=[super initWithComponentDescription:d options:o error:e];if(self){
  AVAudioFormat* format=[[AVAudioFormat alloc]initStandardFormatWithSampleRate:48000 channels:2];
  AUAudioUnitBus* input=[[AUAudioUnitBus alloc]initWithFormat:format error:e];AUAudioUnitBus* output=[[AUAudioUnitBus alloc]initWithFormat:format error:e];
  _inputs=[[AUAudioUnitBusArray alloc]initWithAudioUnit:self busType:AUAudioUnitBusTypeInput busses:@[input]];
  _outputs=[[AUAudioUnitBusArray alloc]initWithAudioUnit:self busType:AUAudioUnitBusTypeOutput busses:@[output]];
 }return self;
}
- (AUAudioUnitBusArray*)inputBusses{return _inputs;}
- (AUAudioUnitBusArray*)outputBusses{return _outputs;}
- (BOOL)isLoadedInProcess{return YES;} // Synthetic mode, not system IPC evidence.
- (BOOL)allocateRenderResourcesAndReturnError:(NSError**)e {
 if(failureKind==1){*e=[NSError errorWithDomain:@"RavetAuHostFixture.prepare" code:INT64_C(0x123456789ab) userInfo:nil];return NO;}
 return [super allocateRenderResourcesAndReturnError:e];
}
- (AUInternalRenderBlock)internalRenderBlock {
 return ^OSStatus(AudioUnitRenderActionFlags* flags,const AudioTimeStamp* time,AUAudioFrameCount frames,NSInteger,AudioBufferList* output,const AURenderEvent*,AURenderPullInputBlock pull){
  ++rendered;if(failureKind==2)return -717;if(failureKind==3)@throw [NSException exceptionWithName:@"RavetAuHostFixture.render" reason:@"injected" userInfo:nil];
  struct {uint32_t count;AudioBuffer buffers[2];} input{output->mNumberBuffers,{}};
  for(uint32_t c=0;c<input.count;++c)input.buffers[c]={1,frames*4,nullptr};
  auto status=pull(flags,time,frames,0,reinterpret_cast<AudioBufferList*>(&input));if(status)return status;
  for(uint32_t c=0;c<input.count;++c){auto* samples=static_cast<float*>(input.buffers[c].mData);for(uint32_t f=0;f<frames;++f)static_cast<float*>(output->mBuffers[c].mData)[f]=samples[f]*2;}
  // Deliberately mutate first pulled memory and request again: host-captured
  // source remains available, not an in-place first pull's modified history.
  if(frames)static_cast<float*>(input.buffers[0].mData)[0]=73;
  status=pull(flags,time,frames,0,reinterpret_cast<AudioBufferList*>(&input));if(status)return status;
  if(frames&&static_cast<float*>(input.buffers[0].mData)[0]!=.25f)return -718;
  if(failureKind==4)output->mBuffers[1].mData=nullptr;
  return 0;
 };
}
- (void)dealloc{check(NSThread.isMainThread,"framework retirement on main");++destroyed;}
@end
static void hook(AudioComponentDescription,AudioComponentInstantiationOptions,Completion completion){check(callbackCount<callbacks.size(),"bounded hook storage");callbacks[callbackCount++]=completion;}
static AUAudioUnit* unit();
static const RavetAuHostEntryV1* reentryEntry=nullptr;static void* reentryCatalogue=nullptr;static bool reentryRan=false;
static void reentrantHook(AudioComponentDescription,AudioComponentInstantiationOptions,Completion completion){
 // Completion invoked synchronously on MAIN while the initiating CONTROL call is
 // still admitted, then ABI reentry from the callback on MAIN and on a worker.
 completion(unit(),nil);completion(unit(),nil);
 RavetAuHostDiagnosticV1 nested;std::memset(&nested,0x5a,sizeof(nested));const auto untouched=nested;
 void* out=reinterpret_cast<void*>(9);RavetAuHostInstanceInfoV1 info{};
 check(reentryEntry->create_async(reentryCatalogue,0,1,&out,&nested)==RAVET_AUH_BUSY&&out==reinterpret_cast<void*>(9),"reentrant create Busy");
 check(reentryEntry->poll(out,&out,&info,&nested)==RAVET_AUH_BUSY,"reentrant poll Busy");
 check(reentryEntry->close_catalogue(reentryCatalogue,&nested)==RAVET_AUH_BUSY,"reentrant catalogue retirement Busy");
 int32_t worker=0;std::thread t([&]{worker=reentryEntry->create_async(reentryCatalogue,0,1,&out,&nested);});t.join();
 check(worker==RAVET_AUH_WRONG_THREAD&&!std::memcmp(&nested,&untouched,sizeof(nested)),"worker reentry WrongThread, diagnostics untouched");
 reentryRan=true;
}
static AUAudioUnit* unit(){NSError* e=nil;auto* result=[[FixtureUnit alloc]initWithComponentDescription:description options:0 error:&e];check(result&&!e,"synthetic SDK unit initialization");return result;}
static void service(){for(unsigned n=0;n<20;++n)[[NSRunLoop mainRunLoop]runUntilDate:[NSDate dateWithTimeIntervalSinceNow:.001]];}
static void counts(uint32_t p,uint32_t i,uint32_t r){uint32_t pp=999,ii=999,rr=999;ravet_auh_test_counts(&pp,&ii,&rr);if(pp!=p||ii!=i||rr!=r)std::fprintf(stderr,"counts %u/%u/%u expected%u/%u/%u\n",pp,ii,rr,p,i,r);check(pp==p&&ii==i&&rr==r,"bounded exact ownership counts");}
int main(){@autoreleasepool {
 [AUAudioUnit registerSubclass:FixtureUnit.class asComponentDescription:description name:@"RAVET: isolated fault fixture" version:1];
 auto* e=ravet_au_host_v1();RavetAuHostDiagnosticV1 d{};RavetAuHostFilterV1 filter{sizeof(filter),description.componentType,description.componentSubType,description.componentManufacturer,4,0};void* catalogue=nullptr;uint32_t count=0;
 check(e->catalogue(&filter,&catalogue,&count,&d)==0&&count==1,"fixture catalogue");check(ravet_auh_test_set_instantiate_hook(hook)==0,"explicit factory hook");
 // Cancelled never-completed work still holds the process pending reservation.
 std::array<void*,16> requests{};
 for(auto& r:requests){check(e->create_async(catalogue,0,1,&r,&d)==0,"sixteen pending");check(e->cancel(r,&d)==0&&d.consumed,"cancel consumes facade only");}
 counts(16,0,16);void* extra=reinterpret_cast<void*>(123);check(e->create_async(catalogue,0,1,&extra,&d)==4&&extra==reinterpret_cast<void*>(123),"cancelled capacity cannot be bypassed");
 // Worker completion retains unit in main delivery. Dropping the caller's own
 // reference occurs on MAIN; framework publication/retirement is main-only.
 for(auto& completion:callbacks){@autoreleasepool{AUAudioUnit* created=unit();std::thread worker([completion,created]{completion(created,nil);completion(created,nil);});worker.join();created=nil;completion=nil;service();}}
 callbackCount=0;service();counts(0,0,0);check(destroyed==16,"late duplicate completion one owner retirement");
 // Original wide NSError survives exactly, separate from OSStatus and local ABI.
 void* pending=nullptr;check(e->create_async(catalogue,0,1,&pending,&d)==0,"failing request");auto completion=callbacks[0];callbacks[0]=nil;callbackCount=0;
 completion(nil,[NSError errorWithDomain:@"RavetAuHostFixture.original" code:INT64_MIN+17 userInfo:nil]);completion=nil;service();
 void* instance=nullptr;RavetAuHostInstanceInfoV1 info{};
 check(e->poll(pending,&instance,&info,&d)==6&&d.consumed&&d.returned==1&&d.has_error&&d.error_code==INT64_MIN+17&&!d.has_os_status,"original NSError exact signed64");counts(0,0,0);
 for(uint32_t failure:{0u,1u,2u,3u,4u}){
  failureKind=failure;check(e->create_async(catalogue,0,1,&pending,&d)==0,"render request");auto c=callbacks[0];callbacks[0]=nil;callbackCount=0;
  c(unit(),nil);c=nil;service();check(e->poll(pending,&instance,&info,&d)==0,"admit synthetic unit");
  RavetAuHostProfileV1 profile{sizeof(profile),16,2,2,48000};auto result=e->prepare(instance,&profile,&d);
  if(failure==1){check(result==6&&d.has_error&&d.error_code==INT64_C(0x123456789ab),"original prepare NSError");check(e->prepare(instance,&profile,&d)==8&&d.attempted==0,"prepare failure sticky no retry");}
  else {
   check(result==0,"fault profile prepared");void* token=nullptr;check(e->begin_audio(instance,&token,&d)==0,"fault profile audio");
   std::array<float,20> in{},out1{},out2{};in.fill(.25f);out1.fill(99);out2.fill(99);
   RavetAuHostInputV1 inputs[2]{{in.data(),20,0},{in.data(),20,0}};RavetAuHostOutputV1 outputs[2]{{out1.data(),20,0},{out2.data(),20,0}};RavetAuHostBlockV1 block{sizeof(block),16,2,2,inputs,outputs,0};
   std::thread worker([&]{result=e->process(token,&block,&d);});worker.join();
   if(failure==0){check(result==0&&out1[15]==.5f&&out2[15]==.5f,"repeated pulls preserve captured input");}
   else {check(result==(failure==2?6:failure==3?7:2),"native fault class");check(d.attempted==1&&d.returned==(failure==3?0u:1u)&&d.exception==(failure==3?1u:0u),"attempted returned exception truthful");if(failure==2)check(d.os_status==-717&&d.has_os_status,"original render status retained");check(out1[0]==0&&out2[15]==0&&out1[16]==99&&out2[16]==99,"whole safe failure silence preserves tails");auto before=rendered.load();check(e->process(token,&block,&d)==8&&rendered==before&&d.attempted==0,"sticky no render replay");}
   check(e->end_audio(token,&d)==0,"fault joined end");
  }
  check(e->destroy(instance,&d)==0&&d.consumed,"fault owner balanced destruction");counts(0,0,0);
 }
 // Completion/cancellation and unrelated facade Drop do not release/reuse
 // any protected slot while an AUDIO sibling is active.
 failureKind=0;
 auto makeReady=[&]()->void*{void* r=nullptr;check(e->create_async(catalogue,0,1,&r,&d)==0,"deferred create");auto c=callbacks[0];callbacks[0]=nil;callbackCount=0;c(unit(),nil);c=nil;service();void* owner=nullptr;check(e->poll(r,&owner,&info,&d)==0,"deferred ready");return owner;};
 void* active=makeReady();void* stopped=makeReady();
 check(e->create_async(catalogue,0,1,&pending,&d)==0,"late completion reserved before audio");
 auto late=callbacks[0];callbacks[0]=nil;callbackCount=0;AUAudioUnit* lateUnit=unit();
 RavetAuHostProfileV1 deferredProfile{sizeof(deferredProfile),16,2,2,48000};check(e->prepare(active,&deferredProfile,&d)==0,"deferred prepare");void* token=nullptr;check(e->begin_audio(active,&token,&d)==0,"deferred begin");
 check(e->cancel(pending,&d)==0&&d.consumed,"live sibling cancel facade");
 check(e->destroy(stopped,&d)==0&&d.consumed,"live sibling destroy facade");
 check(e->close_catalogue(catalogue,&d)==0&&d.consumed,"live sibling catalogue facade");
 std::thread lateWorker([late,lateUnit]{late(lateUnit,nil);});lateWorker.join();late=nil;lateUnit=nil;service();counts(1,2,1);
 check(e->prepare(active,&deferredProfile,&d)==3,"deferred geometry remains Busy");
 check(e->end_audio(token,&d)==0,"final joined boundary retires deferred foreign units");counts(0,1,0);
 check(e->destroy(active,&d)==0,"active unit after deferred retirement");counts(0,0,0);
 check(e->catalogue(&filter,&catalogue,&count,&d)==0,"fresh catalogue after deferred slot reclamation");
 // Callback reentry: an SDK that completes synchronously inside the initiating
 // MAIN call, and re-enters the ABI from that callback, sees Busy/WrongThread
 // before diagnostics are written; delivery still arrives once through MAIN.
 reentryEntry=e;reentryCatalogue=catalogue;check(ravet_auh_test_set_instantiate_hook(reentrantHook)==0,"reentrant factory hook");
 // Pools bound fixture-created autoreleased units, so retirement is observable.
 @autoreleasepool{check(e->create_async(catalogue,0,1,&pending,&d)==0&&reentryRan,"synchronous completion inside initiation");}
 check(ravet_auh_test_set_instantiate_hook(hook)==0,"restore bounded hook");counts(1,0,1);
 @autoreleasepool{service();check(e->poll(pending,&instance,&info,&d)==0&&d.consumed,"synchronous reentrant completion delivered once");}counts(0,1,0);
 {auto before=destroyed.load();check(e->destroy(instance,&d)==0&&d.consumed&&destroyed==before+1,"reentry instance retired on main");}
 // Success together with an NSError is a foreign failure; the delivered unit is
 // retired exactly once on MAIN, never admitted.
 {auto before=destroyed.load();check(e->create_async(catalogue,0,1,&pending,&d)==0,"success+error request");auto c=callbacks[0];callbacks[0]=nil;callbackCount=0;
  @autoreleasepool{AUAudioUnit* both=unit();std::thread w([c,both]{c(both,[NSError errorWithDomain:@"RavetAuHostFixture.both" code:-42 userInfo:nil]);});w.join();both=nil;}c=nil;service();
  instance=reinterpret_cast<void*>(7);check(e->poll(pending,&instance,&info,&d)==6&&d.consumed&&d.has_error&&d.error_code==-42&&instance==reinterpret_cast<void*>(7),"success+error refused without admission");
  service();counts(0,0,0);check(destroyed==before+1,"success+error unit retired once");}
 // Cancellation after delivered completion but before poll retires on MAIN.
 {auto before=destroyed.load();check(e->create_async(catalogue,0,1,&pending,&d)==0,"cancel-after-ready request");auto c=callbacks[0];callbacks[0]=nil;callbackCount=0;
  @autoreleasepool{AUAudioUnit* ready=unit();std::thread w([c,ready]{c(ready,nil);});w.join();ready=nil;}c=nil;service();counts(1,0,1);
  check(e->cancel(pending,&d)==0&&d.consumed,"cancel after delivery");counts(0,0,0);check(destroyed==before+1,"cancel-after-ready unit retired once");
  check(e->poll(pending,&instance,&info,&d)==RAVET_AUH_INVALID,"cancelled facade cannot be polled");}
 // Admission/reservation includes ready instances. A pending request reserves
 // its future slot before callback completion, not after allocating a facade.
 failureKind=0;std::array<void*,256> live{};
 for(auto& v:live){check(e->create_async(catalogue,0,1,&pending,&d)==0,"bounded instance creation");auto c=callbacks[0];callbacks[0]=nil;callbackCount=0;c(unit(),nil);c=nil;service();check(e->poll(pending,&v,&info,&d)==0,"bounded instance completion");}
 counts(0,256,0);check(e->create_async(catalogue,0,1,&pending,&d)==4,"instance capacity pre-creation");
 for(auto v:live)check(e->destroy(v,&d)==0,"instance capacity reclamation");counts(0,0,0);
 check(e->close_catalogue(catalogue,&d)==0,"fixture catalogue retirement");
 std::puts("PASS SDK fault fixture pending16/instances256, worker completion/cancel/duplicate, wide NSError, completion reentry, success+error, cancel-after-ready, prepare/render throw/corruption, repeated pulls and retirement");
}}
