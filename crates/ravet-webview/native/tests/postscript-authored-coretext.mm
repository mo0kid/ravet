#import <Cocoa/Cocoa.h>
#import <CoreText/CoreText.h>
#include <cstdio>
#include <initializer_list>

static void path(void*,const CGPathElement* e) {
 const char* code=nullptr;size_t count=0;
 switch(e->type){case kCGPathElementMoveToPoint:code="M";count=1;break;case kCGPathElementAddLineToPoint:code="L";count=1;break;case kCGPathElementAddQuadCurveToPoint:code="Q";count=2;break;case kCGPathElementAddCurveToPoint:code="C";count=3;break;case kCGPathElementCloseSubpath:code="Z";break;}
 printf("%s",code);for(size_t i=0;i<count;++i)printf("\t%.17g\t%.17g",(double)e->points[i].x,(double)e->points[i].y);puts("");
}
int main(int argc,char**argv) { @autoreleasepool {
 if(argc<2)return 1;
 for(int i=1;i<argc;++i) {
  NSString* name=[NSString stringWithUTF8String:argv[i]];NSData* data=[NSData dataWithContentsOfFile:name];if(!data)return 2;
  CFArrayRef descriptors=CTFontManagerCreateFontDescriptorsFromData((__bridge CFDataRef)data);if(!descriptors||CFArrayGetCount(descriptors)!=1)return 3;
  CTFontDescriptorRef descriptor=(CTFontDescriptorRef)CFArrayGetValueAtIndex(descriptors,0);
  CTFontRef original=CTFontCreateWithFontDescriptorAndOptions(descriptor,1000,nullptr,kCTFontOptionsPreventAutoActivation);if(!original)return 4;
  CFArrayRef axes=CTFontCopyVariationAxes(original);const bool variable=axes&&CFArrayGetCount(axes)>0;if(axes)CFRelease(axes);
  for(double user: {100.,400.,650.,900.}) {
   if(!variable&&user!=100.)continue;
   CTFontRef font=original;
   if(variable){CTFontDescriptorRef varied=CTFontDescriptorCreateCopyWithAttributes(descriptor,(__bridge CFDictionaryRef)@{(__bridge NSString*)kCTFontVariationAttribute:@{@(0x77676874):@(user)}});font=CTFontCreateWithFontDescriptorAndOptions(varied,1000,nullptr,kCTFontOptionsPreventAutoActivation);CFRelease(varied);if(!font)return 5;}
   UniChar ch='A';CGGlyph glyph=0;if(!CTFontGetGlyphsForCharacters(font,&ch,&glyph,1)||glyph!=2)return 6;
   CGSize advance{};CTFontGetAdvancesForGlyphs(font,kCTFontOrientationHorizontal,&glyph,&advance,1);
   printf("glyph\tCT\t%s\t%.0f\t%u\t%.17g\t0\n",argv[i],variable?user:0,(unsigned)glyph,(double)advance.width);
   CGPathRef outline=CTFontCreatePathForGlyph(font,glyph,nullptr);if(!outline)return 7;CGPathApply(outline,nullptr,path);CFRelease(outline);puts("end");
   if(variable)CFRelease(font);
  }
  CFRelease(original);CFRelease(descriptors);
 }
 return 0;
}}
