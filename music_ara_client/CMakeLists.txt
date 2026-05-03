# music_ara_client — JUCE ARA plugin that posts selected-clip file paths to
# the music_browser Rust hub.
#
# Phase 4 of the DAW-integration roadmap (issue #4, PR #25).
#
# Targets:
#   * SendToHubPlugin_VST3  — the ARA-enabled plugin, built when JUCE + ARA_SDK
#                             are available.
#   * sendtohub_core_tests  — host-free GoogleTest unit tests for the pure-C++
#                             helpers (`ARAFilePathExtractor`, `HubClient`).
#                             These always build and run in CI.
#
# Design goal: the *unit tests* never depend on JUCE or the ARA SDK so CI can
# validate the wire contract in ~1 minute even when the plugin build is slow
# or the ARA SDK is unavailable.

cmake_minimum_required(VERSION 3.22)
project(music_ara_client LANGUAGES C CXX VERSION 1.0.0)

set(CMAKE_CXX_STANDARD 17)
set(CMAKE_CXX_STANDARD_REQUIRED ON)
set(CMAKE_CXX_EXTENSIONS OFF)

option(MUSIC_ARA_BUILD_PLUGIN "Build the JUCE ARA plugin (requires network, ~10 min first build)" OFF)
option(MUSIC_ARA_BUILD_TESTS  "Build host-free unit tests"                                         ON)

# ----------------------------------------------------------------------------
# Core library — pure C++17, no JUCE, no ARA.  Encapsulates the logic that
# has to be covered by fast, deterministic unit tests.
# ----------------------------------------------------------------------------
add_library(sendtohub_core STATIC
    src/ARAFilePathExtractor.cpp
    src/HubClient.cpp
)
target_include_directories(sendtohub_core PUBLIC src)
target_compile_features(sendtohub_core PUBLIC cxx_std_17)

if(MSVC)
    target_compile_options(sendtohub_core PRIVATE /W4 /WX)
else()
    target_compile_options(sendtohub_core PRIVATE -Wall -Wextra -Wpedantic -Werror)
endif()

# ----------------------------------------------------------------------------
# Unit tests (always built by default).
# ----------------------------------------------------------------------------
if(MUSIC_ARA_BUILD_TESTS)
    include(FetchContent)
    FetchContent_Declare(
        googletest
        GIT_REPOSITORY https://github.com/google/googletest.git
        GIT_TAG        v1.15.2
    )
    set(gtest_force_shared_crt ON CACHE BOOL "" FORCE)
    FetchContent_MakeAvailable(googletest)

    enable_testing()
    add_executable(sendtohub_core_tests
        tests/test_ara_file_path_extractor.cpp
        tests/test_hub_client.cpp
    )
    target_link_libraries(sendtohub_core_tests PRIVATE sendtohub_core GTest::gtest_main)
    include(GoogleTest)
    gtest_discover_tests(sendtohub_core_tests)
endif()

# ----------------------------------------------------------------------------
# JUCE ARA plugin (opt-in — heavy).
#
# Requirements:
#   * JUCE  >= 7.0.8  (fetched from GitHub by default).
#   * ARA_SDK         (provide via -DARA_SDK_DIR=/path/to/ARA_SDK or fetched).
#
# Enable with:  cmake -B build -DMUSIC_ARA_BUILD_PLUGIN=ON -DARA_SDK_DIR=...
# ----------------------------------------------------------------------------
if(MUSIC_ARA_BUILD_PLUGIN)
    include(FetchContent)

    FetchContent_Declare(
        JUCE
        GIT_REPOSITORY https://github.com/juce-framework/JUCE.git
        GIT_TAG        8.0.4
    )
    FetchContent_MakeAvailable(JUCE)

    if(NOT DEFINED ARA_SDK_DIR OR "${ARA_SDK_DIR}" STREQUAL "")
        FetchContent_Declare(
            ARA_SDK
            GIT_REPOSITORY https://github.com/Celemony/ARA_SDK.git
            GIT_TAG        releases/2.2.0
        )
        FetchContent_MakeAvailable(ARA_SDK)
        set(ARA_SDK_DIR "${ara_sdk_SOURCE_DIR}" CACHE PATH "" FORCE)
    endif()

    juce_set_ara_sdk_path("${ARA_SDK_DIR}")

    find_package(CURL REQUIRED)

    juce_add_plugin(SendToHubPlugin
        COMPANY_NAME           "PersonalMusicBrowser"
        PLUGIN_MANUFACTURER_CODE PmbR
        PLUGIN_CODE            SthP
        FORMATS                VST3
        PRODUCT_NAME           "Send To Music Browser"
        IS_ARA_EFFECT          TRUE
        NEEDS_ARA              TRUE
    )

    target_sources(SendToHubPlugin PRIVATE
        src/PluginProcessor.cpp
        src/PluginEditor.cpp
    )

    target_compile_definitions(SendToHubPlugin PUBLIC
        JUCE_WEB_BROWSER=0
        JUCE_USE_CURL=1
        JUCE_VST3_CAN_REPLACE_VST2=0
    )

    target_link_libraries(SendToHubPlugin PRIVATE
        sendtohub_core
        juce::juce_audio_utils
        juce::juce_audio_processors
        juce::juce_gui_extra
        juce::juce_recommended_config_flags
        juce::juce_recommended_lto_flags
        juce::juce_recommended_warning_flags
        CURL::libcurl
    )
endif()
