// HubClient — tiny wrapper that builds the exact JSON payload the Rust hub
// expects at `POST /api/workflows` and (optionally) ships it over HTTP.
//
// The wire contract is defined by `WorkflowRequest` in
// `music_browser/src/main.rs`:
//     { "target_type": "file",
//       "target_id_or_path": "<absolute path>",
//       "operation":        "generate_sheet_music" | "repomix" | "hitpoints" }
//
// The JSON builder is pure C++ with zero dependencies so it is trivially
// unit-testable off-host.  The actual network send is provided by the plugin
// via a callback (injected from `PluginEditor` with `juce::URL`) — this keeps
// JUCE out of the core library and the unit tests fast and deterministic.

#pragma once

#include <functional>
#include <string>
#include <string_view>

namespace music_ara_client {

enum class Operation {
    GenerateSheetMusic,
    Repomix,
    Hitpoints,
};

/// Returns the hub's canonical string form (`"generate_sheet_music"` etc.).
/// Matches `Operation::as_str` in `music_browser/src/jobs.rs`.
const char* operationWireName(Operation op);

/// Parses a wire name back to `Operation`.  Mirror of
/// `music_browser/src/jobs.rs::Operation::parse`.  Returns false on unknown
/// strings and leaves `out` untouched.
bool parseOperation(std::string_view wire, Operation& out);

struct HubRequest {
    std::string absoluteFilePath;
    Operation   operation = Operation::GenerateSheetMusic;
};

/// Builds the JSON body exactly as the hub expects.  Performs minimal but
/// correct JSON string escaping (quote, backslash, control chars).
std::string buildJsonBody(const HubRequest& request);

/// Default hub endpoint — the Rust web app's local server.
constexpr std::string_view kDefaultHubUrl = "http://localhost:3000/api/workflows";

using HttpPoster =
    std::function<bool(std::string_view url, std::string_view jsonBody, std::string& errorOut)>;

/// Sends `request` to `url` via `post` and writes any transport error into
/// `errorOut`.  Returns true iff the POST completed with a 2xx response
/// (signalling is delegated to `post`).
bool sendRequest(const HubRequest& request,
                 std::string_view  url,
                 const HttpPoster& post,
                 std::string&      errorOut);

}  // namespace music_ara_client
