# CMake toolchain: cross-compile RAVET's C++ plug-in adapters for x86_64 Windows
# (MSVC ABI) from Linux with clang-cl and lld-link, against the Microsoft CRT and
# Windows SDK that cargo-xwin downloads. scripts/windows_cross.py drives this
# from the bundle scripts' --target windows option.
#
# It wraps the toolchain file cargo-xwin writes (path from the environment
# variable CMAKE_TOOLCHAIN_FILE_x86_64_pc_windows_msvc, as printed by
# `cargo xwin env --target x86_64-pc-windows-msvc`, else cargo-xwin's default
# cache location). clang-cl and lld-link must be on PATH (cargo-xwin's cache
# directory provides both).
set(_ravet_xwin "$ENV{CMAKE_TOOLCHAIN_FILE_x86_64_pc_windows_msvc}")
if(NOT _ravet_xwin)
    set(_ravet_xwin "$ENV{HOME}/.cache/cargo-xwin/cmake/clang-cl/x86_64-pc-windows-msvc-toolchain.cmake")
endif()
if(NOT EXISTS "${_ravet_xwin}")
    message(FATAL_ERROR "cargo-xwin toolchain not found at ${_ravet_xwin}; run "
                        "`cargo xwin build --target x86_64-pc-windows-msvc` once first")
endif()
include("${_ravet_xwin}")

# cargo-xwin clears CMake's default Windows system libraries; the Steinberg and
# Avid SDK targets rely on them, as native MSVC builds provide them.
set(_ravet_windows_libraries
    "kernel32.lib user32.lib gdi32.lib winspool.lib shell32.lib ole32.lib oleaut32.lib uuid.lib comdlg32.lib advapi32.lib")
set(CMAKE_C_STANDARD_LIBRARIES "${_ravet_windows_libraries}" CACHE STRING "" FORCE)
set(CMAKE_CXX_STANDARD_LIBRARIES "${_ravet_windows_libraries}" CACHE STRING "" FORCE)
