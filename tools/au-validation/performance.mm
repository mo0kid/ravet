#include <AudioToolbox/AudioToolbox.h>
#include <CoreFoundation/CoreFoundation.h>
#include <array>
#include <cmath>
#include <cstring>
#include <iostream>
#include <limits>
#include <stdexcept>
void require(bool okay,const char *message){if(!okay)throw std::runtime_error(message);}
void okay(OSStatus status,const char *message){if(status){std::cerr<<message<<": "<<status<<'\n';throw std::runtime_error(message);}}
struct Buffers {UInt32 count; AudioBuffer channels[2];};
struct TransportHost {
 UInt32 beatCalls=0,signatureCalls=0,legacyCalls=0,modernCalls=0;
 bool initialized=true,missing=false;OSStatus status=noErr;
 Float64 sample=-240.;Float32 numerator=7.f;
};
OSStatus hostBeat(void *user,Float64 *beat,Float64 *tempo){auto &h=*static_cast<TransportHost *>(user);++h.beatCalls;h.initialized=h.initialized&&std::isnan(*beat)&&std::isnan(*tempo);if(!h.missing){*beat=-1.5;*tempo=120.;}return h.status;}
OSStatus hostSignature(void *user,UInt32 *clock,Float32 *numerator,UInt32 *denominator,Float64 *bar){auto &h=*static_cast<TransportHost *>(user);++h.signatureCalls;h.initialized=h.initialized&&!clock&&!bar&&std::isnan(*numerator)&&!*denominator;if(!h.missing){*numerator=h.numerator;*denominator=8;}return h.status;}
OSStatus hostTransport2(void *user,Boolean *playing,Boolean *recording,Boolean *changed,Float64 *sample,Boolean *cycling,Float64 *start,Float64 *end){auto &h=*static_cast<TransportHost *>(user);++h.modernCalls;h.initialized=h.initialized&&*playing==2&&*recording==2&&*changed==2&&*cycling==2&&std::isnan(*sample)&&std::isnan(*start)&&std::isnan(*end);if(!h.missing){*playing=*recording=*changed=*cycling=1;*sample=h.sample;*start=-4.;*end=8.;}return h.status;}
OSStatus hostTransport(void *user,Boolean *playing,Boolean *changed,Float64 *sample,Boolean *cycling,Float64 *start,Float64 *end){auto &h=*static_cast<TransportHost *>(user);++h.legacyCalls;Boolean recording=2;const auto result=hostTransport2(user,playing,&recording,changed,sample,cycling,start,end);--h.modernCalls;return result;}
int main(int argc,char **argv){
 try{
  require(argc==2,"actual component path");
  auto url=CFURLCreateFromFileSystemRepresentation(nullptr,reinterpret_cast<const UInt8 *>(argv[1]),std::strlen(argv[1]),true);
  auto bundle=CFBundleCreate(nullptr,url);CFRelease(url);require(bundle&&CFBundleLoadExecutable(bundle),"load actual public component");
  auto factory=reinterpret_cast<AudioComponentFactoryFunction>(CFBundleGetFunctionPointerForName(bundle,CFSTR("AUWrapperFactory")));require(factory,"actual MusicDevice factory");
  AudioComponentDescription description{'aumu','RvPS','Ravt',0,0};
  auto component=AudioComponentRegister(&description,CFSTR("RAVET: Performance Sine"),256,factory);require(component,"in-process registration");
  for(UInt32 channels: {1u,2u}){
   AudioUnit unit=nullptr;okay(AudioComponentInstanceNew(component,&unit),"instantiate MusicDevice");
   UInt32 inputs=99,size=sizeof(inputs);okay(AudioUnitGetProperty(unit,kAudioUnitProperty_ElementCount,kAudioUnitScope_Input,0,&inputs,&size),"query audio inputs");require(!inputs,"zero input instrument");
   AudioStreamBasicDescription format{};format.mSampleRate=48000.;format.mFormatID=kAudioFormatLinearPCM;format.mFormatFlags=kAudioFormatFlagsNativeFloatPacked|kAudioFormatFlagIsNonInterleaved;format.mBytesPerPacket=format.mBytesPerFrame=4;format.mFramesPerPacket=1;format.mChannelsPerFrame=channels;format.mBitsPerChannel=32;
   okay(AudioUnitSetProperty(unit,kAudioUnitProperty_StreamFormat,kAudioUnitScope_Output,0,&format,sizeof(format)),"mono/stereo output format");
   auto invalidFormat=format;invalidFormat.mBitsPerChannel=64;invalidFormat.mBytesPerPacket=invalidFormat.mBytesPerFrame=8;
   require(AudioUnitSetProperty(unit,kAudioUnitProperty_StreamFormat,kAudioUnitScope_Output,0,&invalidFormat,sizeof(invalidFormat))!=noErr,"f64 format rejected before configuration");
   invalidFormat=format;invalidFormat.mSampleRate=std::numeric_limits<double>::quiet_NaN();require(AudioUnitSetProperty(unit,kAudioUnitProperty_StreamFormat,kAudioUnitScope_Output,0,&invalidFormat,sizeof(invalidFormat))!=noErr,"nonfinite rate rejected");
   if(channels==2){invalidFormat=format;invalidFormat.mFormatFlags=kAudioFormatFlagsNativeFloatPacked;invalidFormat.mBytesPerPacket=invalidFormat.mBytesPerFrame=8;require(AudioUnitSetProperty(unit,kAudioUnitProperty_StreamFormat,kAudioUnitScope_Output,0,&invalidFormat,sizeof(invalidFormat))!=noErr,"interleaved stereo rejected");}
   UInt32 previousMaximum=0;size=sizeof(previousMaximum);okay(AudioUnitGetProperty(unit,kAudioUnitProperty_MaximumFramesPerSlice,kAudioUnitScope_Global,0,&previousMaximum,&size),"initial preparation capacity");
   for(UInt32 invalidMaximum: {0u,65537u,std::numeric_limits<UInt32>::max()})require(AudioUnitSetProperty(unit,kAudioUnitProperty_MaximumFramesPerSlice,kAudioUnitScope_Global,0,&invalidMaximum,sizeof(invalidMaximum))!=noErr,"invalid preparation rejected");
   UInt32 retainedMaximum=0;size=sizeof(retainedMaximum);okay(AudioUnitGetProperty(unit,kAudioUnitProperty_MaximumFramesPerSlice,kAudioUnitScope_Global,0,&retainedMaximum,&size),"capacity after rejection");require(retainedMaximum==previousMaximum,"invalid preparation does not mutate SDK capacity");
   UInt32 maximum=4096;okay(AudioUnitSetProperty(unit,kAudioUnitProperty_MaximumFramesPerSlice,kAudioUnitScope_Global,0,&maximum,sizeof(maximum)),"prepared capacity");
   AudioUnitParameterInfo info{};size=sizeof(info);okay(AudioUnitGetProperty(unit,kAudioUnitProperty_ParameterInfo,kAudioUnitScope_Global,1,&info,&size),"Level schema");require(info.minValue==0.f&&info.maxValue==1.f&&info.defaultValue==.2f,"normalized complete Level metadata");
   require(info.cfNameString && CFEqual(info.cfNameString,CFSTR("Level")) && (info.flags&kAudioUnitParameterFlag_IsWritable),"native cached complete AU name/role");
   CFRelease(info.cfNameString);if(info.unitName)CFRelease(info.unitName);
   std::array<AudioUnitParameterID,2> listed{77,77};size=sizeof(AudioUnitParameterID);okay(AudioUnitGetProperty(unit,kAudioUnitProperty_ParameterList,kAudioUnitScope_Global,0,listed.data(),&size),"native schema ID list");require(size==4&&listed[0]==1&&listed[1]==77,"complete schema list and tail");
   AudioUnitParameterValue physicalIntent=.375f;AudioUnitParameterStringFromValue formatted{1,&physicalIntent,nullptr};size=sizeof(formatted);
   okay(AudioUnitGetProperty(unit,kAudioUnitProperty_ParameterStringFromValue,kAudioUnitScope_Global,0,&formatted,&size),"cached physical AU text");require(formatted.outString&&CFEqual(formatted.outString,CFSTR("0.375")),"physical text value");CFRelease(formatted.outString);
   AudioUnitParameterValueFromString parsed{1,CFSTR(" +0.375 \n"),77.f};size=sizeof(parsed);okay(AudioUnitGetProperty(unit,kAudioUnitProperty_ParameterValueFromString,kAudioUnitScope_Global,0,&parsed,&size),"cached complete AU text parsing");require(parsed.outValue==.375f,"parsed normalized f32");
   for(const auto badText: {CFSTR("nan"),CFSTR("0.375junk"),CFSTR("1.1"),CFSTR("+-0.375")}){
    parsed.inString=badText;parsed.outValue=77.f;size=sizeof(parsed);require(AudioUnitGetProperty(unit,kAudioUnitProperty_ParameterValueFromString,kAudioUnitScope_Global,0,&parsed,&size)!=noErr&&parsed.outValue==77.f,"failed AU parse preserves output");
   }
   physicalIntent=std::numeric_limits<float>::quiet_NaN();formatted.outString=CFSTR("sentinel");size=sizeof(formatted);require(AudioUnitGetProperty(unit,kAudioUnitProperty_ParameterStringFromValue,kAudioUnitScope_Global,0,&formatted,&size)!=noErr&&CFEqual(formatted.outString,CFSTR("sentinel")),"failed AU format preserves output");
   AudioUnitParameterValue unknown=77.f;require(AudioUnitGetParameter(unit,UINT32_MAX,kAudioUnitScope_Global,0,&unknown)!=noErr&&unknown==77.f,"unknown full-width AU ID preserves getter output");
   SInt32 protocol=0;size=sizeof(protocol);const auto protocolResult=AudioUnitGetProperty(unit,kAudioUnitProperty_AudioUnitMIDIProtocol,kAudioUnitScope_Global,0,&protocol,&size);
   require(protocolResult!=noErr || protocol==1,"does not advertise MIDI2 protocol");
   const auto rawGetProperty = static_cast<OSStatus (*)(AudioUnit, AudioUnitPropertyID, AudioUnitScope, AudioUnitElement, void *, UInt32 *)>(AudioUnitGetProperty);
   size=sizeof(protocol);require(rawGetProperty(unit,kAudioUnitProperty_AudioUnitMIDIProtocol,kAudioUnitScope_Global,0,nullptr,&size)!=noErr,"null property output rejected safely");
   okay(AudioUnitInitialize(unit),"initialize instrument");
   std::array<float,4097> left{},right{};AudioTimeStamp time{};time.mFlags=kAudioTimeStampSampleTimeValid;
   auto render=[&](UInt32 frames){left.fill(99.f);right.fill(99.f);Buffers buffers{channels,{{1,frames*4u,left.data()},{1,frames*4u,right.data()}}};AudioUnitRenderActionFlags flags=0;const auto result=AudioUnitRender(unit,&flags,&time,0,frames,reinterpret_cast<AudioBufferList *>(&buffers));time.mSampleTime+=frames;if(!result){require(left[frames]==99.f&&right[frames]==99.f,"AU active prefix tails");if(channels==2)for(UInt32 i=0;i<frames;++i)require(left[i]==right[i],"center pan duplicates mono");}return result;};
   okay(MusicDeviceMIDIEvent(unit,0x90,69,64,3),"timed MIDI note input");
   require(AudioUnitSetParameter(unit,UINT32_MAX,kAudioUnitScope_Global,0,.5f,0)!=noErr,"unknown full-width AU setter rejected before queue mutation");
   require(AudioUnitSetParameter(unit,1,kAudioUnitScope_Global,1,.5f,0)!=noErr,"unknown AU element rejected before queue mutation");
   require(AudioUnitSetParameter(unit,1,kAudioUnitScope_Global,0,.5f,65536)!=noErr,"invalid parameter offset rejected before queue mutation");
   for(const auto &badWords: {std::array<UInt32,3>{0x190,69,127},{0x90,256,127},{0x90,69,256},{0x90,128,127},{0x00,69,127},{0xf4,0,0},{0xf9,0,0},{0xc0,1,1},{0xf8,1,0}})
    require(MusicDeviceMIDIEvent(unit,badWords[0],badWords[1],badWords[2],3)!=noErr,"malformed full-width MIDI preserves queued notes");
   for(const auto &validWords: {std::array<UInt32,3>{0xb0,64,127},{0xe0,0,64},{0xc0,1,0},{0xf2,1,2},{0xf1,1,0},{0xf8,0,0}})
    okay(MusicDeviceMIDIEvent(unit,validWords[0],validWords[1],validWords[2],3),"defined ignored short MIDI");
   okay(render(300),"timed render");
   for(UInt32 i=0;i<243;++i){if(i<3)require(left[i]==0.f,"leading sample silence");else{const auto n=i-3;const auto expected=std::sin(6.283185307179586*440./48000.*n)*(n+1)/240.*double(.2f)*double(64.f/127.f)/16.;require(std::abs(double(left[i])-expected)<1e-8,"AU native timing/frequency/velocity/attack");}}
   if(channels==2){
    left.fill(99.f);right.fill(99.f);Buffers aliased{2,{{1,256,left.data()},{1,256,left.data()}}};AudioUnitRenderActionFlags aliasFlags=0;
    require(AudioUnitRender(unit,&aliasFlags,&time,0,64,reinterpret_cast<AudioBufferList *>(&aliased))!=noErr,"actual AU prepared mapper rejects aliased output ranges");time.mSampleTime+=64;
    require(left[0]==0.f&&left[63]==0.f&&left[64]==99.f&&right[0]==99.f,"alias rejection silences supported caller prefix without unrelated/tail mutation");
    okay(render(64),"recover AU aliased-output rejection");
    for(UInt32 i=0;i<64;++i){const auto n=297+i;const auto decay=1.-double(n-239)*.2/480.;const auto expected=std::sin(6.283185307179586*440./48000.*n)*decay*double(.2f)*double(64.f/127.f)/16.;require(std::abs(double(left[i])-expected)<1e-8,"actual AU buffer rejection preserves native voice phase and decay");}
   }
   okay(MusicDeviceMIDIEvent(unit,0x80,69,0,0),"note release");okay(render(2401),"release render");require(left[2400]==0.f,"release completes");
   if(channels==2){
    okay(AudioUnitReset(unit,kAudioUnitScope_Global,0),"reset queued caller-preflight case");okay(MusicDeviceMIDIEvent(unit,0x94,69,127,3),"queue accepted note before malformed caller buffers");
    Buffers aliased{2,{{1,256,left.data()},{1,256,left.data()}}};AudioUnitRenderActionFlags flags=0;require(AudioUnitRender(unit,&flags,&time,0,64,reinterpret_cast<AudioBufferList *>(&aliased))!=noErr,"caller preflight rejects before queued-note consumption");time.mSampleTime+=64;
    okay(render(300),"deliver retained note after caller rejection");
    for(UInt32 i=0;i<243;++i){const auto n=i>=3?i-3:0;const auto expected=i<3?0.:std::sin(6.283185307179586*440./48000.*n)*(n+1)/240.*double(.2f)/16.;require(std::abs(double(left[i])-expected)<1e-8,"caller rejection retains accepted timed note");}
   }
   okay(AudioUnitReset(unit,kAudioUnitScope_Global,0),"reset transport-preflight case");
   auto transportRender=[&](UInt32 frames){if(!frames)time.mSampleTime+=1.;return render(frames);};
   TransportHost transportHost;HostCallbackInfo hostCallbacks{&transportHost,hostBeat,hostSignature,hostTransport,hostTransport2};
   auto setCallbacks=[&](){okay(AudioUnitSetProperty(unit,kAudioUnitProperty_HostCallbacks,kAudioUnitScope_Global,0,&hostCallbacks,sizeof(hostCallbacks)),"typed host callbacks");};setCallbacks();
   okay(MusicDeviceMIDIEvent(unit,0x94,69,127,3),"queue accepted note before malformed host transport");
   okay(AudioUnitSetParameter(unit,1,kAudioUnitScope_Global,0,.375f,8),"queue Level before malformed host transport");
   transportHost.sample=0x1p63;require(render(300)!=noErr,"actual AU +2^63 host timeline rejected before native processing");
   for(UInt32 i=0;i<300;++i)require(left[i]==0.f,"transport rejection silences supported caller prefix");
   require(left[300]==99.f&&right[300]==99.f&&transportHost.initialized&&transportHost.modernCalls==1&&!transportHost.legacyCalls,"transport rejection preserves tail and invokes initialized State2 once");
   transportHost.sample=-0x1p63;okay(render(300),"actual AU exact -2^63 recovery");
   for(UInt32 i=0;i<243;++i){const auto n=i>=3?i-3:0;const auto gain=i<8?.2f:.375f;const auto expected=i<3?0.:std::sin(6.283185307179586*440./48000.*n)*(n+1)/240.*double(gain)/16.;require(std::abs(double(left[i])-expected)<1e-8,"malformed transport retains accepted note and timed native Level intent");}
   require(transportHost.beatCalls==2&&transportHost.signatureCalls==2&&transportHost.modernCalls==2&&!transportHost.legacyCalls,"actual component callback counts and precedence");
   transportHost.numerator=3.5f;require(transportRender(0)!=noErr,"actual AU fractional signature preflight");transportHost.numerator=7.f;
   transportHost.missing=true;require(transportRender(0)!=noErr,"actual AU callback success without initialized fields rejected");transportHost.missing=false;
   transportHost.status=kAudioUnitErr_CannotDoInCurrentContext;okay(transportRender(0),"unavailable host tuples produce no invented context");require(!transportHost.legacyCalls,"State2 error has no legacy fallback");transportHost.status=noErr;
   hostCallbacks.transportStateProc2=nullptr;setCallbacks();okay(transportRender(0),"actual legacy transport callback");require(transportHost.legacyCalls==1&&transportHost.initialized,"actual legacy outputs initialized");
   hostCallbacks={};setCallbacks();okay(AudioUnitSetParameter(unit,1,kAudioUnitScope_Global,0,.2f,0),"restore Level after transport fixture");okay(transportRender(0),"absent callbacks accept wholly unknown timeline");
   okay(AudioUnitReset(unit,kAudioUnitScope_Global,0),"reset voices");
   okay(MusicDeviceMIDIEvent(unit,0x91,69,127,0),"channel one note");okay(MusicDeviceMIDIEvent(unit,0x92,69,127,0),"same key different channel");okay(render(64),"independent MIDI voices");
   okay(MusicDeviceMIDIEvent(unit,0x81,69,0,0),"release channel one");okay(render(2500),"independent channel render");require(std::abs(left[2499])>0.f,"different MIDI channel survives");
   okay(MusicDeviceMIDIEvent(unit,0x82,69,0,0),"release second channel");okay(render(2401),"all voices released");require(left[2400]==0.f,"last channel release");
   okay(AudioUnitReset(unit,kAudioUnitScope_Global,0),"reset before identity note");
   MusicDeviceStdNoteParams parameters{2,69.5f,63.123456f};NoteInstanceID identity=999;
   okay(MusicDeviceStartNote(unit,kMusicNoteEvent_UseGroupInstrument,7,&identity,3,reinterpret_cast<const MusicDeviceNoteParams *>(&parameters)),"fractional-pitch identified StartNote");require(identity!=999&&identity<=0x7fffffff,"bounded positive native note identity");
   okay(render(200),"identified fractional render");
   for(UInt32 i=4;i<180;++i){const auto n=i-3;const auto expected=std::sin(6.283185307179586*(440.*std::pow(2.,.5/12.))/48000.*n)*(n+1)/240.*double(.2f)*double(parameters.mVelocity/127.f)/16.;require(std::abs(double(left[i])-expected)<1e-8,"fractional pitch and unquantized AU velocity");}
   require(MusicDeviceStopNote(unit,6,identity,0)!=noErr,"wrong identified group rejected");
   okay(MusicDeviceStopNote(unit,7,identity,0),"identified StopNote");okay(render(2401),"identified release");require(left[2400]==0.f,"identified note completed");
   NoteInstanceID sentinel=777;parameters.mPitch=std::numeric_limits<float>::quiet_NaN();require(MusicDeviceStartNote(unit,0,7,&sentinel,0,reinterpret_cast<const MusicDeviceNoteParams *>(&parameters))!=noErr&&sentinel==777,"invalid StartNote atomic output");
   require(AudioUnitSetParameter(unit,1,kAudioUnitScope_Global,0,std::numeric_limits<float>::quiet_NaN(),0)!=noErr,"nonfinite Level rejected");
   require(AudioUnitSetParameter(unit,1,kAudioUnitScope_Global,0,2.f,0)!=noErr,"out-of-range Level rejected");
   okay(AudioUnitReset(unit,kAudioUnitScope_Global,0),"reset before same-frame order");
   okay(MusicDeviceMIDIEvent(unit,0x90,69,127,0),"same-frame on");okay(MusicDeviceMIDIEvent(unit,0x90,69,0,0),"MIDI zero velocity off");okay(render(64),"same-frame release");for(UInt32 i=0;i<64;++i)require(left[i]==0.f,"MIDI zero convention and source order");
   okay(MusicDeviceMIDIEvent(unit,0x90,69,0,0),"same-frame reverse off");okay(MusicDeviceMIDIEvent(unit,0x90,69,127,0),"same-frame reverse on");okay(AudioUnitSetParameter(unit,1,kAudioUnitScope_Global,0,.5f,8),"sample-offset Level");okay(render(64),"timed Level render");
   for(UInt32 i=1;i<64;++i){const auto gain=i<8?.2f:.5f;const auto expected=std::sin(6.283185307179586*440./48000.*i)*(i+1)/240.*double(gain)/16.;require(std::abs(double(left[i])-expected)<1e-8,"sample-timed global Level");}
   CFPropertyListRef saved=nullptr;size=sizeof(saved);okay(AudioUnitGetProperty(unit,kAudioUnitProperty_ClassInfo,kAudioUnitScope_Global,0,&saved,&size),"complete Level state save");require(saved&&CFGetTypeID(saved)==CFDictionaryGetTypeID(),"state dictionary");
   const auto dictionary=static_cast<CFDictionaryRef>(saved);const auto processor=static_cast<CFDataRef>(CFDictionaryGetValue(dictionary,CFSTR("Processor State")));const auto controller=static_cast<CFDataRef>(CFDictionaryGetValue(dictionary,CFSTR("Controller State")));require(processor&&controller&&CFEqual(processor,controller)&&CFDataGetLength(processor)==24&&!std::memcmp(CFDataGetBytePtr(processor),"RAVETPS\0",8),"coherent canonical Level state");
   okay(AudioUnitSetParameter(unit,1,kAudioUnitScope_Global,0,.25f,0),"change Level");okay(render(64),"changed Level render");
   okay(AudioUnitSetProperty(unit,kAudioUnitProperty_ClassInfo,kAudioUnitScope_Global,0,&saved,sizeof(saved)),"complete Level restore");okay(render(64),"restored Level render");
   AudioUnitParameterValue value=0;okay(AudioUnitGetParameter(unit,1,kAudioUnitScope_Global,0,&value),"restored host parameter");require(value==.5f,"saved Level recalled");
   auto malformed=CFDictionaryCreateMutableCopy(nullptr,0,dictionary);std::array<UInt8,24> bad{};std::memcpy(bad.data(),CFDataGetBytePtr(processor),bad.size());bad[0]^=1;auto badData=CFDataCreate(nullptr,bad.data(),bad.size());CFDictionarySetValue(malformed,CFSTR("Processor State"),badData);CFRelease(badData);
   auto malformedState=static_cast<CFPropertyListRef>(malformed);require(AudioUnitSetProperty(unit,kAudioUnitProperty_ClassInfo,kAudioUnitScope_Global,0,&malformedState,sizeof(malformedState))!=noErr,"malformed complete state rejected");
   okay(AudioUnitGetParameter(unit,1,kAudioUnitScope_Global,0,&value),"parameter after rejected recall");require(value==.5f,"malformed recall preserves host Level");CFRelease(malformed);
   for(const auto key: {CFSTR("version"),CFSTR("subtype"),CFSTR("manufacturer")}){
    malformed=CFDictionaryCreateMutableCopy(nullptr,0,dictionary);SInt32 invalid=42;const auto number=CFNumberCreate(nullptr,kCFNumberSInt32Type,&invalid);CFDictionarySetValue(malformed,key,number);CFRelease(number);malformedState=static_cast<CFPropertyListRef>(malformed);
    require(AudioUnitSetProperty(unit,kAudioUnitProperty_ClassInfoFromDocument,kAudioUnitScope_Global,0,&malformedState,sizeof(malformedState))!=noErr,"wrong AU dictionary identity rejected before SDK mutation");CFRelease(malformed);
   }
   malformed=CFDictionaryCreateMutableCopy(nullptr,0,dictionary);CFDictionarySetValue(malformed,CFSTR("Controller State"),CFSTR("invalid"));malformedState=static_cast<CFPropertyListRef>(malformed);
   require(AudioUnitSetProperty(unit,kAudioUnitProperty_ClassInfo,kAudioUnitScope_Global,0,&malformedState,sizeof(malformedState))!=noErr,"wrong controller stream type rejected");CFRelease(malformed);
   okay(AudioUnitGetParameter(unit,1,kAudioUnitScope_Global,0,&value),"parameter after rejected dictionary");require(value==.5f,"malformed envelope preserves Level");CFRelease(saved);
   okay(AudioUnitSetParameter(unit,1,kAudioUnitScope_Global,0,.6f,0),"zero-frame Level intent");okay(render(0),"zero-frame Level delivery");
   okay(AudioUnitGetParameter(unit,1,kAudioUnitScope_Global,0,&value),"zero-frame Level reflection");require(value==.6f,"zero-frame host Level");
   okay(AudioUnitReset(unit,kAudioUnitScope_Global,0),"capacity reset");for(UInt32 i=0;i<1023;++i)okay(MusicDeviceMIDIEvent(unit,0x90,i%128,127,0),"bounded MIDI queue");require(MusicDeviceMIDIEvent(unit,0x90,69,127,0)!=noErr,"queue capacity rejection");okay(render(64),"maximum bounded queue render");
   okay(AudioUnitReset(unit,kAudioUnitScope_Global,0),"reset invalid timestamp queue");okay(MusicDeviceMIDIEvent(unit,0x90,69,127,100),"queued future event");require(render(64)!=noErr,"offset outside active slice rejected");for(UInt32 i=0;i<64;++i)require(left[i]==0.f,"rejected queued offset silences supported caller buffers");require(left[64]==99.f && right[64]==99.f,"rejected queue preserves tails");okay(render(64),"recover rejected queue");for(UInt32 i=0;i<64;++i)require(left[i]==0.f,"invalid queued offset preserved voice history");
   left.fill(99.f);right.fill(99.f);Buffers oversized{channels,{{1,4097*4,left.data()},{1,4097*4,right.data()}}};AudioUnitRenderActionFlags oversizedFlags=0;
   require(AudioUnitRender(unit,&oversizedFlags,&time,0,4097,reinterpret_cast<AudioBufferList *>(&oversized))!=noErr,"AUBase excessive-slice rejection");
   require(left[0]==0.f && left[4095]==0.f && left[4096]==99.f,"excessive slice silences supported prepared prefix and preserves tail");
   const UInt8 sysex[]{0xf0,1,0xf7};require(MusicDeviceSysEx(unit,sysex,sizeof(sysex))!=noErr,"SysEx explicitly unsupported");
   for(UInt32 cycle=0;cycle<100;++cycle){okay(AudioUnitReset(unit,kAudioUnitScope_Global,0),"cycle reset");okay(MusicDeviceMIDIEvent(unit,0x9f,127,127,3),"cycle note");okay(render(64),"cycle render");require(std::abs(left[63])>0.f,"cycle audible");okay(MusicDeviceMIDIEvent(unit,0x8f,127,64,0),"cycle release");okay(render(2401),"cycle release render");require(left[2400]==0.f,"cycle completion");}
   okay(AudioUnitUninitialize(unit),"uninitialize");okay(AudioComponentInstanceDispose(unit),"dispose");
  }
  CFRelease(bundle);std::cout<<"AUv2 actual native performance bundle: mono/stereo MIDI timing/identity, Level automation/coherent state, bounded host transport/audio faults/recovery and 200 synthetic voice cycles passed\n";
 }catch(const std::exception &error){std::cerr<<error.what()<<'\n';return 1;}
}
