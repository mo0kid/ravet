// Read metadata from a trusted product built for this executable's platform.
#include "RavetPlugin.h"
#include <iostream>
#include <iomanip>
#include <stdexcept>
#include <string>
#ifdef _WIN32
#include <windows.h>
#else
#include <dlfcn.h>
#endif

static void quoted(const char* value) {
    if (!value) throw std::runtime_error("null product string");
    std::cout << '"';
    for (unsigned char c : std::string(value)) {
        if (c == '"' || c == '\\') std::cout << '\\' << c;
        else if (c < 32) std::cout << "\\u" << std::hex << std::setw(4)
                                 << std::setfill('0') << unsigned(c) << std::dec;
        else std::cout << c;
    }
    std::cout << '"';
}
int main(int argc, char** argv) {
    try {
        if (argc != 2) throw std::runtime_error("usage: ravet-product-metadata product-library");
#ifdef _WIN32
        const auto library = LoadLibraryA(argv[1]);
        const auto get = library ? reinterpret_cast<const RavetPluginEntryV1*(*)()>(
            GetProcAddress(library, "ravet_plugin_v1")) : nullptr;
#else
        const auto library = dlopen(argv[1], RTLD_NOW | RTLD_LOCAL);
        if (!library) throw std::runtime_error(dlerror());
        const auto get = reinterpret_cast<const RavetPluginEntryV1*(*)()>(dlsym(library, "ravet_plugin_v1"));
#endif
        if (!get) throw std::runtime_error("cannot load product exporting ravet_plugin_v1");
        const auto table = get();
        if (!table || table->version != 1 || table->byte_size < sizeof(RavetPluginEntryV1) || !table->product)
            throw std::runtime_error("invalid RAVET product table v1");
        const auto& p = *table->product;
        std::cout << "{\"name\":"; quoted(p.name);
        std::cout << ",\"vendor\":"; quoted(p.vendor);
        std::cout << ",\"version\":"; quoted(p.version);
        std::cout << ",\"plugin_id\":"; quoted(p.plugin_id);
        std::cout << ",\"au_type\":" << p.au_type << ",\"au_subtype\":" << p.au_subtype
                  << ",\"au_manufacturer\":" << p.au_manufacturer << "}\n";
        // Retain the trusted Rust image until process exit.
        return 0;
    } catch (const std::exception& error) { std::cerr << error.what() << '\n'; return 1; }
}
