// Synthetic fixed-f64 consumer; distributed Performance AU remains f32.
#include "AudioUnitSDK/MusicDeviceBase.h"
#include "AudioUnitSDK/AUPlugInDispatch.h"
#include "RavetAuAudioFormat.hpp"
#include "RavetPerformance.h"
#include <array>
#include <cmath>
#include <cstring>
#include <iostream>
#include <stdexcept>
namespace {
constexpr AudioUnitPropertyID processCountProperty=8001;
class TestPrecisionAU final:public ausdk::MusicDeviceBase {
public:
    explicit TestPrecisionAU(AudioUnit instance):MusicDeviceBase(instance,0,1),parameters_(ravet_performance_parameters_v1(64)),entry_(parameters_->processor_entry()),core_(entry_->create()){CreateElements();}
    ~TestPrecisionAU()override{if(core_)entry_->destroy(core_);}
    bool CanScheduleParameters()const noexcept override{return false;}
    bool StreamFormatWritable(AudioUnitScope scope,AudioUnitElement element)override{return !IsInitialized()&&scope==kAudioUnitScope_Output&&!element;}
    bool ValidFormat(AudioUnitScope scope,AudioUnitElement element,const AudioStreamBasicDescription &format)override{return scope==kAudioUnitScope_Output&&!element&&format.mChannelsPerFrame<=2&&ravet::au::NativeAuAudioFormat64::format(format,format.mChannelsPerFrame,format.mSampleRate);}
    OSStatus ChangeStreamFormat(AudioUnitScope scope,AudioUnitElement element,const AudioStreamBasicDescription &old,const AudioStreamBasicDescription &next)override {
        if(IsInitialized())return kAudioUnitErr_CannotDoInCurrentContext;
        ravet::au::NativeAuAudioFormat64 candidate;auto config=layout(next);
        if(scope!=kAudioUnitScope_Output||element||!candidate.configure(entry_,config,nullptr,0,&next,1))return kAudioUnitErr_FormatNotSupported;
        const auto status=MusicDeviceBase::ChangeStreamFormat(scope,element,old,next);
        if(!status)profile_=candidate;return status;
    }
    OSStatus Initialize()override {
        if(!core_||!profile_.valid()||parameters_->processor_entry()!=entry_||parameters_->sample_bits!=64)return kAudioUnitErr_FailedInitialization;
        const auto format=Output(0).GetStreamFormat();ravet::au::NativeAuAudioFormat64 candidate;const auto config=layout(format);
        if(!candidate.configure(entry_,config,nullptr,0,&format,1))return kAudioUnitErr_FailedInitialization;
        const auto status=MusicDeviceBase::Initialize();if(status)return status;
        if(entry_->prepare(core_,&config))return kAudioUnitErr_FailedInitialization;profile_=candidate;return noErr;
    }
    void Cleanup()override{if(core_)entry_->deactivate(core_);count_=0;MusicDeviceBase::Cleanup();}
    OSStatus MIDIEvent(UInt32 status,UInt32 data1,UInt32 data2,UInt32 offset)noexcept override {
        if((status&~UInt32(255))||data1>127||data2>127||offset>=65536||count_==events_.size()||((status&0xf0)!=0x90&&(status&0xf0)!=0x80))return kAudioUnitErr_InvalidParameterValue;
        auto &event=events_[count_++];event={};event.kind=(status&0xf0)==0x80||!data2?RAVET_NATIVE_NOTE_OFF:RAVET_NATIVE_NOTE_ON;event.offset=offset;event.channel=status&15;event.key=data1;event.value=double(float(data2)/127.f);return noErr;
    }
    OSStatus GetPropertyInfo(AudioUnitPropertyID id,AudioUnitScope scope,AudioUnitElement element,UInt32 &size,bool &writable)override {
        if(id==processCountProperty){if(scope!=kAudioUnitScope_Global||element)return kAudioUnitErr_InvalidScope;size=sizeof(UInt32);writable=false;return noErr;}return MusicDeviceBase::GetPropertyInfo(id,scope,element,size,writable);
    }
    OSStatus GetProperty(AudioUnitPropertyID id,AudioUnitScope scope,AudioUnitElement element,void *data)override {
        if(id==processCountProperty){if(scope!=kAudioUnitScope_Global||element||!data)return kAudioUnitErr_InvalidPropertyValue;*static_cast<UInt32 *>(data)=processCount_;return noErr;}return MusicDeviceBase::GetProperty(id,scope,element,data);
    }
    OSStatus SaveState(CFPropertyListRef *output)override {
        if(IsInitialized()||!output)return kAudioUnitErr_CannotDoInCurrentContext;
        std::array<uint8_t,24> bytes{};if(parameters_->state_bytes!=bytes.size()||parameters_->save_state(core_,bytes.data(),bytes.size()))return kAudioUnitErr_InvalidPropertyValue;
        const auto data=CFDataCreate(nullptr,bytes.data(),bytes.size());const auto dictionary=CFDictionaryCreateMutable(nullptr,0,&kCFTypeDictionaryKeyCallBacks,&kCFTypeDictionaryValueCallBacks);
        if(!data||!dictionary){if(data)CFRelease(data);if(dictionary)CFRelease(dictionary);return kAudioUnitErr_FailedInitialization;}
        CFDictionarySetValue(dictionary,CFSTR("native-level"),data);CFRelease(data);const SInt32 values[]{0,SInt32('Rv64'),SInt32('Ravt')};uint32_t i=0;
        for(const auto key:{CFSTR("version"),CFSTR("subtype"),CFSTR("manufacturer")}){const auto number=CFNumberCreate(nullptr,kCFNumberSInt32Type,&values[i++]);if(!number){CFRelease(dictionary);return kAudioUnitErr_FailedInitialization;}CFDictionarySetValue(dictionary,key,number);CFRelease(number);}
        *output=dictionary;return noErr;
    }
    OSStatus RestoreState(CFPropertyListRef value)override {
        if(IsInitialized())return kAudioUnitErr_CannotDoInCurrentContext;
        if(!value||CFGetTypeID(value)!=CFDictionaryGetTypeID())return kAudioUnitErr_InvalidPropertyValue;
        const auto dictionary=static_cast<CFDictionaryRef>(value);if(CFDictionaryGetCount(dictionary)!=4)return kAudioUnitErr_InvalidPropertyValue;
        const SInt32 expected[]{0,SInt32('Rv64'),SInt32('Ravt')};uint32_t i=0;
        for(const auto key:{CFSTR("version"),CFSTR("subtype"),CFSTR("manufacturer")}){const auto field=CFDictionaryGetValue(dictionary,key);SInt32 number=0;if(!field||CFGetTypeID(field)!=CFNumberGetTypeID()||!CFNumberGetValue(static_cast<CFNumberRef>(field),kCFNumberSInt32Type,&number)||number!=expected[i++])return kAudioUnitErr_InvalidPropertyValue;}
        const auto field=CFDictionaryGetValue(dictionary,CFSTR("native-level"));if(!field||CFGetTypeID(field)!=CFDataGetTypeID())return kAudioUnitErr_InvalidPropertyValue;const auto data=static_cast<CFDataRef>(field);
        return CFDataGetLength(data)!=parameters_->state_bytes||parameters_->load_state(core_,CFDataGetBytePtr(data),uint32_t(CFDataGetLength(data)))?OSStatus(kAudioUnitErr_InvalidPropertyValue):OSStatus(noErr);
    }
    OSStatus Render(AudioUnitRenderActionFlags &flags,const AudioTimeStamp &,UInt32 frames)noexcept override {
        AudioBufferList *outputs[]{&Output(0).GetBufferList()};ravet::au::NativeAuAudioBlock64 block;
        if(!profile_.map(nullptr,0,outputs,1,frames,block))return kAudioUnitErr_InvalidParameterValue;
        RavetNativeProcessorResultV1 result{};++processCount_;const auto status=entry_->process(core_,block.buses(),1,frames,events_.data(),count_,nullptr,nullptr,0,&result);count_=0;
        if(status||result.status>1||result.event_count)return kAudioUnitErr_InvalidParameterValue;
        if(result.status==1)flags|=kAudioUnitRenderAction_OutputIsSilence;else flags&=~kAudioUnitRenderAction_OutputIsSilence;return noErr;
    }
    OSStatus renderCaller(AudioUnitRenderActionFlags *flags,const AudioTimeStamp *time,UInt32 bus,UInt32 frames,AudioBufferList *buffers) {
        if(!time||!buffers)return kAudio_ParamError;AudioUnitRenderActionFlags temporary{};auto &actual=flags?*flags:temporary;
        const auto channels=profile_.valid()?profile_.config().buses[0].outputs:0;AudioBufferList *outputs[]{buffers};ravet::au::NativeAuAudioBlock64 caller;
        const bool valid=bus==0&&profile_.map(nullptr,0,outputs,1,frames,caller);
        const auto status=valid?DoRender(actual,*time,bus,frames,*buffers):OSStatus(kAudioUnitErr_InvalidParameterValue);
        if(status&&bus==0&&channels&&buffers->mNumberBuffers==channels){for(UInt32 c=0;c<channels;++c){auto &buffer=buffers->mBuffers[c];if(buffer.mNumberChannels==1&&buffer.mData&&uintptr_t(buffer.mData)%alignof(double)==0){const auto count=std::min({frames,GetMaxFramesPerSlice(),buffer.mDataByteSize/UInt32(sizeof(double))});if(uintptr_t(buffer.mData)<=UINTPTR_MAX-uintptr_t(count)*sizeof(double))std::fill_n(static_cast<double *>(buffer.mData),count,0.);}}}
        return status;
    }
private:
    RavetNativeProcessorConfigV1 layout(const AudioStreamBasicDescription &format){RavetNativeProcessorConfigV1 config{};config.version=1;config.maximum_frames=GetMaxFramesPerSlice();config.maximum_events=1024;config.bus_count=1;config.sample_rate=format.mSampleRate;config.buses[0]={0,format.mChannelsPerFrame};return config;}
    const RavetNativeParameterEntryV1 *parameters_;const RavetNativeProcessorEntryV1 *entry_;void *core_;ravet::au::NativeAuAudioFormat64 profile_;std::array<RavetNativeEventV1,1024> events_{};uint32_t count_=0,processCount_=0;
};
OSStatus render(void *self,AudioUnitRenderActionFlags *flags,const AudioTimeStamp *time,UInt32 bus,UInt32 frames,AudioBufferList *buffers){auto *instance=static_cast<ausdk::AudioComponentPlugInInstance *>(self);return reinterpret_cast<TestPrecisionAU *>(&instance->mInstanceStorage)->renderCaller(flags,time,bus,frames,buffers);}
struct PrecisionLookup{static AudioComponentMethod Lookup(SInt16 selector){return selector==kAudioUnitRenderSelect?reinterpret_cast<AudioComponentMethod>(render):ausdk::AUMusicLookup::Lookup(selector);}};
AudioComponentPlugInInterface *factory(const AudioComponentDescription *description){return static_cast<AudioComponentPlugInInterface *>(ausdk::APFactory<PrecisionLookup,TestPrecisionAU>::Factory(description));}
void require(bool value,const char *text){if(!value)throw std::runtime_error(text);}
void okay(OSStatus status,const char *text){if(status){std::cerr<<text<<": "<<status<<'\n';throw std::runtime_error(text);}}
AudioStreamBasicDescription format64(uint32_t channels=1,double rate=48000.){AudioStreamBasicDescription value{};value.mSampleRate=rate;value.mFormatID=kAudioFormatLinearPCM;value.mFormatFlags=UInt32(kAudioFormatFlagsNativeFloatPacked)|UInt32(kAudioFormatFlagIsNonInterleaved);value.mBytesPerFrame=value.mBytesPerPacket=8;value.mFramesPerPacket=1;value.mChannelsPerFrame=channels;value.mBitsPerChannel=64;return value;}
void contracts(){AudioComponentDescription description{'aumu','Rv64','Ravt',0,0};const auto component=AudioComponentRegister(&description,CFSTR("Synthetic native f64 precision contracts"),256,factory);require(component,"registered fixed64 component");AudioUnit unit=nullptr;okay(AudioComponentInstanceNew(component,&unit),"fixed64 instance");
    auto format=format64(2);auto set=[&](const AudioStreamBasicDescription &candidate){return AudioUnitSetProperty(unit,kAudioUnitProperty_StreamFormat,kAudioUnitScope_Output,0,&candidate,sizeof(candidate));};okay(set(format),"stopped stereo64 negotiation");
    auto read=[&](){AudioStreamBasicDescription current{};UInt32 size=sizeof(current);okay(AudioUnitGetProperty(unit,kAudioUnitProperty_StreamFormat,kAudioUnitScope_Output,0,&current,&size),"exact accepted format readback");return current;};
    for(uint32_t test=0;test<5;++test){auto bad=format;if(test==0){bad.mBitsPerChannel=32;bad.mBytesPerFrame=bad.mBytesPerPacket=4;}if(test==1)bad.mFormatFlags=kAudioFormatFlagsNativeFloatPacked;if(test==2)bad.mBytesPerFrame=4;if(test==3)bad.mSampleRate=0.;if(test==4)bad.mChannelsPerFrame=3;require(set(bad)!=noErr&&read().mBitsPerChannel==64&&read().mChannelsPerFrame==2&&read().mBytesPerFrame==8,"rejected negotiation retains accepted64 metadata");}
    UInt32 maximum=512;okay(AudioUnitSetProperty(unit,kAudioUnitProperty_MaximumFramesPerSlice,kAudioUnitScope_Global,0,&maximum,sizeof(maximum)),"bounded64 frames");okay(AudioUnitInitialize(unit),"fixed64 initialized native processor");require(set(format64(1))!=noErr,"active changed format rejected");okay(set(format),"unchanged active ASBD is a no-op");
    std::array<std::array<double,513>,2> samples{};struct Lists{UInt32 count=2;std::array<AudioBuffer,2> channels;}buffers;static_assert(offsetof(Lists,channels)==offsetof(AudioBufferList,mBuffers));for(UInt32 c=0;c<2;++c)buffers.channels[c]={1,512*8,samples[c].data()};AudioTimeStamp time{};time.mFlags=kAudioTimeStampSampleTimeValid;auto block=[&](UInt32 frames){for(auto &channel:samples)channel.fill(99.);AudioUnitRenderActionFlags flags=0;const auto status=AudioUnitRender(unit,&flags,&time,0,frames,reinterpret_cast<AudioBufferList *>(&buffers));time.mSampleTime+=512;require(samples[0][512]==99.&&samples[1][512]==99.,"64 backing tails survive");return status;};
    okay(MusicDeviceMIDIEvent(unit,0x90,69,127,3),"timed native64 note");okay(block(300),"actual64 AU render");for(uint32_t i=0;i<200;++i){const auto n=i>=3?i-3:0;const auto expected=i<3?0.:std::sin(6.283185307179586*440./48000.*n)*(n+1)/240.*double(.2f)/16.;require(std::abs(samples[0][i]-expected)<1e-12&&samples[0][i]==samples[1][i],"actual64 exact timing/analytic instrument output");}require(samples[0][300]==99.,"64 active-prefix tail");
    auto processCount=[&](){UInt32 count=0,size=sizeof(count);okay(AudioUnitGetProperty(unit,processCountProperty,kAudioUnitScope_Global,0,&count,&size),"native64 process count");return count;};const auto beforeFault=processCount();
    buffers.channels[1].mDataByteSize=300*8-1;require(block(300)!=noErr,"64 late undersized caller preflight");require(samples[0][0]==0.&&samples[1][298]==0.&&samples[1][299]==99.,"64 supported prefix silence excludes partial sample and backing tail");require(processCount()==beforeFault,"late64 caller fault performs no DSP");buffers.channels[1].mDataByteSize=512*8;okay(block(16),"fresh64 recovery");
    okay(AudioUnitUninitialize(unit),"fixed64 stopped");CFPropertyListRef saved=nullptr;UInt32 size=sizeof(saved);okay(AudioUnitGetProperty(unit,kAudioUnitProperty_ClassInfo,kAudioUnitScope_Global,0,&saved,&size),"complete stopped native64 Level state");require(saved&&CFGetTypeID(saved)==CFDictionaryGetTypeID(),"typed full64 state dictionary");
    const auto data=static_cast<CFDataRef>(CFDictionaryGetValue(static_cast<CFDictionaryRef>(saved),CFSTR("native-level")));require(data&&CFDataGetLength(data)==24,"complete64 native state24bytes");std::array<uint8_t,24> bytes{};std::copy_n(CFDataGetBytePtr(data),24,bytes.data());const float level=.5f;std::memcpy(bytes.data()+20,&level,4);const auto half=CFDataCreate(nullptr,bytes.data(),24);const auto changed=CFDictionaryCreateMutableCopy(nullptr,0,static_cast<CFDictionaryRef>(saved));CFDictionarySetValue(changed,CFSTR("native-level"),half);CFRelease(half);CFPropertyListRef restore=changed;okay(AudioUnitSetProperty(unit,kAudioUnitProperty_ClassInfo,kAudioUnitScope_Global,0,&restore,sizeof(restore)),"complete64 changed Level restore");
    const SInt32 wrongIdentity=1,correctIdentity=SInt32('Rv64');const auto wrongNumber=CFNumberCreate(nullptr,kCFNumberSInt32Type,&wrongIdentity);CFDictionarySetValue(changed,CFSTR("subtype"),wrongNumber);CFRelease(wrongNumber);require(AudioUnitSetProperty(unit,kAudioUnitProperty_ClassInfo,kAudioUnitScope_Global,0,&restore,sizeof(restore))!=noErr,"foreign identity full64 state rejected before native load");const auto correctNumber=CFNumberCreate(nullptr,kCFNumberSInt32Type,&correctIdentity);CFDictionarySetValue(changed,CFSTR("subtype"),correctNumber);CFRelease(correctNumber);
    bytes[16]=2;const auto malformed=CFDataCreate(nullptr,bytes.data(),24);CFDictionarySetValue(changed,CFSTR("native-level"),malformed);CFRelease(malformed);require(AudioUnitSetProperty(unit,kAudioUnitProperty_ClassInfo,kAudioUnitScope_Global,0,&restore,sizeof(restore))!=noErr,"malformed complete64 restore atomic rejection");CFRelease(changed);
    CFPropertyListRef retained=nullptr;size=sizeof(retained);okay(AudioUnitGetProperty(unit,kAudioUnitProperty_ClassInfo,kAudioUnitScope_Global,0,&retained,&size),"retained64 Level after malformed recall");const auto retainedData=static_cast<CFDataRef>(CFDictionaryGetValue(static_cast<CFDictionaryRef>(retained),CFSTR("native-level")));float observed=0.;std::memcpy(&observed,CFDataGetBytePtr(retainedData)+20,4);require(observed==.5f,"accepted64 Level remains after malformed recall");CFRelease(retained);
    format=format64(1,44100.);okay(set(format),"stopped mono64 rate change");okay(AudioUnitInitialize(unit),"reprepare64 preserves complete Level");buffers.count=1;buffers.channels[0].mDataByteSize=512*8;okay(MusicDeviceMIDIEvent(unit,0x90,69,127,0),"fresh64 note after state recall");okay(block(32),"mono64 render at negotiated rate");const auto expected=std::sin(6.283185307179586*440./44100.)*2./221.*.5/16.;require(std::abs(samples[0][1]-expected)<1e-12,"native64 savedLevel and negotiated rate consumed");okay(AudioUnitUninitialize(unit),"final64 stopped");restore=saved;okay(AudioUnitSetProperty(unit,kAudioUnitProperty_ClassInfo,kAudioUnitScope_Global,0,&restore,sizeof(restore)),"original complete64 recall");CFRelease(saved);okay(AudioComponentInstanceDispose(unit),"fixed64 retired");
}
}
int main(){try{contracts();std::cout<<"PASS synthetic AU fixed64 negotiation/native render/complete Level state contracts\n";return 0;}catch(const std::exception &error){std::cerr<<error.what()<<'\n';return 1;}}
