#import <Cocoa/Cocoa.h>
#include <cstdlib>
#include <cstring>

// Main-thread modal loop keeps Cocoa/WebKit alive; no subprocess or shell.
extern "C" char* ravet_foundry_choose_directory(const char* title, const char* initial, bool probe) {
    if (![NSThread isMainThread]) return nullptr;
    NSOpenPanel* panel = [NSOpenPanel openPanel];
    panel.title = [NSString stringWithUTF8String:title] ?: @"Choose directory";
    panel.canChooseDirectories = YES;
    panel.canChooseFiles = NO;
    panel.allowsMultipleSelection = NO;
    panel.canCreateDirectories = NO;
    if (initial && *initial) panel.directoryURL = [NSURL fileURLWithPath:[NSString stringWithUTF8String:initial] isDirectory:YES];
    if (probe) dispatch_after(dispatch_time(DISPATCH_TIME_NOW, 100 * NSEC_PER_MSEC), dispatch_get_main_queue(), ^{[panel cancel:nil];});
    if ([panel runModal] != NSModalResponseOK || !panel.URL.path.UTF8String) return nullptr;
    return strdup(panel.URL.path.UTF8String);
}
extern "C" void ravet_foundry_free_directory(char* path) { free(path); }
