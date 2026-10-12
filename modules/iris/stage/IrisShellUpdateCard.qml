pragma ComponentBehavior: Bound

import QtQuick
import QtQuick.Layouts
import qs.services
import qs.modules.common.widgets
import qs.modules.iris.style
import qs.modules.iris.components

ColumnLayout {
    id: root

    required property var closeRequestedCallback
    readonly property real d: IrisStyle.density

    spacing: 10 * root.d

    CardHeader {
        glyph: "rocket_launch"
        tint: IrisStyle.secondaryAccent
        title: ShellUpdates.remoteVersion.length > 0
            ? Translation.tr("iNtoo %1").arg(ShellUpdates.remoteVersion) : Translation.tr("New iNtoo")
        detail: ShellUpdates.repoDiverged
            ? Translation.tr("Upstream history changed; your local work is kept")
            : ShellUpdates.commitsBehind > 0
                ? Translation.tr("%1 commits behind").arg(ShellUpdates.commitsBehind)
                : Translation.tr("Ready to update")
    }
    IrisText {
        Layout.fillWidth: true
        visible: text.length > 0
        text: ShellUpdates.latestMessage
        color: IrisStyle.subtext
        wrapMode: Text.WordWrap
        maximumLineCount: 3
        elide: Text.ElideRight
        font.pixelSize: IrisStyle.typeMeta
    }
    IrisButton {
        Layout.fillWidth: true
        enabled: !ShellUpdates.isUpdating
        text: ShellUpdates.isUpdating
            ? Translation.tr("Updating…") : Translation.tr("Update now")
        buttonRadius: IrisStyle.radiusTile
        implicitHeight: Math.round(38 * root.d)
        onClicked: { root.closeRequestedCallback(); ShellUpdates.performUpdate() }
    }
    RowLayout {
        Layout.fillWidth: true
        spacing: 8 * root.d
        IrisButton {
            Layout.fillWidth: true
            quiet: true
            text: Translation.tr("What changed")
            buttonRadius: IrisStyle.radiusTile
            implicitHeight: Math.round(32 * root.d)
            onClicked: { root.closeRequestedCallback(); ShellUpdates.openOverlay() }
        }
        IrisButton {
            Layout.fillWidth: true
            quiet: true
            text: Translation.tr("Not now")
            buttonRadius: IrisStyle.radiusTile
            implicitHeight: Math.round(32 * root.d)
            onClicked: { root.closeRequestedCallback(); ShellUpdates.dismiss() }
        }
    }
}
