# Owned composition of the external SDK registration boundary. Compile the
# installed wrapper source unchanged under a renamed entry in an object target;
# SDK algorithm/parameter/library implementation remains external and unvendored.
add_library(ravet_aax_owned_registration OBJECT
    "${RAVET_VST3_SDK}/public.sdk/source/vst/aaxwrapper/aaxwrapper.cpp")
target_compile_features(ravet_aax_owned_registration PRIVATE cxx_std_17)
target_compile_definitions(ravet_aax_owned_registration PRIVATE
    GetEffectDescriptions=RavetSdkGetEffectDescriptions)
target_include_directories(ravet_aax_owned_registration PRIVATE
    "${RAVET_VST3_SDK}" "${RAVET_AAX_SDK}/Interfaces"
    "${RAVET_AAX_SDK}/Interfaces/ACF" "${RAVET_AAX_SDK}/Libs/AAXLibrary/Include")
set_target_properties(ravet_aax_owned_registration PROPERTIES POSITION_INDEPENDENT_CODE ON)
target_link_libraries(ravet_aax_owned_registration PRIVATE sdk)
target_sources(ravet_gain_aax PRIVATE
    "${CMAKE_CURRENT_SOURCE_DIR}/../gain-aax/registration.cpp"
    $<TARGET_OBJECTS:ravet_aax_owned_registration>)
target_include_directories(ravet_gain_aax PRIVATE
    "${CMAKE_CURRENT_SOURCE_DIR}/../gain-aax"
    "${RAVET_AAX_SDK}/Interfaces" "${RAVET_AAX_SDK}/Interfaces/ACF"
    "${RAVET_AAX_SDK}/Libs/AAXLibrary/Include")
set(ravet_aax_contract_targets ravet_aax_native_parameters ravet_aax_native_parameters_fast)
if(APPLE)
    list(APPEND ravet_aax_contract_targets ravet_aax_registered_contracts)
endif()
foreach(aax_target IN LISTS ravet_aax_contract_targets)
    if(aax_target STREQUAL "ravet_aax_registered_contracts")
        set(aax_source "${CMAKE_CURRENT_SOURCE_DIR}/../../tools/aax-validation/registered.cpp")
    else()
        set(aax_source "${CMAKE_CURRENT_SOURCE_DIR}/../../tools/aax-validation/parameters.cpp")
    endif()
    add_executable(${aax_target} "${aax_source}"
        "${CMAKE_CURRENT_SOURCE_DIR}/../../tools/aax-validation/description-stub.cpp")
    target_compile_features(${aax_target} PRIVATE cxx_std_17)
    target_include_directories(${aax_target} PRIVATE
        "${RAVET_VST3_SDK}" "${RAVET_AAX_SDK}/Interfaces"
        "${RAVET_AAX_SDK}/Interfaces/ACF" "${RAVET_AAX_SDK}/Libs/AAXLibrary/Include"
        "${CMAKE_CURRENT_SOURCE_DIR}/../gain-native/include"
        "${CMAKE_CURRENT_SOURCE_DIR}/../../crates/ravet-native/include")
    target_link_libraries(${aax_target} PRIVATE aax_wrapper sdk "${RAVET_GAIN_LIBRARY}")
    if(APPLE)
        target_link_libraries(${aax_target} PRIVATE "-framework Security" "-framework CoreFoundation"
            "-framework SystemConfiguration" "-framework Carbon" "-framework Cocoa" iconv)
    endif()
    if(NOT aax_target STREQUAL "ravet_aax_registered_contracts")
        add_test(NAME ${aax_target} COMMAND ${aax_target})
    endif()
endforeach()
if(CMAKE_CXX_COMPILER_ID MATCHES "Clang|GNU")
    target_compile_options(ravet_aax_native_parameters_fast PRIVATE -ffast-math)
elseif(MSVC)
    target_compile_options(ravet_aax_native_parameters_fast PRIVATE /fp:fast)
endif()
