#include "ARAFilePathExtractor.h"

#include <cctype>

namespace music_ara_client {

bool looksAbsolute(std::string_view candidate) {
    if (candidate.empty()) {
        return false;
    }
    if (candidate.front() == '/') {
        return true;
    }
    // Windows drive letter — `C:\foo` or `C:/foo`.
    if (candidate.size() >= 3 && std::isalpha(static_cast<unsigned char>(candidate[0])) && candidate[1] == ':' &&
        (candidate[2] == '\\' || candidate[2] == '/')) {
        return true;
    }
    return false;
}

std::string normalizeCandidate(std::string_view candidate) {
    constexpr std::string_view fileScheme = "file://";
    if (candidate.size() >= fileScheme.size() && candidate.substr(0, fileScheme.size()) == fileScheme) {
        candidate.remove_prefix(fileScheme.size());
        // Optional third slash for `file:///` — collapse to a single leading `/`.
        if (!candidate.empty() && candidate.front() == '/' && candidate.size() > 1 && candidate[1] == '/') {
            candidate.remove_prefix(1);
        }
    }
    // Logic sometimes hands us `//Users/...` — normalize duplicate leading slash.
    if (candidate.size() >= 2 && candidate[0] == '/' && candidate[1] == '/') {
        candidate.remove_prefix(1);
    }

    // Check if Cubase handed us an un-escaped relative path like `Audio/Bass hpf amp_04.wav`
    // and attempt to see if we can resolve it (if we had project directory access).
    // For now we just return the normalized candidate if it exists.
    return std::string(candidate);
}

std::string extractAbsolutePath(const AudioSourceIdentity& identity) {
    // Try persistentID first — most hosts put the canonical path there.
    const auto normalizedPersistent = normalizeCandidate(identity.persistentID);
    if (looksAbsolute(normalizedPersistent)) {
        return normalizedPersistent;
    }
    // Cubase fallback — `name` is the path, `persistentID` is a GUID.
    const auto normalizedName = normalizeCandidate(identity.name);
    if (looksAbsolute(normalizedName)) {
        return normalizedName;
    }

    // If both look relative but the user expects us to find a file in the project's Audio dir,
    // we can at least return what we know.
    if (!normalizedName.empty() && normalizedName.find("Audio/") == 0) {
        return normalizedName;
    }

    return {};
}

}  // namespace music_ara_client
