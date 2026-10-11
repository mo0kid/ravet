#import <Cocoa/Cocoa.h>
#import <CoreText/CoreText.h>
#include "SystemFonts.h"
#include <algorithm>
#include <cmath>
#include <cstring>
#include <vector>

@interface RavetFontOwner : NSObject
@property(nonatomic,strong) NSArray* descriptors;
@property(nonatomic) BOOL busy;
@end
@implementation RavetFontOwner
@end
@interface RavetFontResource : NSObject
@property(nonatomic,strong) NSArray<NSData*>* tables;
@property(nonatomic,strong) NSArray<NSNumber*>* tags;
@property(nonatomic,strong) NSData* axes;
@end
@implementation RavetFontResource
@end
static bool mainThread() { return [NSThread isMainThread]; }
static bool readBooleanAttribute(CTFontDescriptorRef descriptor,CFStringRef key,bool absent,bool& valid) {
    CFTypeRef value=CTFontDescriptorCopyAttribute(descriptor,key);
    if(!value)return absent;
    bool result=false;
    if(CFGetTypeID(value)==CFBooleanGetTypeID())result=CFBooleanGetValue((CFBooleanRef)value);
    else if(CFGetTypeID(value)==CFNumberGetTypeID()) {
        // Measured CoreText descriptors encode these documented Boolean fields as
        // CFNumber0/1. Admit exactly those values, never an arbitrary truthy scalar.
        double numeric=0;
        if(!CFNumberGetValue((CFNumberRef)value,kCFNumberDoubleType,&numeric)||(numeric!=0&&numeric!=1))valid=false;
        else result=numeric==1;
    } else valid=false;
    CFRelease(value);return result;
}
static bool booleanAttribute(CTFontDescriptorRef descriptor,CFStringRef key) {
    bool valid=true;const bool result=readBooleanAttribute(descriptor,key,false,valid);
    return valid?result:true; // Unknown downloadable metadata conservatively refuses load.
}
// Collection excludes disabled entries. Explicit false remains unavailable;
// missing descriptor-registration state does not disprove snapshot membership.
static bool availableInSnapshot(CTFontDescriptorRef descriptor) {
    bool valid=true;const bool result=readBooleanAttribute(descriptor,kCTFontEnabledAttribute,true,valid);
    return valid&&result;
}
static bool stringAttribute(CTFontDescriptorRef descriptor,CFStringRef key,char* destination,bool required) {
    CFTypeRef value=CTFontDescriptorCopyAttribute(descriptor,key);
    if(!value)return !required;
    bool result=false;
    if(CFGetTypeID(value)==CFStringGetTypeID()) {
        CFStringRef string=(CFStringRef)value;
        const CFIndex count=CFStringGetLength(string);
        bool controls=false;
        if(count<=1024)for(CFIndex index=0;index<count;++index) {
            const UniChar c=CFStringGetCharacterAtIndex(string,index);
            if(c<0x20||(c>=0x7f&&c<=0x9f)){controls=true;break;}
        }
        if(count<=1024&&(!required||count>0)&&!controls)
            result=CFStringGetCString(string,destination,1025,kCFStringEncodingUTF8);
    }
    CFRelease(value);return result;
}
static bool number(CFDictionaryRef dictionary,CFStringRef key,double& result) {
    if(!dictionary)return true;
    CFTypeRef value=CFDictionaryGetValue(dictionary,key);
    return !value||(CFGetTypeID(value)==CFNumberGetTypeID()&&CFNumberGetValue((CFNumberRef)value,kCFNumberDoubleType,&result)&&std::isfinite(result));
}
static uint32_t describe(CTFontDescriptorRef descriptor,RavetFontDescription& result) {
    if(!stringAttribute(descriptor,kCTFontFamilyNameAttribute,result.family,true)||
       !stringAttribute(descriptor,kCTFontStyleNameAttribute,result.style,false)||
       !stringAttribute(descriptor,kCTFontDisplayNameAttribute,result.display,false)||
       !stringAttribute(descriptor,kCTFontNameAttribute,result.postscript,true))return 6;
    CFTypeRef raw=CTFontDescriptorCopyAttribute(descriptor,kCTFontTraitsAttribute);
    if(raw&&CFGetTypeID(raw)!=CFDictionaryGetTypeID()){CFRelease(raw);return 6;}
    CFDictionaryRef traits=(CFDictionaryRef)raw;
    bool okay=number(traits,kCTFontWeightTrait,result.weight)&&number(traits,kCTFontWidthTrait,result.width)&&number(traits,kCTFontSlantTrait,result.slant);
    double symbolic=0;okay=okay&&number(traits,kCTFontSymbolicTrait,symbolic);
    if(symbolic<0||symbolic>UINT32_MAX||std::floor(symbolic)!=symbolic)okay=false;
    if(okay)result.traits=(uint32_t)symbolic;
    if(raw)CFRelease(raw);
    if(!okay){result.weight=NAN;result.width=NAN;result.slant=NAN;}
    bool availabilityValid=true;
    const bool enabled=readBooleanAttribute(descriptor,kCTFontEnabledAttribute,true,availabilityValid);
    const bool downloadable=readBooleanAttribute(descriptor,kCTFontDownloadableAttribute,false,availabilityValid);
    const bool downloaded=readBooleanAttribute(descriptor,kCTFontDownloadedAttribute,false,availabilityValid);
    result.availability=(enabled?1:0)|(downloadable?2:0)|(downloaded?4:0)|(availabilityValid?0:8);
    CFTypeRef format=CTFontDescriptorCopyAttribute(descriptor,kCTFontFormatAttribute);
    int32_t kind=0;
    if(format&&CFGetTypeID(format)==CFNumberGetTypeID())CFNumberGetValue((CFNumberRef)format,kCFNumberSInt32Type,&kind);
    if(format)CFRelease(format);result.format=kind>=0?(uint32_t)kind:0;
    return 0;
}
uint32_t ravet_fonts_create(void** destination,size_t* count) {
    try {
    if(!mainThread())return 4;if(!destination||!count)return 1;
    @try {
        CTFontCollectionRef collection=CTFontCollectionCreateFromAvailableFonts((__bridge CFDictionaryRef)@{(__bridge NSString*)kCTFontCollectionRemoveDuplicatesOption:@YES,(__bridge NSString*)kCTFontCollectionIncludeDisabledFontsOption:@NO,(__bridge NSString*)kCTFontCollectionDisallowAutoActivationOption:@YES});
        if(!collection)return 7;
        id collectionOwner __attribute__((objc_precise_lifetime))=CFBridgingRelease(collection);(void)collectionOwner;
        CFArrayRef descriptors=CTFontCollectionCreateMatchingFontDescriptors(collection);
        if(!descriptors)return 5;
        if(CFArrayGetCount(descriptors)>4096){CFRelease(descriptors);return 2;}
        RavetFontOwner* owner=[RavetFontOwner new];
        owner.descriptors=CFBridgingRelease(descriptors);
        const size_t size=owner.descriptors.count;
        *destination=(__bridge_retained void*)owner;*count=size;return 0;
    } @catch(NSException*) {return 7;}

    } catch (...) { return 7; }
}
uint32_t ravet_fonts_descriptor(void* raw,size_t index,RavetFontDescription* destination) {
    try {
    if(!mainThread())return 4;if(!raw||!destination)return 1;
    RavetFontOwner* owner=(__bridge RavetFontOwner*)raw;
    if(owner.busy)return 5;if(index>=owner.descriptors.count)return 1;
    @try {
        RavetFontDescription value{};
        const uint32_t status=describe((__bridge CTFontDescriptorRef)owner.descriptors[index],value);
        if(status)return status;*destination=value;return 0;
    } @catch(NSException*) {return 7;}

    } catch (...) { return 7; }
}
static bool effectsSupported(CTFontDescriptorRef descriptor) {
    CFDictionaryRef attributes=CTFontDescriptorCopyAttributes(descriptor);
    if(!attributes)return false;
    bool okay=true;
    CFTypeRef fixed=CFDictionaryGetValue(attributes,kCTFontFixedAdvanceAttribute);
    CFTypeRef matrix=CFDictionaryGetValue(attributes,kCTFontMatrixAttribute);
    CFTypeRef features=CFDictionaryGetValue(attributes,kCTFontFeatureSettingsAttribute);
    if(fixed)okay=false;
    if(matrix) {
        CGAffineTransform transform{};
        if(CFGetTypeID(matrix)!=CFDataGetTypeID()||CFDataGetLength((CFDataRef)matrix)!=(CFIndex)sizeof(transform))okay=false;
        else {std::memcpy(&transform,CFDataGetBytePtr((CFDataRef)matrix),sizeof(transform));if(!CGAffineTransformIsIdentity(transform))okay=false;}
    }
    if(features&&(CFGetTypeID(features)!=CFArrayGetTypeID()||CFArrayGetCount((CFArrayRef)features)!=0))okay=false;
    CFRelease(attributes);return okay;
}
static bool identity(CTFontDescriptorRef selected,CTFontRef font) {
    CTFontDescriptorRef actual=CTFontCopyFontDescriptor(font);
    if(!actual)return false;
    bool okay=true;
    for(CFStringRef key:{kCTFontNameAttribute,kCTFontFamilyNameAttribute,kCTFontStyleNameAttribute,kCTFontURLAttribute}) {
        CFTypeRef a=CTFontDescriptorCopyAttribute(selected,key),b=CTFontDescriptorCopyAttribute(actual,key);
        if(a&&(!b||!CFEqual(a,b)))okay=false;
        if(a)CFRelease(a);if(b)CFRelease(b);
    }
    if(!CGAffineTransformIsIdentity(CTFontGetMatrix(font)))okay=false;
    CFRelease(actual);return okay;
}
static bool supportedFontFormat(uint32_t format) {
    if(format==kCTFontFormatOpenTypeTrueType||format==kCTFontFormatTrueType)return true;
#if RAVET_POSTSCRIPT_FONTS
    if(format==kCTFontFormatOpenTypePostScript)return true;
#endif
    return false;
}
static bool axisValue(CFDictionaryRef axis,CFStringRef key,double& value) {
    CFTypeRef raw=CFDictionaryGetValue(axis,key);
    return raw&&CFGetTypeID(raw)==CFNumberGetTypeID()&&CFNumberGetValue((CFNumberRef)raw,kCFNumberDoubleType,&value)&&std::isfinite(value);
}
static uint32_t readAxes(CTFontRef font,std::vector<RavetFontAxis>& result) {
    CFArrayRef axes=CTFontCopyVariationAxes(font);
    CFDictionaryRef values=CTFontCopyVariation(font);
    if((axes&&CFArrayGetCount(axes)>32)||(values&&CFDictionaryGetCount(values)>32)) {if(axes)CFRelease(axes);if(values)CFRelease(values);return 2;}
    uint32_t status=0;
    if(axes)for(CFIndex index=0;index<CFArrayGetCount(axes);++index) {
        CFTypeRef raw=CFArrayGetValueAtIndex(axes,index);
        if(CFGetTypeID(raw)!=CFDictionaryGetTypeID()){status=6;break;}
        CFDictionaryRef axis=(CFDictionaryRef)raw;RavetFontAxis output{};double tag=0;
        if(!axisValue(axis,kCTFontVariationAxisIdentifierKey,tag)||tag<0||tag>UINT32_MAX||std::floor(tag)!=tag||
           !axisValue(axis,kCTFontVariationAxisMinimumValueKey,output.minimum)||
           !axisValue(axis,kCTFontVariationAxisDefaultValueKey,output.defaultValue)||
           !axisValue(axis,kCTFontVariationAxisMaximumValueKey,output.maximum)||
           output.minimum>output.defaultValue||output.defaultValue>output.maximum){status=6;break;}
        output.tag=(uint32_t)tag;output.value=output.defaultValue;
        if(std::any_of(result.begin(),result.end(),[&](const auto& a){return a.tag==output.tag;})){status=6;break;}
        CFTypeRef hidden=CFDictionaryGetValue(axis,kCTFontVariationAxisHiddenKey);
        if(hidden&&CFGetTypeID(hidden)!=CFBooleanGetTypeID()){status=6;break;}
        output.hidden=hidden&&CFBooleanGetValue((CFBooleanRef)hidden)?1:0;
        int64_t identifier=output.tag;CFNumberRef key=CFNumberCreate(nullptr,kCFNumberSInt64Type,&identifier);
        CFTypeRef value=values?CFDictionaryGetValue(values,key):nullptr;CFRelease(key);
        if(value&&(CFGetTypeID(value)!=CFNumberGetTypeID()||!CFNumberGetValue((CFNumberRef)value,kCFNumberDoubleType,&output.value))){status=6;break;}
        if(!std::isfinite(output.value)||output.value<output.minimum||output.value>output.maximum){status=6;break;}
        result.push_back(output);
    }
    if(!status&&values) {
        std::vector<const void*> keys((size_t)CFDictionaryGetCount(values));
        CFDictionaryGetKeysAndValues(values,keys.data(),nullptr);
        for(const void* raw:keys) {
            int64_t tag=0;
            if(CFGetTypeID(raw)!=CFNumberGetTypeID()||!CFNumberGetValue((CFNumberRef)raw,kCFNumberSInt64Type,&tag)||
                tag<0||tag>UINT32_MAX||!std::any_of(result.begin(),result.end(),[&](const auto& a){return a.tag==(uint32_t)tag;})){status=6;break;}
        }
    }
    if(axes)CFRelease(axes);if(values)CFRelease(values);return status;
}
uint32_t ravet_fonts_load(void* raw,size_t index,void** destination,size_t* tableCount,size_t* axisCount) {
    try {
    if(!mainThread())return 4;if(!raw||!destination||!tableCount||!axisCount)return 1;
    RavetFontOwner* owner=(__bridge RavetFontOwner*)raw;
    if(owner.busy)return 5;if(index>=owner.descriptors.count)return 1;owner.busy=YES;
    @try {
        CTFontDescriptorRef descriptor=(__bridge CTFontDescriptorRef)owner.descriptors[index];
        if(!availableInSnapshot(descriptor)||booleanAttribute(descriptor,kCTFontDownloadableAttribute))return 5;
        RavetFontDescription metadata{};const uint32_t metadataStatus=describe(descriptor,metadata);
        if(metadataStatus)return metadataStatus;
        if(!std::isfinite(metadata.weight)||!std::isfinite(metadata.width)||!std::isfinite(metadata.slant)||
            std::abs(metadata.weight)>1||std::abs(metadata.width)>1||std::abs(metadata.slant)>1||
            !supportedFontFormat(metadata.format))return 6;
        if(!effectsSupported(descriptor))return 6;
        CTFontRef font=CTFontCreateWithFontDescriptorAndOptions(descriptor,12,nullptr,kCTFontOptionsPreventAutoActivation);
        if(!font)return 5;
        // Scoped CF owner across all C++/Objective-C failures.
        id fontOwner __attribute__((objc_precise_lifetime))=CFBridgingRelease(font);(void)fontOwner;
        if(!identity(descriptor,font))return 6;
        std::vector<RavetFontAxis> axes;
        uint32_t status=readAxes(font,axes);if(status)return status;
        CTFontRef defaults=font;
        id defaultsOwner __attribute__((objc_precise_lifetime))=nil;
        if(!axes.empty()) {
            NSMutableDictionary* locations=[NSMutableDictionary dictionary];
            for(const auto& axis:axes)locations[@(axis.tag)]=@(axis.defaultValue);
            CTFontDescriptorRef changed=CTFontDescriptorCreateCopyWithAttributes(descriptor,(__bridge CFDictionaryRef)@{(__bridge NSString*)kCTFontVariationAttribute:locations});
            if(!changed)return 7;
            defaults=CTFontCreateWithFontDescriptorAndOptions(changed,12,nullptr,kCTFontOptionsPreventAutoActivation);CFRelease(changed);
            if(!defaults)return 6;defaultsOwner=CFBridgingRelease(defaults);
            std::vector<RavetFontAxis> defaultAxes;status=readAxes(defaults,defaultAxes);if(status)return status;
            if(defaultAxes.size()!=axes.size())return 6;
            for(const auto& axis:defaultAxes)if(axis.value!=axis.defaultValue)return 6;
        }
        (void)defaultsOwner;
        CFArrayRef tags=CTFontCopyAvailableTables(font,kCTFontTableOptionNoOptions);
        if(!tags)return 6;id tagsOwner __attribute__((objc_precise_lifetime))=CFBridgingRelease(tags);(void)tagsOwner;
        if(CFArrayGetCount(tags)<=0||CFArrayGetCount(tags)>256)return 2;
#if RAVET_POSTSCRIPT_FONTS
        bool cff=false,cff2=false,glyf=false,loca=false;
        for(CFIndex i=0;i<CFArrayGetCount(tags);++i) {
            const uintptr_t tag=(uintptr_t)CFArrayGetValueAtIndex(tags,i);
            cff|=tag==0x43464620u;cff2|=tag==0x43464632u;
            glyf|=tag==0x676c7966u;loca|=tag==0x6c6f6361u;
        }
        if(metadata.format==kCTFontFormatOpenTypePostScript) {
            if(cff==cff2||glyf||loca)return 6;
        } else if(cff||cff2)return 6;
#endif
        NSMutableArray<NSData*>* tables=[NSMutableArray array];NSMutableArray<NSNumber*>* storedTags=[NSMutableArray array];
        size_t total=0;
        for(CFIndex i=0;i<CFArrayGetCount(tags);++i) {
            const uintptr_t integer=(uintptr_t)CFArrayGetValueAtIndex(tags,i);if(integer>UINT32_MAX)return 6;
            const CTFontTableTag tag=(CTFontTableTag)integer;
            CFDataRef data=CTFontCopyTable(font,tag,kCTFontTableOptionNoOptions);if(!data)return 6;
            NSData* table=CFBridgingRelease(data);
            if(table.length>16*1024*1024-total)return 2;total+=table.length;
            if(defaults!=font) {
                CFDataRef normal=CTFontCopyTable(defaults,tag,kCTFontTableOptionNoOptions);
                if(!normal)return 6;const bool equal=CFEqual((__bridge CFDataRef)table,normal);CFRelease(normal);if(!equal)return 6;
            }
            [tables addObject:table];[storedTags addObject:@(tag)];
        }
        RavetFontResource* resource=[RavetFontResource new];resource.tables=tables;resource.tags=storedTags;
        resource.axes=[NSData dataWithBytes:axes.data() length:axes.size()*sizeof(RavetFontAxis)];
        *destination=(__bridge_retained void*)resource;*tableCount=tables.count;*axisCount=axes.size();return 0;
    } @catch(NSException*) {return 7;} @finally {owner.busy=NO;}

    } catch (...) { return 7; }
}
uint32_t ravet_fonts_table(void* raw,size_t index,uint32_t* tag,size_t* length) {
    if(!mainThread())return 4;if(!raw||!tag||!length)return 1;
    RavetFontResource* resource=(__bridge RavetFontResource*)raw;if(index>=resource.tables.count)return 1;
    *tag=resource.tags[index].unsignedIntValue;*length=resource.tables[index].length;return 0;
}
uint32_t ravet_fonts_copy_table(void* raw,size_t index,uint8_t* destination,size_t capacity) {
    if(!mainThread())return 4;if(!raw)return 1;
    RavetFontResource* resource=(__bridge RavetFontResource*)raw;if(index>=resource.tables.count)return 1;
    NSData* data=resource.tables[index];if(capacity<data.length)return 2;if(data.length&&!destination)return 1;
    if(data.length)std::memcpy(destination,data.bytes,data.length);return 0;
}
uint32_t ravet_fonts_axis(void* raw,size_t index,RavetFontAxis* destination) {
    if(!mainThread())return 4;if(!raw||!destination)return 1;
    RavetFontResource* resource=(__bridge RavetFontResource*)raw;if(index>=resource.axes.length/sizeof(RavetFontAxis))return 1;
    std::memcpy(destination,(const uint8_t*)resource.axes.bytes+index*sizeof(RavetFontAxis),sizeof(RavetFontAxis));return 0;
}
void ravet_fonts_resource_destroy(void* raw) {if(!mainThread()||!raw)return;id resource=CFBridgingRelease(raw);(void)resource;}
void ravet_fonts_destroy(void* raw) {if(!mainThread()||!raw)return;RavetFontOwner* owner=(__bridge RavetFontOwner*)raw;if(owner.busy)return;id release=CFBridgingRelease(raw);(void)release;}
