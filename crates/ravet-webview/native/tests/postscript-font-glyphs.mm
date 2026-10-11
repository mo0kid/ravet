#define RAVET_POSTSCRIPT_FONTS 1
#import "../SystemFonts.mm"
#include <cstdio>
int main(){ @autoreleasepool {
 void* raw=nullptr;size_t count=0;if(ravet_fonts_create(&raw,&count))return 1;
 RavetFontOwner* owner=(__bridge RavetFontOwner*)raw;
 for(NSUInteger i=0;i<count;++i){
  CTFontDescriptorRef d=(__bridge CTFontDescriptorRef)owner.descriptors[i];RavetFontDescription info{};
  if(describe(d,info)||(strcmp(info.family,"Academico")&&strcmp(info.family,"D-DIN")&&strcmp(info.family,"Bagnard")))continue;
  CTFontRef small=CTFontCreateWithFontDescriptorAndOptions(d,12,nullptr,kCTFontOptionsPreventAutoActivation);if(!small)return 2;
  unsigned upem=CTFontGetUnitsPerEm(small);CFRelease(small);
  CTFontRef font=CTFontCreateWithFontDescriptorAndOptions(d,(CGFloat)upem,nullptr,kCTFontOptionsPreventAutoActivation);if(!font)return 3;
  for(UniChar ch: {UniChar('A'),UniChar('a'),UniChar('g'),UniChar('m'),UniChar('0')}) {
   CGGlyph glyph=0;if(!CTFontGetGlyphsForCharacters(font,&ch,&glyph,1)||!glyph)return 4;
   CGSize advance{};CTFontGetAdvancesForGlyphs(font,kCTFontOrientationHorizontal,&glyph,&advance,1);
   CGPathRef path=CTFontCreatePathForGlyph(font,glyph,nullptr);if(!path)return 5;
   CGRect bounds=CGPathGetPathBoundingBox(path);
   printf("%s\t%u\t%u\t%u\t%.17g\t%.17g\t%.17g\t%.17g\t%.17g\n",info.postscript,upem,(unsigned)ch,(unsigned)glyph,(double)advance.width,(double)bounds.origin.x,(double)bounds.origin.y,(double)bounds.size.width,(double)bounds.size.height);
   CGPathRelease(path);
  }CFRelease(font);
 }ravet_fonts_destroy(raw);return 0;
}}
