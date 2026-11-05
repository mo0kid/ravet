#include "Table.hpp"
#include <dlfcn.h>
#include <iomanip>
#include <iostream>
#include <sstream>
#include <string>

static std::string quoted(const char *s) {
    std::ostringstream out;
    out << '"';
    for (const unsigned char c : std::string(s)) {
        if (c == '"' || c == '\\') out << '\\' << c;
        else if (c < 32) out << "\\u" << std::hex << std::setw(4) << std::setfill('0') << unsigned(c);
        else out << c;
    }
    out << '"';
    return out.str();
}
int main(int argc, char **argv) {
    try {
        if (argc != 2) throw std::runtime_error("usage: ravet_au_metadata product.dylib");
        void *library = dlopen(argv[1], RTLD_NOW | RTLD_LOCAL);
        if (!library) throw std::runtime_error(dlerror());
        const auto get = reinterpret_cast<const RavetPluginEntryV1 *(*)()>(dlsym(library, "ravet_plugin_v1"));
        if (!get) throw std::runtime_error("product does not export ravet_plugin_v1");
        const auto &p = *ravet::auv2::checked(get()).product;
        std::cout << "{\"name\":" << quoted(p.name) << ",\"vendor\":" << quoted(p.vendor)
                  << ",\"version\":" << quoted(p.version) << ",\"plugin_id\":" << quoted(p.plugin_id)
                  << ",\"au_type\":" << p.au_type << ",\"au_subtype\":" << p.au_subtype
                  << ",\"au_manufacturer\":" << p.au_manufacturer << "}\n";
        // Some Rust runtimes retain process-global state: keep the loaded image
        // alive until exit, just as the actual component does.
        return 0;
    } catch (const std::exception &error) { std::cerr << error.what() << '\n'; return 1; }
}
