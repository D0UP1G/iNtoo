import QtQuick
import QtQuick.Layouts
import qs.modules.waffle.settings
import qs.modules.common

WSettingsPage {
    settingsPageIndex: 14
    pageTitle: Translation.tr("Mascot")
    pageIcon: "pets"
    pageDescription: Translation.tr("Desktop companion powered by OpenCode")

    WSettingsCard {
        title: Translation.tr("Mascot")
        icon: "pets"

        WSettingsInfoBar {
            severity: WSettingsInfoBar.Severity.Info
            message: Translation.tr("Open the companion with Super+S. It uses local Ollama Qwen 3.5 0.8B by default. The model downloads on first use; no API key is needed.")
        }
        WSettingsTextField {
            label: Translation.tr("Assistant model")
            icon: "smart_toy"
            placeholderText: Translation.tr("provider/model; default: ollama/qwen3.5:0.8b")
            text: Config.options?.mascot?.assistant?.model ?? ""
            onEditingFinished: newText => Config.setNestedValue("mascot.assistant.model", newText.trim())
        }
    }
}
