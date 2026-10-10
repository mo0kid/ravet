#include "DirectoryPicker.h"
#ifdef _WIN32
#include <windows.h>
#include <shobjidl.h>
#include <wrl/client.h>
using Microsoft::WRL::ComPtr;
std::filesystem::path foundryChooseDirectory(void* parent,const char* title,const char* initial) {
    const HRESULT initialized=CoInitializeEx(nullptr,COINIT_APARTMENTTHREADED);
    std::filesystem::path result;
    {
        auto wide=[](const char* text) {
            const int size=MultiByteToWideChar(CP_UTF8,MB_ERR_INVALID_CHARS,text,-1,nullptr,0);
            std::wstring value(size>0?size:0,L'\0');
            if(size) MultiByteToWideChar(CP_UTF8,MB_ERR_INVALID_CHARS,text,-1,value.data(),size);
            return value;
        };
        ComPtr<IFileOpenDialog> dialog;
        if(SUCCEEDED(CoCreateInstance(CLSID_FileOpenDialog,nullptr,CLSCTX_INPROC_SERVER,IID_PPV_ARGS(&dialog)))) {
            DWORD flags=0;dialog->GetOptions(&flags);
            dialog->SetOptions(flags|FOS_PICKFOLDERS|FOS_FORCEFILESYSTEM|FOS_PATHMUSTEXIST|FOS_NOCHANGEDIR);
            const auto heading=wide(title);dialog->SetTitle(heading.c_str());
            ComPtr<IShellItem> folder;
            const auto start=wide(initial);
            if(*initial&&SUCCEEDED(SHCreateItemFromParsingName(start.c_str(),nullptr,IID_PPV_ARGS(&folder)))) dialog->SetFolder(folder.Get());
            if(SUCCEEDED(dialog->Show(static_cast<HWND>(parent)))) {
                ComPtr<IShellItem> selected;
                if(SUCCEEDED(dialog->GetResult(&selected))) {
                    PWSTR path=nullptr;
                    if(SUCCEEDED(selected->GetDisplayName(SIGDN_FILESYSPATH,&path))) {result=path;CoTaskMemFree(path);}
                }
            }
        }
    }
    if(SUCCEEDED(initialized)) CoUninitialize();
    return result;
}
#else
#include <gtk/gtk.h>
std::filesystem::path foundryChooseDirectory(void* parent,const char* title,const char* initial) {
    auto* dialog=gtk_file_chooser_dialog_new(title,static_cast<GtkWindow*>(parent),GTK_FILE_CHOOSER_ACTION_SELECT_FOLDER,
        "Cancel",GTK_RESPONSE_CANCEL,"Choose",GTK_RESPONSE_ACCEPT,nullptr);
    gtk_file_chooser_set_create_folders(GTK_FILE_CHOOSER(dialog),FALSE);
    if(*initial) gtk_file_chooser_set_current_folder(GTK_FILE_CHOOSER(dialog),initial);
    std::filesystem::path result;
    if(gtk_dialog_run(GTK_DIALOG(dialog))==GTK_RESPONSE_ACCEPT) {
        char* path=gtk_file_chooser_get_filename(GTK_FILE_CHOOSER(dialog));
        if(path) {result=std::filesystem::u8path(path);g_free(path);}
    }
    gtk_widget_destroy(dialog);
    return result;
}
#endif
