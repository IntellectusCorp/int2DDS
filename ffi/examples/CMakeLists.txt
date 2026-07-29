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

if(WIN32)
    set(INT2DDS_FFI_LIB "${INT2DDS_FFI_LIB_DIR}/int2dds_ffi.dll.lib")
    set(INT2DDS_FFI_DLL "${INT2DDS_FFI_LIB_DIR}/int2dds_ffi.dll")
elseif(APPLE)
    set(INT2DDS_FFI_LIB "${INT2DDS_FFI_LIB_DIR}/libint2dds_ffi.dylib")
    set(INT2DDS_FFI_DLL "${INT2DDS_FFI_LIB_DIR}/libint2dds_ffi.dylib")
else()
    set(INT2DDS_FFI_LIB "${INT2DDS_FFI_LIB_DIR}/libint2dds_ffi.so")
endif()

include_directories(${INT2DDS_FFI_INCLUDE_DIR})

# Minimal Hello World smoke examples.
add_executable(hello_world_pub hello_world/hello_world_pub.c)
target_link_libraries(hello_world_pub ${INT2DDS_FFI_LIB})

add_executable(hello_world_sub hello_world/hello_world_sub.c)
target_link_libraries(hello_world_sub ${INT2DDS_FFI_LIB})

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
