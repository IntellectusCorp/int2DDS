cmake_minimum_required(VERSION 3.10)
project(int2dds_ffi_examples C)

set(CMAKE_C_STANDARD 99)
set(CMAKE_C_STANDARD_REQUIRED ON)

# int2dds-ffi headers + built library (relative to this repo).
set(INT2DDS_FFI_INCLUDE_DIR "${CMAKE_CURRENT_SOURCE_DIR}/../include")

if(CMAKE_BUILD_TYPE STREQUAL "Release")
    set(INT2DDS_FFI_LIB_DIR "${CMAKE_CURRENT_SOURCE_DIR}/../../target/release")
else()
    set(INT2DDS_FFI_LIB_DIR "${CMAKE_CURRENT_SOURCE_DIR}/../../target/debug")
endif()

# Collapse the ../.. so the value is usable verbatim as an rpath entry below.
get_filename_component(INT2DDS_FFI_LIB_DIR "${INT2DDS_FFI_LIB_DIR}" ABSOLUTE)

if(WIN32)
    set(INT2DDS_FFI_LIB "${INT2DDS_FFI_LIB_DIR}/int2dds_ffi.dll.lib")
    set(INT2DDS_FFI_DLL "${INT2DDS_FFI_LIB_DIR}/int2dds_ffi.dll")
elseif(APPLE)
    set(INT2DDS_FFI_LIB "${INT2DDS_FFI_LIB_DIR}/libint2dds_ffi.dylib")
    set(INT2DDS_FFI_DLL "${INT2DDS_FFI_LIB_DIR}/libint2dds_ffi.dylib")
else()
    set(INT2DDS_FFI_LIB "${INT2DDS_FFI_LIB_DIR}/libint2dds_ffi.so")

    # ffi/build.rs injects -Wl,-soname,libint2dds_ffi.so.<major>, so anything that
    # links this library records the versioned name in DT_NEEDED. cargo only ever
    # writes the unversioned file, so the soname has to be linked in beside it or
    # the examples die at startup with "cannot open shared object file".
    # Major version comes from the same [workspace.package] version build.rs uses.
    file(READ "${CMAKE_CURRENT_SOURCE_DIR}/../../Cargo.toml" INT2DDS_CARGO_TOML)
    string(REGEX MATCH "\nversion *= *\"([0-9]+)\\." _ "${INT2DDS_CARGO_TOML}")
    set(INT2DDS_FFI_SOVERSION "${CMAKE_MATCH_1}")
    if(INT2DDS_FFI_SOVERSION STREQUAL "")
        message(FATAL_ERROR "Could not read [workspace.package] version from the repo-root Cargo.toml")
    endif()

    if(NOT EXISTS "${INT2DDS_FFI_LIB}")
        message(WARNING "${INT2DDS_FFI_LIB} not found - run cargo build --package int2dds-ffi first")
    elseif(NOT EXISTS "${INT2DDS_FFI_LIB}.${INT2DDS_FFI_SOVERSION}")
        # Relative link text, so the pair survives being copied elsewhere.
        execute_process(COMMAND ${CMAKE_COMMAND} -E create_symlink
            "libint2dds_ffi.so" "${INT2DDS_FFI_LIB}.${INT2DDS_FFI_SOVERSION}")
    endif()
endif()

include_directories(${INT2DDS_FFI_INCLUDE_DIR})

# Minimal Hello World smoke examples.
add_executable(hello_world_pub hello_world/hello_world_pub.c)
target_link_libraries(hello_world_pub ${INT2DDS_FFI_LIB})

add_executable(hello_world_sub hello_world/hello_world_sub.c)
target_link_libraries(hello_world_sub ${INT2DDS_FFI_LIB})

# ffi/build.rs gives the macOS dylib the install name @rpath/libint2dds_ffi.dylib,
# which dyld can only resolve if the consumer carries a matching rpath entry --
# without one the examples fail to launch even though the file is right there.
# Stated explicitly rather than left to CMake's implicit link-directory rpath,
# since this is the platform the CI matrix does not cover. On ELF it additionally
# saves setting LD_LIBRARY_PATH by hand; on Windows the property is ignored.
set_target_properties(hello_world_pub hello_world_sub PROPERTIES
    BUILD_RPATH "${INT2DDS_FFI_LIB_DIR}")

# Copy DLL next to the executables on Windows.
# Quoted because INT2DDS_FFI_DLL is only set on Windows/macOS.
if(WIN32 AND EXISTS "${INT2DDS_FFI_DLL}")
    foreach(target hello_world_pub hello_world_sub)
        add_custom_command(TARGET ${target} POST_BUILD
            COMMAND ${CMAKE_COMMAND} -E copy_if_different
            ${INT2DDS_FFI_DLL}
            $<TARGET_FILE_DIR:${target}>)
    endforeach()
endif()

message(STATUS "")
message(STATUS "=== int2dds FFI Hello World examples ===")
message(STATUS "  1. Build int2dds-ffi: cargo build --package int2dds-ffi")
message(STATUS "  2. mkdir build && cd build && cmake .. && cmake --build .")
message(STATUS "  Run: ./hello_world_pub / ./hello_world_sub")
message(STATUS "")
