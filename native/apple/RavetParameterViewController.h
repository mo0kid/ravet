#pragma once
#import <CoreAudioKit/CoreAudioKit.h>
#import "RavetAudioUnit.h"
NS_ASSUME_NONNULL_BEGIN
/// Programmatic AUv3 editor using native accessible system sliders. Subclasses
/// select their AUAudioUnit class; embedded editors never create device services.
@interface RavetParameterViewController : AUViewController <AUAudioUnitFactory>
+ (Class)audioUnitClass;
@end
NS_ASSUME_NONNULL_END
