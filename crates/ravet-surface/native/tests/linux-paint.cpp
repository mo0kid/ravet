// Check the native GTK paint path, not only JavaScript execution or DOM state.
#include "RavetWebView.h"
#include <gtk/gtk.h>
#include <gtk/gtkx.h>
#include <webkit2/webkit2.h>
#include <chrono>
#include <thread>
#include <iostream>
#include <string>
#include <algorithm>
static GtkWidget* findWeb(GtkWidget* widget) {
    if(WEBKIT_IS_WEB_VIEW(widget)) return widget;
    if(GTK_IS_CONTAINER(widget)) {
        GList* children=gtk_container_get_children(GTK_CONTAINER(widget));GtkWidget* found=nullptr;
        for(GList* p=children;p&&!found;p=p->next) found=findWeb(GTK_WIDGET(p->data));
        g_list_free(children);return found;
    }
    return nullptr;
}
static void pump(int ms) {
    auto end=std::chrono::steady_clock::now()+std::chrono::milliseconds(ms);
    while(std::chrono::steady_clock::now()<end){ravet_web_pump();std::this_thread::sleep_for(std::chrono::milliseconds(2));}
}
int main(int argc,char** argv) {
    if(argc!=2||!gtk_init_check(nullptr,nullptr)) return 2;
    auto* parent=gtk_window_new(GTK_WINDOW_TOPLEVEL);auto* socket=gtk_socket_new();
    gtk_window_set_default_size(GTK_WINDOW(parent),320,180);gtk_window_set_resizable(GTK_WINDOW(parent),FALSE);
    gtk_container_add(GTK_CONTAINER(parent),socket);gtk_widget_show_all(parent);
    auto native=reinterpret_cast<void*>(gtk_socket_get_id(GTK_SOCKET(socket)));
    auto* view=ravet_web_create_html("<!doctype html><html><body style='margin:0;display:flex;height:100vh'><div style='width:50%;background:#123456'></div><div style='width:50%;background:#e87932'></div></body></html>",320,180,nullptr,nullptr,nullptr);
    if(!view) return 3;
    ravet_web_attach(view,native);
    GtkWidget* web=nullptr;GList* windows=gtk_window_list_toplevels();
    for(GList* p=windows;p&&!web;p=p->next) web=findWeb(GTK_WIDGET(p->data));g_list_free(windows);
    if(!web) return 4;
    pump(2500);
    GtkAllocation allocation;gtk_widget_get_allocation(web,&allocation);
    auto* surface=cairo_image_surface_create(CAIRO_FORMAT_ARGB32,allocation.width,allocation.height);
    auto* cr=cairo_create(surface);gtk_widget_draw(web,cr);cairo_destroy(cr);cairo_surface_flush(surface);
    auto* data=cairo_image_surface_get_data(surface);int stride=cairo_image_surface_get_stride(surface);
    long blue=0,orange=0,total=long(allocation.width)*allocation.height;
    for(int y=0;y<allocation.height;++y)for(int x=0;x<allocation.width;++x){auto* p=data+y*stride+x*4;auto close=[](int a,int b){return std::abs(a-b)<4;};if(close(p[2],0x12)&&close(p[1],0x34)&&close(p[0],0x56))++blue;if(close(p[2],0xe8)&&close(p[1],0x79)&&close(p[0],0x32))++orange;}
    const bool saved=cairo_surface_write_to_png(surface,argv[1])==CAIRO_STATUS_SUCCESS;
    cairo_surface_destroy(surface);
    bool painted=allocation.width==320&&allocation.height==180&&blue>total*0.4&&orange>total*0.4&&saved;
    std::cout<<allocation.width<<"x"<<allocation.height<<" blue="<<blue<<" orange="<<orange<<" total="<<total<<" painted="<<painted<<'\n';
    ravet_web_destroy(view);gtk_widget_destroy(parent);pump(100);
    return painted?0:1;
}
