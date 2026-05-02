#include "PluginProcessor.h"
#include "PluginEditor.h"
#include "ARAFilePathExtractor.h"

#include <ARA_Library/PlugIn/ARAPlug.h>

namespace music_ara_client {

SendToHubProcessor::SendToHubProcessor()
    : AudioProcessor(BusesProperties()
                         .withInput("Input", juce::AudioChannelSet::stereo(), true)
                         .withOutput("Output", juce::AudioChannelSet::stereo(), true)) {}

juce::AudioProcessorEditor* SendToHubProcessor::createEditor() {
    return new SendToHubEditor(*this);
}

const ARA::PlugIn::AudioSource* SendToHubProcessor::getLastSelectedAudioSource() const {
    auto* docController = this->juce::AudioProcessorARAExtension::getDocumentController();
    if (docController == nullptr) {
        return nullptr;
    }

    const auto* doc = docController->getDocument();
    if (doc == nullptr) {
        return nullptr;
    }

    const ARA::PlugIn::AudioSource* chosen = nullptr;
    for (const auto* sequence : doc->getRegionSequences()) {
        if (sequence == nullptr) continue;
        for (const auto* region : sequence->getPlaybackRegions()) {
            if (region == nullptr) continue;
            if (const auto* modification = region->getAudioModification()) {
                if (auto* src = modification->getAudioSource()) {
                    chosen = src;
                }
            }
        }
    }

    if (chosen == nullptr && !doc->getAudioSources().empty()) {
        chosen = doc->getAudioSources().back();
    }

    return chosen;
}

}  // namespace music_ara_client

namespace {
class DummyDocumentController : public juce::ARADocumentControllerSpecialisation {
public:
    using juce::ARADocumentControllerSpecialisation::ARADocumentControllerSpecialisation;

protected:
    bool doRestoreObjectsFromStream(juce::ARAInputStream&, const ARA::PlugIn::RestoreObjectsFilter*) override {
        return true;
    }
    bool doStoreObjectsToStream(juce::ARAOutputStream&, const ARA::PlugIn::StoreObjectsFilter*) override {
        return true;
    }
};
}  // namespace

// JUCE entry points.
juce::AudioProcessor* JUCE_CALLTYPE createPluginFilter() {
    return new music_ara_client::SendToHubProcessor();
}

const ARA::ARAFactory* JUCE_CALLTYPE createARAFactory() {
    return juce::ARADocumentControllerSpecialisation::createARAFactory<DummyDocumentController>();
}
