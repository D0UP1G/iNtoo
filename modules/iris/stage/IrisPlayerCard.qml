pragma ComponentBehavior: Bound

import QtQuick
import Quickshell
import qs.services
import qs.modules.common
import qs.modules.common.widgets
import qs.modules.iris.style
import qs.modules.iris.components

Item {
    id: player

    property bool contentActive: true
    readonly property real d: IrisStyle.density
    ColorQuantizer {
        id: tintQuantizer
        source: MediaArtwork.displaySource
        depth: 2
        rescaleSize: 48
    }
    readonly property color tint: IrisStyle.artTintOf(tintQuantizer.colors)
    // The artwork is the card's material, edge to edge in its own contour; the player sits on it at the card's margin.
    readonly property real inset: Math.max(0, IrisStyle.cardPad - Math.round(14 * player.d))
    implicitHeight: card.implicitHeight + player.inset * 2
    Rectangle {
        anchors.fill: parent
        color: IrisStyle.surfaceHigh
        visible: !artLoader.active
    }
    Loader {
        id: artLoader
        anchors.fill: parent
        active: (Config.options?.iris?.player?.artworkBackground ?? true)
            && MediaArtwork.displaySource.length > 0
        sourceComponent: Item {
            IrisMediaBackdrop { anchors.fill: parent; source: MediaArtwork.displaySource; strength: 0.9 }
            Rectangle {
                anchors.fill: parent
                gradient: Gradient {
                    orientation: Gradient.Horizontal
                    GradientStop { position: 0; color: IrisStyle.tintFill(player.tint) }
                    GradientStop { position: 1; color: "transparent" }
                }
            }
        }
    }
    IrisMediaCard {
        id: card
        anchors.left: parent.left
        anchors.right: parent.right
        anchors.top: parent.top
        anchors.margins: player.inset
        showBackground: false
        active: player.contentActive
        tint: player.tint
    }
}
