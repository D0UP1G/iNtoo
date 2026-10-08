import QtQuick
import QtQuick.Layouts
import qs.modules.waffle.settings

WSettingsPage {
    settingsPageIndex: 14
    pageTitle: Translation.tr("Mascot")
    pageIcon: "pets"
    pageDescription: Translation.tr("Optional artwork")

    WSettingsCard {
        title: Translation.tr("Mascot")
        icon: "pets"

        WSettingsInfoBar {
            severity: WSettingsInfoBar.Severity.Info
            message: Translation.tr("The upstream iNiR mascot artwork is not included in iNtoo. This page is kept for compatibility with existing settings layouts.")
        }
    }
}
