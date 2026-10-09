#include "RavetGain.h"
#include "RavetGainPrograms.h"
#include <array>
#include <cstring>
#include <iostream>
#include <stdexcept>
void require(bool condition, const char* message) {
  if (!condition) throw std::runtime_error(message);
}
int main() {
  static_assert(sizeof(RavetGainProgramInfo) == 144, "program layout");
  require(ravet_gain_program_count() == 3, "count");
  RavetGainProgramInfo info;
  std::memset(&info, 0xa5, sizeof(info));
  auto original = info;
  require(ravet_gain_program_info(3, &info) == 1, "index rejection");
  require(std::memcmp(&info, &original, sizeof(info)) == 0, "rejected output preservation");
  require(ravet_gain_program_info(0, nullptr) == 1, "null rejection");
  auto* f32 = ravet_gain_create(); auto* f64 = ravet_gain64_create();
  require(f32 && f64, "create");
  require(ravet_gain_prepare(f32, 48000., 1, 4) == 0, "f32 prepare");
  require(ravet_gain64_prepare(f64, 48000., 1, 4) == 0, "f64 prepare");
  const char* names[] = {"Unity", "Silence", "Bypass"};
  for (uint32_t index = 0; index < 3; ++index) {
    require(ravet_gain_program_info(index, &info) == 0, "info");
    require(info.id == index + 1 && info.midi_bank == 0 && info.midi_number == index && info.reserved == 0, "stable metadata");
    require(std::strcmp(info.name, names[index]) == 0 && info.name[127] == 0, "complete name");
    require(info.gain == (index == 1 ? 0.f : 1.f) && info.bypass == (index == 2 ? 1u : 0u), "complete values");
    std::array<uint8_t, 32> record{};
    require(ravet_gain_encode(info.gain, info.bypass, record.data(), record.size()) == 0, "encode");
    float gain = 99; uint32_t bypass = 99;
    require(ravet_gain_decode(record.data(), record.size(), &gain, &bypass) == 0, "decode");
    require(gain == info.gain && bypass == info.bypass, "state values");
    require(ravet_gain_restore(f32, gain, bypass) == 0 && ravet_gain64_restore(f64, gain, bypass) == 0, "restore");
    std::array<float,4> a{1,1,1,1}; std::array<double,4> b{1,1,1,1};
    const float* ia[] = {a.data()}; float* oa[] = {a.data()};
    const double* ib[] = {b.data()}; double* ob[] = {b.data()};
    require(ravet_gain_process(f32,ia,oa,1,4,nullptr,0,&gain,&bypass)==0, "f32 render");
    require(ravet_gain64_process(f64,ib,ob,1,4,nullptr,0,&gain,&bypass)==0, "f64 render");
    for (size_t i=0;i<4;++i) require(a[i]==(index==1?0.f:1.f) && b[i]==(index==1?0.:1.), "program audio");
  }
  ravet_gain_destroy(f32); ravet_gain64_destroy(f64);
  std::cout << "PASS gain program public-header ABI: metadata, rejection, complete state and both precision renders\n";
}
