// Public synthetic ABI acceptance; no proprietary resources or format SDK needed.
#include "RavetGain.h"
#include <array>
#include <cmath>
#include <iostream>
#include <limits>
#include <stdexcept>
void check(bool value, const char *reason) {
  if (!value) throw std::runtime_error(reason);
}
int main() {
  try {
    unsigned blocks = 0;
    const auto exact = std::nextafter(1., 2.);
    for (double rate : {44100., 48000., 96000.}) {
      for (unsigned channels : {1u, 2u}) {
        auto *handle = ravet_gain64_create();
        check(handle != nullptr, "create");
        check(ravet_gain64_prepare(handle, rate, channels, 8) == 0, "prepare");
        std::array<double, 9> left{}, right{};
        const double *inputs[]{left.data(), right.data()};
        double *outputs[]{channels == 1 ? left.data() : right.data(), left.data()};
        float gain = 1.; uint32_t bypass = 0;
        for (unsigned iteration = 0; iteration < 1000; ++iteration) {
          left.fill(exact); right.fill(-exact);
          auto frames = iteration % 8 + 1;
          check(ravet_gain64_restore(handle, 0.5f, 0) == 0, "restore");
          check(ravet_gain64_process(handle, inputs, outputs, channels, frames,
                nullptr, 0, &gain, &bypass) == 0, "process");
          check(outputs[0][0] == exact * 0.5, "retained f64 precision");
          if (channels == 2) check(outputs[1][0] == -exact * 0.5, "cross-channel alias");
          check(left[8] == exact && right[8] == -exact, "backing tails");
          ++blocks;
        }
        RavetGainEvent event{0, 2, 1.f};
        check(ravet_gain64_process(handle, nullptr, nullptr, channels, 0,
              &event, 1, &gain, &bypass) == 0 && bypass == 1, "zero-frame bypass");
        ++blocks;
        left.fill(exact); right.fill(-exact);
        check(ravet_gain64_process(handle, inputs, outputs, channels, 8,
              nullptr, 1025, &gain, &bypass) == 1, "overflow rejection");
        for (unsigned channel = 0; channel < channels; ++channel)
          for (unsigned frame = 0; frame < 8; ++frame)
            check(outputs[channel][frame] == 0., "fault silence");
        ravet_gain64_reset(handle);
        ravet_gain64_destroy(handle);
      }
    }
    std::cout << "PASS native f64 C ABI: " << blocks
              << " blocks, six mono/stereo rate cycles, precision/alias/zero-frame/fault checks\n";
    return 0;
  } catch (const std::exception &error) {
    std::cerr << "FAIL native f64 C ABI: " << error.what() << '\n';
    return 1;
  }
}
