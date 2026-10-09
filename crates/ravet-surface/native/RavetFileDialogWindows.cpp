#include "RavetFileDialog.h"
#include <windows.h>
#include <commdlg.h>
namespace ravet::surface {
bool openFile(const FileDialogOptions& options, FileDialogCompletion callback) {
    if (!callback) return false;
    auto wide = [](const std::string& text) {
        const int n = MultiByteToWideChar(CP_UTF8, MB_ERR_INVALID_CHARS, text.data(), int(text.size()), nullptr, 0);
        std::wstring result(n, 0);
        if (n) MultiByteToWideChar(CP_UTF8, MB_ERR_INVALID_CHARS, text.data(), int(text.size()), result.data(), n);
        return result;
    };
    auto title = wide(options.title);
    auto pattern = options.extension.empty() ? std::wstring(L"*.*") : L"*." + wide(options.extension);
    auto filter = pattern + L'\0' + pattern + L'\0' + L'\0';
    wchar_t path[32768]{};
    OPENFILENAMEW dialog{};
    dialog.lStructSize = sizeof(dialog);
    dialog.hwndOwner = static_cast<HWND>(options.parent);
    dialog.lpstrTitle = title.c_str();
    dialog.lpstrFilter = filter.c_str();
    dialog.lpstrFile = path;
    dialog.nMaxFile = 32768;
    dialog.Flags = OFN_FILEMUSTEXIST | OFN_NOCHANGEDIR | OFN_EXPLORER;
    callback(GetOpenFileNameW(&dialog) ? std::filesystem::path(path) : std::filesystem::path());
    return true;
}
}
