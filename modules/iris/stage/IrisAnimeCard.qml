pragma ComponentBehavior: Bound

import QtQuick
import QtQuick.Layouts
import Quickshell.Widgets
import qs.services
import qs.services.deferred
import qs.modules.common
import qs.modules.common.widgets
import qs.modules.iris.style
import qs.modules.iris.components
import qs.modules.iris.pieces

ColumnLayout {
    id: root
    readonly property real d: IrisStyle.density
    readonly property var shows: IrisPieces.animeUpcoming.slice(0,
        Math.max(3, Math.min(8, Number(Config.options?.iris?.anime?.shows ?? 5))))
    readonly property var next: IrisPieces.animeNext
    spacing: 8 * root.d
    Component.onCompleted: AnimeService.fetchTopAiring()
    CardHeader {
        glyph: IrisPieces.glyphOf("anime", "")
        tint: IrisStyle.identity.pink
        title: Translation.tr("Airing")
        detail: root.next
            ? Translation.tr("Ep %1 in %2").arg(root.next.nextEpisode).arg(IrisPieces.animeWait(root.next.airingAt))
            : !Network.online ? Network.offlineReason
            : AnimeService.lastError.length > 0 && !AnimeService.loading ? Translation.tr("AniList didn't answer (´・ω・`)")
            : Translation.tr("Nothing scheduled yet")
        IrisIconButton {
            enabled: Network.online
            materialIcon: "refresh"
            Accessible.name: Translation.tr("Check again")
            onClicked: { AnimeService.invalidateTopCache(); AnimeService.fetchTopAiring() }
        }
    }
    Repeater {
        model: root.shows
        MouseArea {
            id: showRow
            required property var modelData
            readonly property bool followed: IrisPieces.animeFollows(showRow.modelData.id)
            Layout.fillWidth: true
            implicitHeight: Math.round(46 * root.d)
            cursorShape: Qt.PointingHandCursor
            hoverEnabled: true
            Accessible.role: Accessible.CheckBox
            Accessible.checked: showRow.followed
            Accessible.name: Translation.tr("Follow %1").arg(showRow.modelData.title)
            onClicked: IrisPieces.animeToggleFollow(showRow.modelData.id)
            Rectangle {
                anchors.fill: parent
                radius: IrisStyle.radiusTile
                color: showRow.containsMouse ? IrisStyle.fillHover : "transparent"
                Behavior on color { ColorAnimation { duration: IrisStyle.duration(120); easing.type: IrisStyle.feedbackEasing } }
            }
            RowLayout {
                anchors.fill: parent
                anchors.leftMargin: Math.round(6 * root.d)
                anchors.rightMargin: Math.round(8 * root.d)
                spacing: 10 * root.d
                ClippingRectangle {
                    implicitWidth: Math.round(28 * root.d)
                    implicitHeight: Math.round(38 * root.d)
                    radius: IrisStyle.iconRadius(width)
                    color: IrisStyle.fillQuiet
                    IrisImage {
                        anchors.fill: parent
                        source: String(showRow.modelData.imageSmall ?? showRow.modelData.image ?? "")
                    }
                }
                ColumnLayout {
                    Layout.fillWidth: true
                    spacing: 0
                    IrisText {
                        Layout.fillWidth: true
                        text: showRow.modelData.title
                        elide: Text.ElideRight
                        font.pixelSize: IrisStyle.typeLabel
                        font.weight: showRow.followed ? Font.DemiBold : Font.Normal
                    }
                    IrisText {
                        Layout.fillWidth: true
                        text: Translation.tr("Ep %1").arg(showRow.modelData.nextEpisode)
                        color: IrisStyle.muted
                        elide: Text.ElideRight
                        font.pixelSize: IrisStyle.typeFootnote
                    }
                }
                IrisText {
                    text: IrisPieces.animeWait(showRow.modelData.airingAt)
                    color: showRow.followed ? IrisStyle.identity.pink : IrisStyle.subtext
                    font.family: IrisStyle.fontNumbers
                    font.features: ({ "tnum": 1 })
                    font.pixelSize: IrisStyle.typeMeta
                    font.weight: IrisStyle.weight(Font.DemiBold)
                }
                MaterialSymbol {
                    opacity: showRow.followed ? 1 : showRow.containsMouse ? 0.5 : 0
                    text: "bookmark"
                    fill: showRow.followed ? 1 : 0
                    iconSize: Math.round(15 * root.d)
                    color: showRow.followed ? IrisStyle.identity.pink : IrisStyle.subtext
                    Behavior on opacity { NumberAnimation { duration: IrisStyle.duration(120); easing.type: IrisStyle.feedbackEasing } }
                }
            }
        }
    }
    IrisText {
        Layout.fillWidth: true
        visible: root.shows.length > 0
        text: IrisPieces.animeFollowing.length > 0
            ? Translation.tr("The bubble shows what you follow first")
            : Translation.tr("Tap a show to follow it")
        color: IrisStyle.muted
        font.pixelSize: IrisStyle.typeMeta
        horizontalAlignment: Text.AlignHCenter
    }
}
