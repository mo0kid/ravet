#include "RavetAaxEffectParameters.hpp"
#ifndef RAVET_TEST_OUTPUT_CAPACITY
#define RAVET_TEST_OUTPUT_CAPACITY 16
#endif
#ifndef RAVET_TEST_SAMPLE_BITS
#define RAVET_TEST_SAMPLE_BITS 32
#endif
using Profile=ravet::aax::NativeAaxProcessor<1,32,true,1024,true,RAVET_TEST_OUTPUT_CAPACITY,false,RAVET_TEST_SAMPLE_BITS,true>;
using Model=ravet::aax::NativeAaxEffectParameters<1,32,true,1024,true,false,RAVET_TEST_OUTPUT_CAPACITY,false,RAVET_TEST_SAMPLE_BITS,true>;
static_assert(sizeof(Profile)>0 && sizeof(Model)>0);
