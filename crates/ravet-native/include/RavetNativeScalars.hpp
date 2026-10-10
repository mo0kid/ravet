#pragma once
// IEEE classification of referenced foreign memory remains valid under fast-math.
#include <cstdint>
#include <cstring>
#include <limits>
namespace ravet::native::scalar {
static_assert(sizeof(float) == 4 && sizeof(double) == 8 &&
              std::numeric_limits<float>::is_iec559 && std::numeric_limits<double>::is_iec559,
              "Native scalar validation requires IEEE binary32/binary64");
inline bool finite(const double &value) {
    uint64_t bits = 0; std::memcpy(&bits, &value, sizeof(bits));
    return (bits & UINT64_C(0x7ff0000000000000)) != UINT64_C(0x7ff0000000000000);
}
inline bool finite(const float &value) {
    uint32_t bits = 0; std::memcpy(&bits, &value, sizeof(bits));
    return (bits & UINT32_C(0x7f800000)) != UINT32_C(0x7f800000);
}
} // namespace ravet::native::scalar
