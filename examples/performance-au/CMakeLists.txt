cmake_minimum_required(VERSION 3.25)
project(ravet_performance_au VERSION 0.1.0 LANGUAGES C CXX OBJCXX)
if(NOT APPLE OR NOT EXISTS "${RAVET_AUDIOUNIT_SDK}/include/AudioUnitSDK/MusicDeviceBase.h" OR NOT EXISTS "${RAVET_AUDIOUNIT_LIBRARY}" OR NOT EXISTS "${RAVET_PERFORMANCE_LIBRARY}")
    message(FATAL_ERROR "Supply macOS, external AudioUnitSDK/library and public performance native archive")
endif()
# Share external SDK/VST3 configuration, without changing the Gain targets.
add_subdirectory(../gain-vst3 framework-build)
add_library(ravet_performance_au MODULE PerformanceAU.mm
    "${RAVET_VST3_SDK}/public.sdk/source/vst/auwrapper/aucocoaview.mm"
    "${RAVET_VST3_SDK}/public.sdk/source/vst/auwrapper/NSDataIBStream.mm")
target_compile_features(ravet_performance_au PRIVATE cxx_std_23)
target_compile_definitions(ravet_performance_au PRIVATE SMTG_AUWRAPPER_USES_AUSDK CA_USE_AUDIO_PLUGIN_ONLY=0 SMTG_AUCocoaUIBase_CLASS_NAME=RavetPerformanceAUCocoaView)
target_include_directories(ravet_performance_au PRIVATE "${RAVET_VST3_SDK}" "${RAVET_AUDIOUNIT_SDK}/include" ../performance-native/include ../../crates/ravet-native/include)
target_link_libraries(ravet_performance_au PRIVATE sdk_hosting "${RAVET_AUDIOUNIT_LIBRARY}" "${RAVET_PERFORMANCE_LIBRARY}"
    "-framework AudioUnit" "-framework CoreMIDI" "-framework AudioToolbox" "-framework CoreFoundation"
    "-framework Carbon" "-framework Cocoa" "-framework CoreAudio" "-framework Security" "-framework SystemConfiguration" iconv)
set_target_properties(ravet_performance_au PROPERTIES BUNDLE TRUE BUNDLE_EXTENSION component
    MACOSX_BUNDLE_INFO_PLIST "${CMAKE_CURRENT_SOURCE_DIR}/Info.plist.in" LIBRARY_OUTPUT_DIRECTORY "${CMAKE_BINARY_DIR}/AU")
add_dependencies(ravet_performance_au ravet_performance_vst3)

add_executable(ravet_au_performance_contracts ../../tools/au-validation/performance.mm)
target_compile_features(ravet_au_performance_contracts PRIVATE cxx_std_17)
target_link_libraries(ravet_au_performance_contracts PRIVATE "-framework AudioToolbox" "-framework CoreFoundation")
add_executable(ravet_au_native_parameters_contracts ../../tools/au-validation/native-parameters.cpp)
target_compile_features(ravet_au_native_parameters_contracts PRIVATE cxx_std_17)
target_include_directories(ravet_au_native_parameters_contracts PRIVATE ../../crates/ravet-native/include ../gain-native/include ../performance-native/include)
target_link_libraries(ravet_au_native_parameters_contracts PRIVATE "${RAVET_GAIN_LIBRARY}" "${RAVET_PERFORMANCE_LIBRARY}"
    "-framework AudioToolbox" "-framework CoreFoundation" "-framework Security" "-framework SystemConfiguration" iconv)
add_executable(ravet_au_native_audio_contracts ../../tools/au-validation/audio.cpp)
target_compile_features(ravet_au_native_audio_contracts PRIVATE cxx_std_17)
target_include_directories(ravet_au_native_audio_contracts PRIVATE ../../crates/ravet-native/include ../performance-native/include)
target_link_libraries(ravet_au_native_audio_contracts PRIVATE "${RAVET_GAIN_LIBRARY}" "${RAVET_PERFORMANCE_LIBRARY}"
    "-framework AudioToolbox" "-framework CoreFoundation" "-framework Security" "-framework SystemConfiguration" iconv)
enable_testing()
add_test(NAME ravet_au_performance_contracts COMMAND ravet_au_performance_contracts "${CMAKE_BINARY_DIR}/AU/ravet_performance_au.component")
add_test(NAME ravet_au_native_parameters_contracts COMMAND ravet_au_native_parameters_contracts)
add_test(NAME ravet_au_native_audio_contracts COMMAND ravet_au_native_audio_contracts)

foreach(target ravet_au_native_transport_contracts ravet_au_native_transport_fast_contracts)
    add_executable(${target} ../../tools/au-validation/transport.cpp)
    target_compile_features(${target} PRIVATE cxx_std_17)
    target_include_directories(${target} PRIVATE ../../crates/ravet-native/include ../performance-native/include "${RAVET_VST3_SDK}")
    target_link_libraries(${target} PRIVATE "${RAVET_PERFORMANCE_LIBRARY}" "-framework AudioToolbox" "-framework CoreFoundation" "-framework Security" "-framework SystemConfiguration" iconv)
    add_test(NAME ${target} COMMAND ${target})
endforeach()
target_compile_options(ravet_au_native_transport_fast_contracts PRIVATE -O3 -ffast-math)

# Standalone reusable output helpers and explicitly synthetic component evidence;
# this does not add a MIDI output to the public Performance factory.
foreach(target ravet_au_native_midi_output_contracts ravet_au_native_midi_output_fast_contracts)
    add_executable(${target} ../../tools/au-validation/midi-output.cpp)
    target_compile_features(${target} PRIVATE cxx_std_17)
    target_include_directories(${target} PRIVATE ../../crates/ravet-native/include ../performance-native/include)
    target_link_libraries(${target} PRIVATE "${RAVET_PERFORMANCE_LIBRARY}" "-framework AudioToolbox" "-framework CoreMIDI"
        "-framework CoreFoundation" "-framework Security" "-framework SystemConfiguration" iconv)
    add_test(NAME ${target} COMMAND ${target})
endforeach()
target_compile_options(ravet_au_native_midi_output_fast_contracts PRIVATE -O3 -ffast-math)
add_executable(ravet_au_synthetic_midi_output_contracts ../../tools/au-validation/midi-output-component.mm)
target_compile_features(ravet_au_synthetic_midi_output_contracts PRIVATE cxx_std_23)
target_include_directories(ravet_au_synthetic_midi_output_contracts PRIVATE ../../crates/ravet-native/include ../performance-native/include "${RAVET_AUDIOUNIT_SDK}/include")
target_link_libraries(ravet_au_synthetic_midi_output_contracts PRIVATE "${RAVET_AUDIOUNIT_LIBRARY}" "${RAVET_PERFORMANCE_LIBRARY}"
    "-framework AudioUnit" "-framework CoreMIDI" "-framework AudioToolbox" "-framework CoreFoundation"
    "-framework CoreAudio" "-framework Security" "-framework SystemConfiguration" iconv)
add_test(NAME ravet_au_synthetic_midi_output_contracts COMMAND ravet_au_synthetic_midi_output_contracts)

# Separate generic precision fixtures remain independent of the distributed factory.
foreach(target ravet_au_native_audio64_contracts ravet_au_native_audio64_fast_contracts)
  add_executable(${target} ../../tools/au-validation/audio64.cpp)
  target_compile_features(${target} PRIVATE cxx_std_17)
  target_include_directories(${target} PRIVATE ../../crates/ravet-native/include ../performance-native/include)
  target_link_libraries(${target} PRIVATE "${RAVET_GAIN_LIBRARY}" "${RAVET_PERFORMANCE_LIBRARY}"
    "-framework AudioToolbox" "-framework CoreFoundation" "-framework Security" "-framework SystemConfiguration" "-liconv")
  add_test(NAME ${target} COMMAND ${target})
endforeach()
target_compile_options(ravet_au_native_audio64_fast_contracts PRIVATE -O3 -ffast-math)
add_executable(ravet_au_synthetic_audio64_contracts ../../tools/au-validation/precision-component.mm)
target_compile_features(ravet_au_synthetic_audio64_contracts PRIVATE cxx_std_23)
target_include_directories(ravet_au_synthetic_audio64_contracts PRIVATE ../../crates/ravet-native/include ../performance-native/include "${RAVET_AUDIOUNIT_SDK}/include")
target_link_libraries(ravet_au_synthetic_audio64_contracts PRIVATE "${RAVET_AUDIOUNIT_LIBRARY}" "${RAVET_PERFORMANCE_LIBRARY}"
  "-framework AudioToolbox" "-framework CoreMIDI" "-framework CoreFoundation" "-framework Security" "-framework SystemConfiguration" "-liconv")
add_test(NAME ravet_au_synthetic_audio64_contracts COMMAND ravet_au_synthetic_audio64_contracts)

# Actual distributed component precision/lifecycle and explicit native schema provenance.
foreach(target ravet_au_performance64_contracts ravet_au_performance_precision_contracts)
  if(target STREQUAL "ravet_au_performance64_contracts")
    set(contract_source performance64.mm)
  else()
    set(contract_source performance-precision.mm)
  endif()
  add_executable(${target} ../../tools/au-validation/${contract_source})
  target_compile_features(${target} PRIVATE cxx_std_17)
  target_link_libraries(${target} PRIVATE "-framework AudioToolbox" "-framework CoreFoundation")
  add_test(NAME ${target} COMMAND ${target} "${CMAKE_BINARY_DIR}/AU/ravet_performance_au.component")
endforeach()
foreach(target ravet_au_native_precision_schema_contracts ravet_au_native_precision_schema_fast_contracts)
  add_executable(${target} ../../tools/au-validation/native-precision-schema.cpp)
  target_compile_features(${target} PRIVATE cxx_std_17)
  target_include_directories(${target} PRIVATE ../../crates/ravet-native/include ../performance-native/include)
  target_link_libraries(${target} PRIVATE "${RAVET_PERFORMANCE_LIBRARY}" "-framework AudioToolbox"
    "-framework CoreFoundation" "-framework Security" "-framework SystemConfiguration" iconv)
  add_test(NAME ${target} COMMAND ${target})
endforeach()
target_compile_options(ravet_au_native_precision_schema_fast_contracts PRIVATE -O3 -ffast-math)
