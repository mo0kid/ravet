# Generic RAVET AAX adapter (decision 0281): one .aaxplugin per product, built
# from the product's static library exporting ravet_plugin_v1 (RavetPlugin.h).
#
#   cmake -S crates/ravet-plugin/aax -B <build> -DRAVET_AAX_SDK=<external AAX SDK> \
#         [-DRAVET_PLUGIN_LIBRARY=<libproduct.a | product.lib>] \
#         [-DRAVET_PLUGIN_LINK_LIBRARIES="<native libs>"] [-DRAVET_AAX_NAME=Name]
#   cmake --build <build> --config Release
#
# Without RAVET_PLUGIN_LIBRARY the bundled C++ mock table is used and the adapter
# tests are built. scripts/bundle-aax.py --plugin <product> drives this for a
# product crate. The AAX SDK is never vendored. Builds are unsigned: PACE/Avid
# signing is the plug-in vendor's own step (see docs/aax.md).
cmake_minimum_required(VERSION 3.22)
if(APPLE AND NOT CMAKE_OSX_DEPLOYMENT_TARGET)
    set(CMAKE_OSX_DEPLOYMENT_TARGET "11.0" CACHE STRING "Minimum macOS version")
endif()
# Rust static libraries use the DLL C runtime (/MD) unless built with
# -C target-feature=+crt-static; match that, or opt into /MT for both.
option(RAVET_AAX_STATIC_CRT "Windows: static C runtime (/MT); the product library must match" OFF)
if(RAVET_AAX_STATIC_CRT)
    set(CMAKE_MSVC_RUNTIME_LIBRARY "MultiThreaded$<$<CONFIG:Debug>:Debug>")
else()
    set(CMAKE_MSVC_RUNTIME_LIBRARY "MultiThreaded$<$<CONFIG:Debug>:Debug>DLL")
endif()
project(ravet_aax_plugin VERSION 0.1.0 LANGUAGES C CXX)

set(RAVET_AAX_SDK "" CACHE PATH "External Avid AAX SDK root (Interfaces/, Libs/AAXLibrary/source/)")
set(RAVET_PLUGIN_LIBRARY "" CACHE FILEPATH "Product static library exporting ravet_plugin_v1; empty selects the mock")
set(RAVET_PLUGIN_LINK_LIBRARIES "" CACHE STRING "Native libraries the product library needs (rustc --print native-static-libs)")
set(RAVET_AAX_NAME "RavetMock" CACHE STRING "Bundle and binary name")
set(RAVET_AAX_BUNDLE_ID "org.ravet.mock.aax" CACHE STRING "macOS bundle identifier")
set(RAVET_AAX_VERSION "0.1.0" CACHE STRING "Bundle version")
option(RAVET_AAX_TESTS "Build the adapter tests (mock table only)" ON)

if(NOT APPLE AND NOT WIN32)
    message(FATAL_ERROR "AAX plug-ins are built on macOS and Windows only")
endif()
foreach(required Interfaces/AAX.h Interfaces/AAX_Exports.cpp Interfaces/ACF/CACFClassFactory.cpp
                 Libs/AAXLibrary/source/AAX_CEffectParameters.cpp)
    if(NOT EXISTS "${RAVET_AAX_SDK}/${required}")
        message(FATAL_ERROR "RAVET_AAX_SDK=${RAVET_AAX_SDK} is not a complete AAX SDK: ${required} is missing")
    endif()
endforeach()

set(CMAKE_CXX_STANDARD 17)
set(CMAKE_CXX_STANDARD_REQUIRED ON)
set(CMAKE_POSITION_INDEPENDENT_CODE ON)
set(RAVET_ROOT "${CMAKE_CURRENT_SOURCE_DIR}/../../..")

# ---- AAX Library, compiled from the external SDK -------------------------------
file(GLOB aax_library_sources "${RAVET_AAX_SDK}/Libs/AAXLibrary/source/*.cpp")
add_library(ravet_aax_library STATIC ${aax_library_sources} "${RAVET_AAX_SDK}/Interfaces/ACF/CACFClassFactory.cpp")
target_include_directories(ravet_aax_library SYSTEM PUBLIC
    "${RAVET_AAX_SDK}/Interfaces" "${RAVET_AAX_SDK}/Interfaces/ACF" "${RAVET_AAX_SDK}/Libs/AAXLibrary/include")
if(MSVC)
    target_compile_definitions(ravet_aax_library PUBLIC WIN32 _CRT_SECURE_NO_WARNINGS NOMINMAX)
    target_compile_options(ravet_aax_library PRIVATE /W0)
else()
    target_compile_options(ravet_aax_library PRIVATE -w)
endif()

# ---- Adapter -------------------------------------------------------------------
add_library(ravet_aax_adapter OBJECT RavetAaxPlugin.cpp)
target_include_directories(ravet_aax_adapter PUBLIC
    "${CMAKE_CURRENT_SOURCE_DIR}" "${RAVET_ROOT}/crates/ravet-plugin/include" "${RAVET_ROOT}/crates/ravet-native/include")
target_link_libraries(ravet_aax_adapter PUBLIC ravet_aax_library)
if(MSVC)
    target_compile_options(ravet_aax_adapter PRIVATE /W4 /wd4100)
else()
    target_compile_options(ravet_aax_adapter PRIVATE -Wall -Wextra -Wno-unused-parameter -fvisibility=hidden)
endif()

if(RAVET_PLUGIN_LIBRARY)
    if(NOT EXISTS "${RAVET_PLUGIN_LIBRARY}")
        message(FATAL_ERROR "RAVET_PLUGIN_LIBRARY=${RAVET_PLUGIN_LIBRARY} does not exist")
    endif()
    add_library(ravet_aax_product STATIC IMPORTED)
    set_target_properties(ravet_aax_product PROPERTIES IMPORTED_LOCATION "${RAVET_PLUGIN_LIBRARY}")
    separate_arguments(product_links NATIVE_COMMAND "${RAVET_PLUGIN_LINK_LIBRARIES}")
    target_link_libraries(ravet_aax_product INTERFACE ${product_links})
else()
    add_library(ravet_aax_product STATIC mock/RavetPluginMock.cpp)
    target_include_directories(ravet_aax_product PRIVATE "${RAVET_ROOT}/crates/ravet-plugin/include" "${RAVET_ROOT}/crates/ravet-native/include")
endif()

# ---- The .aaxplugin ------------------------------------------------------------
add_library(ravet_aax MODULE "${RAVET_AAX_SDK}/Interfaces/AAX_Exports.cpp" $<TARGET_OBJECTS:ravet_aax_adapter>)
target_link_libraries(ravet_aax PRIVATE ravet_aax_library ravet_aax_product)
set_target_properties(ravet_aax PROPERTIES OUTPUT_NAME "${RAVET_AAX_NAME}")
if(APPLE)
    set_target_properties(ravet_aax PROPERTIES
        BUNDLE TRUE BUNDLE_EXTENSION aaxplugin
        MACOSX_BUNDLE_INFO_PLIST "${CMAKE_CURRENT_SOURCE_DIR}/Info.plist.in"
        MACOSX_BUNDLE_BUNDLE_NAME "${RAVET_AAX_NAME}"
        MACOSX_BUNDLE_GUI_IDENTIFIER "${RAVET_AAX_BUNDLE_ID}"
        MACOSX_BUNDLE_SHORT_VERSION_STRING "${RAVET_AAX_VERSION}"
        MACOSX_BUNDLE_BUNDLE_VERSION "${RAVET_AAX_VERSION}"
        LIBRARY_OUTPUT_DIRECTORY "${CMAKE_BINARY_DIR}/$<1:>")
    target_link_options(ravet_aax PRIVATE "-Wl,-exported_symbols_list,${CMAKE_CURRENT_SOURCE_DIR}/exports.txt" -Wl,-dead_strip)
    target_link_libraries(ravet_aax PRIVATE "-framework CoreFoundation" "-framework Cocoa")
    add_custom_command(TARGET ravet_aax POST_BUILD
        COMMAND ${CMAKE_COMMAND} -E echo_append "TDMwPTul" > "$<TARGET_BUNDLE_CONTENT_DIR:ravet_aax>/PkgInfo")
else()
    # <Name>.aaxplugin/Contents/x64/<Name>.aaxplugin is the DLL.
    set_target_properties(ravet_aax PROPERTIES
        SUFFIX ".aaxplugin" PREFIX ""
        LIBRARY_OUTPUT_DIRECTORY "${CMAKE_BINARY_DIR}/${RAVET_AAX_NAME}.aaxplugin/Contents/x64/$<1:>"
        RUNTIME_OUTPUT_DIRECTORY "${CMAKE_BINARY_DIR}/${RAVET_AAX_NAME}.aaxplugin/Contents/x64/$<1:>")
    target_link_libraries(ravet_aax PRIVATE ole32 user32 shell32)
    if(EXISTS "${RAVET_AAX_SDK}/Utilities/PlugIn.ico")
        add_custom_command(TARGET ravet_aax POST_BUILD
            COMMAND ${CMAKE_COMMAND} -E make_directory "${CMAKE_BINARY_DIR}/${RAVET_AAX_NAME}.aaxplugin/Contents/Resources"
            COMMAND ${CMAKE_COMMAND} -E copy "${RAVET_AAX_SDK}/Utilities/PlugIn.ico"
                    "${CMAKE_BINARY_DIR}/${RAVET_AAX_NAME}.aaxplugin/PlugIn.ico")
    endif()
endif()

# ---- Checks --------------------------------------------------------------------
enable_testing()
# Loads any built bundle and exercises it through the public SDK interfaces with a
# synthetic host. Not Avid's validator and not Pro Tools.
add_executable(ravet_aax_check tests/check-bundle.cpp)
target_include_directories(ravet_aax_check PRIVATE "${RAVET_ROOT}/tools/aax-validation" "${RAVET_ROOT}/crates/ravet-plugin/include")
target_link_libraries(ravet_aax_check PRIVATE ravet_aax_library)
if(APPLE)
    target_link_libraries(ravet_aax_check PRIVATE "-framework CoreFoundation")
    add_test(NAME ravet_aax_check COMMAND ravet_aax_check "$<TARGET_BUNDLE_DIR:ravet_aax>")
else()
    add_test(NAME ravet_aax_check COMMAND ravet_aax_check "${CMAKE_BINARY_DIR}/${RAVET_AAX_NAME}.aaxplugin")
endif()

if(RAVET_AAX_TESTS AND NOT RAVET_PLUGIN_LIBRARY)
    add_executable(ravet_aax_adapter_tests tests/adapter-tests.cpp)
    target_include_directories(ravet_aax_adapter_tests PRIVATE "${RAVET_ROOT}/tools/aax-validation")
    target_link_libraries(ravet_aax_adapter_tests PRIVATE ravet_aax_adapter ravet_aax_product ravet_aax_library)
    if(APPLE)
        target_link_libraries(ravet_aax_adapter_tests PRIVATE "-framework CoreFoundation" "-framework Cocoa")
    endif()
    add_test(NAME ravet_aax_adapter_tests COMMAND ravet_aax_adapter_tests)
endif()
