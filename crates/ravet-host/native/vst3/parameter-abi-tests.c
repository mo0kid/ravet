#include "RavetVst3Parameters.h"
#include "RavetVst3State.h"
#include <stddef.h>
#include <stdio.h>
_Static_assert(sizeof(RavetVst3Provider)==40,"unchanged catalogue table");
_Static_assert(sizeof(RavetVst3ProviderV2)==112,"unchanged instance table");
_Static_assert(sizeof(RavetVst3StateProviderV1)==32,"unchanged state table");
_Static_assert(sizeof(RavetVst3ParameterPoint)==16,"point wire");
_Static_assert(offsetof(RavetVst3ParameterPoint,value)==8,"point normalized value");
_Static_assert(sizeof(RavetVst3ParameterRecord)==1568,"copied UTF8 metadata wire");
_Static_assert(offsetof(RavetVst3ParameterRecord,default_normalized)==24,"metadata normalized default");
_Static_assert(offsetof(RavetVst3ParameterRecord,title)==32,"metadata title");
_Static_assert(sizeof(RavetVst3ParameterReport)==24,"report wire");
_Static_assert(offsetof(RavetVst3ParameterReport,native_result)==20,"native diagnostic wire");
_Static_assert(sizeof(RavetVst3ParameterProviderV1)==72,"independent parameter table");
_Static_assert(offsetof(RavetVst3ParameterProviderV1,provider)==8,"exact instance table identity");
_Static_assert(offsetof(RavetVst3ParameterProviderV1,process)==64,"parameter audio callback");
int main(void){puts("PASS C11 parameter metadata/points/reports and unchanged catalogue/instance/state ABI");return 0;}
