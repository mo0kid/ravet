#include "GainParameters.hpp"
#include "RavetAaxFactoryRegistration.hpp"
AAX_Result RavetSdkGetEffectDescriptions(AAX_ICollection *);
namespace {
AAX_IEffectParameters *AAX_CALLBACK mono() { try { return new RavetGainAaxParameters(0); } catch (...) { return nullptr; } }
AAX_IEffectParameters *AAX_CALLBACK stereo() { try { return new RavetGainAaxParameters(1); } catch (...) { return nullptr; } }
}
// The actual SDK ACF registration caller resolves this owned descriptor entry.
// Visibility also permits explicit trusted local binary registration probes;
// this SDK C++ signature is not a separate stable public C ABI.
#if defined(_WIN32)
__declspec(dllexport)
#else
__attribute__((visibility("default")))
#endif
AAX_Result GetEffectDescriptions(AAX_ICollection *host) {
    if (!host) return AAX_ERROR_NULL_ARGUMENT;
    const ravet::aax::NativeAaxParameterFactory profiles[]{
        {"org.ravet.gain.mono", reinterpret_cast<void *>(mono)},
        {"org.ravet.gain.stereo", reinterpret_cast<void *>(stereo)}};
    ravet::aax::NativeAaxFactoryRegistration<2> registration(*host, profiles, 2);
    return registration.valid() ? RavetSdkGetEffectDescriptions(&registration) : AAX_ERROR_INVALID_ARGUMENT;
}
