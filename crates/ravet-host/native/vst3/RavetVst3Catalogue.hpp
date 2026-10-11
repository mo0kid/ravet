#pragma once
#include "pluginterfaces/base/ipluginbase.h"
#include <algorithm>
#include <array>
#include <cstdint>
#include <cstring>
#include <stdexcept>
#include <string>
#include <vector>

namespace ravet::vst3 {
enum class Error { Capacity, Metadata, Native };
struct Failure { Error error; Steinberg::tresult native = Steinberg::kResultOk; };
struct Class {
    std::array<std::uint8_t, 16> id{};
    std::int32_t cardinality{};
    std::uint32_t flags{};
    std::string category, name, vendor, version, sdkVersion, subcategories;
    bool vendorFromFactory{};
};
struct Catalogue {
    std::string vendor, url, email;
    std::int32_t flags{};
    std::vector<Class> classes;
};
inline void check(Steinberg::tresult result) {
    if (result != Steinberg::kResultOk) throw Failure{Error::Native, result};
}
inline bool utf8(const std::string& value) {
    for (std::size_t i = 0; i < value.size();) {
        const auto first = static_cast<unsigned char>(value[i++]);
        if (first < 0x80) continue;
        unsigned int count, scalar, minimum;
        if (first >= 0xc2 && first <= 0xdf) { count = 1; scalar = first & 0x1f; minimum = 0x80; }
        else if (first >= 0xe0 && first <= 0xef) { count = 2; scalar = first & 0x0f; minimum = 0x800; }
        else if (first >= 0xf0 && first <= 0xf4) { count = 3; scalar = first & 7; minimum = 0x10000; }
        else return false;
        if (count > value.size() - i) return false;
        while (count--) {
            const auto next = static_cast<unsigned char>(value[i++]);
            if ((next & 0xc0) != 0x80) return false;
            scalar = (scalar << 6) | (next & 0x3f);
        }
        if (scalar < minimum || scalar > 0x10ffff || (scalar >= 0xd800 && scalar <= 0xdfff)) return false;
    }
    return true;
}
template<std::size_t N> std::string text(const char (&value)[N], bool required = false) {
    const auto end = std::find(std::begin(value), std::end(value), '\0');
    if (end == std::end(value) || (required && end == std::begin(value))) throw Failure{Error::Metadata};
    std::string result(value, end);
    if (!utf8(result)) throw Failure{Error::Metadata};
    return result;
}
inline void appendScalar(std::string& output, std::uint32_t scalar) {
    if (scalar < 0x80) output.push_back(static_cast<char>(scalar));
    else if (scalar < 0x800) {
        output.push_back(static_cast<char>(0xc0 | (scalar >> 6)));
        output.push_back(static_cast<char>(0x80 | (scalar & 0x3f)));
    } else if (scalar < 0x10000) {
        output.push_back(static_cast<char>(0xe0 | (scalar >> 12)));
        output.push_back(static_cast<char>(0x80 | ((scalar >> 6) & 0x3f)));
        output.push_back(static_cast<char>(0x80 | (scalar & 0x3f)));
    } else {
        output.push_back(static_cast<char>(0xf0 | (scalar >> 18)));
        output.push_back(static_cast<char>(0x80 | ((scalar >> 12) & 0x3f)));
        output.push_back(static_cast<char>(0x80 | ((scalar >> 6) & 0x3f)));
        output.push_back(static_cast<char>(0x80 | (scalar & 0x3f)));
    }
}
template<std::size_t N> std::string text(const Steinberg::char16 (&value)[N], bool required = false) {
    const auto end = std::find(std::begin(value), std::end(value), 0);
    if (end == std::end(value) || (required && end == std::begin(value))) throw Failure{Error::Metadata};
    std::string result;
    result.reserve((N - 1) * 3);
    for (auto at = std::begin(value); at != end; ++at) {
        std::uint32_t scalar = static_cast<std::uint16_t>(*at);
        if (scalar >= 0xd800 && scalar <= 0xdbff) {
            if (++at == end) throw Failure{Error::Metadata};
            const auto low = static_cast<std::uint16_t>(*at);
            if (low < 0xdc00 || low > 0xdfff) throw Failure{Error::Metadata};
            scalar = 0x10000 + ((scalar - 0xd800) << 10) + low - 0xdc00;
        } else if (scalar >= 0xdc00 && scalar <= 0xdfff) throw Failure{Error::Metadata};
        appendScalar(result, scalar);
    }
    return result;
}
template<class Info> Class basic(const Info& info) {
    Class result;
    std::memcpy(result.id.data(), info.cid, result.id.size());
    if (std::all_of(result.id.begin(), result.id.end(), [](auto byte) { return byte == 0; }) || info.cardinality <= 0)
        throw Failure{Error::Metadata};
    result.cardinality = info.cardinality;
    result.category = text(info.category, true);
    result.name = text(info.name, true);
    return result;
}
template<class Info> Class extended(const Info& info) {
    auto result = basic(info);
    result.flags = info.classFlags;
    result.vendor = text(info.vendor);
    result.version = text(info.version);
    result.sdkVersion = text(info.sdkVersion);
    result.subcategories = text(info.subCategories);
    return result;
}
// Caller owns compliant factory/interface references through this synchronous
// off-audio operation. QueryInterface/thread policy belongs to the owner.
inline Catalogue inspect(Steinberg::IPluginFactory& factory,
                         Steinberg::IPluginFactory2* version2,
                         Steinberg::IPluginFactory3* version3) {
    const auto count = factory.countClasses();
    if (count < 1 || count > 256) throw Failure{Error::Capacity};
    Steinberg::PFactoryInfo info{};
    check(factory.getFactoryInfo(&info));
    Catalogue candidate;
    candidate.vendor = text(info.vendor, true);
    candidate.url = text(info.url);
    candidate.email = text(info.email);
    candidate.flags = info.flags;
    candidate.classes.reserve(static_cast<std::size_t>(count));
    for (Steinberg::int32 index = 0; index < count; ++index) {
        Class item;
        if (version3) {
            Steinberg::PClassInfoW record{};
            check(version3->getClassInfoUnicode(index, &record));
            item = extended(record);
        } else if (version2) {
            Steinberg::PClassInfo2 record{};
            check(version2->getClassInfo2(index, &record));
            item = extended(record);
        } else {
            Steinberg::PClassInfo record{};
            check(factory.getClassInfo(index, &record));
            item = basic(record);
        }
        if (std::any_of(candidate.classes.begin(), candidate.classes.end(), [&](const Class& prior) { return prior.id == item.id; }))
            throw Failure{Error::Metadata};
        if (item.vendor.empty()) { item.vendor = candidate.vendor; item.vendorFromFactory = true; }
        candidate.classes.push_back(std::move(item));
    }
    return candidate;
}
} // namespace ravet::vst3
