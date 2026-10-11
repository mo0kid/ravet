#include "RavetVst3HostedAudio.h"
#include <stddef.h>
_Static_assert(sizeof(RavetVst3HostedAudioBusRequest)==16,"bus request size");
_Static_assert(sizeof(RavetVst3HostedAudioProfile)==544,"fixed profile size");
_Static_assert(sizeof(RavetVst3HostedAudioReport)==24,"control report size");
_Static_assert(sizeof(RavetVst3HostedAudioBus)==416,"complete copied bus size");
_Static_assert(sizeof(RavetVst3HostedAudioTiming)==24,"timing size");
_Static_assert(offsetof(RavetVst3HostedAudioProviderV1,instance_origin)==8,"origin offset");
_Static_assert(offsetof(RavetVst3HostedAudioProviderV1,processing_origin)==8+sizeof(void*),"processing origin offset");
int main(void){return 0;}
