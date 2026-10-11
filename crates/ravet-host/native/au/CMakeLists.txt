cmake_minimum_required(VERSION 3.25)
project(ravet_au_host LANGUAGES C CXX OBJCXX)
if(NOT APPLE OR NOT CMAKE_SYSTEM_NAME STREQUAL "Darwin")
    message(FATAL_ERROR "This optional AUv3 host provider has local macOS evidence only")
endif()
set(CMAKE_CXX_STANDARD 17)
set(CMAKE_OBJCXX_STANDARD 17)
get_filename_component(RAVET_ROOT "${CMAKE_CURRENT_SOURCE_DIR}/../../../.." ABSOLUTE)
set(RAVET_AU_HOST_VALIDATION "${RAVET_ROOT}/tools/au-host-validation")

add_library(ravet_au_host STATIC provider.mm)
target_include_directories(ravet_au_host PUBLIC "${CMAKE_CURRENT_SOURCE_DIR}")
target_compile_options(ravet_au_host PRIVATE -fobjc-arc -Wall -Wextra -Werror)
target_link_libraries(ravet_au_host PUBLIC "-framework AudioToolbox" "-framework AVFoundation" "-framework Foundation")

include(CTest)
if(BUILD_TESTING)
    # Injected SDK factory fixture: bounded pending/instance slots, completion
    # reentry/duplicates, NSError/OSStatus domains and render faults. Synthetic
    # load-mode values are not system IPC evidence.
    add_executable(ravet-au-host-failure-tests provider.mm failure-tests.mm)
    target_include_directories(ravet-au-host-failure-tests PRIVATE "${CMAKE_CURRENT_SOURCE_DIR}")
    target_compile_definitions(ravet-au-host-failure-tests PRIVATE RAVET_AUH_TESTING=1)
    target_compile_options(ravet-au-host-failure-tests PRIVATE -fobjc-arc -Wall -Wextra -Werror)
    target_link_libraries(ravet-au-host-failure-tests PRIVATE "-framework AudioToolbox" "-framework AVFoundation" "-framework Foundation")
    add_test(NAME ravet-au-host-failure-tests COMMAND ravet-au-host-failure-tests)
endif()

# Actual public Gain targets need an explicit caller-built Gain native archive.
# Without RAVET_GAIN_LIBRARY those actual-component tests are absent.
if(RAVET_GAIN_LIBRARY)
    if(NOT EXISTS "${RAVET_GAIN_LIBRARY}")
        message(FATAL_ERROR "RAVET_GAIN_LIBRARY does not exist: ${RAVET_GAIN_LIBRARY}")
    endif()
    set(ravet_au_gain_sources
        "${RAVET_ROOT}/native/apple/RavetAudioUnit.mm"
        "${RAVET_ROOT}/examples/gain-auv3/GainAudioUnit.mm")
    set(ravet_au_gain_includes
        "${RAVET_ROOT}/native/apple" "${RAVET_ROOT}/native/common"
        "${RAVET_ROOT}/examples/gain-auv3" "${RAVET_ROOT}/crates/ravet-native/include")
    # Test-process registered Gain subclass for the Rust consumer driven by
    # tools/au-host-validation/check-au-host.py.
    add_library(ravet_au_host_fixture STATIC ${ravet_au_gain_sources}
        "${RAVET_AU_HOST_VALIDATION}/fixture.mm")
    target_include_directories(ravet_au_host_fixture PRIVATE ${ravet_au_gain_includes})
    target_compile_options(ravet_au_host_fixture PRIVATE -fobjc-arc -Wall -Wextra -Werror)
    if(BUILD_TESTING)
        add_executable(ravet-au-host-gain-tests ${ravet_au_gain_sources} gain-tests.mm)
        target_include_directories(ravet-au-host-gain-tests PRIVATE ${ravet_au_gain_includes})
        target_compile_options(ravet-au-host-gain-tests PRIVATE -fobjc-arc -Wall -Wextra -Werror)
        target_link_libraries(ravet-au-host-gain-tests PRIVATE ravet_au_host "${RAVET_GAIN_LIBRARY}"
            "-framework CoreMIDI" "-framework Security" iconv)
        add_test(NAME ravet-au-host-gain-tests COMMAND ravet-au-host-gain-tests)
    endif()
endif()

# Manual-only out-of-process probe. It never registers, installs or signs, and
# it is deliberately not a CTest: run it only after a separately reviewed local
# extension registration. UNAVAILABLE (exit 20) is not a pass.
add_executable(ravet-au-host-extension-probe EXCLUDE_FROM_ALL
    "${RAVET_AU_HOST_VALIDATION}/extension.mm")
target_compile_options(ravet-au-host-extension-probe PRIVATE -fobjc-arc -Wall -Wextra -Werror)
target_link_libraries(ravet-au-host-extension-probe PRIVATE ravet_au_host)
