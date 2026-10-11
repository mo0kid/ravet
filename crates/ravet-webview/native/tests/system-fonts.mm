#import "../SystemFonts.mm"
#include <cassert>
#include <cstdio>
#include <thread>
int main(){unsigned wrong=0;std::thread worker([&]{@autoreleasepool {void* out=(void*)123;size_t count=999;wrong=ravet_fonts_create(&out,&count);assert(out==(void*)123&&count==999);}});worker.join();assert(wrong==4);
 __weak RavetFontOwner* weakOwner=nil;__weak RavetFontResource* weakResource=nil;
 @autoreleasepool {
  void* raw=nullptr;size_t count=0;assert(ravet_fonts_create(&raw,&count)==0&&count<=4096);weakOwner=(__bridge RavetFontOwner*)raw;
  size_t chosen=SIZE_MAX;for(size_t i=0;i<count;++i){RavetFontDescription d{};assert(ravet_fonts_descriptor(raw,i,&d)==0);if(strcmp(d.postscript,"ArialMT")==0)chosen=i;}assert(chosen!=SIZE_MAX);
  void* resource=(void*)123;size_t tables=999,axes=999;assert(ravet_fonts_load(raw,count,&resource,&tables,&axes)==1);assert(resource==(void*)123&&tables==999&&axes==999);
  RavetFontOwner* owner=(__bridge RavetFontOwner*)raw;owner.busy=YES;assert(ravet_fonts_load(raw,chosen,&resource,&tables,&axes)==5);assert(owner.busy);owner.busy=NO;
  assert(ravet_fonts_load(raw,chosen,&resource,&tables,&axes)==0);weakResource=(__bridge RavetFontResource*)resource;assert(weakResource!=nil);
  uint32_t tag=999;size_t length=999;assert(ravet_fonts_table(resource,tables,&tag,&length)==1&&tag==999&&length==999);
  assert(ravet_fonts_table(resource,0,&tag,&length)==0);std::vector<uint8_t> copied(length,0);assert(ravet_fonts_copy_table(resource,0,copied.data(),length-1)==2);assert(std::all_of(copied.begin(),copied.end(),[](auto b){return b==0;}));assert(ravet_fonts_copy_table(resource,0,copied.data(),length)==0);
  ravet_fonts_resource_destroy(resource);ravet_fonts_destroy(raw);
 }
 assert(weakOwner==nil&&weakResource==nil);
 @autoreleasepool {
  const auto boolean=[&](id value,bool expected,bool validExpected){CTFontDescriptorRef d=CTFontDescriptorCreateWithAttributes((__bridge CFDictionaryRef)@{(__bridge NSString*)kCTFontEnabledAttribute:value});bool valid=true;bool actual=readBooleanAttribute(d,kCTFontEnabledAttribute,false,valid);CFRelease(d);assert(actual==expected&&valid==validExpected);};
  boolean(@YES,true,true);boolean(@NO,false,true);boolean(@1,true,true);boolean(@0,false,true);boolean(@2,false,false);boolean(@(NAN),false,false);boolean(@"1",false,false);
  RavetFontDescription out{};memset(out.family,'x',sizeof(out.family));CTFontDescriptorRef bad=CTFontDescriptorCreateWithAttributes((__bridge CFDictionaryRef)@{(__bridge NSString*)kCTFontFamilyNameAttribute:@"a\0b",(__bridge NSString*)kCTFontNameAttribute:@"A"});assert(describe(bad,out)==6);CFRelease(bad);
 }
 puts("Native wrong-thread/output-last/busy/owned-resource/table-copy/Boolean/name7 groups PASS");return 0;
}
