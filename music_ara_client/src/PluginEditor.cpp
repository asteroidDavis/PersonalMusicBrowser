#include "PluginEditor.h"
#include "PluginProcessor.h"
#include "ARAFilePathExtractor.h"

namespace music_ara_client {

namespace {

constexpr int kOpIdGenerateSheetMusic = 1;
constexpr int kOpIdHitpoints = 2;
constexpr int kOpIdRepomix = 3;

Operation operationFromComboId(int id) {
    switch (id) {
        case kOpIdHitpoints:
            return Operation::Hitpoints;
        case kOpIdRepomix:
            return Operation::Repomix;
        default:
            return Operation::GenerateSheetMusic;
    }
}

// Thin adapter: `juce::URL` → `music_ara_client::HttpPoster`.  Kept out of
// `sendtohub_core` so the unit tests stay JUCE-free.
HttpPoster makeJuceHttpPoster() {
    return [](std::string_view url, const HubRequest& request, std::string& errorOut) -> bool {
        auto u = juce::URL(juce::String(std::string(url)));
        auto options = juce::URL::InputStreamOptions(juce::URL::ParameterHandling::inPostData)
                           .withConnectionTimeoutMs(30000);

        juce::URL postUrl = u;

        if (!request.exportedWavPath.empty()) {
            juce::File audioFile(juce::String(request.exportedWavPath));
            if (!audioFile.existsAsFile()) {
                errorOut = "Exported WAV missing: " + request.exportedWavPath;
                return false;
            }
            postUrl =
                postUrl.withParameter("target_type", "file")
                    .withParameter("target_id_or_path", juce::String(request.absoluteFilePath))
                    .withParameter("operation", juce::String(operationWireName(request.operation)))
                    .withFileToUpload("audio_file", audioFile, "audio/wav");
        } else {
            postUrl = postUrl.withPOSTData(juce::String(buildJsonBody(request)));
        }

        int statusCode = 0;
        auto stream = postUrl.createInputStream(
            request.exportedWavPath.empty()
                ? options.withExtraHeaders("Content-Type: application/json")
                      .withStatusCode(&statusCode)
                : options.withStatusCode(&statusCode));
        if (stream == nullptr) {
            errorOut = "failed to open POST stream to " + std::string(url);
            return false;
        }
        // Drain — the hub returns JSON we don't need in the UI.
        (void)stream->readEntireStreamAsString();

        if (!request.exportedWavPath.empty()) {
            juce::File(juce::String(request.exportedWavPath)).deleteFile();
        }

        if (statusCode < 200 || statusCode >= 300) {
            errorOut = "hub returned HTTP " + std::to_string(statusCode);
            return false;
        }
        return true;
    };
}

}  // namespace

SendToHubEditor::SendToHubEditor(SendToHubProcessor& p)
    : juce::AudioProcessorEditor(&p), processor_(p) {
    setSize(520, 180);

    selectedPathLabel_.setJustificationType(juce::Justification::centredLeft);
    addAndMakeVisible(selectedPathLabel_);

    operationBox_.addItem("Generate Sheet Music", kOpIdGenerateSheetMusic);
    operationBox_.addItem("Add Hitpoints", kOpIdHitpoints);
    operationBox_.addItem("Repomix", kOpIdRepomix);
    operationBox_.setSelectedId(kOpIdGenerateSheetMusic);
    addAndMakeVisible(operationBox_);

    hubUrlEditor_.setText(juce::String(std::string(kDefaultHubUrl)));
    hubUrlEditor_.setMultiLine(false);
    addAndMakeVisible(hubUrlEditor_);

    sendButton_.onClick = [this] { onSendClicked(); };
    refreshButton_.onClick = [this] { refreshSelectedPath(); };
    addAndMakeVisible(sendButton_);
    addAndMakeVisible(refreshButton_);

    statusLabel_.setJustificationType(juce::Justification::centredLeft);
    addAndMakeVisible(statusLabel_);

    refreshSelectedPath();
}

void SendToHubEditor::paint(juce::Graphics& g) {
    g.fillAll(juce::Colours::darkgrey.darker());
}

void SendToHubEditor::resized() {
    auto area = getLocalBounds().reduced(12);
    selectedPathLabel_.setBounds(area.removeFromTop(24));
    area.removeFromTop(8);
    auto row = area.removeFromTop(28);
    operationBox_.setBounds(row.removeFromLeft(220));
    row.removeFromLeft(8);
    refreshButton_.setBounds(row.removeFromLeft(140));
    area.removeFromTop(8);
    hubUrlEditor_.setBounds(area.removeFromTop(24));
    area.removeFromTop(8);
    auto bottom = area.removeFromTop(32);
    sendButton_.setBounds(bottom.removeFromLeft(200));
    bottom.removeFromLeft(12);
    statusLabel_.setBounds(bottom);
}

void SendToHubEditor::refreshSelectedPath() {
    const auto* src = processor_.getLastSelectedAudioSource();

    if (src == nullptr) {
        selectedPathLabel_.setText("Selected clip: (no ARA selection — click a clip in the DAW)",
                                   juce::dontSendNotification);
        sendButton_.setEnabled(false);
        return;
    }

    // We have a valid audio source - enable the button regardless of path extraction
    // since we'll extract the actual audio data via ARA.
    AudioSourceIdentity identity;
    identity.persistentID = src->getPersistentID();
    identity.name = src->getName();
    std::string path = extractAbsolutePath(identity);

    // If we couldn't extract an absolute path, use the name as identifier
    // (even relative paths or just filenames work since we send the actual audio)
    if (path.empty()) {
        path = identity.name.empty() ? identity.persistentID : identity.name;
    }

    // Final fallback - if still empty, use a generic identifier
    if (path.empty()) {
        path = "(audio source)";
    }

    selectedPathLabel_.setText("Selected clip: " + juce::String(path), juce::dontSendNotification);
    sendButton_.setEnabled(true);
}

void SendToHubEditor::showStatus(const juce::String& text, juce::Colour colour) {
    statusLabel_.setText(text, juce::dontSendNotification);
    statusLabel_.setColour(juce::Label::textColourId, colour);
}

void SendToHubEditor::onSendClicked() {
    const auto* src = processor_.getLastSelectedAudioSource();

    if (src == nullptr) {
        showStatus("no clip selected", juce::Colours::orange);
        return;
    }

    // Get whatever identifier we can (path, name, or persistentID)
    AudioSourceIdentity identity;
    identity.persistentID = src->getPersistentID();
    identity.name = src->getName();
    std::string path = extractAbsolutePath(identity);

    juce::File file{juce::String(path)};
    if (path.empty() || !file.existsAsFile()) {
        showStatus("selecting backup file...", juce::Colours::lightyellow);
        fileChooser_ = std::make_unique<juce::FileChooser>(
            "Cannot find audio file on disk. Please select the file manually:",
            juce::File::getSpecialLocation(juce::File::userHomeDirectory),
            "*.wav;*.aif;*.aiff;*.mp3;*.flac");

        auto chooserFlags =
            juce::FileBrowserComponent::openMode | juce::FileBrowserComponent::canSelectFiles;
        fileChooser_->launchAsync(chooserFlags, [this](const juce::FileChooser& fc) {
            auto result = fc.getResult();
            if (result.existsAsFile()) {
                proceedWithFile(result.getFullPathName().toStdString());
            } else {
                showStatus("cancelled file selection", juce::Colours::orange);
            }
        });
        return;
    }

    proceedWithFile(path);
}

void SendToHubEditor::proceedWithFile(std::string path) {
    if (path.empty()) {
        path = "(audio source)";
    }

    HubRequest request;
    request.absoluteFilePath = path;
    request.operation = operationFromComboId(operationBox_.getSelectedId());

    const std::string url = hubUrlEditor_.getText().toStdString();
    const auto poster = makeJuceHttpPoster();

    showStatus("copying audio...", juce::Colours::lightyellow);

    juce::Thread::launch([this, request, url, poster, path] {
        auto tempFile = juce::File::getSpecialLocation(juce::File::tempDirectory)
                            .getChildFile("pmb_ara_export.wav");
        tempFile.deleteFile();

        juce::File originalFile{juce::String(path)};
        if (!originalFile.copyFileTo(tempFile)) {
            juce::MessageManager::callAsync(
                [this] { showStatus("failed to copy audio file", juce::Colours::red); });
            return;
        }

        std::string wavPath = tempFile.getFullPathName().toStdString();

        juce::MessageManager::callAsync([this, request, url, poster, wavPath] {
            HubRequest finalRequest = request;
            finalRequest.exportedWavPath = wavPath;

            showStatus("sending...", juce::Colours::lightyellow);

            juce::Thread::launch([this, finalRequest, url, poster] {
                std::string err;
                const bool ok = sendRequest(finalRequest, url, poster, err);
                juce::MessageManager::callAsync([this, ok, err] {
                    if (ok) {
                        showStatus("enqueued ok", juce::Colours::lightgreen);
                    } else {
                        showStatus("error: " + juce::String(err), juce::Colours::orangered);
                    }
                });
            });
        });
    });
}

}  // namespace music_ara_client
