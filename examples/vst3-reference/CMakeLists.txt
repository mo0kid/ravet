cmake_minimum_required(VERSION 3.25)
project(ravet_vst3_reference VERSION 0.0.1 LANGUAGES C CXX)
# RAVET-owned reference VST3 modules for hosted-VST3 tests (ADR 0274):
# RAVET Keyed Sidechain (stereo main + mono/stereo auxiliary sidechain) and
# RAVET Event Probe (all nine SDK input event kinds rendered analytically).
# Bundles stay in this build tree, ad-hoc signed; nothing is installed.
if(NOT APPLE)
    message(FATAL_ERROR "The reference VST3 modules have local macOS evidence only")
endif()
enable_language(OBJCXX)
if(NOT EXISTS "${RAVET_VST3_SDK}/CMakeLists.txt" OR NOT EXISTS "${RAVET_VST3_REFERENCE_LIBRARY}")
    message(FATAL_ERROR "Set RAVET_VST3_SDK to the external SDK and RAVET_VST3_REFERENCE_LIBRARY to the ravet-vst3-reference-native static library")
endif()
set(SMTG_ENABLE_VST3_PLUGIN_EXAMPLES OFF CACHE BOOL "" FORCE)
set(SMTG_ENABLE_VST3_HOSTING_EXAMPLES ON CACHE BOOL "" FORCE)
set(SMTG_ENABLE_VSTGUI_SUPPORT OFF CACHE BOOL "" FORCE)
set(SMTG_CREATE_PLUGIN_LINK OFF CACHE BOOL "" FORCE)
# Never redirect bundles to a user or system plug-in folder.
set(SMTG_PLUGIN_TARGET_USER_PATH "" CACHE PATH "" FORCE)
set(SMTG_RUN_VST_VALIDATOR OFF CACHE BOOL "" FORCE)
set(SMTG_ENABLE_AUV2_BUILDS OFF CACHE BOOL "" FORCE)
set(SMTG_ENABLE_AAX_BUILDS OFF CACHE BOOL "" FORCE)
add_subdirectory("${RAVET_VST3_SDK}" sdk-build)
if(SMTG_CREATE_PLUGIN_LINK OR NOT SMTG_PLUGIN_TARGET_USER_PATH STREQUAL "")
    message(FATAL_ERROR "Reference bundles must stay in the build tree")
endif()
set(public_sdk_SOURCE_DIR "${RAVET_VST3_SDK}/public.sdk")
enable_testing()
foreach(plugin IN ITEMS sidechain event_probe)
    set(target "ravet_${plugin}_vst3")
    smtg_add_vst3plugin(${target} src/${plugin}.cpp)
    smtg_target_add_library_main(${target})
    target_sources(${target} PRIVATE
        "${RAVET_VST3_SDK}/public.sdk/source/vst/vstaudioeffect.cpp"
        "${RAVET_VST3_SDK}/public.sdk/source/vst/vsteditcontroller.cpp")
    target_compile_features(${target} PRIVATE cxx_std_17)
    target_compile_options(${target} PRIVATE -Wall -Wextra)
    target_include_directories(${target} PRIVATE "${RAVET_VST3_SDK}"
        "${CMAKE_CURRENT_SOURCE_DIR}/../vst3-reference-native/include")
    target_link_libraries(${target} PRIVATE sdk "${RAVET_VST3_REFERENCE_LIBRARY}"
        "-framework Security" "-framework CoreFoundation" "-framework SystemConfiguration" iconv)
    string(REPLACE "_" "-" identifier "${plugin}")
    smtg_target_set_bundle(${target} BUNDLE_IDENTIFIER "org.ravet.${identifier}.vst3" COMPANY_NAME "RAVET")
    # Local ad-hoc signature over the complete build-tree bundle; no identity,
    # no notarization, no installation or plug-in folder link.
    add_custom_command(TARGET ${target} POST_BUILD
        COMMAND codesign --force --sign - "$<TARGET_BUNDLE_DIR:${target}>"
        VERBATIM)
    add_test(NAME ravet_reference_vst3_${plugin}_signature
        COMMAND sh -c "codesign --verify --strict --verbose=2 \"$0\" && codesign -dv \"$0\" 2>&1 | grep -q '^Signature=adhoc$'"
            "$<TARGET_BUNDLE_DIR:${target}>")
    # No install path: the bundle lives under this build tree, no install rule
    # names it, and no default user/system VST3 folder holds a copy or link.
    add_test(NAME ravet_reference_vst3_${plugin}_no_install
        COMMAND sh -c "case \"$0\" in \"$1\"/*) ;; *) exit 1;; esac; \
name=$(basename \"$0\"); \
for d in \"$HOME/Library/Audio/Plug-Ins/VST3\" /Library/Audio/Plug-Ins/VST3; do \
  if [ -e \"$d/$name\" ] || [ -L \"$d/$name\" ]; then exit 2; fi; done; \
if find \"$1\" -name cmake_install.cmake -exec grep -l \"$name\" {} + | grep -q .; then exit 3; fi"
            "$<TARGET_BUNDLE_DIR:${target}>" "${CMAKE_BINARY_DIR}")
    add_test(NAME ravet_reference_vst3_${plugin}_validator
        COMMAND validator "$<TARGET_BUNDLE_DIR:${target}>")
endforeach()
configure_file("${RAVET_VST3_SDK}/LICENSE.txt" "${CMAKE_CURRENT_BINARY_DIR}/vst3-sdk-LICENSE.txt" COPYONLY)
