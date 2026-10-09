#include "RavetFileDialog.h"
#import <Cocoa/Cocoa.h>
#include <iostream>
int main() {
    @autoreleasepool {
        [NSApplication sharedApplication];
        auto parent = [[NSWindow alloc] initWithContentRect:NSMakeRect(0,0,200,100)
            styleMask:NSWindowStyleMaskTitled backing:NSBackingStoreBuffered defer:NO];
        int completions=0; bool cancelled=false;
        if(!ravet::surface::openFile({"RAVET test","key",(__bridge void*)parent},[&](std::filesystem::path path) {
            ++completions; cancelled=path.empty();
        })) return 1;
        auto deadline=[NSDate dateWithTimeIntervalSinceNow:3];
        while(!parent.attachedSheet && deadline.timeIntervalSinceNow>0)
            [[NSRunLoop currentRunLoop] runMode:NSDefaultRunLoopMode beforeDate:[NSDate dateWithTimeIntervalSinceNow:.01]];
        if(!parent.attachedSheet) return 2;
        [parent endSheet:parent.attachedSheet returnCode:NSModalResponseCancel];
        while(!completions && deadline.timeIntervalSinceNow>0)
            [[NSRunLoop currentRunLoop] runMode:NSDefaultRunLoopMode beforeDate:[NSDate dateWithTimeIntervalSinceNow:.01]];
        if(completions!=1 || !cancelled) return 3;
        std::cout<<"Owned Cocoa file dialog cancel and callback lifetime passed\n";
        return 0;
    }
}
