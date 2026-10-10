#import "RavetAudioUnit.h"
#import <AudioToolbox/AUAudioUnitImplementation.h>
#include "RavetNativeSession.hpp"
#include <algorithm>
#include <limits>
#include <memory>

namespace {
struct BufferList { UInt32 count; AudioBuffer buffers[64]; };
struct Ramp { uint32_t remaining = 0; double target = 0, step = 0; };
struct Scheduled { RavetNativeEventV1 event{}; uint32_t duration = 0; };
thread_local const void *refreshingUnit = nullptr;
bool midiValid(const AUMIDIEvent &m) {
    if (m.cable || m.length < 1 || m.length > 3 || m.data[0] < 0x80) return false;
    const uint8_t s = m.data[0];
    const unsigned length = s < 0xF0 ? ((s & 0xE0) == 0xC0 ? 2 : 3) :
        (s == 0xF1 || s == 0xF3 ? 2 : s == 0xF2 ? 3 :
         s == 0xF6 || s == 0xF8 || s == 0xFA || s == 0xFB || s == 0xFC || s == 0xFE || s == 0xFF ? 1 : 0);
    if (m.length != length) return false;
    for (unsigned i = 1; i < length; ++i) if (m.data[i] > 127) return false;
    return true;
}
void silence(AudioBufferList *list, uint32_t frames) {
    if (!list || list->mNumberBuffers > 64) return;
    for (uint32_t c = 0; c < list->mNumberBuffers; ++c)
        if (list->mBuffers[c].mData)
            std::memset(list->mBuffers[c].mData, 0, std::min<size_t>(list->mBuffers[c].mDataByteSize, size_t(frames) * 4));
}
NSError *error(OSStatus code, NSString *message) {
    return [NSError errorWithDomain:NSOSStatusErrorDomain code:code userInfo:@{NSLocalizedDescriptionKey: message}];
}
}

@implementation RavetAudioUnit {
    std::unique_ptr<ravet::NativeSession> _session;
    AUAudioUnitBusArray *_inputs, *_outputs;
    AUParameterTree *_tree;
    std::vector<float> _inputStorage, _pullStorage, _outputStorage;
    BufferList _inputList;
    std::array<const float *, 64> _inputPlanes;
    std::array<float *, 64> _outputPlanes;
    std::array<Scheduled, 1024> _scheduled;
    std::array<Ramp, ravet::NativeSession::maximumParameters> _ramps;
    uint32_t _inputChannels, _outputChannels, _maximumFrames;
    double _sampleRate;
    RavetNativeProcessorTimingV1 _timing;
    dispatch_source_t _parameterTimer;
}
+ (const RavetNativeStateEntryV1 *)nativeStateEntry { return nullptr; }
+ (NSUInteger)initialInputChannels { return 2; }
+ (NSUInteger)initialOutputChannels { return 2; }
+ (BOOL)acceptsMIDI { return NO; }
- (BOOL)isMusicDeviceOrEffect { return [[self class] acceptsMIDI]; }
- (NSInteger)virtualMIDICableCount { return [[self class] acceptsMIDI] ? 1 : 0; }
- (MIDIProtocolID)AudioUnitMIDIProtocol { return kMIDIProtocol_2_0; }
- (instancetype)initWithComponentDescription:(AudioComponentDescription)description
                                    options:(AudioComponentInstantiationOptions)options error:(NSError **)outError {
    self = [super initWithComponentDescription:description options:options error:outError];
    if (!self) return nil;
    try { _session = std::make_unique<ravet::NativeSession>([[self class] nativeStateEntry]); }
    catch (const std::exception &e) {
        if (outError) *outError = error(kAudioUnitErr_FailedInitialization, @(e.what()));
        return nil;
    }
    const NSUInteger ins = [[self class] initialInputChannels], outs = [[self class] initialOutputChannels];
    if (ins > 64 || outs < 1 || outs > 64) {
        if (outError) *outError = error(kAudioUnitErr_FormatNotSupported, @"Invalid initial channel counts");
        return nil;
    }
    AUAudioUnitBus *output = [[AUAudioUnitBus alloc] initWithFormat:[[AVAudioFormat alloc]
        initStandardFormatWithSampleRate:48000 channels:(AVAudioChannelCount)outs] error:outError];
    if (!output) return nil;
    output.maximumChannelCount = 64;
    _outputs = [[AUAudioUnitBusArray alloc] initWithAudioUnit:self busType:AUAudioUnitBusTypeOutput busses:@[output]];
    NSArray *inputBusses = @[];
    if (ins) {
        AUAudioUnitBus *input = [[AUAudioUnitBus alloc] initWithFormat:[[AVAudioFormat alloc]
            initStandardFormatWithSampleRate:48000 channels:(AVAudioChannelCount)ins] error:outError];
        if (!input) return nil;
        input.maximumChannelCount = 64;
        inputBusses = @[input];
    }
    _inputs = [[AUAudioUnitBusArray alloc] initWithAudioUnit:self busType:AUAudioUnitBusTypeInput busses:inputBusses];
    NSMutableArray *nodes = [NSMutableArray array];
    for (uint32_t i = 0; i < _session->count(); ++i) {
        const auto &p = _session->schema[i];
        NSString *name = [[NSString alloc] initWithBytes:p.name length:p.name_length encoding:NSUTF8StringEncoding];
        NSString *unit = [[NSString alloc] initWithBytes:p.unit length:p.unit_length encoding:NSUTF8StringEncoding];
        AudioUnitParameterOptions flags = kAudioUnitParameterFlag_IsReadable;
        if (!(p.flags & RAVET_PARAMETER_READ_ONLY)) flags |= kAudioUnitParameterFlag_IsWritable;
        if (!(p.flags & RAVET_PARAMETER_AUTOMATABLE)) flags |= kAudioUnitParameterFlag_NonRealTime;
        if (p.scale == 1) flags |= kAudioUnitParameterFlag_DisplayLogarithmic;
        AUParameter *parameter = [AUParameterTree createParameterWithIdentifier:[NSString stringWithFormat:@"p%u", p.id]
            name:name address:p.id min:p.minimum max:p.maximum unit:p.intervals ? kAudioUnitParameterUnit_Indexed : kAudioUnitParameterUnit_CustomUnit
            unitName:unit flags:flags valueStrings:nil dependentParameters:nil];
        parameter.value = p.default_value;
        [nodes addObject:parameter];
    }
    _tree = [AUParameterTree createTreeWithChildren:nodes];
    __weak RavetAudioUnit *weakSelf = self;
    _tree.implementorValueObserver = ^(AUParameter *p, AUValue value) {
        RavetAudioUnit *owner = weakSelf;
        if (owner && refreshingUnit != (__bridge const void *)owner) owner->_session->set(uint32_t(p.address), value);
    };
    _tree.implementorValueProvider = ^AUValue(AUParameter *p) {
        RavetAudioUnit *owner = weakSelf;
        return owner ? owner->_session->get(uint32_t(p.address)) : 0;
    };
    self.maximumFramesToRender = 4096;
    _parameterTimer = dispatch_source_create(DISPATCH_SOURCE_TYPE_TIMER, 0, 0, dispatch_get_main_queue());
    dispatch_source_set_timer(_parameterTimer, dispatch_time(DISPATCH_TIME_NOW, NSEC_PER_SEC / 30), NSEC_PER_SEC / 30, NSEC_PER_SEC / 300);
    dispatch_source_set_event_handler(_parameterTimer, ^{ [weakSelf refreshParameterValues]; });
    dispatch_resume(_parameterTimer);
    return self;
}
- (void)dealloc { if (_parameterTimer) dispatch_source_cancel(_parameterTimer); }
- (void)refreshParameterValues {
    std::array<RavetNativeParameterValueV1, ravet::NativeSession::maximumParameters> values{};
    RavetNativeStateRevisionV1 revision{};
    if (_session->state->snapshot(_session->publication, values.data(), _session->count(), &revision)) return;
    const void *previous = refreshingUnit; refreshingUnit = (__bridge const void *)self;
    for (uint32_t i = 0; i < _session->count(); ++i) {
        AUParameter *p = [_tree parameterWithAddress:values[i].id];
        if (p.value != values[i].value) [p setValue:values[i].value originator:nil];
    }
    refreshingUnit = previous;
}
- (AUAudioUnitBusArray *)inputBusses { return _inputs; }
- (AUAudioUnitBusArray *)outputBusses { return _outputs; }
- (AUParameterTree *)parameterTree { return _tree; }
- (BOOL)shouldBypassEffect {
    if (!_session) return [super shouldBypassEffect];
    for (uint32_t i = 0; i < _session->count(); ++i)
        if (_session->schema[i].flags & RAVET_PARAMETER_BYPASS)
            return _session->get(_session->schema[i].id) != _session->schema[i].minimum;
    return [super shouldBypassEffect];
}
- (void)setShouldBypassEffect:(BOOL)bypass {
    if (refreshingUnit == (__bridge const void *)self) return;
    if (!_session) { [super setShouldBypassEffect:bypass]; return; }
    for (uint32_t i = 0; i < _session->count(); ++i)
        if (_session->schema[i].flags & RAVET_PARAMETER_BYPASS) {
            const auto &p = _session->schema[i]; _session->set(p.id, bypass ? p.maximum : p.minimum);
            return;
        }
    [super setShouldBypassEffect:bypass];
}
- (BOOL)canProcessInPlace { return YES; }
- (BOOL)allocateRenderResourcesAndReturnError:(NSError **)outError {
    AVAudioFormat *output = _outputs[0].format;
    AVAudioFormat *input = _inputs.count ? _inputs[0].format : nil;
    if (output.commonFormat != AVAudioPCMFormatFloat32 || output.interleaved || output.channelCount > 64 ||
        !output.channelCount || (input && (input.commonFormat != AVAudioPCMFormatFloat32 || input.interleaved ||
        input.channelCount > 64 || input.sampleRate != output.sampleRate)) ||
        self.maximumFramesToRender < 1 || self.maximumFramesToRender > 65536) {
        if (outError) *outError = error(kAudioUnitErr_FormatNotSupported, @"Use bounded noninterleaved f32 buses at one rate");
        return NO;
    }
    if (![super allocateRenderResourcesAndReturnError:outError]) return NO;
    _inputChannels = input ? input.channelCount : 0;
    _outputChannels = output.channelCount; _sampleRate = output.sampleRate;
    _maximumFrames = self.maximumFramesToRender;
    try {
        _inputStorage.resize(size_t(_inputChannels) * _maximumFrames);
        _pullStorage.resize(size_t(_inputChannels) * _maximumFrames);
        _outputStorage.resize(size_t(_outputChannels) * _maximumFrames);
        if (!_session->prepare(_sampleRate, _inputChannels, _outputChannels, _maximumFrames))
            throw std::runtime_error("Processor rejected bus/rate configuration");
        if (_session->processor->timing(_session->dsp, &_timing))
            throw std::runtime_error("Processor timing query failed");
    } catch (const std::exception &e) {
        _session->stop(); [super deallocateRenderResources];
        if (outError) *outError = error(kAudioUnitErr_FailedInitialization, @(e.what()));
        return NO;
    }
    for (auto &r : _ramps) r = {};
    return YES;
}
- (void)deallocateRenderResources { _session->stop(); [super deallocateRenderResources]; }
- (void)reset {
    if (_session->prepared) _session->processor->reset(_session->dsp);
    for (auto &r : _ramps) r = {};
}
- (NSTimeInterval)latency {
    return self.renderResourcesAllocated ? _timing.latency_samples / _sampleRate : 0;
}
- (NSTimeInterval)tailTime {
    if (!self.renderResourcesAllocated) return 0;
    return _timing.tail_kind == 2 ? std::numeric_limits<double>::infinity() : _timing.tail_samples / _sampleRate;
}
- (NSDictionary *)fullState {
    const auto bytes = _session->save();
    if (bytes.empty()) return nil;
    NSMutableDictionary *result = [[super fullState] mutableCopy] ?: [NSMutableDictionary dictionary];
    result[@"ravet.native.state"] = [NSData dataWithBytes:bytes.data() length:bytes.size()];
    return result;
}
- (void)setFullState:(NSDictionary *)state {
    id data = state[@"ravet.native.state"];
    if (![data isKindOfClass:[NSData class]] || [data length] > 8208 ||
        !_session->restore((const uint8_t *)[data bytes], uint32_t([data length]))) return;
    // The canonical native record is authoritative over potentially cached AU
    // parameter values in the superclass dictionary. Metadata recall must not
    // publish those stale values over the complete native restore.
    const void *previous = refreshingUnit; refreshingUnit = (__bridge const void *)self;
    [super setFullState:state];
    refreshingUnit = previous;
    [self refreshParameterValues];
}
- (NSDictionary *)fullStateForDocument { return self.fullState; }
- (void)setFullStateForDocument:(NSDictionary *)state { self.fullState = state; }
- (AUInternalRenderBlock)internalRenderBlock {
    // Host must retain this AudioUnit until its cached render block is retired.
    __unsafe_unretained RavetAudioUnit *unit = self;
    return ^AUAudioUnitStatus(AudioUnitRenderActionFlags *flags, const AudioTimeStamp *time,
        AUAudioFrameCount frames, NSInteger bus, AudioBufferList *output, const AURenderEvent *events,
        AURenderPullInputBlock pull) {
        return [unit render:flags time:time frames:frames bus:bus output:output events:events pull:pull];
    };
}
- (AUAudioUnitStatus)render:(AudioUnitRenderActionFlags *)flags time:(const AudioTimeStamp *)time
    frames:(uint32_t)frames bus:(NSInteger)bus output:(AudioBufferList *)output
    events:(const AURenderEvent *)events pull:(AURenderPullInputBlock)pull {
    auto fail = [&](OSStatus code) { silence(output, frames); if (flags) *flags |= kAudioUnitRenderAction_OutputIsSilence; return code; };
    if (!_session->prepared || bus != 0 || !time || !output || frames > _maximumFrames ||
        output->mNumberBuffers != _outputChannels) return fail(kAudioUnitErr_Uninitialized);
    for (uint32_t c = 0; c < _outputChannels; ++c) {
        auto &buffer = output->mBuffers[c];
        if (buffer.mNumberChannels != 1 || (buffer.mData && buffer.mDataByteSize < frames * 4)) return fail(kAudioUnitErr_FormatNotSupported);
        if (!buffer.mData) { buffer.mData = _outputStorage.data() + c * _maximumFrames; buffer.mDataByteSize = frames * 4; }
        _outputPlanes[c] = static_cast<float *>(buffer.mData);
        const uintptr_t start = reinterpret_cast<uintptr_t>(buffer.mData);
        if (start % alignof(float) || start > UINTPTR_MAX - size_t(frames) * 4) return fail(kAudioUnitErr_FormatNotSupported);
        for (uint32_t previousChannel = 0; previousChannel < c; ++previousChannel) {
            const uintptr_t other = reinterpret_cast<uintptr_t>(_outputPlanes[previousChannel]);
            if (frames && start < other + size_t(frames) * 4 && other < start + size_t(frames) * 4)
                return fail(kAudioUnitErr_FormatNotSupported);
        }
    }
    // Validate the complete bounded list before input pull or DSP mutation.
    uint32_t count = 0, previous = 0, links = 0;
    for (const AURenderEvent *event = events; event; event = event->head.next) {
        if (++links > 1024 || event->head.reserved || !std::isfinite(time->mSampleTime)) return fail(kAudioUnitErr_InvalidParameter);
        double relative = event->head.eventSampleTime == AUEventSampleTimeImmediate ? 0 : double(event->head.eventSampleTime) - time->mSampleTime;
        relative = std::max(0.0, relative);
        if (relative > UINT32_MAX || (frames ? relative >= frames : relative != 0) || relative != std::floor(relative) || relative < previous)
            return fail(kAudioUnitErr_InvalidParameter);
        previous = uint32_t(relative);
        auto append = [&](RavetNativeEventV1 native, uint32_t duration) {
            if (count == _scheduled.size()) return false;
            native.offset = previous; _scheduled[count++] = {native, duration}; return true;
        };
        if (event->head.eventType == AURenderEventParameter || event->head.eventType == AURenderEventParameterRamp) {
            const auto &p = event->parameter;
            if (p.parameterAddress > UINT32_MAX || p.reserved[1] || p.reserved[2]) return fail(kAudioUnitErr_InvalidParameter);
            const int slot = _session->index(uint32_t(p.parameterAddress));
            if (slot < 0 || !std::isfinite(p.value) || p.value < _session->schema[slot].minimum ||
                p.value > _session->schema[slot].maximum || (_session->schema[slot].flags & RAVET_PARAMETER_READ_ONLY))
                return fail(kAudioUnitErr_InvalidParameter);
            RavetNativeEventV1 native{}; native.kind = RAVET_NATIVE_PARAMETER_VALUE;
            native.parameter_id = uint32_t(p.parameterAddress); native.value = double(p.value);
            if (!append(native, event->head.eventType == AURenderEventParameterRamp ? p.rampDurationSampleFrames : 0)) return fail(kAudioUnitErr_TooManyFramesToProcess);
        } else if (event->head.eventType == AURenderEventMIDI) {
            if (![[self class] acceptsMIDI] || !midiValid(event->MIDI)) return fail(kAudioUnitErr_InvalidParameter);
            RavetNativeEventV1 native{}; native.kind = RAVET_NATIVE_MIDI; native.detail = event->MIDI.length;
            for (unsigned i = 0; i < native.detail; ++i) native.words[0] |= uint32_t(event->MIDI.data[i]) << (8 * i);
            if (!append(native, 0)) return fail(kAudioUnitErr_TooManyFramesToProcess);
        } else if (event->head.eventType == AURenderEventMIDIEventList) {
            const auto &m = event->MIDIEventsList;
            if (![[self class] acceptsMIDI] || m.cable || m.eventList.numPackets > 1024 ||
                (m.eventList.protocol != kMIDIProtocol_1_0 && m.eventList.protocol != kMIDIProtocol_2_0)) return fail(kAudioUnitErr_InvalidParameter);
            const MIDIEventPacket *packet = &m.eventList.packet[0];
            static constexpr uint32_t sizes[] = {1,1,1,2,2,4,1,1,2,2,2,3,3,4,4,4};
            for (uint32_t i = 0; i < m.eventList.numPackets; ++i) {
                if (packet->wordCount > 4096) return fail(kAudioUnitErr_InvalidParameter);
                for (uint32_t word = 0; word < packet->wordCount;) {
                    RavetNativeEventV1 native{}; native.kind = RAVET_NATIVE_UMP;
                    native.detail = sizes[packet->words[word] >> 28];
                    if (native.detail > packet->wordCount - word) return fail(kAudioUnitErr_InvalidParameter);
                    for (uint32_t j = 0; j < native.detail; ++j) native.words[j] = packet->words[word++];
                    if (!append(native, 0)) return fail(kAudioUnitErr_TooManyFramesToProcess);
                }
                packet = MIDIEventPacketNext(packet);
            }
        } else return fail(kAudioUnitErr_InvalidParameter);
    }
    if (count > 1024 - _session->count()) return fail(kAudioUnitErr_TooManyFramesToProcess);
    RavetNativeTransportV1 transport{};
    AUHostMusicalContextBlock musical = self.musicalContextBlock;
    AUHostTransportStateBlock hostTransport = self.transportStateBlock;
    double numerator = 0, beats = 0, tempo = 0, bar = 0;
    NSInteger denominator = 0;
    if (musical && musical(&tempo, &numerator, &denominator, &beats, nullptr, &bar)) {
        if (!std::isfinite(tempo) || tempo <= 0 || !std::isfinite(beats) || !std::isfinite(bar) ||
            !std::isfinite(numerator) || numerator < 1 || numerator > 65535 || numerator != std::floor(numerator) ||
            denominator < 1 || denominator > 65535) return fail(kAudioUnitErr_InvalidParameter);
        transport.present = RAVET_TRANSPORT_TEMPO | RAVET_TRANSPORT_BEATS | RAVET_TRANSPORT_DETAILS |
            RAVET_TRANSPORT_SIGNATURE | RAVET_TRANSPORT_BAR;
        transport.tempo = tempo; transport.beat_position = beats; transport.bar_start = bar;
        transport.signature_numerator = uint32_t(numerator); transport.signature_denominator = uint32_t(denominator);
    }
    AUHostTransportStateFlags stateFlags = 0;
    double samples = 0, loopStart = 0, loopEnd = 0;
    if (hostTransport && hostTransport(&stateFlags, &samples, &loopStart, &loopEnd)) {
        if (!std::isfinite(samples) || samples < -0x1p63 || samples >= 0x1p63) return fail(kAudioUnitErr_InvalidParameter);
        transport.present |= RAVET_TRANSPORT_PLAYING | RAVET_TRANSPORT_SAMPLES | RAVET_TRANSPORT_DETAILS;
        transport.sample_position = int64_t(samples);
        if (stateFlags & AUHostTransportStateMoving) transport.flags |= RAVET_TRANSPORT_FLAG_PLAYING;
        if (stateFlags & AUHostTransportStateChanged) transport.flags |= RAVET_TRANSPORT_FLAG_DISCONTINUITY;
        if (stateFlags & AUHostTransportStateRecording) transport.flags |= RAVET_TRANSPORT_FLAG_RECORDING;
        if (stateFlags & AUHostTransportStateCycling) {
            transport.flags |= RAVET_TRANSPORT_FLAG_LOOP_ACTIVE;
            if (transport.present & RAVET_TRANSPORT_BEATS) {
                if (!std::isfinite(loopStart) || !std::isfinite(loopEnd) || loopStart >= loopEnd) return fail(kAudioUnitErr_InvalidParameter);
                transport.present |= RAVET_TRANSPORT_BEAT_LOOP; transport.beat_loop_start = loopStart; transport.beat_loop_end = loopEnd;
            }
        }
    }
    if (transport.flags & RAVET_TRANSPORT_FLAG_PLAYING) {
        if ((transport.present & RAVET_TRANSPORT_SAMPLES) && transport.sample_position > INT64_MAX - frames)
            return fail(kAudioUnitErr_InvalidParameter);
        if ((transport.present & (RAVET_TRANSPORT_BEATS | RAVET_TRANSPORT_TEMPO)) == (RAVET_TRANSPORT_BEATS | RAVET_TRANSPORT_TEMPO) &&
            !std::isfinite(transport.beat_position + frames * (transport.tempo / (60 * _sampleRate))))
            return fail(kAudioUnitErr_InvalidParameter);
    }
    if (_inputChannels) {
        if (!pull) return fail(kAudioUnitErr_NoConnection);
        _inputList.count = _inputChannels;
        for (uint32_t c = 0; c < _inputChannels; ++c)
            _inputList.buffers[c] = {1, frames * 4, _pullStorage.data() + c * _maximumFrames};
        const OSStatus code = pull(flags, time, frames, 0, reinterpret_cast<AudioBufferList *>(&_inputList));
        if (code) return fail(code);
        if (_inputList.count != _inputChannels) return fail(kAudioUnitErr_FormatNotSupported);
        for (uint32_t c = 0; c < _inputChannels; ++c) {
            const auto &buffer = _inputList.buffers[c];
            if (buffer.mNumberChannels != 1 || buffer.mDataByteSize < frames * 4 || (frames && !buffer.mData)) return fail(kAudioUnitErr_FormatNotSupported);
            // Pull may supply aliased host input. Copy every channel before the
            // first segmented output write, including cross-channel aliases.
            if (frames) std::memmove(_inputStorage.data() + c * _maximumFrames, buffer.mData, frames * 4);
            _inputPlanes[c] = _inputStorage.data() + c * _maximumFrames;
        }
    }
    bool changed = false;
    if (_session->apply(&changed)) return fail(kAudioUnitErr_CannotDoInCurrentContext);
    if (changed) for (auto &r : _ramps) r = {};
    // Segment ordinary blocks; active ramps render one sample at a time, with
    // bounded physical events and no expansion/allocation of a whole ramp.
    uint32_t cursor = 0, position = 0;
    do {
        std::array<RavetNativeEventV1, 1024 + ravet::NativeSession::maximumParameters> native{};
        uint32_t n = 0;
        for (uint32_t i = 0; i < _session->count(); ++i) if (_ramps[i].remaining && frames) {
            auto &r = _ramps[i]; auto &value = _session->audioValues[i];
            value.value = --r.remaining ? float(double(value.value) + r.step) : float(r.target);
            native[n].kind = RAVET_NATIVE_PARAMETER_VALUE; native[n].parameter_id = value.id; native[n++].value = double(value.value);
        }
        while (cursor < count && _scheduled[cursor].event.offset == position) {
            const auto &s = _scheduled[cursor++];
            if (s.event.kind == RAVET_NATIVE_PARAMETER_VALUE) {
                const int slot = _session->index(s.event.parameter_id);
                if (s.duration) {
                    _ramps[slot] = {s.duration, s.event.value, (s.event.value - _session->audioValues[slot].value) / s.duration};
                    continue;
                }
                _ramps[slot] = {}; _session->audioValues[slot].value = float(s.event.value);
            }
            native[n] = s.event; native[n++].offset = 0;
        }
        if (n > 1024) return fail(kAudioUnitErr_TooManyFramesToProcess);
        bool ramping = false;
        for (const auto &r : _ramps) ramping |= r.remaining != 0;
        uint32_t length = frames ? (ramping ? 1 : (cursor < count ? _scheduled[cursor].event.offset : frames) - position) : 0;
        std::array<const float *, 64> in{}; std::array<float *, 64> out{};
        for (uint32_t c = 0; c < _inputChannels; ++c) in[c] = _inputPlanes[c] + position;
        for (uint32_t c = 0; c < _outputChannels; ++c) out[c] = _outputPlanes[c] + position;
        auto segmentTransport = transport;
        if (position) {
            segmentTransport.flags &= ~RAVET_TRANSPORT_FLAG_DISCONTINUITY;
            if (transport.flags & RAVET_TRANSPORT_FLAG_PLAYING) {
                if (transport.present & RAVET_TRANSPORT_SAMPLES) {
                    if (transport.sample_position > INT64_MAX - position) return fail(kAudioUnitErr_InvalidParameter);
                    segmentTransport.sample_position += position;
                }
                if ((transport.present & (RAVET_TRANSPORT_BEATS | RAVET_TRANSPORT_TEMPO)) == (RAVET_TRANSPORT_BEATS | RAVET_TRANSPORT_TEMPO))
                    segmentTransport.beat_position += position * (transport.tempo / (60 * _sampleRate));
            }
        }
        if (_session->render(in.data(), out.data(), _inputChannels, _outputChannels, length, native.data(), n, &segmentTransport)) return fail(kAudioUnitErr_CannotDoInCurrentContext);
        if (_session->generatedCount) return fail(kAudioUnitErr_InvalidParameter); // Output event routing requires a consumer profile.
        position += length;
    } while (position < frames);
    const int reflected = _session->reflect();
    if (reflected != 0 && reflected != 3 && reflected != 4) return fail(kAudioUnitErr_CannotDoInCurrentContext);
    if (flags) *flags &= ~kAudioUnitRenderAction_OutputIsSilence;
    return noErr;
}
@end
