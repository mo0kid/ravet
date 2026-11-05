// Register exactly the requested trusted component inside a private auvaltool
// process. The system registrar can otherwise select an installed product with
// the same identity, or omit an Intel-only component on Apple silicon.
#include <AudioToolbox/AudioToolbox.h>
#include <CoreFoundation/CoreFoundation.h>
#include <cstdio>
#include <cstdlib>
#include <cstring>

namespace {
[[noreturn]] void fail(const char* message) {
    std::fprintf(stderr, "RAVET AU validation registration: %s\n", message);
    std::exit(1);
}

CFTypeRef field(CFDictionaryRef dictionary, CFStringRef key, CFTypeID type) {
    auto value = CFDictionaryGetValue(dictionary, key);
    if (!value || CFGetTypeID(value) != type) fail("missing or invalid component metadata");
    return value;
}

UInt32 fourcc(CFDictionaryRef component, CFStringRef key) {
    auto value = static_cast<CFStringRef>(field(component, key, CFStringGetTypeID()));
    char text[5]{};
    if (CFStringGetLength(value) != 4 ||
        !CFStringGetCString(value, text, sizeof(text), kCFStringEncodingASCII)) fail("invalid FourCC");
    return (UInt32(text[0]) << 24) | (UInt32(text[1]) << 16) | (UInt32(text[2]) << 8) | UInt32(text[3]);
}

__attribute__((constructor)) void register_product() {
    const char* path = std::getenv("RAVET_AUVAL_BUNDLE");
    if (!path || !*path) fail("RAVET_AUVAL_BUNDLE is required");
    auto url = CFURLCreateFromFileSystemRepresentation(nullptr,
        reinterpret_cast<const UInt8*>(path), std::strlen(path), true);
    if (!url) fail("invalid bundle path");
    auto bundle = CFBundleCreate(nullptr, url);
    CFRelease(url);
    if (!bundle) fail("cannot open bundle");
    auto info = CFBundleGetInfoDictionary(bundle);
    if (!info) fail("missing Info.plist");
    auto components = static_cast<CFArrayRef>(field(info, CFSTR("AudioComponents"), CFArrayGetTypeID()));
    if (CFArrayGetCount(components) != 1) fail("expected one AudioComponent");
    auto value = CFArrayGetValueAtIndex(components, 0);
    if (!value || CFGetTypeID(value) != CFDictionaryGetTypeID()) fail("invalid AudioComponent");
    auto component = static_cast<CFDictionaryRef>(value);
    AudioComponentDescription description{fourcc(component, CFSTR("type")),
        fourcc(component, CFSTR("subtype")), fourcc(component, CFSTR("manufacturer")), 0, 0};
    auto symbol = static_cast<CFStringRef>(field(component, CFSTR("factoryFunction"), CFStringGetTypeID()));
    auto factory = reinterpret_cast<AudioComponentFactoryFunction>(CFBundleGetFunctionPointerForName(bundle, symbol));
    UInt32 version{};
    auto number = static_cast<CFNumberRef>(field(component, CFSTR("version"), CFNumberGetTypeID()));
    if (!CFNumberGetValue(number, kCFNumberSInt32Type, &version)) fail("invalid version");
    auto name = static_cast<CFStringRef>(field(component, CFSTR("name"), CFStringGetTypeID()));
    if (!factory || !AudioComponentRegister(&description, name, version, factory)) fail("factory registration failed");
#if defined(__aarch64__)
    constexpr const char* architecture = "arm64";
#else
    constexpr const char* architecture = "x86_64";
#endif
    std::fprintf(stderr, "RAVET process-local AU registration (%s): %s\n", architecture, path);
    // Retain the bundle and its factory until the private validator exits.
}
}
