cmake_minimum_required(VERSION 3.22)
project(ravet_product_metadata LANGUAGES CXX)
add_executable(ravet-product-metadata main.cpp)
target_compile_features(ravet-product-metadata PRIVATE cxx_std_17)
target_include_directories(ravet-product-metadata PRIVATE
  ../../crates/ravet-plugin/include ../../crates/ravet-native/include)
target_link_libraries(ravet-product-metadata PRIVATE ${CMAKE_DL_LIBS})
