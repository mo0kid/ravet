#include <jni.h>
#include "RavetAAudio.hpp"
#include <memory>
#include <cstdio>

// All JNI lifetime/control calls are serialized by the Java owner. MIDI input
// closes before destroy; the audio callback accesses only its native owner.
extern "C" JNIEXPORT jlong JNICALL Java_org_ravet_gain_MainActivity_create(JNIEnv *, jclass) {
    try { return reinterpret_cast<jlong>(new ravet::AAudioHost(ravet_native_state_v1(32))); } catch (...) { return 0; }
}
extern "C" JNIEXPORT void JNICALL Java_org_ravet_gain_MainActivity_destroy(JNIEnv *, jclass, jlong handle) { delete reinterpret_cast<ravet::AAudioHost *>(handle); }
extern "C" JNIEXPORT jboolean JNICALL Java_org_ravet_gain_MainActivity_start(JNIEnv *, jclass, jlong handle, jint rate, jint frames, jint input, jint output) {
    try { return handle && reinterpret_cast<ravet::AAudioHost *>(handle)->start(rate, frames, input, output); } catch (...) { return false; }
}
extern "C" JNIEXPORT void JNICALL Java_org_ravet_gain_MainActivity_stop(JNIEnv *, jclass, jlong handle) { if (handle) reinterpret_cast<ravet::AAudioHost *>(handle)->stop(); }
extern "C" JNIEXPORT jboolean JNICALL Java_org_ravet_gain_MainActivity_set(JNIEnv *, jclass, jlong handle, jint id, jfloat value) { return handle && reinterpret_cast<ravet::AAudioHost *>(handle)->session.set(uint32_t(id), value); }
extern "C" JNIEXPORT jstring JNICALL Java_org_ravet_gain_MainActivity_status(JNIEnv *env, jclass, jlong handle) {
    if (!handle) return env->NewStringUTF("Native instance unavailable");
    auto &host = *reinterpret_cast<ravet::AAudioHost *>(handle);
    char text[256]; std::snprintf(text, sizeof(text), "%d Hz · %d frames · %d channels · input shortages %llu · error %d", host.rate, host.bufferFrames, host.channels,
        static_cast<unsigned long long>(host.inputShortages.load()), host.failure.load());
    return env->NewStringUTF(text);
}
extern "C" JNIEXPORT jint JNICALL Java_org_ravet_gain_MainActivity_failure(JNIEnv *, jclass, jlong handle) {
    return handle ? reinterpret_cast<ravet::AAudioHost *>(handle)->failure.load() : AAUDIO_ERROR_INVALID_STATE;
}
extern "C" JNIEXPORT jboolean JNICALL Java_org_ravet_gain_MainActivity_midi(JNIEnv *env, jclass, jlong handle, jbyteArray data, jint length) {
    if (!handle || !data || length < 1 || length > 3 || env->GetArrayLength(data) < length) return false;
    std::array<uint8_t, 3> bytes{}; env->GetByteArrayRegion(data, 0, length, reinterpret_cast<jbyte *>(bytes.data()));
    return reinterpret_cast<ravet::AAudioHost *>(handle)->midi(bytes.data(), length);
}
