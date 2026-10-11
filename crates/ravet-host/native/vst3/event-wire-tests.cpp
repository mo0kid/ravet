#include "EventWire.hpp"
#include <iostream>
#include <stdexcept>
#include <limits>
using namespace ravet::vst3::processing_draft;
static void require(bool value,const char* text){if(!value)throw std::runtime_error(text);}
static Record initial(uint16_t type){
    Record result{};result.struct_size=sizeof(result);result.type=type;
    result.note_id=17;result.pitch=60;result.velocity=.75f;result.value=.25;
    result.expression_id=4;result.root=60;result.bass=48;
    result.integer_value=UINT64_MAX;return result;
}
static void scalarDomains(){
    EventBuses buses{};buses[0]=2;buses[15]=16;
    for(uint16_t type:{uint16_t(0),uint16_t(1),uint16_t(2),uint16_t(3),uint16_t(4),
            uint16_t(5),uint16_t(6),uint16_t(7),uint16_t(8),uint16_t(65535)}){
        auto record=initial(type);
        require(validate(record,nullptr,0,32,buses,true),"all ten SDK kinds admit their valid scalar/empty payload domains");
        require(validate(record,nullptr,0,32,buses,false)==(type!=65535),"legacy controller event is strictly output-only");
        record.offset=31;record.bus=15;record.channel=15;record.flags=0xc001;
        require(validate(record,nullptr,0,32,buses,true),"boundary bus/channel/offset/live/private bits preserved");
        record.offset=32;require(!validate(record,nullptr,0,32,buses,true),"block endpoint rejected");
        record.offset=0;require(validate(record,nullptr,0,0,buses,true),"zero-block event uses offset zero");
        record.offset=1;require(!validate(record,nullptr,0,0,buses,true),"zero-block nonzero offset refused");
    }
    for(int32_t id:{INT32_MIN,-10000,-1000,-2,-1,0,INT32_MAX}){
        auto record=initial(0);record.note_id=id;
        require(validate(record,nullptr,0,32,buses,true),"complete signed output note identity domain preserved");
        require(validate(record,nullptr,0,32,buses,false)==(id>=-1),"host does not invent negative plugin note identities");
        record.type=4;
        require(validate(record,nullptr,0,32,buses,false)==(id>=0),"input expression has concrete note identity");
    }
    auto record=initial(8);record.integer_value=UINT64_MAX;
    require(validate(record,nullptr,0,32,buses,false)&&record.integer_value==UINT64_MAX,"integer expression never coerced to double");
    record=initial(6);record.mask=0xffff;require(validate(record,nullptr,0,32,buses,false),"event mask is sixteen bits, not context chord mask");
    record.mask=0x10000;require(!validate(record,nullptr,0,32,buses,false),"mask narrowing cannot silently truncate");
    record=initial(0);uint32_t nan=0x7f800001;std::memcpy(&record.velocity,&nan,4);
    require(!validate(record,nullptr,0,32,buses,false),"foreign signaling NaN velocity rejected under fast math");
    uint32_t inf=0x7f800000;std::memcpy(&record.tuning,&inf,4);record.velocity=.5f;
    require(!validate(record,nullptr,0,32,buses,false),"foreign infinite tuning rejected under fast math");
    record=initial(4);uint64_t wide=UINT64_C(0x7ff8000000000001);std::memcpy(&record.value,&wide,8);
    require(!validate(record,nullptr,0,32,buses,false),"foreign NaN expression rejected under fast math");
    record=initial(0);record.bus=16;require(!validate(record,nullptr,0,32,buses,false),"complete bus index checked before array access");
    record.bus=1;require(!validate(record,nullptr,0,32,buses,false),"unselected bus cannot receive an event");
    record.bus=0;record.flags=2;require(!validate(record,nullptr,0,32,buses,false),"undefined flags refused");
    record.flags=0;require(!validate(record,nullptr,0,65537,buses,false),"bounded frame profile remains explicit");
}
static void payloadDomains(){
    EventBuses buses{};buses[0]=16;
    // Deliberately byte-misaligned UTF-16 source, including a supplementary scalar.
    std::array<uint8_t,10> bytes{};const uint16_t text[]{'A',0xd83d,0xde00,'B'};
    std::memcpy(bytes.data()+1,text,sizeof(text));
    auto record=initial(5);record.payload_offset=1;record.payload_bytes=8;
    require(validate(record,bytes.data(),10,32,buses,false),"copied counted Unicode payload need not be source-aligned");
    uint32_t end=99;require(storedPayloadEnd(record,1,end)&&end==12,"packed UTF-16 alignment and owned terminator included in cap");
    record.payload_bytes=7;require(!validate(record,bytes.data(),10,32,buses,false),"partial UTF-16 code unit refused");
    record.payload_bytes=4;require(!validate(record,bytes.data(),10,32,buses,false),"split supplementary pair refused");
    record.payload_offset=3;record.payload_bytes=2;require(!validate(record,bytes.data(),10,32,buses,false),"isolated high surrogate refused");
    record.payload_offset=5;require(!validate(record,bytes.data(),10,32,buses,false),"isolated low surrogate refused");
    const uint8_t nul[]{0,0};record.payload_offset=0;record.payload_bytes=2;
    require(!validate(record,nul,2,32,buses,false),"embedded NUL cannot disagree with counted text");
    record.payload_offset=UINT32_MAX;record.payload_bytes=UINT32_MAX;
    require(!validate(record,bytes.data(),10,32,buses,false),"overflow-safe complete payload range");
    std::array<uint16_t,4097> large{};large.fill('x');record=initial(5);record.payload_bytes=8192;
    require(validate(record,reinterpret_cast<const uint8_t*>(large.data()),8194,32,buses,false),"exact 4096-code-unit text cap");
    record.payload_bytes=8194;require(!validate(record,reinterpret_cast<const uint8_t*>(large.data()),8194,32,buses,false),"one unit over cap rejected");
    std::array<uint8_t,65536> sysex{};sysex[0]=0xf0;sysex[1]=0xf8;sysex[65535]=0xf7;
    record=initial(2);record.payload_bytes=65536;
    require(validate(record,sysex.data(),65536,32,buses,false)&&storedPayloadEnd(record,0,end)&&end==65536,"opaque full SysEx carrier preserves framing/real-time bytes");
    require(!storedPayloadEnd(record,1,end),"aggregate byte bound rejects before destination write");
    record=initial(5);record.payload_bytes=0;end=77;
    require(!storedPayloadEnd(record,65535,end)&&end==77,"empty text still needs aligned terminating storage; rejected result unchanged");
    require(storedPayloadEnd(record,65534,end)&&end==65536,"exact final owned empty text terminator fits");
    require(!validate(record,nullptr,1,32,buses,false),"nonempty declared source span requires a live pointer");
}
int main(){try{scalarDomains();payloadDomains();std::cout<<"VST3 ten-kind scalar/identity/Unicode/payload admission PASS\n";return 0;}catch(const std::exception& error){std::cerr<<error.what()<<'\n';return 1;}}
