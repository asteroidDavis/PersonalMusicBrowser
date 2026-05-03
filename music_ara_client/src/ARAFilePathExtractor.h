// ARAFilePathExtractor — pure C++17 helper that turns an ARA audio-source
// identifier into an absolute file-system path.
//
// Why this lives in a header-only / non-JUCE translation unit:
//   * CI unit tests must run without JUCE or the ARA SDK present.
//   * The only ARA-host-specific behavior we care about is: *given the string
//     a host hands us as the audio source's persistent ID or name, what
//     absolute path does it map to?*
//
// Host conventions (verified against the Celemony ARA_SDK docs and the JUCE
// `ARAPluginDemo` project, 2024):
//   * REAPER          — `persistentID` is the absolute path, verbatim.
//   * Cubase / Nuendo — `persistentID` is an opaque GUID; the absolute path is
//                       instead the `name` field.  The plugin is expected to
//                       prefer `name` when it looks like a path.
//   * Studio One      — `persistentID` is a `file://` URL.
//   * Logic           — `persistentID` is an absolute POSIX path, often with
//                       a `//` prefix (we strip the duplicate leading slash).
//
// `extractAbsolutePath` tries each of the above rules in order and returns
// the first candidate that looks like an absolute file-system path.  If no
// candidate qualifies the result is empty — callers must treat empty as a
// hard error and surface it to the user (the hub will reject empty paths).

#pragma once

#include <string>
#include <string_view>

namespace music_ara_client {

struct AudioSourceIdentity {
    std::string persistentID;  // ARA::ARAAudioSourceProperties::persistentID
    std::string name;          // ARA::ARAAudioSourceProperties::name
};

/// Returns the absolute path encoded in `identity`, or an empty string if no
/// rule applied.  Never throws.
std::string extractAbsolutePath(const AudioSourceIdentity& identity);

/// Exposed for unit tests; strips `file://` and Logic's `//` prefix, returns
/// `candidate` unchanged if no known prefix applied.
std::string normalizeCandidate(std::string_view candidate);

/// Exposed for unit tests; returns true if `candidate` looks like an
/// absolute POSIX or Windows path (`/foo`, `C:\foo`, `C:/foo`).
bool looksAbsolute(std::string_view candidate);

}  // namespace music_ara_client
