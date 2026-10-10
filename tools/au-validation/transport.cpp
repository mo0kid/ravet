#include "RavetAuVst3Transport.hpp"
#include "RavetVst3Transport.hpp"
#include "RavetPerformance.h"
#include <cstdlib>
#include <cstring>
#include <iostream>
#include <new>
#include <limits>
#include <stdexcept>
namespace allocation { bool track = false; size_t created = 0, retired = 0; }
void *operator new(size_t size) { if (allocation::track) ++allocation::created; if (auto p = std::malloc(size ? size : 1)) return p; throw std::bad_alloc(); }
void *operator new[](size_t size) { return ::operator new(size); }
void operator delete(void *p) noexcept { if (p && allocation::track) ++allocation::retired; std::free(p); }
void operator delete[](void *p) noexcept { ::operator delete(p); }
void operator delete(void *p, size_t) noexcept { ::operator delete(p); }
void operator delete[](void *p, size_t) noexcept { ::operator delete(p); }
namespace {
void require(bool okay, const char *message) { if (!okay) throw std::runtime_error(message); }
struct Host {
    OSStatus beatStatus = noErr, signatureStatus = noErr, transportStatus = noErr;
    bool missingBeat = false, missingSignature = false, missingTransport = false, sentinels = true;
    int beats = 0, signatures = 0, legacy = 0, modern = 0;
    double beat = -1.5, tempo = 120., sample = -240., start = -4., end = 8.;
    float numerator = 7.f; uint32_t denominator = 8;
    Boolean playing = 1, changed = 1, cycling = 1, recording = 1;
};
__attribute__((noinline)) OSStatus beat(void *user, Float64 *position, Float64 *tempo) {
    auto &h = *static_cast<Host *>(user); ++h.beats;
    h.sentinels = h.sentinels && !ravet::native::scalar::finite(*position) && !ravet::native::scalar::finite(*tempo);
    if (!h.missingBeat) { *position = h.beat; *tempo = h.tempo; } return h.beatStatus;
}
__attribute__((noinline)) OSStatus signature(void *user, UInt32 *clock, Float32 *numerator, UInt32 *denominator, Float64 *bar) {
    auto &h = *static_cast<Host *>(user); ++h.signatures;
    h.sentinels = h.sentinels && !clock && !bar && !ravet::native::scalar::finite(*numerator) && !*denominator;
    if (!h.missingSignature) { *numerator = h.numerator; *denominator = h.denominator; } return h.signatureStatus;
}
__attribute__((noinline)) OSStatus transport2(void *user, Boolean *playing, Boolean *recording, Boolean *changed,
                    Float64 *samples, Boolean *cycling, Float64 *start, Float64 *end) {
    auto &h = *static_cast<Host *>(user); ++h.modern;
    h.sentinels = h.sentinels && *playing == 2 && *recording == 2 && *changed == 2 &&
        *cycling == 2 && !ravet::native::scalar::finite(*samples) && !ravet::native::scalar::finite(*start) && !ravet::native::scalar::finite(*end);
    if (!h.missingTransport) {
        *playing = h.playing; *recording = h.recording; *changed = h.changed;
        *samples = h.sample; *cycling = h.cycling; *start = h.start; *end = h.end;
    }
    return h.transportStatus;
}
__attribute__((noinline)) OSStatus transport(void *user, Boolean *playing, Boolean *changed, Float64 *samples,
                   Boolean *cycling, Float64 *start, Float64 *end) {
    auto &h = *static_cast<Host *>(user); ++h.legacy;
    Boolean recording = 2; const auto status = transport2(user, playing, &recording, changed, samples, cycling, start, end);
    --h.modern; return status;
}
HostCallbackInfo callbacks(Host &host) { return {&host, beat, signature, transport, transport2}; }
double foreignInfinity() { volatile uint64_t foreignBits = UINT64_C(0x7ff0000000000000); uint64_t bits = foreignBits; double value; std::memcpy(&value, &bits, sizeof(value)); return value; }
void foreignNaN(double &value) { volatile uint64_t foreignBits = UINT64_C(0x7ff8000000000000); uint64_t bits = foreignBits; std::memcpy(&value, &bits, sizeof(value)); }
void foreignNaN(float &value) { volatile uint32_t foreignBits = UINT32_C(0x7fc00000); uint32_t bits = foreignBits; std::memcpy(&value, &bits, sizeof(value)); }
void nativeCanonical() {
    using namespace ravet::au;
    const auto *entry=ravet_performance_processor_v1(32);require(entry,"actual native performance entry");
    auto *processor=entry->create();require(processor,"native processor create");
    RavetNativeProcessorConfigV1 config{};config.version=1;config.maximum_frames=64;config.maximum_events=1024;config.bus_count=1;config.sample_rate=48000.;config.buses[0]={0,1};
    require(!entry->prepare(processor,&config),"actual native zero-frame preparation");
    const RavetNativeProcessorBusV1 bus{0,1,nullptr,nullptr};
    auto accepted=[&](const RavetNativeTransportV1 &transport){RavetNativeProcessorResultV1 result{};return entry->process(processor,&bus,1,0,nullptr,0,&transport,nullptr,0,&result)==0&&result.status==0&&!result.event_count;};
    Host h;auto host=callbacks(h);NativeAuTransportSnapshot snapshot;
    require(nativeTransport(host,snapshot)&&accepted(snapshot.transport()),"authoritative native codec accepts complete AU snapshot");
    host.beatAndTempoProc=nullptr;host.transportStateProc=nullptr;host.transportStateProc2=nullptr;
    require(nativeTransport(host,snapshot)&&snapshot.transport().present==(RAVET_TRANSPORT_DETAILS|RAVET_TRANSPORT_SIGNATURE)&&accepted(snapshot.transport()),"native codec accepts signature-only AU DETAILS placeholders");
    auto invalid=snapshot.transport();invalid.present&=~RAVET_TRANSPORT_DETAILS;require(!accepted(invalid),"native codec rejects signature without canonical DETAILS dependency");
    host=callbacks(h);host.musicalTimeLocationProc=nullptr;host.transportStateProc=nullptr;host.transportStateProc2=nullptr;
    require(nativeTransport(host,snapshot)&&snapshot.transport().present==(RAVET_TRANSPORT_BEATS|RAVET_TRANSPORT_TEMPO)&&accepted(snapshot.transport()),"native codec accepts independent beat/tempo-only AU snapshot");
    invalid=snapshot.transport();invalid.sample_position=1;require(!accepted(invalid),"native codec rejects noncanonical unused sample field");
    host=callbacks(h);host.beatAndTempoProc=nullptr;host.musicalTimeLocationProc=nullptr;host.transportStateProc2=nullptr;
    require(nativeTransport(host,snapshot)&&snapshot.callbacks()==LegacyTransport&&accepted(snapshot.transport()),"native codec accepts legacy AU transport-only snapshot");
    invalid=snapshot.transport();invalid.reserved=1;require(!accepted(invalid),"native codec rejects reserved native fields");
    require(nativeTransport(HostCallbackInfo{},snapshot)&&accepted(snapshot.transport()),"native codec accepts wholly unknown AU snapshot");
    require(!entry->deactivate(processor),"stop native zero-frame validation owner");entry->destroy(processor);
}
void cases() {
    using namespace ravet::au;
    using C = Steinberg::Vst::ProcessContext;
    Host h; auto host = callbacks(h); NativeAuTransportSnapshot snapshot{};
    require(nativeTransport(host, snapshot), "complete callback collection");
    require(h.sentinels && h.beats == 1 && h.signatures == 1 && h.modern == 1 && !h.legacy, "single callbacks with initialized outputs and State2 precedence");
    require(snapshot.callbacks() == (BeatAndTempo | Signature | Transport2) && snapshot.transport().sample_position == -240 &&
        snapshot.transport().flags == (RAVET_TRANSPORT_FLAG_PLAYING | RAVET_TRANSPORT_FLAG_DISCONTINUITY |
            RAVET_TRANSPORT_FLAG_RECORDING | RAVET_TRANSPORT_FLAG_LOOP_ACTIVE), "actual callback availability and native detail flags");
    C context{}; require(vst3Transport(snapshot, 48000., context) == &context && context.projectTimeSamples == -240 &&
        context.projectTimeMusic == -1.5 && context.timeSigNumerator == 7 && context.timeSigDenominator == 8 &&
        (context.state & C::kRecording) && (context.state & C::kCycleValid), "complete safe SDK projection");
    RavetNativeTransportV1 roundtrip{}; require(ravet::vst3::nativeTransport(&context, roundtrip) &&
        !(roundtrip.flags & RAVET_TRANSPORT_FLAG_DISCONTINUITY) && roundtrip.beat_loop_start == -4., "bridge omits unrepresentable discontinuity");
    host.transportStateProc2 = nullptr; require(nativeTransport(host, snapshot) && snapshot.callbacks() == (BeatAndTempo | Signature | LegacyTransport) &&
        !(snapshot.transport().flags & RAVET_TRANSPORT_FLAG_RECORDING), "legacy recording remains unavailable through callback sidecar");
    require(vst3Transport(snapshot, 48000., context) && !(context.state & C::kRecording), "legacy projection never invents recording");
    host = callbacks(h); h.transportStatus = kAudioUnitErr_CannotDoInCurrentContext;
    const auto legacyCount = h.legacy;
    require(nativeTransport(host, snapshot) && snapshot.callbacks() == (BeatAndTempo | Signature) && h.legacy == legacyCount, "modern callback error omits tuple with no legacy fallback");
    context.projectTimeSamples = 99;
    require(!vst3Transport(snapshot, 48000., context) && context.projectTimeSamples == 99 && snapshot.transport().tempo == 120., "partial direct snapshot survives while SDK bridge has no context");
    h.transportStatus = noErr; h.beatStatus = -1; h.signatureStatus = -1;
    require(nativeTransport(host, snapshot) && snapshot.callbacks() == Transport2 && !(snapshot.transport().present & RAVET_TRANSPORT_BEAT_LOOP) &&
        (snapshot.transport().flags & RAVET_TRANSPORT_FLAG_LOOP_ACTIVE), "transport-only availability never invents beat or signature");
    require(vst3Transport(snapshot, 48000., context) && !(context.state & (C::kTempoValid | C::kProjectTimeMusicValid | C::kCycleValid)), "transport-only bridge flags");
    require(nativeTransport(HostCallbackInfo{}, snapshot) && !snapshot.callbacks() && !snapshot.transport().present && !snapshot.transport().flags, "absent callbacks yield wholly unknown native context");
    host = callbacks(h); h = Host{};
    require(nativeTransport(host, snapshot), "restore valid snapshot");
    const auto retained = snapshot;
    auto reject = [&](const char *message) {
        require(!nativeTransport(host, snapshot) && snapshot.callbacks() == retained.callbacks() &&
            std::memcmp(&snapshot.transport(), &retained.transport(), sizeof(retained.transport())) == 0, message);
    };
    h.sample = 0x1p63; reject("+2^63 rejects before cast");
    h.sample = std::nextafter(-0x1p63, -foreignInfinity()); reject("below -2^63 rejects before cast");
    h.sample = .5; reject("fractional sample rejects before cast");
    foreignNaN(h.sample); reject("NaN sample rejects before cast");
    h.sample = foreignInfinity(); reject("infinite sample rejects before cast");
    h.sample = -240; h.numerator = 3.5f; reject("fractional signature rejects before cast");
    foreignNaN(h.numerator); reject("NaN signature rejects before cast");
    h.numerator = 65536.f; reject("oversized signature rejects before cast");
    h.numerator = 7; h.denominator = 65536; reject("oversized denominator rejects");
    h.denominator = 8; h.tempo = 0; reject("zero tempo rejects");
    h.tempo = 120; h.beat = foreignInfinity(); reject("nonfinite beat rejects");
    h.beat = -1.5; h.playing = 2; reject("invalid playing boolean rejects"); h.playing = 1;
    h.changed = 2; reject("invalid discontinuity boolean rejects"); h.changed = 1;
    h.recording = 2; reject("invalid recording boolean rejects"); h.recording = 1;
    h.cycling = 2; reject("invalid loop boolean rejects"); h.cycling = 1;
    h.end = h.start; reject("active unordered loop rejects"); h.end = 8;
    h.missingBeat = true; reject("success without beat/tempo initialization rejects"); h.missingBeat = false;
    h.missingSignature = true; reject("success without signature initialization rejects"); h.missingSignature = false;
    h.missingTransport = true; reject("success without transport initialization rejects"); h.missingTransport = false;
    h.sample = -0x1p63; require(nativeTransport(host, snapshot) && snapshot.transport().sample_position == INT64_MIN, "exact -2^63 accepted");
    h.sample = std::nextafter(0x1p63, 0.); require(nativeTransport(host, snapshot) && snapshot.transport().sample_position == INT64_MAX - 1023, "last representable Float64 below +2^63 accepted");
    h.sample = 0; h.cycling = 0; foreignNaN(h.start); h.end = foreignInfinity();
    h.numerator = 65535; h.denominator = 65535;
    require(nativeTransport(host, snapshot) && !(snapshot.transport().present & RAVET_TRANSPORT_BEAT_LOOP), "inactive irrelevant loop bounds ignored and maximum u16 signature accepted");
    require(vst3Transport(snapshot, 48000., context) && context.timeSigNumerator == 65535, "maximum validated signature projects safely");
    double invalidRate; foreignNaN(invalidRate);
    require(!vst3Transport(snapshot, invalidRate, context) && !vst3Transport(snapshot, 0., context), "invalid SDK rate yields no context");
    h = Host{}; bool okay = true; allocation::created = allocation::retired = 0; allocation::track = true;
    for (uint32_t cycle = 0; cycle < 4000; ++cycle) {
        okay = nativeTransport(host, snapshot) && vst3Transport(snapshot, 48000., context) && okay;
        h.sample = 0x1p63; okay = !nativeTransport(host, snapshot) && okay; h.sample = -240;
    }
    allocation::track = false;
    require(okay && !allocation::created && !allocation::retired, "4000 accepted/rejected callback collections and projections have no C++ heap activity");
}
}
int main() { try { nativeCanonical(); cases(); std::cout << "PASS AU native transport callback/preflight/projection contracts\n"; return 0; } catch (const std::exception &error) { std::cerr << error.what() << '\n'; return 1; } }
