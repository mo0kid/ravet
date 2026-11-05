#import <Cocoa/Cocoa.h>
#import <AudioUnit/AUCocoaUIView.h>
#include <AudioToolbox/AudioToolbox.h>
#include <CoreMIDI/CoreMIDI.h>
#include <array>
#include <cmath>
#include <cstring>
#include <iostream>
#include <stdexcept>
#include <thread>
#include <vector>
#include <atomic>
#include <dlfcn.h>
#include <exception>

static void require(bool condition, const char *message) { if (!condition) throw std::runtime_error(message); }
static void okay(OSStatus status, const char *message) {
    if (status) { std::cerr << message << ": " << status << '\n'; throw std::runtime_error(message); }
}
static OSStatus input(void *, AudioUnitRenderActionFlags *, const AudioTimeStamp *, UInt32, UInt32 frames, AudioBufferList *buffers) {
    for (UInt32 c = 0; c < buffers->mNumberBuffers; ++c) std::fill_n(static_cast<float *>(buffers->mBuffers[c].mData), frames, c ? -.5f : .25f);
    return noErr;
}
static void pump(double seconds) { [[NSRunLoop mainRunLoop] runUntilDate:[NSDate dateWithTimeIntervalSinceNow:seconds]]; }
static OSStatus beat(void *, Float64 *position, Float64 *tempo) { *position = 5; *tempo = 123; return noErr; }
// Like REAPER 7, report the whole sample 456 with one ulp of Float64 residue;
// products (and the mock's transport check) must receive exactly 456.
static OSStatus transport(void *, Boolean *playing, Boolean *changed, Float64 *samples, Boolean *cycling, Float64 *start, Float64 *end) {
    *playing = true; *changed = false; *samples = std::nextafter(456., 1e300); *cycling = false; *start = 0; *end = 0; return noErr;
}
struct Events { uint32_t begin = 0, value = 0, end = 0, latency = 0, midi = 0; };
static void event(void *data, void *, const AudioUnitEvent *e, UInt64, AudioUnitParameterValue) {
    auto &events = *static_cast<Events *>(data);
    if (e->mEventType == kAudioUnitEvent_BeginParameterChangeGesture) ++events.begin;
    if (e->mEventType == kAudioUnitEvent_ParameterValueChange) ++events.value;
    if (e->mEventType == kAudioUnitEvent_EndParameterChangeGesture) ++events.end;
}
static void property(void *data, AudioUnit, AudioUnitPropertyID, AudioUnitScope, AudioUnitElement) { ++static_cast<Events *>(data)->latency; }
static OSStatus midi(void *data, const AudioTimeStamp *, UInt32, const MIDIPacketList *list) {
    auto &events = *static_cast<Events *>(data);
    require(list->numPackets == 1 && list->packet[0].timeStamp == 7 && list->packet[0].length == 3 &&
        list->packet[0].data[0] == 0x90 && list->packet[0].data[1] == 60 && list->packet[0].data[2] == 100, "MIDI output bytes/offset");
    ++events.midi; return noErr;
}
struct MidiRecord { uint64_t offset; std::vector<uint8_t> bytes; };
static OSStatus synthMidi(void *data, const AudioTimeStamp *, UInt32, const MIDIPacketList *list) {
    auto &records = *static_cast<std::vector<MidiRecord> *>(data);
    const auto *packet = &list->packet[0];
    for (UInt32 i = 0; i < list->numPackets; ++i) {
        records.push_back({packet->timeStamp, {packet->data, packet->data + packet->length}});
        packet = MIDIPacketNext(packet);
    }
    return noErr;
}
int main(int argc, char **argv) {
    @autoreleasepool { try {
        require(argc == 2 || argc == 3, "usage: contracts component-path [real]");
        const bool real = argc == 3;
        auto url = CFURLCreateFromFileSystemRepresentation(nullptr, reinterpret_cast<const UInt8 *>(argv[1]), std::strlen(argv[1]), true);
        auto bundle = CFBundleCreate(nullptr, url); CFRelease(url);
        require(bundle && CFBundleLoadExecutable(bundle), "load component");
        const auto factory = reinterpret_cast<AudioComponentFactoryFunction>(CFBundleGetFunctionPointerForName(bundle, CFSTR("RavetAUFactory")));
        require(factory, "native factory");
        const auto info = CFBundleGetInfoDictionary(bundle);
        const auto components = static_cast<CFArrayRef>(CFDictionaryGetValue(info, CFSTR("AudioComponents")));
        const auto componentInfo = static_cast<CFDictionaryRef>(CFArrayGetValueAtIndex(components, 0));
        auto code = [&](CFStringRef key) {
            char text[8]{}; require(CFStringGetCString(static_cast<CFStringRef>(CFDictionaryGetValue(componentInfo, key)), text, sizeof(text), kCFStringEncodingASCII), "fourcc");
            return (uint32_t(text[0]) << 24) | (uint32_t(text[1]) << 16) | (uint32_t(text[2]) << 8) | uint32_t(text[3]);
        };
        AudioComponentDescription description{code(CFSTR("type")), code(CFSTR("subtype")), code(CFSTR("manufacturer")), 0, 0};
        auto component = AudioComponentRegister(&description, CFSTR("RAVET AU TEST in-process"), 256, factory);
        require(component, "register native factory");
        const bool evidenceSynth = real && CFEqual(CFDictionaryGetValue(componentInfo,CFSTR("name")),CFSTR("RAVET: RAVET Evidence Synth"));
        Events events;
        // Retain a Cocoa view across each actual AU disposal; its callbacks and
        // timer run again afterwards. This is the release-plan lifetime case.
        for (int cycle = 0; cycle < 3; ++cycle) {
            AudioUnit unit = nullptr; okay(AudioComponentInstanceNew(component, &unit), "create");
            UInt32 size = 0; Boolean writable = false;
            okay(AudioUnitGetPropertyInfo(unit, kAudioUnitProperty_ParameterList, kAudioUnitScope_Global, 0, &size, &writable), "parameter list info");
            std::vector<AudioUnitParameterID> ids(size / sizeof(AudioUnitParameterID));
            okay(AudioUnitGetProperty(unit, kAudioUnitProperty_ParameterList, kAudioUnitScope_Global, 0, ids.data(), &size), "parameter list");
            require(!ids.empty(), "product IDs");
            if (!real) require(ids == std::vector<AudioUnitParameterID>({101, 707, 999}), "permanent host IDs");
            const uint32_t gain = real ? ids[0] : 101;
            AudioUnitParameterInfo parameterInfo{}; size = sizeof(parameterInfo);
            okay(AudioUnitGetProperty(unit, kAudioUnitProperty_ParameterInfo, kAudioUnitScope_Global, gain, &parameterInfo, &size), "parameter metadata");
            if (!real) require(parameterInfo.minValue == 0 && parameterInfo.maxValue == 2 && parameterInfo.defaultValue == 1, "physical parameter range");
            if (parameterInfo.cfNameString) CFRelease(parameterInfo.cfNameString);
            if (parameterInfo.unitName) CFRelease(parameterInfo.unitName);
            if (!real) {
                CFArrayRef strings = nullptr; size = sizeof(strings);
                okay(AudioUnitGetProperty(unit, kAudioUnitProperty_ParameterValueStrings, kAudioUnitScope_Global, 999, &strings, &size), "indexed strings");
                require(CFArrayGetCount(strings) == 3 && CFEqual(CFArrayGetValueAtIndex(strings, 1), CFSTR("Second")), "product value strings"); CFRelease(strings);
                AudioUnitParameterValue value = 2;
                AudioUnitParameterStringFromValue request{999, &value, nullptr}; size = sizeof(request);
                okay(AudioUnitGetProperty(unit, kAudioUnitProperty_ParameterStringFromValue, kAudioUnitScope_Global, 0, &request, &size), "value to string");
                require(CFEqual(request.outString, CFSTR("Third")), "product text"); CFRelease(request.outString);
                AudioUnitParameterValueFromString parse{999, CFSTR("Second"), 0}; size = sizeof(parse);
                okay(AudioUnitGetProperty(unit, kAudioUnitProperty_ParameterValueFromString, kAudioUnitScope_Global, 0, &parse, &size), "string to value");
                require(parse.outValue == 1, "product text parsing");
                require(AudioUnitSetParameter(unit, 123456, kAudioUnitScope_Global, 0, 1, 0) != noErr, "unknown parameter rejects");
                require(AudioUnitSetParameter(unit, 101, kAudioUnitScope_Global, 0, NAN, 0) != noErr, "nonfinite rejects");
            }
            AudioStreamBasicDescription format{}; format.mSampleRate = 48000.; format.mFormatID = kAudioFormatLinearPCM;
            format.mFormatFlags = UInt32(kAudioFormatFlagsNativeFloatPacked) | UInt32(kAudioFormatFlagIsNonInterleaved);
            format.mBytesPerPacket = format.mBytesPerFrame = 4; format.mFramesPerPacket = 1; format.mChannelsPerFrame = 2; format.mBitsPerChannel = 32;
            const bool instrument = description.componentType == kAudioUnitType_MusicDevice;
            if (!instrument) {
                okay(AudioUnitSetProperty(unit, kAudioUnitProperty_StreamFormat, kAudioUnitScope_Input, 0, &format, sizeof(format)), "input format");
                AURenderCallbackStruct callback{input, nullptr};
                okay(AudioUnitSetProperty(unit, kAudioUnitProperty_SetRenderCallback, kAudioUnitScope_Input, 0, &callback, sizeof(callback)), "input callback");
            }
            okay(AudioUnitSetProperty(unit, kAudioUnitProperty_StreamFormat, kAudioUnitScope_Output, 0, &format, sizeof(format)), "output format");
            UInt32 maximum = 64;
            okay(AudioUnitSetProperty(unit, kAudioUnitProperty_MaximumFramesPerSlice, kAudioUnitScope_Global, 0, &maximum, sizeof(maximum)), "maximum frames");
            std::vector<MidiRecord> synthOutput;
            if (evidenceSynth && instrument) {
                AUMIDIOutputCallbackStruct callback{synthMidi, &synthOutput};
                okay(AudioUnitSetProperty(unit, kAudioUnitProperty_MIDIOutputCallback, kAudioUnitScope_Global, 0, &callback, sizeof(callback)), "synth output callback");
            }
            if (!real && description.componentType != kAudioUnitType_Effect) {
                AUMIDIOutputCallbackStruct callback{midi, &events};
                okay(AudioUnitSetProperty(unit, kAudioUnitProperty_MIDIOutputCallback, kAudioUnitScope_Global, 0, &callback, sizeof(callback)), "MIDI output callback");
            }
            okay(AudioUnitInitialize(unit), "prepare native product");
            HostCallbackInfo host{}; host.beatAndTempoProc = beat; host.transportStateProc = transport;
            okay(AudioUnitSetProperty(unit, kAudioUnitProperty_HostCallbacks, kAudioUnitScope_Global, 0, &host, sizeof(host)), "host transport callbacks");
            std::array<float, 65> left{}, right{};
            struct Buffers { UInt32 count; AudioBuffer buffers[2]; } buffers{2, {{1, 256, left.data()}, {1, 256, right.data()}}};
            AudioTimeStamp time{}; time.mFlags = kAudioTimeStampSampleTimeValid;
            auto render = [&] {
                left.fill(99); right.fill(99); AudioUnitRenderActionFlags flags = 0;
                okay(AudioUnitRender(unit, &flags, &time, 0, 64, reinterpret_cast<AudioBufferList *>(&buffers)), "native render");
                time.mSampleTime += 64;
                require(left[64] == 99 && right[64] == 99, "buffer tails untouched");
                for (int i = 0; i < 64; ++i) require(std::isfinite(left[i]) && std::isfinite(right[i]), "finite audio");
            };
            render();
            if (!real) {
                okay(AudioUnitSetParameter(unit, 101, kAudioUnitScope_Global, 0, .5, 0), "main cache set");
                CFPropertyListRef saved = nullptr; size = sizeof(saved);
                okay(AudioUnitGetProperty(unit, kAudioUnitProperty_ClassInfo, kAudioUnitScope_Global, 0, &saved, &size), "save before render");
                okay(AudioUnitSetParameter(unit, 101, kAudioUnitScope_Global, 0, 1, 0), "alternate value");
                const auto malformed = CFDictionaryCreateMutableCopy(nullptr, 0, static_cast<CFDictionaryRef>(saved));
                const UInt8 bad = 0; const auto badData = CFDataCreate(nullptr, &bad, 1);
                CFDictionarySetValue(malformed, CFSTR("RAVET"), badData); CFRelease(badData);
                CFPropertyListRef badState = malformed;
                require(AudioUnitSetProperty(unit, kAudioUnitProperty_ClassInfo, kAudioUnitScope_Global, 0, &badState, sizeof(badState)) != noErr, "malformed envelope rejects");
                CFRelease(malformed);
                AudioUnitParameterValue retainedValue = 0;
                okay(AudioUnitGetParameter(unit, 101, kAudioUnitScope_Global, 0, &retainedValue), "value after malformed state");
                require(retainedValue == 1, "malformed state preserves cache");
                UInt32 bypass = 1;
                okay(AudioUnitSetProperty(unit, kAudioUnitProperty_BypassEffect, kAudioUnitScope_Global, 0, &bypass, sizeof(bypass)), "bypass");
                render(); require(left[0] == .25, "bypass dry");
                okay(AudioUnitSetProperty(unit, kAudioUnitProperty_ClassInfo, kAudioUnitScope_Global, 0, &saved, sizeof(saved)), "load envelope"); CFRelease(saved);
                size = sizeof(bypass); okay(AudioUnitGetProperty(unit, kAudioUnitProperty_BypassEffect, kAudioUnitScope_Global, 0, &bypass, &size), "bypass restored");
                require(!bypass, "complete state restores bypass");
                render(); require(left[0] == .125, "state included cached host set before audio");
                AudioUnitParameterEvent scheduled{}; scheduled.scope = kAudioUnitScope_Global; scheduled.parameter = 101;
                scheduled.eventType = kParameterEvent_Immediate; scheduled.eventValues.immediate = {16, 2};
                okay(AudioUnitScheduleParameters(unit, &scheduled, 1), "timed parameter");
                render(); require(left[15] == .125 && left[16] == .5, "sample offset preserved");
                std::array<AudioUnitParameterEvent, 2> invalid{scheduled, scheduled};
                invalid[0].eventValues.immediate = {0, .25}; invalid[1].parameter = 123456;
                require(AudioUnitScheduleParameters(unit, invalid.data(), invalid.size()) != noErr, "late invalid schedule rejects entire list");
                render(); require(left[0] == .5, "failed schedule commits no prefix");
                scheduled.eventType = kParameterEvent_Ramped;
                scheduled.eventValues.ramp = {32, 32, 2, 0};
                okay(AudioUnitScheduleParameters(unit, &scheduled, 1), "ramped parameter");
                render(); require(left[31] == .5 && left[32] == .5 && left[63] == .015625, "ramp samples and offset");
                render(); require(left[0] == 0 && left[63] == 0, "ramp endpoint held in next block");
                scheduled.eventType = kParameterEvent_Immediate;
                okay(AudioUnitAddPropertyListener(unit, kAudioUnitProperty_Latency, property, &events), "latency listener");
                scheduled.parameter = 999; scheduled.eventValues.immediate = {0, 2};
                okay(AudioUnitScheduleParameters(unit, &scheduled, 1), "timing change"); render(); pump(.08);
                Float64 latency = 0, tail = 0; size = sizeof(latency);
                okay(AudioUnitGetProperty(unit, kAudioUnitProperty_Latency, kAudioUnitScope_Global, 0, &latency, &size), "latency"); size = sizeof(tail);
                okay(AudioUnitGetProperty(unit, kAudioUnitProperty_TailTime, kAudioUnitScope_Global, 0, &tail, &size), "tail");
                require(latency == .002 && tail == .01 && events.latency > 0, "timing and notification");
                if (description.componentType != kAudioUnitType_Effect) {
                    const auto before = events.midi;
                    okay(MusicDeviceMIDIEvent(unit, 0x90, 60, 100, 7), "MIDI input"); render();
                    require(events.midi == before + 1, "MIDI loopback");
                }
            }
            if (evidenceSynth && instrument) {
                require(ids == std::vector<AudioUnitParameterID>({1, 2}), "evidence synth controls");
                okay(MusicDeviceMIDIEvent(unit, 0xb0, 1, 64, 0), "synth CC input");
                okay(MusicDeviceMIDIEvent(unit, 0x90, 60, 100, 7), "synth note input");
                okay(MusicDeviceMIDIEvent(unit, 0x80, 60, 0, 15), "synth note off input");
                render();
                const float amplitude = (100.f/127.f)*.25f*(64.f/127.f);
                for (int i=0; i<64; ++i) require(std::fabs(left[i] - (i < 39 ? 0.f : (i%2 ? amplitude : -amplitude))) < 1e-7, "synth exact onset/CC");
                render();
                for (int i=0; i<64; ++i) require(std::fabs(left[i] - (i < 39 ? (i%2 ? amplitude : -amplitude) : 0.f)) < 1e-7, "synth finite tail");
                require(synthOutput.size()==3 && synthOutput[0].offset==0 && synthOutput[0].bytes==std::vector<uint8_t>({0xb0,1,64}) &&
                    synthOutput[1].offset==7 && synthOutput[1].bytes==std::vector<uint8_t>({0x90,60,100}) &&
                    synthOutput[2].offset==15 && synthOutput[2].bytes==std::vector<uint8_t>({0x80,60,0}), "synth MIDI output bytes/offsets");
                Float64 latency=0,tail=0; size=sizeof(latency);
                okay(AudioUnitGetProperty(unit,kAudioUnitProperty_Latency,kAudioUnitScope_Global,0,&latency,&size),"synth latency"); size=sizeof(tail);
                okay(AudioUnitGetProperty(unit,kAudioUnitProperty_TailTime,kAudioUnitScope_Global,0,&tail,&size),"synth tail");
                require(latency==32./48000. && tail==96./48000.,"synth timing");
                const auto notifications=events.latency;
                okay(AudioUnitAddPropertyListener(unit,kAudioUnitProperty_Latency,property,&events),"synth latency listener");
                okay(AudioUnitSetParameter(unit,2,kAudioUnitScope_Global,0,0,0),"synth latency change"); render(); pump(.08);
                size=sizeof(latency); okay(AudioUnitGetProperty(unit,kAudioUnitProperty_Latency,kAudioUnitScope_Global,0,&latency,&size),"new synth latency");
                require(latency==0 && events.latency>notifications,"synth latency notification");
                okay(AudioUnitUninitialize(unit),"synth uninitialize"); okay(AudioComponentInstanceDispose(unit),"synth dispose");
                std::cout << "PASS evidence synth AU: notes/CC, exact delayed audio/tail, MIDI bytes/offsets, latency notification cycle " << cycle << "\n";
                continue;
            }
            AUEventListenerRef listener = nullptr;
            okay(AUEventListenerCreate(event, &events, CFRunLoopGetMain(), kCFRunLoopDefaultMode, 0, 0, &listener), "automation listener");
            for (const auto kind : {kAudioUnitEvent_BeginParameterChangeGesture, kAudioUnitEvent_ParameterValueChange, kAudioUnitEvent_EndParameterChangeGesture}) {
                AudioUnitEvent e{}; e.mEventType = kind; e.mArgument.mParameter = {unit, gain, kAudioUnitScope_Global, 0};
                okay(AUEventListenerAddEventType(listener, &events, &e), "automation event subscription");
            }
            AudioUnitCocoaViewInfo viewInfo{}; size = sizeof(viewInfo);
            okay(AudioUnitGetProperty(unit, kAudioUnitProperty_CocoaUI, kAudioUnitScope_Global, 0, &viewInfo, &size), "Cocoa factory property");
            NSBundle *viewBundle = [NSBundle bundleWithURL:(__bridge NSURL *)viewInfo.mCocoaAUViewBundleLocation];
            require([viewBundle load], "load Cocoa bundle");
            Class cls = NSClassFromString((__bridge NSString *)viewInfo.mCocoaAUViewClass[0]);
            id<AUCocoaUIBase> cocoa = [[cls alloc] init];
            NSView *retained = [cocoa uiViewForAudioUnit:unit withSize:NSMakeSize(320, 180)];
            CFRelease(viewInfo.mCocoaAUViewBundleLocation); CFRelease(viewInfo.mCocoaAUViewClass[0]);
            require(retained, "open native editor"); pump(.15);
            if (!real) require(events.begin >= uint32_t(cycle + 1) && events.value >= uint32_t(cycle + 1) && events.end >= uint32_t(cycle + 1), "ordered editor gestures reach host");
            [retained setFrameSize:NSMakeSize(400, 200)];
            okay(AUListenerDispose(listener), "listener teardown");
            okay(AudioUnitUninitialize(unit), "deactivate");
            okay(AudioComponentInstanceDispose(unit), "dispose AU with retained view");
            [retained setFrameSize:NSMakeSize(420, 220)]; pump(.1); retained = nil; pump(.05);
        }
        if (!real) {
            // Mono negotiation and worker-thread control calls exercise separate
            // SDK lifecycles. MAIN table calls must still run on AppKit's thread.
            std::atomic<bool> done{false}; std::exception_ptr failure;
            std::thread worker([&] {
                try {
                    AudioUnit unit = nullptr; okay(AudioComponentInstanceNew(component, &unit), "worker create");
                    AudioStreamBasicDescription mono{}; mono.mSampleRate = 48000; mono.mFormatID = kAudioFormatLinearPCM;
                    mono.mFormatFlags = kAudioFormatFlagsNativeFloatPacked; mono.mBytesPerFrame = mono.mBytesPerPacket = 4;
                    mono.mFramesPerPacket = 1; mono.mBitsPerChannel = 32; mono.mChannelsPerFrame = 1;
                    if (description.componentType != kAudioUnitType_MusicDevice) {
                        okay(AudioUnitSetProperty(unit, kAudioUnitProperty_StreamFormat, kAudioUnitScope_Input, 0, &mono, sizeof(mono)), "mono input");
                        AURenderCallbackStruct callback{input, nullptr};
                        okay(AudioUnitSetProperty(unit, kAudioUnitProperty_SetRenderCallback, kAudioUnitScope_Input, 0, &callback, sizeof(callback)), "mono input callback");
                    }
                    okay(AudioUnitSetProperty(unit, kAudioUnitProperty_StreamFormat, kAudioUnitScope_Output, 0, &mono, sizeof(mono)), "mono output");
                    okay(AudioUnitInitialize(unit), "worker prepare");
                    std::array<float, 64> samples{}; AudioBufferList buffers{1, {{1, 256, samples.data()}}};
                    AudioUnitRenderActionFlags flags = 0; AudioTimeStamp time{}; time.mFlags = kAudioTimeStampSampleTimeValid;
                    okay(AudioUnitRender(unit, &flags, &time, 0, 64, &buffers), "mono render");
                    require(samples[0] == .25f, "mono product audio");
                    CFPropertyListRef state = nullptr; UInt32 size = sizeof(state);
                    okay(AudioUnitGetProperty(unit, kAudioUnitProperty_ClassInfo, kAudioUnitScope_Global, 0, &state, &size), "worker save");
                    okay(AudioUnitSetProperty(unit, kAudioUnitProperty_ClassInfo, kAudioUnitScope_Global, 0, &state, sizeof(state)), "worker load"); CFRelease(state);
                    okay(AudioUnitUninitialize(unit), "worker stop"); okay(AudioComponentInstanceDispose(unit), "worker dispose");
                } catch (...) { failure = std::current_exception(); }
                done.store(true, std::memory_order_release);
            });
            while (!done.load(std::memory_order_acquire)) pump(.01);
            worker.join(); if (failure) std::rethrow_exception(failure);
            NSBundle *nativeBundle = [NSBundle bundleWithPath:[NSString stringWithUTF8String:argv[1]]];
            const auto libraries = [[NSFileManager defaultManager] contentsOfDirectoryAtPath:nativeBundle.privateFrameworksPath error:nil];
            require(libraries.count == 1, "one embedded product library");
            void *library = dlopen([[nativeBundle.privateFrameworksPath stringByAppendingPathComponent:libraries[0]] fileSystemRepresentation], RTLD_NOW | RTLD_LOCAL);
            const auto stats = reinterpret_cast<void (*)(uint32_t *)>(dlsym(library, "ravet_au_mock_stats"));
            require(stats, "mock lifecycle evidence");
            uint32_t counts[6]{}; stats(counts);
            require(counts[0] == 0 && counts[1] == 3 && counts[2] == 3 && counts[4] == 0 && counts[5] > 0,
                "balanced view/instance lifetime, main-thread table calls and transported host context");
            const auto idles = counts[3]; pump(.1); stats(counts); require(counts[3] == idles, "no retired editor timer calls");
        }
        std::cout << "PASS generic AUv2 " << (real ? "product metadata/render (instrument MIDI/timing or effect editor lifetime)" :
            "mock metadata/stereo+mono/state/schedules/timing/transport/MIDI/editor lifetime/MAIN dispatch") << " (3 lifecycle cycles)\n";
        CFRelease(bundle); return 0;
    } catch (const std::exception &error) { std::cerr << "FAIL " << error.what() << '\n'; return 1; } }
}
