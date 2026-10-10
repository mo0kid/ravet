#import <UIKit/UIKit.h>
#import <AVKit/AVKit.h>
#import "GainAudioUnit.h"
#import "RavetMobileAudio.h"

@interface GainController : UIViewController
@end
@implementation GainController {
    RavetMobileAudio *_audio;
    UILabel *_status;
    UISlider *_gain;
    UISwitch *_bypass;
}
- (void)viewDidLoad {
    [super viewDidLoad]; self.view.backgroundColor = UIColor.systemBackgroundColor;
    self.title = @"RAVET Gain";
    const AudioComponentDescription description{kAudioUnitType_Effect, 'RvG3', 'Ravt', 0, 0};
    [AUAudioUnit registerSubclass:[RavetGainAudioUnit class] asComponentDescription:description name:@"RAVET: Gain" version:1];
    _audio = [[RavetMobileAudio alloc] initWithComponent:description];
    __weak GainController *weak = self;
    _audio.statusChanged = ^(NSString *message) {
        GainController *owner = weak; if (!owner) return;
        owner->_status.text = message;
        if (owner->_audio.running) [owner controlsChanged];
    };
    self.navigationItem.rightBarButtonItem = [[UIBarButtonItem alloc] initWithTitle:@"Audio/MIDI" style:UIBarButtonItemStylePlain target:self action:@selector(settings)];
    _status = [[UILabel alloc] init]; _status.numberOfLines = 0; _status.text = @"Stopped · microphone monitoring uses your output route";
    UILabel *gainLabel = [[UILabel alloc] init]; gainLabel.text = @"Gain (0–4)";
    _gain = [[UISlider alloc] init]; _gain.minimumValue = 0; _gain.maximumValue = 4; _gain.value = 1;
    [_gain addTarget:self action:@selector(controlsChanged) forControlEvents:UIControlEventValueChanged];
    UILabel *bypassLabel = [[UILabel alloc] init]; bypassLabel.text = @"Bypass";
    _bypass = [[UISwitch alloc] init]; [_bypass addTarget:self action:@selector(controlsChanged) forControlEvents:UIControlEventValueChanged];
    UIButton *start = [UIButton buttonWithType:UIButtonTypeSystem]; [start setTitle:@"Start audio" forState:UIControlStateNormal];
    [start addTarget:self action:@selector(start) forControlEvents:UIControlEventTouchUpInside];
    UIButton *stop = [UIButton buttonWithType:UIButtonTypeSystem]; [stop setTitle:@"Stop audio" forState:UIControlStateNormal];
    [stop addTarget:_audio action:@selector(stop) forControlEvents:UIControlEventTouchUpInside];
    AVRoutePickerView *route = [[AVRoutePickerView alloc] init];
    [route.heightAnchor constraintEqualToConstant:44].active = YES;
    UIStackView *stack = [[UIStackView alloc] initWithArrangedSubviews:@[_status, gainLabel, _gain, bypassLabel, _bypass, start, stop, route]];
    stack.axis = UILayoutConstraintAxisVertical; stack.spacing = 16; stack.translatesAutoresizingMaskIntoConstraints = NO;
    UIScrollView *scroll = [[UIScrollView alloc] init]; scroll.translatesAutoresizingMaskIntoConstraints = NO;
    [self.view addSubview:scroll]; [scroll addSubview:stack];
    [NSLayoutConstraint activateConstraints:@[
        [scroll.leadingAnchor constraintEqualToAnchor:self.view.safeAreaLayoutGuide.leadingAnchor],
        [scroll.trailingAnchor constraintEqualToAnchor:self.view.safeAreaLayoutGuide.trailingAnchor],
        [scroll.topAnchor constraintEqualToAnchor:self.view.safeAreaLayoutGuide.topAnchor],
        [scroll.bottomAnchor constraintEqualToAnchor:self.view.safeAreaLayoutGuide.bottomAnchor],
        [stack.widthAnchor constraintEqualToAnchor:scroll.frameLayoutGuide.widthAnchor constant:-48],
        [stack.leadingAnchor constraintEqualToAnchor:scroll.contentLayoutGuide.leadingAnchor constant:24],
        [stack.trailingAnchor constraintEqualToAnchor:scroll.contentLayoutGuide.trailingAnchor constant:-24],
        [stack.topAnchor constraintEqualToAnchor:scroll.contentLayoutGuide.topAnchor constant:24],
        [stack.bottomAnchor constraintEqualToAnchor:scroll.contentLayoutGuide.bottomAnchor constant:-24]]];
    [NSNotificationCenter.defaultCenter addObserver:self selector:@selector(background) name:UIApplicationDidEnterBackgroundNotification object:nil];
}
- (void)background { [_audio stop]; }
- (void)controlsChanged {
    [_audio.audioUnit.parameterTree parameterWithAddress:1].value = _gain.value;
    [_audio.audioUnit.parameterTree parameterWithAddress:2].value = _bypass.on ? 1 : 0;
}
- (void)start {
    __weak GainController *weak = self;
    [AVAudioApplication requestRecordPermissionWithCompletionHandler:^(BOOL granted) {
        dispatch_async(dispatch_get_main_queue(), ^{
            GainController *owner = weak; if (!owner) return;
            if (granted) [owner->_audio start]; else owner->_status.text = @"Enable microphone permission in Settings";
        });
    }];
}
- (void)settings {
    UIAlertController *sheet = [UIAlertController alertControllerWithTitle:@"Audio/MIDI" message:
        @"The system route selects available input/output channels. Preferred rate/buffer requests may negotiate to other values; the status shows the actual configuration." preferredStyle:UIAlertControllerStyleActionSheet];
    for (NSNumber *rate in @[@44100, @48000, @96000])
        [sheet addAction:[UIAlertAction actionWithTitle:[NSString stringWithFormat:@"%.0f Hz", rate.doubleValue] style:UIAlertActionStyleDefault handler:^(UIAlertAction *) {
            BOOL restart = self->_audio.running; [self->_audio stop]; self->_audio.preferredSampleRate = rate.doubleValue; if (restart) [self->_audio start];
        }]];
    for (NSNumber *frames in @[@128, @256, @512])
        [sheet addAction:[UIAlertAction actionWithTitle:[NSString stringWithFormat:@"%@ frames", frames] style:UIAlertActionStyleDefault handler:^(UIAlertAction *) {
            BOOL restart = self->_audio.running; [self->_audio stop]; self->_audio.preferredBufferFrames = frames.unsignedIntegerValue; if (restart) [self->_audio start];
        }]];
    NSArray *inputs = [_audio midiInputs];
    for (NSUInteger i = 0; i < inputs.count; ++i)
        [sheet addAction:[UIAlertAction actionWithTitle:[@"MIDI: " stringByAppendingString:inputs[i]] style:UIAlertActionStyleDefault handler:^(UIAlertAction *) {
            if (![self->_audio selectMIDIInput:i]) self->_status.text = @"Start audio before selecting MIDI";
        }]];
    [sheet addAction:[UIAlertAction actionWithTitle:@"Disconnect MIDI" style:UIAlertActionStyleDefault handler:^(UIAlertAction *) { [self->_audio selectMIDIInput:-1]; }]];
    [sheet addAction:[UIAlertAction actionWithTitle:@"Close" style:UIAlertActionStyleCancel handler:nil]];
    sheet.popoverPresentationController.barButtonItem = self.navigationItem.rightBarButtonItem;
    [self presentViewController:sheet animated:YES completion:nil];
}
@end

@interface GainApp : UIResponder <UIApplicationDelegate>
@property(nonatomic, strong) UIWindow *window;
@end
@implementation GainApp
- (BOOL)application:(UIApplication *)application didFinishLaunchingWithOptions:(NSDictionary *)options {
    (void)application; (void)options;
    self.window = [[UIWindow alloc] initWithFrame:UIScreen.mainScreen.bounds];
    self.window.rootViewController = [[UINavigationController alloc] initWithRootViewController:[[GainController alloc] init]];
    [self.window makeKeyAndVisible]; return YES;
}
@end
#ifdef RAVET_AUV3_CONTRACTS_EMBEDDED
int ravet_auv3_contracts();
#endif
int main(int argc, char **argv) { @autoreleasepool {
#ifdef RAVET_AUV3_CONTRACTS_EMBEDDED
    if (getenv("RAVET_RUN_AUV3_CONTRACTS")) return ravet_auv3_contracts();
#endif
    return UIApplicationMain(argc, argv, nil, NSStringFromClass(GainApp.class));
} }
