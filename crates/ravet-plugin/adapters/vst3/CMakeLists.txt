cmake_minimum_required(VERSION 3.25)
# Windows: compile the adapter's own SDK sources with the C runtime the SDK uses
# (/MD unless SMTG_USE_STATIC_CRT). Cross toolchains such as cargo-xwin's
# clang-cl file leave this unset, and clang-cl would otherwise default to /MT.
if(SMTG_USE_STATIC_CRT)
    set(CMAKE_MSVC_RUNTIME_LIBRARY "MultiThreaded$<$<CONFIG:Debug>:Debug>")
else()
    set(CMAKE_MSVC_RUNTIME_LIBRARY "MultiThreaded$<$<CONFIG:Debug>:Debug>DLL")
endif()
project(ravet_plugin_vst3 VERSION 0.1.0 LANGUAGES C CXX)
if(APPLE)
    enable_language(OBJCXX)
endif()
if(NOT EXISTS "${RAVET_VST3_SDK}/CMakeLists.txt")
    message(FATAL_ERROR "Set RAVET_VST3_SDK to a complete external Steinberg SDK")
endif()
option(RAVET_VST3_MOCK "Build the independent C++ table fixture" OFF)
set(RAVET_VST3_NAME ravet_plugin_vst3 CACHE STRING "Module/bundle name")
if(NOT RAVET_VST3_MOCK AND NOT EXISTS "${RAVET_PRODUCT_LIBRARY}")
    message(FATAL_ERROR "Set RAVET_PRODUCT_LIBRARY to the product cdylib, or RAVET_VST3_MOCK=ON")
endif()
set(SMTG_ENABLE_VST3_PLUGIN_EXAMPLES OFF CACHE BOOL "" FORCE)
# The SDK builds the validator and sdk_hosting regardless; the hosting examples
# (editorhost) would add GTK/gtkmm on Linux, which the adapter does not need.
set(SMTG_ENABLE_VST3_HOSTING_EXAMPLES OFF CACHE BOOL "" FORCE)
set(SMTG_ENABLE_VSTGUI_SUPPORT OFF CACHE BOOL "" FORCE)
set(SMTG_CREATE_PLUGIN_LINK OFF CACHE BOOL "" FORCE)
set(SMTG_RUN_VST_VALIDATOR OFF CACHE BOOL "" FORCE)
set(SMTG_CREATE_MODULE_INFO OFF CACHE BOOL "" FORCE)
set(SMTG_CREATE_BUNDLE_FOR_WINDOWS ON CACHE BOOL "" FORCE)
set(SMTG_ENABLE_AUV2_BUILDS OFF CACHE BOOL "" FORCE)
set(SMTG_ENABLE_AAX_BUILDS OFF CACHE BOOL "" FORCE)
add_subdirectory("${RAVET_VST3_SDK}" sdk-build)
set(public_sdk_SOURCE_DIR "${RAVET_VST3_SDK}/public.sdk")
# The SDK places the Windows module inside <name>.vst3/Contents/x86_64-win
# through per-configuration output directories for CMAKE_CONFIGURATION_TYPES,
# which it sets only in its own directory. Single-configuration generators
# (Ninja, Makefiles, e.g. clang-cl cross builds) need the same list here.
if(WIN32 AND NOT CMAKE_CONFIGURATION_TYPES)
    set(CMAKE_CONFIGURATION_TYPES Debug Release)
endif()
smtg_add_vst3plugin(${RAVET_VST3_NAME} adapter.cpp)
smtg_target_add_library_main(${RAVET_VST3_NAME})
function(ravet_adapter_target target)
    target_sources(${target} PRIVATE
        "${RAVET_VST3_SDK}/public.sdk/source/vst/vstaudioeffect.cpp"
        "${RAVET_VST3_SDK}/public.sdk/source/vst/vsteditcontroller.cpp")
    target_compile_features(${target} PRIVATE cxx_std_17)
    target_include_directories(${target} PRIVATE "${RAVET_VST3_SDK}"
        "${CMAKE_CURRENT_SOURCE_DIR}/../../include"
        "${CMAKE_CURRENT_SOURCE_DIR}/../../../ravet-native/include")
    target_link_libraries(${target} PRIVATE sdk ${CMAKE_DL_LIBS})
    if(APPLE)
        target_link_libraries(${target} PRIVATE "-framework CoreFoundation")
    endif()
    if(RAVET_VST3_MOCK)
        target_sources(${target} PRIVATE mock.cpp)
    else()
        get_filename_component(product_name "${RAVET_PRODUCT_LIBRARY}" NAME)
        target_compile_definitions(${target} PRIVATE RAVET_PRODUCT_LIBRARY_NAME="${product_name}")
        # Relink/copy when Cargo updates the image, even if C++ is unchanged.
        set_property(TARGET ${target} APPEND PROPERTY LINK_DEPENDS "${RAVET_PRODUCT_LIBRARY}")
        add_custom_command(TARGET ${target} POST_BUILD COMMAND ${CMAKE_COMMAND} -E copy_if_different
            "${RAVET_PRODUCT_LIBRARY}" "$<TARGET_FILE_DIR:${target}>/${product_name}")
    endif()
endfunction()
ravet_adapter_target(${RAVET_VST3_NAME})
if(APPLE)
    smtg_target_set_bundle(${RAVET_VST3_NAME} BUNDLE_IDENTIFIER "org.ravet.generic.vst3" COMPANY_NAME "RAVET")
endif()
include(CTest)
if(BUILD_TESTING AND RAVET_VST3_MOCK)
    add_executable(ravet_generic_vst3_contracts contracts.cpp
        "${RAVET_VST3_SDK}/public.sdk/source/common/memorystream.cpp")
    ravet_adapter_target(ravet_generic_vst3_contracts)
    target_link_libraries(ravet_generic_vst3_contracts PRIVATE sdk_hosting)
    add_test(NAME ravet_generic_vst3_contracts COMMAND ravet_generic_vst3_contracts)
endif()

option(RAVET_VST3_INSTRUMENT_TESTS "Check the real Evidence Synth product through adapter classes" OFF)
if(BUILD_TESTING AND RAVET_VST3_INSTRUMENT_TESTS AND NOT RAVET_VST3_MOCK)
    add_executable(ravet_vst3_instrument_contracts instrument.cpp)
    ravet_adapter_target(ravet_vst3_instrument_contracts)
    target_link_libraries(ravet_vst3_instrument_contracts PRIVATE sdk_hosting)
    add_test(NAME ravet_vst3_instrument_contracts COMMAND ravet_vst3_instrument_contracts)
endif()
