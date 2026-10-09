#include "RavetFileDialog.h"
#include <gtk/gtk.h>
namespace ravet::surface {
struct DialogContext {
    FileDialogCompletion callback;
    bool responded = false;
    ~DialogContext() { if(!responded) callback({}); }
};
bool openFile(const FileDialogOptions& options, FileDialogCompletion callback) {
    if (!callback || !gdk_display_get_default()) return false;
    auto* dialog = gtk_file_chooser_dialog_new(options.title.c_str(), static_cast<GtkWindow*>(options.parent),
        GTK_FILE_CHOOSER_ACTION_OPEN, "Cancel", GTK_RESPONSE_CANCEL, "Open", GTK_RESPONSE_ACCEPT, nullptr);
    if (!options.extension.empty()) {
        auto* filter = gtk_file_filter_new();
        gtk_file_filter_add_pattern(filter, ("*." + options.extension).c_str());
        gtk_file_chooser_add_filter(GTK_FILE_CHOOSER(dialog), filter);
    }
    auto* owned = new DialogContext{std::move(callback),false};
    g_signal_connect_data(dialog, "response", G_CALLBACK(+[](GtkDialog* dialog, gint response, gpointer context) {
        auto* state = static_cast<DialogContext*>(context);
        auto callback = state->callback;
        state->responded = true;
        std::filesystem::path path;
        if (response == GTK_RESPONSE_ACCEPT) {
            char* name = gtk_file_chooser_get_filename(GTK_FILE_CHOOSER(dialog));
            if (name) path = name;
            g_free(name);
        }
        gtk_widget_destroy(GTK_WIDGET(dialog));
        callback(std::move(path));
    }), owned, +[](gpointer context, GClosure*) { delete static_cast<DialogContext*>(context); }, GConnectFlags(0));
    gtk_widget_show(dialog);
    return true;
}
}
