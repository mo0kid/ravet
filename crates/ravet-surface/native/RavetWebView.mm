#import <Cocoa/Cocoa.h>
#import <WebKit/WebKit.h>
#include "RavetWebView.h"
#include "RavetObjcNames.h"
#include <cmath>
#include <cstring>
#include <algorithm>
#include <array>

@interface RavetWebBridge : NSObject <WKScriptMessageHandler, WKNavigationDelegate>
@property(nonatomic, weak) WKWebView* web;
@property(nonatomic, strong) NSURL* root;
@property(nonatomic) void* context;
@property(nonatomic) RavetWebMessage message;
@property(nonatomic) RavetWebSnapshot snapshot;
@property(nonatomic) RavetWebSnapshotBuffer bufferSnapshot;
@property(nonatomic, strong) NSTimer* timer;
@property(nonatomic, copy) NSString* delivered;
@property(nonatomic) BOOL evaluating;
- (void)poll:(NSTimer*)timer;
@end

@implementation RavetWebBridge
- (void)userContentController:(WKUserContentController*)controller didReceiveScriptMessage:(WKScriptMessage*)message {
    if (!_message || ![message.body isKindOfClass:[NSDictionary class]]) return;
    NSData* json = [NSJSONSerialization dataWithJSONObject:message.body options:0 error:nil];
    if (json.length > 65536) return;
    NSString* text = [[NSString alloc] initWithData:json encoding:NSUTF8StringEncoding];
    if (text) _message(_context, text.UTF8String);
}
- (void)poll:(NSTimer*)timer {
    // Snapshot consumers may destroy their own view during the callback.
    RavetWebBridge* owner __attribute__((objc_precise_lifetime)) = self;
    (void)owner;
    if ((!_snapshot && !_bufferSnapshot) || !_web || _evaluating) return;
    std::array<char,65537> buffer;
    const char* data = nullptr;
    if (_bufferSnapshot) {
        const size_t length = _bufferSnapshot(_context,buffer.data(),buffer.size());
        if (!length || length >= buffer.size()) return;
        buffer[length]=0; data=buffer.data();
    } else { data = _snapshot(_context); }
    // A callback may destroy its editor. Stop before using the released context/view.
    if (!data || !_web) return;
    if (strnlen(data, 65537) > 65536) return;
    NSString* json = [NSString stringWithUTF8String:data];
    if (!json) return;
    if ([_delivered isEqualToString:json]) return;
    id decoded = [NSJSONSerialization JSONObjectWithData:[json dataUsingEncoding:NSUTF8StringEncoding] options:0 error:nil];
    if (![decoded isKindOfClass:NSDictionary.class] && ![decoded isKindOfClass:NSArray.class]) return;
    NSString* script = [NSString stringWithFormat:@"typeof window.ravetUpdate==='function'&&(window.ravetUpdate(%@),true)", json];
    _evaluating = YES;
    __weak RavetWebBridge* weak = self;
    [_web evaluateJavaScript:script completionHandler:^(id result, NSError* error) {
        RavetWebBridge* bridge = weak;
        if (!bridge) return;
        bridge.evaluating = NO;
        if (!error && [result isKindOfClass:NSNumber.class] && [result boolValue]) bridge.delivered = json;
    }];
}
- (void)webView:(WKWebView*)web didFinishNavigation:(WKNavigation*)navigation { _delivered = nil; }
- (void)webView:(WKWebView*)web decidePolicyForNavigationAction:(WKNavigationAction*)action decisionHandler:(void (^)(WKNavigationActionPolicy))handler {
    NSURL* url = action.request.URL;
    BOOL local = _root && url.isFileURL && [url.URLByResolvingSymlinksInPath.path.stringByStandardizingPath hasPrefix:[_root.path stringByAppendingString:@"/"]];
    handler(local || [url.absoluteString isEqualToString:@"about:blank"] ? WKNavigationActionPolicyAllow : WKNavigationActionPolicyCancel);
}
@end

@interface RavetEditorWebView : WKWebView
@end
@implementation RavetEditorWebView
- (BOOL)acceptsFirstMouse:(NSEvent*)event { return YES; }
@end

@class RavetAccessibilityOwner;
@interface RavetWebContainer : NSView
@property(nonatomic, strong) WKWebView* web;
@property(nonatomic, strong) RavetWebBridge* bridge;
@property(nonatomic, strong) NSColor* background;
@property(nonatomic,strong) RavetAccessibilityOwner* accessibilityOwner;
@property(nonatomic) uint64_t accessibilityRevision;
@property(nonatomic) BOOL accessibilityTransition;
@end
#include "RavetAccessibility.inc"
@implementation RavetWebContainer
- (BOOL)isAccessibilityElement { if(!NSThread.isMainThread)return NO; return self.accessibilityOwner ? YES : [super isAccessibilityElement]; }
- (NSAccessibilityRole)accessibilityRole { if(!NSThread.isMainThread)return NSAccessibilityUnknownRole; return self.accessibilityOwner ? NSAccessibilityGroupRole : [super accessibilityRole]; }
- (NSString*)accessibilityLabel { if(!NSThread.isMainThread)return nil; return self.accessibilityOwner ? self.accessibilityOwner.label : [super accessibilityLabel]; }
- (NSArray*)accessibilityChildren { if(!NSThread.isMainThread)return @[]; return self.accessibilityOwner ? [self.accessibilityOwner children] : [super accessibilityChildren]; }
- (NSArray*)accessibilityChildrenInNavigationOrder { if(!NSThread.isMainThread)return @[]; return self.accessibilityOwner ? [self.accessibilityOwner children] : [super accessibilityChildrenInNavigationOrder]; }
- (id)accessibilityFocusedUIElement { if(!NSThread.isMainThread)return nil; return self.accessibilityOwner ? [self.accessibilityOwner focusedElement] : [super accessibilityFocusedUIElement]; }
- (id)accessibilityHitTest:(NSPoint)point {
    if(!NSThread.isMainThread)return nil;
    if(!self.accessibilityOwner)return [super accessibilityHitTest:point];
    NSArray* children=[self.accessibilityOwner children];
    for(RavetAccessibilityElement* element in children.reverseObjectEnumerator)
        if(NSPointInRect(point,element.accessibilityFrame))return element;
    return [self.accessibilityOwner visible]&&NSPointInRect(point,NSAccessibilityFrameInView(self,self.bounds))?self:nil;
}
- (BOOL)isFlipped { return YES; }
- (BOOL)isOpaque { return YES; }
- (void)drawRect:(NSRect)dirtyRect { [_background setFill]; NSRectFill(dirtyRect); }
@end

static RavetWebContainer* createContainer(double width, double height, void* context, RavetWebMessage message, RavetWebSnapshot snapshot, RavetWebSnapshotBuffer bufferSnapshot) {
    NSCAssert(NSThread.isMainThread, @"RAVET editors require the main thread");
    if (!std::isfinite(width) || !std::isfinite(height) || width <= 0 || height <= 0) return nil;
    RavetWebContainer* container = [[RavetWebContainer alloc] initWithFrame:NSMakeRect(0,0,width,height)];
    container.background = NSColor.whiteColor;
    container.wantsLayer = YES;
    container.layer.backgroundColor = container.background.CGColor;
    RavetWebBridge* bridge = [RavetWebBridge new];
    bridge.context = context; bridge.message = message; bridge.snapshot = snapshot; bridge.bufferSnapshot = bufferSnapshot;
    WKWebViewConfiguration* configuration = [WKWebViewConfiguration new];
    // Cancel only the browser menu; controls still receive right-click events.
    [configuration.userContentController addUserScript:[[WKUserScript alloc]
        initWithSource:@"window.addEventListener('contextmenu',event=>event.preventDefault(),{capture:true,passive:false});"
        injectionTime:WKUserScriptInjectionTimeAtDocumentStart forMainFrameOnly:NO]];
    [configuration.userContentController addScriptMessageHandler:bridge name:@"ravet"];
    WKWebView* web = [[RavetEditorWebView alloc] initWithFrame:container.bounds configuration:configuration];
    web.wantsLayer = YES;
    web.autoresizingMask = NSViewWidthSizable | NSViewHeightSizable;
    web.navigationDelegate = bridge; bridge.web = web;
    container.web = web; container.bridge = bridge; [container addSubview:web];
    bridge.timer = [NSTimer timerWithTimeInterval:0.05 target:bridge selector:@selector(poll:) userInfo:nil repeats:YES];
    [[NSRunLoop mainRunLoop] addTimer:bridge.timer forMode:NSRunLoopCommonModes];
    return container;
}
extern "C" bool ravet_web_is_main_thread() { return NSThread.isMainThread; }
extern "C" void ravet_web_pump() {}
extern "C" void* ravet_web_create(const char* htmlPath, double width, double height, void* context, RavetWebMessage message, RavetWebSnapshot snapshot) {
    NSCAssert(NSThread.isMainThread, @"main thread required");
    if (!htmlPath) return nullptr;
    NSString* path=[NSString stringWithUTF8String:htmlPath];
    if (!path) return nullptr;
    NSURL* url=[[NSURL fileURLWithPath:path] URLByResolvingSymlinksInPath];
    if (![[NSFileManager defaultManager] fileExistsAtPath:url.path]) return nullptr;
    RavetWebContainer* container=createContainer(width,height,context,message,snapshot,nullptr);
    if (!container) return nullptr;
    container.bridge.root=[url URLByDeletingLastPathComponent];
    [container.web loadFileURL:url allowingReadAccessToURL:container.bridge.root];
    return (__bridge_retained void*)container;
}
extern "C" void* ravet_web_create_html(const char* html, double width, double height, void* context, RavetWebMessage message, RavetWebSnapshotBuffer snapshot) {
    NSCAssert(NSThread.isMainThread, @"main thread required");
    if (!html || strnlen(html,1048577)>1048576) return nullptr;
    NSString* page=[NSString stringWithUTF8String:html];
    if (!page) return nullptr;
    RavetWebContainer* container=createContainer(width,height,context,message,nullptr,snapshot);
    if (!container) return nullptr;
    [container.web loadHTMLString:page baseURL:nil];
    return (__bridge_retained void*)container;
}
extern "C" bool ravet_web_set_size(void* view, double width, double height) {
    NSCAssert(NSThread.isMainThread, @"main thread required");
    if (!view || !std::isfinite(width) || !std::isfinite(height) || width <= 0 || height <= 0 || width > 16384 || height > 16384) return false;
    [(__bridge NSView*)view setFrameSize:NSMakeSize(width,height)];
    return true;
}
extern "C" void ravet_web_set_visible(void* view,bool visible) {
    NSCAssert(NSThread.isMainThread, @"main thread required");
    if (view) ((__bridge NSView*)view).hidden=!visible;
}
extern "C" bool ravet_web_set_background(void* view, double red, double green, double blue) {
    NSCAssert(NSThread.isMainThread, @"main thread required");
    if (!view || !std::isfinite(red) || !std::isfinite(green) || !std::isfinite(blue)) return false;
    RavetWebContainer* container = (__bridge RavetWebContainer*)view;
    container.background = [NSColor colorWithSRGBRed:std::clamp(red,0.0,1.0) green:std::clamp(green,0.0,1.0) blue:std::clamp(blue,0.0,1.0) alpha:1];
    container.layer.backgroundColor = container.background.CGColor;
    if (@available(macOS 12.0, *)) container.web.underPageBackgroundColor = container.background;
    container.needsDisplay = YES;
    return true;
}
extern "C" void ravet_web_attach(void* view, void* parent) {
    NSCAssert(NSThread.isMainThread, @"main thread required");
    if (view && parent) [(__bridge NSView*)parent addSubview:(__bridge NSView*)view];
}
extern "C" void ravet_web_detach(void* view) {
    NSCAssert(NSThread.isMainThread, @"main thread required");
    if (view) [(__bridge NSView*)view removeFromSuperview];
}
extern "C" void ravet_web_destroy(void* view) {
    NSCAssert(NSThread.isMainThread, @"main thread required");
    if (!view) return;
    RavetWebContainer* container = (__bridge_transfer RavetWebContainer*)view;
    [container.accessibilityOwner retire]; container.accessibilityOwner=nil;
    [container.bridge.timer invalidate]; container.bridge.timer = nil;
    container.bridge.message = nullptr; container.bridge.snapshot = nullptr; container.bridge.bufferSnapshot = nullptr; container.bridge.context = nullptr;
    [container.web.configuration.userContentController removeScriptMessageHandlerForName:@"ravet"];
    container.web.navigationDelegate = nil; [container.web stopLoading]; container.bridge.web = nil;
    [container removeFromSuperview]; container.web = nil; container.bridge = nil;
}
