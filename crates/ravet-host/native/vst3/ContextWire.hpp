#ifndef RAVET_VST3_CONTEXT_WIRE_HPP
#define RAVET_VST3_CONTEXT_WIRE_HPP
#include "RavetVst3Processing.h"
#include "BoundedParameterChanges.hpp"
#include "pluginterfaces/vst/ivstprocesscontext.h"
#include "pluginterfaces/vst/ivstaudioprocessor.h"
namespace ravet::vst3::processing_draft {
using Context=RavetVst3ContextRecord;
using parameters_draft::finite;
using NativeContext=Steinberg::Vst::ProcessContext;
static_assert(RAVET_VST3_CONTEXT_PLAYING==uint32_t(NativeContext::kPlaying));
static_assert(RAVET_VST3_CONTEXT_CYCLE_ACTIVE==uint32_t(NativeContext::kCycleActive));
static_assert(RAVET_VST3_CONTEXT_RECORDING==uint32_t(NativeContext::kRecording));
static_assert(RAVET_VST3_CONTEXT_SYSTEM_TIME==uint32_t(NativeContext::kSystemTimeValid));
static_assert(RAVET_VST3_CONTEXT_MUSIC==uint32_t(NativeContext::kProjectTimeMusicValid));
static_assert(RAVET_VST3_CONTEXT_TEMPO==uint32_t(NativeContext::kTempoValid));
static_assert(RAVET_VST3_CONTEXT_BAR==uint32_t(NativeContext::kBarPositionValid));
static_assert(RAVET_VST3_CONTEXT_CYCLE==uint32_t(NativeContext::kCycleValid));
static_assert(RAVET_VST3_CONTEXT_TIME_SIGNATURE==uint32_t(NativeContext::kTimeSigValid));
static_assert(RAVET_VST3_CONTEXT_SMPTE==uint32_t(NativeContext::kSmpteValid));
static_assert(RAVET_VST3_CONTEXT_CLOCK==uint32_t(NativeContext::kClockValid));
static_assert(RAVET_VST3_CONTEXT_CONTINUOUS==uint32_t(NativeContext::kContTimeValid));
static_assert(RAVET_VST3_CONTEXT_CHORD==uint32_t(NativeContext::kChordValid));
static_assert(RAVET_VST3_FRAME_PULL_DOWN==uint32_t(Steinberg::Vst::FrameRate::kPullDownRate));
static_assert(RAVET_VST3_FRAME_DROP==uint32_t(Steinberg::Vst::FrameRate::kDropRate));
inline constexpr uint32_t ContextFlags=RAVET_VST3_CONTEXT_PLAYING|RAVET_VST3_CONTEXT_CYCLE_ACTIVE|
    RAVET_VST3_CONTEXT_RECORDING|RAVET_VST3_CONTEXT_SYSTEM_TIME|RAVET_VST3_CONTEXT_MUSIC|
    RAVET_VST3_CONTEXT_TEMPO|RAVET_VST3_CONTEXT_BAR|RAVET_VST3_CONTEXT_CYCLE|
    RAVET_VST3_CONTEXT_TIME_SIGNATURE|RAVET_VST3_CONTEXT_SMPTE|RAVET_VST3_CONTEXT_CLOCK|
    RAVET_VST3_CONTEXT_CONTINUOUS|RAVET_VST3_CONTEXT_CHORD;
// Whole pure validation. Requests are metadata, not a demand that the host
// invent unavailable context fields. Independent transport/presence bits stay
// independent, and absent storage is never interpreted as supplied data.
inline bool validateContext(const Context& record,double sampleRate) {
    const auto state=record.state;
    if(record.struct_size!=sizeof(Context)||record.reserved||(state&~ContextFlags)||
       !finite(sampleRate)||sampleRate<=0||!finite(record.sample_rate)||record.sample_rate!=sampleRate)return false;
    if((state&RAVET_VST3_CONTEXT_MUSIC)&&!finite(record.project_time_music))return false;
    if((state&RAVET_VST3_CONTEXT_BAR)&&!finite(record.bar_position_music))return false;
    if((state&RAVET_VST3_CONTEXT_CYCLE)&&(!finite(record.cycle_start_music)||
       !finite(record.cycle_end_music)||record.cycle_end_music<record.cycle_start_music))return false;
    if((state&RAVET_VST3_CONTEXT_TEMPO)&&(!finite(record.tempo)||record.tempo<=0))return false;
    if((state&RAVET_VST3_CONTEXT_TIME_SIGNATURE)&&
       (record.time_signature_numerator<=0||record.time_signature_denominator<=0))return false;
    if((state&RAVET_VST3_CONTEXT_CHORD)&&(record.chord_key>UINT8_MAX||
       record.chord_root>UINT8_MAX||record.chord_mask>uint32_t(Steinberg::Vst::Chord::kChordMask)))return false;
    if((state&RAVET_VST3_CONTEXT_SMPTE)&&(!record.frames_per_second||
       (record.frame_rate_flags&~uint32_t(RAVET_VST3_FRAME_PULL_DOWN|RAVET_VST3_FRAME_DROP))))return false;
    return true;
}
inline bool sdkContext(const Context& record,double sampleRate,NativeContext& destination) {
    if(!validateContext(record,sampleRate))return false;
    NativeContext result{};const auto state=record.state;
    result.state=state;result.sampleRate=record.sample_rate;result.projectTimeSamples=record.project_time_samples;
    if(state&RAVET_VST3_CONTEXT_SYSTEM_TIME)result.systemTime=record.system_time;
    if(state&RAVET_VST3_CONTEXT_CONTINUOUS)result.continousTimeSamples=record.continuous_time_samples;
    if(state&RAVET_VST3_CONTEXT_MUSIC)result.projectTimeMusic=record.project_time_music;
    if(state&RAVET_VST3_CONTEXT_BAR)result.barPositionMusic=record.bar_position_music;
    if(state&RAVET_VST3_CONTEXT_CYCLE){result.cycleStartMusic=record.cycle_start_music;result.cycleEndMusic=record.cycle_end_music;}
    if(state&RAVET_VST3_CONTEXT_TEMPO)result.tempo=record.tempo;
    if(state&RAVET_VST3_CONTEXT_TIME_SIGNATURE){result.timeSigNumerator=record.time_signature_numerator;result.timeSigDenominator=record.time_signature_denominator;}
    if(state&RAVET_VST3_CONTEXT_CHORD){result.chord.keyNote=uint8_t(record.chord_key);result.chord.rootNote=uint8_t(record.chord_root);result.chord.chordMask=int16_t(record.chord_mask);}
    if(state&RAVET_VST3_CONTEXT_SMPTE){result.smpteOffsetSubframes=record.smpte_offset_subframes;result.frameRate.framesPerSecond=record.frames_per_second;result.frameRate.flags=record.frame_rate_flags;}
    if(state&RAVET_VST3_CONTEXT_CLOCK)result.samplesToNextClock=record.samples_to_next_clock;
    destination=result;return true;
}
inline bool validateContextRequirements(uint32_t flags) {
    using Requirements=Steinberg::Vst::IProcessContextRequirements;
    constexpr uint32_t known=Requirements::kNeedSystemTime|Requirements::kNeedContinousTimeSamples|
        Requirements::kNeedProjectTimeMusic|Requirements::kNeedBarPositionMusic|Requirements::kNeedCycleMusic|
        Requirements::kNeedSamplesToNextClock|Requirements::kNeedTempo|Requirements::kNeedTimeSignature|
        Requirements::kNeedChord|Requirements::kNeedFrameRate|Requirements::kNeedTransportState;
    static_assert(known==2047);return !(flags&~known);
}
}
#endif
