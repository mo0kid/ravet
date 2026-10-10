#import "RavetMobileAudio.h"
#import <CoreMIDI/CoreMIDI.h>

@implementation RavetMobileAudio {
    AVAudioEngine *_engine;
    AVAudioUnit *_node;
    AudioComponentDescription _description;
    BOOL _wanted, _starting;
    NSUInteger _generation;
    id _interruption, _route, _reset;
    MIDIClientRef _midiClient;
    MIDIPortRef _midiPort;
    MIDIEndpointRef _midiSource;
    AUScheduleMIDIEventBlock _midiSchedule;
    NSDictionary *_savedState;
    SInt32 _selectedMidiID;
}
- (instancetype)initWithComponent:(AudioComponentDescription)description {
    self = [super init]; if (!self) return nil;
    _description = description; _preferredSampleRate = 48000; _preferredBufferFrames = 256;
    __weak RavetMobileAudio *weak = self;
    NSNotificationCenter *center = NSNotificationCenter.defaultCenter;
    _interruption = [center addObserverForName:AVAudioSessionInterruptionNotification object:nil queue:NSOperationQueue.mainQueue
        usingBlock:^(NSNotification *note) {
            RavetMobileAudio *owner = weak; if (!owner) return;
            if ([note.userInfo[AVAudioSessionInterruptionTypeKey] unsignedIntegerValue] == AVAudioSessionInterruptionTypeBegan) {
                [owner suspend]; [owner report:@"Audio interrupted"];
            } else if (owner->_wanted && ([note.userInfo[AVAudioSessionInterruptionOptionKey] unsignedIntegerValue] & AVAudioSessionInterruptionOptionShouldResume)) [owner start];
        }];
    _route = [center addObserverForName:AVAudioSessionRouteChangeNotification object:nil queue:NSOperationQueue.mainQueue
        usingBlock:^(NSNotification *) { RavetMobileAudio *owner = weak; if (owner && owner->_wanted) { [owner suspend]; [owner start]; } }];
    _reset = [center addObserverForName:AVAudioSessionMediaServicesWereResetNotification object:nil queue:NSOperationQueue.mainQueue
        usingBlock:^(NSNotification *) { RavetMobileAudio *owner = weak; if (owner && owner->_wanted) { [owner suspend]; [owner start]; } }];
    return self;
}
- (void)dealloc {
    [_engine stop];
    if (_midiPort) MIDIPortDispose(_midiPort);
    if (_midiClient) MIDIClientDispose(_midiClient);
    for (id token in @[_interruption, _route, _reset]) [NSNotificationCenter.defaultCenter removeObserver:token];
}
- (AUAudioUnit *)audioUnit { return _node.AUAudioUnit; }
- (BOOL)running { return _engine.running; }
- (void)report:(NSString *)message { if (self.statusChanged) self.statusChanged(message); }
- (void)suspend {
    ++_generation; _starting = NO;
    [self disconnectMIDI];
    [_engine stop];
    _savedState = _node.AUAudioUnit.fullState ?: _savedState;
    _node = nil; _engine = nil;
}
- (void)stop {
    _wanted = NO; [self suspend];
    [AVAudioSession.sharedInstance setActive:NO withOptions:AVAudioSessionSetActiveOptionNotifyOthersOnDeactivation error:nil];
    [self report:@"Stopped"];
}
- (void)start {
    NSAssert(NSThread.isMainThread, @"Standalone lifecycle requires main thread");
    _wanted = YES;
    if (_starting || _engine.running) return;
    AVAudioSession *session = AVAudioSession.sharedInstance;
    NSError *error = nil;
    if (![session setCategory:AVAudioSessionCategoryPlayAndRecord mode:AVAudioSessionModeMeasurement
        options:AVAudioSessionCategoryOptionDefaultToSpeaker | AVAudioSessionCategoryOptionAllowBluetoothHFP error:&error] ||
        ![session setPreferredSampleRate:_preferredSampleRate error:&error] ||
        ![session setPreferredIOBufferDuration:_preferredBufferFrames / _preferredSampleRate error:&error] ||
        ![session setActive:YES error:&error]) { [self report:error.localizedDescription]; return; }
    _starting = YES; const NSUInteger generation = ++_generation;
    __weak RavetMobileAudio *weak = self;
    [AVAudioUnit instantiateWithComponentDescription:_description options:0 completionHandler:^(AVAudioUnit *node, NSError *failure) {
        dispatch_async(dispatch_get_main_queue(), ^{
            RavetMobileAudio *owner = weak; if (!owner || generation != owner->_generation) return;
            owner->_starting = NO;
            if (failure || !node) { [owner report:failure.localizedDescription ?: @"AudioUnit creation failed"]; return; }
            owner->_engine = [[AVAudioEngine alloc] init]; owner->_node = node;
            if (owner->_savedState) node.AUAudioUnit.fullState = owner->_savedState;
            [owner->_engine attachNode:node];
            AVAudioInputNode *input = owner->_engine.inputNode;
            AVAudioFormat *format = [input outputFormatForBus:0];
            NSError *configurationError = nil;
            if (!format.channelCount || format.channelCount > 2 ||
                ![node.AUAudioUnit.inputBusses[0] setFormat:format error:&configurationError] ||
                ![node.AUAudioUnit.outputBusses[0] setFormat:format error:&configurationError]) {
                [owner suspend]; [owner report:configurationError.localizedDescription ?: @"Use a mono/stereo input route"]; return;
            }
            [owner->_engine connect:input to:node format:format];
            [owner->_engine connect:node to:owner->_engine.mainMixerNode format:format];
            [owner->_engine prepare];
            NSError *startError = nil;
            if (![owner->_engine startAndReturnError:&startError]) { [owner suspend]; [owner report:startError.localizedDescription]; return; }
            if (owner->_selectedMidiID) for (ItemCount i = 0; i < MIDIGetNumberOfSources(); ++i) {
                SInt32 identity = 0; MIDIObjectGetIntegerProperty(MIDIGetSource(i), kMIDIPropertyUniqueID, &identity);
                if (identity == owner->_selectedMidiID) { [owner selectMIDIInput:NSInteger(i)]; break; }
            }
            [owner report:[NSString stringWithFormat:@"Running · %.0f Hz · %.0f frames · %u input channels", session.sampleRate,
                session.IOBufferDuration * session.sampleRate, format.channelCount]];
        });
    }];
}
- (NSArray<NSString *> *)midiInputs {
    NSMutableArray *names = [NSMutableArray array];
    for (ItemCount i = 0; i < MIDIGetNumberOfSources(); ++i) {
        CFStringRef name = nullptr;
        MIDIObjectGetStringProperty(MIDIGetSource(i), kMIDIPropertyDisplayName, &name);
        [names addObject:name ? CFBridgingRelease(name) : @"MIDI input"];
    }
    return names;
}
- (void)disconnectMIDI {
    if (_midiPort) { MIDIPortDispose(_midiPort); _midiPort = 0; }
    _midiSource = 0; _midiSchedule = nil;
}
- (BOOL)selectMIDIInput:(NSInteger)index {
    [self disconnectMIDI];
    if (index < 0) { _selectedMidiID = 0; return YES; }
    if (!_node || NSUInteger(index) >= MIDIGetNumberOfSources()) return NO;
    if (!_midiClient && MIDIClientCreateWithBlock(CFSTR("RAVET mobile"), &_midiClient, ^(const MIDINotification *) {}) != noErr) return NO;
    _midiSchedule = _node.AUAudioUnit.scheduleMIDIEventBlock;
    if (!_midiSchedule) return NO;
    // Capture the block, not a mutable Objective-C owner, in the MIDI callback.
    // Port disposal stops delivery before this captured render schedule retires.
    AUScheduleMIDIEventBlock schedule = _midiSchedule;
#pragma clang diagnostic push
#pragma clang diagnostic ignored "-Wdeprecated-declarations"
    OSStatus result = MIDIInputPortCreateWithBlock(_midiClient, CFSTR("RAVET input"), &_midiPort,
        ^(const MIDIPacketList *packets, void *) {
            const MIDIPacket *packet = &packets->packet[0];
            for (UInt32 i = 0; i < packets->numPackets && i < 1024; ++i) {
                // AU's legacy scheduler accepts complete MIDI byte streams;
                // the adapter itself rejects unsupported SysEx/cables.
                schedule(AUEventSampleTimeImmediate, 0, packet->length, packet->data);
                packet = MIDIPacketNext(packet);
            }
        });
#pragma clang diagnostic pop
    if (result) return NO;
    _midiSource = MIDIGetSource(index);
    if (MIDIPortConnectSource(_midiPort, _midiSource, nullptr) != noErr) { [self selectMIDIInput:-1]; return NO; }
    MIDIObjectGetIntegerProperty(_midiSource, kMIDIPropertyUniqueID, &_selectedMidiID);
    return YES;
}
@end
