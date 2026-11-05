// Generic RAVET LV2 UI, written once against RavetPlugin.h.
//
// The UI never touches the DSP instance. LV2 hosts may run UIs in another
// process and instance-access is not universal, so the UI owns a separate
// "mirror" instance of the product table that is never prepared or processed:
//   host → UI:   port_event on a parameter port  → set_value on the mirror,
//                which the editor sees on its next editor_idle;
//   UI → host:   the editor's edits (take_edits) → the host's write function
//                on the same control port (float protocol), bracketed with
//                ui:touch when the host offers it. The DSP instance then sees
//                an ordinary control-port change.
// The bypass parameter appears as the inverted lv2:enabled port.
//
// The view kind follows the platform: X11UI (Linux and other Unix), CocoaUI
// (macOS) and WindowsUI (Windows). The editor is opened inside ui:parent.
#include "RavetLv2Product.hpp"

#include "lv2/core/lv2.h"
#include "lv2/ui/ui.h"

#include <array>
#include <cmath>
#include <cstdio>
#include <cstring>
#include <memory>
#include <mutex>
#include <string>
#include <vector>

#if defined(__APPLE__)
#include <objc/message.h>
#include <objc/runtime.h>
#endif

namespace {
using namespace ravet::lv2;

#if defined(_WIN32)
constexpr uint32_t viewKind = RAVET_PLUGIN_VIEW_WIN32;
#elif defined(__APPLE__)
constexpr uint32_t viewKind = RAVET_PLUGIN_VIEW_COCOA;
#else
constexpr uint32_t viewKind = RAVET_PLUGIN_VIEW_X11;
#endif

struct Library {
    const RavetPluginEntryV1 *entry = nullptr;
    std::string error, uri;
    LV2UI_Descriptor descriptor{};
};
Library &library();

struct Ui {
    const RavetPluginEntryV1 *entry = nullptr;
    void *mirror = nullptr;
    LV2UI_Write_Function write = nullptr;
    LV2UI_Controller controller = nullptr;
    LV2UI_Touch *touch = nullptr;
    std::vector<Port> ports;
    std::vector<int32_t> portOfParameter;  // -1 when the parameter has no input port
    RavetPluginParameterV1 bypass{};
    bool open = false;
};

// The native child the product created inside `parent`, used as the LV2 widget.
// editor_open does not return it, so it is found as the parent's newest child.
void *childView(void *parent) {
#if defined(__APPLE__)
    auto send = reinterpret_cast<id (*)(id, SEL)>(objc_msgSend);
    id subviews = send(static_cast<id>(parent), sel_registerName("subviews"));
    id last = subviews ? send(subviews, sel_registerName("lastObject")) : nullptr;
    return last ? static_cast<void *>(last) : parent;
#elif defined(_WIN32)
    HWND child = GetWindow(static_cast<HWND>(parent), GW_CHILD);
    return child ? static_cast<void *>(child) : parent;
#else
    // Xlib is loaded at run time so the UI module has no X11 build dependency.
    using Display = void;
    using Window = unsigned long;
    static void *x11 = dlopen("libX11.so.6", RTLD_NOW | RTLD_LOCAL);
    if (!x11) return parent;
    auto openDisplay = reinterpret_cast<Display *(*)(const char *)>(dlsym(x11, "XOpenDisplay"));
    auto closeDisplay = reinterpret_cast<int (*)(Display *)>(dlsym(x11, "XCloseDisplay"));
    auto queryTree = reinterpret_cast<int (*)(Display *, Window, Window *, Window *, Window **, unsigned *)>(
        dlsym(x11, "XQueryTree"));
    auto xfree = reinterpret_cast<int (*)(void *)>(dlsym(x11, "XFree"));
    auto sync = reinterpret_cast<int (*)(Display *, int)>(dlsym(x11, "XSync"));
    using Handler = int (*)(Display *, void *);
    auto setHandler = reinterpret_cast<Handler (*)(Handler)>(dlsym(x11, "XSetErrorHandler"));
    if (!openDisplay || !closeDisplay || !queryTree || !xfree || !sync || !setHandler) return parent;
    Display *display = openDisplay(nullptr);
    if (!display) return parent;
    // A bad parent must not reach Xlib's default handler, which exits.
    const Handler previous = setHandler([](Display *, void *) { return 0; });
    Window root = 0, up = 0, *children = nullptr;
    unsigned count = 0;
    Window found = Window(reinterpret_cast<uintptr_t>(parent));
    if (queryTree(display, found, &root, &up, &children, &count) && count && children) found = children[count - 1];
    if (children) xfree(children);
    sync(display, 0);
    setHandler(previous);
    closeDisplay(display);
    return reinterpret_cast<void *>(uintptr_t(found));
#endif
}

const LV2_Feature *findFeature(const LV2_Feature *const *features, const char *uri) {
    for (auto f = features; f && *f; ++f) if (!std::strcmp((*f)->URI, uri)) return *f;
    return nullptr;
}

LV2UI_Handle instantiate(const LV2UI_Descriptor *, const char *pluginUri, const char *, LV2UI_Write_Function write,
                         LV2UI_Controller controller, LV2UI_Widget *widget, const LV2_Feature *const *features) {
    const auto *entry = library().entry;
    if (!entry || !pluginUri || library().uri != pluginUri || !write) return nullptr;
    const auto &product = *entry->product;
    if (!product.editor_width || !product.editor_height) return nullptr;
    const auto *parentFeature = findFeature(features, LV2_UI__parent);
    if (!parentFeature || !parentFeature->data) return nullptr;
    std::unique_ptr<Ui> self;
    try {
        self = std::make_unique<Ui>();
        self->entry = entry;
        self->write = write;
        self->controller = controller;
        if (const auto *touch = findFeature(features, LV2_UI__touch)) self->touch = static_cast<LV2UI_Touch *>(touch->data);
        self->ports = portLayout(*entry);
        self->portOfParameter.assign(product.parameter_count, -1);
        for (uint32_t i = 0; i < self->ports.size(); ++i) {
            const auto kind = self->ports[i].kind;
            if (kind == PortKind::Parameter || kind == PortKind::Enabled)
                self->portOfParameter[self->ports[i].parameter] = int32_t(i);
        }
        if (product.bypass_index >= 0) entry->parameter(uint32_t(product.bypass_index), &self->bypass);
    } catch (...) {
        return nullptr;
    }
    self->mirror = entry->create();
    if (!self->mirror) return nullptr;
    if (entry->editor_open(self->mirror, viewKind, parentFeature->data, product.editor_width, product.editor_height) != 0) {
        entry->destroy(self->mirror);
        return nullptr;
    }
    self->open = true;
    if (widget) *widget = childView(parentFeature->data);
    if (const auto *resize = findFeature(features, LV2_UI__resize)) {
        auto *host = static_cast<LV2UI_Resize *>(resize->data);
        if (host && host->ui_resize) host->ui_resize(host->handle, int(product.editor_width), int(product.editor_height));
    }
    return self.release();
}

void cleanup(LV2UI_Handle handle) {
    auto *self = static_cast<Ui *>(handle);
    if (self->open) self->entry->editor_close(self->mirror);
    self->entry->destroy(self->mirror);
    delete self;
}

void portEvent(LV2UI_Handle handle, uint32_t port, uint32_t size, uint32_t format, const void *buffer) {
    auto *self = static_cast<Ui *>(handle);
    if (format != 0 || size != sizeof(float) || !buffer || port >= self->ports.size()) return;
    const auto &info = self->ports[port];
    const float value = *static_cast<const float *>(buffer);
    if (!std::isfinite(value)) return;
    if (info.kind == PortKind::Parameter || info.kind == PortKind::ParameterOut)
        self->entry->set_value(self->mirror, info.parameter, double(value));
    else if (info.kind == PortKind::Enabled)
        self->entry->set_value(self->mirror, info.parameter, bypassPhysical(self->bypass, value < 0.5f));
}

int idle(LV2UI_Handle handle) {
    auto *self = static_cast<Ui *>(handle);
    std::array<RavetPluginEditV1, 64> edits{};
    for (;;) {
        const uint32_t count = self->entry->take_edits(self->mirror, edits.data(), uint32_t(edits.size()));
        for (uint32_t i = 0; i < count && i < edits.size(); ++i) {
            const auto &edit = edits[i];
            if (edit.index >= self->portOfParameter.size() || self->portOfParameter[edit.index] < 0) continue;
            const auto port = uint32_t(self->portOfParameter[edit.index]);
            if (edit.kind == RAVET_PLUGIN_EDIT_BEGIN || edit.kind == RAVET_PLUGIN_EDIT_END) {
                if (self->touch && self->touch->touch)
                    self->touch->touch(self->touch->handle, port, edit.kind == RAVET_PLUGIN_EDIT_BEGIN);
            } else if (edit.kind == RAVET_PLUGIN_EDIT_VALUE && std::isfinite(edit.value)) {
                float value = self->ports[port].kind == PortKind::Enabled
                                  ? (isBypassed(self->bypass, edit.value) ? 0.f : 1.f)
                                  : float(edit.value);
                self->write(self->controller, port, sizeof(float), 0, &value);
            }
        }
        if (count < edits.size()) break;
    }
    self->entry->editor_idle(self->mirror);
    return 0;
}

int resize(LV2UI_Feature_Handle handle, int width, int height) {
    auto *self = static_cast<Ui *>(handle);
    if (width <= 0 || height <= 0 || !self->entry->product->editor_resizable) return 1;
    return self->entry->editor_resize(self->mirror, uint32_t(width), uint32_t(height)) == 0 ? 0 : 1;
}

const void *extensionData(const char *uri) {
    static const LV2UI_Idle_Interface idleInterface{idle};
    static const LV2UI_Resize resizeInterface{nullptr, resize};
    if (!std::strcmp(uri, LV2_UI__idleInterface)) return &idleInterface;
    if (!std::strcmp(uri, LV2_UI__resize)) return &resizeInterface;
    return nullptr;
}

std::string uiUri(const std::string &plugin) { return plugin + (plugin.find('#') == std::string::npos ? "#ui" : "-ui"); }

Library &library() {
    static Library instance;
    static std::once_flag once;
    std::call_once(once, [] {
        const std::string directory = moduleDirectory(reinterpret_cast<const void *>(&extensionData));
        instance.entry = loadProduct(directory + productFile, instance.error);
        if (!instance.entry) {
            std::fprintf(stderr, "RAVET LV2 UI: %s\n", instance.error.c_str());
            return;
        }
        instance.uri = instance.entry->product->lv2_uri;
        static std::string ui = uiUri(instance.uri);
        instance.descriptor = {ui.c_str(), instantiate, cleanup, portEvent, extensionData};
    });
    return instance;
}

} // namespace

LV2_SYMBOL_EXPORT const LV2UI_Descriptor *lv2ui_descriptor(uint32_t index) {
    // As in the DSP module: no C++ exception may reach the C host.
    try {
        auto &loaded = library();
        return index == 0 && loaded.entry && loaded.entry->product->editor_width ? &loaded.descriptor : nullptr;
    } catch (...) {
        return nullptr;
    }
}
