#include "lv2/core/lv2.h"
#include <array>
#include <cmath>
#include <cstring>
#include <dlfcn.h>
#include <iostream>
#include <limits>
#include <stdexcept>
void require(bool okay, const char *message) {
  if (!okay)
    throw std::runtime_error(message);
}
int main(int argc, char **argv) {
  try {
    require(argc == 2, "binary argument");
    auto *library = dlopen(argv[1], RTLD_NOW | RTLD_LOCAL);
    require(library, "load actual binary");
    auto descriptor = reinterpret_cast<const LV2_Descriptor *(*)(uint32_t)>(
        dlsym(library, "lv2_descriptor"));
    require(descriptor && !descriptor(2), "bounded descriptors");
    for (uint32_t index = 0; index < 2; ++index) {
      const auto *d = descriptor(index);
      const uint32_t channels = index + 1;
      const LV2_Feature *features[] = {nullptr};
      require(!d->instantiate(d, 0., "", features), "invalid rate rejection");
      auto instance = d->instantiate(d, 48000., "", features);
      require(instance, "instantiate");
      std::array<float, 65> left, right, outLeft, outRight;
      left.fill(.25f);
      right.fill(-.5f);
      outLeft.fill(99.f);
      outRight.fill(99.f);
      d->connect_port(instance, 0, left.data());
      if (channels == 2)
        d->connect_port(instance, 1, right.data());
      d->connect_port(instance, channels, outLeft.data());
      if (channels == 2)
        d->connect_port(instance, channels + 1, outRight.data());
      float gain = 2.f, bypass = 0.f;
      d->connect_port(instance, 2 * channels, &gain);
      d->connect_port(instance, 2 * channels + 1, &bypass);
      auto check = [&](float l, float r) {
        for (uint32_t i = 0; i < 64; ++i) {
          require(outLeft[i] == l, "left samples");
          if (channels == 2)
            require(outRight[i] == r, "right samples");
        }
        require(outLeft[64] == 99.f && outRight[64] == 99.f, "tails");
      };
      d->activate(instance);
      for (int i = 0; i < 5; ++i)
        d->run(instance, 64);
      check(.5f, -1.f);
      bypass = 1.f;
      d->run(instance, 64);
      check(.25f, -.5f);
      // LV2 host stores control-port values as state; emulate recall at run
      // boundary.
      gain = .5f;
      bypass = 0.f;
      for (int i = 0; i < 5; ++i)
        d->run(instance, 64);
      check(.125f, -.25f);
      gain = 2.f;
      bypass = 1.f;
      d->run(instance, 0);
      d->run(instance, 64);
      check(.25f, -.5f);
      gain = std::numeric_limits<float>::quiet_NaN();
      d->run(instance, 64);
      check(0.f, 0.f);
      gain = 2.f;
      bypass = 0.f;
      d->deactivate(instance);
      d->run(instance, 64);
      check(0.f, 0.f);
      d->activate(instance);
      d->run(instance, 64);
      check(.5f, -1.f);
      d->connect_port(instance, channels, left.data());
      if (channels == 2)
        d->connect_port(instance, channels + 1, right.data());
      d->run(instance, 64);
      require(left[0] == .5f && (channels == 1 || right[0] == -1.f),
              "in-place rendering");
      d->deactivate(instance);
      d->cleanup(instance);
    }
    dlclose(library);
    std::cout << "LV2 mono/stereo actual-binary lifecycle, controls, bypass, "
                 "recall, faults, zero-frame, tails and aliases passed\n";
  } catch (const std::exception &error) {
    std::cerr << error.what() << '\n';
    return 1;
  }
}
