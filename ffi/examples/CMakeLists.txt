cmake_minimum_required(VERSION 3.10)
project(int2dds_ffi_examples C)

set(CMAKE_C_STANDARD 99)
set(CMAKE_C_STANDARD_REQUIRED ON)

# Find the int2dds-ffi library
# Adjust these paths based on your build configuration
set(INT2DDS_FFI_INCLUDE_DIR "${CMAKE_CURRENT_SOURCE_DIR}/../include")

# Determine library path based on build type
if(CMAKE_BUILD_TYPE STREQUAL "Release")
    set(INT2DDS_FFI_LIB_DIR "${CMAKE_CURRENT_SOURCE_DIR}/../../target/release")
else()
    set(INT2DDS_FFI_LIB_DIR "${CMAKE_CURRENT_SOURCE_DIR}/../../target/debug")
endif()

# Platform-specific library name
if(WIN32)
    set(INT2DDS_FFI_LIB "${INT2DDS_FFI_LIB_DIR}/int2dds_ffi.dll.lib")
    set(INT2DDS_FFI_DLL "${INT2DDS_FFI_LIB_DIR}/int2dds_ffi.dll")
elseif(APPLE)
    set(INT2DDS_FFI_LIB "${INT2DDS_FFI_LIB_DIR}/libint2dds_ffi.dylib")
    set(INT2DDS_FFI_DLL "${INT2DDS_FFI_LIB_DIR}/libint2dds_ffi.dylib")
else()
    set(INT2DDS_FFI_LIB "${INT2DDS_FFI_LIB_DIR}/libint2dds_ffi.so")
endif()

# Include directories
include_directories(${INT2DDS_FFI_INCLUDE_DIR})

# Hello World Examples
add_executable(hello_world_publisher hello_world/hello_world_publisher.c)
target_link_libraries(hello_world_publisher ${INT2DDS_FFI_LIB})

add_executable(hello_world_subscriber hello_world/hello_world_subscriber.c)
target_link_libraries(hello_world_subscriber ${INT2DDS_FFI_LIB})

# Hello World with multicast TTL Examples (PropertyQosPolicy)
add_executable(hello_world_with_ttl_publisher hello_world/hello_world_with_ttl_publisher.c)
target_link_libraries(hello_world_with_ttl_publisher ${INT2DDS_FFI_LIB})

add_executable(hello_world_with_ttl_subscriber hello_world/hello_world_with_ttl_subscriber.c)
target_link_libraries(hello_world_with_ttl_subscriber ${INT2DDS_FFI_LIB})

# WaitSet Examples
add_executable(waitset_publisher hello_world/waitset_publisher.c)
target_link_libraries(waitset_publisher ${INT2DDS_FFI_LIB})

add_executable(waitset_subscriber hello_world/waitset_subscriber.c)
target_link_libraries(waitset_subscriber ${INT2DDS_FFI_LIB})

# Keyed Data Examples
add_executable(keyed_publisher hello_world/keyed_publisher.c)
target_link_libraries(keyed_publisher ${INT2DDS_FFI_LIB})

add_executable(keyed_subscriber hello_world/keyed_subscriber.c)
target_link_libraries(keyed_subscriber ${INT2DDS_FFI_LIB})

# Listener Examples
add_executable(listener_publisher listener/listener_publisher.c)
target_link_libraries(listener_publisher ${INT2DDS_FFI_LIB})

add_executable(listener_subscriber listener/listener_subscriber.c)
target_link_libraries(listener_subscriber ${INT2DDS_FFI_LIB})

# Multiple Participant Examples
add_executable(multi_pub_sub multiple_participant/multi_pub_sub.c)
target_link_libraries(multi_pub_sub ${INT2DDS_FFI_LIB})

add_executable(multi_participant_1 multiple_participant/multi_participant_1.c)
target_link_libraries(multi_participant_1 ${INT2DDS_FFI_LIB})

add_executable(multi_participant_2 multiple_participant/multi_participant_2.c)
target_link_libraries(multi_participant_2 ${INT2DDS_FFI_LIB})

# Dynamic XTypes Examples (PHASE 1)
add_executable(dynamic_type_publisher xtypes/dynamic_type_publisher.c)
target_link_libraries(dynamic_type_publisher ${INT2DDS_FFI_LIB})

add_executable(dynamic_type_subscriber xtypes/dynamic_type_subscriber.c)
target_link_libraries(dynamic_type_subscriber ${INT2DDS_FFI_LIB})

# Copy DLL to output directory on Windows
if(WIN32 AND EXISTS ${INT2DDS_FFI_DLL})
    foreach(target
        hello_world_publisher hello_world_subscriber
        waitset_publisher waitset_subscriber
        keyed_publisher keyed_subscriber
        listener_publisher listener_subscriber
        multi_pub_sub multi_participant_1 multi_participant_2
        dynamic_type_publisher dynamic_type_subscriber)
        add_custom_command(TARGET ${target} POST_BUILD
            COMMAND ${CMAKE_COMMAND} -E copy_if_different
            ${INT2DDS_FFI_DLL}
            $<TARGET_FILE_DIR:${target}>)
    endforeach()
endif()

# Instructions
message(STATUS "")
message(STATUS "=== int2dds FFI Examples ===")
message(STATUS "Build instructions:")
message(STATUS "  1. First build int2dds-ffi: cargo build --package int2dds-ffi")
message(STATUS "  2. Create build directory: mkdir build && cd build")
message(STATUS "  3. Generate build files: cmake ..")
message(STATUS "  4. Build examples: cmake --build .")
message(STATUS "")
message(STATUS "Run examples:")
message(STATUS "  ./hello_world_publisher")
message(STATUS "  ./hello_world_subscriber")
message(STATUS "  ./waitset_publisher")
message(STATUS "  ./waitset_subscriber")
message(STATUS "  ./keyed_publisher")
message(STATUS "  ./keyed_subscriber")
message(STATUS "  ./listener_publisher")
message(STATUS "  ./listener_subscriber")
message(STATUS "  ./multi_pub_sub")
message(STATUS "  ./multi_participant_1")
message(STATUS "  ./multi_participant_2")
message(STATUS "  ./dynamic_type_publisher")
message(STATUS "  ./dynamic_type_subscriber")
message(STATUS "")
