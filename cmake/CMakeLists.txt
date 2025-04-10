cmake_minimum_required(VERSION 3.27)

if(NOT "$ENV{PICO_BUILD_ZIG_TARGET}" STREQUAL "")
    set(CMAKE_TOOLCHAIN_FILE "${CMAKE_CURRENT_LIST_DIR}/zig/zig-toolchain.cmake")
endif()

project(picobundler-cmake)

set(CMAKE_CXX_EXTENSIONS OFF)
set(CMAKE_CXX_STANDARD 17)
set(CMAKE_CXX_VISIBILITY_PRESET hidden)
set(CMAKE_OSX_ARCHITECTURES $ENV{PICO_BUILD_OSX_ARCH})
set(CMAKE_OSX_DEPLOYMENT_TARGET 10.13)
set(CMAKE_MSVC_RUNTIME_LIBRARY "MultiThreaded$<$<CONFIG:Debug>:Debug>")
set(CMAKE_VISIBILITY_INLINES_HIDDEN ON)
set(CMAKE_POSITION_INDEPENDENT_CODE ON)
set(CMAKE_BUILD_TYPE $ENV{PICO_BUILD_TYPE})

if (APPLE)
    enable_language(OBJC)
    enable_language(OBJCXX)
    set(CMAKE_OBJC_VISIBILITY_PRESET hidden)
    set(CMAKE_OBJCXX_VISIBILITY_PRESET hidden)
endif()

set(CLAP_SDK_ROOT "${CMAKE_CURRENT_LIST_DIR}/clap")
set(VST3_SDK_ROOT "$ENV{PICO_SDK_VST3}")
set(CLAP_WRAPPER_DOWNLOAD_DEPENDENCIES TRUE)
set(CLAP_WRAPPER_BUILD_AUV2 TRUE)
set(CLAP_WRAPPER_OUTPUT_NAME "$ENV{PICO_PLUGIN_NAME}")

# no more warnings for clap-wrapper
if (MSVC)
    add_compile_options(/W0)
else()
    add_compile_options(-w)
endif()

add_subdirectory(${CMAKE_CURRENT_LIST_DIR}/clap-wrapper)
include_directories(${CLAP_SDK_ROOT}/include)

add_custom_target(pico-wrappable-target DEPENDS "$ENV{PICO_PLUGIN_STATIC_LIB}")
add_library(pico-wrappable STATIC IMPORTED GLOBAL)
add_dependencies(pico-wrappable pico-wrappable-target)
target_link_libraries(pico-wrappable INTERFACE $ENV{PICO_BUILD_NATIVE_STATIC_LIBS})
set_target_properties(pico-wrappable PROPERTIES IMPORTED_LOCATION "$ENV{PICO_PLUGIN_STATIC_LIB}")

make_clapfirst_plugins(
    TARGET_NAME $ENV{PICO_PLUGIN_NAME}
    IMPL_TARGET pico-wrappable
    OUTPUT_NAME "$ENV{PICO_PLUGIN_NAME}"
    ENTRY_SOURCE "${CMAKE_CURRENT_LIST_DIR}/entry.cpp"

    COPY_AFTER_BUILD FALSE
    PLUGIN_FORMATS CLAP $ENV{PICO_PLUGIN_WANT_AUV2} $ENV{PICO_PLUGIN_WANT_VST3}
    ASSET_OUTPUT_DIRECTORY ${CMAKE_BINARY_DIR}/clap-wrapper-output/$ENV{PICO_PLUGIN_NAME}
)
