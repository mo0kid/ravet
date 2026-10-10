#pragma once
#import <AVFoundation/AVFoundation.h>
#import "RavetAudioUnit.h"

NS_ASSUME_NONNULL_BEGIN
/// Main-thread iOS/iPadOS standalone effect host. Microphone permission belongs
/// to the app. Audio interruptions and route changes stop/rebuild the engine;
/// callers stop before destruction. Embedded AUv3 editors never use this host.
@interface RavetMobileAudio : NSObject
@property(nonatomic, readonly, nullable) AUAudioUnit *audioUnit;
@property(nonatomic, readonly) BOOL running;
@property(nonatomic) double preferredSampleRate;
@property(nonatomic) NSUInteger preferredBufferFrames;
@property(nonatomic, copy, nullable) void (^statusChanged)(NSString *status);
- (instancetype)initWithComponent:(AudioComponentDescription)description;
- (void)start;
- (void)stop;
/// Available CoreMIDI source names; an explicit index selects one connection.
- (NSArray<NSString *> *)midiInputs;
- (BOOL)selectMIDIInput:(NSInteger)index; // -1 disconnects.
@end
NS_ASSUME_NONNULL_END
