// macOS helpers for ravet_aax_check --editor: a real NSView parent in an
// off-screen window, and a short run-loop pump so the web view can load.
#import <Cocoa/Cocoa.h>

extern "C" void *ravet_check_make_view(double width, double height) {
    [NSApplication sharedApplication];
    NSWindow *window = [[NSWindow alloc] initWithContentRect:NSMakeRect(-10000, -10000, width, height)
                                                   styleMask:NSWindowStyleMaskBorderless
                                                     backing:NSBackingStoreBuffered
                                                       defer:NO];
    NSView *view = [[NSView alloc] initWithFrame:NSMakeRect(0, 0, width, height)];
    [window setContentView:view];
    [window orderFront:nil];
    return (__bridge_retained void *)view;
}

extern "C" unsigned ravet_check_subviews(void *view) {
    return unsigned([[(__bridge NSView *)view subviews] count]);
}

extern "C" void ravet_check_pump(double seconds) {
    [[NSRunLoop currentRunLoop] runUntilDate:[NSDate dateWithTimeIntervalSinceNow:seconds]];
}
