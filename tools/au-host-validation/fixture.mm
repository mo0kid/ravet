#import "GainAudioUnit.h"
#import <Foundation/Foundation.h>
static __weak AUAudioUnit* current;
@interface RustHostedGain : RavetGainAudioUnit
@end
@implementation RustHostedGain
- (instancetype)initWithComponentDescription:(AudioComponentDescription)d options:(AudioComponentInstantiationOptions)o error:(NSError**)e {
 self=[super initWithComponentDescription:d options:o error:e];if(self)current=self;return self;
}
@end
extern "C" void ravet_auh_fixture_register_gain(){
 [AUAudioUnit registerSubclass:RustHostedGain.class asComponentDescription:AudioComponentDescription{kAudioUnitType_Effect,'RvG3','Ravt',0,0} name:@"RAVET: Gain host fixture" version:1];
}
extern "C" void ravet_auh_fixture_main_service(){@autoreleasepool{[[NSRunLoop mainRunLoop] runUntilDate:[NSDate dateWithTimeIntervalSinceNow:.001]];}}
extern "C" bool ravet_auh_fixture_gain_two(){auto unit=current;if(!unit)return false;[unit.parameterTree parameterWithAddress:1].value=2;return true;}
extern "C" int ravet_auh_fixture_run(int (*run)()){ @autoreleasepool{return run();} }
