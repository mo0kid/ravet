#import "RavetAuHost.h"
#import <Foundation/Foundation.h>
#include <array>
#include <cmath>
#include <cstdio>
#include <thread>
// No registerSubclass or native DSP link: this process must discover/load the
// separately registered exact public extension through Apple's async factory.
int main(){@autoreleasepool {
 auto* e=ravet_au_host_v1();RavetAuHostDiagnosticV1 d{};void* catalogue=nullptr;uint32_t count=0;
 RavetAuHostFilterV1 f{sizeof(f),uint32_t('aufx'),uint32_t('RvG3'),uint32_t('Ravt'),4,0};
 int32_t status=0;NSDate* discoveryDeadline=[NSDate dateWithTimeIntervalSinceNow:10];
 do {
  status=e->catalogue(&f,&catalogue,&count,&d);if(status||count)break;
  e->close_catalogue(catalogue,&d);catalogue=nullptr;
  [[NSRunLoop mainRunLoop]runUntilDate:[NSDate dateWithTimeIntervalSinceNow:.01]];
 }while(discoveryDeadline.timeIntervalSinceNow>0);
 if(status||count!=1){std::printf("UNAVAILABLE exact public Gain count%u status%d OSStatus%d returned%u\n",count,status,d.os_status,d.returned);if(catalogue)e->close_catalogue(catalogue,&d);return 20;}
 for(uint32_t channels:{1u,2u}){
  void* request=nullptr;status=e->create_async(catalogue,0,RAVET_AUH_OUT_OF_PROCESS,&request,&d);
  if(status){std::printf("REFUSED initiation status%d exception%u attempted%u returned%u\n",status,d.exception,d.attempted,d.returned);e->close_catalogue(catalogue,&d);return 21;}
  void* instance=nullptr;RavetAuHostInstanceInfoV1 info{};NSDate* deadline=[NSDate dateWithTimeIntervalSinceNow:30];
  do {
   status=e->poll(request,&instance,&info,&d);
   if(status==1){if(deadline.timeIntervalSinceNow<=0){e->cancel(request,&d);e->close_catalogue(catalogue,&d);std::puts("UNAVAILABLE async completion deadline; cancelled facade/underlying reservation retained until completion or process exit");return 22;}
    [[NSRunLoop mainRunLoop]runUntilDate:[NSDate dateWithTimeIntervalSinceNow:.002]];}
  }while(status==1);
  if(status){std::printf("REFUSED async status%d consumed%u NSErrorPresence%u code%lld domain%.*s attempted%u returned%u exception%u\n",status,d.consumed,d.has_error,(long long)d.error_code,int(d.domain_length<=256?d.domain_length:0),d.domain,d.attempted,d.returned,d.exception);if(!d.consumed)e->cancel(request,&d);e->close_catalogue(catalogue,&d);return 23;}
  if(info.observed_in_process!=0){std::puts("FAIL observed in-process after required out-of-process admission");return 24;}
  std::printf("OBSERVED actual out-of-process public Gain channels%u\n",channels);
  RavetAuHostProfileV1 p{sizeof(p),512,channels,channels,48000};status=e->prepare(instance,&p,&d);
  if(status){std::printf("REFUSED prepare status%d NSErrorPresence%u code%lld domain%.*s\n",status,d.has_error,(long long)d.error_code,int(d.domain_length<=256?d.domain_length:0),d.domain);e->destroy(instance,&d);e->close_catalogue(catalogue,&d);return 25;}
  void* token=nullptr;if(e->begin_audio(instance,&token,&d)){e->destroy(instance,&d);e->close_catalogue(catalogue,&d);return 26;}
  std::array<float,520> l{},r{},ol{},orr{};for(unsigned n=0;n<520;++n){l[n]=.125f*std::sin(.017f*n);r[n]=-.25f*std::cos(.031f*n);}ol.fill(99);orr.fill(99);
  RavetAuHostInputV1 input[2]{{l.data(),520,0},{r.data(),520,0}};RavetAuHostOutputV1 output[2]{{ol.data(),520,0},{orr.data(),520,0}};RavetAuHostBlockV1 b{sizeof(b),512,channels,channels,input,output,0};
  std::thread worker([&]{status=e->process(token,&b,&d);});worker.join();
  float maximumError=0;for(unsigned n=0;n<512;++n){maximumError=std::max(maximumError,std::abs(ol[n]-l[n]));if(channels==2)maximumError=std::max(maximumError,std::abs(orr[n]-r[n]));}
  bool okay=status==0&&maximumError<1e-6&&ol[512]==99&&orr[512]==99;
  std::printf("RENDER status%d originalOS%d attempted%u returned%u maximumError%.9g tails%u\n",status,d.os_status,d.attempted,d.returned,maximumError,uint32_t(ol[512]==99&&orr[512]==99));
  auto ended=e->end_audio(token,&d);auto stopped=e->stop(instance,&d);auto destroyed=e->destroy(instance,&d);
  std::printf("RETIRE end%d stop%d destroy%d consumed%u\n",ended,stopped,destroyed,d.consumed);
  if(!okay||ended||stopped||destroyed){e->close_catalogue(catalogue,&d);return 27;}
 }
 status=e->close_catalogue(catalogue,&d);if(status)return 28;
 std::puts("PASS actual observed out-of-process async Gain mono/stereo public renderBlock, independent unity samples/tails and joined MAIN retirement");return 0;
}}
