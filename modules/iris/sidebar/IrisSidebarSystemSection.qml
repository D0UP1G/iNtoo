pragma ComponentBehavior: Bound

import QtQuick
import QtQuick.Layouts
import qs.services
import qs.modules.common
import qs.modules.iris.components
import qs.modules.iris.style

IrisSidebarSectionFrame {
    id: systemCard
    expandable: false
    title: Translation.tr("System")
    glyph: "monitor_heart"
    tint: IrisStyle.identity.teal
    readonly property bool polling: systemCard.sectionHost.contentActive
    property bool holding: false
    function syncPolling(): void {
        if (systemCard.polling && !systemCard.holding) { ResourceUsage.keepAlive(); systemCard.holding = true }
        else if (!systemCard.polling && systemCard.holding) { ResourceUsage.releaseKeepAlive(); systemCard.holding = false }
    }
    onPollingChanged: systemCard.syncPolling()
    Component.onCompleted: systemCard.syncPolling()
    Component.onDestruction: if (systemCard.holding) ResourceUsage.releaseKeepAlive()
    RowLayout {
        Layout.fillWidth: true
        spacing: 4 * systemCard.sectionHost.d
        Repeater {
            model: [
                { label: Translation.tr("CPU"), level: ResourceUsage.cpuUsage, value: Math.round(ResourceUsage.cpuUsage * 100) + "%", warn: 0.85 },
                { label: Translation.tr("Memory"), level: ResourceUsage.memoryUsedPercentage, value: Math.round(ResourceUsage.memoryUsedPercentage * 100) + "%", warn: 0.85 },
                { label: Translation.tr("Heat"), level: ResourceUsage.tempPercentage, value: ResourceUsage.maxTemp + "°", warn: ResourceUsage.tempWarningThreshold / 100 },
                { label: Translation.tr("Disk"), level: ResourceUsage.diskUsedPercentage, value: Math.round(ResourceUsage.diskUsedPercentage * 100) + "%", warn: 0.9 }
            ]
            ColumnLayout {
                id: gauge
                required property var modelData
                readonly property real level: Math.max(0, Math.min(1, Number(gauge.modelData.level) || 0))
                readonly property color tint: gauge.level >= gauge.modelData.warn ? IrisStyle.danger : IrisStyle.accent
                // Four equal columns, each ring centred in its own: a layout with no filling child stops at its implicit width.
                Layout.fillWidth: true
                Layout.preferredWidth: 1
                Layout.maximumWidth: Number.POSITIVE_INFINITY
                spacing: 4 * systemCard.sectionHost.d
                Item {
                    Layout.alignment: Qt.AlignHCenter
                    implicitWidth: Math.round(52 * systemCard.sectionHost.d)
                    implicitHeight: implicitWidth
                    IrisSidebarGaugeRing {
                        density: systemCard.sectionHost.d
                        anchors.fill: parent
                        tint: gauge.tint
                        progress: gauge.level
                    }
                    IrisText {
                        anchors.centerIn: parent
                        text: gauge.modelData.value
                        font.family: IrisStyle.fontNumbers
                        font.pixelSize: IrisStyle.typeLabel
                        font.weight: IrisStyle.weight(Font.Bold)
                    }
                }
                IrisText { Layout.alignment: Qt.AlignHCenter; text: gauge.modelData.label; color: IrisStyle.muted; font.pixelSize: IrisStyle.typeFootnote }
            }
        }
    }
}
