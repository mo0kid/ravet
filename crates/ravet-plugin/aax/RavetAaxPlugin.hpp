#pragma once
// Generic AAX adapter over the RAVET plug-in table (RavetPlugin.h, decision 0281).
//
// One adapter for every product: descriptors, parameters, state, processing,
// MIDI, latency and the editor are read from `ravet_plugin_v1()` at run time.
// The AAX SDK is external and never redistributed; builds are unsigned (PACE
// wrapping is the plug-in vendor's own step). See docs/aax.md.
#include "RavetPlugin.h"
#include "AAX.h"
#include "AAX_CEffectGUI.h"
#include "AAX_CEffectParameters.h"
#include <array>
#include <atomic>
#include <cstdint>
#include <memory>
#include <string>
#include <unordered_map>
#include <vector>

class AAX_ICollection;

namespace ravet::aax::plugin {

// AAX native processing buffers never exceed 1024 frames
// (AAX_eAudioBufferLengthNative_Max).
constexpr uint32_t kMaximumFrames = 1024;
constexpr uint32_t kMaximumEvents = 1024;
// Chunk carrying the RAVET state envelope unchanged ('RvSt').
constexpr AAX_CTypeID kStateChunkId = 0x52765374;
// Algorithm context fields, assigned at run time per layout: the private owner
// pointer, the frame count and the outputs always come first.
constexpr uint32_t kMaximumFields = 6;

// The table the adapter is built against. The default reads ravet_plugin_v1();
// tests may substitute another table before any descriptor is registered.
const RavetPluginEntryV1 *entry();
void setEntryForTesting(const RavetPluginEntryV1 *table);

// Host parameter ID of a table parameter: the SDK master-bypass ID
// (cDefaultMasterBypassID, "MasterBypassID") for the bypass role,
// otherwise "p" followed by the host ID in uppercase hexadecimal.
std::string parameterId(const RavetPluginEntryV1 &table, uint32_t index);
// Channel count to AAX stem format; 0 → None. Only 0, 1 and 2 are mapped.
bool stemFormat(uint32_t channels, AAX_EStemFormat &out);

// Field indices of one registered layout.
struct FieldLayout {
    int32_t inputs = -1, midiInput = -1, midiOutput = -1;
    uint32_t count = 3;
};
FieldLayout fieldLayout(const RavetPluginEntryV1 &table, const RavetPluginLayoutV1 &layout);

// AUDIO-side state shared by the data model and the algorithm callback.
struct Owner {
    const RavetPluginEntryV1 *table = nullptr;
    void *instance = nullptr;
    FieldLayout fields{};
    uint32_t inputs = 0, outputs = 0;
    const AAX_ITransport *transport = nullptr;
    std::atomic<bool> ready{false};
    std::atomic<int32_t> lastStatus{0};
    std::atomic<uint32_t> midiRejected{0}, midiDropped{0};
    // Latency handshake: MAIN asks, AUDIO reads timing() between blocks,
    // MAIN reports the published value to the host.
    std::atomic<bool> timingRequested{false}, timingPublished{false};
    std::atomic<uint32_t> latency{0};
    std::array<RavetNativeEventV1, kMaximumEvents> input{}, output{};
};

// The data model: one per plug-in instance.
class Parameters final : public AAX_CEffectParameters {
public:
    explicit Parameters(const RavetPluginEntryV1 *table);
    ~Parameters() AAX_OVERRIDE;
    static AAX_IEffectParameters *AAX_CALLBACK create();
    // The algorithm callback registered for every layout.
    static void AAX_CALLBACK process(void *const instances[], const void *end);

    AAX_Result EffectInit() AAX_OVERRIDE;
    AAX_Result Uninitialize() AAX_OVERRIDE;
    AAX_Result UpdateParameterNormalizedValue(AAX_CParamID id, double value, AAX_EUpdateSource source) AAX_OVERRIDE;
    AAX_Result ResetFieldData(AAX_CFieldIndex field, void *data, uint32_t size) const AAX_OVERRIDE;
    AAX_Result TimerWakeup() AAX_OVERRIDE;
    AAX_Result NotificationReceived(AAX_CTypeID type, const void *data, uint32_t size) AAX_OVERRIDE;
    AAX_Result GetMasterBypassParameter(AAX_IString *id) const AAX_OVERRIDE;
    AAX_Result GetNumberOfChunks(int32_t *count) const AAX_OVERRIDE;
    AAX_Result GetChunkIDFromIndex(int32_t index, AAX_CTypeID *id) const AAX_OVERRIDE;
    AAX_Result GetChunkSize(AAX_CTypeID id, uint32_t *size) const AAX_OVERRIDE;
    AAX_Result GetChunk(AAX_CTypeID id, AAX_SPlugInChunk *chunk) const AAX_OVERRIDE;
    AAX_Result SetChunk(AAX_CTypeID id, const AAX_SPlugInChunk *chunk) AAX_OVERRIDE;
    AAX_Result CompareActiveChunk(const AAX_SPlugInChunk *chunk, AAX_CBoolean *equal) const AAX_OVERRIDE;

    const RavetPluginEntryV1 *table() const { return table_; }
    void *instance() const { return owner_ ? owner_->instance : nullptr; }
    Owner *owner() const { return owner_.get(); }
    // Report queued editor edits as AAX touch/set/release (MAIN).
    void reportEdits();
    // Publish the table's latency to the host once AUDIO has read it (MAIN).
    void reportLatency();

private:
    bool saveState(std::vector<uint8_t> &out) const;
    void reflectValues();
    const RavetPluginEntryV1 *table_;
    std::unique_ptr<Owner> owner_;
    std::unordered_map<std::string, uint32_t> indices_;
    std::vector<std::string> ids_;
    mutable std::vector<uint8_t> chunk_;
    std::vector<double> readOnly_;
    int32_t reportedLatency_ = -1;
    bool prepared_ = false;
};

// The editor: hosts the product's web editor in the AAX view container.
class Gui final : public AAX_CEffectGUI {
public:
    static AAX_IEffectGUI *AAX_CALLBACK create();
    AAX_Result GetViewSize(AAX_Point *size) const AAX_OVERRIDE;
    AAX_Result TimerWakeup() AAX_OVERRIDE;
protected:
    void CreateViewContents() AAX_OVERRIDE {}
    void CreateViewContainer() AAX_OVERRIDE;
    void DeleteViewContainer() AAX_OVERRIDE;
private:
    Parameters *model() const;
    bool open_ = false;
};

// Registers one effect per supported layout from the table.
AAX_Result describe(AAX_ICollection &collection, const RavetPluginEntryV1 &table);

} // namespace ravet::aax::plugin
