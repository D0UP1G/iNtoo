import QtQuick
import QtQuick.Effects
import Quickshell
import qs.modules.iris.style

Item {
    id: root
    property real implicitSize: 22 * IrisStyle.density
    property bool orbiting: false
    property color color: "transparent"
    implicitWidth: implicitSize
    implicitHeight: implicitSize

    Image {
        id: mark
        anchors.fill: parent
        source: Quickshell.shellPath("assets/icons/intoo-mark.svg")
        fillMode: Image.PreserveAspectFit
        smooth: true
        visible: root.color.a === 0
    }

    MultiEffect {
        anchors.fill: parent
        source: mark
        visible: root.color.a > 0
        colorization: 1
        colorizationColor: root.color
    }
}
