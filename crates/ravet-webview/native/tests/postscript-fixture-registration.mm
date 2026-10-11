#import <Foundation/Foundation.h>
#import <CoreText/CoreText.h>
extern "C" void* ravet_cff_fixture_register(const char* path) {
 @autoreleasepool {
  if(!path)return nullptr;NSString* name=[NSString stringWithUTF8String:path];if(!name)return nullptr;
  CFURLRef url=(CFURLRef)CFBridgingRetain([NSURL fileURLWithPath:name]);CFErrorRef error=nullptr;
  if(!CTFontManagerRegisterFontsForURL(url,kCTFontManagerScopeProcess,&error)) {
   if(error)CFRelease(error);CFRelease(url);return nullptr;
  }return (void*)url;
 }
}
extern "C" bool ravet_cff_fixture_unregister(void* raw) {
 if(!raw)return false;CFErrorRef error=nullptr;
 const bool result=CTFontManagerUnregisterFontsForURL((CFURLRef)raw,kCTFontManagerScopeProcess,&error);
 if(error)CFRelease(error);CFRelease((CFURLRef)raw);return result;
}
