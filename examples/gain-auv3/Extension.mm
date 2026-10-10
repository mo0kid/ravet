#import <Foundation/Foundation.h>
#import "GainAudioUnit.h"
#import "RavetParameterViewController.h"
@interface RavetGainExtension : RavetParameterViewController
@end
@implementation RavetGainExtension
+ (Class)audioUnitClass { return RavetGainAudioUnit.class; }
@end
