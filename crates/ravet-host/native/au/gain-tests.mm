#import "GainAudioUnit.h"
#import "RavetAuHost.h"
#import <Foundation/Foundation.h>
#include <array>
#include <atomic>
#include <cmath>
#include <cstdio>
#include <cstdlib>
#include <thread>
#include <cstring>
#include <new>
#include <vector>
static thread_local bool countAllocations=false;
static thread_local uint32_t allocations=0;
void* operator new(std::size_t n){if(countAllocations)++allocations;if(auto* p=std::malloc(n?n:1))return p;throw std::bad_alloc();}
void* operator new[](std::size_t n){return ::operator new(n);}
void operator delete(void* p)noexcept{std::free(p);}
void operator delete[](void* p)noexcept{std::free(p);}
static void check(bool c,const char* m){if(!c){std::fprintf(stderr,"FAIL: %s\n",m);std::exit(1);}}
static __weak AUAudioUnit* latest;
@interface HostedGain : RavetGainAudioUnit
@end
@implementation HostedGain
- (instancetype)initWithComponentDescription:(AudioComponentDescription)d options:(AudioComponentInstantiationOptions)o error:(NSError**)e {
 self=[super initWithComponentDescription:d options:o error:e];if(self)latest=self;return self;
}
@end
static void* load(const RavetAuHostEntryV1* e,void* catalogue,uint32_t mode,RavetAuHostDiagnosticV1& d){
 void* pending=nullptr;check(e->create_async(catalogue,0,mode,&pending,&d)==0&&pending,"async initiation");
 const auto deadline=[NSDate dateWithTimeIntervalSinceNow:10];
 for(;;){void* instance=nullptr;RavetAuHostInstanceInfoV1 info{};auto status=e->poll(pending,&instance,&info,&d);
  if(status==RAVET_AUH_PENDING){check(deadline.timeIntervalSinceNow>0,"async deadline");[[NSRunLoop mainRunLoop] runUntilDate:[NSDate dateWithTimeIntervalSinceNow:.001]];continue;}
  if(status)std::fprintf(stderr,"load status%d NSError%lld domain%.*s returned%u consumed%u\n",status,(long long)d.error_code,int(d.domain_length),d.domain,d.returned,d.consumed);
  if(mode==RAVET_AUH_OUT_OF_PROCESS){check(status==RAVET_AUH_MODE&&d.consumed==1&&!instance,"observed process mode mismatch refused");return nullptr;}
  check(status==0&&d.consumed&&instance&&info.observed_in_process==1,"actual observed in-process mode");return instance;
 }
}
int main(){@autoreleasepool {
 auto* e=ravet_au_host_v1();check(e&&e->version==1,"entry");
 AudioComponentDescription desc{kAudioUnitType_Effect,'WhG3','Ravt',0,0};
 [AUAudioUnit registerSubclass:HostedGain.class asComponentDescription:desc name:@"RAVET: isolated hosted Gain" version:1];
 RavetAuHostFilterV1 filter{sizeof(filter),desc.componentType,desc.componentSubType,desc.componentManufacturer,4,0};
 RavetAuHostDiagnosticV1 d{};void* catalogue=nullptr;uint32_t count=999;
 check(e->catalogue(&filter,&catalogue,&count,&d)==0&&count==1,"exact catalogue");
 RavetAuHostComponentV1 component{};check(e->component(catalogue,0,&component,&d)==0&&component.type==desc.componentType&&component.subtype==desc.componentSubType&&component.manufacturer==desc.componentManufacturer,"exact description");
 check((component.flags&kAudioComponentFlag_IsV3AudioUnit)!=0,"actual V3 registration metadata");
 for(uint32_t channels:{1u,2u}){
  void* instance=load(e,catalogue,RAVET_AUH_IN_PROCESS,d);
  auto unit=latest;check(unit!=nil,"actual registered class");[unit.parameterTree parameterWithAddress:1].value=2;
  RavetAuHostProfileV1 p{sizeof(p),512,channels,channels,48000};check(e->prepare(instance,&p,&d)==0,"prepare");
  void* token=nullptr;check(e->begin_audio(instance,&token,&d)==0,"audio handoff");
  check(e->prepare(instance,&p,&d)==RAVET_AUH_BUSY,"main geometry excluded during audio");
  std::array<float,520> l{},r{},ol{},orr{};l.fill(.25f);r.fill(-.125f);ol.fill(99);orr.fill(99);
  RavetAuHostInputV1 input[2]{{l.data(),520,0},{r.data(),520,0}};RavetAuHostOutputV1 output[2]{{ol.data(),520,0},{orr.data(),520,0}};
  RavetAuHostBlockV1 b{sizeof(b),512,channels,channels,input,output,1000};
  int32_t result=999;RavetAuHostDiagnosticV1 audioDiag{};
  std::thread worker([&]{result=e->process(token,&b,&audioDiag);});worker.join();
  if(result)std::fprintf(stderr,"render status%d original%d exception%u\n",result,audioDiag.os_status,audioDiag.exception);
  check(result==0&&audioDiag.attempted==1&&audioDiag.returned==1&&audioDiag.has_os_status&&audioDiag.os_status==0,"public renderBlock returned");
  check(std::abs(ol[511]-.5f)<1e-6f&&(channels==1||std::abs(orr[511]+.25f)<1e-6f),"independent doubled Gain");check(ol[512]==99&&orr[512]==99,"backing tails");
  // Actual public render observer attempts same-token reentry and waits while
  // MAIN tries a geometry mutation. Neither may alter active outer ownership.
  std::atomic<bool> entered{false},release{false};std::atomic<int32_t> nested{999};
  RavetAuHostDiagnosticV1 nestedDiag{};std::memset(&nestedDiag,0x5a,sizeof(nestedDiag));const auto untouched=nestedDiag;
  auto* enteredPtr=&entered;auto* releasePtr=&release;auto* nestedPtr=&nested;auto* nestedDiagPtr=&nestedDiag;auto* blockPtr=&b;
  auto observer=[unit tokenByAddingRenderObserver:^(AudioUnitRenderActionFlags action,const AudioTimeStamp*,AUAudioFrameCount,NSInteger){
   if(action&kAudioUnitRenderAction_PostRender)return;
   nestedPtr->store(e->process(token,blockPtr,nestedDiagPtr),std::memory_order_relaxed);
   enteredPtr->store(true,std::memory_order_release);while(!releasePtr->load(std::memory_order_acquire))std::this_thread::yield();
  }];
  std::thread concurrent([&]{result=e->process(token,&b,&audioDiag);});
  while(!entered.load(std::memory_order_acquire))std::this_thread::yield();
  check(e->prepare(instance,&p,&d)==RAVET_AUH_BUSY,"live render geometry mutation rejected");
  check(e->end_audio(token,&d)==RAVET_AUH_BUSY,"unjoined active render cannot end");
  release.store(true,std::memory_order_release);concurrent.join();
  check(result==0&&nested==RAVET_AUH_BUSY&&!std::memcmp(&nestedDiag,&untouched,sizeof(nestedDiag)),"nested process Busy before diagnostics/output/DSP");
  [unit removeRenderObserver:observer];
  std::thread measured([&]{allocations=0;countAllocations=true;
   for(unsigned n=0;n<2000;++n){if(e->process(token,&b,&audioDiag)!=0){countAllocations=false;std::abort();}}
   countAllocations=false;result=static_cast<int32_t>(allocations);
  });measured.join();check(result==0,"2000 actual prepared calls no ordinary C++ new");
  // Whole late shape rejection has no DSP call and preserves every output.
  auto prior=ol;output[channels-1].capacity=1;check(e->process(token,&b,&audioDiag)==RAVET_AUH_INVALID&&audioDiag.attempted==0&&ol==prior,"late output preflight");output[channels-1].capacity=520;
  // Capture both sources before copying cross-aliased host outputs.
  output[0].samples=channels==1?l.data():r.data();if(channels==2)output[1].samples=l.data();
  check(e->process(token,&b,&audioDiag)==0,"input output alias through scratch");
  check(std::abs((channels==1?l:r)[511]-.5f)<1e-6f&&(channels==1||std::abs(l[511]+.25f)<1e-6f),"cross alias independent sources");
  b.frames=0;check(e->process(token,&b,&audioDiag)==0,"zero frames");
  check(e->end_audio(token,&d)==0,"joined retirement");check(e->stop(instance,&d)==0,"stopped resource retirement");
  check(e->prepare(instance,&p,&d)==0,"reprepare after clean stop");check(e->stop(instance,&d)==0,"stop before maximum profile");
  auto maximum=p;maximum.maximum_frames=65536;auto excessive=maximum;excessive.maximum_frames=65537;
  check(e->prepare(instance,&excessive,&d)==RAVET_AUH_INVALID&&d.attempted==0,"maximum overflow before foreign preparation");
  check(e->prepare(instance,&maximum,&d)==0,"actual maximum65536 preparation");
  std::vector<float> maxInput1(65544,.25f),maxInput2(65544,-.125f),maxOutput1(65544,99),maxOutput2(65544,99);
  RavetAuHostInputV1 maxInputs[2]{{maxInput1.data(),65544,0},{maxInput2.data(),65544,0}};RavetAuHostOutputV1 maxOutputs[2]{{maxOutput1.data(),65544,0},{maxOutput2.data(),65544,0}};
  RavetAuHostBlockV1 maxBlock{sizeof(maxBlock),65536,channels,channels,maxInputs,maxOutputs,0};
  check(e->begin_audio(instance,&token,&d)==0,"maximum AUDIO admission");
  std::thread maxWorker([&]{result=e->process(token,&maxBlock,&audioDiag);});maxWorker.join();
  check(result==0&&std::abs(maxOutput1[65535]-.5f)<1e-6&&(channels==1||std::abs(maxOutput2[65535]+.25f)<1e-6)&&maxOutput1[65536]==99&&maxOutput2[65536]==99,"actual maximum audio and retained tails");
  check(e->end_audio(token,&d)==0,"maximum joined retirement");
  check(e->destroy(instance,&d)==0&&d.consumed,"instance ownership consumed");unit=nil;
 }
 // The test-only in-process subclass cannot substantiate actual IPC. A require
 // policy must reject Apple's observed fallback, never infer mode from options.
 (void)load(e,catalogue,RAVET_AUH_OUT_OF_PROCESS,d);
 check(e->close_catalogue(catalogue,&d)==0&&d.consumed,"catalogue retirement");
 std::puts("PASS actual async AUv3 Gain mono/stereo public renderBlock, aliases/tails, observed mode refusal");
}}
