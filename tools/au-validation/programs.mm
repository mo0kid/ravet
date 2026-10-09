// Actual local component factory preset contract, no installed/global plug-ins.
#include <AudioToolbox/AudioToolbox.h>
#include <CoreFoundation/CoreFoundation.h>
#include <array>
#include <cassert>
#include <cstring>
#include <iostream>
OSStatus input(void*,AudioUnitRenderActionFlags*,const AudioTimeStamp*,UInt32,UInt32 frames,AudioBufferList* buffers) {
    for(UInt32 channel=0;channel<buffers->mNumberBuffers;++channel) {
        auto* data=static_cast<float*>(buffers->mBuffers[channel].mData);
        for(UInt32 frame=0;frame<frames;++frame)data[frame]=channel==0?.25f:-.5f;
    }return noErr;
}
int main(int argc,char** argv) {
    assert(argc==2);
    auto url=CFURLCreateFromFileSystemRepresentation(nullptr,reinterpret_cast<const UInt8*>(argv[1]),std::strlen(argv[1]),true);
    auto bundle=CFBundleCreate(nullptr,url);CFRelease(url);assert(bundle && CFBundleLoadExecutable(bundle));
    auto factory=reinterpret_cast<AudioComponentFactoryFunction>(CFBundleGetFunctionPointerForName(bundle,CFSTR("AUWrapperFactory")));assert(factory);
    AudioComponentDescription description{'aufx','RvGn','Ravt',0,0};
    auto component=AudioComponentRegister(&description,CFSTR("RAVET: Gain"),256,factory);assert(component);
    unsigned blocks=0;
    for(int cycle=0;cycle<3;++cycle) {
        AudioUnit unit=nullptr;assert(AudioComponentInstanceNew(component,&unit)==noErr);
        AudioStreamBasicDescription format{};format.mSampleRate=48000.;format.mFormatID=kAudioFormatLinearPCM;
        format.mFormatFlags=kAudioFormatFlagsNativeFloatPacked|kAudioFormatFlagIsNonInterleaved;
        format.mBytesPerPacket=format.mBytesPerFrame=4;format.mFramesPerPacket=1;format.mChannelsPerFrame=2;format.mBitsPerChannel=32;
        assert(AudioUnitSetProperty(unit,kAudioUnitProperty_StreamFormat,kAudioUnitScope_Input,0,&format,sizeof(format))==noErr);
        assert(AudioUnitSetProperty(unit,kAudioUnitProperty_StreamFormat,kAudioUnitScope_Output,0,&format,sizeof(format))==noErr);
        UInt32 maximum=64;assert(AudioUnitSetProperty(unit,kAudioUnitProperty_MaximumFramesPerSlice,kAudioUnitScope_Global,0,&maximum,sizeof(maximum))==noErr);
        AURenderCallbackStruct callback{input,nullptr};assert(AudioUnitSetProperty(unit,kAudioUnitProperty_SetRenderCallback,kAudioUnitScope_Input,0,&callback,sizeof(callback))==noErr);
        assert(AudioUnitInitialize(unit)==noErr);
        CFArrayRef presets=nullptr;UInt32 size=sizeof(presets);
        assert(AudioUnitGetProperty(unit,kAudioUnitProperty_FactoryPresets,kAudioUnitScope_Global,0,&presets,&size)==noErr && presets && CFArrayGetCount(presets)==3);
        CFStringRef names[]={CFSTR("Unity"),CFSTR("Silence"),CFSTR("Bypass")};
        for(CFIndex index=0;index<3;++index) {
            auto* preset=static_cast<const AUPreset*>(CFArrayGetValueAtIndex(presets,index));
            assert(preset && preset->presetNumber==index && CFEqual(preset->presetName,names[index]));
        }
        std::array<float,64> left{},right{};
        struct Buffers {UInt32 count;AudioBuffer channels[2];} buffers{2,{{1,256,left.data()},{1,256,right.data()}}};
        AudioTimeStamp time{};time.mFlags=kAudioTimeStampSampleTimeValid;
        auto render=[&](float gain) {
            for(int block=0;block<10;++block) {
                left.fill(99);right.fill(99);AudioUnitRenderActionFlags flags=0;
                assert(AudioUnitRender(unit,&flags,&time,0,64,reinterpret_cast<AudioBufferList*>(&buffers))==noErr);time.mSampleTime+=64;++blocks;
                if(block==9)for(size_t frame=0;frame<64;++frame){assert(left[frame]==.25f*gain);assert(right[frame]==-.5f*gain);}
            }
        };
        for(int32_t index:{0,1,2,2,1,0}) {
            auto* preset=static_cast<const AUPreset*>(CFArrayGetValueAtIndex(presets,index));
            assert(AudioUnitSetProperty(unit,kAudioUnitProperty_PresentPreset,kAudioUnitScope_Global,0,preset,sizeof(*preset))==noErr);
            render(index==1?0.f:1.f);
            AudioUnitParameterValue gain=99;assert(AudioUnitGetParameter(unit,1,kAudioUnitScope_Global,0,&gain)==noErr);
            assert(gain==(index==1?0.f:.25f));
            UInt32 bypass=99;size=sizeof(bypass);assert(AudioUnitGetProperty(unit,kAudioUnitProperty_BypassEffect,kAudioUnitScope_Global,0,&bypass,&size)==noErr);
            assert(bypass==(index==2?1u:0u));
            AUPreset current{};size=sizeof(current);assert(AudioUnitGetProperty(unit,kAudioUnitProperty_PresentPreset,kAudioUnitScope_Global,0,&current,&size)==noErr);
            assert(current.presetNumber==index && CFEqual(current.presetName,names[index]));
            if(current.presetName)CFRelease(current.presetName);
        }
        // Negative numbers denote custom preset metadata in AUBase; they do
        // not request a factory program or change the current DSP controls.
        AUPreset custom{-1,CFSTR("Custom")};
        assert(AudioUnitSetProperty(unit,kAudioUnitProperty_PresentPreset,kAudioUnitScope_Global,0,&custom,sizeof(custom))==noErr);
        render(1.f);
        for(auto value:{.25f,1.f,0.f}) {
            assert(AudioUnitSetParameter(unit,3,kAudioUnitScope_Global,0,value,0)==noErr);
            const auto index=value==.25f?1:(value==1.f?2:0);
            render(index==1?0.f:1.f);
            AUPreset current{};size=sizeof(current);
            assert(AudioUnitGetProperty(unit,kAudioUnitProperty_PresentPreset,kAudioUnitScope_Global,0,&current,&size)==noErr && current.presetNumber==index);
            if(current.presetName)CFRelease(current.presetName);
        }
        assert(AudioUnitSetParameter(unit,3,kAudioUnitScope_Global,0,1.f,1)!=noErr);
        CFPropertyListRef saved=nullptr;size=sizeof(saved);
        assert(AudioUnitGetProperty(unit,kAudioUnitProperty_ClassInfo,kAudioUnitScope_Global,0,&saved,&size)==noErr && saved);
        assert(AudioUnitSetParameter(unit,3,kAudioUnitScope_Global,0,.5f,0)==noErr);
        render(0.f);
        assert(AudioUnitSetProperty(unit,kAudioUnitProperty_ClassInfoFromDocument,kAudioUnitScope_Global,0,&saved,sizeof(saved))==noErr);
        render(1.f);CFRelease(saved);
        for(int32_t bad:{3,2147483647}) {
            AUPreset preset{bad,CFSTR("invalid")};
            assert(AudioUnitSetProperty(unit,kAudioUnitProperty_PresentPreset,kAudioUnitScope_Global,0,&preset,sizeof(preset))!=noErr);
            render(1.f);
            AudioUnitParameterValue gain=99;assert(AudioUnitGetParameter(unit,1,kAudioUnitScope_Global,0,&gain)==noErr && gain==.25f);
        }
        CFRelease(presets);assert(AudioUnitUninitialize(unit)==noErr);assert(AudioComponentInstanceDispose(unit)==noErr);
    }
    CFRelease(bundle);
    std::cout<<"PASS actual AUv2 factory programs: three cycles, complete names/indices, repeated/reversed recall, gain/bypass audio/property coherence and invalid preset preservation; "<<blocks<<" render blocks\n";
}
