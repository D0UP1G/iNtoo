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
            message: Translation.tr("Open the companion with Super+S. It uses original in-project graphics. The upstream iNiR mascot artwork is not included in iNtoo. Connect a provider in your terminal: opencode auth login")
        }
        WSettingsTextField {
            label: Translation.tr("OpenCode model")
            icon: "smart_toy"
            placeholderText: Translation.tr("provider/model; empty uses OpenCode's default")
            text: Config.options?.mascot?.assistant?.model ?? ""
            onEditingFinished: newText => Config.setNestedValue("mascot.assistant.model", newText.trim())
        }
    }
}
