// Injected SDK view fixture for the MAIN-only editor owner: creation faults,
// lifecycle, bounded size policy, IPlugFrame reentrancy, content scale,
// plug-in-retained frames and ordered teardown.
#include "RavetVst3EditorOwner.hpp"
#include "editor-view-fixture.hpp"
#include <iostream>
#include <limits>
#include <thread>
#include <csignal>
#include <sys/wait.h>
#include <unistd.h>

using namespace editor_fixture;
using namespace ravet::vst3::editor_draft;
namespace {
int parentStorage = 0;
void* const parent = &parentStorage;
// Non-finite values from bit patterns, so fast-math builds exercise the same refusal.
float fromBits(uint32_t bits) { float value; std::memcpy(&value, &bits, sizeof(value)); return value; } // opaque: the fixture view never dereferences it
std::unique_ptr<Owner> make(Script& script, Extent& initial, Outcome& outcome) {
    Controller* controller = new Controller(script);
    auto owner = Owner::create(*controller, initial, outcome);
    controller->release(); // the view keeps no controller reference in this fixture
    return owner;
}
std::unique_ptr<Owner> make(Script& script) {
    Extent initial; Outcome outcome;
    auto owner = make(script, initial, outcome);
    require(owner && outcome.ok() && initial == (Extent{400, 300}));
    return owner;
}
void expect(const Outcome& outcome, Stage stage, Failure failure, int32_t status) {
    if (outcome.stage != stage || outcome.failure != failure || outcome.status != status)
        std::cerr << "outcome stage " << uint32_t(outcome.stage) << " failure " << uint32_t(outcome.failure)
                  << " status " << outcome.status << '\n';
    require(outcome.stage == stage && outcome.failure == failure && outcome.status == status);
    require((outcome.failure == Failure::Returned) == bool(outcome.flags & RAVET_VST3_EDITOR_RETURNED));
    require((outcome.failure == Failure::Thrown) == bool(outcome.flags & RAVET_VST3_EDITOR_THROWN));
}
void creationFaults() {
    const int64_t frames = Frame::live().load();
    {   Script s; s.create = Script::Create::Null; Extent e; Outcome o;
        require(!make(s, e, o)); expect(o, Stage::CreateView, Failure::Unavailable, RAVET_VST3_EDITOR_UNAVAILABLE); }
    {   Script s; s.create = Script::Create::Throw; Extent e; Outcome o;
        require(!make(s, e, o)); expect(o, Stage::CreateView, Failure::Thrown, RAVET_VST3_NATIVE); }
    {   Script s; s.platform = false; Extent e; Outcome o;
        require(!make(s, e, o)); expect(o, Stage::Platform, Failure::Unsupported, RAVET_VST3_EDITOR_UNSUPPORTED);
        require(s.live == 0 && s.count("setFrame") == 0 && s.count("clearFrame") == 0); }
    {   Script s; s.throwPlatform = true; Extent e; Outcome o;
        require(!make(s, e, o)); expect(o, Stage::Platform, Failure::Thrown, RAVET_VST3_NATIVE); require(s.live == 0); }
    {   Script s; s.setFrameResult = rejected; Extent e; Outcome o;
        require(!make(s, e, o)); expect(o, Stage::SetFrame, Failure::Returned, RAVET_VST3_NATIVE);
        require(o.native == rejected && s.live == 0 && s.count("clearFrame") == 1); }
    {   Script s; s.throwSetFrame = true; Extent e; Outcome o;
        require(!make(s, e, o)); expect(o, Stage::SetFrame, Failure::Thrown, RAVET_VST3_NATIVE);
        require(o.native == 0 && s.count("clearFrame") == 1 && s.live == 0); }
    {   Script s; s.throwCanResize = true; Extent e; Outcome o;
        require(!make(s, e, o)); expect(o, Stage::CanResize, Failure::Thrown, RAVET_VST3_NATIVE); require(s.live == 0); }
    {   Script s; s.getSizeResult = rejected; Extent e; Outcome o;
        require(!make(s, e, o)); expect(o, Stage::GetSize, Failure::Returned, RAVET_VST3_NATIVE); require(s.live == 0); }
    for (ViewRect bad : {ViewRect(0, 0, 0, 10), ViewRect(0, 0, 16385, 10), ViewRect(10, 0, 5, 10),
                         ViewRect(-2147483647 - 1, 0, 2147483647, 10), ViewRect(0, 0, 10, 0)}) {
        Script s; s.size = bad; Extent e{7, 7}; Outcome o;
        require(!make(s, e, o)); expect(o, Stage::GetSize, Failure::Malformed, RAVET_VST3_METADATA);
        require(e == (Extent{7, 7}) && s.live == 0);
        // Ordered failure cleanup: frame cleared before the view is released.
        require(s.index("clearFrame") < s.index("view-destroyed"));
    }
    {   Script s; s.size = ViewRect(5, 5, 16389, 16389); Extent e; Outcome o;
        auto owner = make(s, e, o); require(owner && e == (Extent{16384, 16384})); require(owner->retire().ok()); }
    {   Script s; s.scaleSupported = true; s.queryScaleResult = kResultFalse; Extent e; Outcome o;
        auto owner = make(s, e, o); require(owner && !(owner->state().flags & 4)); require(owner->retire().ok() && s.live == 0); }
    {   Script s; s.scaleSupported = true; s.throwQuery = true; Extent e; Outcome o;
        require(!make(s, e, o)); expect(o, Stage::Query, Failure::Thrown, RAVET_VST3_NATIVE); require(s.live == 0); }
    {   Script s; s.reenterFrom = "getSize"; Extent e; Outcome o;
        auto owner = make(s, e, o); require(owner && s.frameResults.back() == kResultFalse);
        require(owner->state().frame_reentry_refused == 1); require(owner->retire().ok()); }
    require(Frame::live().load() == frames);
}
void attachAndDeferred() {
    Script s; s.resizeInAttached = ViewRect(0, 0, 500, 350);
    auto owner = make(s);
    Extent applied;
    auto o = owner->attach(parent, applied);
    expect(o, Stage::Attached, Failure::None, RAVET_VST3_OK);
    require(o.flags & RAVET_VST3_EDITOR_ATTACHED && o.flags & RAVET_VST3_EDITOR_DEFERRED);
    require(applied == (Extent{500, 350}) && s.lastParent == parent && s.frameResults.back() == kResultTrue);
    // The deferred onSize happens after attached returned, exactly once.
    require(s.index("attached-end") < s.index("onSize") && s.count("onSize") == 1);
    Extent request;
    require(owner->takeRequest(request) && request == (Extent{500, 350}) && !owner->takeRequest(request));
    require(owner->attach(parent, applied).status == RAVET_VST3_LIFECYCLE);
    require(owner->detach().ok() && !owner->attached());
    require(owner->detach().status == RAVET_VST3_LIFECYCLE);
    require(owner->attach(nullptr, applied).status == RAVET_VST3_INVALID);
    auto r = owner->retire(); require(r.ok() && s.live == 0 && s.count("removed") == 1);
}
void idleFrameResize() {
    Script s; auto owner = make(s);
    Extent applied; require(owner->attach(parent, applied).ok());
    View* view = s.current;
    const auto sizes = s.onSizes;
    require(view->frame(ViewRect(10, 10, 610, 410)) == kResultTrue && s.onSizes == sizes + 1);
    require(owner->current() == (Extent{600, 400}) && s.size.getWidth() == 600);
    Extent request; require(owner->takeRequest(request) && request == (Extent{600, 400}));
    require(view->frame(ViewRect(0, 0, 600, 400)) == kResultTrue && s.onSizes == sizes + 1); // unchanged: no onSize
    require(!owner->takeRequest(request));
    s.onSizeResult = rejected;
    require(view->frame(ViewRect(0, 0, 700, 500)) == kResultFalse && owner->current() == (Extent{600, 400}));
    s.onSizeResult = kResultOk;
    ViewRect bad(0, 0, 0, 5);
    require(view->frame(bad) == kInvalidArgument);
    require(view->frameFor(nullptr, &bad) == kInvalidArgument);
    View other(s); ViewRect fine(0, 0, 300, 200);
    require(view->frameFor(&other, &fine) == kInvalidArgument);
    require(view->frameFor(view, nullptr) == kInvalidArgument);
    tresult offMain = kResultOk;
    std::thread worker([&] { offMain = view->currentFrame()->resizeView(view, &fine); });
    worker.join();
    require(offMain == kResultFalse && owner->current() == (Extent{600, 400}));
    const auto state = owner->state();
    require(state.frame_calls == 8 && state.frame_applied == 2 && state.frame_refused == 6 && state.frame_reentry_refused == 0);
    require(state.flags == 1u + 2u); // attached + resizable
    require(owner->retire().ok());
}
void reentrancy() {
    for (const char* call : {"onSize", "constraint", "canResize", "getSize", "removed"}) {
        Script s; s.reenterFrom = call; auto owner = make(s);
        Extent applied; require(owner->attach(parent, applied).ok());
        if (std::string(call) == "removed") require(owner->detach().ok());
        else if (std::string(call) == "getSize") { Extent e; require(owner->size(e).ok() && e == (Extent{400, 300})); }
        else require(owner->resize({640, 480}, applied).ok() && applied == (Extent{640, 480}));
        require(s.frameResults.back() == kResultFalse);
        require(owner->state().frame_reentry_refused >= 1);
        require(owner->current() == (std::string(call) == "removed" || std::string(call) == "getSize" ? Extent{400, 300} : Extent{640, 480}));
        require(owner->retire().ok());
    }
    {   // A frame-initiated onSize may not recurse through another resizeView.
        Script s; s.reenterFrom = "onSize"; auto owner = make(s);
        Extent attachedSize; require(owner->attach(parent, attachedSize).ok());
        require(s.current->frame(ViewRect(0, 0, 450, 320)) == kResultTrue);
        require(s.frameResults.size() >= 2 && s.frameResults[s.frameResults.size() - 2] == kResultFalse);
        require(owner->current() == (Extent{450, 320}) && owner->state().frame_reentry_refused == 1);
        require(owner->retire().ok());
    }
    {   // A closed containing-module gate refuses an idle frame resize before onSize;
        // an open gate is entered and released around exactly that onSize.
        Script s; auto owner = make(s);
        struct Gate { bool open = false; int entered = 0, released = 0; } gate;
        owner->gate([](void* context, bool enter) noexcept {
            auto& g = *static_cast<Gate*>(context);
            if (!enter) { ++g.released; return true; }
            if (!g.open) return false;
            ++g.entered; return true;
        }, &gate);
        Extent attachedSize; require(owner->attach(parent, attachedSize).ok());
        const auto sizes = s.onSizes;
        require(s.current->frame(ViewRect(0, 0, 520, 380)) == kResultFalse && s.onSizes == sizes);
        require(owner->state().frame_reentry_refused == 1 && gate.entered == 0 && gate.released == 0);
        gate.open = true;
        require(s.current->frame(ViewRect(0, 0, 520, 380)) == kResultTrue && s.onSizes == sizes + 1);
        require(gate.entered == 1 && gate.released == 1);
        s.throwOnSize = true;
        require(s.current->frame(ViewRect(0, 0, 530, 390)) == kInternalError && gate.released == 2 && owner->faulted());
        require(owner->retire().ok());
    }
    {   // Owner operations from inside a view callback are refused before any foreign call.
        Script s; std::unique_ptr<Owner> owner; Outcome nested; bool fired = false;
        s.hook = [&](const std::string& name) {
            if (name != "onSize" || fired || !owner) return;
            fired = true; Extent e;
            nested = owner->size(e);
            require(owner->retire().status == RAVET_VST3_EDITOR_REENTRY);
            require(owner->detach().status == RAVET_VST3_EDITOR_REENTRY);
        };
        owner = make(s);
        Extent applied; require(owner->resize({500, 400}, applied).ok());
        require(fired); expect(nested, Stage::Admission, Failure::Reentry, RAVET_VST3_EDITOR_REENTRY);
        s.hook = nullptr;
        require(owner->retire().ok());
    }
}
void resizePolicy() {
    Script s; auto owner = make(s); Extent applied{1, 1};
    for (Extent bad : {Extent{0, 10}, Extent{10, 0}, Extent{16385, 10}, Extent{10, 16385}}) {
        const auto calls = s.log.size();
        expect(owner->resize(bad, applied), Stage::Constraint, Failure::Local, RAVET_VST3_INVALID);
        require(s.log.size() == calls && applied == (Extent{1, 1}));
    }
    require(owner->resize({100, 100}, applied).ok() && applied == (Extent{200, 150})); // clamped by constraint
    require(owner->resize({2000, 2000}, applied).ok() && applied == (Extent{1200, 900}));
    s.canResizeResult = kResultFalse;
    auto calls = s.count("constraint");
    expect(owner->resize({640, 480}, applied), Stage::CanResize, Failure::Unsupported, RAVET_VST3_EDITOR_UNSUPPORTED);
    require(s.count("constraint") == calls && !(owner->state().flags & 2));
    s.canResizeResult = kResultTrue;
    s.constraintResult = kResultFalse;
    calls = s.onSizes;
    auto o = owner->resize({640, 480}, applied);
    expect(o, Stage::Constraint, Failure::Returned, RAVET_VST3_NATIVE); require(o.native == kResultFalse && s.onSizes == calls);
    s.constraintResult = kResultTrue;
    s.constraintOverride = ViewRect(0, 0, 20000, 10);
    expect(owner->resize({640, 480}, applied), Stage::Constraint, Failure::Malformed, RAVET_VST3_METADATA);
    require(s.onSizes == calls);
    s.constraintOverride.reset();
    s.onSizeResult = rejected;
    o = owner->resize({640, 480}, applied);
    expect(o, Stage::OnSize, Failure::Returned, RAVET_VST3_NATIVE);
    require(o.native == rejected && owner->current() == (Extent{1200, 900}) && applied == (Extent{1200, 900}));
    s.onSizeResult = kResultOk;
    Extent size; require(owner->size(size).ok());
    s.getSizeResult = kResultFalse;
    expect(owner->size(size), Stage::GetSize, Failure::Returned, RAVET_VST3_NATIVE);
    s.getSizeResult = kResultOk; s.size = ViewRect(0, 0, 0, 0);
    expect(owner->size(size), Stage::GetSize, Failure::Malformed, RAVET_VST3_METADATA);
    s.size = ViewRect(0, 0, 400, 300);
    s.throwConstraint = true;
    expect(owner->resize({640, 480}, applied), Stage::Constraint, Failure::Thrown, RAVET_VST3_NATIVE);
    require(owner->faulted() && owner->state().flags & 16);
    expect(owner->size(size), Stage::Admission, Failure::Reentry, RAVET_VST3_LIFECYCLE);
    require(s.current->frame(ViewRect(0, 0, 300, 300)) == kResultFalse); // faulted owner refuses frame resizes
    require(owner->retire().ok() && s.live == 0);
}
void contentScale() {
    {   Script s; auto owner = make(s); Extent applied;
        expect(owner->scale(2.f, applied), Stage::Scale, Failure::Unsupported, RAVET_VST3_EDITOR_UNSUPPORTED);
        require(s.count("scale") == 0); require(owner->retire().ok()); }
    {   Script s; s.scaleSupported = true; auto owner = make(s); Extent applied;
        require(owner->state().flags & 4);
        for (float bad : {0.f, 0.2f, 8.5f, -1.f, fromBits(0x7f800000u), fromBits(0xff800000u), fromBits(0x7fc00000u)})
            expect(owner->scale(bad, applied), Stage::Scale, Failure::Local, RAVET_VST3_INVALID);
        require(owner->attach(parent, applied).ok());
        auto o = owner->scale(2.f, applied);
        expect(o, Stage::Scale, Failure::None, RAVET_VST3_OK);
        require(o.flags & RAVET_VST3_EDITOR_DEFERRED && applied == (Extent{800, 600}));
        require(s.index("scale") < s.index("onSize"));
        Extent request; require(owner->takeRequest(request) && request == (Extent{800, 600}));
        s.scaleResult = kResultFalse;
        o = owner->scale(1.f, applied);
        expect(o, Stage::Scale, Failure::Returned, RAVET_VST3_NATIVE);
        require(owner->current() == (Extent{800, 600}) && !owner->takeRequest(request));
        s.scaleResult = kResultTrue; s.throwScale = true;
        expect(owner->scale(1.f, applied), Stage::Scale, Failure::Thrown, RAVET_VST3_NATIVE);
        require(owner->faulted() && owner->attached());
        const auto r = owner->retire(); require(r.ok() && s.count("removed") == 1 && s.live == 0); }
}
void lifecycleFaults() {
    {   Script s; s.attachedResult = rejected; auto owner = make(s); Extent applied{9, 9};
        s.resizeInAttached = ViewRect(0, 0, 500, 400);
        auto o = owner->attach(parent, applied);
        expect(o, Stage::Attached, Failure::Returned, RAVET_VST3_NATIVE);
        require(o.native == rejected && !owner->attached() && !(o.flags & RAVET_VST3_EDITOR_ATTACHED) && applied == (Extent{9, 9}));
        // A refused attach keeps the acknowledged request pending, never applies it detached...
        require(s.count("onSize") == 0 && (owner->state().flags & 32) && owner->state().deferred_discarded == 0);
        s.attachedResult = kResultOk; s.resizeInAttached.reset();
        // ...and the next successful attach honours it exactly once.
        o = owner->attach(parent, applied);
        require(o.ok() && (o.flags & RAVET_VST3_EDITOR_DEFERRED) && applied == (Extent{500, 400}) && s.count("onSize") == 1);
        require(s.index("attached-end") < s.index("onSize") && !(owner->state().flags & 32));
        s.removedResult = rejected;
        o = owner->detach();
        expect(o, Stage::Removed, Failure::Returned, RAVET_VST3_NATIVE);
        require(owner->attached() && o.flags & RAVET_VST3_EDITOR_ATTACHED); // retryable, still attached
        s.removedResult = kResultOk;
        require(owner->detach().ok() && !owner->attached());
        require(owner->retire().ok() && s.count("removed") == 2); }
    {   Script s; s.throwAttached = true; auto owner = make(s); Extent applied;
        auto o = owner->attach(parent, applied);
        expect(o, Stage::Attached, Failure::Thrown, RAVET_VST3_NATIVE);
        require(o.flags & RAVET_VST3_EDITOR_ATTACHED && owner->attached() && owner->faulted());
        expect(owner->detach(), Stage::Admission, Failure::Reentry, RAVET_VST3_LIFECYCLE);
        const auto r = owner->retire(); require(r.ok() && s.count("removed") == 1); } // conservative removal duty
    {   Script s; s.throwRemoved = true; auto owner = make(s); Extent applied;
        require(owner->attach(parent, applied).ok());
        expect(owner->detach(), Stage::Removed, Failure::Thrown, RAVET_VST3_NATIVE);
        require(!owner->attached() && owner->faulted());
        s.throwRemoved = false;
        require(owner->retire().ok() && s.count("removed") == 1); } // never asked to remove twice
    {   Script s; s.throwOnSize = true; s.resizeInAttached = ViewRect(0, 0, 500, 400); auto owner = make(s); Extent applied;
        auto o = owner->attach(parent, applied);
        expect(o, Stage::OnSize, Failure::Thrown, RAVET_VST3_NATIVE);
        require(o.flags & RAVET_VST3_EDITOR_ATTACHED && owner->attached() && owner->faulted());
        require(owner->retire().ok() && s.count("removed") == 1); }
}
void orderedTeardown() {
    {   Script s; auto owner = make(s); Extent applied; require(owner->attach(parent, applied).ok());
        s.log.clear();
        const auto r = owner->retire();
        require(r.ok() && r.status == RAVET_VST3_OK);
        require(s.log == (std::vector<std::string>{"removed", "clearFrame", "view-destroyed"}));
        require(owner->retire().status == RAVET_VST3_LIFECYCLE);
        expect(owner->attach(parent, applied), Stage::Admission, Failure::Reentry, RAVET_VST3_LIFECYCLE); }
    {   // First failure wins; teardown always consumes and releases the view.
        Script s; auto owner = make(s); Extent applied; require(owner->attach(parent, applied).ok());
        s.removedResult = rejected; s.throwClearFrame = true;
        auto r = owner->retire();
        expect(r, Stage::Removed, Failure::Returned, RAVET_VST3_RETIRED_NATIVE);
        require(r.native == rejected && s.live == 0 && s.count("clearFrame") == 1); }
    {   Script s; auto owner = make(s); s.throwRelease = true;
        auto r = owner->retire();
        expect(r, Stage::Release, Failure::Thrown, RAVET_VST3_RETIRED_NATIVE); require(s.live == 0); }
    {   // A plug-in that keeps its frame reference: later calls are refused and
        // the frame is freed only on the plug-in's final release (any thread).
        const auto frames = Frame::live().load();
        Script s; s.retainFrame = true; auto owner = make(s);
        require(owner->retire().ok() && s.live == 0 && s.retained);
        require(Frame::live().load() == frames + 1);
        ViewRect rect(0, 0, 300, 200);
        require(s.retained->resizeView(nullptr, &rect) == kResultFalse);
        void* queried = nullptr;
        require(s.retained->queryInterface(IPlugFrame::iid, &queried) == kResultOk && queried == s.retained);
        s.retained->release();
        std::thread worker([&] { require(s.retained->release() == 0); });
        worker.join();
        require(Frame::live().load() == frames);
        owner.reset(); }
}
void ownerThread() {
    Script s; auto owner = make(s); Extent e; Outcome size, retired;
    std::thread worker([&] { size = owner->size(e); retired = owner->retire(); });
    worker.join();
    expect(size, Stage::Admission, Failure::Local, RAVET_VST3_THREAD);
    expect(retired, Stage::Admission, Failure::Local, RAVET_VST3_THREAD);
    Controller* controller = new Controller(s); Extent initial; Outcome created; std::unique_ptr<Owner> other;
    std::thread creator([&] { other = Owner::create(*controller, initial, created); });
    creator.join();
    require(!other); expect(created, Stage::Admission, Failure::Local, RAVET_VST3_THREAD);
    controller->release();
    require(owner->retire().ok() && s.live == 0);
}

// F1: every refusal on MAIN carries the authoritative attached state.
void refusalState() {
    const uint32_t both = RAVET_VST3_EDITOR_STATE | RAVET_VST3_EDITOR_ATTACHED;
    {   Script s; auto owner = make(s); Extent e;
        auto o = owner->attach(nullptr, e);                       // local invalid, detached
        expect(o, Stage::Attached, Failure::Local, RAVET_VST3_INVALID);
        require((o.flags & both) == RAVET_VST3_EDITOR_STATE);
        o = owner->detach();                                        // not attached
        require(o.status == RAVET_VST3_LIFECYCLE && (o.flags & both) == RAVET_VST3_EDITOR_STATE);
        require(owner->attach(parent, e).ok());
        o = owner->attach(parent, e);                               // second attach
        expect(o, Stage::Attached, Failure::Reentry, RAVET_VST3_LIFECYCLE);
        require((o.flags & both) == both && owner->attached());
        o = owner->attach(nullptr, e);                              // invalid while attached
        require(o.status == RAVET_VST3_INVALID && (o.flags & both) == both);
        o = owner->resize({0, 5}, e);
        require(o.status == RAVET_VST3_INVALID && (o.flags & both) == both);
        o = owner->scale(-1.f, e);
        require(o.status == RAVET_VST3_INVALID && (o.flags & both) == both);
        Outcome nested; bool fired = false;                         // owner busy (reentry)
        s.hook = [&](const std::string& name) { if (name == "getSize" && !fired) { fired = true; Extent x; nested = owner->detach(); (void)x; } };
        require(owner->size(e).ok() && fired);
        s.hook = nullptr;
        require(nested.status == RAVET_VST3_EDITOR_REENTRY && (nested.flags & both) == both);
        Outcome offMain; std::thread worker([&] { offMain = owner->size(e); }); worker.join();
        require(offMain.status == RAVET_VST3_THREAD && !(offMain.flags & RAVET_VST3_EDITOR_STATE)); // no owner state read
        s.throwGetSize = true;                                      // sticky fault while attached
        require(owner->size(e).failure == Failure::Thrown && owner->faulted());
        s.throwGetSize = false;
        for (auto refused : {owner->attach(parent, e), owner->detach(), owner->scale(1.f, e), owner->size(e), owner->resize({500, 400}, e)}) {
            require(refused.status == RAVET_VST3_LIFECYCLE && refused.stage == Stage::Admission && (refused.flags & both) == both);
        }
        auto r = owner->retire();
        require(r.ok() && (r.flags & both) == RAVET_VST3_EDITOR_STATE && s.count("removed") == 1); }
    {   Script s; auto owner = make(s); Extent e;                   // fault while detached
        s.throwConstraint = true; require(owner->resize({500, 400}, e).failure == Failure::Thrown);
        auto o = owner->attach(parent, e);
        require(o.status == RAVET_VST3_LIFECYCLE && (o.flags & both) == RAVET_VST3_EDITOR_STATE);
        require(owner->retire().ok() && s.count("removed") == 0); }
}
// F6: idle plug-in requests are never applied to a detached view; the consumer
// mailbox is cleared by detach, retire and a successful host resize.
void detachedRequestsAndMailbox() {
    Script s; auto owner = make(s); Extent e;
    auto sizes = s.onSizes;
    require(s.current->frame(ViewRect(0, 0, 640, 480)) == kResultFalse && s.onSizes == sizes);   // before attach
    require(owner->current() == (Extent{400, 300}) && owner->state().frame_refused == 1);
    require(owner->attach(parent, e).ok());
    require(s.current->frame(ViewRect(0, 0, 640, 480)) == kResultTrue && (owner->state().flags & 8));
    require(owner->detach().ok() && !(owner->state().flags & 8));                                // detach clears
    Extent request; require(!owner->takeRequest(request));
    sizes = s.onSizes;
    require(s.current->frame(ViewRect(0, 0, 700, 500)) == kResultFalse && s.onSizes == sizes);   // after detach
    require(owner->attach(parent, e).ok());
    require(s.current->frame(ViewRect(0, 0, 720, 520)) == kResultTrue && (owner->state().flags & 8));
    require(owner->resize({800, 600}, e).ok() && e == (Extent{800, 600}) && !owner->takeRequest(request)); // host resize supersedes
    require(s.current->frame(ViewRect(0, 0, 900, 700)) == kResultTrue && (owner->state().flags & 8));
    require(owner->retire().ok() && !owner->takeRequest(request));                               // retire clears
}
// F7: acknowledged deferred requests are applied later or counted, never silently lost.
void deferredHonoured() {
    {   Script s; auto owner = make(s); Extent e; View* view = nullptr;   // superseded inside attached
        s.hook = [&](const std::string& name) { if (name == "attached") { view = s.current; view->frame(ViewRect(0, 0, 450, 330)); view->frame(ViewRect(0, 0, 470, 340)); } };
        auto o = owner->attach(parent, e); s.hook = nullptr;
        require(o.ok() && (o.flags & RAVET_VST3_EDITOR_DEFERRED) && e == (Extent{470, 340}));
        require(s.frameResults[0] == kResultTrue && s.frameResults[1] == kResultTrue && s.count("onSize") == 1);
        require(owner->state().deferred_discarded == 1);
        require(owner->retire().ok()); }
    {   Script s; s.throwAttached = true; s.resizeInAttached = ViewRect(0, 0, 500, 400);   // lost to a fault: counted
        auto owner = make(s); Extent e;
        require(owner->attach(parent, e).failure == Failure::Thrown && owner->state().deferred_discarded == 1);
        require(owner->retire().ok()); }
    {   Script s; s.resizeInAttached = ViewRect(0, 0, 500, 400); s.onSizeResult = rejected;  // plug-in refuses its own request
        auto owner = make(s); Extent e;
        auto o = owner->attach(parent, e);
        require(o.ok() && !(o.flags & RAVET_VST3_EDITOR_DEFERRED) && e == (Extent{400, 300}));
        const auto st = owner->state();
        require(st.deferred_discarded == 1 && st.frame_refused == 1 && !(st.flags & 8));
        require(owner->retire().ok()); }
    {   Script s; s.scaleSupported = true; s.scaleResult = kResultFalse; s.resizeInScaleAlways = true;
        auto owner = make(s); Extent e;
        require(owner->scale(2.f, e).failure == Failure::Returned);      // detached: kept pending
        require((owner->state().flags & 32) && s.count("onSize") == 0 && e == (Extent{400, 300}));
        auto o = owner->attach(parent, e);                                // applied at attach
        require(o.ok() && (o.flags & RAVET_VST3_EDITOR_DEFERRED) && e == (Extent{800, 600}));
        o = owner->scale(1.f, e);                                         // refused scale while attached: still honoured
        require(o.failure == Failure::Returned && (o.flags & RAVET_VST3_EDITOR_DEFERRED) && e == (Extent{400, 300}));
        require(owner->state().deferred_discarded == 0);
        require(owner->retire().ok()); }
    {   Script s; s.attachedResult = rejected; s.resizeInAttached = ViewRect(0, 0, 500, 400);   // pending at retire: counted
        auto owner = make(s); Extent e;
        require(owner->attach(parent, e).failure == Failure::Returned && (owner->state().flags & 32));
        require(owner->retire().ok() && s.count("onSize") == 0); }
}
// F8: the recorded size is the re-queried getSize after onSize.
void requeriedSize() {
    Script s; auto owner = make(s); Extent e;
    require(owner->attach(parent, e).ok());
    s.layoutAfterOnSize = ViewRect(0, 0, 610, 470);
    require(owner->resize({640, 480}, e).ok() && e == (Extent{610, 470}) && owner->current() == e);
    require(owner->state().size_mismatches == 1);
    require(s.current->frame(ViewRect(0, 0, 700, 500)) == kResultTrue && owner->current() == (Extent{610, 470}));
    Extent request; require(owner->takeRequest(request) && request == (Extent{610, 470}) && owner->state().size_mismatches == 2);
    s.layoutAfterOnSize.reset();
    require(owner->resize({640, 480}, e).ok() && e == (Extent{640, 480}) && owner->state().size_mismatches == 2);
    s.getSizeResult = kResultFalse;    // failed re-query: passed size kept, counted
    require(owner->resize({660, 490}, e).ok() && e == (Extent{660, 490}) && owner->state().size_mismatches == 3);
    s.getSizeResult = kResultOk; s.throwGetSizeAfterOnSize = true;
    auto o = owner->resize({680, 500}, e);
    expect(o, Stage::GetSize, Failure::Thrown, RAVET_VST3_NATIVE);
    require(owner->faulted() && owner->current() == (Extent{680, 500}));
    require(owner->retire().ok());
}
// F9: checkSizeConstraint accepts only kResultTrue (== kResultOk).
void constraintSemantics() {
    static_assert(kResultTrue == kResultOk, "SDK defines kResultTrue as kResultOk");
    for (tresult result : {tresult(kResultFalse), tresult(kNotImplemented), tresult(kInvalidArgument), rejected}) {
        Script s; s.constraintResult = result; auto owner = make(s); Extent e{1, 1};
        auto o = owner->resize({640, 480}, e);
        expect(o, Stage::Constraint, Failure::Returned, RAVET_VST3_NATIVE);
        require(o.native == result && s.onSizes == 0 && e == (Extent{1, 1}) && owner->current() == (Extent{400, 300}));
        require(owner->retire().ok());
    }
    Script s; s.constraintResult = kResultTrue; auto owner = make(s); Extent e;
    require(owner->resize({100, 2000}, e).ok() && e == (Extent{200, 900}));
    require(owner->retire().ok());
}
// F10: destroying an unretired owner from inside its own callback terminates.
void destructorInvariant() {
    std::cout.flush(); std::cerr.flush();
    const pid_t child = fork();
    require(child >= 0);
    if (child == 0) {
        Script s; std::unique_ptr<Owner> owner = make(s);
        s.hook = [&](const std::string& name) { if (name == "getSize") owner.reset(); };
        Extent e; (void)owner->size(e);
        _exit(0); // reaching here would mean the refused retire was ignored
    }
    int status = 0;
    require(waitpid(child, &status, 0) == child);
    require(WIFSIGNALED(status) && WTERMSIG(status) == SIGABRT);
}
void repeated() {
    const auto frames = Frame::live().load();
    Script s; s.scaleSupported = true;
    for (int cycle = 0; cycle < 1000; ++cycle) {
        auto owner = make(s); Extent applied;
        require(owner->attach(parent, applied).ok());
        require(owner->resize({uint32_t(300 + cycle % 500), 400}, applied).ok());
        require(s.current->frame(ViewRect(0, 0, 640, 480)) == kResultTrue);
        Extent request; require(owner->takeRequest(request) && request == (Extent{640, 480}));
        require(owner->scale(1.f, applied).ok());
        require(owner->detach().ok() && owner->retire().ok());
        s.size = ViewRect(0, 0, 400, 300);
    }
    require(s.created == 1000 && s.destroyed == 1000 && s.live == 0 && Frame::live().load() == frames);
}
}
int main() {
    try {
        require(pthread_main_np());
        creationFaults(); attachAndDeferred(); idleFrameResize(); reentrancy(); resizePolicy();
        contentScale(); lifecycleFaults(); orderedTeardown(); ownerThread(); refusalState();
        detachedRequestsAndMailbox(); deferredHonoured(); requeriedSize(); constraintSemantics(); destructorInvariant(); repeated();
        std::cout << "PASS editor owner: creation faults, attach/removed lifecycle, bounded constraint policy, deferred "
                     "and idle frame resizes, refused reentry, scale, retained frames, ordered teardown, thread "
                     "refusal, authoritative refusal state, detached-request refusal and mailbox clearing, honoured/counted deferred requests, re-queried sizes, kResultTrue-only constraints, destructor invariant and 1000 balanced cycles\n";
    } catch (const std::exception& error) {
        std::cerr << error.what() << '\n';
        return 1;
    }
}
