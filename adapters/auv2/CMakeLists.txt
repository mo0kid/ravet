cmake_minimum_required(VERSION 3.25)
project(ravet_auv2 LANGUAGES CXX OBJCXX)
if(NOT APPLE)
    message(FATAL_ERROR "AUv2 requires macOS")
endif()
set(CMAKE_CXX_STANDARD 23)
set(CMAKE_OBJCXX_STANDARD 23)
set(CMAKE_OSX_DEPLOYMENT_TARGET 12.0)
set(ROOT "${CMAKE_CURRENT_SOURCE_DIR}/../..")
include_directories("${ROOT}/crates/ravet-plugin/include" "${ROOT}/crates/ravet-native/include")
add_executable(ravet_au_metadata metadata.cpp)
add_library(ravet_auval_registration SHARED auval_registration.cpp)
target_link_libraries(ravet_auval_registration PRIVATE "-framework AudioToolbox" "-framework CoreFoundation")
add_library(ravet_au_mock SHARED "${ROOT}/tools/au-plugin-validation/mock.cpp")
add_library(ravet_au_mock_instrument SHARED "${ROOT}/tools/au-plugin-validation/mock.cpp")
target_compile_definitions(ravet_au_mock_instrument PRIVATE RAVET_MOCK_INSTRUMENT=1)
add_library(ravet_au_mock_midi SHARED "${ROOT}/tools/au-plugin-validation/mock.cpp")
target_compile_definitions(ravet_au_mock_midi PRIVATE RAVET_MOCK_MIDI=1)
add_library(ravet_au_mock_sidechain SHARED "${ROOT}/tools/au-plugin-validation/mock.cpp")
target_compile_definitions(ravet_au_mock_sidechain PRIVATE RAVET_MOCK_SIDECHAIN=1)
add_library(ravet_au_mock_instrument_sidechain SHARED "${ROOT}/tools/au-plugin-validation/mock.cpp")
target_compile_definitions(ravet_au_mock_instrument_sidechain PRIVATE RAVET_MOCK_INSTRUMENT_SIDECHAIN=1)
add_library(ravet_au_mock_midi_sidechain SHARED "${ROOT}/tools/au-plugin-validation/mock.cpp")
target_compile_definitions(ravet_au_mock_midi_sidechain PRIVATE RAVET_MOCK_MIDI_SIDECHAIN=1)
add_library(ravet_au_mock_midi_generator SHARED "${ROOT}/tools/au-plugin-validation/mock.cpp")
target_compile_definitions(ravet_au_mock_midi_generator PRIVATE RAVET_MOCK_MIDI_GENERATOR=1)
add_executable(ravet_au_plugin_contracts "${ROOT}/tools/au-plugin-validation/contracts.mm")
target_compile_options(ravet_au_plugin_contracts PRIVATE -fobjc-arc)
target_link_libraries(ravet_au_plugin_contracts PRIVATE "-framework Cocoa" "-framework AudioToolbox" "-framework CoreMIDI")
add_executable(ravet_au_delay_contracts "${ROOT}/tools/au-plugin-validation/delay.mm")
target_link_libraries(ravet_au_delay_contracts PRIVATE "-framework AudioToolbox" "-framework CoreFoundation")

if(RAVET_PRODUCT_LIBRARY)
    if(NOT EXISTS "${RAVET_AUDIOUNIT_SDK}/include/AudioUnitSDK/AUBase.h")
        message(FATAL_ERROR "Supply an external Apple AudioUnitSDK checkout (include/AudioUnitSDK and src/AudioUnitSDK)")
    endif()
    # The adapter uses AudioUnitSDK 1.4 APIs. The SDK has no version macro, so
    # detect one of them (AUIOElement::GetBufferListOrError, added in 1.4) and
    # stop here rather than with a compile error. Older copies, such as the one
    # bundled with JUCE, lack it.
    file(STRINGS "${RAVET_AUDIOUNIT_SDK}/include/AudioUnitSDK/AUScopeElement.h" RAVET_AUSDK_14_API REGEX "GetBufferListOrError")
    if(NOT RAVET_AUSDK_14_API)
        message(FATAL_ERROR "RAVET's AUv2 adapter requires Apple AudioUnitSDK 1.4 or later. "
            "${RAVET_AUDIOUNIT_SDK} is an older copy: its include/AudioUnitSDK/AUScopeElement.h "
            "lacks AUIOElement::GetBufferListOrError (added in 1.4). Use github.com/apple/AudioUnitSDK 1.4 or later.")
    endif()
    # Build the SDK directly from its external source tree; never copy/vendor it.
    file(GLOB SDK_SOURCES CONFIGURE_DEPENDS "${RAVET_AUDIOUNIT_SDK}/src/AudioUnitSDK/*.cpp")
    if(NOT SDK_SOURCES)
        message(FATAL_ERROR "AudioUnitSDK sources missing")
    endif()
    add_library(ravet_external_ausdk STATIC ${SDK_SOURCES})
    target_include_directories(ravet_external_ausdk PUBLIC "${RAVET_AUDIOUNIT_SDK}/include")
    add_library(ravet_au MODULE PluginAU.mm)
    target_compile_options(ravet_au PRIVATE -fobjc-arc)
    target_compile_definitions(ravet_au PRIVATE
        RAVET_AU_VIEW_CLASS=RavetAUView_${RAVET_AU_CLASS_SUFFIX}
        RAVET_AU_FACTORY_CLASS=RavetAUFactory_${RAVET_AU_CLASS_SUFFIX}
        RAVET_AU_BUNDLE_ID="${RAVET_AU_BUNDLE_ID}")
    target_link_libraries(ravet_au PRIVATE ravet_external_ausdk "${RAVET_PRODUCT_LIBRARY}"
        "-framework Cocoa" "-framework AudioToolbox" "-framework CoreMIDI" "-framework CoreAudio")
    set_target_properties(ravet_au PROPERTIES BUNDLE TRUE BUNDLE_EXTENSION component
        MACOSX_BUNDLE_INFO_PLIST "${RAVET_AU_PLIST}" LIBRARY_OUTPUT_DIRECTORY "${CMAKE_BINARY_DIR}/AU"
        BUILD_RPATH "@loader_path/../Frameworks" INSTALL_RPATH "@loader_path/../Frameworks")
endif()
