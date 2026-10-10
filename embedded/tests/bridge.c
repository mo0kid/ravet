#include "../../examples/embedded-gain/include/RavetEmbeddedGain.h"
#include <assert.h>
int main(void) {
    RavetEmbeddedGain state;
    _Static_assert(sizeof(state)==16, "reference state layout");
    assert(ravet_embedded_gain_init(&state,48000,64)==0);
    assert(ravet_embedded_gain_set(&state,2)==0);
    float data[]={0.25f,-0.25f,0.5f,-0.5f,99,99};
    assert(ravet_embedded_gain_process(&state,data,6,data,6,2)==0);
    assert(data[0]==0.5f && data[2]==1 && data[4]==99);
    assert(ravet_embedded_gain_process(&state,NULL,4,data,6,2)==-1);
    assert(data[0]==0 && data[3]==0 && data[4]==99);
    assert(ravet_embedded_gain_process(&state,NULL,0,NULL,0,0)==0);
}
