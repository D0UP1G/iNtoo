import QtQuick
import QtQuick.Layouts
import qs.modules.common
import qs.modules.common.widgets
import qs.services

ContentPage {
    settingsPageIndex: 19
    settingsPageName: Translation.tr("Mascot")

    SettingsCardSection {
        expanded: true
        icon: "pets"
        title: Translation.tr("Mascot")

        SettingsGroup {
            StyledText {
                Layout.fillWidth: true
                text: Translation.tr("Open the companion with Super+S. It uses original in-project graphics. The upstream iNiR mascot artwork is not included in iNtoo. Chat through OpenCode, attach saved command history, or allow actions for one message.")
                font.pixelSize: Appearance.font.pixelSize.smaller
                color: Appearance.colors.colOnSecondaryContainer
                wrapMode: Text.Wrap
            }
            MaterialTextField {
                Layout.fillWidth: true
                placeholderText: Translation.tr("OpenCode model (provider/model); empty uses OpenCode's default")
                text: Config.options?.mascot?.assistant?.model ?? ""
                onEditingFinished: Config.setNestedValue("mascot.assistant.model", text.trim())
            }
            StyledText {
                Layout.fillWidth: true
                text: Translation.tr("Connect a provider in your terminal: opencode auth login")
                color: Appearance.colors.colSubtext
                wrapMode: Text.WordWrap
            }
        }
    }
}
