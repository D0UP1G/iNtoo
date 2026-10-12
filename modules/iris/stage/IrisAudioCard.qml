pragma ComponentBehavior: Bound

import QtQuick
import QtQuick.Layouts
import qs.services
import qs.modules.common
import qs.modules.common.widgets
import qs.modules.iris.style
import qs.modules.iris.components
import qs.modules.iris.sidebar

Loader {
    id: root
    required property bool input
    required property bool contentActive
    required property var closeRequestedCallback
    readonly property real d: IrisStyle.density
    readonly property var cardOptions: Config.options?.iris?.appearance?.surfaces?.cards ?? ({})
    sourceComponent: root.input ? micContent : soundContent

    function diagnosticSnapshot(): var {
        const status = root.status === Loader.Ready ? "ready"
            : root.status === Loader.Loading ? "loading"
            : root.status === Loader.Error ? "error" : "null"
        const loaders = [{ status: status, source: root.source.toString(), itemPresent: root.item !== null }]
        const mixer = root.item?.diagnosticMixerSection ?? null
        if (!root.input && mixer?.visible) {
            const nested = mixer.diagnosticSnapshot()
            if (nested && Array.isArray(nested.loaders)) loaders.push(...nested.loaders)
        }
        return { loaders: loaders }
    }

    component LevelCard: ColumnLayout {
        id: level
        property bool input: false
        readonly property bool muted: level.input ? Audio.micMuted : (Audio.sink?.audio?.muted ?? false)
        readonly property real value: Math.min(1, (level.input ? Audio.micVolume : Audio.value) ?? 0)
        spacing: 12 * root.d
        CardHeader {
            visible: root.cardOptions?.header ?? true
            glyph: level.input ? "mic" : "volume_up"
            tint: level.input ? IrisStyle.identity.orange : IrisStyle.identity.indigo
            title: level.input ? Translation.tr("Microphone") : Translation.tr("Sound")
            detail: Audio.friendlyDeviceName(level.input ? Audio.source : Audio.defaultSink)
            IrisNumber {
                text: Math.round(level.value * 100) + "%"
                color: IrisStyle.subtext
                pixelSize: IrisStyle.typeLabel
                weight: Font.DemiBold
            }
        }
        IrisCapsuleSlider {
            Layout.fillWidth: true
            implicitHeight: Math.round(48 * root.d)
            muted: level.muted
            icon: level.input ? (level.muted ? "mic_off" : "mic")
                : level.muted ? "volume_off" : level.value < 0.34 ? "volume_mute" : level.value < 0.67 ? "volume_down" : "volume_up"
            value: level.value
            Accessible.name: level.input ? Translation.tr("Microphone") : Translation.tr("Volume")
            onMoved: next => level.input ? Audio.setSourceVolume(next) : Audio.setSinkVolume(next)
            onIconClicked: level.input ? Audio.toggleMicMute() : Audio.toggleMute()
        }
        IrisDeviceList {
            visible: root.cardOptions?.devices ?? true
            Layout.fillWidth: true
            Layout.leftMargin: -6 * root.d
            Layout.rightMargin: -6 * root.d
            outputs: !level.input
            inputs: level.input
        }
    }
    component SectionCard: IrisSidebarSection {
        bare: true
        expanded: true
        contentActive: root.contentActive
        onNavigate: root.closeRequestedCallback()
    }
    Component {
        id: soundContent
        ColumnLayout {
            id: soundPanel
            property var diagnosticMixerSection: mixerSection
            spacing: 6 * root.d
            LevelCard { Layout.fillWidth: true }
            SectionCard {
                id: mixerSection
                visible: root.cardOptions?.mixer ?? true
                Layout.fillWidth: true
                Layout.leftMargin: -14 * root.d
                Layout.rightMargin: -14 * root.d
                kind: "mixer"
            }
        }
    }
    Component {
        id: micContent
        LevelCard { input: true }
    }
}
