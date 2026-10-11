#include "RavetVst3State.h"
#include <stddef.h>
#include <stdio.h>
_Static_assert(sizeof(RavetVst3StateRecord)==32,"state record");
_Static_assert(sizeof(RavetVst3StateReport)==24,"state report");
_Static_assert(offsetof(RavetVst3StateRecord,class_id)==16,"class UID offset");
_Static_assert(offsetof(RavetVst3StateReport,native_result)==16,"returned diagnostic");
_Static_assert(sizeof(RavetVst3Provider)==40,"unchanged v1");
_Static_assert(sizeof(RavetVst3ProviderV2)==112,"unchanged v2");
_Static_assert(sizeof(RavetVst3StateProviderV1)==32,"independent state companion");
_Static_assert(offsetof(RavetVst3StateProviderV1,provider)==8,"exact paired table");
int main(void){puts("PASS C11 state companion wire and unchanged v1/v2 ABI");return 0;}
