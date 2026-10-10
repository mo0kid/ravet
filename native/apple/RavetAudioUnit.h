#pragma once
#import <AudioToolbox/AudioToolbox.h>
#import <AVFoundation/AVFoundation.h>
#include "RavetNativeState.h"

NS_ASSUME_NONNULL_BEGIN
/// AUv3 f32 main-bus adapter for explicitly paired RAVET native entries.
/// Subclasses supply immutable entries and initial bus counts. Hosts own audio
/// scheduling, device routing and editor containment. Control edits/state recall
/// use the native publication; prepare/deallocate require stopped callbacks.
@interface RavetAudioUnit : AUAudioUnit
+ (const RavetNativeStateEntryV1 *)nativeStateEntry;
+ (NSUInteger)initialInputChannels;
+ (NSUInteger)initialOutputChannels;
/// Explicit MIDI input profile. Ordinary effects leave this disabled.
+ (BOOL)acceptsMIDI;
/// Refresh host parameter observers from one complete snapshot, on control.
/// The adapter also services this on the main queue at 30 Hz; no callback timer
/// delivery guarantee is implied. Call explicitly in a headless host without a
/// main run loop when reading cached AUParameter values after direct rendering.
- (void)refreshParameterValues;
@end
NS_ASSUME_NONNULL_END
