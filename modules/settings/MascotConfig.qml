pragma ComponentBehavior: Bound

import QtQuick
import QtQuick.Layouts
import qs.modules.common
import qs.modules.common.widgets
import qs.services

ContentPage {
    id: root
    settingsPageIndex: 19
    settingsPageName: Translation.tr("Mascot")

    readonly property string assistantProvider: Config.options?.mascot?.assistant?.provider ?? ""
    readonly property bool customEndpoint: assistantProvider.length > 0 && assistantProvider !== "ollama"

    SettingsCardSection {
        expanded: true
        icon: "pets"
        title: Translation.tr("Mascot")

        SettingsGroup {
            StyledText {
                Layout.fillWidth: true
                text: Translation.tr("Open the companion with Super+S. It uses original in-project graphics. The upstream iNiR mascot artwork is not included in iNtoo. Chat through OpenCode, attach saved command history, or turn on system access to approve each action before it runs.")
                font.pixelSize: Appearance.font.pixelSize.smaller
                color: Appearance.colors.colOnSecondaryContainer
                wrapMode: Text.Wrap
            }

            ContentSubsection {
                title: Translation.tr("Provider")
                tooltip: Translation.tr("Which provider answers. Connect its credentials once with: intoo assistant auth")

                ConfigSelectionArray {
                    enableSettingsSearch: false
                    currentValue: Config.options?.mascot?.assistant?.provider ?? ""
                    onSelected: newValue => MascotAssistant.selectProvider(newValue)
                    options: [
                        { displayName: Translation.tr("From model name"), icon: "auto_awesome", value: "" },
                        { displayName: "Ollama", icon: "ollama-symbolic", value: "ollama" },
                        { displayName: "OpenAI", icon: "openai-symbolic", value: "openai" },
                        { displayName: "Anthropic", icon: "anthropic-symbolic", value: "anthropic" },
                        { displayName: "Google", icon: "google-gemini-symbolic", value: "google" },
                        { displayName: "GitHub Copilot", icon: "code", value: "github-copilot" },
                        { displayName: "OpenRouter", icon: "spark-symbolic", value: "openrouter" },
                        { displayName: "Groq", icon: "spark-symbolic", value: "groq" },
                        { displayName: "Mistral", icon: "mistral-symbolic", value: "mistral" },
                        { displayName: "OpenCode", icon: "code", value: "opencode" }
                    ]
                }
            }

            ContentSubsection {
                title: Translation.tr("Model")
                tooltip: Translation.tr("provider/model, for example ollama/qwen3.5:0.8b or gpt-4.1-mini")

                MaterialTextField {
                    Layout.fillWidth: true
                    placeholderText: root.assistantProvider && root.assistantProvider !== "ollama"
                        ? root.assistantProvider + "/model"
                        : Translation.tr("Assistant model; default: ollama/qwen3.5:0.8b")
                    text: Config.options?.mascot?.assistant?.model ?? ""
                    onEditingFinished: Config.setNestedValue("mascot.assistant.model", text.trim())
                }
            }

            ContentSubsection {
                visible: root.customEndpoint
                title: Translation.tr("API endpoint")
                tooltip: Translation.tr("Optional OpenAI-compatible base URL. Leave empty to use the provider's own endpoint.")

                MaterialTextField {
                    Layout.fillWidth: true
                    placeholderText: "https://api.deepseek.com/v1"
                    text: Config.options?.mascot?.assistant?.apiBase ?? ""
                    onEditingFinished: Config.setNestedValue("mascot.assistant.apiBase", text.trim())
                }

                ConfigSelectionArray {
                    enableSettingsSearch: false
                    currentValue: Config.options?.mascot?.assistant?.apiFormat ?? "openai"
                    onSelected: newValue => Config.setNestedValue("mascot.assistant.apiFormat", newValue)
                    options: [
                        { displayName: Translation.tr("OpenAI compatible"), icon: "smart_toy", value: "openai" },
                        { displayName: "Anthropic", icon: "psychology", value: "anthropic" },
                        { displayName: "Google", icon: "auto_awesome", value: "google" }
                    ]
                }
            }

            StyledText {
                Layout.fillWidth: true
                text: root.assistantProvider === "ollama" || root.assistantProvider.length === 0
                    ? Translation.tr("Default: local Ollama Qwen 3.5 0.8B. The model downloads on first use; no API key is needed.")
                    : Translation.tr("Credentials come from OpenCode's own store. Run `intoo assistant auth` once per provider.")
                color: Appearance.colors.colSubtext
                wrapMode: Text.WordWrap
            }

            ContentSubsection {
                title: Translation.tr("System access")
                tooltip: Translation.tr("Whether the companion can act on the system, and how it asks for approval.")

                ConfigSwitch {
                    text: Translation.tr("Approve each action")
                    description: Translation.tr("Let OpenCode run commands and edit files, showing every action for your approval before it runs.")
                    checked: Config.options?.mascot?.assistant?.systemAccess ?? false
                    onCheckedChanged: Config.setNestedValue("mascot.assistant.systemAccess", checked)
                }
            }
        }
    }
    Component.onCompleted: MascotAssistant.refreshModels()
}
