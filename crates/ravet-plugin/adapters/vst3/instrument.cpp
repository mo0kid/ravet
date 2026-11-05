// Synthetic public-SDK host of the real Evidence Synth through the same adapter
// classes as the bundle. The SDK validator separately loads the final module.
#define RAVET_VST3_CONTRACT_TEST 1
#include "adapter.cpp"
#include "public.sdk/source/vst/hosting/hostclasses.h"
#include "public.sdk/source/vst/hosting/parameterchanges.h"
#include "public.sdk/source/vst/hosting/eventlist.h"
#include <cstdio>
#include <cstdlib>
using namespace ravet::plugin::vst3;
#define CHECK(x) do { if (!(x)) { std::fprintf(stderr,"FAIL instrument line %d: %s\n",__LINE__,#x); std::abort(); } } while (0)
class Handler final : public FObject, public IComponentHandler {
public:
    uint32_t restarts=0;
    tresult PLUGIN_API beginEdit(ParamID) override { return kResultOk; }
    tresult PLUGIN_API performEdit(ParamID,ParamValue) override { return kResultOk; }
    tresult PLUGIN_API endEdit(ParamID) override { return kResultOk; }
    tresult PLUGIN_API restartComponent(int32 flags) override { CHECK(flags==kLatencyChanged); ++restarts; return kResultOk; }
    OBJ_METHODS(Handler,FObject)
    DEFINE_INTERFACES DEF_INTERFACE(IComponentHandler) END_DEFINE_INTERFACES(FObject)
    REFCOUNT_METHODS(FObject)
};
int main() {
    CHECK(validTable(entry()) && !std::strcmp(entry()->product->plugin_id,"org.ravet.evidence-synth"));
    auto host=owned(new HostApplication); auto handler=owned(new Handler);
    auto component=owned(new ravet::plugin::vst3::Component); auto controller=owned(new Controller);
    CHECK(component->initialize(host)==kResultOk && controller->initialize(host)==kResultOk);
    CHECK(controller->setComponentHandler(handler)==kResultOk);
    CHECK(component->connect(controller)==kResultOk && controller->connect(component)==kResultOk);
    CHECK(component->getBusCount(kAudio,kInput)==0 && component->getBusCount(kAudio,kOutput)==1);
    CHECK(component->getBusCount(kEvent,kInput)==1 && component->getBusCount(kEvent,kOutput)==1);
    ParamID cc=0; CHECK(controller->getMidiControllerAssignment(0,0,1,cc)==kResultOk);
    ParameterInfo info{}; bool hidden=false;
    for (int32 i=0;i<controller->getParameterCount();++i) if (controller->getParameterInfo(i,info)==kResultOk && info.id==cc) hidden=info.flags&ParameterInfo::kIsHidden;
    CHECK(hidden);
    ProcessSetup setup{}; setup.sampleRate=48000.; setup.maxSamplesPerBlock=128; setup.symbolicSampleSize=kSample32;
    CHECK(component->setupProcessing(setup)==kResultOk && component->setActive(true)==kResultOk && component->setProcessing(true)==kResultOk);
    CHECK(component->getLatencySamples()==32 && component->getTailSamples()==96);
    std::array<float,129> left{},right{}; left.fill(99);right.fill(99); float *pointers[]={left.data(),right.data()};
    AudioBusBuffers out{}; out.numChannels=2;out.channelBuffers32=pointers;
    ParameterChanges changes; int32 qi=0,pi=0; changes.addParameterData(cc,qi)->addPoint(15,64./127.,pi);
    EventList input,output; Event note{}; note.busIndex=0;note.sampleOffset=7;note.type=Event::kNoteOnEvent;
    note.noteOn={0,60,0.f,1.f,0,17};CHECK(input.addEvent(note)==kResultOk);
    note.sampleOffset=17;note.type=Event::kNoteOffEvent;note.noteOff={0,60,0.f,17,0.f};CHECK(input.addEvent(note)==kResultOk);
    ProcessData data{};data.numSamples=128;data.symbolicSampleSize=kSample32;data.numOutputs=1;data.outputs=&out;
    data.inputParameterChanges=&changes;data.inputEvents=&input;data.outputEvents=&output;
    CHECK(component->process(data)==kResultOk);
    for (size_t f=0;f<128;++f) {
        const float expected=f>=39 && f<103 ? ((f-39)%2 ? -.25f : .25f)*(f<47?1.f:64.f/127.f) : 0.f;
        CHECK(left[f]==expected && right[f]==expected);
    }
    CHECK(left[128]==99 && right[128]==99 && output.getEventCount()==3);
    Event e{};CHECK(output.getEvent(0,e)==kResultOk && e.type==Event::kNoteOnEvent && e.sampleOffset==7 && e.noteOn.noteId==17 && e.noteOn.pitch==60 && e.noteOn.velocity==1.f);
    CHECK(output.getEvent(1,e)==kResultOk && e.type==Event::kLegacyMIDICCOutEvent && e.sampleOffset==15 && e.midiCCOut.controlNumber==1 && e.midiCCOut.value==64);
    CHECK(output.getEvent(2,e)==kResultOk && e.type==Event::kNoteOffEvent && e.sampleOffset==17 && e.noteOff.noteId==17 && e.noteOff.pitch==60);
    ParameterChanges latency;latency.addParameterData(2,qi)->addPoint(0,0.,pi);data.inputParameterChanges=&latency;data.inputEvents=nullptr;
    CHECK(component->process(data)==kResultOk && component->getLatencySamples()==0 && component->getTailSamples()==64);
    controller->idle();CHECK(handler->restarts==1);
    CHECK(component->setProcessing(false)==kResultOk && component->setActive(false)==kResultOk);
    CHECK(controller->disconnect(component)==kResultOk && component->disconnect(controller)==kResultOk);
    CHECK(controller->terminate()==kResultOk && component->terminate()==kResultOk);
    std::puts("PASS real-product VST3 adapter: hidden CC mapping, note IDs/output, exact audio/tail, latency change notification");
}
