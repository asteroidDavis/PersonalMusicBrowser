// PluginEditor — the Cubase "Lower Zone" UI described in Phase 4.
//
// Layout:
//   +----------------------------------------------------+
//   | Selected clip: /abs/path/to/clip.wav               |
//   |                                                    |
//   | Operation: [Generate Sheet Music v]                |
//   |                                                    |
//   | [ Send to Music Browser ]    status: Queued OK     |
//   +----------------------------------------------------+
//
// The "Send" click is non-blocking: we spawn a short-lived
// `juce::Thread::launch` task so the DAW's message thread never waits on the
// HTTP socket.  The user returns to the DAW immediately.

#pragma once

#include <juce_audio_processors/juce_audio_processors.h>
#include <juce_gui_extra/juce_gui_extra.h>

#include "HubClient.h"

namespace music_ara_client {

class SendToHubProcessor;

class SendToHubEditor final : public juce::AudioProcessorEditor {
public:
    explicit SendToHubEditor(SendToHubProcessor& processor);
    ~SendToHubEditor() override = default;

    void resized() override;
    void paint(juce::Graphics& g) override;

private:
    void refreshSelectedPath();
    void onSendClicked();
    void showStatus(const juce::String& text, juce::Colour colour);

    SendToHubProcessor& processor_;

    juce::Label       selectedPathLabel_  { {}, "Selected clip: (none)" };
    juce::ComboBox    operationBox_;
    juce::TextButton  sendButton_         { "Send to Music Browser" };
    juce::TextButton  refreshButton_      { "Refresh selection" };
    juce::Label       statusLabel_        { {}, "" };
    juce::TextEditor  hubUrlEditor_;

    JUCE_DECLARE_NON_COPYABLE_WITH_LEAK_DETECTOR(SendToHubEditor)
};

}  // namespace music_ara_client
