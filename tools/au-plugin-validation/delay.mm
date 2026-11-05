// Product-specific expectations against the actual generic AU component.
#include <AudioToolbox/AudioToolbox.h>
#include <algorithm>
#include <array>
#include <cmath>
#include <cstring>
#include <iostream>
#include <stdexcept>
#include <vector>

static void require(bool value, const char *text) { if (!value) throw std::runtime_error(text); }
static void okay(OSStatus status, const char *text) {
    if (status) { std::cerr << text << ": " << status << '\n'; throw std::runtime_error(text); }
}
struct Host { std::array<float, 4096> input{}; };
static OSStatus input(void *user, AudioUnitRenderActionFlags *, const AudioTimeStamp *, UInt32, UInt32 frames, AudioBufferList *buffers) {
    const auto &host = *static_cast<Host *>(user);
    if (frames > host.input.size()) return kAudioUnitErr_TooManyFramesToProcess;
    for (UInt32 c = 0; c < buffers->mNumberBuffers; ++c) std::copy_n(host.input.begin(), frames, static_cast<float *>(buffers->mBuffers[c].mData));
    return noErr;
}
static OSStatus tempo(void *, Float64 *beat, Float64 *bpm) { *beat = 0; *bpm = 120; return noErr; }
// REAPER 7.82's calling pattern at 48 kHz/512: SetParameter from the main thread
// before rendering, beat/tempo, signature and TransportState2 callbacks on every
// block, and transport sample positions carrying Float64 residue (one ulp above
// or below the whole block start on some blocks), as recorded from the host.
struct Reaper { double position = 0; std::array<double, 2> impulses{}; };
static OSStatus reaperInput(void *user, AudioUnitRenderActionFlags *, const AudioTimeStamp *time, UInt32, UInt32 frames, AudioBufferList *buffers) {
    const auto &host = *static_cast<Reaper *>(user);
    for (UInt32 c = 0; c < buffers->mNumberBuffers; ++c) {
        auto *samples = static_cast<float *>(buffers->mBuffers[c].mData);
        std::fill_n(samples, frames, 0.f);
        for (const auto impulse : host.impulses)
            if (impulse >= time->mSampleTime && impulse < time->mSampleTime + frames) samples[UInt32(impulse - time->mSampleTime)] = 1;
    }
    return noErr;
}
static OSStatus reaperBeat(void *user, Float64 *beat, Float64 *bpm) {
    *bpm = 150; *beat = static_cast<Reaper *>(user)->position * 150 / 60 / 48000; return noErr;
}
static OSStatus reaperSignature(void *, UInt32 *, Float32 *numerator, UInt32 *denominator, Float64 *downbeat) {
    *numerator = 4; *denominator = 4; if (downbeat) *downbeat = 0; return noErr;
}
static OSStatus reaperTransport(void *user, Boolean *playing, Boolean *recording, Boolean *changed, Float64 *samples,
                                Boolean *cycling, Float64 *start, Float64 *end) {
    const auto position = static_cast<Reaper *>(user)->position;
    const auto block = int64_t(position) / 512;
    *playing = true; *recording = false; *changed = position == 0; *cycling = false; *start = *end = 0;
    *samples = block % 7 == 3 ? std::nextafter(position, 1e300) : block % 7 == 5 ? std::nextafter(position, 0.) : position;
    return noErr;
}
static void reaperPattern(AudioComponent component) {
    AudioUnit unit = nullptr; okay(AudioComponentInstanceNew(component, &unit), "REAPER-pattern create");
    AudioStreamBasicDescription format{}; format.mSampleRate = 48000; format.mFormatID = kAudioFormatLinearPCM;
    format.mFormatFlags = UInt32(kAudioFormatFlagsNativeFloatPacked) | UInt32(kAudioFormatFlagIsNonInterleaved);
    format.mChannelsPerFrame = 2; format.mBytesPerPacket = format.mBytesPerFrame = 4; format.mBitsPerChannel = 32; format.mFramesPerPacket = 1;
    for (const auto scope : {kAudioUnitScope_Input, kAudioUnitScope_Output})
        okay(AudioUnitSetProperty(unit, kAudioUnitProperty_StreamFormat, scope, 0, &format, sizeof(format)), "REAPER-pattern format");
    UInt32 maximum = 512;
    okay(AudioUnitSetProperty(unit, kAudioUnitProperty_MaximumFramesPerSlice, kAudioUnitScope_Global, 0, &maximum, sizeof(maximum)), "REAPER-pattern block size");
    Reaper host;
    AURenderCallbackStruct callback{reaperInput, &host};
    okay(AudioUnitSetProperty(unit, kAudioUnitProperty_SetRenderCallback, kAudioUnitScope_Input, 0, &callback, sizeof(callback)), "REAPER-pattern input");
    HostCallbackInfo callbacks{&host, reaperBeat, reaperSignature, nullptr, reaperTransport};
    okay(AudioUnitSetProperty(unit, kAudioUnitProperty_HostCallbacks, kAudioUnitScope_Global, 0, &callbacks, sizeof(callbacks)), "REAPER-pattern callbacks");
    okay(AudioUnitInitialize(unit), "REAPER-pattern prepare");
    auto set = [&](uint32_t id, float value) { okay(AudioUnitSetParameter(unit, id, kAudioUnitScope_Global, 0, value, 0), "REAPER-pattern parameter"); };
    std::vector<float> rendered;
    std::array<float, 512> left{}, right{};
    auto run = [&](double seconds) {
        rendered.clear(); host.position = 0;
        for (const auto scope : {kAudioUnitScope_Global, kAudioUnitScope_Input, kAudioUnitScope_Output}) AudioUnitReset(unit, scope, 0);
        struct Buffers { UInt32 count; AudioBuffer channels[2]; } buffers{2, {{1, 512 * 4, left.data()}, {1, 512 * 4, right.data()}}};
        while (host.position < seconds * 48000) {
            AudioUnitRenderActionFlags flags = 0; AudioTimeStamp time{}; time.mFlags = kAudioTimeStampSampleTimeValid; time.mSampleTime = host.position;
            buffers.channels[0].mDataByteSize = buffers.channels[1].mDataByteSize = 512 * 4;
            okay(AudioUnitRender(unit, &flags, &time, 0, 512, reinterpret_cast<AudioBufferList *>(&buffers)), "REAPER-pattern render");
            rendered.insert(rendered.end(), left.begin(), left.end()); host.position += 512;
        }
    };
    auto peaks = [&] {
        std::vector<size_t> found;
        for (size_t i = 0; i < rendered.size(); ++i) if (std::abs(rendered[i]) > 1e-3) found.push_back(i);
        return found;
    };
    host.impulses = {24000, 96000};
    set(1, 100); set(2, 0); set(3, 100); set(5, 0); set(6, 0); // fixed 100 ms, no feedback, fully wet
    run(2.5);
    auto found = peaks();
    require(found == std::vector<size_t>({28800, 100800}) && std::abs(rendered[28800] - 1) < 1e-6 && std::abs(rendered[100800] - 1) < 1e-6,
        "REAPER-pattern fixed 100 ms echoes at exactly +4800");
    set(3, 50); set(6, 1); set(7, 6); // synced 1/8 at 150 BPM: 200 ms, half dry
    run(2.5);
    found = peaks();
    require(found == std::vector<size_t>({24000, 33600, 96000, 105600}), "REAPER-pattern synced 1/8 echoes at exactly +9600");
    okay(AudioUnitUninitialize(unit), "REAPER-pattern stop"); okay(AudioComponentInstanceDispose(unit), "REAPER-pattern dispose");
}
int main(int argc, char **argv) {
    try {
        require(argc == 2, "usage: delay-contracts component");
        auto url = CFURLCreateFromFileSystemRepresentation(nullptr, reinterpret_cast<const UInt8 *>(argv[1]), std::strlen(argv[1]), true);
        auto bundle = CFBundleCreate(nullptr, url); CFRelease(url);
        require(bundle && CFBundleLoadExecutable(bundle), "load delay component");
        const auto factory = reinterpret_cast<AudioComponentFactoryFunction>(CFBundleGetFunctionPointerForName(bundle, CFSTR("RavetAUFactory")));
        require(factory, "native delay factory");
        const AudioComponentDescription description{'aufx', 'QFKS', 'TYDS', 0, 0};
        const auto component = AudioComponentRegister(&description, CFSTR("RAVET Delay native AU contracts"), 256, factory);
        require(component, "delay component identity");
        AudioUnit unit = nullptr; okay(AudioComponentInstanceNew(component, &unit), "delay create");
        AudioStreamBasicDescription format{}; format.mSampleRate = 8000; format.mFormatID = kAudioFormatLinearPCM;
        format.mFormatFlags = UInt32(kAudioFormatFlagsNativeFloatPacked) | UInt32(kAudioFormatFlagIsNonInterleaved);
        format.mChannelsPerFrame = 2; format.mBytesPerPacket = format.mBytesPerFrame = 4; format.mBitsPerChannel = 32; format.mFramesPerPacket = 1;
        for (const auto scope : {kAudioUnitScope_Input, kAudioUnitScope_Output})
            okay(AudioUnitSetProperty(unit, kAudioUnitProperty_StreamFormat, scope, 0, &format, sizeof(format)), "delay format");
        UInt32 maximum = 4096;
        okay(AudioUnitSetProperty(unit, kAudioUnitProperty_MaximumFramesPerSlice, kAudioUnitScope_Global, 0, &maximum, sizeof(maximum)), "delay block size");
        Host host;
        AURenderCallbackStruct callback{input, &host};
        okay(AudioUnitSetProperty(unit, kAudioUnitProperty_SetRenderCallback, kAudioUnitScope_Input, 0, &callback, sizeof(callback)), "delay input");
        HostCallbackInfo callbacks{}; callbacks.beatAndTempoProc = tempo;
        okay(AudioUnitSetProperty(unit, kAudioUnitProperty_HostCallbacks, kAudioUnitScope_Global, 0, &callbacks, sizeof(callbacks)), "delay host tempo");
        okay(AudioUnitInitialize(unit), "delay prepare");
        auto set = [&](uint32_t id, float value) { okay(AudioUnitSetParameter(unit, id, kAudioUnitScope_Global, 0, value, 0), "delay host parameter"); };
        set(2, 0); set(3, 100); set(6, 1); set(7, 6); // no feedback, fully wet, synced eighth note
        CFPropertyListRef state = nullptr; UInt32 size = sizeof(state);
        okay(AudioUnitGetProperty(unit, kAudioUnitProperty_ClassInfo, kAudioUnitScope_Global, 0, &state, &size), "delay save before audio");
        auto envelope = static_cast<CFDataRef>(CFDictionaryGetValue(static_cast<CFDictionaryRef>(state), CFSTR("RAVET")));
        require(envelope && CFDataGetLength(envelope) > 8 && !std::memcmp(CFDataGetBytePtr(envelope), "RAVETSTA", 8), "unchanged Rust state envelope");
        std::array<float, 4097> left{}, right{};
        struct Buffers { UInt32 count; AudioBuffer channels[2]; } buffers{2, {{1, 4096 * 4, left.data()}, {1, 4096 * 4, right.data()}}};
        AudioTimeStamp time{}; time.mFlags = kAudioTimeStampSampleTimeValid;
        auto render = [&] {
            left.fill(99); right.fill(99); AudioUnitRenderActionFlags flags = 0;
            okay(AudioUnitRender(unit, &flags, &time, 0, 4096, reinterpret_cast<AudioBufferList *>(&buffers)), "delay render"); time.mSampleTime += 4096;
            require(left[4096] == 99 && right[4096] == 99, "delay backing tails");
        };
        render(); host.input[0] = 1; render();
        for (uint32_t i = 0; i < 4096; ++i) {
            const float expected = i == 2000 ? 1.f : 0.f;
            require(std::abs(left[i] - expected) < 1e-6 && std::abs(right[i] - expected) < 1e-6,
                "120 BPM eighth-note impulse at exactly 2000 samples");
        }
        set(3, 0); set(7, 0); UInt32 bypass = 1;
        okay(AudioUnitSetProperty(unit, kAudioUnitProperty_BypassEffect, kAudioUnitScope_Global, 0, &bypass, sizeof(bypass)), "delay host bypass");
        okay(AudioUnitSetProperty(unit, kAudioUnitProperty_ClassInfo, kAudioUnitScope_Global, 0, &state, sizeof(state)), "delay complete restore");
        float mix = 0, division = 0;
        okay(AudioUnitGetParameter(unit, 3, kAudioUnitScope_Global, 0, &mix), "restored mix");
        okay(AudioUnitGetParameter(unit, 7, kAudioUnitScope_Global, 0, &division), "restored division");
        size = sizeof(bypass); okay(AudioUnitGetProperty(unit, kAudioUnitProperty_BypassEffect, kAudioUnitScope_Global, 0, &bypass, &size), "restored bypass");
        require(mix == 100 && division == 6 && !bypass, "state includes main cache and bypass");
        host.input.fill(0); render();
        AudioUnitParameterEvent schedule{}; schedule.scope = kAudioUnitScope_Global; schedule.parameter = 3;
        schedule.eventType = kParameterEvent_Immediate; schedule.eventValues.immediate = {16, 0};
        okay(AudioUnitScheduleParameters(unit, &schedule, 1), "delay sample-accurate mix");
        host.input.fill(.25); render();
        for (int i = 0; i < 16; ++i) require(std::abs(left[i]) < 1e-6, "automation retains wet prefix before offset 16");
        require(std::abs(left[80] - .25f) < 1e-6 && std::abs(right[80] - .25f) < 1e-6, "automation reaches dry after product smoothing");
        set(3, 100); bypass = 1;
        okay(AudioUnitSetProperty(unit, kAudioUnitProperty_BypassEffect, kAudioUnitScope_Global, 0, &bypass, sizeof(bypass)), "delay bypass set"); render(); render();
        require(std::abs(left[0] - .25f) < 1e-6, "delay table owns bypass processing");
        Float64 tail = 0; size = sizeof(tail);
        okay(AudioUnitGetProperty(unit, kAudioUnitProperty_TailTime, kAudioUnitScope_Global, 0, &tail, &size), "delay tail");
        require(tail == 2.000125, "tail refreshes without a latency change");
        CFRelease(state); okay(AudioUnitUninitialize(unit), "delay stop"); okay(AudioComponentInstanceDispose(unit), "delay dispose");
        reaperPattern(component); CFRelease(bundle);
        std::cout << "PASS real delay AUv2 8 kHz stereo: 120 BPM repeat at sample 2000, cached-state envelope, complete bypass recall, offset-16 automation, tail and backing tails\n";
        std::cout << "PASS real delay AUv2 REAPER pattern 48 kHz/512 with Float64-residue transport samples: fixed echoes +4800, synced 1/8 at 150 BPM +9600\n";
        return 0;
    } catch (const std::exception &error) { std::cerr << "FAIL " << error.what() << '\n'; return 1; }
}
