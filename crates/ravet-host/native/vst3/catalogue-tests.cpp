#include "RavetVst3Catalogue.hpp"
#include <iostream>

using namespace Steinberg;
using namespace ravet::vst3;
static void require(bool condition) { if (!condition) throw std::runtime_error("contract failed"); }
struct Factory final : IPluginFactory3 {
    int count = 2, enumerated = 0, narrow = 0, wide = 0;
    int fault = 0;
    tresult failure = kInternalError;
    tresult PLUGIN_API queryInterface(const TUID, void** object) override { *object = nullptr; return kNoInterface; }
    uint32 PLUGIN_API addRef() override { return 1; }
    uint32 PLUGIN_API release() override { return 1; }
    int32 PLUGIN_API countClasses() override { return count; }
    tresult PLUGIN_API getFactoryInfo(PFactoryInfo* info) override {
        *info = {}; std::strcpy(info->vendor, "RAVET");
        std::strcpy(info->url, "https://example.invalid/");
        info->flags = PFactoryInfo::kClassesDiscardable;
        if (fault == 1) return failure;
        if (fault == 2) std::memset(info->vendor, 'x', sizeof(info->vendor));
        return kResultOk;
    }
    tresult PLUGIN_API getClassInfo(int32 index, PClassInfo* info) override {
        ++enumerated; *info = {}; info->cid[0] = static_cast<char>(index + 1);
        info->cid[1] = static_cast<char>((index + 1) >> 8);
        info->cardinality = PClassInfo::kManyInstances;
        std::strcpy(info->category, "Audio Module Class");
        std::strcpy(info->name, index ? "second" : "first");
        if (index == 1 && fault == 3) info->cid[0] = 1;
        if (index == 1 && fault == 4) info->cid[0] = 0;
        if (index == 1 && fault == 5) std::memset(info->name, 'x', sizeof(info->name));
        if (index == 1 && fault == 6) { info->name[0] = static_cast<char>(0xc0); info->name[1] = static_cast<char>(0x80); }
        if (index == 1 && fault == 7) return failure;
        if (index == 1 && fault == 8) info->cardinality = 0;
        return kResultOk;
    }
    tresult PLUGIN_API getClassInfo2(int32 index, PClassInfo2* info) override {
        ++narrow; PClassInfo base{}; const auto result = getClassInfo(index, &base);
        *info = {}; std::memcpy(info->cid, base.cid, sizeof(base.cid));
        info->cardinality = base.cardinality; std::memcpy(info->category, base.category, sizeof(base.category));
        std::memcpy(info->name, base.name, sizeof(base.name));
        info->classFlags = 0x1234; std::strcpy(info->subCategories, "Fx|Dynamics");
        std::strcpy(info->version, "1.2.3"); std::strcpy(info->sdkVersion, "3.8.1");
        if (index) std::strcpy(info->vendor, "Second vendor");
        return result;
    }
    tresult PLUGIN_API getClassInfoUnicode(int32 index, PClassInfoW* info) override {
        ++wide; *info = {}; info->cid[0] = static_cast<char>(index + 1);
        info->cardinality = PClassInfo::kManyInstances; std::strcpy(info->category, "Audio Module Class");
        info->name[0] = 0x03b1; info->name[1] = 0xd83c; info->name[2] = 0xdfb9;
        info->version[0] = '1'; info->classFlags = 7;
        std::strcpy(info->subCategories, "Instrument|Synth");
        if (index == 1 && fault == 9) info->name[2] = 'x';
        if (index == 1 && fault == 10) info->name[0] = 0xdc00;
        if (index == 1 && fault == 11) std::fill(std::begin(info->vendor), std::end(info->vendor), 'x');
        if (index == 1 && fault == 12) return failure;
        return kResultOk;
    }
    tresult PLUGIN_API createInstance(FIDString, FIDString, void** object) override { *object = nullptr; return kNotImplemented; }
    tresult PLUGIN_API setHostContext(FUnknown*) override { return kResultOk; }
};
static void rejects(Factory& factory, int version, Error expected) {
    bool rejected = false;
    try { (void)inspect(factory, version == 2 ? &factory : nullptr, version == 3 ? &factory : nullptr); }
    catch (const Failure& failure) {
        rejected = true; require(failure.error == expected);
        if (expected == Error::Native) require(failure.native == factory.failure);
    }
    require(rejected);
}
int main() {
    try {
        for (const int count : {-1, 0, 257}) {
            Factory factory; factory.count = count; rejects(factory, 1, Error::Capacity); require(factory.enumerated == 0);
        }
        Factory maximum; maximum.count = 256;
        require(inspect(maximum, nullptr, nullptr).classes.size() == 256);
        require(maximum.enumerated == 256);
        Factory plain; auto catalogue = inspect(plain, nullptr, nullptr);
        require(catalogue.classes.size() == 2 && catalogue.classes[0].vendorFromFactory && catalogue.classes[1].name == "second");
        require(catalogue.vendor == "RAVET" && catalogue.url == "https://example.invalid/" && catalogue.flags == PFactoryInfo::kClassesDiscardable);
        Factory extendedFactory; catalogue = inspect(extendedFactory, &extendedFactory, nullptr);
        require(catalogue.classes[0].flags == 0x1234 && catalogue.classes[0].subcategories == "Fx|Dynamics");
        require(catalogue.classes[0].version == "1.2.3" && catalogue.classes[0].sdkVersion == "3.8.1");
        require(catalogue.classes[0].vendorFromFactory && !catalogue.classes[1].vendorFromFactory && catalogue.classes[1].vendor == "Second vendor");
        Factory unicode; catalogue = inspect(unicode, &unicode, &unicode);
        require(unicode.narrow == 0 && unicode.wide == 2 && catalogue.classes[0].name == "\xce\xb1\xf0\x9f\x8e\xb9");
        require(catalogue.classes[0].subcategories == "Instrument|Synth" && catalogue.classes[0].flags == 7);
        for (int fault = 1; fault <= 8; ++fault) {
            Factory factory; factory.fault = fault;
            rejects(factory, 1, fault == 1 || fault == 7 ? Error::Native : Error::Metadata);
            require(factory.enumerated == (fault <= 2 ? 0 : 2));
        }
        for (int fault = 9; fault <= 12; ++fault) {
            Factory factory; factory.fault = fault;
            rejects(factory, 3, fault == 12 ? Error::Native : Error::Metadata);
            require(factory.wide == 2 && factory.narrow == 0 && factory.enumerated == 0);
        }
        for (const std::string invalid : {"\xc0\x80", "\xed\xa0\x80", "\xf4\x90\x80\x80", "\xe2\x82", "\x80"}) require(!utf8(invalid));
        require(utf8("\x7f\xc2\x80\xe0\xa0\x80\xf0\x90\x80\x80\xf4\x8f\xbf\xbf"));
        std::cout << "PASS bounded raw SDK factory catalogue, narrow/unicode, late metadata faults\n";
    } catch (const std::exception& error) { std::cerr << error.what() << '\n'; return 1; }
}
