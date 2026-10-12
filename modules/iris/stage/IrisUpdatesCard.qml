pragma ComponentBehavior: Bound

import QtQuick
import QtQuick.Layouts
import qs.services
import qs.services.deferred
import qs.modules.common
import qs.modules.common.widgets
import qs.modules.iris.style
import qs.modules.iris.components

ColumnLayout {
    id: root
    required property var closeRequestedCallback
    readonly property real d: IrisStyle.density
    spacing: 10 * root.d
    CardHeader {
        glyph: Updates.count > 0 ? "deployed_code_update" : "task_alt"
        tint: Updates.count > 0 ? IrisStyle.secondaryAccent : IrisStyle.identity.green
        title: Translation.tr("Updates")
        detail: Updates.count > 0
            ? Translation.tr("%1 packages waiting").arg(Updates.count)
            : Translation.tr("Everything is up to date")
        IrisIconButton {
            materialIcon: "refresh"
            Accessible.name: Translation.tr("Check again")
            onClicked: Updates.refresh()
        }
    }
    IrisNumber {
        Layout.alignment: Qt.AlignHCenter
        text: Updates.count
        pixelSize: 44 * IrisStyle.typeScale
        weight: Font.Bold
        color: Updates.count > 0 ? IrisStyle.secondaryAccent : IrisStyle.identity.green
    }
    IrisButton {
        Layout.fillWidth: true
        visible: Updates.count > 0
        text: Translation.tr("Update now")
        buttonRadius: IrisStyle.radiusTile
        implicitHeight: Math.round(38 * root.d)
        onClicked: {
            root.closeRequestedCallback()
            const cmd = Config.options?.apps?.update ?? "kitty -e sudo pacman -Syu"
            if (PackageSearch.shouldUseNativeUpdateCommand(cmd))
                PackageSearch.updateSystem()
            else
                ShellExec.execCmd(cmd)
        }
    }
}
