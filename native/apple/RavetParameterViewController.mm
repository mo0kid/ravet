#import "RavetParameterViewController.h"
#import <TargetConditionals.h>

@implementation RavetParameterViewController {
    RavetAudioUnit *_audioUnit;
    NSMutableDictionary<NSNumber *, id> *_controls;
    dispatch_source_t _timer;
}
+ (Class)audioUnitClass { return RavetAudioUnit.class; }
- (AUAudioUnit *)createAudioUnitWithComponentDescription:(AudioComponentDescription)description error:(NSError **)error {
    _audioUnit = [[[[self class] audioUnitClass] alloc] initWithComponentDescription:description options:0 error:error];
    __weak RavetParameterViewController *weak = self;
    dispatch_async(dispatch_get_main_queue(), ^{ [weak rebuild]; });
    return _audioUnit;
}
- (void)loadView {
#if TARGET_OS_OSX
    self.view = [[NSView alloc] initWithFrame:NSMakeRect(0, 0, 420, 280)];
#else
    self.view = [[UIView alloc] initWithFrame:CGRectMake(0, 0, 420, 280)];
    self.view.backgroundColor = UIColor.systemBackgroundColor;
#endif
}
- (void)viewDidLoad {
    [super viewDidLoad]; [self rebuild];
    __weak RavetParameterViewController *weak = self;
    _timer = dispatch_source_create(DISPATCH_SOURCE_TYPE_TIMER, 0, 0, dispatch_get_main_queue());
    dispatch_source_set_timer(_timer, dispatch_time(DISPATCH_TIME_NOW, NSEC_PER_SEC / 30), NSEC_PER_SEC / 30, NSEC_PER_SEC / 300);
    dispatch_source_set_event_handler(_timer, ^{ [weak refresh]; }); dispatch_resume(_timer);
}
- (void)dealloc { if (_timer) dispatch_source_cancel(_timer); }
- (void)rebuild {
    if (!self.isViewLoaded || !_audioUnit) return;
    for (id child in self.view.subviews) [child removeFromSuperview];
    _controls = [NSMutableDictionary dictionary];
#if TARGET_OS_OSX
    NSStackView *stack = [[NSStackView alloc] init]; stack.orientation = NSUserInterfaceLayoutOrientationVertical;
    stack.alignment = NSLayoutAttributeLeading;
#else
    UIStackView *stack = [[UIStackView alloc] init]; stack.axis = UILayoutConstraintAxisVertical;
#endif
    stack.spacing = 12; stack.translatesAutoresizingMaskIntoConstraints = NO;
    for (AUParameter *p in _audioUnit.parameterTree.allParameters) {
#if TARGET_OS_OSX
        NSTextField *label = [NSTextField labelWithString:p.displayName];
        NSSlider *slider = [NSSlider sliderWithValue:0 minValue:0 maxValue:1 target:self action:@selector(change:)];
        slider.continuous = YES; slider.enabled = (p.flags & kAudioUnitParameterFlag_IsWritable) != 0;
        slider.tag = NSInteger(p.address); slider.accessibilityLabel = p.displayName;
        [stack addArrangedSubview:label]; [stack addArrangedSubview:slider];
#else
        UILabel *label = [[UILabel alloc] init]; label.text = p.displayName;
        UISlider *slider = [[UISlider alloc] init]; slider.minimumValue = 0; slider.maximumValue = 1;
        slider.enabled = (p.flags & kAudioUnitParameterFlag_IsWritable) != 0;
        slider.tag = NSInteger(p.address); slider.accessibilityLabel = p.displayName;
        [slider addTarget:self action:@selector(change:) forControlEvents:UIControlEventValueChanged];
        [slider addTarget:self action:@selector(begin:) forControlEvents:UIControlEventTouchDown];
        [slider addTarget:self action:@selector(end:) forControlEvents:UIControlEventTouchUpInside | UIControlEventTouchUpOutside | UIControlEventTouchCancel];
        [stack addArrangedSubview:label]; [stack addArrangedSubview:slider];
#endif
        _controls[@(p.address)] = slider;
    }
    [self.view addSubview:stack];
    [NSLayoutConstraint activateConstraints:@[[stack.leadingAnchor constraintEqualToAnchor:self.view.leadingAnchor constant:20],
        [stack.trailingAnchor constraintEqualToAnchor:self.view.trailingAnchor constant:-20],
        [stack.topAnchor constraintEqualToAnchor:self.view.topAnchor constant:20]]];
    [self refresh];
}
- (void)refresh {
    if (!_audioUnit) return;
    const auto *parameters = [[_audioUnit class] nativeStateEntry]->parameters_entry();
    [_audioUnit refreshParameterValues];
    for (AUParameter *p in _audioUnit.parameterTree.allParameters) {
        float value = 0; if (parameters->normalize(uint32_t(p.address), p.value, &value)) continue;
#if TARGET_OS_OSX
        NSSlider *slider = _controls[@(p.address)];
        if (self.view.window.firstResponder != slider) slider.doubleValue = value;
#else
        UISlider *slider = _controls[@(p.address)]; if (!slider.tracking) slider.value = value;
#endif
    }
}
- (void)change:(id)sender {
    AUParameter *p = [_audioUnit.parameterTree parameterWithAddress:[sender tag]];
    const auto *parameters = [[_audioUnit class] nativeStateEntry]->parameters_entry();
    float physical = 0;
#if TARGET_OS_OSX
    const float normalized = float([sender doubleValue]);
#else
    const float normalized = [(UISlider *)sender value];
#endif
    if (!parameters->denormalize(uint32_t(p.address), normalized, &physical)) [p setValue:physical originator:nil];
}
#if !TARGET_OS_OSX
- (void)begin:(UISlider *)slider {
    AUParameter *p = [_audioUnit.parameterTree parameterWithAddress:slider.tag];
    [p setValue:p.value originator:nil atHostTime:0 eventType:AUParameterAutomationEventTypeTouch];
}
- (void)end:(UISlider *)slider {
    AUParameter *p = [_audioUnit.parameterTree parameterWithAddress:slider.tag];
    [p setValue:p.value originator:nil atHostTime:0 eventType:AUParameterAutomationEventTypeRelease];
}
#endif
@end
