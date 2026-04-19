#include "HubClient.h"

#include <cstdio>
#include <string>

namespace music_ara_client {

const char* operationWireName(Operation op) {
    switch (op) {
        case Operation::GenerateSheetMusic: return "generate_sheet_music";
        case Operation::Repomix:            return "repomix";
        case Operation::Hitpoints:          return "hitpoints";
    }
    return "generate_sheet_music";
}

bool parseOperation(std::string_view wire, Operation& out) {
    if (wire == "generate_sheet_music") { out = Operation::GenerateSheetMusic; return true; }
    if (wire == "repomix")              { out = Operation::Repomix;            return true; }
    if (wire == "hitpoints")            { out = Operation::Hitpoints;          return true; }
    return false;
}

namespace {

void appendJsonEscaped(std::string& out, std::string_view value) {
    out.push_back('"');
    for (char c : value) {
        switch (c) {
            case '"':  out.append("\\\"");  break;
            case '\\': out.append("\\\\");  break;
            case '\b': out.append("\\b");   break;
            case '\f': out.append("\\f");   break;
            case '\n': out.append("\\n");   break;
            case '\r': out.append("\\r");   break;
            case '\t': out.append("\\t");   break;
            default:
                if (static_cast<unsigned char>(c) < 0x20) {
                    char buf[8];
                    std::snprintf(buf, sizeof(buf), "\\u%04x", static_cast<unsigned char>(c));
                    out.append(buf);
                } else {
                    out.push_back(c);
                }
        }
    }
    out.push_back('"');
}

}  // namespace

std::string buildJsonBody(const HubRequest& request) {
    std::string out;
    out.reserve(128 + request.absoluteFilePath.size());
    out.append("{\"target_type\":\"file\",\"target_id_or_path\":");
    appendJsonEscaped(out, request.absoluteFilePath);
    out.append(",\"operation\":");
    appendJsonEscaped(out, operationWireName(request.operation));
    out.push_back('}');
    return out;
}

bool sendRequest(const HubRequest& request,
                 std::string_view  url,
                 const HttpPoster& post,
                 std::string&      errorOut) {
    if (!post) {
        errorOut = "no HTTP poster configured";
        return false;
    }
    if (request.absoluteFilePath.empty()) {
        errorOut = "refusing to POST: empty file path (ARA audio source did not expose one)";
        return false;
    }
    const auto body = buildJsonBody(request);
    return post(url, body, errorOut);
}

}  // namespace music_ara_client
