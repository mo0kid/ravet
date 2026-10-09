#pragma once
#include <filesystem>
#include <functional>
#include <string>
namespace ravet::surface {
struct FileDialogOptions {
    std::string title;
    std::string extension; // Single extension without dot; empty allows all files.
    void* parent = nullptr; // NSWindow / HWND / GtkWindow, or null.
};
using FileDialogCompletion = std::function<void(std::filesystem::path)>;
// UI thread only, after native editor/toolkit initialization. Owns callback on
// success; invokes once with empty path for cancellation. Cocoa/GTK asynchronous;
// Windows runs the native modal loop and completes before returning. The caller
// must retain callback state until completion; no raw editor pointer is retained.
// Callback must not throw. Invalid filter/initialization can reject with false.
bool openFile(const FileDialogOptions&, FileDialogCompletion);
}
