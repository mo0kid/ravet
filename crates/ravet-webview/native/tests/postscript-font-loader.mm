#import "../SystemFonts.mm"
#include <cassert>
#include <cstdio>
#include <initializer_list>
#include <stdexcept>
#define CHECK(x) do {if(!(x))throw std::runtime_error(#x);}while(0)
class ProcessFont {
 CFURLRef url=nullptr;
 public: explicit ProcessFont(NSString* path) {
  url=(CFURLRef)CFBridgingRetain([NSURL fileURLWithPath:path]);CFErrorRef error=nullptr;
  if(!CTFontManagerRegisterFontsForURL(url,kCTFontManagerScopeProcess,&error)) {
   if(error)CFRelease(error);CFRelease(url);url=nullptr;throw std::runtime_error("first-party process font registration refused");
  }
 }
 ~ProcessFont() {if(url) {CFErrorRef error=nullptr;const bool ok=CTFontManagerUnregisterFontsForURL(url,kCTFontManagerScopeProcess,&error);printf("first-party process unregister=%d\n",ok);if(error)CFRelease(error);CFRelease(url);}}
 ProcessFont(const ProcessFont&)=delete;ProcessFont&operator=(const ProcessFont&)=delete;
};
int main(int argc,char**argv) {try { @autoreleasepool {
 if(argc!=3)return 1;unsigned groups=0;
 for(int i=1;i<argc;++i) {
  NSData* bytes=[NSData dataWithContentsOfFile:[NSString stringWithUTF8String:argv[i]]];if(!bytes)return 2;
  CFArrayRef fromData=CTFontManagerCreateFontDescriptorsFromData((__bridge CFDataRef)bytes);if(!fromData||CFArrayGetCount(fromData)!=1)return 3;
  CTFontDescriptorRef dataDescriptor=(CTFontDescriptorRef)CFArrayGetValueAtIndex(fromData,0);
  CFStringRef expectedName=(CFStringRef)CTFontDescriptorCopyAttribute(dataDescriptor,kCTFontNameAttribute);if(!expectedName)return 4;
  ProcessFont registration([NSString stringWithUTF8String:argv[i]]);
  CTFontCollectionRef collection=CTFontCollectionCreateFromAvailableFonts((__bridge CFDictionaryRef)@{(__bridge NSString*)kCTFontCollectionRemoveDuplicatesOption:@YES,(__bridge NSString*)kCTFontCollectionIncludeDisabledFontsOption:@NO,(__bridge NSString*)kCTFontCollectionDisallowAutoActivationOption:@YES});
  CFArrayRef sources=CTFontCollectionCreateMatchingFontDescriptors(collection);CFRelease(collection);
  CTFontDescriptorRef original=nullptr;
  for(CFIndex n=0;n<CFArrayGetCount(sources);++n) {
   CTFontDescriptorRef d=(CTFontDescriptorRef)CFArrayGetValueAtIndex(sources,n);
   CFTypeRef name=CTFontDescriptorCopyAttribute(d,kCTFontNameAttribute);
   if(name&&CFEqual(name,expectedName))original=d;if(name)CFRelease(name);
  }
  CFRelease(expectedName);CFRelease(fromData);CHECK(original);
  for(double user:{100.,400.,650.,900.}) {
   if(i==1&&user!=100.)continue;
   CTFontDescriptorRef descriptor=i==1?(CTFontDescriptorRef)CFRetain(original):CTFontDescriptorCreateCopyWithAttributes(original,(__bridge CFDictionaryRef)@{(__bridge NSString*)kCTFontVariationAttribute:@{@(0x77676874):@(user)}});
   RavetFontOwner* owner=[RavetFontOwner new];owner.descriptors=@[(__bridge id)descriptor];CFRelease(descriptor);
   void* raw=(__bridge_retained void*)owner;
#if RAVET_POSTSCRIPT_FONTS
   __weak RavetFontResource* weak=nil;
#endif
   void* resource=(void*)123;size_t tables=999,axes=999;
   RavetFontDescription meta{};const auto ms=describe((__bridge CTFontDescriptorRef)owner.descriptors[0],meta);
   printf("meta status=%u format=%u availability=%u weight=%.17g width=%.17g slant=%.17g\n",ms,meta.format,meta.availability,meta.weight,meta.width,meta.slant);
   CTFontRef inspect=CTFontCreateWithFontDescriptorAndOptions((__bridge CTFontDescriptorRef)owner.descriptors[0],12,nullptr,kCTFontOptionsPreventAutoActivation);
   printf("native format=%u identity=%d effects=%d\n",meta.format,inspect?identity((__bridge CTFontDescriptorRef)owner.descriptors[0],inspect):0,effectsSupported((__bridge CTFontDescriptorRef)owner.descriptors[0]));
   if(inspect)CFRelease(inspect);
   const auto status=ravet_fonts_load(raw,0,&resource,&tables,&axes);
#if RAVET_POSTSCRIPT_FONTS
   printf("authored\t%s\t%.0f\tstatus=%u tables=%zu axes=%zu\n",argv[i],i==1?0:user,status,tables,axes);
   if(meta.format!=kCTFontFormatOpenTypePostScript) {
    CHECK(status==6&&resource==(void*)123&&tables==999&&axes==999);
    ravet_fonts_destroy(raw);++groups;continue;
   }
   CHECK(status==0&&resource!=(void*)123&&tables>0&&axes==(i==1?0u:1u));weak=(__bridge RavetFontResource*)resource;
   bool found=false;
   for(size_t t=0;t<tables;++t) {
    uint32_t tag=0;size_t length=0;CHECK(ravet_fonts_table(resource,t,&tag,&length)==0);
    if(tag==(i==1?0x43464620u:0x43464632u)) {
     std::vector<uint8_t> copy(length,0);CHECK(length>0);
     CHECK(ravet_fonts_copy_table(resource,t,copy.data(),length-1)==2);
     CHECK(std::all_of(copy.begin(),copy.end(),[](auto b){return b==0;}));
     CHECK(ravet_fonts_copy_table(resource,t,copy.data(),length)==0);
     CTFontRef native=CTFontCreateWithFontDescriptorAndOptions((__bridge CTFontDescriptorRef)owner.descriptors[0],12,nullptr,kCTFontOptionsPreventAutoActivation);
     CFDataRef originalTable=CTFontCopyTable(native,tag,kCTFontTableOptionNoOptions);CHECK(originalTable);
     CHECK(CFDataGetLength(originalTable)==(CFIndex)copy.size()&&memcmp(CFDataGetBytePtr(originalTable),copy.data(),copy.size())==0);
     CFRelease(originalTable);CFRelease(native);found=true;
    }
   }CHECK(found);
   if(axes) {RavetFontAxis axis{};CHECK(ravet_fonts_axis(resource,0,&axis)==0);CHECK(axis.tag==0x77676874u&&axis.minimum==100&&axis.maximum==900&&axis.defaultValue==400&&axis.value==user);}
   ravet_fonts_resource_destroy(resource);CHECK(weak==nil);
#else
   CHECK(status==6&&resource==(void*)123&&tables==999&&axes==999);
#endif
   owner.busy=YES;CHECK(ravet_fonts_load(raw,0,&resource,&tables,&axes)==5);owner.busy=NO;
   ravet_fonts_destroy(raw);++groups;
  }CFRelease(sources);
 }
 printf("%u actual authored native PostScript-loader profiles PASS\n",groups);return 0;
}}catch(const std::exception& e){fprintf(stderr,"%s\n",e.what());return 90;} }
