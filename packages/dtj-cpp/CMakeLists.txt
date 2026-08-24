cmake_minimum_required(VERSION 3.16)
project(dtj-cpp LANGUAGES CXX)

set(CMAKE_CXX_STANDARD 17)
set(CMAKE_CXX_STANDARD_REQUIRED ON)
set(CMAKE_CXX_EXTENSIONS OFF)

# Compiler flags
set(CMAKE_CXX_FLAGS "${CMAKE_CXX_FLAGS} -Wall -Wextra -Wpedantic")

if(CMAKE_BUILD_TYPE STREQUAL "Debug")
    set(CMAKE_CXX_FLAGS "${CMAKE_CXX_FLAGS} -g -O0 -fsanitize=address -fno-omit-frame-pointer")
    set(CMAKE_EXE_LINKER_FLAGS "${CMAKE_EXE_LINKER_FLAGS} -fsanitize=address")
elseif(CMAKE_BUILD_TYPE STREQUAL "Release")
    set(CMAKE_CXX_FLAGS "${CMAKE_CXX_FLAGS} -O3 -DNDEBUG")
endif()

# Find dtj-c library (assumes built in sibling directory)
set(DTJ_C_DIR "${CMAKE_CURRENT_SOURCE_DIR}/../dtj-c" CACHE PATH "Path to dtj-c source directory")
find_library(DTJ_LIB dtj PATHS "${DTJ_C_DIR}/build" NO_DEFAULT_PATH)

if(NOT DTJ_LIB)
    message(WARNING "dtj-c library not found. Building without dtj-c linking. Run cmake on dtj-c first.")
else()
    message(STATUS "Found dtj-c: ${DTJ_LIB}")
endif()

# Include directories
include_directories(include)

# Library target (header-only wrapper, links to dtj-c)
add_library(dtj_cpp INTERFACE)
target_include_directories(dtj_cpp INTERFACE include)
target_link_libraries(dtj_cpp INTERFACE ${DTJ_LIB})

# Tests
enable_testing()

add_executable(cpp_wrapper_tests tests/cpp_wrapper_tests.cpp)
target_link_libraries(cpp_wrapper_tests dtj_cpp)

add_test(NAME cpp_wrapper_tests COMMAND cpp_wrapper_tests)

# E2E test (opt-in via environment variable)
add_executable(cpp_e2e tests/cpp_e2e_tests.cpp)
target_link_libraries(cpp_e2e dtj_cpp)