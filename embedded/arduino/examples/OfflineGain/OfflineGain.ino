#include <RavetEmbedded.h>

// Arduino reference: headless synthetic fixture, no I2S/codec/interrupt setup.
// Keep this instance outside the small callback stack. Serial runs in setup,
// never in a future audio callback.
ravet::embedded::StereoAdapter<ravet::embedded::Gain, 64> audio;

void setup() {
    Serial.begin(115200);
    const float input[] = {0.25f, -0.25f, 0.5f, -0.5f};
    float output[4] = {};
    const ravet::embedded::ParameterEvent event{0, 0, 2.0f};
    const bool prepared = audio.prepare({48000.0, 64, 8});
    const auto status = audio.process(input, 4, output, 4, 2, &event, 1);
    const bool passed = prepared && status == ravet::embedded::Status::Continue
        && output[0] == 0.5f && output[1] == -0.5f && output[2] == 1.0f && output[3] == -1.0f;
    Serial.println(passed ? "RAVET offline gain PASS" : "RAVET offline gain FAIL");
    audio.deactivate();
}
void loop() {}
