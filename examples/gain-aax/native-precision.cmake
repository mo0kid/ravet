# Prepared native f64 processing remains behind the unchanged AAX float32 ABI.
# These actual native consumers test conversion, lifecycle, auxiliary routing,
# generated output, timing candidates and returned native/local fault separation.
set(ravet_aax_precision_targets)
foreach(precision_group IN ITEMS precision owner model multibus generated timing faults maximum)
    foreach(precision_mode IN ITEMS normal fast)
        set(precision_target ravet_aax_precision_${precision_group}_${precision_mode})
        list(APPEND ravet_aax_precision_targets ${precision_target})
        add_executable(${precision_target}
            "${CMAKE_CURRENT_SOURCE_DIR}/../../tools/aax-validation/precision/${precision_group}.cpp")
        target_compile_features(${precision_target} PRIVATE cxx_std_17)
        target_compile_options(${precision_target} PRIVATE -Wall -Wextra -Werror -O2)
        target_compile_definitions(${precision_target} PRIVATE RAVET_AAX_NATIVE_SAMPLE_BITS=64)
        target_include_directories(${precision_target} PRIVATE
            "${RAVET_AAX_SDK}/Interfaces" "${RAVET_AAX_SDK}/Interfaces/ACF"
            "${CMAKE_CURRENT_SOURCE_DIR}/../../tools/aax-validation"
            "${CMAKE_CURRENT_SOURCE_DIR}/../gain-aax"
            "${CMAKE_CURRENT_SOURCE_DIR}/../gain-native/include"
            "${CMAKE_CURRENT_SOURCE_DIR}/../bus-routing-native/include"
            "${CMAKE_CURRENT_SOURCE_DIR}/../../crates/ravet-native/include")
        if(NOT precision_group MATCHES "^(precision|maximum)$")
            target_sources(${precision_target} PRIVATE
                "${CMAKE_CURRENT_SOURCE_DIR}/../../tools/aax-validation/description-stub.cpp")
            if(precision_group MATCHES "^(multibus|generated|timing)$")
                set(precision_archive "${RAVET_BUS_ROUTING_LIBRARY}")
            else()
                set(precision_archive "${RAVET_GAIN_LIBRARY}")
            endif()
            target_link_libraries(${precision_target} PRIVATE aax_wrapper sdk "${precision_archive}"
                "-framework Security" "-framework CoreFoundation" "-framework SystemConfiguration"
                "-framework Carbon" "-framework Cocoa" iconv)
        endif()
        if(precision_mode STREQUAL "fast" AND CMAKE_CXX_COMPILER_ID MATCHES "Clang|GNU")
            target_compile_options(${precision_target} PRIVATE -ffast-math)
        endif()
        add_test(NAME ${precision_target} COMMAND ${precision_target})
    endforeach()
endforeach()
