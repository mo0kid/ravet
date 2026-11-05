/* X11 probe for a RAVET editor inside a host window (Linux, X11/XWayland).
 *
 *   cc -O2 -o x11-editor-probe x11-editor-probe.c -lX11 -lXtst
 *   x11-editor-probe <title substring> <width> <height> <out prefix> [x,y | drag:x,y,dy]...
 *
 * Finds the top-level window whose name contains the title, then the deepest
 * viewable window inside it within 2 pixels of width x height (the editor's
 * plug; hosts may round a resizable editor's size), and prints
 * the window chain, the plug's map state and whether each ancestor carries
 * _XEMBED_INFO. Captures only the plug (XGetImage) to <prefix>-0.ppm, then for
 * each action injects XTest input at plug-relative coordinates (a click, or a
 * vertical drag by dy pixels) and captures <prefix>-<n>.ppm. Exit 0 when the
 * plug is found, viewable and its first capture is not a single colour.
 * Use it only on windows of a host you started for testing.
 */
#include <X11/Xlib.h>
#include <X11/Xutil.h>
#include <X11/extensions/XTest.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <time.h>
#include <unistd.h>

static Display *display;

static int named(Window window, const char *title) {
    char *name = NULL;
    int found = XFetchName(display, window, &name) && name && strstr(name, title);
    if (name) XFree(name);
    return found;
}
static Window find_title(Window window, const char *title, int depth) {
    if (named(window, title)) {
        XWindowAttributes a;
        /* Skip unmapped windows with the title (hosts may keep hidden copies). */
        if (XGetWindowAttributes(display, window, &a) && a.map_state == IsViewable) return window;
    }
    if (depth > 6) return 0;
    Window root, parent, *children = NULL, found = 0;
    unsigned count = 0;
    if (!XQueryTree(display, window, &root, &parent, &children, &count)) return 0;
    for (unsigned i = 0; i < count && !found; ++i) found = find_title(children[i], title, depth + 1);
    if (children) XFree(children);
    return found;
}
static int near(int a, int b) { return a >= b - 2 && a <= b + 2; }
static Window find_size(Window window, int width, int height, int depth) {
    XWindowAttributes a;
    if (depth > 12) return 0;
    Window root, parent, *children = NULL, found = 0;
    unsigned count = 0;
    if (XQueryTree(display, window, &root, &parent, &children, &count))
        for (unsigned i = 0; i < count && !found; ++i) found = find_size(children[i], width, height, depth + 1);
    if (children) XFree(children);
    if (!found && depth && XGetWindowAttributes(display, window, &a) && near(a.width, width) &&
        near(a.height, height) && a.map_state == IsViewable) found = window;
    return found;
}
static int has_property(Window window, const char *name) {
    Atom atom = XInternAtom(display, name, True), type;
    int format;
    unsigned long items, after;
    unsigned char *data = NULL;
    if (atom == None) return 0;
    int ok = XGetWindowProperty(display, window, atom, 0, 4, False, AnyPropertyType, &type, &format, &items,
                                &after, &data) == Success && type != None;
    if (data) XFree(data);
    return ok;
}
/* Writes a PPM; returns the number of distinct colours sampled (capped). */
static int capture(Window window, int width, int height, const char *path) {
    XImage *image = XGetImage(display, window, 0, 0, (unsigned)width, (unsigned)height, AllPlanes, ZPixmap);
    if (!image) { fprintf(stderr, "XGetImage failed\n"); return 0; }
    FILE *file = fopen(path, "wb");
    unsigned long seen[64];
    int distinct = 0;
    if (file) fprintf(file, "P6\n%d %d\n255\n", width, height);
    for (int y = 0; y < height; ++y)
        for (int x = 0; x < width; ++x) {
            unsigned long pixel = XGetPixel(image, x, y);
            unsigned char rgb[3] = {(unsigned char)((pixel & image->red_mask) >> 16),
                                    (unsigned char)((pixel & image->green_mask) >> 8),
                                    (unsigned char)(pixel & image->blue_mask)};
            if (file) fwrite(rgb, 1, 3, file);
            int known = 0;
            for (int i = 0; i < distinct && !known; ++i) known = seen[i] == pixel;
            if (!known && distinct < 64) seen[distinct++] = pixel;
        }
    if (file) fclose(file);
    XDestroyImage(image);
    return distinct;
}
static void pause_ms(int ms) { struct timespec t = {ms / 1000, (ms % 1000) * 1000000L}; nanosleep(&t, NULL); }
static void move(Window plug, int x, int y) {
    int rx, ry; Window child;
    XTranslateCoordinates(display, plug, DefaultRootWindow(display), x, y, &rx, &ry, &child);
    XTestFakeMotionEvent(display, -1, rx, ry, CurrentTime);
    XFlush(display);
    pause_ms(60);
}
static void button(int down) { XTestFakeButtonEvent(display, 1, down, CurrentTime); XFlush(display); pause_ms(80); }

int main(int argc, char **argv) {
    if (argc < 5) { fprintf(stderr, "usage: %s <title> <width> <height> <prefix> [x,y|drag:x,y,dy]...\n", argv[0]); return 2; }
    display = XOpenDisplay(NULL);
    if (!display) { fprintf(stderr, "no X display\n"); return 2; }
    const char *title = argv[1], *prefix = argv[4];
    int width = atoi(argv[2]), height = atoi(argv[3]);
    Window top = 0, plug = 0;
    for (int tries = 0; tries < 100 && !plug; ++tries) {
        top = find_title(DefaultRootWindow(display), title, 0);
        if (top) plug = find_size(top, width, height, 0);
        if (!plug) pause_ms(100);
    }
    if (!top) { fprintf(stderr, "no window titled *%s*\n", title); return 1; }
    if (!plug) {
        fprintf(stderr, "no viewable %dx%d window inside 0x%lx; windows there:\n", width, height, top);
        Window root, parent, *children = NULL; unsigned count = 0;
        if (XQueryTree(display, top, &root, &parent, &children, &count))
            for (unsigned i = 0; i < count; ++i) {
                XWindowAttributes a; XGetWindowAttributes(display, children[i], &a);
                fprintf(stderr, "  0x%lx %dx%d+%d+%d map_state=%d\n", children[i], a.width, a.height, a.x, a.y, a.map_state);
            }
        if (children) XFree(children);
        return 1;
    }
    printf("top 0x%lx\n", top);
    {
        XWindowAttributes a; XGetWindowAttributes(display, plug, &a);
        width = a.width; height = a.height;
    }
    for (Window w = plug; w && w != DefaultRootWindow(display);) {
        XWindowAttributes a; XGetWindowAttributes(display, w, &a);
        char *name = NULL; XFetchName(display, w, &name);
        printf("window 0x%lx %dx%d+%d+%d map_state=%s xembed_info=%d name=%s\n", w, a.width, a.height, a.x, a.y,
               a.map_state == IsViewable ? "viewable" : a.map_state == IsUnmapped ? "unmapped" : "unviewable",
               has_property(w, "_XEMBED_INFO"), name ? name : "");
        if (name) XFree(name);
        Window root, parent, *children = NULL; unsigned count = 0;
        if (!XQueryTree(display, w, &root, &parent, &children, &count)) break;
        if (children) XFree(children);
        if (w == top) break;
        w = parent;
    }
    char path[4096];
    snprintf(path, sizeof path, "%s-0.ppm", prefix);
    int colours = capture(plug, width, height, path);
    printf("capture %s colours>=%d\n", path, colours);
    for (int i = 5; i < argc; ++i) {
        int x, y, dy;
        if (sscanf(argv[i], "drag:%d,%d,%d", &x, &y, &dy) == 3) {
            move(plug, x, y); button(1);
            for (int step = 1; step <= 10; ++step) move(plug, x, y + dy * step / 10);
            button(0);
        } else if (sscanf(argv[i], "%d,%d", &x, &y) == 2) {
            move(plug, x, y); button(1); button(0);
        } else { fprintf(stderr, "bad action %s\n", argv[i]); return 2; }
        pause_ms(700);
        snprintf(path, sizeof path, "%s-%d.ppm", prefix, i - 4);
        printf("action %s capture %s colours>=%d\n", argv[i], path, capture(plug, width, height, path));
    }
    XCloseDisplay(display);
    return colours > 1 ? 0 : 1;
}
