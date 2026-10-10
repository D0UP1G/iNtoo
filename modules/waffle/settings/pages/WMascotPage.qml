import QtQuick
import QtQuick.Layouts
import qs.modules.waffle.settings
import qs.modules.common

WSettingsPage {
    settingsPageIndex: 14
    pageTitle: Translation.tr("Mascot")
    pageIcon: "pets"
    pageDescription: Translation.tr("Desktop companion powered by OpenCode")

    readonly property string assistantProvider: Config.options?.mascot?.assistant?.provider ?? ""
    readonly property bool customEndpoint: assistantProvider.length > 0 && assistantProvider !== "ollama"

    WSettingsCard {
        title: Translation.tr("Mascot")
        icon: "pets"

        WSettingsInfoBar {
            severity: WSettingsInfoBar.Severity.Info
            message: Translation.tr("Open the companion with Super+S. It uses local Ollama Qwen 3.5 0.8B by default. The model downloads on first use; no API key is needed.")
        }

        WSettingsDropdown {
            label: Translation.tr("Provider")
            description: Translation.tr("Connect the provider once with: inir assistant auth")
            options: [
                { value: "", displayName: Translation.tr("From model name") },
                { value: "ollama", displayName: "Ollama" },
                { value: "openai", displayName: "OpenAI" },
                { value: "anthropic", displayName: "Anthropic" },
                { value: "google", displayName: "Google" },
                { value: "groq", displayName: "Groq" },
                { value: "mistral", displayName: "Mistral" },
                { value: "opencode", displayName: "OpenCode" }
            ]
            currentValue: Config.options?.mascot?.assistant?.provider ?? ""
            onSelected: newValue => Config.setNestedValue("mascot.assistant.provider", newValue)
        }

        WSettingsTextField {
            label: Translation.tr("Assistant model")
            icon: "smart_toy"
            placeholderText: Translation.tr("provider/model; default: ollama/qwen3.5:0.8b")
            text: Config.options?.mascot?.assistant?.model ?? ""
            onEditingFinished: newText => Config.setNestedValue("mascot.assistant.model", newText.trim())
        }

        WSettingsTextField {
            visible: root.customEndpoint
            label: Translation.tr("API base URL")
            icon: "globe-search"
            description: Translation.tr("Leave empty to use the provider's own endpoint")
            placeholderText: "https://api.deepseek.com/v1"
            text: Config.options?.mascot?.assistant?.apiBase ?? ""
            onEditingFinished: newText => Config.setNestedValue("mascot.assistant.apiBase", newText.trim())
        }

        WSettingsChoiceGroup {
            visible: root.customEndpoint
            columns: 3
            currentValue: Config.options?.mascot?.assistant?.apiFormat ?? "openai"
            onSelected: newValue => Config.setNestedValue("mascot.assistant.apiFormat", newValue)
            options: [
                { label: Translation.tr("OpenAI compatible"), value: "openai" },
                { label: "Anthropic", value: "anthropic" },
                { label: "Google", value: "google" }
            ]
        }
    }
}
