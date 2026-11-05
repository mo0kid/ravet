# Generic RAVET LV2 adapter: one DSP module and one UI module for every product,
# a build-time Turtle generator, a mock product and contract tests.
#
#   cmake -S crates/ravet-plugin/lv2 -B target/lv2/generic -DCMAKE_BUILD_TYPE=Release \
#         -DRAVET_LV2_SDK=/path/to/lv2 [-DRAVET_PLUGIN_LIBRARY=/path/to/libproduct.so \
#         -DRAVET_LV2_BUNDLE_NAME=ravet_delay] [-DRAVET_LV2_LICENSE=<URI>]
#   cmake --build target/lv2/generic --parallel 2
#   ctest --test-dir target/lv2/generic --output-on-failure
#
# RAVET_LV2_SDK is an external LV2 source tree, either the official layout
# (include/lv2/core/lv2.h) or a flat one (lv2/core/lv2.h). It is never vendored.
# The mock bundle (bundles/ravet_lv2_mock.lv2) is always built and tested. With
# RAVET_PLUGIN_LIBRARY, the product library that exports ravet_plugin_v1 is
# packaged as bundles/<RAVET_LV2_BUNDLE_NAME>.lv2 and smoke-tested.
cmake_minimum_required(VERSION 3.21)
project(ravet_lv2_generic LANGUAGES CXX)

set(RAVET_LV2_SDK "" CACHE PATH "External LV2 source SDK")
set(RAVET_PLUGIN_LIBRARY "" CACHE FILEPATH "Product library exporting ravet_plugin_v1 (optional)")
set(RAVET_LV2_BUNDLE_NAME "" CACHE STRING "Bundle name for RAVET_PLUGIN_LIBRARY (without .lv2)")
set(RAVET_LV2_LICENSE "" CACHE STRING "Optional doap:license URI written to plugin.ttl")
# The product table hosts editors in Cocoa views only in this release, so
# product bundles declare their LV2 UI on macOS by default. Turn this on
# elsewhere once the table hosts X11/Win32 editors. The mock bundle always
# includes the UI, so the UI module's X11/Win32 paths stay tested.
if(APPLE)
    set(ravet_lv2_ui_default ON)
else()
    set(ravet_lv2_ui_default OFF)
endif()
option(RAVET_LV2_UI "Declare and ship the LV2 UI in product bundles" ${ravet_lv2_ui_default})

if(EXISTS "${RAVET_LV2_SDK}/include/lv2/core/lv2.h")
    set(lv2_include "${RAVET_LV2_SDK}/include")
elseif(EXISTS "${RAVET_LV2_SDK}/lv2/core/lv2.h")
    set(lv2_include "${RAVET_LV2_SDK}")
else()
    message(FATAL_ERROR "Set RAVET_LV2_SDK to an external LV2 SDK containing include/lv2/core/lv2.h or lv2/core/lv2.h")
endif()
foreach(header atom/util.h buf-size/buf-size.h midi/midi.h options/options.h state/state.h time/time.h ui/ui.h urid/urid.h)
    if(NOT EXISTS "${lv2_include}/lv2/${header}")
        message(FATAL_ERROR "The LV2 SDK is missing lv2/${header}")
    endif()
endforeach()

set(CMAKE_CXX_STANDARD 17)
set(CMAKE_CXX_STANDARD_REQUIRED ON)
set(CMAKE_CXX_VISIBILITY_PRESET hidden)
set(CMAKE_VISIBILITY_INLINES_HIDDEN ON)
set(ravet_include "${CMAKE_CURRENT_SOURCE_DIR}/../include" "${CMAKE_CURRENT_SOURCE_DIR}/../../ravet-native/include")

if(WIN32)
    set(ravet_lv2_suffix ".dll")
    set(ravet_lv2_ui_class "WindowsUI")
elseif(APPLE)
    set(ravet_lv2_suffix ".dylib")
    set(ravet_lv2_ui_class "CocoaUI")
else()
    set(ravet_lv2_suffix ".so")
    set(ravet_lv2_ui_class "X11UI")
endif()

function(ravet_lv2_target target)
    target_include_directories(${target} PRIVATE "${CMAKE_CURRENT_SOURCE_DIR}" ${ravet_include} "${lv2_include}")
    if(MSVC)
        target_compile_options(${target} PRIVATE /W4)
    else()
        target_compile_options(${target} PRIVATE -Wall -Wextra)
    endif()
    if(UNIX)
        target_link_libraries(${target} PRIVATE ${CMAKE_DL_LIBS})
    endif()
endfunction()

add_library(ravet_lv2 MODULE adapter.cpp)
add_library(ravet_lv2_ui MODULE ui.cpp)
foreach(target ravet_lv2 ravet_lv2_ui)
    ravet_lv2_target(${target})
    set_target_properties(${target} PROPERTIES PREFIX "" SUFFIX "${ravet_lv2_suffix}")
endforeach()
if(APPLE)
    target_link_libraries(ravet_lv2_ui PRIVATE objc)
endif()

add_executable(ravet_lv2_turtle turtle.cpp)
ravet_lv2_target(ravet_lv2_turtle)

add_library(ravet_lv2_mock SHARED mock/mock_plugin.cpp)
target_include_directories(ravet_lv2_mock PRIVATE ${ravet_include})
set_target_properties(ravet_lv2_mock PROPERTIES PREFIX "" SUFFIX "${ravet_lv2_suffix}")

add_executable(ravet_lv2_contracts tests/contracts.cpp)
ravet_lv2_target(ravet_lv2_contracts)
if(APPLE)
    target_link_libraries(ravet_lv2_contracts PRIVATE objc)
endif()

# Assemble <bundle>.lv2 from the generic modules, a product library and Turtle
# generated from that library's table.
function(ravet_lv2_bundle name library dependency with_ui)
    set(bundle "${CMAKE_BINARY_DIR}/bundles/${name}.lv2")
    set(ui_commands "")
    set(ui_args "")
    if(with_ui)
        set(ui_commands COMMAND "${CMAKE_COMMAND}" -E copy "$<TARGET_FILE:ravet_lv2_ui>" "${bundle}/ravet_lv2_ui${ravet_lv2_suffix}")
        set(ui_args --ui-binary "ravet_lv2_ui${ravet_lv2_suffix}" --ui-class "${ravet_lv2_ui_class}")
    endif()
    set(license_args "")
    if(RAVET_LV2_LICENSE)
        set(license_args --license "${RAVET_LV2_LICENSE}")
    endif()
    add_custom_command(
        OUTPUT "${bundle}/plugin.ttl" "${bundle}/manifest.ttl"
        COMMAND "${CMAKE_COMMAND}" -E make_directory "${bundle}"
        COMMAND "${CMAKE_COMMAND}" -E copy "$<TARGET_FILE:ravet_lv2>" "${bundle}/ravet_lv2${ravet_lv2_suffix}"
        ${ui_commands}
        COMMAND "${CMAKE_COMMAND}" -E copy "${library}" "${bundle}/ravet_product${ravet_lv2_suffix}"
        # The target name (not its file) lets CMake prepend
        # CMAKE_CROSSCOMPILING_EMULATOR, e.g. wine for Windows cross builds.
        COMMAND ravet_lv2_turtle "${bundle}/ravet_product${ravet_lv2_suffix}" "${bundle}"
                --binary "ravet_lv2${ravet_lv2_suffix}" ${ui_args} ${license_args}
        DEPENDS ravet_lv2 ravet_lv2_ui ravet_lv2_turtle ${dependency} "${library}"
        VERBATIM)
    add_custom_target(${name}_bundle ALL DEPENDS "${bundle}/plugin.ttl")
endfunction()

ravet_lv2_bundle(ravet_lv2_mock "$<TARGET_FILE:ravet_lv2_mock>" ravet_lv2_mock ON)

enable_testing()
add_test(NAME ravet_lv2_mock_contracts
         COMMAND ravet_lv2_contracts "${CMAKE_BINARY_DIR}/bundles/ravet_lv2_mock.lv2"
                 "ravet_lv2${ravet_lv2_suffix}" "ravet_lv2_ui${ravet_lv2_suffix}" --mock)

# Optional independent check through lilv (pkg-config lilv-0), when installed.
# Skipped when cross-compiling: pkg-config would find the build machine's lilv.
if(NOT CMAKE_CROSSCOMPILING)
    find_package(PkgConfig QUIET)
endif()
if(PkgConfig_FOUND)
    pkg_check_modules(LILV QUIET IMPORTED_TARGET lilv-0)
endif()
if(LILV_FOUND)
    add_executable(ravet_lv2_lilv_host tests/lilv_host.cpp)
    target_link_libraries(ravet_lv2_lilv_host PRIVATE PkgConfig::LILV)
    add_test(NAME ravet_lv2_mock_lilv
             COMMAND ravet_lv2_lilv_host "${CMAKE_BINARY_DIR}/bundles/ravet_lv2_mock.lv2" --mock)
else()
    message(STATUS "lilv-0 not found: skipping the lilv host check")
endif()

if(RAVET_PLUGIN_LIBRARY)
    if(NOT EXISTS "${RAVET_PLUGIN_LIBRARY}")
        message(FATAL_ERROR "RAVET_PLUGIN_LIBRARY does not exist: ${RAVET_PLUGIN_LIBRARY}")
    endif()
    if(NOT RAVET_LV2_BUNDLE_NAME)
        get_filename_component(RAVET_LV2_BUNDLE_NAME "${RAVET_PLUGIN_LIBRARY}" NAME_WE)
        string(REGEX REPLACE "^lib" "" RAVET_LV2_BUNDLE_NAME "${RAVET_LV2_BUNDLE_NAME}")
    endif()
    ravet_lv2_bundle(${RAVET_LV2_BUNDLE_NAME} "${RAVET_PLUGIN_LIBRARY}" "" ${RAVET_LV2_UI})
    if(RAVET_LV2_UI)
        set(product_ui "ravet_lv2_ui${ravet_lv2_suffix}")
    else()
        set(product_ui "-")
    endif()
    add_test(NAME ${RAVET_LV2_BUNDLE_NAME}_smoke
             COMMAND ravet_lv2_contracts "${CMAKE_BINARY_DIR}/bundles/${RAVET_LV2_BUNDLE_NAME}.lv2"
                     "ravet_lv2${ravet_lv2_suffix}" "${product_ui}")
    if(LILV_FOUND)
        add_test(NAME ${RAVET_LV2_BUNDLE_NAME}_lilv
                 COMMAND ravet_lv2_lilv_host "${CMAKE_BINARY_DIR}/bundles/${RAVET_LV2_BUNDLE_NAME}.lv2")
    endif()
endif()
