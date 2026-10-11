// Authored SDK-interface editor fixture: an IEditController that creates a
// scripted IPlugView (+ optional IPlugViewContentScaleSupport). Valid objects
// only; faults are scripted results, throws, reentry and retained references.
#ifndef RAVET_VST3_EDITOR_VIEW_FIXTURE_HPP
#define RAVET_VST3_EDITOR_VIEW_FIXTURE_HPP
#include "pluginterfaces/gui/iplugview.h"
#include "pluginterfaces/gui/iplugviewcontentscalesupport.h"
#include "pluginterfaces/vst/ivsteditcontroller.h"
#include <pthread.h>
#include <algorithm>
#include <cstring>
#include <functional>
#include <optional>
#include <stdexcept>
#include <string>
#include <vector>

namespace editor_fixture {
using namespace Steinberg;
using namespace Steinberg::Vst;
inline void requireAt(bool value, const char* file, int line) {
    if (!value) throw std::runtime_error(std::string("editor fixture assertion ") + file + ":" + std::to_string(line));
}
#define require(value) ::editor_fixture::requireAt(bool(value), __FILE__, __LINE__)
constexpr tresult rejected = 4242;
struct Script {
    enum class Create { View, Null, Throw } create = Create::View;
    bool platform = true, throwPlatform = false;
    tresult setFrameResult = kResultOk; bool throwSetFrame = false, throwClearFrame = false;
    ViewRect size{0, 0, 400, 300}; tresult getSizeResult = kResultOk; bool throwGetSize = false;
    tresult attachedResult = kResultOk; bool throwAttached = false;
    std::optional<ViewRect> resizeInAttached;
    tresult removedResult = kResultOk; bool throwRemoved = false;
    tresult canResizeResult = kResultTrue; bool throwCanResize = false;
    tresult constraintResult = kResultTrue; bool throwConstraint = false;
    std::optional<ViewRect> constraintOverride; // replaces the clamped rect
    int32 minimumWidth = 200, maximumWidth = 1200, minimumHeight = 150, maximumHeight = 900;
    tresult onSizeResult = kResultOk; bool throwOnSize = false;
    std::optional<ViewRect> layoutAfterOnSize; // accepted onSize lays out at this size instead
    bool throwGetSizeAfterOnSize = false;
    bool resizeInScaleAlways = false; // request a resize even when not handling the scale
    bool scaleSupported = false; tresult scaleResult = kResultTrue; bool throwScale = false;
    bool resizeInScale = true; tresult queryScaleResult = kResultOk; bool throwQuery = false;
    bool retainFrame = false, throwRelease = false;
    // Reentrant plug-in frame attempt from inside the named host call.
    std::string reenterFrom; ViewRect reenterRect{0, 0, 333, 222};
    std::function<void(const std::string&)> hook; // arbitrary reentry (e.g. provider calls)
    // Observations.
    std::vector<std::string> log;
    std::vector<tresult> frameResults;
    int live = 0, created = 0, destroyed = 0, onSizes = 0;
    void* lastParent = nullptr;
    IPlugFrame* retained = nullptr;
    class View* current = nullptr;
    int count(const std::string& name) const { return int(std::count(log.begin(), log.end(), name)); }
    size_t index(const std::string& name) const {
        auto found = std::find(log.begin(), log.end(), name);
        require(found != log.end());
        return size_t(found - log.begin());
    }
};
class View final : public IPlugView, public IPlugViewContentScaleSupport {
public:
    explicit View(Script& script) : s_(script) { ++s_.live; ++s_.created; s_.current = this; }
    ~View() { --s_.live; ++s_.destroyed; s_.log.emplace_back("view-destroyed"); if (s_.current == this) s_.current = nullptr; }
    tresult PLUGIN_API queryInterface(const TUID id, void** output) override {
        *output = nullptr;
        if (FUnknownPrivate::iidEqual(id, FUnknown::iid) || FUnknownPrivate::iidEqual(id, IPlugView::iid)) {
            *output = static_cast<IPlugView*>(this); addRef(); return kResultOk;
        }
        if (FUnknownPrivate::iidEqual(id, IPlugViewContentScaleSupport::iid) && s_.scaleSupported) {
            *output = static_cast<IPlugViewContentScaleSupport*>(this); addRef();
            if (s_.throwQuery) throw std::runtime_error("query after output");
            return s_.queryScaleResult;
        }
        return kNoInterface;
    }
    uint32 PLUGIN_API addRef() override { return ++refs_; }
    uint32 PLUGIN_API release() override {
        require(pthread_main_np());
        const auto remaining = --refs_;
        if (!remaining) {
            const bool throwing = s_.throwRelease;
            delete this;
            if (throwing) throw std::runtime_error("release after destruction");
        }
        return remaining;
    }
    tresult PLUGIN_API isPlatformTypeSupported(FIDString type) override {
        enter("platform");
        if (s_.throwPlatform) throw std::runtime_error("platform");
        return s_.platform && type && std::strcmp(type, kPlatformTypeNSView) == 0 ? kResultTrue : kResultFalse;
    }
    tresult PLUGIN_API attached(void* parent, FIDString type) override {
        enter("attached");
        require(type && std::strcmp(type, kPlatformTypeNSView) == 0);
        s_.lastParent = parent;
        if (s_.resizeInAttached) { auto rect = *s_.resizeInAttached; frame(rect); }
        if (s_.throwAttached) throw std::runtime_error("attached");
        s_.log.emplace_back("attached-end");
        return s_.attachedResult;
    }
    tresult PLUGIN_API removed() override {
        enter("removed");
        if (s_.throwRemoved) throw std::runtime_error("removed");
        return s_.removedResult;
    }
    tresult PLUGIN_API onWheel(float) override { return kResultFalse; }
    tresult PLUGIN_API onKeyDown(char16, int16, int16) override { return kResultFalse; }
    tresult PLUGIN_API onKeyUp(char16, int16, int16) override { return kResultFalse; }
    tresult PLUGIN_API getSize(ViewRect* size) override {
        enter("getSize");
        if (s_.throwGetSize) throw std::runtime_error("getSize");
        if (s_.throwGetSizeAfterOnSize && s_.onSizes) throw std::runtime_error("getSize after onSize");
        if (size) *size = s_.size;
        return s_.getSizeResult;
    }
    tresult PLUGIN_API onSize(ViewRect* size) override {
        enter("onSize");
        ++s_.onSizes;
        require(size && size->left == 0 && size->top == 0);
        if (s_.throwOnSize) throw std::runtime_error("onSize");
        if (s_.onSizeResult == kResultOk) s_.size = s_.layoutAfterOnSize ? *s_.layoutAfterOnSize : *size;
        return s_.onSizeResult;
    }
    tresult PLUGIN_API onFocus(TBool) override { return kResultFalse; }
    tresult PLUGIN_API setFrame(IPlugFrame* value) override {
        enter(value ? "setFrame" : "clearFrame");
        if (value && s_.throwSetFrame) throw std::runtime_error("setFrame");
        if (!value && s_.throwClearFrame) throw std::runtime_error("clearFrame");
        if (value && s_.retainFrame && !s_.retained) { value->addRef(); s_.retained = value; }
        frame_ = value;
        return value ? s_.setFrameResult : kResultOk;
    }
    tresult PLUGIN_API canResize() override {
        enter("canResize");
        if (s_.throwCanResize) throw std::runtime_error("canResize");
        return s_.canResizeResult;
    }
    tresult PLUGIN_API checkSizeConstraint(ViewRect* rect) override {
        enter("constraint");
        if (s_.throwConstraint) throw std::runtime_error("constraint");
        if (s_.constraintOverride) *rect = *s_.constraintOverride;
        else {
            rect->right = rect->left + std::clamp(rect->getWidth(), s_.minimumWidth, s_.maximumWidth);
            rect->bottom = rect->top + std::clamp(rect->getHeight(), s_.minimumHeight, s_.maximumHeight);
        }
        return s_.constraintResult;
    }
    tresult PLUGIN_API setContentScaleFactor(ScaleFactor factor) override {
        enter("scale");
        if (s_.throwScale) throw std::runtime_error("scale");
        if (s_.resizeInScaleAlways || (s_.resizeInScale && s_.scaleResult == kResultTrue)) {
            ViewRect rect(0, 0, int32(400 * factor), int32(300 * factor));
            frame(rect);
        }
        return s_.scaleResult;
    }
    // Plug-in initiated resize, e.g. from a UI event on MAIN or another thread.
    tresult frame(ViewRect rect) {
        require(frame_);
        const auto result = frame_->resizeView(this, &rect);
        s_.frameResults.push_back(result);
        return result;
    }
    tresult frameFor(IPlugView* other, ViewRect* rect) { require(frame_); return frame_->resizeView(other, rect); }
    IPlugFrame* currentFrame() const { return frame_; }
private:
    void enter(const char* name) {
        s_.log.emplace_back(name);
        require(pthread_main_np());
        if (s_.reenterFrom == name) s_.frameResults.push_back(frame_ ? frame_->resizeView(this, &s_.reenterRect) : kNotInitialized);
        if (s_.hook) s_.hook(name);
    }
    Script& s_;
    uint32 refs_ = 1;
    IPlugFrame* frame_ = nullptr;
};
// Separate edit controller whose createView follows the script.
class Controller final : public IEditController {
public:
    explicit Controller(Script& script) : s_(script) {}
    tresult PLUGIN_API queryInterface(const TUID id, void** output) override {
        *output = nullptr;
        if (FUnknownPrivate::iidEqual(id, FUnknown::iid) || FUnknownPrivate::iidEqual(id, IEditController::iid) ||
            FUnknownPrivate::iidEqual(id, IPluginBase::iid)) {
            *output = static_cast<IEditController*>(this); addRef(); return kResultOk;
        }
        return kNoInterface;
    }
    uint32 PLUGIN_API addRef() override { return ++refs_; }
    uint32 PLUGIN_API release() override { const auto remaining = --refs_; if (!remaining) { s_.log.emplace_back("controller-destroyed"); delete this; } return remaining; }
    tresult PLUGIN_API initialize(FUnknown*) override { s_.log.emplace_back("controller-initialize"); return kResultOk; }
    tresult PLUGIN_API terminate() override { s_.log.emplace_back("controller-terminate"); return kResultOk; }
    tresult PLUGIN_API setComponentState(IBStream*) override { return kResultOk; }
    tresult PLUGIN_API setState(IBStream*) override { return kResultOk; }
    tresult PLUGIN_API getState(IBStream*) override { return kResultOk; }
    int32 PLUGIN_API getParameterCount() override { return 0; }
    tresult PLUGIN_API getParameterInfo(int32, ParameterInfo&) override { return kResultFalse; }
    tresult PLUGIN_API getParamStringByValue(ParamID, ParamValue, String128) override { return kResultFalse; }
    tresult PLUGIN_API getParamValueByString(ParamID, TChar*, ParamValue&) override { return kResultFalse; }
    ParamValue PLUGIN_API normalizedParamToPlain(ParamID, ParamValue value) override { return value; }
    ParamValue PLUGIN_API plainParamToNormalized(ParamID, ParamValue value) override { return value; }
    ParamValue PLUGIN_API getParamNormalized(ParamID) override { return 0; }
    tresult PLUGIN_API setParamNormalized(ParamID, ParamValue) override { return kResultOk; }
    tresult PLUGIN_API setComponentHandler(IComponentHandler* handler) override {
        s_.log.emplace_back(handler ? "handler" : "handler-clear"); return kResultOk;
    }
    IPlugView* PLUGIN_API createView(FIDString name) override {
        s_.log.emplace_back("createView");
        require(pthread_main_np() && name && std::strcmp(name, ViewType::kEditor) == 0);
        if (s_.create == Script::Create::Throw) throw std::runtime_error("createView");
        if (s_.create == Script::Create::Null) return nullptr;
        return static_cast<IPlugView*>(new View(s_));
    }
private:
    Script& s_;
    uint32 refs_ = 1;
};
} // namespace editor_fixture
#endif
