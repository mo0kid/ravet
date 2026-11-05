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
// Keyboard focus while a page text field is focused (see beginTyping).
@property(nonatomic) BOOL typing;
@property(nonatomic) BOOL madeKey;
@property(nonatomic, weak) NSWindow* previousKey;
@property(nonatomic, strong) id keyMonitor;
@property(nonatomic, strong) id mouseMonitor;
@property(nonatomic, strong) id resignObserver;
- (void)poll:(NSTimer*)timer;
- (void)endTyping:(BOOL)blur;
@end

// Reports to native code whether the focused element accepts typed text, so a
// host that keeps key status for its own shortcuts (Pro Tools) can be asked
// for it only while the page is editing text. Posts true/false on change only.
static NSString* const kKeyboardScript = @"(()=>{'use strict';"
    "const h=window.webkit&&window.webkit.messageHandlers&&window.webkit.messageHandlers.ravetKeyboard;if(!h)return;"
    "const plain=new Set(['button','checkbox','color','file','hidden','image','radio','range','reset','submit']);"
    "const editable=e=>!!e&&(e.isContentEditable||(!e.disabled&&!e.readOnly&&(e.tagName==='TEXTAREA'||"
    "(e.tagName==='INPUT'&&!plain.has((e.type||'').toLowerCase())))));"
    "let on=false,timer=0;"
    "const check=()=>{const now=editable(document.activeElement);if(now===on)return;on=now;"
    "if(on)timer=setInterval(check,100);else{clearInterval(timer);timer=0;}h.postMessage(on);};"
    "document.addEventListener('focusin',check,true);"
    "document.addEventListener('focusout',()=>setTimeout(check,0),true);"
    "})();";

@implementation RavetWebBridge
- (void)userContentController:(WKUserContentController*)controller didReceiveScriptMessage:(WKScriptMessage*)message {
    // The message callback may destroy this editor; keep the bridge alive until return.
    RavetWebBridge* owner __attribute__((objc_precise_lifetime)) = self;
    (void)owner;
    if ([message.name isEqualToString:@"ravetKeyboard"]) {
        if (![message.body isKindOfClass:NSNumber.class]) return;
        if ([message.body boolValue]) [self beginTyping];
        else [self endTyping:NO];
        return;
    }
    if (!_message || ![message.body isKindOfClass:[NSDictionary class]]) return;
    // Page bodies may hold values JSON cannot encode (a Date, NaN), for which
    // dataWithJSONObject raises inside WebKit's delivery; drop those instead.
    if (![NSJSONSerialization isValidJSONObject:message.body]) return;
    NSData* json = [NSJSONSerialization dataWithJSONObject:message.body options:0 error:nil];
    if (!json || json.length > 65536) return;
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
// A text field has focus. Make the editor's window key and the web view first
// responder; hosts such as Logic and Live already have, so nothing changes
// there. If the host refuses key status, forward plain typing (no Command key)
// from the application's key events to the web view until the field loses
// focus, a mouse button goes down in another window, or the application
// resigns active; the last two blur the field. The AAX SDK has no keyboard
// focus call, so this is done in AppKit for every format.
- (void)beginTyping {
    WKWebView* web = _web;
    NSWindow* window = web.window;
    if (_typing || !web || !window) return;
    _typing = YES;
    if (!window.isKeyWindow) {
        NSWindow* previous = NSApp.keyWindow;
        if (window.canBecomeKeyWindow) [window makeKeyWindow];
        if (window.isKeyWindow) { _madeKey = YES; _previousKey = previous; }
    }
    NSResponder* responder = window.firstResponder;
    if (![responder isKindOfClass:NSView.class] || ![(NSView*)responder isDescendantOf:web])
        [window makeFirstResponder:web];
    if (window.isKeyWindow) return;
    __weak RavetWebBridge* weak = self;
    _keyMonitor = [NSEvent addLocalMonitorForEventsMatchingMask:NSEventMaskKeyDown | NSEventMaskKeyUp
                                                         handler:^NSEvent*(NSEvent* event) {
        WKWebView* view = weak.web;
        if (!view || !view.window || view.window.isKeyWindow || event.window == view.window ||
            (event.modifierFlags & NSEventModifierFlagCommand)) return event;
        if (event.type == NSEventTypeKeyDown) [view keyDown:event];
        else [view keyUp:event];
        return nil;
    }];
    _mouseMonitor = [NSEvent addLocalMonitorForEventsMatchingMask:NSEventMaskLeftMouseDown |
                         NSEventMaskRightMouseDown | NSEventMaskOtherMouseDown
                                                           handler:^NSEvent*(NSEvent* event) {
        RavetWebBridge* bridge = weak;
        if (bridge && event.window != bridge.web.window) [bridge endTyping:YES];
        return event;
    }];
    _resignObserver = [NSNotificationCenter.defaultCenter addObserverForName:NSApplicationDidResignActiveNotification
        object:nil queue:nil usingBlock:^(NSNotification*) { [weak endTyping:YES]; }];
}
// The field lost focus (or, with blur, must lose it). Give key status back to
// the window that had it, but only if this editor took it and still has it.
- (void)endTyping:(BOOL)blur {
    if (!_typing) return;
    _typing = NO;
    if (_keyMonitor) [NSEvent removeMonitor:_keyMonitor];
    if (_mouseMonitor) [NSEvent removeMonitor:_mouseMonitor];
    if (_resignObserver) [NSNotificationCenter.defaultCenter removeObserver:_resignObserver];
    _keyMonitor = _mouseMonitor = _resignObserver = nil;
    WKWebView* web = _web;
    NSWindow* window = web.window;
    NSWindow* previous = _previousKey;
    if (_madeKey && window.isKeyWindow && previous && previous != window && previous.isVisible &&
        previous.canBecomeKeyWindow) [previous makeKeyWindow];
    _madeKey = NO; _previousKey = nil;
    if (blur && web) [web evaluateJavaScript:@"document.activeElement&&document.activeElement.blur()" completionHandler:nil];
}
- (void)webView:(WKWebView*)web didFinishNavigation:(WKNavigation*)navigation { _delivered = nil; }
- (void)webView:(WKWebView*)web didStartProvisionalNavigation:(WKNavigation*)navigation { [self endTyping:NO]; }
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
    [configuration.userContentController addUserScript:[[WKUserScript alloc]
        initWithSource:kKeyboardScript injectionTime:WKUserScriptInjectionTimeAtDocumentStart forMainFrameOnly:YES]];
    [configuration.userContentController addScriptMessageHandler:bridge name:@"ravet"];
    [configuration.userContentController addScriptMessageHandler:bridge name:@"ravetKeyboard"];
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
extern "C" void* ravet_web_create(const char* htmlPath, double width, double height, void* context, RavetWebMessage message, RavetWebSnapshot snapshot) noexcept {
    if (!NSThread.isMainThread || !htmlPath) return nullptr;
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
extern "C" void* ravet_web_create_html(const char* html, double width, double height, void* context, RavetWebMessage message, RavetWebSnapshotBuffer snapshot) noexcept {
    if (!NSThread.isMainThread || !html || strnlen(html,1048577)>1048576) return nullptr;
    NSString* page=[NSString stringWithUTF8String:html];
    if (!page) return nullptr;
    RavetWebContainer* container=createContainer(width,height,context,message,nullptr,snapshot);
    if (!container) return nullptr;
    [container.web loadHTMLString:page baseURL:nil];
    return (__bridge_retained void*)container;
}
extern "C" bool ravet_web_set_size(void* view, double width, double height) {
    if (!NSThread.isMainThread || !view || !std::isfinite(width) || !std::isfinite(height) || width <= 0 || height <= 0 || width > 16384 || height > 16384) return false;
    [(__bridge NSView*)view setFrameSize:NSMakeSize(width,height)];
    return true;
}
extern "C" void ravet_web_set_visible(void* view,bool visible) {
    if (NSThread.isMainThread && view) ((__bridge NSView*)view).hidden=!visible;
}
extern "C" bool ravet_web_set_background(void* view, double red, double green, double blue) {
    if (!NSThread.isMainThread || !view || !std::isfinite(red) || !std::isfinite(green) || !std::isfinite(blue)) return false;
    RavetWebContainer* container = (__bridge RavetWebContainer*)view;
    container.background = [NSColor colorWithSRGBRed:std::clamp(red,0.0,1.0) green:std::clamp(green,0.0,1.0) blue:std::clamp(blue,0.0,1.0) alpha:1];
    container.layer.backgroundColor = container.background.CGColor;
    if (@available(macOS 12.0, *)) container.web.underPageBackgroundColor = container.background;
    container.needsDisplay = YES;
    return true;
}
extern "C" void ravet_web_attach(void* view, void* parent) {
    if (NSThread.isMainThread && view && parent) [(__bridge NSView*)parent addSubview:(__bridge NSView*)view];
}
extern "C" void ravet_web_detach(void* view) {
    if (!NSThread.isMainThread || !view) return;
    [((__bridge RavetWebContainer*)view).bridge endTyping:NO];
    [(__bridge NSView*)view removeFromSuperview];
}
extern "C" void ravet_web_destroy(void* view) {
    if (!NSThread.isMainThread || !view) return;
    RavetWebContainer* container = (__bridge_transfer RavetWebContainer*)view;
    [container.accessibilityOwner retire]; container.accessibilityOwner=nil;
    [container.bridge endTyping:NO];
    [container.bridge.timer invalidate]; container.bridge.timer = nil;
    container.bridge.message = nullptr; container.bridge.snapshot = nullptr; container.bridge.bufferSnapshot = nullptr; container.bridge.context = nullptr;
    [container.web.configuration.userContentController removeScriptMessageHandlerForName:@"ravet"];
    [container.web.configuration.userContentController removeScriptMessageHandlerForName:@"ravetKeyboard"];
    container.web.navigationDelegate = nil; [container.web stopLoading]; container.bridge.web = nil;
    [container removeFromSuperview]; container.web = nil; container.bridge = nil;
}
