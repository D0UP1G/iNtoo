pragma ComponentBehavior: Bound

import QtQuick
import qs.services
import qs.modules.iris.components
import qs.modules.iris.style

Item {
    id: mediaHost
    required property var sectionHost
    property bool animate: false
    Component.onCompleted: Qt.callLater(() => mediaHost.animate = true)
    implicitHeight: mediaCard.implicitHeight
    Behavior on implicitHeight {
        enabled: mediaHost.animate
        NumberAnimation { id: mediaMorph; duration: IrisStyle.morphDuration; easing.type: Easing.BezierSpline; easing.bezierCurve: IrisStyle.morphCurve }
    }
    Binding { target: mediaHost.sectionHost; property: "morphing"; value: mediaMorph.running }
    clip: true
    IrisMediaCard {
        id: mediaCard
        width: parent.width
        active: mediaHost.sectionHost.contentActive
        showBackground: true
        compact: !mediaHost.sectionHost.expanded
    }
    MouseArea {
        visible: !mediaHost.sectionHost.expanded
        anchors.left: parent.left
        anchors.top: parent.top
        anchors.bottom: parent.bottom
        width: parent.width * 0.55
        cursorShape: Qt.PointingHandCursor
        Accessible.role: Accessible.Button
        Accessible.name: Translation.tr("Full player")
        onClicked: mediaHost.sectionHost.toggleRequested()
    }
    IrisIconButton {
        visible: mediaHost.sectionHost.expanded
        anchors.top: parent.top
        anchors.right: parent.right
        anchors.margins: 8 * mediaHost.sectionHost.d
        materialIcon: "unfold_less"
        iconSize: Math.round(16 * mediaHost.sectionHost.d)
        Accessible.name: mediaHost.sectionHost.expanded ? Translation.tr("Compact player") : Translation.tr("Full player")
        onClicked: mediaHost.sectionHost.toggleRequested()
    }
}
