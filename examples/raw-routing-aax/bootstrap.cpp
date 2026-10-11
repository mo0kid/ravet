// The external wrapper supplies AAX module infrastructure and calls the public
// Steinberg factory bootstrap referenced by wrapper infrastructure. The AAX-only reference
// exposes zero VST3 classes: its native factory/algorithm is registered separately.
#include "public.sdk/source/main/pluginfactory.h"
using namespace Steinberg;
BEGIN_FACTORY("RAVET", "", "", PFactoryInfo::kNoFlags)
END_FACTORY
