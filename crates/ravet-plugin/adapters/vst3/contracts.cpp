#define RAVET_VST3_CONTRACT_TEST 1
#include "adapter.cpp"
#include "public.sdk/source/common/memorystream.h"
#include "public.sdk/source/vst/hosting/hostclasses.h"
#include "public.sdk/source/vst/hosting/parameterchanges.h"
#include "public.sdk/source/vst/hosting/eventlist.h"
#include <cstdio>
#include <cstdlib>
#include <atomic>
#include <new>
// Allocation failure injection: every host-facing method that allocates must
// report failure instead of letting std::bad_alloc unwind into the host.
static std::atomic<bool> failAllocations{false};
void* operator new(std::size_t size) {
    if (failAllocations.load(std::memory_order_relaxed)) throw std::bad_alloc();
    if (void* memory = std::malloc(size ? size : 1)) return memory;
    throw std::bad_alloc();
}
void operator delete(void* memory) noexcept { std::free(memory); }
void operator delete(void* memory, std::size_t) noexcept { std::free(memory); }
namespace ravet_mock {
extern std::array<RavetNativeEventV1, 1024> received;
extern uint32_t receivedCount, processCalls, closed, idled, preparedBuses;
extern RavetPluginProductV1 product;
extern const RavetPluginAuxiliaryBusV1 auxiliaries[2];
}
using namespace ravet::plugin::vst3;
#define CHECK(x) do { if (!(x)) { std::fprintf(stderr, "FAIL line %d: %s\n", __LINE__, #x); std::abort(); } } while (0)
class Handler final : public FObject, public IComponentHandler {
public:
    std::vector<int> edits;
    uint32_t restarts = 0;
    tresult PLUGIN_API beginEdit(ParamID id) override { CHECK(id == 101); edits.push_back(0); return kResultOk; }
    tresult PLUGIN_API performEdit(ParamID id, ParamValue v) override { CHECK(id == 101 && v == .75); edits.push_back(1); return kResultOk; }
    tresult PLUGIN_API endEdit(ParamID id) override { CHECK(id == 101); edits.push_back(2); return kResultOk; }
    tresult PLUGIN_API restartComponent(int32 flags) override { CHECK(flags == kLatencyChanged); ++restarts; return kResultOk; }
    OBJ_METHODS(Handler, FObject)
    DEFINE_INTERFACES DEF_INTERFACE(IComponentHandler) END_DEFINE_INTERFACES(FObject)
    REFCOUNT_METHODS(FObject)
};
// A stereo sidechain and a stereo auxiliary output after the main bus: kAux
// buses, not default-active, fixed arrangements, every bus passed to the table
// with silent inputs and discarded scratch outputs for buses the host leaves out.
static void auxiliaryContracts(FUnknown* host) {
    ravet_mock::product.auxiliary_count = 2; ravet_mock::product.auxiliaries = ravet_mock::auxiliaries;
    CHECK(validTable(entry()));
    auto component = owned(new ravet::plugin::vst3::Component);
    CHECK(component->initialize(host) == kResultOk);
    CHECK(component->getBusCount(kAudio, kInput) == 2 && component->getBusCount(kAudio, kOutput) == 2);
    BusInfo bus{};
    CHECK(component->getBusInfo(kAudio, kInput, 0, bus) == kResultOk && bus.busType == kMain && (bus.flags & BusInfo::kDefaultActive));
    CHECK(component->getBusInfo(kAudio, kInput, 1, bus) == kResultOk && bus.busType == kAux &&
          !(bus.flags & BusInfo::kDefaultActive) && bus.channelCount == 2 && utf8(bus.name) == "Sidechain");
    CHECK(component->getBusInfo(kAudio, kOutput, 1, bus) == kResultOk && bus.busType == kAux &&
          !(bus.flags & BusInfo::kDefaultActive) && bus.channelCount == 2 && utf8(bus.name) == "Aux Out");
    SpeakerArrangement mono = SpeakerArr::kMono, stereo = SpeakerArr::kStereo, arrangement = 0;
    SpeakerArrangement ins[] = {mono, stereo}, outs[] = {mono, stereo}, monoSidechain[] = {mono, mono};
    // Main-only lists set the main bus and keep the declared auxiliary arrangements.
    CHECK(component->setBusArrangements(&mono, 1, &mono, 1) == kResultOk);
    CHECK(component->getBusArrangement(kInput, 0, arrangement) == kResultOk && arrangement == mono);
    CHECK(component->getBusArrangement(kInput, 1, arrangement) == kResultOk && arrangement == stereo);
    CHECK(component->getBusArrangement(kOutput, 0, arrangement) == kResultOk && arrangement == mono);
    CHECK(component->getBusArrangement(kOutput, 1, arrangement) == kResultOk && arrangement == stereo);
    CHECK(component->setBusArrangements(&stereo, 1, &stereo, 1) == kResultOk);
    CHECK(component->getBusArrangement(kInput, 0, arrangement) == kResultOk && arrangement == stereo);
    // Wrong auxiliary arrangements, a mismatched main bus, and partial lists reject.
    SpeakerArrangement threeIns[] = {mono, stereo, stereo};
    CHECK(component->setBusArrangements(monoSidechain, 2, outs, 2) == kResultFalse);
    CHECK(component->setBusArrangements(&mono, 1, &stereo, 1) == kResultFalse);
    CHECK(component->setBusArrangements(ins, 2, &mono, 1) == kResultFalse);
    CHECK(component->setBusArrangements(threeIns, 3, outs, 2) == kResultFalse);
    CHECK(component->getBusArrangement(kInput, 0, arrangement) == kResultOk && arrangement == stereo);
    CHECK(component->setBusArrangements(ins, 2, outs, 2) == kResultOk);
    CHECK(component->getBusArrangement(kInput, 0, arrangement) == kResultOk && arrangement == mono);
    CHECK(component->getBusArrangement(kInput, 1, arrangement) == kResultOk && arrangement == stereo);
    CHECK(component->getBusArrangement(kOutput, 1, arrangement) == kResultOk && arrangement == stereo);
    ProcessSetup setup{}; setup.sampleRate = 48000.; setup.maxSamplesPerBlock = 16; setup.symbolicSampleSize = kSample32;
    CHECK(component->setupProcessing(setup) == kResultOk && ravet_mock::preparedBuses == 3);
    CHECK(component->activateBus(kAudio, kInput, 1, true) == kResultTrue);
    CHECK(component->activateBus(kAudio, kOutput, 1, true) == kResultTrue);
    CHECK(component->activateBus(kAudio, kInput, 2, true) != kResultTrue);
    CHECK(component->setActive(true) == kResultOk && component->setProcessing(true) == kResultOk);
    float mainIn[17], mainOut[17], left[17], right[17], auxLeft[17], auxRight[17];
    std::fill_n(mainIn, 17, 2.f); std::fill_n(left, 17, 3.f); std::fill_n(right, 17, 4.f);
    auto refill = [&] { std::fill_n(mainOut, 17, 99.f); std::fill_n(auxLeft, 17, 99.f); std::fill_n(auxRight, 17, 99.f); };
    float* mainInputs[] = {mainIn}; float* mainOutputs[] = {mainOut};
    float* sidechain[] = {left, right}; float* auxOutputs[] = {auxLeft, auxRight};
    AudioBusBuffers inputs[2]{}, outputs[2]{};
    inputs[0].numChannels = outputs[0].numChannels = 1; inputs[0].channelBuffers32 = mainInputs; outputs[0].channelBuffers32 = mainOutputs;
    inputs[1].numChannels = outputs[1].numChannels = 2; inputs[1].channelBuffers32 = sidechain; outputs[1].channelBuffers32 = auxOutputs;
    ProcessData data{}; data.numSamples = 16; data.symbolicSampleSize = kSample32; data.numInputs = data.numOutputs = 2;
    data.inputs = inputs; data.outputs = outputs;
    auto unchanged = [&] { return auxLeft[0] == 99.f && auxLeft[15] == 99.f && auxRight[15] == 99.f; };
    // Active buses: host buffers reach the processor (aux output copies the sidechain).
    refill(); outputs[1].silenceFlags = 3;
    CHECK(component->process(data) == kResultOk);
    CHECK(mainOut[0] == 2.f && mainOut[15] == 2.f && mainOut[16] == 99.f);
    CHECK(auxLeft[0] == 3.f && auxLeft[15] == 3.f && auxRight[15] == 4.f && auxLeft[16] == 99.f && outputs[1].silenceFlags == 0);
    // Inactive sidechain: the processor receives silence.
    CHECK(component->activateBus(kAudio, kInput, 1, false) == kResultTrue);
    refill(); CHECK(component->process(data) == kResultOk);
    CHECK(auxLeft[0] == 0.f && auxRight[15] == 0.f && auxLeft[16] == 99.f);
    // Fewer buses than declared: the omitted ones use prepared buffers.
    CHECK(component->activateBus(kAudio, kInput, 1, true) == kResultTrue);
    data.numInputs = data.numOutputs = 1;
    refill(); CHECK(component->process(data) == kResultOk && mainOut[15] == 2.f && unchanged());
    data.numInputs = data.numOutputs = 2;
    // Inactive auxiliary output: its host buffer is left alone, scratch is discarded.
    CHECK(component->activateBus(kAudio, kOutput, 1, false) == kResultTrue);
    refill(); CHECK(component->process(data) == kResultOk && mainOut[15] == 2.f && unchanged());
    // Null channel pointers on an active sidechain fall back to silence.
    CHECK(component->activateBus(kAudio, kOutput, 1, true) == kResultTrue);
    inputs[1].channelBuffers32 = nullptr;
    refill(); CHECK(component->process(data) == kResultOk && auxLeft[0] == 0.f && auxRight[15] == 0.f);
    inputs[1].channelBuffers32 = sidechain;
    // An auxiliary output aliasing the main output is rejected and silenced.
    auxOutputs[1] = mainOut;
    refill(); CHECK(component->process(data) == kResultFalse && mainOut[0] == 0.f && mainOut[15] == 0.f && auxLeft[15] == 0.f);
    auxOutputs[1] = auxRight;
    // Zero-frame flush with omitted audio directions.
    ProcessData flush{}; flush.symbolicSampleSize = kSample32;
    CHECK(component->process(flush) == kResultOk);
    CHECK(component->setProcessing(false) == kResultOk && component->setActive(false) == kResultOk);
    // Re-arranging the main bus keeps the auxiliary buses and their activation.
    SpeakerArrangement stereoIns[] = {stereo, stereo}, stereoOuts[] = {stereo, stereo};
    CHECK(component->setBusArrangements(stereoIns, 2, stereoOuts, 2) == kResultOk);
    CHECK(component->getBusInfo(kAudio, kInput, 1, bus) == kResultOk && bus.busType == kAux && utf8(bus.name) == "Sidechain");
    CHECK(component->terminate() == kResultOk);
    ravet_mock::product.auxiliary_count = 0; ravet_mock::product.auxiliaries = nullptr;
}
int main() {
    CHECK(validTable(entry()));
    auto host = owned(new HostApplication);
    auto component = owned(new ravet::plugin::vst3::Component); auto controller = owned(new Controller);
    auto handler = owned(new Handler);
    CHECK(component->initialize(host) == kResultOk);
    CHECK(controller->initialize(host) == kResultOk);
    CHECK(controller->setComponentHandler(handler) == kResultOk);
    CHECK(component->connect(controller) == kResultOk);
    CHECK(controller->connect(component) == kResultOk);
    CHECK(controller->getParameterCount() == 5 + 16 * 130);
    ParameterInfo info{}; CHECK(controller->getParameterInfo(1, info) == kResultOk);
    CHECK(info.id == 102 && info.stepCount == 1 && (info.flags & ParameterInfo::kIsBypass));
    CHECK(controller->getParameterInfo(2, info) == kResultOk && info.stepCount == 3);
    CHECK(controller->getParameterInfo(3, info) == kResultOk && (info.flags & ParameterInfo::kIsHidden));
    CHECK(controller->getParameterInfo(4, info) == kResultOk && (info.flags & ParameterInfo::kIsReadOnly));
    CHECK(controller->normalizedParamToPlain(101, .75) == 1.5);
    CHECK(controller->plainParamToNormalized(101, 1.5) == .75);
    String128 text{}; CHECK(controller->getParamStringByValue(101, .75, text) == kResultOk);
    double value = 0.; CHECK(controller->getParamValueByString(101, text, value) == kResultOk && value == .75);
    CHECK(controller->setParamNormalized(101, .25) == kResultOk);
    MemoryStream state; CHECK(component->getState(&state) == kResultOk);
    CHECK(state.getSize() == 48 && !std::memcmp(state.getData(), "MOCK0001", 8));
    double saved = 0.; std::memcpy(&saved, static_cast<char*>(state.getData()) + 8, 8); CHECK(saved == .5);
    CHECK(controller->setParamNormalized(103, .51) == kResultOk && controller->getParamNormalized(103) == 2. / 3.);
    SpeakerArrangement mono = SpeakerArr::kMono, stereo = SpeakerArr::kStereo, bad = SpeakerArr::k51;
    CHECK(component->setBusArrangements(&bad, 1, &bad, 1) == kResultFalse);
    CHECK(component->setBusArrangements(&mono, 1, &stereo, 1) == kResultFalse);
    CHECK(component->setBusArrangements(&mono, 1, &mono, 1) == kResultOk);
    ProcessSetup setup{}; setup.sampleRate = 48000.; setup.maxSamplesPerBlock = 16; setup.symbolicSampleSize = kSample32;
    CHECK(component->setupProcessing(setup) == kResultOk);
    auto rejected = setup; rejected.sampleRate = 1234567.8;
    CHECK(component->setupProcessing(rejected) == kResultFalse);
    CHECK(component->canProcessSampleSize(kSample64) == kResultFalse);
    CHECK(component->setActive(true) == kResultOk);
    CHECK(component->getTailSamples() == 24000);
    CHECK(component->setProcessing(true) == kResultOk);
    float input[17], output[17]; std::fill_n(input, 17, 2.f); std::fill_n(output, 17, 99.f);
    float* ip[] = {input}; float* op[] = {output};
    AudioBusBuffers in{}, out{}; in.numChannels = out.numChannels = 1; in.channelBuffers32 = ip; out.channelBuffers32 = op;
    ParameterChanges changes; int32 qi = 0, pi = 0;
    auto gain = changes.addParameterData(101, qi); CHECK(gain); gain->addPoint(3, .75, pi);
    auto bypass = changes.addParameterData(102, qi); CHECK(bypass); bypass->addPoint(8, 1., pi);
    ParamID cc = 0, bend = 0, pressure = 0;
    CHECK(controller->getMidiControllerAssignment(0, 2, 74, cc) == kResultOk);
    CHECK(controller->getMidiControllerAssignment(0, 2, kPitchBend, bend) == kResultOk);
    CHECK(controller->getMidiControllerAssignment(0, 2, kAfterTouch, pressure) == kResultOk);
    CHECK(controller->getMidiControllerAssignment(1, 2, 74, cc) == kResultFalse);
    changes.addParameterData(cc, qi)->addPoint(4, 1., pi);
    changes.addParameterData(bend, qi)->addPoint(5, 1., pi);
    changes.addParameterData(pressure, qi)->addPoint(6, 1., pi);
    EventList events, generated;
    Event note{}; note.busIndex = 0; note.sampleOffset = 4; note.type = Event::kNoteOnEvent;
    note.noteOn = {2, 60, 0.f, .5f, 0, 7}; events.addEvent(note);
    note.sampleOffset = 12; note.type = Event::kNoteOffEvent; note.noteOff = {2, 60, .25f, 7, 0.f}; events.addEvent(note);
    ProcessData data{}; data.numSamples = 16; data.symbolicSampleSize = kSample32; data.numInputs = data.numOutputs = 1;
    data.inputs = &in; data.outputs = &out; data.inputParameterChanges = &changes; data.inputEvents = &events; data.outputEvents = &generated;
    CHECK(component->process(data) == kResultOk);
    CHECK(output[0] == 1.f && output[3] == 3.f && output[7] == 3.f && output[8] == 2.f && output[16] == 99.f);
    CHECK(ravet_mock::receivedCount == 7);
    CHECK(ravet_mock::received[1].kind == RAVET_NATIVE_MIDI && ravet_mock::received[1].offset == 4 && ravet_mock::received[1].words[0] == 0x7f4ab2);
    CHECK(ravet_mock::received[2].kind == RAVET_NATIVE_NOTE_ON_WITH_ID && ravet_mock::received[2].note_id == 7);
    CHECK(generated.getEventCount() == 5);
    Event result{}; generated.getEvent(0, result); CHECK(result.type == Event::kLegacyMIDICCOutEvent && result.sampleOffset == 4 && result.midiCCOut.controlNumber == 74);
    generated.getEvent(1, result); CHECK(result.type == Event::kNoteOnEvent && result.noteOn.noteId == 7);
    ParameterChanges latencyChanges;
    latencyChanges.addParameterData(103, qi)->addPoint(0, 1., pi);
    data.inputParameterChanges = &latencyChanges; data.inputEvents = nullptr;
    CHECK(component->process(data) == kResultOk && component->getLatencySamples() == 32);
    CHECK(handler->restarts == 0); controller->idle(); CHECK(handler->restarts == 1);
    controller->idle(); CHECK(handler->restarts == 1);
    data.inputParameterChanges = &changes; data.inputEvents = &events;
    changes.addParameterData(101, qi)->addPoint(16, .5, pi);
    auto calls = ravet_mock::processCalls;
    CHECK(component->process(data) == kResultFalse && ravet_mock::processCalls == calls);
    CHECK(output[0] == 0.f && output[15] == 0.f && output[16] == 99.f);
    CHECK(component->setProcessing(false) == kResultOk);
    CHECK(component->setActive(false) == kResultOk);
    state.seek(0, IBStream::kIBSeekSet, nullptr); CHECK(component->setState(&state) == kResultOk);
    state.seek(0, IBStream::kIBSeekSet, nullptr); CHECK(controller->setComponentState(&state) == kResultOk);
    CHECK(controller->getParamNormalized(101) == .25);
    // Zero-frame flush with omitted audio directions; state remains byte-for-byte.
    CHECK(component->setBusArrangements(&stereo, 1, &stereo, 1) == kResultOk);
    CHECK(component->setActive(true) == kResultOk);
    ProcessData flush{}; flush.symbolicSampleSize = kSample32;
    CHECK(component->process(flush) == kResultOk);
    MemoryStream restored; CHECK(component->getState(&restored) == kResultOk);
    CHECK(restored.getSize() == state.getSize() && !std::memcmp(restored.getData(), state.getData(), size_t(state.getSize())));
    CHECK(component->setActive(false) == kResultOk);
    // Short streams reject without replacing the accepted envelope.
    MemoryStream invalid; const char badState[] = "bad"; invalid.write(const_cast<char*>(badState), 3, nullptr);
    invalid.seek(0, IBStream::kIBSeekSet, nullptr); CHECK(component->setState(&invalid) == kResultFalse);
    // Out of memory: refused, never thrown into the host; nothing is replaced.
    failAllocations = true;
    state.seek(0, IBStream::kIBSeekSet, nullptr);
    const bool stateRefused = component->setState(&state) == kResultFalse;
    MemoryStream unsaved; const bool saveRefused = component->getState(&unsaved) == kResultFalse;
    state.seek(0, IBStream::kIBSeekSet, nullptr);
    const bool controllerRefused = controller->setState(&state) == kResultFalse;
    const bool noComponent = ravet::plugin::vst3::Component::create(nullptr) == nullptr, noController = Controller::create(nullptr) == nullptr;
    const bool noView = controller->createView(ViewType::kEditor) == nullptr;
    failAllocations = false;
    CHECK(stateRefused && saveRefused && controllerRefused && noComponent && noController && noView);
    MemoryStream intact; CHECK(component->getState(&intact) == kResultOk);
    CHECK(intact.getSize() == state.getSize() && !std::memcmp(intact.getData(), state.getData(), size_t(state.getSize())));
    auto view = owned(controller->createView(ViewType::kEditor)); CHECK(view);
#if SMTG_OS_MACOS
    const auto platform = kPlatformTypeNSView;
#elif SMTG_OS_WINDOWS
    const auto platform = kPlatformTypeHWND;
#else
    const auto platform = kPlatformTypeX11EmbedWindowID;
#endif
    CHECK(view->attached(reinterpret_cast<void*>(1), platform) == kResultOk);
    ViewRect constraint(0, 0, 1, 9000); CHECK(view->checkSizeConstraint(&constraint) == kResultOk);
    CHECK(constraint.getWidth() == 400 && constraint.getHeight() == 960);
    ViewRect rect(0, 0, 600, 300); CHECK(view->onSize(&rect) == kResultOk);
    controller->idle(); CHECK(ravet_mock::idled == 1 && handler->edits == std::vector<int>({0, 1, 2}));
    MemoryStream edited; CHECK(component->getState(&edited) == kResultOk);
    std::memcpy(&saved, static_cast<char*>(edited.getData()) + 8, 8); CHECK(saved == 1.5);
    CHECK(controller->disconnect(component) == kResultOk); CHECK(component->disconnect(controller) == kResultOk);
    CHECK(controller->terminate() == kResultOk); controller = nullptr;
    CHECK(ravet_mock::closed == 1);
    CHECK(view->onSize(&rect) == kResultFalse); CHECK(view->removed() == kResultOk);
    CHECK(component->terminate() == kResultOk);
    auxiliaryContracts(host);
    std::puts("PASS: generic VST3 contracts (metadata, layouts, auxiliary buses, timed audio/automation, MIDI/notes, envelope, edits, retained view)");
}
