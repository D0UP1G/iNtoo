import QtQuick
import QtQuick.Layouts
import qs.modules.common
import qs.modules.common.widgets

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
                text: Translation.tr("The upstream iNiR mascot artwork is not included in iNtoo. This page is kept for compatibility with existing settings layouts.")
                font.pixelSize: Appearance.font.pixelSize.smaller
                color: Appearance.colors.colOnSecondaryContainer
                wrapMode: Text.Wrap
            }
        }
    }
}
