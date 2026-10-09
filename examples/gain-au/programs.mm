// RAVET-owned reference adapter; external SDK sources remain untouched.
#include "public.sdk/source/vst/auwrapper/auwrapper.h"
#include "AudioUnitSDK/AUPlugInDispatch.h"
#include <cmath>
using namespace Steinberg;
using namespace Steinberg::Vst;
class GainProgramAU final : public AUWrapper {
public:
    explicit GainProgramAU(ComponentInstanceRecord* instance) : AUWrapper(instance) {}
    ComponentResult SetParameter(AudioUnitParameterID id,AudioUnitScope scope,AudioUnitElement element,AudioUnitParameterValue value,UInt32 offset) override {
        if(id!=factoryProgramChangedID)return AUWrapper::SetParameter(id,scope,element,value,offset);
        if(scope!=kAudioUnitScope_Global || element!=0)return kAudioUnitErr_InvalidScope;
        if(!std::isfinite(value) || value<0.f || value>1.f || !presets || numPresets<=0 || offset!=0)
            return kAudioUnitErr_InvalidParameterValue;
        const auto index=static_cast<int32>(std::floor(double(value)*double(numPresets-1)+.5));
        return NewFactoryPresetSet(presets[index]);
    }
    OSStatus NewFactoryPresetSet(const AUPreset& preset) override {
        if(!editController || !presets || numPresets<=0 || preset.presetNumber<0 || preset.presetNumber>=numPresets)
            return kAudioUnitErr_InvalidPropertyValue;
        // The SDK's stock wrapper divides by count. VST3's list parameter uses
        // count-1 intervals, including both endpoints; avoid truncating the last.
        const auto normalized=numPresets==1?0.:double(preset.presetNumber)/double(numPresets-1);
        if(editController->setParamNormalized(factoryProgramChangedID,normalized)!=kResultOk)
            return kAudioUnitErr_InvalidPropertyValue;
        transferParamChanges.addChange(factoryProgramChangedID,normalized,0);
        if(bypassParamID!=kNoParamId) {
            UInt32 bypass=editController->getParamNormalized(bypassParamID)>=.5?1:0;
            const auto status=AUWrapper::SetProperty(kAudioUnitProperty_BypassEffect,kAudioUnitScope_Global,0,&bypass,sizeof(bypass));
            if(status!=noErr)return status;
        }
        SetAFactoryPresetAsCurrent(presets[preset.presetNumber]);
        PropertyChanged(kAudioUnitProperty_PresentPreset,kAudioUnitScope_Global,0);
        PropertyChanged(kAudioUnitProperty_CurrentPreset,kAudioUnitScope_Global,0);
        return noErr;
    }
    ComponentResult SetProperty(AudioUnitPropertyID id,AudioUnitScope scope,AudioUnitElement element,const void* data,UInt32 size) override {
        // Preserve the SDK's document-restore extension through normal AUBase
        // dispatch, alongside its ordinary virtual properties/render callbacks.
        if(id==kAudioUnitProperty_ClassInfoFromDocument) {
            if(scope!=kAudioUnitScope_Global || element!=0)return kAudioUnitErr_InvalidScope;
            if(!data || size!=sizeof(CFPropertyListRef))return kAudioUnitErr_InvalidPropertyValue;
            auto state=*static_cast<const CFPropertyListRef*>(data);
            if(!state)return kAudioUnitErr_InvalidPropertyValue;
            return restoreState(state,true);
        }
        return AUWrapper::SetProperty(id,scope,element,data,size);
    }
};
extern "C" __attribute__((visibility("default"))) void* AUWrapperFactory(const AudioComponentDescription* description) {
    return ausdk::AUBaseFactory<GainProgramAU>::Factory(description);
}
