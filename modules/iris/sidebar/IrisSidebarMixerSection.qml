pragma ComponentBehavior: Bound

import QtQuick
import QtQuick.Layouts
import Quickshell
import qs.services
import qs.modules.iris.components
import qs.modules.iris.style

IrisSidebarSectionFrame {
    id: mixer

    component EmptyLabel: IrisText {
        Layout.fillWidth: true
        color: IrisStyle.muted
        wrapMode: Text.WordWrap
        font.pixelSize: IrisStyle.typeMeta
    }

    readonly property var streams: Audio.outputAppNodes.filter(node => node?.audio)
    title: Translation.tr("Sound mixer")
    glyph: "graphic_eq"
    tint: IrisStyle.identity.indigo
    detail: mixer.streams.length > 0 ? String(mixer.streams.length) : ""
    summary: [
        EmptyLabel { visible: mixer.streams.length === 0; text: Translation.tr("No app is playing sound.") },
        Row {
            visible: mixer.streams.length > 0
            spacing: 6 * mixer.sectionHost.d
            Repeater {
                model: mixer.streams.slice(0, 8)
                IrisImage {
                    required property var modelData
                    width: Math.round(22 * mixer.sectionHost.d)
                    height: width
                    source: Quickshell.iconPath(MprisController.streamIconName(modelData), "audio-x-generic")
                }
            }
        }
    ]
    EmptyLabel { visible: mixer.streams.length === 0; text: Translation.tr("No app is playing sound.") }
    Repeater {
        model: mixer.streams
        RowLayout {
            id: stream
            required property var modelData
            readonly property bool muted: stream.modelData?.audio?.muted ?? false
            Layout.fillWidth: true
            spacing: 10 * mixer.sectionHost.d
            IrisImage {
                Layout.preferredWidth: Math.round(26 * mixer.sectionHost.d)
                Layout.preferredHeight: Layout.preferredWidth
                source: Quickshell.iconPath(MprisController.streamIconName(stream.modelData), "audio-x-generic")
                opacity: stream.muted ? 0.45 : 1
            }
            ColumnLayout {
                Layout.fillWidth: true
                spacing: 4 * mixer.sectionHost.d
                IrisText {
                    Layout.fillWidth: true
                    text: Audio.appNodeDisplayName(stream.modelData)
                    font.pixelSize: IrisStyle.typeMeta
                    font.weight: IrisStyle.weight(Font.Medium)
                    elide: Text.ElideRight
                }
                IrisScrubber {
                    Layout.fillWidth: true
                    fillColor: stream.muted ? IrisStyle.muted : IrisStyle.text
                    value: Math.min(1, stream.modelData?.audio?.volume ?? 0)
                    onMoved: next => { if (stream.modelData?.audio) stream.modelData.audio.volume = next }
                }
            }
            IrisIconButton {
                materialIcon: stream.muted ? "volume_off" : "volume_up"
                selected: stream.muted
                Accessible.name: Translation.tr("Mute")
                onClicked: if (stream.modelData?.audio) stream.modelData.audio.muted = !stream.muted
            }
        }
    }
}
