cmake_minimum_required(VERSION 3.10)
project(int2dds_cdr_tests C)

set(CMAKE_C_STANDARD 99)
set(CMAKE_C_STANDARD_REQUIRED ON)

# int2dds_cdr.h is header-only under INT2DDS_CDR_STATIC, so these tests link
# against nothing and need no built artifact.
add_executable(test_cdr_bounds test_cdr_bounds.c)
target_include_directories(test_cdr_bounds PRIVATE "${CMAKE_CURRENT_SOURCE_DIR}/../../include")

if(MSVC)
    target_compile_options(test_cdr_bounds PRIVATE /W4)
else()
    target_compile_options(test_cdr_bounds PRIVATE -Wall -Wextra)
endif()

# Off by default; the CI job that turns this on is tracked in #381.
option(INT2DDS_CDR_TEST_SANITIZE "Build the C CDR tests with ASan/UBSan" OFF)
if(INT2DDS_CDR_TEST_SANITIZE AND NOT MSVC)
    target_compile_options(test_cdr_bounds PRIVATE -fsanitize=address,undefined -fno-omit-frame-pointer -g)
    target_link_options(test_cdr_bounds PRIVATE -fsanitize=address,undefined)
endif()

enable_testing()
add_test(NAME cdr_bounds COMMAND test_cdr_bounds)
