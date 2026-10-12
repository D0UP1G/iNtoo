pragma ComponentBehavior: Bound

import QtQuick
import QtQuick.Layouts
import Quickshell.Services.SystemTray
import qs.services
import qs.modules.common
import qs.modules.iris.style
import qs.modules.iris.bar

ColumnLayout {
    id: root

    readonly property real d: IrisStyle.density
    readonly property var items: SystemTray.items.values.filter(item => item && item.id
        && (!(Config.options?.iris?.tray?.hidePassive ?? false) || item.status !== Status.Passive))

    spacing: 12 * root.d

    CardHeader {
        glyph: "apps"
        tint: IrisStyle.identity.teal
        title: Translation.tr("Tray")
        detail: root.items.length > 0
            ? Translation.tr("%1 background apps").arg(root.items.length)
            : Translation.tr("Background apps")
    }
    IrisTray { Layout.fillWidth: true; showHeader: false; items: root.items }
}
