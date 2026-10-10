#import "GainAudioUnit.h"
#import <AudioToolbox/AUAudioUnitImplementation.h>
#import "RavetParameterViewController.h"
#import <TargetConditionals.h>
#include <array>
#include <cmath>
#include <cstdio>
#include <cstdlib>

static void check(bool condition, const char *message) {
    if (!condition) { std::fprintf(stderr, "FAIL: %s\n", message); std::exit(1); }
}
@interface MIDIUnit : RavetGainAudioUnit
@end
@implementation MIDIUnit
+ (BOOL)acceptsMIDI { return YES; }
@end
@interface Editor : RavetParameterViewController
@end
@implementation Editor
+ (Class)audioUnitClass { return RavetGainAudioUnit.class; }
@end
int ravet_auv3_contracts() { @autoreleasepool {
#if TARGET_OS_OSX
    [NSApplication sharedApplication];
#endif
    const AudioComponentDescription description{kAudioUnitType_Effect, 'RvG3', 'Ravt', 0, 0};
    [AUAudioUnit registerSubclass:[RavetGainAudioUnit class] asComponentDescription:description name:@"RAVET: Gain" version:1];
    for (unsigned cycle = 0; cycle < 3; ++cycle) {
        NSError *error = nil;
        RavetGainAudioUnit *unit = [[RavetGainAudioUnit alloc] initWithComponentDescription:description options:0 error:&error];
        check(unit != nil, "create");
        check(unit.parameterTree.allParameters.count == 2, "schema");
        check([unit.parameterTree parameterWithAddress:1].maxValue == 4, "physical gain");
        unit.maximumFramesToRender = 512;
        const bool prepared = [unit allocateRenderResourcesAndReturnError:&error];
        if (!prepared) std::fprintf(stderr, "%s\n", error.description.UTF8String);
        check(prepared, "prepare");
        auto block = unit.internalRenderBlock;
        std::array<float, 520> left{}, right{};
        left.fill(99); right.fill(99);
        struct { UInt32 count; AudioBuffer buffers[2]; } storage{2, {{1, sizeof(left), left.data()}, {1, sizeof(right), right.data()}}};
        auto output = reinterpret_cast<AudioBufferList *>(&storage);
        AudioTimeStamp timestamp{}; timestamp.mSampleTime = 1000; timestamp.mFlags = kAudioTimeStampSampleTimeValid;
        AudioUnitRenderActionFlags flags = 0;
        AURenderPullInputBlock pull = ^OSStatus(AudioUnitRenderActionFlags *, const AudioTimeStamp *, AUAudioFrameCount frames,
            NSInteger, AudioBufferList *input) {
            for (unsigned c = 0; c < 2; ++c) for (unsigned i = 0; i < frames; ++i) static_cast<float *>(input->mBuffers[c].mData)[i] = c ? -0.25f : 0.5f;
            return noErr;
        };
        check(block(&flags, &timestamp, 512, 0, output, nullptr, pull) == 0, "render");
        check(std::abs(left[511] - 0.5f) < 1e-6f && right[511] == -0.25f, "unity");
        check(left[512] == 99 && right[512] == 99, "tails");
        NSDictionary *saved = unit.fullState;
        [unit.parameterTree parameterWithAddress:1].value = 2;
        check(block(&flags, &timestamp, 512, 0, output, nullptr, pull) == 0 && left[511] > 0.99f, "control publication");
        AURenderEvent automated{}; automated.parameter.eventType = AURenderEventParameter;
        automated.parameter.eventSampleTime = 1000; automated.parameter.parameterAddress = 1; automated.parameter.value = 3;
        check(block(&flags, &timestamp, 512, 0, output, &automated, pull) == 0, "automation before unrefreshed save");
        NSDictionary *unrefreshed = unit.fullState;
        unit.fullState = saved;
        unit.fullState = unrefreshed;
        check(block(&flags, &timestamp, 512, 0, output, nullptr, pull) == 0 && left[511] == 1.5f, "native state wins cached AU values");
        unit.fullState = saved;
        check(block(&flags, &timestamp, 512, 0, output, nullptr, pull) == 0 && left[511] == 0.5f, "live restore");
        AURenderEvent bypass{}; bypass.parameter.eventType = AURenderEventParameter;
        bypass.parameter.eventSampleTime = 1256; bypass.parameter.parameterAddress = 2; bypass.parameter.value = 1;
        [unit.parameterTree parameterWithAddress:1].value = 2;
        check(block(&flags, &timestamp, 512, 0, output, &bypass, pull) == 0, "timed bypass");
        check(left[255] > 0.99f && left[256] == 0.5f, "bypass offset");
        AURenderEvent bad = bypass; bad.parameter.parameterAddress = 999; bypass.head.next = &bad;
        check(block(&flags, &timestamp, 512, 0, output, &bypass, pull) != 0 && left[0] == 0 && left[512] == 99, "whole event rejection");
        bypass.head.next = nullptr;
        unit.fullState = saved;
        AURenderEvent ramp{}; ramp.parameter.eventType = AURenderEventParameterRamp;
        ramp.parameter.eventSampleTime = 1000; ramp.parameter.parameterAddress = 1; ramp.parameter.value = 2; ramp.parameter.rampDurationSampleFrames = 768;
        check(block(&flags, &timestamp, 512, 0, output, &ramp, pull) == 0, "ramp first block");
        timestamp.mSampleTime += 512;
        check(block(&flags, &timestamp, 512, 0, output, nullptr, pull) == 0, "ramp continuation");
        [unit refreshParameterValues];
        check([unit.parameterTree parameterWithAddress:1].value == 2, "ramp target reflected");
        unit.shouldBypassEffect = YES;
        check(block(&flags, &timestamp, 512, 0, output, nullptr, pull) == 0 && left[511] == 0.5f, "host bypass");
        unit.shouldBypassEffect = NO;
        unit.scheduleParameterBlock(AUEventSampleTimeImmediate, 0, 1, 1);
        check(unit.renderBlock(&flags, &timestamp, 512, 0, output, pull) == 0 && left[511] == 0.5f, "host render wrapper and scheduler");
        check(unit.latency == 0 && unit.tailTime == 0, "cached timing");
        unit.musicalContextBlock = ^BOOL(double *tempo, double *numerator, NSInteger *denominator, double *beats, NSInteger *, double *bar) {
            *tempo = 120; *numerator = 4; *denominator = 4; *beats = 3; *bar = 0; return YES;
        };
        unit.transportStateBlock = ^BOOL(AUHostTransportStateFlags *state, double *samples, double *start, double *end) {
            *state = AUHostTransportStateMoving | AUHostTransportStateRecording | AUHostTransportStateCycling;
            *samples = 1000; *start = 0; *end = 8; return YES;
        };
        check(block(&flags, &timestamp, 512, 0, output, nullptr, pull) == 0, "transport conversion");
        [unit reset];
        [unit deallocateRenderResources];
        check(block(&flags, &timestamp, 512, 0, output, nullptr, pull) != 0 && left[0] == 0, "stopped render");
        check([unit allocateRenderResourcesAndReturnError:&error], "reprepare");
        check(block(&flags, &timestamp, 0, 0, output, nullptr, pull) == 0, "zero frames");
        [unit deallocateRenderResources];
    }
    NSError *error = nil;
    MIDIUnit *midi = [[MIDIUnit alloc] initWithComponentDescription:description options:0 error:&error];
    check([midi allocateRenderResourcesAndReturnError:&error], "MIDI-profile prepare");
    std::array<float, 8> l{}, r{};
    struct { UInt32 count; AudioBuffer buffers[2]; } buffers{2, {{1, sizeof(l), l.data()}, {1, sizeof(r), r.data()}}};
    AudioTimeStamp time{}; time.mFlags = kAudioTimeStampSampleTimeValid;
    AudioUnitRenderActionFlags flags = 0;
    AURenderPullInputBlock pull = ^OSStatus(AudioUnitRenderActionFlags *, const AudioTimeStamp *, AUAudioFrameCount, NSInteger, AudioBufferList *) { return 0; };
    AURenderEvent note{}; note.MIDI.eventType = AURenderEventMIDI;
    note.MIDI.length = 3; note.MIDI.data[0] = 0x90; note.MIDI.data[1] = 60; note.MIDI.data[2] = 100;
    check(midi.internalRenderBlock(&flags, &time, 8, 0, reinterpret_cast<AudioBufferList *>(&buffers), &note, pull) == 0, "short MIDI input");
    AURenderEvent ump{}; ump.MIDIEventsList.eventType = AURenderEventMIDIEventList;
    ump.MIDIEventsList.eventList.protocol = kMIDIProtocol_2_0;
    ump.MIDIEventsList.eventList.numPackets = 1;
    ump.MIDIEventsList.eventList.packet[0].wordCount = 2;
    ump.MIDIEventsList.eventList.packet[0].words[0] = 0x40903c00;
    ump.MIDIEventsList.eventList.packet[0].words[1] = 0x80000000;
    check(midi.internalRenderBlock(&flags, &time, 8, 0, reinterpret_cast<AudioBufferList *>(&buffers), &ump, pull) == 0, "UMP input");
    ump.MIDIEventsList.eventList.packet[0].wordCount = 1;
    check(midi.internalRenderBlock(&flags, &time, 8, 0, reinterpret_cast<AudioBufferList *>(&buffers), &ump, pull) != 0, "whole UMP rejection");
    [midi deallocateRenderResources];
    Editor *editor = [[Editor alloc] init];
    check([editor createAudioUnitWithComponentDescription:description error:&error] != nil, "editor factory");
    (void)editor.view;
    check(editor.view.subviews.count == 1, "programmatic editor controls");
    std::puts("AUv3 schema/audio/state/timing/ramp/lifecycle contracts passed");
} return 0; }
#ifndef RAVET_AUV3_CONTRACTS_EMBEDDED
int main() { return ravet_auv3_contracts(); }
#endif
