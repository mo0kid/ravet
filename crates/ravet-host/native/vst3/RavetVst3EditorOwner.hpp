#ifndef RAVET_VST3_EDITOR_OWNER_HPP
#define RAVET_VST3_EDITOR_OWNER_HPP
// Internal MAIN-only owner of one IPlugView created by an edit controller.
// SDK interfaces only: no Cocoa, provider or Rust dependency. The caller keeps
// the edit controller, component and module alive until retire() returns.
#include "RavetVst3Editor.h"
#include "pluginterfaces/gui/iplugview.h"
#include "pluginterfaces/gui/iplugviewcontentscalesupport.h"
#include "pluginterfaces/vst/ivsteditcontroller.h"
#include <pthread.h>
#include <algorithm>
#include <atomic>
#include <cmath>
#include <cstdint>
#include <cstring>
#include <memory>
#include <new>
#include <optional>

namespace ravet::vst3::editor_draft {
using Steinberg::tresult;
enum class Stage : uint32_t { Admission = 200, CreateView, Platform, SetFrame, GetSize, Attached, CanResize,
    Constraint, OnSize, Scale, Removed, ClearFrame, Release, Query };
enum class Failure : uint32_t { None, Local, Returned, Thrown, Capacity, Allocation, Malformed, Unavailable,
    Unsupported, Reentry };
struct Outcome {
    Stage stage = Stage::Admission;
    Failure failure = Failure::None;
    tresult native = Steinberg::kResultOk;
    uint32_t flags = 0;
    int32_t status = RAVET_VST3_OK;
    bool ok() const noexcept { return failure == Failure::None; }
};
struct Extent {
    uint32_t width = 0, height = 0;
    bool operator==(const Extent& other) const noexcept { return width == other.width && height == other.height; }
};
inline bool bounded(int64_t width, int64_t height) noexcept {
    return width >= RAVET_VST3_EDITOR_MINIMUM_EXTENT && width <= RAVET_VST3_EDITOR_MAXIMUM_EXTENT &&
           height >= RAVET_VST3_EDITOR_MINIMUM_EXTENT && height <= RAVET_VST3_EDITOR_MAXIMUM_EXTENT;
}
inline bool extentOf(const Steinberg::ViewRect& rect, Extent& output) noexcept {
    const int64_t width = int64_t(rect.right) - rect.left, height = int64_t(rect.bottom) - rect.top;
    if (!bounded(width, height)) return false;
    output = {uint32_t(width), uint32_t(height)};
    return true;
}
inline Steinberg::ViewRect rectOf(Extent extent) noexcept {
    return Steinberg::ViewRect(0, 0, int32_t(extent.width), int32_t(extent.height));
}
inline void saturating(uint32_t& value) noexcept { if (value != UINT32_MAX) ++value; }
inline Outcome local(Stage stage, Failure failure, int32_t status) noexcept { return {stage, failure, 0, 0, status}; }
inline Outcome returned(Stage stage, tresult native) noexcept {
    return {stage, Failure::Returned, native, RAVET_VST3_EDITOR_RETURNED, RAVET_VST3_NATIVE};
}
inline Outcome thrown(Stage stage) noexcept { return {stage, Failure::Thrown, 0, RAVET_VST3_EDITOR_THROWN, RAVET_VST3_NATIVE}; }
inline bool mainThread() noexcept { return pthread_main_np() != 0; }
// Bit test, so fast-math translation units cannot fold the NaN/infinity check.
inline bool finite(float value) noexcept {
    uint32_t bits = 0;
    std::memcpy(&bits, &value, sizeof(bits));
    return (bits & 0x7f800000u) != 0x7f800000u;
}

class Owner;
// Host frame. The host keeps one reference; the plug-in may keep more. Deleted
// only on the final release, which may happen on any thread after detach().
class Frame final : public Steinberg::IPlugFrame {
public:
    explicit Frame(Owner* owner) noexcept : owner_(owner) { live().fetch_add(1, std::memory_order_relaxed); }
    Frame(const Frame&) = delete;
    Frame& operator=(const Frame&) = delete;
    tresult PLUGIN_API queryInterface(const Steinberg::TUID id, void** output) override {
        if (!output) return Steinberg::kInvalidArgument;
        *output = nullptr;
        if (Steinberg::FUnknownPrivate::iidEqual(id, Steinberg::FUnknown::iid) ||
            Steinberg::FUnknownPrivate::iidEqual(id, Steinberg::IPlugFrame::iid)) {
            *output = static_cast<Steinberg::IPlugFrame*>(this);
            addRef();
            return Steinberg::kResultOk;
        }
        return Steinberg::kNoInterface;
    }
    Steinberg::uint32 PLUGIN_API addRef() override { return refs_.fetch_add(1, std::memory_order_relaxed) + 1; }
    Steinberg::uint32 PLUGIN_API release() override {
        const auto remaining = refs_.fetch_sub(1, std::memory_order_acq_rel) - 1;
        if (!remaining) delete this;
        return remaining;
    }
    tresult PLUGIN_API resizeView(Steinberg::IPlugView* view, Steinberg::ViewRect* rect) override;
    // MAIN only. After detach no owner state is read, including by later calls.
    void detach() noexcept { owner_ = nullptr; }
    uint32_t offMain() const noexcept { return offMain_.load(std::memory_order_relaxed); }
    uint32_t detachedCalls() const noexcept { return detached_.load(std::memory_order_relaxed); }
    // Test observability: frames constructed and not yet finally released.
    static std::atomic<int64_t>& live() noexcept { static std::atomic<int64_t> count{0}; return count; }
private:
    ~Frame() { live().fetch_sub(1, std::memory_order_relaxed); }
    static void bump(std::atomic<uint32_t>& value) noexcept {
        auto current = value.load(std::memory_order_relaxed);
        while (current != UINT32_MAX && !value.compare_exchange_weak(current, current + 1, std::memory_order_relaxed)) {}
    }
    std::atomic<uint32_t> refs_{1};
    Owner* owner_; // read and written only on MAIN
    std::atomic<uint32_t> offMain_{0}, detached_{0};
};

class Owner {
public:
    enum class Call { None, Attach, Scale, Other, FrameSize };
    Owner(const Owner&) = delete;
    Owner& operator=(const Owner&) = delete;
    // Creation: createView, optional scale query, NSView check, setFrame,
    // canResize and bounded getSize. Any failure retires in order, returns null.
    static std::unique_ptr<Owner> create(Steinberg::Vst::IEditController& controller, Extent& initial,
            Outcome& outcome) noexcept {
        outcome = {};
        if (!mainThread()) { outcome = local(Stage::Admission, Failure::Local, RAVET_VST3_THREAD); return nullptr; }
        std::unique_ptr<Owner> owner;
        try { owner.reset(new Owner); owner->frame_ = new Frame(owner.get()); }
        catch (...) { outcome = local(Stage::Admission, Failure::Allocation, RAVET_VST3_ALLOCATION); return nullptr; }
        Steinberg::IPlugView* raw = nullptr;
        bool threw = false;
        {
            Scope scope(*owner, Call::Other);
            try { raw = controller.createView(Steinberg::Vst::ViewType::kEditor); }
            catch (...) { threw = true; }
        }
        if (threw) { outcome = thrown(Stage::CreateView); owner->quiet(); return nullptr; }
        if (!raw) {
            outcome = local(Stage::CreateView, Failure::Unavailable, RAVET_VST3_EDITOR_UNAVAILABLE);
            owner->quiet();
            return nullptr;
        }
        owner->view_ = raw; // createView returns an owned reference
        if (!owner->querySupport(outcome) || !owner->negotiate(initial, outcome)) { owner->quiet(); return nullptr; }
        outcome = {Stage::GetSize, Failure::None, 0, 0, RAVET_VST3_OK};
        return owner;
    }
    // Callers retire explicitly (the provider checks busy() first). Destroying
    // an unretired owner off MAIN, or from inside one of its own view callbacks
    // where retire() is refused, would leave the frame pointing at freed state:
    // that invariant breach terminates deterministically instead.
    ~Owner() {
        if (retired_) return;
        if (!mainThread()) std::terminate(); // never run SDK UI teardown off MAIN
        (void)retire();
        if (!retired_) std::terminate();
    }
    Outcome attach(void* parent, Extent& applied) noexcept {
        if (auto refused = admit()) return *refused;
        if (!parent) return finish(local(Stage::Attached, Failure::Local, RAVET_VST3_INVALID));
        if (attached_) return finish(local(Stage::Attached, Failure::Reentry, RAVET_VST3_LIFECYCLE));
        tresult status = Steinberg::kInternalError;
        {
            Scope scope(*this, Call::Attach);
            try { status = view_->attached(parent, Steinberg::kPlatformTypeNSView); }
            catch (...) {
                // Plug-in effects are unknown: removal stays a teardown duty.
                attached_ = true; fault_ = true; discardDeferred();
                return finish(thrown(Stage::Attached));
            }
        }
        // A refused attach keeps an acknowledged deferred request pending: it is
        // applied after the next successful attach (never while detached).
        if (status != Steinberg::kResultOk) return finish(returned(Stage::Attached, status));
        attached_ = true;
        Outcome outcome{Stage::Attached};
        applyDeferred(outcome);
        applied = current_;
        return finish(outcome);
    }
    Outcome size(Extent& output) noexcept {
        if (auto refused = admit()) return *refused;
        Steinberg::ViewRect rect;
        tresult status = Steinberg::kInternalError;
        {
            Scope scope(*this, Call::Other);
            try { status = view_->getSize(&rect); }
            catch (...) { fault_ = true; return finish(thrown(Stage::GetSize)); }
        }
        if (status != Steinberg::kResultOk) return finish(returned(Stage::GetSize, status));
        Extent extent;
        if (!extentOf(rect, extent)) return finish(local(Stage::GetSize, Failure::Malformed, RAVET_VST3_METADATA));
        current_ = extent;
        output = extent;
        return finish(Outcome{Stage::GetSize});
    }
    // checkSizeConstraint policy: only kResultTrue (== kResultOk, 0) accepts the
    // adjusted rect. kResultFalse, kNotImplemented or any other result is a
    // returned refusal: the SDK defines kResultTrue as "rect is (now) valid", and
    // no unconstrained fallback is applied (mirrors the CLAP adjust_size policy).
    // Host-initiated resize: canResize, checkSizeConstraint then onSize. No
    // unconstrained fallback; only a bounded adjusted extent is applied.
    Outcome resize(Extent proposed, Extent& applied) noexcept {
        if (auto refused = admit()) return *refused;
        if (!bounded(proposed.width, proposed.height)) return finish(local(Stage::Constraint, Failure::Local, RAVET_VST3_INVALID));
        Scope scope(*this, Call::Other);
        tresult status = Steinberg::kInternalError;
        try { status = view_->canResize(); }
        catch (...) { fault_ = true; return finish(thrown(Stage::CanResize)); }
        resizable_ = status == Steinberg::kResultTrue;
        if (!resizable_) return finish(local(Stage::CanResize, Failure::Unsupported, RAVET_VST3_EDITOR_UNSUPPORTED));
        auto rect = rectOf(proposed);
        try { status = view_->checkSizeConstraint(&rect); }
        catch (...) { fault_ = true; return finish(thrown(Stage::Constraint)); }
        if (status != Steinberg::kResultTrue) return finish(returned(Stage::Constraint, status));
        Extent adjusted;
        if (!extentOf(rect, adjusted)) return finish(local(Stage::Constraint, Failure::Malformed, RAVET_VST3_METADATA));
        auto normalized = rectOf(adjusted);
        try { status = view_->onSize(&normalized); }
        catch (...) { fault_ = true; return finish(thrown(Stage::OnSize)); }
        if (status != Steinberg::kResultOk) return finish(returned(Stage::OnSize, status));
        if (!requery(adjusted)) return finish(thrown(Stage::GetSize));
        request_.reset(); // the host now knows its own size; a stale request is superseded
        applied = current_;
        return finish(Outcome{Stage::OnSize});
    }
    Outcome scale(float factor, Extent& applied) noexcept {
        if (auto refused = admit()) return *refused;
        if (!finite(factor) || factor < 0.25f || factor > 8.0f)
            return finish(local(Stage::Scale, Failure::Local, RAVET_VST3_INVALID));
        if (!scale_) return finish(local(Stage::Scale, Failure::Unsupported, RAVET_VST3_EDITOR_UNSUPPORTED));
        tresult status = Steinberg::kInternalError;
        {
            Scope scope(*this, Call::Scale);
            try { status = scale_->setContentScaleFactor(factor); }
            catch (...) { fault_ = true; discardDeferred(); return finish(thrown(Stage::Scale)); }
        }
        // A request acknowledged during the call is honoured whatever the scale
        // result: applied now when attached, otherwise at the next attach.
        Outcome outcome = status == Steinberg::kResultTrue ? Outcome{Stage::Scale} : returned(Stage::Scale, status);
        if (attached_) applyDeferred(outcome);
        applied = current_;
        return finish(outcome);
    }
    Outcome detach() noexcept {
        if (auto refused = admit()) return *refused;
        if (!attached_) return finish(local(Stage::Removed, Failure::Reentry, RAVET_VST3_LIFECYCLE));
        tresult status = Steinberg::kInternalError;
        {
            Scope scope(*this, Call::Other);
            try { status = view_->removed(); }
            catch (...) { attached_ = false; fault_ = true; request_.reset(); return finish(thrown(Stage::Removed)); }
        }
        if (status != Steinberg::kResultOk) return finish(returned(Stage::Removed, status));
        attached_ = false;
        request_.reset(); // no consumer container to resize while detached
        return finish(Outcome{Stage::Removed});
    }
    // Ordered, consuming teardown: removed (if attached), setFrame(nullptr),
    // frame detach, scale/view release, host frame release. First failure wins.
    Outcome retire() noexcept {
        if (!mainThread()) return local(Stage::Admission, Failure::Local, RAVET_VST3_THREAD);
        if (retired_) return local(Stage::Admission, Failure::Reentry, RAVET_VST3_LIFECYCLE);
        if (call_ != Call::None) return local(Stage::Admission, Failure::Reentry, RAVET_VST3_EDITOR_REENTRY);
        retired_ = true;
        Outcome first{Stage::Release};
        {
            Scope scope(*this, Call::Other);
            if (attached_) { attached_ = false; cleanup(Stage::Removed, first, [&] { return view_->removed(); }); }
            if (frameSet_) { frameSet_ = false; cleanup(Stage::ClearFrame, first, [&] { return view_->setFrame(nullptr); }); }
            if (frame_) frame_->detach();
            discardDeferred();
            request_.reset();
            releaseInterface(scale_, first);
            releaseInterface(view_, first);
        }
        if (frame_) { frame_->release(); frame_ = nullptr; }
        if (!first.ok()) first.status = RAVET_VST3_RETIRED_NATIVE;
        first.flags |= RAVET_VST3_EDITOR_STATE; // authoritative: no longer attached
        return first;
    }
    tresult frameResize(Steinberg::IPlugView* view, Steinberg::ViewRect* rect) noexcept {
        saturating(calls_);
        if (retired_ || !view_) { saturating(refused_); return Steinberg::kResultFalse; }
        if (view != view_ || !rect) { saturating(refused_); return Steinberg::kInvalidArgument; }
        Extent extent;
        if (!extentOf(*rect, extent)) { saturating(refused_); return Steinberg::kInvalidArgument; }
        if (fault_) { saturating(refused_); return Steinberg::kResultFalse; }
        switch (call_) {
            case Call::Attach: case Call::Scale:
                // Acknowledged now and honoured later (latest wins; a superseded
                // acknowledged request is counted, never silently lost).
                if (deferred_) saturating(discarded_);
                deferred_ = extent;
                return Steinberg::kResultTrue;
            case Call::None: break;
            default: saturating(reentry_); saturating(refused_); return Steinberg::kResultFalse;
        }
        // Idle requests are applied only to an attached view.
        if (!attached_) { saturating(refused_); return Steinberg::kResultFalse; }
        if (extent == current_) { saturating(applied_); return Steinberg::kResultTrue; }
        if (gate_ && !gate_(gateContext_, true)) { saturating(reentry_); saturating(refused_); return Steinberg::kResultFalse; }
        tresult status = Steinberg::kInternalError;
        bool threw = false;
        {
            Scope scope(*this, Call::FrameSize);
            auto normalized = rectOf(extent);
            try { status = view_->onSize(&normalized); }
            catch (...) { threw = true; }
        }
        bool queried = true;
        if (!threw && status == Steinberg::kResultOk) {
            Scope scope(*this, Call::FrameSize);
            queried = requery(extent);
        }
        if (gate_) gate_(gateContext_, false);
        if (threw || !queried) { fault_ = true; saturating(refused_); return Steinberg::kInternalError; }
        if (status != Steinberg::kResultOk) { saturating(refused_); return Steinberg::kResultFalse; }
        request_ = current_; saturating(applied_);
        return Steinberg::kResultTrue;
    }
    // Optional containing-module gate held around an idle plug-in-initiated
    // onSize (which runs outside any provider entry). enter=false releases.
    using Gate = bool (*)(void* context, bool enter) noexcept;
    void gate(Gate function, void* context) noexcept { gate_ = function; gateContext_ = context; }
    bool busy() const noexcept { return call_ != Call::None; }
    bool attached() const noexcept { return attached_; }
    bool faulted() const noexcept { return fault_; }
    Extent current() const noexcept { return current_; }
    bool takeRequest(Extent& output) noexcept {
        if (!request_) return false;
        output = *request_; request_.reset();
        return true;
    }
    RavetVst3EditorState state() const noexcept {
        RavetVst3EditorState state{};
        state.struct_size = sizeof(state);
        state.flags = (attached_ ? 1u : 0u) | (resizable_ ? 2u : 0u) | (scale_ ? 4u : 0u) |
                      (request_ ? 8u : 0u) | (fault_ ? 16u : 0u) | (deferred_ ? 32u : 0u);
        state.width = current_.width; state.height = current_.height;
        const uint64_t off = frame_ ? frame_->offMain() + uint64_t(frame_->detachedCalls()) : 0;
        auto add = [](uint32_t value, uint64_t extra) { return uint32_t(std::min<uint64_t>(UINT32_MAX, value + extra)); };
        state.frame_calls = add(calls_, off);
        state.frame_applied = applied_;
        state.frame_refused = add(refused_, off);
        state.frame_reentry_refused = reentry_;
        state.deferred_discarded = discarded_;
        state.size_mismatches = mismatches_;
        return state;
    }
private:
    Owner() = default;
    struct Scope {
        Owner& owner; Call previous;
        Scope(Owner& o, Call call) noexcept : owner(o), previous(o.call_) { owner.call_ = call; }
        ~Scope() { owner.call_ = previous; }
    };
    // Refusals on MAIN carry the authoritative attached state (STATE flag), so a
    // refused second attach or a post-fault refusal never reads as "detached".
    // An off-MAIN refusal reads no owner state and reports no STATE.
    std::optional<Outcome> admit() const noexcept {
        if (!mainThread()) return local(Stage::Admission, Failure::Local, RAVET_VST3_THREAD);
        if (call_ != Call::None) return finish(local(Stage::Admission, Failure::Reentry, RAVET_VST3_EDITOR_REENTRY));
        if (retired_ || fault_ || !view_) return finish(local(Stage::Admission, Failure::Reentry, RAVET_VST3_LIFECYCLE));
        return std::nullopt;
    }
    Outcome finish(Outcome outcome) const noexcept {
        outcome.flags |= RAVET_VST3_EDITOR_STATE;
        if (attached_) outcome.flags |= RAVET_VST3_EDITOR_ATTACHED;
        return outcome;
    }
    // After a successful onSize(passed), re-query getSize as the SDK sequence
    // permits and record that size when valid; a differing or failed re-query
    // is counted and the passed (checked) size is kept. A throw faults.
    bool requery(Extent passed) noexcept {
        Steinberg::ViewRect rect;
        tresult status = Steinberg::kInternalError;
        try { status = view_->getSize(&rect); }
        catch (...) { fault_ = true; current_ = passed; return false; }
        Extent queried;
        if (status == Steinberg::kResultOk && extentOf(rect, queried)) {
            if (!(queried == passed)) saturating(mismatches_);
            current_ = queried;
        } else {
            saturating(mismatches_);
            current_ = passed;
        }
        return true;
    }
    void discardDeferred() noexcept { if (deferred_) { saturating(discarded_); deferred_.reset(); } }
    void applyDeferred(Outcome& outcome) noexcept {
        if (!deferred_) return;
        const auto extent = *deferred_;
        deferred_.reset();
        if (extent == current_) { saturating(applied_); return; }
        tresult status = Steinberg::kInternalError;
        {
            Scope scope(*this, Call::FrameSize);
            auto normalized = rectOf(extent);
            try { status = view_->onSize(&normalized); }
            catch (...) { fault_ = true; saturating(refused_); saturating(discarded_); outcome = thrown(Stage::OnSize); return; }
            if (status == Steinberg::kResultOk && !requery(extent)) {
                saturating(refused_); outcome = thrown(Stage::GetSize); return;
            }
        }
        // The plug-in refused its own acknowledged request: counted, not silent.
        if (status != Steinberg::kResultOk) { saturating(refused_); saturating(discarded_); return; }
        request_ = current_; saturating(applied_);
        outcome.flags |= RAVET_VST3_EDITOR_DEFERRED;
    }
    bool querySupport(Outcome& outcome) noexcept {
        Scope scope(*this, Call::Other);
        void* pointer = nullptr;
        tresult status = Steinberg::kInternalError;
        try { status = view_->queryInterface(Steinberg::IPlugViewContentScaleSupport::iid, &pointer); }
        catch (...) {
            auto* candidate = static_cast<Steinberg::IPlugViewContentScaleSupport*>(pointer);
            if (candidate) { try { candidate->release(); } catch (...) {} }
            outcome = thrown(Stage::Query);
            return false;
        }
        auto* candidate = static_cast<Steinberg::IPlugViewContentScaleSupport*>(pointer);
        if (status == Steinberg::kResultOk && candidate) { scale_ = candidate; return true; }
        if (candidate) { // a pointer returned with a failed status is still released
            try { candidate->release(); } catch (...) { outcome = thrown(Stage::Query); return false; }
        }
        return true;
    }
    bool negotiate(Extent& initial, Outcome& outcome) noexcept {
        Scope scope(*this, Call::Other);
        tresult status = Steinberg::kInternalError;
        try { status = view_->isPlatformTypeSupported(Steinberg::kPlatformTypeNSView); }
        catch (...) { outcome = thrown(Stage::Platform); return false; }
        if (status != Steinberg::kResultTrue) {
            outcome = local(Stage::Platform, Failure::Unsupported, RAVET_VST3_EDITOR_UNSUPPORTED);
            return false;
        }
        frameSet_ = true; // a refused or throwing setter may still have stored it
        try { status = view_->setFrame(frame_); }
        catch (...) { outcome = thrown(Stage::SetFrame); return false; }
        if (status != Steinberg::kResultOk) { outcome = returned(Stage::SetFrame, status); return false; }
        try { status = view_->canResize(); }
        catch (...) { outcome = thrown(Stage::CanResize); return false; }
        resizable_ = status == Steinberg::kResultTrue;
        Steinberg::ViewRect rect;
        try { status = view_->getSize(&rect); }
        catch (...) { outcome = thrown(Stage::GetSize); return false; }
        if (status != Steinberg::kResultOk) { outcome = returned(Stage::GetSize, status); return false; }
        if (!extentOf(rect, current_)) {
            outcome = local(Stage::GetSize, Failure::Malformed, RAVET_VST3_METADATA);
            return false;
        }
        initial = current_;
        return true;
    }
    void quiet() noexcept { (void)retire(); }
    template<class Call> void cleanup(Stage stage, Outcome& first, Call call) noexcept {
        tresult status = Steinberg::kInternalError;
        try { status = call(); }
        catch (...) { if (first.ok()) first = thrown(stage); return; }
        if (first.ok() && status != Steinberg::kResultOk) first = returned(stage, status);
    }
    template<class Interface> void releaseInterface(Interface*& pointer, Outcome& first) noexcept {
        auto* owned = pointer;
        pointer = nullptr; // never released twice, even if release throws
        if (!owned) return;
        try { owned->release(); }
        catch (...) { if (first.ok()) first = thrown(Stage::Release); }
    }
    Steinberg::IPlugView* view_ = nullptr;
    Steinberg::IPlugViewContentScaleSupport* scale_ = nullptr;
    Frame* frame_ = nullptr;
    bool frameSet_ = false, attached_ = false, fault_ = false, retired_ = false, resizable_ = false;
    Call call_ = Call::None;
    Gate gate_ = nullptr;
    void* gateContext_ = nullptr;
    Extent current_{};
    std::optional<Extent> deferred_, request_;
    uint32_t calls_ = 0, applied_ = 0, refused_ = 0, reentry_ = 0, discarded_ = 0, mismatches_ = 0;
};

inline tresult PLUGIN_API Frame::resizeView(Steinberg::IPlugView* view, Steinberg::ViewRect* rect) {
    if (!mainThread()) { bump(offMain_); return Steinberg::kResultFalse; }
    if (!owner_) { bump(detached_); return Steinberg::kResultFalse; }
    return owner_->frameResize(view, rect);
}
} // namespace ravet::vst3::editor_draft
#endif
