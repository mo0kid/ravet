#pragma once
// RAVET-owned optional external-SDK audio parameter input helpers. No SDK source
// is vendored. Construct/retire cached profiles on CONTROL after callbacks stop;
// keep the exact native cache/table/library and borrowed host queues alive.
#include "RavetCachedParameters.h"
#include "RavetNativeScalars.hpp"
#include "pluginterfaces/vst/ivstparameterchanges.h"
#include <algorithm>
#include <array>
#include <cstring>
#include <limits>

namespace ravet::vst3 {
namespace input_detail {
using ::ravet::native::scalar::finite;
struct Target { uint32_t id, flags; };
} // namespace input_detail

enum class ParameterInputResult { Value, Unknown, Rejected };
// Immutable prepared native cache and sorted target roles. <=1024 targets,
// VST3-reserved all-ones ID rejected. No copy/move prevents hidden ownership
// retirement; caller explicitly stops borrowers before destruction. Cached reads
// use no providers, framework heap actions, locks or I/O. Host/consumer callback
// behavior and native deadlines remain separate obligations.
class CachedParameterInput final {
public:
    explicit CachedParameterInput(const RavetNativeCachedParameterEntryV1 *table) : table_(table) {
        if (!table || table->version != 1 || table->byte_size != sizeof(*table) ||
            (table->sample_bits != 32 && table->sample_bits != 64) || table->parameter_count > 1024 ||
            !table->create || !table->destroy || !table->info || !table->denormalize) return;
        cache_ = table->create(); if (!cache_) return;
        count_ = table->parameter_count;
        for (uint32_t index = 0; index < count_; ++index) {
            RavetNativeParameterInfoV1 info{};
            if (table->info(cache_, index, &info) != 0 || info.id == Steinberg::Vst::kNoParamId || (info.flags & ~7u)) { retire(); return; }
            targets_[index] = {info.id, info.flags};
        }
        std::sort(targets_.begin(), targets_.begin() + count_, [](const auto &left, const auto &right) { return left.id < right.id; });
        for (uint32_t index = 1; index < count_; ++index) if (targets_[index - 1].id == targets_[index].id) { retire(); return; }
    }
    ~CachedParameterInput() { retire(); }
    CachedParameterInput(const CachedParameterInput &) = delete;
    CachedParameterInput &operator=(const CachedParameterInput &) = delete;
    CachedParameterInput(CachedParameterInput &&) = delete;
    CachedParameterInput &operator=(CachedParameterInput &&) = delete;
    bool valid() const { return cache_ != nullptr; }
    uint32_t sampleBits() const { return table_ ? table_->sample_bits : 0; }
    // Explicit read-only policy; false rejects declared read-only input. No
    // implicit nonautomatable filtering. Unknown IDs remain distinguishable so
    // consumers choose routing/filtering after full raw host-point validation.
    // Finite normalized f64 0..1 intent narrows explicitly to global f32 mapping.
    // Failed/unknown mappings preserve physical output.
    ParameterInputResult map(uint32_t id, const double &normalized, float &physical, bool allowReadOnly) const {
        if (!valid() || !input_detail::finite(normalized) || normalized < 0. || normalized > 1.) return ParameterInputResult::Rejected;
        const auto end = targets_.begin() + count_;
        const auto target = std::lower_bound(targets_.begin(), end, id, [](const auto &target, uint32_t id) { return target.id < id; });
        if (target == end || target->id != id) return ParameterInputResult::Unknown;
        if (!allowReadOnly && (target->flags & RAVET_PARAMETER_READ_ONLY)) return ParameterInputResult::Rejected;
        float candidate = 0.f;
        if (table_->denormalize(cache_, id, float(normalized), &candidate) != 0 || !input_detail::finite(candidate)) return ParameterInputResult::Rejected;
        physical = candidate; return ParameterInputResult::Value;
    }
private:
    const RavetNativeCachedParameterEntryV1 *table_ = nullptr;
    void *cache_ = nullptr;
    uint32_t count_ = 0;
    std::array<input_detail::Target, 1024> targets_{};
    void retire() { if (cache_) table_->destroy(cache_); cache_ = nullptr; count_ = 0; }
};

struct GlobalParameterPoint { uint32_t offset, id; double normalized; };
// Validate/copy ALL raw global points before calling a prepared visitor, including
// unknown IDs. <=1024 queues/total points and <=65536 frames; zero-frame offsets
// must be zero. SDK success must initialize outputs; queues remain borrowed and
// serialized. One native getPoint read per point, complete validation before any
// visitor side effect. Stack scratch <=16KiB; no sort/heap. Queue traversal and
// equal-offset order are retained, not globally sorted across queues.
// Visitor can route/filter/expand into private prepared scratch; false may retain
// prior visitor side effects, so defer DSP/state/output publication until success.
// Visitor/host calls must obey consumer audio constraints. This helper supplies
// no automatic gestures, scoped events, merge, SDK output or host scheduling.
template <class Visitor>
bool visitValidatedGlobalParameters(Steinberg::Vst::IParameterChanges *changes,
                                    Steinberg::int32 frames, Visitor &&visitor) {
    if (frames < 0 || frames > 65536) return false;
    if (!changes) return true;
    const auto queues = changes->getParameterCount(); if (queues < 0 || queues > 1024) return false;
    std::array<GlobalParameterPoint, 1024> points;
    size_t used = 0;
    for (Steinberg::int32 index = 0; index < queues; ++index) {
        auto *queue = changes->getParameterData(index); if (!queue) return false;
        const auto count = queue->getPointCount(); if (count < 0 || size_t(count) > points.size() - used) return false;
        const auto id = queue->getParameterId(); Steinberg::int32 previous = 0;
        for (Steinberg::int32 point = 0; point < count; ++point) {
            Steinberg::int32 offset = -1; double value = -1.;
            if (queue->getPoint(point, offset, value) != Steinberg::kResultOk || offset < previous || offset < 0 ||
                (frames == 0 ? offset != 0 : offset >= frames) || !input_detail::finite(value) || value < 0. || value > 1.) return false;
            previous = offset; points[used++] = {uint32_t(offset), id, value};
        }
    }
    for (size_t index = 0; index < used; ++index) if (!visitor(points[index])) return false;
    return true;
}
} // namespace ravet::vst3
