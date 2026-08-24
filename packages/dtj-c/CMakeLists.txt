cmake_minimum_required(VERSION 3.16)
project(dtj-c VERSION 0.1.0 LANGUAGES C)

set(CMAKE_C_STANDARD 11)
set(CMAKE_C_STANDARD_REQUIRED ON)
set(CMAKE_C_EXTENSIONS OFF)

# Compiler flags
set(CMAKE_C_FLAGS "${CMAKE_C_FLAGS} -Wall -Wextra -Wpedantic -fPIC")

if(CMAKE_BUILD_TYPE STREQUAL "Debug")
    set(CMAKE_C_FLAGS "${CMAKE_C_FLAGS} -g -O0 -fsanitize=address -fno-omit-frame-pointer")
    set(CMAKE_EXE_LINKER_FLAGS "${CMAKE_EXE_LINKER_FLAGS} -fsanitize=address")
elseif(CMAKE_BUILD_TYPE STREQUAL "Release")
    set(CMAKE_C_FLAGS "${CMAKE_C_FLAGS} -O3 -DNDEBUG")
endif()

# Source files
set(DTJ_SOURCES
    src/errors.c
    src/protocol.c
    src/discovery.c
    src/client.c
)

# Include directory
include_directories(include)

# Library target
add_library(dtj SHARED ${DTJ_SOURCES})
target_include_directories(dtj PUBLIC include)
set_target_properties(dtj PROPERTIES
    VERSION ${PROJECT_VERSION}
    SOVERSION 1
    POSITION_INDEPENDENT_CODE ON
    C_STANDARD 11
    C_STANDARD_REQUIRED ON
)

# Install targets
install(TARGETS dtj
    LIBRARY DESTINATION lib
    ARCHIVE DESTINATION lib
    RUNTIME DESTINATION bin
)
install(DIRECTORY include/ DESTINATION include)

# Tests
enable_testing()

# Test executable
add_executable(dtj_tests tests/protocol_tests.c tests/client_tests.c)
target_link_libraries(dtj_tests dtj)
target_include_directories(dtj_tests PRIVATE include)

add_test(NAME dtj_unit_tests COMMAND dtj_tests)

# E2E test (separate, opt-in)
add_executable(dtj_e2e tests/e2e_tests.c)
target_link_libraries(dtj_e2e dtj)
target_include_directories(dtj_e2e PRIVATE include)

# Only run E2E if DTJ_RUN_AGENT_E2E=1 is set via ctest --extra-args or environment

# pkg-config file generation
configure_file(
    ${CMAKE_CURRENT_SOURCE_DIR}/dtj.pc.in
    ${CMAKE_CURRENT_BINARY_DIR}/dtj.pc
    @ONLY
)
install(FILES ${CMAKE_CURRENT_BINARY_DIR}/dtj.pc DESTINATION lib/pkgconfig)