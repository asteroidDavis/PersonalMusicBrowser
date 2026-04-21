// PluginProcessor — ARA-effect plugin processor.
//
// This class exists mostly to satisfy JUCE's `AudioProcessor` contract.  All
// the interesting behavior lives in `PluginEditor` (which owns the UI and
// the hub POST) and the `sendtohub_core` library.
//
// The processor does however own the document controller, from which the
// editor retrieves the currently-selected audio source when the user clicks
// "Send to Music Browser".  See `getLastSelectedAudioFilePath`.

#pragma once

#include <juce_audio_processors/juce_audio_processors.h>
#include <juce_audio_utils/juce_audio_utils.h>
#include <ARA_Library/PlugIn/ARAPlug.h>

#include <atomic>
#include <mutex>
#include <string>

namespace music_ara_client {

class SendToHubProcessor final : public juce::AudioProcessor,
                                 public juce::AudioProcessorARAExtension {
public:
    SendToHubProcessor();
    ~SendToHubProcessor() override = default;

    // ---- juce::AudioProcessor ----
    const juce::String getName() const override { return "Send To Music Browser"; }
    void prepareToPlay(double, int) override {}
    void releaseResources() override {}
    void processBlock(juce::AudioBuffer<float>& buffer, juce::MidiBuffer&) override {
        buffer.clear();
    }
    juce::AudioProcessorEditor* createEditor() override;
    bool hasEditor() const override { return true; }
    bool acceptsMidi() const override { return false; }
    bool producesMidi() const override { return false; }
    double getTailLengthSeconds() const override { return 0.0; }
    int getNumPrograms() override { return 1; }
    int getCurrentProgram() override { return 0; }
    void setCurrentProgram(int) override {}
    const juce::String getProgramName(int) override { return {}; }
    void changeProgramName(int, const juce::String&) override {}
    void getStateInformation(juce::MemoryBlock&) override {}
    void setStateInformation(const void*, int) override {}

    /// Walks the current ARA document and returns the most-recently-touched
    /// region's audio source, or nullptr if no ARA host is attached or no
    /// selection is available.
    const ARA::PlugIn::AudioSource* getLastSelectedAudioSource() const;

private:
    JUCE_DECLARE_NON_COPYABLE_WITH_LEAK_DETECTOR(SendToHubProcessor)
};

}  // namespace music_ara_client
