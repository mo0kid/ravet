#pragma once
// Scriptable test double, never included in Android builds. Its deliberately
// small declarations model only APIs used by the backend, not the Android SDK.
#include <stdint.h>
typedef int32_t aaudio_result_t;
typedef int32_t aaudio_data_callback_result_t;
struct AAudioStream;
struct AAudioStreamBuilder;
enum { AAUDIO_OK = 0, AAUDIO_DIRECTION_OUTPUT = 0, AAUDIO_DIRECTION_INPUT = 1,
       AAUDIO_FORMAT_PCM_FLOAT = 2, AAUDIO_PERFORMANCE_MODE_LOW_LATENCY = 12,
       AAUDIO_SHARING_MODE_SHARED = 1, AAUDIO_ERROR_INVALID_FORMAT = -1,
       AAUDIO_ERROR_INTERNAL = -2, AAUDIO_ERROR_INVALID_STATE = -3,
       AAUDIO_CALLBACK_RESULT_STOP = 1, AAUDIO_CALLBACK_RESULT_CONTINUE = 0 };
typedef aaudio_data_callback_result_t (*AAudioStream_dataCallback)(AAudioStream *, void *, void *, int32_t);
typedef void (*AAudioStream_errorCallback)(AAudioStream *, void *, aaudio_result_t);
aaudio_result_t AAudio_createStreamBuilder(AAudioStreamBuilder **);
void AAudioStreamBuilder_setDirection(AAudioStreamBuilder *, int32_t);
void AAudioStreamBuilder_setFormat(AAudioStreamBuilder *, int32_t);
void AAudioStreamBuilder_setChannelCount(AAudioStreamBuilder *, int32_t);
void AAudioStreamBuilder_setSampleRate(AAudioStreamBuilder *, int32_t);
void AAudioStreamBuilder_setDeviceId(AAudioStreamBuilder *, int32_t);
void AAudioStreamBuilder_setPerformanceMode(AAudioStreamBuilder *, int32_t);
void AAudioStreamBuilder_setSharingMode(AAudioStreamBuilder *, int32_t);
void AAudioStreamBuilder_setDataCallback(AAudioStreamBuilder *, AAudioStream_dataCallback, void *);
void AAudioStreamBuilder_setErrorCallback(AAudioStreamBuilder *, AAudioStream_errorCallback, void *);
aaudio_result_t AAudioStreamBuilder_openStream(AAudioStreamBuilder *, AAudioStream **);
aaudio_result_t AAudioStreamBuilder_delete(AAudioStreamBuilder *);
int32_t AAudioStream_getSampleRate(AAudioStream *);
int32_t AAudioStream_getChannelCount(AAudioStream *);
int32_t AAudioStream_getFormat(AAudioStream *);
aaudio_result_t AAudioStream_setBufferSizeInFrames(AAudioStream *, int32_t);
aaudio_result_t AAudioStream_requestStart(AAudioStream *);
aaudio_result_t AAudioStream_requestStop(AAudioStream *);
aaudio_result_t AAudioStream_close(AAudioStream *);
aaudio_result_t AAudioStream_read(AAudioStream *, void *, int32_t, int64_t);
