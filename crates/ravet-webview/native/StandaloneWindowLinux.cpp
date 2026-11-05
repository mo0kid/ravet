// GTK 3 standalone window for ravet-webview: the same C ABI as StandaloneWindow.mm.
// The editor embeds through a GtkSocket because the Linux web backend attaches to
// an X11 window ID; X11 (including XWayland) is therefore required. Sizes are GTK
// logical pixels. Audio/MIDI settings persistence is handled in Rust.
#include "../../ravet-surface/native/RavetWebView.h"
#include <gtk/gtk.h>
#include <gtk/gtkx.h>
#include <gdk/gdkx.h>
#include <algorithm>
#include <cmath>
#include <memory>

namespace {
struct Host {
    GtkWidget* window = nullptr;
    GtkWidget* socket = nullptr;
    void* view = nullptr;
    bool closed = false, settings = false;
    int titlebar = 0;  // client-side header bar height, part of the window size
    double aspectWidth = 0, aspectHeight = 0;
    double minimumWidth = 0, minimumHeight = 0, maximumWidth = 0, maximumHeight = 0;
    double originalMinimumWidth = 0, originalMinimumHeight = 0;
};
// GTK 3.20+ applies hints to the whole window, header bar included, and does
// not honour a base size for the aspect ratio of a client-side-decorated
// window. Only min/max sizes are hinted; the page letterboxes itself at other
// aspect ratios, and programmatic sizes keep the locked aspect.
void hints(Host& w) {
    GdkGeometry geometry{};
    int mask = GDK_HINT_MIN_SIZE;
    geometry.min_width = int(std::lround(w.minimumWidth));
    geometry.min_height = int(std::lround(w.minimumHeight)) + w.titlebar;
    if (w.maximumWidth > 0) {
        mask |= GDK_HINT_MAX_SIZE;
        geometry.max_width = int(std::lround(w.maximumWidth));
        geometry.max_height = int(std::lround(w.maximumHeight)) + w.titlebar;
    }
    gtk_window_set_geometry_hints(GTK_WINDOW(w.window), nullptr, &geometry, GdkWindowHints(mask));
}
void openSettings(Host& w) {
    if (w.view && !w.closed)
        ravet_web_eval(w.view, "typeof window.ravetOpenSettings==='function'&&window.ravetOpenSettings()", nullptr, nullptr);
}
Host* live(void* handle) {
    auto* w = static_cast<Host*>(handle);
    return w && !w->closed ? w : nullptr;
}
}

extern "C" void* ravet_standalone_create(bool hidden, bool settings, const char* title, double width, double height, bool lockAspect) {
    if (!title || !ravet_web_is_main_thread()) return nullptr;
    if (!gdk_display_get_default()) gdk_set_allowed_backends("x11");
    if (!gtk_init_check(nullptr, nullptr) || !GDK_IS_X11_DISPLAY(gdk_display_get_default())) return nullptr;
    auto w = std::make_unique<Host>();
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
    w->window = gtk_window_new(GTK_WINDOW_TOPLEVEL);
    g_object_ref_sink(w->window);
    gtk_window_set_title(GTK_WINDOW(w->window), title);
    gtk_window_set_position(GTK_WINDOW(w->window), GTK_WIN_POS_CENTER);
    if (settings) {
        GtkWidget* bar = gtk_header_bar_new();
        gtk_header_bar_set_title(GTK_HEADER_BAR(bar), title);
        gtk_header_bar_set_show_close_button(GTK_HEADER_BAR(bar), TRUE);
        GtkWidget* button = gtk_button_new_with_label("Audio / MIDI…");
        g_signal_connect(button, "clicked", G_CALLBACK(+[](GtkButton*, gpointer data) { openSettings(*static_cast<Host*>(data)); }), w.get());
        gtk_header_bar_pack_end(GTK_HEADER_BAR(bar), button);
        gtk_window_set_titlebar(GTK_WINDOW(w->window), bar);
    }
    w->socket = gtk_socket_new();
    gtk_widget_set_hexpand(w->socket, TRUE);
    gtk_widget_set_vexpand(w->socket, TRUE);
    gtk_widget_set_size_request(w->socket, 1, 1);
    gtk_container_add(GTK_CONTAINER(w->window), w->socket);
    // Keep the socket when the editor detaches; it is reused on reattachment.
    g_signal_connect(w->socket, "plug-removed", G_CALLBACK(+[](GtkSocket*, gpointer) -> gboolean { return TRUE; }), nullptr);
    g_signal_connect(w->socket, "size-allocate", G_CALLBACK(+[](GtkWidget*, GdkRectangle* rect, gpointer data) {
        auto& w = *static_cast<Host*>(data);
        if (w.view && !w.closed && rect->width > 1 && rect->height > 1) ravet_web_set_size(w.view, rect->width, rect->height);
    }), w.get());
    g_signal_connect(w->window, "delete-event", G_CALLBACK(+[](GtkWidget* widget, GdkEvent*, gpointer data) -> gboolean {
        // Close admission is terminal; destruction belongs to ravet_standalone_destroy.
        static_cast<Host*>(data)->closed = true;
        gtk_widget_hide(widget);
        return TRUE;
    }), w.get());
    // Default and programmatic sizes exclude the header bar; geometry hints
    // include it (GTK 3.20+), so the minimum adds its height.
    if (GtkWidget* bar = gtk_window_get_titlebar(GTK_WINDOW(w->window))) {
        gtk_widget_show_all(bar);
        gtk_widget_get_preferred_height(bar, nullptr, &w->titlebar);
    }
    gtk_window_set_default_size(GTK_WINDOW(w->window), int(std::lround(width)), int(std::lround(height)));
    hints(*w);
    gtk_widget_realize(w->window);
    gtk_widget_realize(w->socket);
    if (!hidden) gtk_widget_show_all(w->window);
    return w.release();
}
extern "C" void ravet_standalone_attach_editor(void* handle, void* view) {
    auto* w = static_cast<Host*>(handle);
    if (!w) return;
    w->view = view;
    GtkAllocation rect;
    gtk_widget_get_allocation(w->socket, &rect);
    if (view && rect.width > 1 && rect.height > 1) ravet_web_set_size(view, rect.width, rect.height);
}
extern "C" void* ravet_standalone_view(void* handle) {
    auto* w = live(handle);
    if (!w) return nullptr;
    return reinterpret_cast<void*>(uintptr_t(gtk_socket_get_id(GTK_SOCKET(w->socket))));
}
extern "C" bool ravet_standalone_pump(void* handle, double seconds) {
    auto* w = static_cast<Host*>(handle);
    gint64 end = g_get_monotonic_time() + gint64(std::max(0.0, seconds) * 1e6);
    for (;;) {
        while (g_main_context_iteration(nullptr, FALSE)) {}
        gint64 left = end - g_get_monotonic_time();
        if (left <= 0) break;
        // Block until an event or the deadline, whichever is first.
        GSource* timeout = g_timeout_source_new(guint(std::max<gint64>(1, left / 1000)));
        g_source_set_callback(timeout, +[](gpointer) -> gboolean { return G_SOURCE_REMOVE; }, nullptr, nullptr);
        g_source_attach(timeout, nullptr);
        g_main_context_iteration(nullptr, TRUE);
        g_source_destroy(timeout);
        g_source_unref(timeout);
    }
    return w && !w->closed && gtk_widget_get_visible(w->window);
}
extern "C" void ravet_standalone_fit_editor(void* handle) {
    if (auto* w = live(handle)) ravet_standalone_attach_editor(w, w->view);
}
extern "C" bool ravet_standalone_eval(void* handle, const char* script) {
    auto* w = live(handle);
    if (!w || !w->view || !script) return false;
    struct Result { bool done = false, truthy = false; };
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
    auto* w = static_cast<Host*>(handle);
    if (!w) return;
    w->closed = true;
    gtk_widget_destroy(w->window);
    g_object_unref(w->window);
    while (g_main_context_iteration(nullptr, FALSE)) {}
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
    gtk_window_resize(GTK_WINDOW(w->window), int(std::lround(width)), int(std::lround(height)));
    return !w->closed;
}
extern "C" bool ravet_standalone_content_size(void* handle, double* width, double* height) {
    auto* w = live(handle);
    if (!w || !width || !height) return false;
    GtkAllocation rect;
    gtk_widget_get_allocation(w->socket, &rect);
    if (rect.width <= 1 || rect.height <= 1) return false;
    *width = rect.width;
    *height = rect.height;
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
        hints(*w);
        return ravet_standalone_set_size(handle, width, height);
    }
    w->minimumWidth = w->originalMinimumWidth;
    w->minimumHeight = w->originalMinimumHeight;
    w->maximumWidth = w->maximumHeight = 0;
    hints(*w);
    return true;
}
extern "C" bool ravet_standalone_state(void* handle, bool* closed, bool* visible, bool* key, bool* miniaturized) {
    auto* w = static_cast<Host*>(handle);
    if (!w || !closed || !visible || !key || !miniaturized) return false;
    GdkWindow* native = gtk_widget_get_window(w->window);
    *closed = w->closed;
    *visible = !w->closed && gtk_widget_get_visible(w->window);
    *key = gtk_window_is_active(GTK_WINDOW(w->window));
    *miniaturized = native && (gdk_window_get_state(native) & GDK_WINDOW_STATE_ICONIFIED);
    return true;
}
extern "C" bool ravet_standalone_request_close(void* handle) {
    auto* w = static_cast<Host*>(handle);
    if (!w) return false;
    if (!w->closed) gtk_window_close(GTK_WINDOW(w->window));
    while (!w->closed && g_main_context_iteration(nullptr, FALSE)) {}
    return w->closed;
}
