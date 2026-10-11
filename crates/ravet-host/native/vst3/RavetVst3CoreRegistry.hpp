#ifndef RAVET_VST3_CORE_REGISTRY_HPP
#define RAVET_VST3_CORE_REGISTRY_HPP
#include <array>
#include <memory>
#include <mutex>
#include <stdexcept>
#include <string>

namespace ravet::vst3::draft {
// CONTROL-only owner bookkeeping. Core owns native module, factory and host.
// No native methods or last-reference destruction occur under this mutex.
// Production process-retention storage must itself survive until OS image exit;
// ordinary stack/static destruction of this draft registry is NOT that policy.
template<class Core, std::size_t Capacity = 256> class CoreRegistry {
    static_assert(Capacity > 0 && Capacity <= 256);
    struct Slot {
        const void* identity = nullptr;
        Core* active = nullptr;
        std::shared_ptr<Core> pin;
        bool retiring = false;
    };
    std::mutex mutex_;
    std::array<Slot, Capacity> slots_{};
public:
    enum class Result { Accepted, Busy, CapacityFull, Invalid, NotFound };
    CoreRegistry() = default;
    CoreRegistry(const CoreRegistry&) = delete;
    CoreRegistry& operator=(const CoreRegistry&) = delete;

    bool activeReservation(const void* identity,const std::shared_ptr<Core>& core) {
        if(!identity||!core)return false;
        std::lock_guard<std::mutex> lock(mutex_);
        for(const auto& slot:slots_)if(slot.identity==identity)
            return slot.active==core.get()&&!slot.retiring&&(!slot.pin||slot.pin.get()==core.get());
        return false;
    }

    Result reserve(const void* identity, const std::shared_ptr<Core>& core) {
        if (!identity || !core) return Result::Invalid;
        std::lock_guard<std::mutex> lock(mutex_);
        Slot* empty = nullptr;
        for (auto& slot : slots_) {
            if (slot.identity == identity) return Result::Busy;
            if (!slot.identity && !empty) empty = &slot;
        }
        if (!empty) return Result::CapacityFull;
        empty->identity = identity;
        empty->active = core.get();
        return Result::Accepted;
    }
    // Caller must invoke before the first foreign createInstance, even if that
    // call later fails. Capacity was reserved before any context mutation.
    Result pin(const void* identity, const std::shared_ptr<Core>& core) {
        if (!identity || !core) return Result::Invalid;
        std::lock_guard<std::mutex> lock(mutex_);
        for (auto& slot : slots_) if (slot.identity == identity) {
            if (slot.active != core.get() || slot.retiring) return Result::Invalid;
            if (slot.pin && slot.pin.get() != core.get()) return Result::Invalid;
            if (!slot.pin) slot.pin = core;
            return Result::Accepted;
        }
        return Result::Invalid;
    }
    // The returned strong owner is installed only after the lock is released.
    // A v1 caller never uses this method: reserve() keeps pinned IDs busy.
    Result reopen(const void* identity, std::shared_ptr<Core>& output) {
        if (!identity || output) return Result::Invalid;
        std::shared_ptr<Core> candidate;
        Result result = Result::Invalid;
        {
            std::lock_guard<std::mutex> lock(mutex_);
            for (auto& slot : slots_) if (slot.identity == identity) {
                if (slot.active) return Result::Busy;
                if (!slot.pin) return Result::Invalid;
                candidate = slot.pin;
                slot.active = candidate.get();
                result = Result::Accepted;
                break;
            }
        }
        if (result == Result::Accepted) output = std::move(candidate);
        return result;
    }
    // CONTROL filesystem canonicalization occurs before this lookup. Core's
    // identityKey() is a pure immutable private string accessor, never an SDK
    // or consumer callback. Known path/symlink aliases need no repeated foreign
    // moduleEntry/moduleExit to discover the already owned singleton factory.
    Result reopenKey(const std::string& key, bool allowPinned, std::shared_ptr<Core>& output) {
        if (key.empty() || output) return Result::Invalid;
        std::shared_ptr<Core> candidate;
        {
            std::lock_guard<std::mutex> lock(mutex_);
            for (auto& slot : slots_) {
                const auto* core = slot.active ? slot.active : slot.pin.get();
                if (!core || core->identityKey() != key) continue;
                if (slot.active || slot.retiring || !allowPinned) return Result::Busy;
                if (!slot.pin) return Result::Invalid;
                candidate = slot.pin;
                slot.active = candidate.get();
                break;
            }
        }
        if (!candidate) return Result::NotFound;
        output = std::move(candidate);
        return Result::Accepted;
    }
    // Reserve identity THROUGH foreign cleanup: a reentrant open must not
    // replace the context during factory release or module termination.
    // Caller retains core and, for an ordinary owner only, performs checked
    // native teardown between beginRetire and finishRetire, outside the lock.
    Result beginRetire(const void* identity, const std::shared_ptr<Core>& core, bool& pinned) {
        return beginRetire(identity, core.get(), pinned);
    }
    // Private native-core destructor may use its own still-live address after
    // its last shared owner is released. No new shared owner is fabricated.
    Result beginRetire(const void* identity, Core* core, bool& pinned) {
        if (!identity || !core) return Result::Invalid;
        std::lock_guard<std::mutex> lock(mutex_);
        for (auto& slot : slots_) if (slot.identity == identity) {
            if (slot.active != core || slot.retiring) return Result::Invalid;
            pinned = bool(slot.pin);
            slot.retiring = true;
            return Result::Accepted;
        }
        return Result::Invalid;
    }
    Result finishRetire(const void* identity, const std::shared_ptr<Core>& core) {
        return finishRetire(identity, core.get());
    }
    Result finishRetire(const void* identity, Core* core) {
        if (!identity || !core) return Result::Invalid;
        std::lock_guard<std::mutex> lock(mutex_);
        for (auto& slot : slots_) if (slot.identity == identity) {
            if (slot.active != core || !slot.retiring) return Result::Invalid;
            slot.active = nullptr;
            slot.retiring = false;
            if (!slot.pin) slot.identity = nullptr;
            return Result::Accepted;
        }
        return Result::Invalid;
    }
};
}
#endif
