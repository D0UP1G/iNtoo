pragma ComponentBehavior: Bound

import QtQuick
import QtQuick.Layouts
import qs.modules.common.widgets
import qs.modules.iris.components
import qs.modules.iris.style

Rectangle {
    id: section
    required property var sectionHost
    property string title
    property string detail: ""
    property string glyph: ""
    property color tint: IrisStyle.accent
    property color ink: IrisStyle.text
    property bool expandable: true
    readonly property bool open: !section.expandable || section.sectionHost.expanded || section.sectionHost.bare
    default property alias content: body.data
    property alias summary: summaryColumn.data
    property alias actions: actionRow.data
    readonly property real pad: Math.round(14 * section.sectionHost.d)
    readonly property real gap: Math.round(10 * section.sectionHost.d)
    readonly property real innerHeight: section.open ? body.implicitHeight : summaryColumn.implicitHeight
    property real shownHeight: header.height + section.pad * 2 + (section.innerHeight > 0 ? section.gap + section.innerHeight : 0)
    property bool animate: false
    Component.onCompleted: Qt.callLater(() => section.animate = true)
    Behavior on shownHeight {
        enabled: section.animate
        NumberAnimation { id: morph; duration: IrisStyle.morphDuration; easing.type: Easing.BezierSpline; easing.bezierCurve: IrisStyle.morphCurve }
    }
    Binding { target: section.sectionHost; property: "morphing"; value: morph.running }
    implicitHeight: section.shownHeight
    radius: section.sectionHost.bare ? 0 : IrisStyle.radiusCard
    color: section.sectionHost.bare ? "transparent" : IrisStyle.surfaceHigh
    clip: true

    MouseArea {
        id: header
        x: section.pad
        y: section.pad
        width: section.width - section.pad * 2
        height: Math.round(26 * section.sectionHost.d)
        enabled: section.expandable && !section.sectionHost.bare
        cursorShape: section.expandable && !section.sectionHost.bare ? Qt.PointingHandCursor : Qt.ArrowCursor
        Accessible.role: Accessible.Button
        Accessible.name: section.title
        onClicked: section.sectionHost.toggleRequested()
        RowLayout {
            anchors.fill: parent
            spacing: 8 * section.sectionHost.d
            Rectangle {
                visible: section.glyph.length > 0
                implicitWidth: Math.round(24 * section.sectionHost.d)
                implicitHeight: implicitWidth
                radius: IrisStyle.iconRadius(width)
                gradient: Gradient {
                    GradientStop { position: 0; color: Qt.lighter(section.tint, 1.2) }
                    GradientStop { position: 1; color: section.tint }
                }
                MaterialSymbol { anchors.centerIn: parent; text: section.glyph; fill: 1; iconSize: Math.round(15 * section.sectionHost.d); color: IrisStyle.onTint }
            }
            IrisText {
                Layout.fillWidth: true
                Layout.minimumWidth: 0
                text: section.title
                color: section.ink
                font.weight: IrisStyle.weight(Font.DemiBold)
                font.pixelSize: IrisStyle.typeBody
                elide: Text.ElideRight
            }
            IrisText {
                visible: text.length > 0
                text: section.detail
                color: IrisStyle.secondaryOf(section.ink)
                font.family: IrisStyle.fontNumbers
                font.pixelSize: IrisStyle.typeMeta
            }
            RowLayout {
                id: actionRow
                spacing: 0
                visible: section.open
            }
            MaterialSymbol {
                visible: section.expandable && !section.sectionHost.bare
                text: "keyboard_arrow_down"
                iconSize: Math.round(18 * section.sectionHost.d)
                color: IrisStyle.secondaryOf(section.ink)
                rotation: section.open ? 180 : 0
                Behavior on rotation { NumberAnimation { duration: IrisStyle.morphDuration; easing.type: Easing.BezierSpline; easing.bezierCurve: IrisStyle.morphCurve } }
            }
        }
    }
    ColumnLayout {
        id: summaryColumn
        x: section.pad
        y: header.y + header.height + section.gap
        width: section.width - section.pad * 2
        spacing: 6 * section.sectionHost.d
        opacity: section.open ? 0 : 1
        visible: opacity > 0
        Behavior on opacity { NumberAnimation { duration: IrisStyle.duration(section.open ? 90 : 180); easing.type: IrisStyle.feedbackEasing } }
    }
    ColumnLayout {
        id: body
        x: section.pad
        y: header.y + header.height + section.gap
        width: section.width - section.pad * 2
        spacing: 10 * section.sectionHost.d
        opacity: section.open ? 1 : 0
        visible: opacity > 0
        enabled: section.open
        Behavior on opacity { NumberAnimation { duration: IrisStyle.duration(section.open ? 200 : 90); easing.type: IrisStyle.feedbackEasing } }
    }
}
