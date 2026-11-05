// Win32 standalone window for ravet-webview: the same C ABI as StandaloneWindow.mm.
// Sizes crossing this ABI are logical points (96 DPI); the window is per-monitor
// DPI aware and converts at its current DPI. Audio/MIDI settings persistence is
// handled in Rust (see standalone.rs); this file only provides the menu action.
#include "../../ravet-surface/native/RavetWebView.h"
#include <windows.h>
#include <algorithm>
#include <cmath>
#include <memory>
#include <string>

namespace {
constexpr UINT SettingsCommand = 1;
constexpr wchar_t ClassName[] = L"RavetStandaloneWindow";
struct Window {
    HWND hwnd = nullptr;
    void* view = nullptr;
    bool closed = false, hidden = false, settings = false;
    double aspectWidth = 0, aspectHeight = 0;
    double minimumWidth = 0, minimumHeight = 0, maximumWidth = 0, maximumHeight = 0;
    double originalMinimumWidth = 0, originalMinimumHeight = 0;
};
std::wstring wide(const char* text) {
    int n = MultiByteToWideChar(CP_UTF8, 0, text, -1, nullptr, 0);
    std::wstring out(n > 0 ? n - 1 : 0, 0);
    if (n > 1) MultiByteToWideChar(CP_UTF8, 0, text, -1, out.data(), n);
    return out;
}
double scaleOf(HWND hwnd) {
    using GetDpi = UINT(WINAPI*)(HWND);
    static auto get = reinterpret_cast<GetDpi>(GetProcAddress(GetModuleHandleW(L"user32.dll"), "GetDpiForWindow"));
    UINT dpi = get && hwnd ? get(hwnd) : 96;
    return (dpi ? dpi : 96) / 96.0;
}
DWORD style() { return WS_OVERLAPPEDWINDOW | WS_CLIPCHILDREN; }
// Outer window size for a client size in physical pixels.
SIZE frameFor(const Window& w, int width, int height) {
    RECT rect{0, 0, width, height};
    AdjustWindowRectEx(&rect, style(), w.settings, 0);
    return {rect.right - rect.left, rect.bottom - rect.top};
}
void fitView(Window& w) {
    if (!w.view) return;
    RECT client;
    GetClientRect(w.hwnd, &client);
    if (client.right > 0 && client.bottom > 0) ravet_web_set_size(w.view, client.right, client.bottom);
}
void openSettings(Window& w) {
    if (w.view && !w.closed)
        ravet_web_eval(w.view, "typeof window.ravetOpenSettings==='function'&&window.ravetOpenSettings()", nullptr, nullptr);
}
LRESULT CALLBACK windowProc(HWND hwnd, UINT message, WPARAM wp, LPARAM lp) {
    auto* w = reinterpret_cast<Window*>(GetWindowLongPtrW(hwnd, GWLP_USERDATA));
    if (!w) return DefWindowProcW(hwnd, message, wp, lp);
    switch (message) {
    case WM_SIZE: fitView(*w); return 0;
    case WM_COMMAND:
        if (LOWORD(wp) == SettingsCommand) { openSettings(*w); return 0; }
        break;
    case WM_CLOSE:
        // Close admission is terminal; destruction belongs to ravet_standalone_destroy.
        w->closed = true;
        ShowWindow(hwnd, SW_HIDE);
        return 0;
    case WM_DPICHANGED: {
        auto* rect = reinterpret_cast<RECT*>(lp);
        SetWindowPos(hwnd, nullptr, rect->left, rect->top, rect->right - rect->left, rect->bottom - rect->top, SWP_NOZORDER | SWP_NOACTIVATE);
        return 0;
    }
    case WM_GETMINMAXINFO: {
        auto* info = reinterpret_cast<MINMAXINFO*>(lp);
        double scale = scaleOf(hwnd);
        if (w->minimumWidth > 0) {
            SIZE minimum = frameFor(*w, int(std::lround(w->minimumWidth * scale)), int(std::lround(w->minimumHeight * scale)));
            info->ptMinTrackSize = {minimum.cx, minimum.cy};
        }
        if (w->maximumWidth > 0) {
            SIZE maximum = frameFor(*w, int(std::lround(w->maximumWidth * scale)), int(std::lround(w->maximumHeight * scale)));
            info->ptMaxTrackSize = {maximum.cx, maximum.cy};
        }
        return 0;
    }
    case WM_SIZING:
        if (w->aspectWidth > 0) {
            auto* rect = reinterpret_cast<RECT*>(lp);
            SIZE frame = frameFor(*w, 0, 0);
            double width = rect->right - rect->left - frame.cx, height = rect->bottom - rect->top - frame.cy;
            double ratio = w->aspectWidth / w->aspectHeight;
            bool horizontal = wp == WMSZ_LEFT || wp == WMSZ_RIGHT;
            if (horizontal) height = width / ratio; else width = height * ratio;
            int outerWidth = int(std::lround(width)) + frame.cx, outerHeight = int(std::lround(height)) + frame.cy;
            if (wp == WMSZ_LEFT || wp == WMSZ_TOPLEFT || wp == WMSZ_BOTTOMLEFT) rect->left = rect->right - outerWidth;
            else rect->right = rect->left + outerWidth;
            if (wp == WMSZ_TOP || wp == WMSZ_TOPLEFT || wp == WMSZ_TOPRIGHT) rect->top = rect->bottom - outerHeight;
            else rect->bottom = rect->top + outerHeight;
            return TRUE;
        }
        break;
    case WM_ERASEBKGND: return 1;
    }
    return DefWindowProcW(hwnd, message, wp, lp);
}
Window* live(void* handle) {
    auto* w = static_cast<Window*>(handle);
    return w && !w->closed ? w : nullptr;
}
}

extern "C" void* ravet_standalone_create(bool hidden, bool settings, const char* title, double width, double height, bool lockAspect) {
    if (!title || !ravet_web_is_main_thread()) return nullptr;
    using SetAwareness = BOOL(WINAPI*)(HANDLE);
    if (auto set = reinterpret_cast<SetAwareness>(GetProcAddress(GetModuleHandleW(L"user32.dll"), "SetProcessDpiAwarenessContext")))
        set(reinterpret_cast<HANDLE>(-4)); // DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2
    HINSTANCE instance = GetModuleHandleW(nullptr);
    WNDCLASSEXW cls{sizeof(cls)};
    cls.lpfnWndProc = windowProc;
    cls.hInstance = instance;
    cls.lpszClassName = ClassName;
    cls.hCursor = LoadCursorW(nullptr, MAKEINTRESOURCEW(32512));
    cls.hIcon = LoadIconW(instance, MAKEINTRESOURCEW(1));
    if (!cls.hIcon) cls.hIcon = LoadIconW(nullptr, MAKEINTRESOURCEW(32512));
    cls.hbrBackground = reinterpret_cast<HBRUSH>(GetStockObject(BLACK_BRUSH));
    RegisterClassExW(&cls);
    auto w = std::make_unique<Window>();
    w->hidden = hidden;
    w->settings = settings;
    if (lockAspect) {
        w->aspectWidth = width;
        w->aspectHeight = height;
        double minimum = std::max(640. / width, 540. / height);
        w->minimumWidth = width * minimum;
        w->minimumHeight = height * minimum;
    } else {
        w->minimumWidth = 640;
        w->minimumHeight = 540;
    }
    w->originalMinimumWidth = w->minimumWidth;
    w->originalMinimumHeight = w->minimumHeight;
    HMENU menu = nullptr;
    if (settings) {
        menu = CreateMenu();
        AppendMenuW(menu, MF_STRING, SettingsCommand, L"&Audio / MIDI…");
    }
    w->hwnd = CreateWindowExW(0, ClassName, wide(title).c_str(), style(), CW_USEDEFAULT, CW_USEDEFAULT, 100, 100,
                              nullptr, menu, instance, nullptr);
    if (!w->hwnd) return nullptr;
    SetWindowLongPtrW(w->hwnd, GWLP_USERDATA, reinterpret_cast<LONG_PTR>(w.get()));
    double scale = scaleOf(w->hwnd);
    SIZE frame = frameFor(*w, int(std::lround(width * scale)), int(std::lround(height * scale)));
    RECT work{};
    SystemParametersInfoW(SPI_GETWORKAREA, 0, &work, 0);
    int x = work.left + std::max(0L, (work.right - work.left - frame.cx) / 2);
    int y = work.top + std::max(0L, (work.bottom - work.top - frame.cy) / 2);
    SetWindowPos(w->hwnd, nullptr, x, y, frame.cx, frame.cy, SWP_NOZORDER | SWP_NOACTIVATE);
    if (!hidden) {
        ShowWindow(w->hwnd, SW_SHOW);
        SetForegroundWindow(w->hwnd);
    }
    return w.release();
}
extern "C" void ravet_standalone_attach_editor(void* handle, void* view) {
    auto* w = static_cast<Window*>(handle);
    if (!w) return;
    w->view = view;
    fitView(*w);
}
extern "C" void* ravet_standalone_view(void* handle) {
    auto* w = live(handle);
    return w ? w->hwnd : nullptr;
}
extern "C" bool ravet_standalone_pump(void* handle, double seconds) {
    auto* w = static_cast<Window*>(handle);
    ULONGLONG end = GetTickCount64() + ULONGLONG(std::max(0.0, seconds) * 1000.0);
    for (;;) {
        MSG msg;
        while (PeekMessageW(&msg, nullptr, 0, 0, PM_REMOVE)) {
            TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
        ULONGLONG now = GetTickCount64();
        if (now >= end) break;
        MsgWaitForMultipleObjectsEx(0, nullptr, DWORD(end - now), QS_ALLINPUT, MWMO_INPUTAVAILABLE);
    }
    return w && !w->closed && IsWindowVisible(w->hwnd);
}
extern "C" void ravet_standalone_fit_editor(void* handle) {
    if (auto* w = live(handle)) fitView(*w);
}
extern "C" bool ravet_standalone_eval(void* handle, const char* script) {
    auto* w = live(handle);
    if (!w || !w->view || !script) return false;
    struct Result { bool done = false, truthy = false; };
    // Shared with the completion so a late callback never touches this frame.
    auto* result = new std::shared_ptr<Result>(std::make_shared<Result>());
    auto keep = *result;
    if (!ravet_web_eval(w->view, script, result, [](void* context, bool truthy) {
            std::unique_ptr<std::shared_ptr<Result>> r(static_cast<std::shared_ptr<Result>*>(context));
            (*r)->done = true;
            (*r)->truthy = truthy;
        })) {
        delete result;
        return false;
    }
    for (int i = 0; i < 60 && !keep->done && !w->closed; ++i) ravet_standalone_pump(handle, .05);
    return !w->closed && keep->done && keep->truthy;
}
extern "C" bool ravet_standalone_snapshot(void*, const char*) { return false; }
extern "C" void ravet_standalone_destroy(void* handle) {
    auto* w = static_cast<Window*>(handle);
    if (!w) return;
    w->closed = true;
    SetWindowLongPtrW(w->hwnd, GWLP_USERDATA, 0);
    DestroyWindow(w->hwnd);
    delete w;
}
extern "C" bool ravet_standalone_open_settings(void* handle) {
    auto* w = live(handle);
    if (!w || !w->settings || !w->view) return false;
    openSettings(*w);
    return true;
}
extern "C" bool ravet_standalone_set_size(void* handle, double width, double height) {
    auto* w = live(handle);
    if (!w) return false;
    if (w->aspectWidth > 0) {
        double scale = std::min(width / w->aspectWidth, height / w->aspectHeight);
        width = w->aspectWidth * scale;
        height = w->aspectHeight * scale;
    }
    double scale = scaleOf(w->hwnd);
    SIZE frame = frameFor(*w, int(std::lround(width * scale)), int(std::lround(height * scale)));
    SetWindowPos(w->hwnd, nullptr, 0, 0, frame.cx, frame.cy, SWP_NOMOVE | SWP_NOZORDER | SWP_NOACTIVATE);
    return !w->closed;
}
extern "C" bool ravet_standalone_content_size(void* handle, double* width, double* height) {
    auto* w = live(handle);
    if (!w || !width || !height) return false;
    RECT client;
    if (!GetClientRect(w->hwnd, &client) || client.right <= 0 || client.bottom <= 0) return false;
    double scale = scaleOf(w->hwnd);
    *width = client.right / scale;
    *height = client.bottom / scale;
    return true;
}
extern "C" bool ravet_standalone_content_constraints(void* handle, bool enabled, double minWidth, double minHeight,
                                                     double maxWidth, double maxHeight, double width, double height) {
    auto* w = live(handle);
    if (!w) return false;
    if (enabled) {
        if (!std::isfinite(minWidth) || !std::isfinite(minHeight) || !std::isfinite(maxWidth) || !std::isfinite(maxHeight)
            || minWidth < 320 || maxWidth > 4096 || minHeight < 240 || maxHeight > 4096 || minWidth > maxWidth || minHeight > maxHeight
            || !std::isfinite(width) || !std::isfinite(height) || width < minWidth || width > maxWidth || height < minHeight || height > maxHeight)
            return false;
        w->minimumWidth = minWidth;
        w->minimumHeight = minHeight;
        w->maximumWidth = maxWidth;
        w->maximumHeight = maxHeight;
        return ravet_standalone_set_size(handle, width, height);
    }
    w->minimumWidth = w->originalMinimumWidth;
    w->minimumHeight = w->originalMinimumHeight;
    w->maximumWidth = w->maximumHeight = 0;
    return true;
}
extern "C" bool ravet_standalone_state(void* handle, bool* closed, bool* visible, bool* key, bool* miniaturized) {
    auto* w = static_cast<Window*>(handle);
    if (!w || !closed || !visible || !key || !miniaturized) return false;
    *closed = w->closed;
    *visible = !w->closed && IsWindowVisible(w->hwnd);
    *key = GetForegroundWindow() == w->hwnd;
    *miniaturized = IsIconic(w->hwnd) != 0;
    return true;
}
extern "C" bool ravet_standalone_request_close(void* handle) {
    auto* w = static_cast<Window*>(handle);
    if (!w) return false;
    if (!w->closed) SendMessageW(w->hwnd, WM_CLOSE, 0, 0);
    return w->closed;
}
