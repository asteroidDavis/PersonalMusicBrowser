// Unit tests for HubClient — proves the exact JSON payload the Rust hub
// (`music_browser/src/main.rs::workflows_enqueue`) expects.
//
// We parse the generated JSON with a *hand-rolled* tiny parser (so the test
// itself has no external deps beyond GoogleTest) and assert field-by-field.
// If either side of the contract changes, these tests fail loudly with the
// offending JSON pretty-printed in the assertion message.

#include "HubClient.h"

#include <gtest/gtest.h>

#include <optional>
#include <string>
#include <vector>

using music_ara_client::buildJsonBody;
using music_ara_client::HubRequest;
using music_ara_client::Operation;
using music_ara_client::operationWireName;
using music_ara_client::parseOperation;
using music_ara_client::sendRequest;

namespace {

// Minimal string-value extractor: finds `"key":"value"` and returns value
// with JSON escapes decoded.  Good enough for our 3-field payload.
std::optional<std::string> findString(const std::string& json, const std::string& key) {
    const std::string needle = "\"" + key + "\":\"";
    auto pos = json.find(needle);
    if (pos == std::string::npos) return std::nullopt;
    pos += needle.size();
    std::string out;
    while (pos < json.size() && json[pos] != '"') {
        if (json[pos] == '\\' && pos + 1 < json.size()) {
            char esc = json[pos + 1];
            switch (esc) {
                case '"':
                    out.push_back('"');
                    break;
                case '\\':
                    out.push_back('\\');
                    break;
                case 'n':
                    out.push_back('\n');
                    break;
                case 'r':
                    out.push_back('\r');
                    break;
                case 't':
                    out.push_back('\t');
                    break;
                default:
                    out.push_back(esc);
                    break;
            }
            pos += 2;
        } else {
            out.push_back(json[pos++]);
        }
    }
    if (pos >= json.size()) return std::nullopt;
    return out;
}

TEST(BuildJsonBody, MatchesHubSchemaForFileTarget) {
    HubRequest req;
    req.absoluteFilePath = "/home/nate/music/clip.wav";
    req.operation = Operation::GenerateSheetMusic;

    const auto json = buildJsonBody(req);

    auto targetType = findString(json, "target_type");
    auto path = findString(json, "target_id_or_path");
    auto operation = findString(json, "operation");

    ASSERT_TRUE(targetType.has_value()) << "target_type missing in payload: " << json;
    ASSERT_TRUE(path.has_value()) << "target_id_or_path missing in payload: " << json;
    ASSERT_TRUE(operation.has_value()) << "operation missing in payload: " << json;

    EXPECT_EQ(*targetType, "file") << "payload: " << json;
    EXPECT_EQ(*path, "/home/nate/music/clip.wav") << "payload: " << json;
    EXPECT_EQ(*operation, "generate_sheet_music") << "payload: " << json;
}

TEST(BuildJsonBody, EscapesDoubleQuotesAndBackslashes) {
    HubRequest req;
    req.absoluteFilePath = R"(C:\Users\nate\she said "hi".wav)";
    req.operation = Operation::Repomix;

    const auto json = buildJsonBody(req);
    auto path = findString(json, "target_id_or_path");
    ASSERT_TRUE(path.has_value()) << "path missing in payload: " << json;
    EXPECT_EQ(*path, R"(C:\Users\nate\she said "hi".wav)") << "payload: " << json;
}

TEST(BuildJsonBody, EscapesControlCharacters) {
    HubRequest req;
    req.absoluteFilePath = std::string("/tmp/a\tb\nc.wav");
    req.operation = Operation::Hitpoints;

    const auto json = buildJsonBody(req);
    // Raw JSON must NOT contain the unescaped control bytes.
    EXPECT_EQ(json.find('\t'), std::string::npos) << "raw tab present in JSON (should be \\t): " << json;
    EXPECT_EQ(json.find('\n'), std::string::npos) << "raw newline present in JSON (should be \\n): " << json;
}

TEST(OperationWire, RoundTripsAllVariants) {
    for (auto op : {Operation::GenerateSheetMusic, Operation::Repomix, Operation::Hitpoints}) {
        const std::string wire = operationWireName(op);
        Operation parsed{};
        ASSERT_TRUE(parseOperation(wire, parsed)) << "failed to parse wire name '" << wire << "' back to Operation";
        EXPECT_EQ(static_cast<int>(parsed), static_cast<int>(op))
            << "wire name '" << wire << "' round-tripped to a different enum value";
    }
}

TEST(OperationWire, RejectsUnknownStrings) {
    Operation parsed = Operation::Hitpoints;
    EXPECT_FALSE(parseOperation("unknown_op", parsed)) << "parseOperation should reject unknown strings";
    EXPECT_EQ(static_cast<int>(parsed), static_cast<int>(Operation::Hitpoints))
        << "out parameter must be left untouched on failure";
}

TEST(SendRequest, RefusesEmptyPathWithoutCallingPoster) {
    HubRequest req;  // absoluteFilePath deliberately empty
    req.operation = Operation::GenerateSheetMusic;

    bool posterCalled = false;
    auto poster = [&](std::string_view, const HubRequest&, std::string&) {
        posterCalled = true;
        return true;
    };
    std::string err;
    const bool ok = sendRequest(req, "http://localhost:3000/api/workflows", poster, err);
    EXPECT_FALSE(ok) << "must refuse empty path";
    EXPECT_FALSE(posterCalled) << "poster must not be invoked for empty path";
    EXPECT_NE(err.find("empty file path"), std::string::npos) << "expected 'empty file path' in error, got: " << err;
}

TEST(SendRequest, PassesBuiltBodyToPoster) {
    HubRequest req;
    req.absoluteFilePath = "/abs/song.wav";
    req.operation = Operation::Repomix;

    std::string seenBody;
    std::string seenUrl;
    auto poster = [&](std::string_view url, const HubRequest& r, std::string&) {
        seenUrl.assign(url);
        seenBody.assign(buildJsonBody(r));
        return true;
    };
    std::string err;
    EXPECT_TRUE(sendRequest(req, "http://localhost:3000/api/workflows", poster, err)) << "unexpected error: " << err;
    EXPECT_EQ(seenUrl, "http://localhost:3000/api/workflows");
    EXPECT_EQ(seenBody, buildJsonBody(req)) << "sendRequest must pass the canonical body to the poster";
}

TEST(SendRequest, SurfacesPosterError) {
    HubRequest req;
    req.absoluteFilePath = "/abs/song.wav";
    auto poster = [](std::string_view, const HubRequest&, std::string& errorOut) {
        errorOut = "boom";
        return false;
    };
    std::string err;
    EXPECT_FALSE(sendRequest(req, "http://localhost:3000/api/workflows", poster, err));
    EXPECT_EQ(err, "boom") << "sendRequest must surface the poster's error message verbatim";
}

}  // namespace
