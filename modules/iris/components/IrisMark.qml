import QtQuick
import Quickshell

Item {
    id: root
    property real implicitSize: 22 * IrisStyle.density
    property bool orbiting: false
    implicitWidth: implicitSize
    implicitHeight: implicitSize

    Image {
        anchors.fill: parent
        source: Quickshell.shellPath("assets/icons/intoo-mark.svg")
        fillMode: Image.PreserveAspectFit
        smooth: true
    }
}
