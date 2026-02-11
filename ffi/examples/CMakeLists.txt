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

# Multiple Participant Examples
add_executable(multi_pub_sub multiple_participant/multi_pub_sub.c)
target_link_libraries(multi_pub_sub ${INT2DDS_FFI_LIB})

add_executable(multi_participant_1 multiple_participant/multi_participant_1.c)
target_link_libraries(multi_participant_1 ${INT2DDS_FFI_LIB})

add_executable(multi_participant_2 multiple_participant/multi_participant_2.c)
target_link_libraries(multi_participant_2 ${INT2DDS_FFI_LIB})

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

# Performance Test Examples
add_executable(perftest_publisher performance_test/perftest_publisher.c)
target_link_libraries(perftest_publisher ${INT2DDS_FFI_LIB})

add_executable(perftest_subscriber performance_test/perftest_subscriber.c)
target_link_libraries(perftest_subscriber ${INT2DDS_FFI_LIB})

# Performance Test (IDL/CDR) Examples - LatencyTest
add_executable(perftest_cdr_publisher LatencyTest/perftest_publisher.c)
target_include_directories(perftest_cdr_publisher PRIVATE ${CMAKE_CURRENT_SOURCE_DIR}/LatencyTest)
target_link_libraries(perftest_cdr_publisher ${INT2DDS_FFI_LIB})

add_executable(perftest_cdr_subscriber LatencyTest/perftest_subscriber.c)
target_include_directories(perftest_cdr_subscriber PRIVATE ${CMAKE_CURRENT_SOURCE_DIR}/LatencyTest)
target_link_libraries(perftest_cdr_subscriber ${INT2DDS_FFI_LIB})

# Listener Examples
add_executable(listener_publisher listener/listener_publisher.c)
target_link_libraries(listener_publisher ${INT2DDS_FFI_LIB})

add_executable(listener_subscriber listener/listener_subscriber.c)
target_link_libraries(listener_subscriber ${INT2DDS_FFI_LIB})

# Copy DLL to output directory on Windows
if(WIN32 AND EXISTS ${INT2DDS_FFI_DLL})
    add_custom_command(TARGET hello_world_publisher POST_BUILD
        COMMAND ${CMAKE_COMMAND} -E copy_if_different
        ${INT2DDS_FFI_DLL}
        $<TARGET_FILE_DIR:hello_world_publisher>)
    add_custom_command(TARGET hello_world_subscriber POST_BUILD
        COMMAND ${CMAKE_COMMAND} -E copy_if_different
        ${INT2DDS_FFI_DLL}
        $<TARGET_FILE_DIR:hello_world_subscriber>)
    add_custom_command(TARGET multi_pub_sub POST_BUILD
        COMMAND ${CMAKE_COMMAND} -E copy_if_different
        ${INT2DDS_FFI_DLL}
        $<TARGET_FILE_DIR:multi_pub_sub>)
    add_custom_command(TARGET waitset_subscriber POST_BUILD
        COMMAND ${CMAKE_COMMAND} -E copy_if_different
        ${INT2DDS_FFI_DLL}
        $<TARGET_FILE_DIR:waitset_subscriber>)
    add_custom_command(TARGET keyed_publisher POST_BUILD
        COMMAND ${CMAKE_COMMAND} -E copy_if_different
        ${INT2DDS_FFI_DLL}
        $<TARGET_FILE_DIR:keyed_publisher>)
    add_custom_command(TARGET keyed_subscriber POST_BUILD
        COMMAND ${CMAKE_COMMAND} -E copy_if_different
        ${INT2DDS_FFI_DLL}
        $<TARGET_FILE_DIR:keyed_subscriber>)
    add_custom_command(TARGET waitset_publisher POST_BUILD
        COMMAND ${CMAKE_COMMAND} -E copy_if_different
        ${INT2DDS_FFI_DLL}
        $<TARGET_FILE_DIR:waitset_publisher>)
    add_custom_command(TARGET multi_participant_1 POST_BUILD
        COMMAND ${CMAKE_COMMAND} -E copy_if_different
        ${INT2DDS_FFI_DLL}
        $<TARGET_FILE_DIR:multi_participant_1>)
    add_custom_command(TARGET multi_participant_2 POST_BUILD
        COMMAND ${CMAKE_COMMAND} -E copy_if_different
        ${INT2DDS_FFI_DLL}
        $<TARGET_FILE_DIR:multi_participant_2>)
    add_custom_command(TARGET perftest_publisher POST_BUILD
        COMMAND ${CMAKE_COMMAND} -E copy_if_different
        ${INT2DDS_FFI_DLL}
        $<TARGET_FILE_DIR:perftest_publisher>)
    add_custom_command(TARGET perftest_subscriber POST_BUILD
        COMMAND ${CMAKE_COMMAND} -E copy_if_different
        ${INT2DDS_FFI_DLL}
        $<TARGET_FILE_DIR:perftest_subscriber>)
    add_custom_command(TARGET listener_publisher POST_BUILD
        COMMAND ${CMAKE_COMMAND} -E copy_if_different
        ${INT2DDS_FFI_DLL}
        $<TARGET_FILE_DIR:listener_publisher>)
    add_custom_command(TARGET listener_subscriber POST_BUILD
        COMMAND ${CMAKE_COMMAND} -E copy_if_different
        ${INT2DDS_FFI_DLL}
        $<TARGET_FILE_DIR:listener_subscriber>)
    add_custom_command(TARGET perftest_cdr_publisher POST_BUILD
        COMMAND ${CMAKE_COMMAND} -E copy_if_different
        ${INT2DDS_FFI_DLL}
        $<TARGET_FILE_DIR:perftest_cdr_publisher>)
    add_custom_command(TARGET perftest_cdr_subscriber POST_BUILD
        COMMAND ${CMAKE_COMMAND} -E copy_if_different
        ${INT2DDS_FFI_DLL}
        $<TARGET_FILE_DIR:perftest_cdr_subscriber>)
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
message(STATUS "  ./perftest_publisher --help")
message(STATUS "  ./perftest_subscriber --help")
message(STATUS "")
message(STATUS "Performance test (old API) usage:")
message(STATUS "  Throughput: ./perftest_publisher -m throughput -s 1024 -t 30")
message(STATUS "              ./perftest_subscriber -m throughput -s 1024 -t 30")
message(STATUS "  Latency:    ./perftest_publisher -m latency -s 1024 -t 30")
message(STATUS "              ./perftest_subscriber -m latency -t 30")
message(STATUS "  Local lat:  ./perftest_publisher -m local_latency -s 1024 -t 30")
message(STATUS "              ./perftest_subscriber -m local_latency -s 1024 -t 30")
message(STATUS "")
message(STATUS "Performance test (IDL/CDR) usage:")
message(STATUS "  Throughput:     ./perftest_cdr_publisher --test-mode throughput --data-len 1024")
message(STATUS "                  ./perftest_cdr_subscriber --test-mode throughput --data-len 1024")
message(STATUS "  Latency:        ./perftest_cdr_publisher --test-mode latency --data-len 1024 --hz 1000")
message(STATUS "                  ./perftest_cdr_subscriber --test-mode latency")
message(STATUS "  Local latency:  ./perftest_cdr_publisher --test-mode local_latency --data-len 1024")
message(STATUS "                  ./perftest_cdr_subscriber --test-mode local_latency")
message(STATUS "")
