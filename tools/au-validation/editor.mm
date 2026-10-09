// Actual public AUv2 Cocoa view protocol in an owned synthetic host.
#import <Cocoa/Cocoa.h>
#import <AudioUnit/AUCocoaUIView.h>
#include <AudioToolbox/AudioToolbox.h>
#include <AudioToolbox/AudioUnitUtilities.h>
#include <array>
#include <cassert>
#include <cstring>
#include <iostream>
#include <limits>
extern "C" void* editor_probe_create();
extern "C" void* editor_probe_view(void*);
extern "C" size_t editor_probe_children(void*);
extern "C" bool editor_probe_eval(void*,const char*);
extern "C" bool editor_probe_snapshot(void*,const char*);
extern "C" void editor_probe_pump(double);
extern "C" void editor_probe_destroy(void*);
extern "C" void editor_probe_resize(void*,double,double);
extern "C" void editor_probe_show(void*);
void ready(void* parent,const char* expression) {
    bool okay=false;
    for(int i=0;i<60&&!okay;++i){editor_probe_pump(.05);okay=editor_probe_eval(parent,expression);}
    if(!okay)std::cerr<<"AU editor readiness failed: "<<expression<<"\n";
    assert(okay);
}
bool invalidMeterAudio=false;
OSStatus input(void*,AudioUnitRenderActionFlags*,const AudioTimeStamp*,UInt32,UInt32 frames,AudioBufferList* buffers) {
    for(UInt32 channel=0;channel<buffers->mNumberBuffers;++channel) {
        auto* samples=static_cast<float*>(buffers->mBuffers[channel].mData);
        for(UInt32 frame=0;frame<frames;++frame)samples[frame]=(invalidMeterAudio && channel==0 && frame==0)?std::numeric_limits<float>::quiet_NaN():(channel==0?.25f:-.5f);
    }return noErr;
}
void setGain(AudioUnit unit,AudioUnitParameterValue value) {
    AudioUnitParameter parameter{unit,1,kAudioUnitScope_Global,0};
    assert(AUParameterSet(nullptr,nullptr,&parameter,value,0)==noErr);
}
int main(int argc,char** argv) {
    @autoreleasepool {
        assert(argc>=2);
        void* parent=editor_probe_create();
        editor_probe_show(parent);
        NSURL* url=[NSURL fileURLWithPath:[NSString stringWithUTF8String:argv[1]]];
        CFBundleRef bundle=CFBundleCreate(nullptr,(__bridge CFURLRef)url);
        assert(bundle && CFBundleLoadExecutable(bundle));
        auto factory=reinterpret_cast<AudioComponentFactoryFunction>(CFBundleGetFunctionPointerForName(bundle,CFSTR("AUWrapperFactory")));
        assert(factory);
        AudioComponentDescription description{'aufx','RvGn','Ravt',0,0};
        AudioComponent component=AudioComponentRegister(&description,CFSTR("RAVET: Gain"),256,factory);
        AudioUnit unit=nullptr;assert(AudioComponentInstanceNew(component,&unit)==noErr);
        AudioStreamBasicDescription format{};
        format.mSampleRate=48000.;format.mFormatID=kAudioFormatLinearPCM;
        format.mFormatFlags=kAudioFormatFlagsNativeFloatPacked|kAudioFormatFlagIsNonInterleaved;
        format.mBytesPerPacket=format.mBytesPerFrame=4;format.mFramesPerPacket=1;format.mChannelsPerFrame=2;format.mBitsPerChannel=32;
        assert(AudioUnitSetProperty(unit,kAudioUnitProperty_StreamFormat,kAudioUnitScope_Input,0,&format,sizeof(format))==noErr);
        assert(AudioUnitSetProperty(unit,kAudioUnitProperty_StreamFormat,kAudioUnitScope_Output,0,&format,sizeof(format))==noErr);
        UInt32 maximum=64;
        assert(AudioUnitSetProperty(unit,kAudioUnitProperty_MaximumFramesPerSlice,kAudioUnitScope_Global,0,&maximum,sizeof(maximum))==noErr);
        AURenderCallbackStruct callback{input,nullptr};
        assert(AudioUnitSetProperty(unit,kAudioUnitProperty_SetRenderCallback,kAudioUnitScope_Input,0,&callback,sizeof(callback))==noErr);
        assert(AudioUnitInitialize(unit)==noErr);
        AudioUnitCocoaViewInfo cocoa{};UInt32 size=sizeof(cocoa);
        assert(AudioUnitGetProperty(unit,kAudioUnitProperty_CocoaUI,kAudioUnitScope_Global,0,&cocoa,&size)==noErr);
        assert(size==sizeof(cocoa) && cocoa.mCocoaAUViewBundleLocation && cocoa.mCocoaAUViewClass[0]);
        NSString* name=(__bridge NSString*)cocoa.mCocoaAUViewClass[0];
        Class type=NSClassFromString(name);assert(type && [type conformsToProtocol:@protocol(AUCocoaUIBase)]);
        id<AUCocoaUIBase> provider=[[type alloc] init];assert(provider && [provider interfaceVersion]==0);
        CFRelease(cocoa.mCocoaAUViewBundleLocation);CFRelease(cocoa.mCocoaAUViewClass[0]);
        std::array<float,64> left{},right{};
        struct Buffers {UInt32 count;AudioBuffer buffers[2];} buffers{2,{{1,256,left.data()},{1,256,right.data()}}};
        AudioTimeStamp time{};time.mFlags=kAudioTimeStampSampleTimeValid;
        auto render=[&](float a,float b) {
            for(int block=0;block<10;++block){AudioUnitRenderActionFlags flags=0;assert(AudioUnitRender(unit,&flags,&time,0,64,reinterpret_cast<AudioBufferList*>(&buffers))==noErr);time.mSampleTime+=64;}
            assert(left[63]==a && right[63]==b);
        };
        for(int cycle=0;cycle<3;++cycle) {
            @autoreleasepool {
                NSView* view=[provider uiViewForAudioUnit:unit withSize:NSMakeSize(480,480)];assert(view);
                editor_probe_resize(parent,480,480);
                [(__bridge NSView*)editor_probe_view(parent) addSubview:view];
                ready(parent,"document.getElementById('gain-number')?.value==='1'");
                assert(editor_probe_eval(parent,"(()=>{const n=document.getElementById('gain-number');n.value='3';n.dispatchEvent(new Event('change'));return true;})()"));
                editor_probe_pump(.2);
                AudioUnitParameterValue gain=0;
                assert(AudioUnitGetParameter(unit,1,kAudioUnitScope_Global,0,&gain)==noErr && gain==.75f);
                render(.75f,-1.5f);
                ready(parent,"document.getElementById('meters').hidden===false && document.getElementById('meter-0').value===0.25 && document.getElementById('meter-2').value===0.75 && document.getElementById('meter-3').value===1.5 && document.getElementById('meter-3').getAttribute('aria-label')==='Output R audio level'");
                const float levels[]{.0625f,.0625f,0.f,.125f,.125f,0.f,.1875f,.1875f,0.f,.375f,.375f,0.f};
                for(UInt32 index=0;index<12;++index) {
                    AudioUnitParameterInfo info{};UInt32 metadataSize=sizeof(info);
                    assert(AudioUnitGetProperty(unit,kAudioUnitProperty_ParameterInfo,kAudioUnitScope_Global,10+index,&info,&metadataSize)==noErr);
                    assert((info.flags&kAudioUnitParameterFlag_IsReadable) && !(info.flags&kAudioUnitParameterFlag_IsWritable));
                    if((info.flags&kAudioUnitParameterFlag_CFNameRelease) && info.cfNameString) CFRelease(info.cfNameString);
                    AudioUnitParameterValue level=-1;
                    assert(AudioUnitGetParameter(unit,10+index,kAudioUnitScope_Global,0,&level)==noErr && level==levels[index]);
                }
                invalidMeterAudio=true;render(.75f,-1.5f);
                ready(parent,"document.getElementById('reading-0').textContent.includes('invalid audio') && document.getElementById('reading-2').textContent.includes('invalid audio')");
                AudioUnitParameterValue diagnostic=0;
                assert(AudioUnitGetParameter(unit,12,kAudioUnitScope_Global,0,&diagnostic)==noErr && diagnostic==1.f);
                assert(AudioUnitGetParameter(unit,18,kAudioUnitScope_Global,0,&diagnostic)==noErr && diagnostic==1.f);
                invalidMeterAudio=false;render(.75f,-1.5f);
                ready(parent,"!document.getElementById('reading-0').textContent.includes('invalid audio') && !document.getElementById('reading-2').textContent.includes('invalid audio')");
                CFPropertyListRef saved=nullptr;size=sizeof(saved);
                assert(AudioUnitGetProperty(unit,kAudioUnitProperty_ClassInfo,kAudioUnitScope_Global,0,&saved,&size)==noErr && saved);
                assert(editor_probe_eval(parent,"document.getElementById('bypass').click();true"));
                editor_probe_pump(.2);
                UInt32 bypass=0;size=sizeof(bypass);
                assert(AudioUnitGetProperty(unit,kAudioUnitProperty_BypassEffect,kAudioUnitScope_Global,0,&bypass,&size)==noErr && bypass==1);
                render(.25f,-.5f);
                bypass=0;
                assert(AudioUnitSetProperty(unit,kAudioUnitProperty_BypassEffect,kAudioUnitScope_Global,0,&bypass,sizeof(bypass))==noErr);
                setGain(unit,.125f);
                ready(parent,"document.getElementById('gain-number').value==='0.5'&&!document.getElementById('bypass').checked");
                render(.125f,-.25f);
                ready(parent,"document.getElementById('meter-2').value===0.125 && document.getElementById('meter-3').value===0.25");
                assert(AudioUnitSetProperty(unit,kAudioUnitProperty_ClassInfo,kAudioUnitScope_Global,0,&saved,sizeof(saved))==noErr);
                // Bulk preset recall needs the standard all-parameter listener
                // notification, including listener caches used by later edits.
                AudioUnitParameter all{unit,kAUParameterListener_AnyParameter,kAudioUnitScope_Global,0};
                assert(AUParameterListenerNotify(nullptr,nullptr,&all)==noErr);
                ready(parent,"document.getElementById('gain-number').value==='3'&&!document.getElementById('bypass').checked");
                render(.75f,-1.5f);
                CFRelease(saved);
                setGain(unit,.125f);
                ready(parent,"document.getElementById('gain-number').value==='0.5'");
                editor_probe_resize(parent,640,480);
                [view setFrame:NSMakeRect(0,0,640,480)];
                ready(parent,"window.innerWidth===640&&window.innerHeight===480");
                editor_probe_pump(.2);
                if(cycle==0 && argc>=3)assert(editor_probe_snapshot(parent,argv[2]));
                [view removeFromSuperview];
                assert(editor_probe_children(parent)==0);
                view=nil;
            }
            editor_probe_pump(.1);
            setGain(unit,.25f);
        }
        provider=nil;
        assert(AudioUnitUninitialize(unit)==noErr);
        assert(AudioComponentInstanceDispose(unit)==noErr);
        editor_probe_destroy(parent);CFRelease(bundle);
        std::cout<<"PASS actual AUv2 Cocoa editor: three lifecycle cycles, gain/bypass edits and audio, meter transfer/diagnostics, host/preset refresh and resize\n";
    }
}
