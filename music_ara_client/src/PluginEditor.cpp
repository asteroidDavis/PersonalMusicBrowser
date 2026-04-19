#include "PluginEditor.h"
#include "PluginProcessor.h"

namespace music_ara_client {

namespace {

constexpr int kOpIdGenerateSheetMusic = 1;
constexpr int kOpIdHitpoints          = 2;
constexpr int kOpIdRepomix            = 3;

Operation operationFromComboId(int id) {
    switch (id) {
        case kOpIdHitpoints: return Operation::Hitpoints;
        case kOpIdRepomix:   return Operation::Repomix;
        default:             return Operation::GenerateSheetMusic;
    }
}

// Thin adapter: `juce::URL` → `music_ara_client::HttpPoster`.  Kept out of
// `sendtohub_core` so the unit tests stay JUCE-free.
HttpPoster makeJuceHttpPoster() {
    return [](std::string_view url, std::string_view body, std::string& errorOut) -> bool {
        const juce::URL u(juce::String(std::string(url)));
        auto options = juce::URL::InputStreamOptions(juce::URL::ParameterHandling::inPostData)
                           .withExtraHeaders("Content-Type: application/json")
                           .withConnectionTimeoutMs(5000);
        auto postUrl = u.withPOSTData(juce::String(std::string(body)));
        int statusCode = 0;
        auto stream = postUrl.createInputStream(options.withStatusCode(&statusCode));
        if (stream == nullptr) {
            errorOut = "failed to open POST stream to " + std::string(url);
            return false;
        }
        // Drain — the hub returns JSON we don't need in the UI.
        (void)stream->readEntireStreamAsString();
        if (statusCode < 200 || statusCode >= 300) {
            errorOut = "hub returned HTTP " + std::to_string(statusCode);
            return false;
        }
        return true;
    };
}

}  // namespace

SendToHubEditor::SendToHubEditor(SendToHubProcessor& processor)
    : juce::AudioProcessorEditor(&processor), processor_(processor) {
    setSize(520, 180);

    selectedPathLabel_.setJustificationType(juce::Justification::centredLeft);
    addAndMakeVisible(selectedPathLabel_);

    operationBox_.addItem("Generate Sheet Music", kOpIdGenerateSheetMusic);
    operationBox_.addItem("Add Hitpoints",        kOpIdHitpoints);
    operationBox_.addItem("Repomix",              kOpIdRepomix);
    operationBox_.setSelectedId(kOpIdGenerateSheetMusic);
    addAndMakeVisible(operationBox_);

    hubUrlEditor_.setText(juce::String(std::string(kDefaultHubUrl)));
    hubUrlEditor_.setMultiLine(false);
    addAndMakeVisible(hubUrlEditor_);

    sendButton_.onClick    = [this] { onSendClicked(); };
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
    const auto path = processor_.getLastSelectedAudioFilePath();
    if (path.empty()) {
        selectedPathLabel_.setText("Selected clip: (no ARA selection — click a clip in the DAW)",
                                   juce::dontSendNotification);
        sendButton_.setEnabled(false);
    } else {
        selectedPathLabel_.setText("Selected clip: " + juce::String(path),
                                   juce::dontSendNotification);
        sendButton_.setEnabled(true);
    }
}

void SendToHubEditor::showStatus(const juce::String& text, juce::Colour colour) {
    statusLabel_.setText(text, juce::dontSendNotification);
    statusLabel_.setColour(juce::Label::textColourId, colour);
}

void SendToHubEditor::onSendClicked() {
    const auto path = processor_.getLastSelectedAudioFilePath();
    if (path.empty()) {
        showStatus("no clip selected", juce::Colours::orange);
        return;
    }

    HubRequest request;
    request.absoluteFilePath = path;
    request.operation        = operationFromComboId(operationBox_.getSelectedId());

    const std::string url    = hubUrlEditor_.getText().toStdString();
    const auto        poster = makeJuceHttpPoster();

    showStatus("sending...", juce::Colours::lightyellow);

    // Non-blocking: off-thread POST, marshal result back to message thread.
    juce::Thread::launch([this, request, url, poster] {
        std::string err;
        const bool  ok = sendRequest(request, url, poster, err);
        juce::MessageManager::callAsync([this, ok, err] {
            if (ok) {
                showStatus("enqueued ok", juce::Colours::lightgreen);
            } else {
                showStatus("error: " + juce::String(err), juce::Colours::orangered);
            }
        });
    });
}

}  // namespace music_ara_client
