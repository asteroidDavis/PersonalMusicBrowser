// Host-free unit tests for the ARA file-path extractor.
//
// The production plugin only ever talks to ARA via JUCE, but the *rules* by
// which we turn an ARA audio source into an absolute path are pure string
// manipulation and deserve coverage that doesn't require a DAW.  Each case
// encodes one real-world host convention (REAPER / Cubase / Studio One /
// Logic) and fails loudly with the offending input in the assertion message.

#include "ARAFilePathExtractor.h"

#include <gtest/gtest.h>

#include <string>
#include <tuple>

using music_ara_client::AudioSourceIdentity;
using music_ara_client::extractAbsolutePath;
using music_ara_client::looksAbsolute;
using music_ara_client::normalizeCandidate;

namespace {

struct HostCase {
    const char* label;
    AudioSourceIdentity in;
    std::string         expected;
};

class HostConventionTest : public ::testing::TestWithParam<HostCase> {};

TEST_P(HostConventionTest, ResolvesExpectedPath) {
    const auto& c = GetParam();
    const auto got = extractAbsolutePath(c.in);
    EXPECT_EQ(got, c.expected)
        << "host=" << c.label
        << " persistentID=\"" << c.in.persistentID << "\""
        << " name=\""         << c.in.name         << "\"";
}

INSTANTIATE_TEST_SUITE_P(
    Hosts, HostConventionTest,
    ::testing::Values(
        HostCase{"REAPER_posix",
                 {"/home/nate/audio/clip.wav", "clip.wav"},
                 "/home/nate/audio/clip.wav"},
        HostCase{"Cubase_guid_with_path_in_name",
                 {"{8F2B3C5D-1234-4B56-9E01-ABCDEF012345}", "/Users/nate/OneDrive/track.wav"},
                 "/Users/nate/OneDrive/track.wav"},
        HostCase{"StudioOne_file_url",
                 {"file:///Users/nate/music/loop.wav", "loop.wav"},
                 "/Users/nate/music/loop.wav"},
        HostCase{"Logic_double_slash_prefix",
                 {"//Users/nate/Library/take.wav", "take"},
                 "/Users/nate/Library/take.wav"},
        HostCase{"Windows_drive_letter",
                 {"C:/Users/nate/audio/clip.wav", "clip.wav"},
                 "C:/Users/nate/audio/clip.wav"},
        HostCase{"Windows_backslash_drive",
                 {R"(D:\Music\song.wav)", "song"},
                 R"(D:\Music\song.wav)"},
        HostCase{"Both_opaque_returns_empty",
                 {"uuid:abc-123", "unnamed"},
                 ""}),
    [](const ::testing::TestParamInfo<HostCase>& info) { return std::string(info.param.label); });

TEST(AbsoluteDetection, RejectsRelativeAndEmpty) {
    EXPECT_FALSE(looksAbsolute(""))               << "empty string must not be treated as absolute";
    EXPECT_FALSE(looksAbsolute("relative/path"))  << "relative POSIX must be rejected";
    EXPECT_FALSE(looksAbsolute("C:"))             << "bare drive letter must be rejected";
}

TEST(Normalize, PassesThroughUnknownPrefixes) {
    EXPECT_EQ(normalizeCandidate("/already/normal.wav"), "/already/normal.wav");
    EXPECT_EQ(normalizeCandidate("opaque-id"),           "opaque-id");
}

TEST(Normalize, StripsFileScheme) {
    EXPECT_EQ(normalizeCandidate("file:///a/b.wav"), "/a/b.wav");
    EXPECT_EQ(normalizeCandidate("file://host/a.wav"), "host/a.wav")
        << "file://<host>/... is non-local and should not be treated as absolute POSIX";
}

}  // namespace
