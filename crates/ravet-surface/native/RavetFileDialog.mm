#include "RavetFileDialog.h"
#import <Cocoa/Cocoa.h>
#import <UniformTypeIdentifiers/UniformTypeIdentifiers.h>
namespace ravet::surface {
bool openFile(const FileDialogOptions& options, FileDialogCompletion callback) {
    if (![NSThread isMainThread] || !callback) return false;
    NSOpenPanel* panel = [NSOpenPanel openPanel];
    panel.title = [NSString stringWithUTF8String:options.title.c_str()] ?: @"Open";
    panel.allowsMultipleSelection = NO;
    panel.canChooseDirectories = NO;
    if (!options.extension.empty()) {
        auto* extension=[NSString stringWithUTF8String:options.extension.c_str()];
        UTType* type=extension?[UTType typeWithFilenameExtension:extension]:nil;
        if(!type) return false;
        panel.allowedContentTypes = @[type];
    }
    auto completion = [callback = std::move(callback), panel](NSModalResponse response) {
        std::filesystem::path path;
        if (response == NSModalResponseOK && panel.URL.path.UTF8String) path = std::filesystem::u8path(panel.URL.path.UTF8String);
        callback(std::move(path));
    };
    if (options.parent) [panel beginSheetModalForWindow:(__bridge NSWindow*)options.parent completionHandler:^(NSModalResponse response) { completion(response); }];
    else [panel beginWithCompletionHandler:^(NSModalResponse response) { completion(response); }];
    return true;
}
}
