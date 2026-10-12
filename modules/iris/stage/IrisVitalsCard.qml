pragma ComponentBehavior: Bound

import QtQuick
import QtQuick.Layouts
import qs.services
import qs.modules.common.widgets
import qs.modules.iris.style
import qs.modules.iris.components
import qs.modules.iris.bar.island

ColumnLayout {
    id: vitalsBody

    property bool contentActive: true
    readonly property real d: IrisStyle.density
    spacing: 10 * vitalsBody.d
    ResourceUsageMonitor { target: vitalsBody; active: vitalsBody.contentActive }
    CardHeader {
        glyph: "monitoring"
        tint: IrisStyle.identity.teal
        title: Translation.tr("Vitals")
        detail: Translation.tr("What this machine is doing")
    }
    Repeater {
        model: [
            { label: Translation.tr("Processor"), glyph: "memory", level: ResourceUsage.cpuUsage, value: Math.round(ResourceUsage.cpuUsage * 100), unit: "%", warn: 0.85 },
            { label: Translation.tr("Memory"), glyph: "memory_alt", level: ResourceUsage.memoryUsedPercentage, value: Math.round(ResourceUsage.memoryUsedPercentage * 100), unit: "%", warn: 0.85 },
            { label: Translation.tr("Heat"), glyph: "device_thermostat", level: ResourceUsage.tempPercentage, value: ResourceUsage.maxTemp, unit: "°", warn: ResourceUsage.tempWarningThreshold / 100 },
            { label: Translation.tr("Disk"), glyph: "hard_drive", level: ResourceUsage.diskUsedPercentage, value: Math.round(ResourceUsage.diskUsedPercentage * 100), unit: "%", warn: 0.9 }
        ]
        RowLayout {
            id: vital
            required property var modelData
            readonly property real level: Math.max(0, Math.min(1, Number(vital.modelData.level) || 0))
            readonly property color tint: vital.level >= vital.modelData.warn ? IrisStyle.danger : IrisStyle.accent
            Layout.fillWidth: true
            spacing: 10 * vitalsBody.d
            Accessible.name: vital.modelData.label + ", " + vital.modelData.value + vital.modelData.unit
            Item {
                Layout.preferredWidth: Math.round(30 * vitalsBody.d)
                Layout.preferredHeight: Layout.preferredWidth
                ProgressRing {
                    anchors.fill: parent
                    stroke: Math.max(2, 2.4 * vitalsBody.d)
                    tint: vital.tint
                    progress: vital.level
                    Behavior on progress { NumberAnimation { duration: IrisStyle.morphDuration; easing.type: Easing.BezierSpline; easing.bezierCurve: IrisStyle.moveCurve } }
                }
                MaterialSymbol {
                    anchors.centerIn: parent
                    text: vital.modelData.glyph
                    fill: 1
                    iconSize: Math.round(13 * vitalsBody.d)
                    color: vital.tint
                }
            }
            IrisText {
                Layout.fillWidth: true
                text: vital.modelData.label
                elide: Text.ElideRight
            }
            Metric {
                value: vital.modelData.value
                unit: vital.modelData.unit
                pixelSize: IrisStyle.typeHeadline
                weight: Font.Bold
                color: vital.tint
            }
        }
    }
}
