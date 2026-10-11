// Thin Windows/Linux shell. RAVET owns the web editor; Rust owns project actions.
#include "RavetWebView.h"
#include "DirectoryPicker.h"
#include <iostream>
#include <vector>
#include <stdexcept>
#include <cstring>
#ifdef _WIN32
#include <windows.h>
#else
#include <gtk/gtk.h>
#include <gtk/gtkx.h>
#endif
extern "C" {
void* ravet_foundry_create();
void ravet_foundry_message(void*, const char*);
size_t ravet_foundry_snapshot(void*, char*, size_t);
void ravet_foundry_destroy(void*);
size_t ravet_foundry_document(char*, size_t);
bool ravet_foundry_browse_request(const char*,char*,char*,char*);
void ravet_foundry_directory_selected(void*,const char*,const char*);
}
struct Owner {
    void* controller = nullptr;
    void* editor = nullptr;
    void* dialogParent = nullptr;
    ~Owner() { if(editor) ravet_web_destroy(editor); if(controller) ravet_foundry_destroy(controller); }
};
static void message(void* context,const char* text) {
    auto* owner=static_cast<Owner*>(context);
    try {
        char field[64]{},initial[4097]{},title[128]{};
        if(ravet_foundry_browse_request(text,field,initial,title)) {
            auto path=foundryChooseDirectory(owner->dialogParent,title,initial);
            // u8string() is std::u8string (char8_t) from C++20; the C ABI takes UTF-8 bytes.
            if(!path.empty()) {auto utf8=path.u8string();ravet_foundry_directory_selected(owner->controller,field,reinterpret_cast<const char*>(utf8.c_str()));}
        } else ravet_foundry_message(owner->controller,text);
    } catch(const std::exception& error) {std::cerr<<"Directory selection failed: "<<error.what()<<'\n';}
}
static size_t snapshot(void* context,char* output,size_t size) {
    return ravet_foundry_snapshot(static_cast<Owner*>(context)->controller,output,size);
}
#ifdef _WIN32
static LRESULT CALLBACK windowProc(HWND window, UINT message, WPARAM w, LPARAM l) {
    if(message == WM_CLOSE) { DestroyWindow(window); return 0; }
    if(message == WM_DESTROY) { PostQuitMessage(0); return 0; }
    if(message == WM_SIZE) {
        // The RAVET child occupies the client's entire area.
        for(HWND child=GetWindow(window,GW_CHILD);child;child=GetWindow(child,GW_HWNDNEXT))
            SetWindowPos(child,nullptr,0,0,LOWORD(l),HIWORD(l),SWP_NOZORDER|SWP_NOACTIVATE);
        return 0;
    }
    return DefWindowProcW(window,message,w,l);
}
#endif
int main(int argc, char** argv) {
    try {
        if(argc != 1) throw std::runtime_error("This is the Foundry GUI. Use ravet-foundry --help for the CLI.");
        (void)argv;
        Owner state; state.controller=ravet_foundry_create();
        if(!state.controller) throw std::runtime_error("Project controller could not start");
        std::vector<char> html(ravet_foundry_document(nullptr,0));
        ravet_foundry_document(html.data(),html.size());
#ifdef _WIN32
        WNDCLASSW cls{};cls.lpfnWndProc=windowProc;cls.hInstance=GetModuleHandleW(nullptr);
        cls.lpszClassName=L"RavetFoundryWindow";cls.hCursor=LoadCursorW(nullptr,IDC_ARROW);
        RegisterClassW(&cls);
        auto parent=CreateWindowExW(0,cls.lpszClassName,L"RAVET Foundry",WS_OVERLAPPEDWINDOW,
            CW_USEDEFAULT,CW_USEDEFAULT,1140,860,nullptr,nullptr,cls.hInstance,nullptr);
        if(!parent) throw std::runtime_error("Foundry window could not be created");
        void* nativeParent=parent;
#else
        if(!gtk_init_check(nullptr,nullptr)) throw std::runtime_error("Foundry requires an X11 display (XWayland is acceptable)");
        auto* parent=gtk_window_new(GTK_WINDOW_TOPLEVEL);
        gtk_window_set_title(GTK_WINDOW(parent),"RAVET Foundry");
        gtk_window_set_default_size(GTK_WINDOW(parent),1120,800);
        auto* socket=gtk_socket_new();
        g_signal_connect(socket,"plug-removed",G_CALLBACK(+[](GtkSocket*,gpointer)->gboolean{return TRUE;}),nullptr);
        gtk_container_add(GTK_CONTAINER(parent),socket);
        bool closed=false;
        g_signal_connect(parent,"delete-event",G_CALLBACK(+[](GtkWidget*,GdkEvent*,gpointer context)->gboolean{*static_cast<bool*>(context)=true;return TRUE;}),&closed);
        gtk_widget_show_all(parent);
        void* nativeParent=reinterpret_cast<void*>(gtk_socket_get_id(GTK_SOCKET(socket)));
#endif
        state.dialogParent=parent;
        state.editor=ravet_web_create_html(html.data(),1120,800,&state,message,snapshot);
        if(!state.editor) throw std::runtime_error("RAVET editor creation failed. Check native UI dependencies and the WebView2 Runtime/loader on Windows.");
        ravet_web_set_background(state.editor,23./255.,21./255.,29./255.);
        ravet_web_attach(state.editor,nativeParent);ravet_web_set_visible(state.editor,true);
#ifdef _WIN32
        ShowWindow(parent,SW_SHOW);UpdateWindow(parent);
        MSG message;while(GetMessageW(&message,nullptr,0,0)>0){TranslateMessage(&message);DispatchMessageW(&message);}
#else
        while(!closed) {ravet_web_pump();g_usleep(10000);}
        ravet_web_destroy(state.editor);state.editor=nullptr;gtk_widget_destroy(parent);
#endif
        return 0;
    } catch(const std::exception& error) {std::cerr<<"Foundry: "<<error.what()<<'\n';return 1;}
}
