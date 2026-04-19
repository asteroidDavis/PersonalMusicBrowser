#include "PluginProcessor.h"
#include "PluginEditor.h"
#include "ARAFilePathExtractor.h"

#include <ARA_Library/PlugIn/ARAPlug.h>

namespace music_ara_client {

SendToHubProcessor::SendToHubProcessor()
    : AudioProcessor(BusesProperties().withInput("Input", juce::AudioChannelSet::stereo(), true)
                                      .withOutput("Output", juce::AudioChannelSet::stereo(), true)) {}

juce::AudioProcessorEditor* SendToHubProcessor::createEditor() {
    return new SendToHubEditor(*this);
}

std::string SendToHubProcessor::getLastSelectedAudioFilePath() const {
    // Access the attached ARA document via the AudioProcessorARAExtension
    // base class.  If no ARA host is present (standalone / DAW without ARA)
    // this returns nullptr and we fall through to an empty result.
    auto* docController = getARADocumentController();
    if (docController == nullptr) {
        return {};
    }

    const auto* doc = docController->getDocument();
    if (doc == nullptr) {
        return {};
    }

    // Strategy: prefer the audio source of the last ARA playback region the
    // host has created (this is the one the user most-recently selected in
    // the DAW timeline for Cubase/REAPER).  Fall back to the last audio
    // source in the document.
    const ARA::PlugIn::AudioSource* chosen = nullptr;
    for (const auto* region : doc->getPlaybackRegions()) {
        if (region == nullptr) continue;
        if (const auto* modification = region->getAudioModification()) {
            if (auto* src = modification->getAudioSource()) {
                chosen = src;  // keep overwriting — end up with the last one.
            }
        }
    }
    if (chosen == nullptr) {
        const auto& sources = doc->getAudioSources();
        if (!sources.empty()) {
            chosen = sources.back();
        }
    }
    if (chosen == nullptr) {
        return {};
    }

    AudioSourceIdentity identity;
    identity.persistentID = chosen->getPersistentID() != nullptr ? chosen->getPersistentID() : "";
    identity.name         = chosen->getName()         != nullptr ? chosen->getName()         : "";
    return extractAbsolutePath(identity);
}

}  // namespace music_ara_client

// JUCE entry point.
juce::AudioProcessor* JUCE_CALLTYPE createPluginFilter() {
    return new music_ara_client::SendToHubProcessor();
}
