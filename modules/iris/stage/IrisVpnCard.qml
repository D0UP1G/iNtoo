pragma ComponentBehavior: Bound

import QtQuick
import QtQuick.Layouts
import Quickshell
import qs
import qs.services
import qs.modules.common.functions
import qs.modules.common.widgets
import qs.modules.iris.style
import qs.modules.iris.components

ColumnLayout {
    id: root
    readonly property real d: IrisStyle.density
    spacing: 8 * root.d
    Component.onCompleted: Vpn.keepAlive()
    Component.onDestruction: Vpn.releaseKeepAlive()
    // Hides addresses and the account for a screenshot; the values still copy whole.
    property bool masked: true
    property bool copied: false
    // A window the card hands off to (a file chooser, NetworkManager's editor, a terminal) opens with the card gone.
    Connections {
        target: Vpn
        function onHandOff(): void { GlobalStates.irisBubbleCard = null }
    }
    Timer { id: copiedTimer; interval: 1400; onTriggered: root.copied = false }
    function copy(value: string): void {
        Quickshell.clipboardText = value
        root.copied = true
        copiedTimer.restart()
    }
    function hide(value: string): string {
        if (!root.masked || value.length === 0) return value
        if (/^\d+\.\d+\.\d+\.\d+/.test(value)) return value.replace(/^(\d+\.\d+)\.\d+\.\d+/, "$1.•••.•••")
        if (value.includes("@")) return value.slice(0, 2) + "•••" + value.slice(value.indexOf("@"))
        if (value.includes(":")) return value.split(":")[0] + ":••••:••••"
        return value.split(".")[0] + ".•••"
    }
    function rate(bytes: real): string {
        return bytes >= 1048576 ? (bytes / 1048576).toFixed(1) + " MB/s"
            : bytes >= 1024 ? (bytes / 1024).toFixed(1) + " KB/s" : Math.round(bytes) + " B/s"
    }
    function traffic(dev: string): string {
        const r = Vpn.rates[dev]
        return r ? "↓ " + root.rate(r.down) + "   ↑ " + root.rate(r.up) : ""
    }
    CardHeader {
        glyph: Vpn.connected ? "vpn_lock" : "vpn_key_off"
        tint: Vpn.connected ? IrisStyle.identity.green : IrisStyle.muted
        title: Translation.tr("VPN")
        detail: root.copied ? Translation.tr("Copied")
            : Vpn.busy ? Translation.tr("Working…")
            : Vpn.connected ? Translation.tr("On through %1").arg(Vpn.activeName)
            : Translation.tr("Not connected")
        IrisIconButton {
            visible: Vpn.details
            materialIcon: root.masked ? "visibility_off" : "visibility"
            selected: root.masked
            Accessible.name: root.masked ? Translation.tr("Show addresses") : Translation.tr("Hide addresses")
            onClicked: root.masked = !root.masked
        }
        IrisIconButton {
            materialIcon: "info"
            selected: Vpn.details
            Accessible.name: Vpn.details ? Translation.tr("Hide details") : Translation.tr("Show details")
            onClicked: Vpn.setDetails(!Vpn.details)
        }
        IrisIconButton {
            materialIcon: "refresh"
            Accessible.name: Translation.tr("Check again")
            onClicked: Vpn.refresh()
        }
    }
    IrisText {
        Layout.fillWidth: true
        visible: Vpn.profiles.length === 0
        text: Vpn.available
            ? Translation.tr("OpenVPN and WireGuard profiles you add in NetworkManager show up here too.")
            : Translation.tr("No VPN yet. Add an OpenVPN or WireGuard profile in NetworkManager, or install Tailscale.")
        color: IrisStyle.muted
        wrapMode: Text.WordWrap
        font.pixelSize: IrisStyle.typeMeta
    }
    VpnRow {
        Layout.fillWidth: true
        visible: Vpn.tailscaleInstalled
        label: "Tailscale"
        detail: Vpn.tailscaleUp ? root.hide(Vpn.tailscaleAddress)
            : Vpn.tailscaleSignedOut ? Translation.tr("Signed out") : Translation.tr("Off")
        on: Vpn.tailscaleUp
        onToggled: Vpn.toggleTailscale()
    }
    IrisButton {
        Layout.fillWidth: true
        visible: Vpn.tailscaleSignedOut
        enabled: !Vpn.signingIn
        emphasized: true
        text: Vpn.signingIn ? Translation.tr("Finish in your browser…") : Translation.tr("Sign in to Tailscale")
        buttonRadius: IrisStyle.radiusTile
        implicitHeight: Math.round(34 * root.d)
        onClicked: Vpn.signInTailscale()
    }
    VpnFacts {
        Layout.fillWidth: true
        visible: Vpn.details && Vpn.tailscaleInstalled && Vpn.tailscaleAddress.length > 0
        VpnFact { Layout.fillWidth: true; label: Translation.tr("IPv4"); value: Vpn.tailscaleAddress; shown: root.hide(value); onCopy: value => root.copy(value) }
        VpnFact { Layout.fillWidth: true; label: Translation.tr("IPv6"); value: Vpn.tailscaleAddress6; shown: root.hide(value); onCopy: value => root.copy(value) }
        VpnFact { Layout.fillWidth: true; label: Translation.tr("Name"); value: Vpn.tailscaleDns; shown: root.hide(value); onCopy: value => root.copy(value) }
        VpnFact { Layout.fillWidth: true; label: Translation.tr("Account"); value: Vpn.tailscaleAccount; shown: root.hide(value); onCopy: value => root.copy(value) }
        VpnFact { Layout.fillWidth: true; label: Translation.tr("Exit node"); value: Vpn.tailscaleExitName.length > 0 ? Vpn.tailscaleExitName : Translation.tr("None, direct"); onCopy: value => root.copy(value) }
        VpnFact { Layout.fillWidth: true; label: Translation.tr("Traffic"); value: Vpn.tailscaleUp ? root.traffic("tailscale0") : ""; onCopy: value => root.copy(value) }
    }
    ColumnLayout {
        Layout.fillWidth: true
        visible: Vpn.details && Vpn.tailscaleDevices.length > 0
        spacing: Math.round(2 * root.d)
        IrisText {
            Layout.leftMargin: Math.round(4 * root.d)
            Layout.topMargin: Math.round(4 * root.d)
            text: Translation.tr("Devices · %1 of %2 online").arg(Vpn.tailscaleOnline).arg(Vpn.tailscaleDevices.length)
            color: IrisStyle.muted
            font.pixelSize: IrisStyle.typeFootnote
            font.weight: IrisStyle.weight(Font.Medium)
        }
        Repeater {
            model: Vpn.tailscaleDevices.slice(0, 6)
            MouseArea {
                id: device
                required property var modelData
                Layout.fillWidth: true
                implicitHeight: Math.round(36 * root.d)
                hoverEnabled: true
                cursorShape: Qt.PointingHandCursor
                onClicked: root.copy(device.modelData.address)
                Accessible.role: Accessible.Button
                Accessible.name: device.modelData.name
                Rectangle {
                    anchors.fill: parent
                    radius: IrisStyle.radiusRow
                    color: device.containsMouse ? IrisStyle.fillHover : ColorUtils.applyAlpha(IrisStyle.fillHover, 0)
                    Behavior on color { ColorAnimation { duration: IrisStyle.duration(120); easing.type: IrisStyle.feedbackEasing } }
                }
                RowLayout {
                    anchors.fill: parent
                    anchors.leftMargin: Math.round(10 * root.d)
                    anchors.rightMargin: Math.round(8 * root.d)
                    spacing: Math.round(10 * root.d)
                    Rectangle {
                        implicitWidth: Math.round(8 * root.d)
                        implicitHeight: implicitWidth
                        radius: width / 2
                        color: device.modelData.online ? IrisStyle.identity.green : IrisStyle.fillActive
                    }
                    ColumnLayout {
                        Layout.fillWidth: true
                        spacing: 0
                        IrisText {
                            Layout.fillWidth: true
                            text: device.modelData.name
                            elide: Text.ElideRight
                            font.pixelSize: IrisStyle.typeLabel
                            color: device.modelData.online ? IrisStyle.text : IrisStyle.textSecondary
                        }
                        IrisText {
                            Layout.fillWidth: true
                            text: [root.hide(device.modelData.address), device.modelData.os].filter(part => part.length > 0).join(" · ")
                            elide: Text.ElideRight
                            color: IrisStyle.muted
                            font.pixelSize: IrisStyle.typeFootnote
                            font.features: ({ "tnum": 1 })
                        }
                    }
                    IrisChip {
                        visible: Vpn.tailscaleUp && (device.modelData.exitOption || device.modelData.exitNode)
                        selected: device.modelData.exitNode
                        glyph: "logout"
                        label: device.modelData.exitNode ? Translation.tr("Exit node") : Translation.tr("Use as exit")
                        Accessible.name: device.modelData.exitNode ? Translation.tr("Stop using %1 as exit node").arg(device.modelData.name)
                            : Translation.tr("Send traffic through %1").arg(device.modelData.name)
                        onClicked: Vpn.setExitNode(device.modelData.exitNode ? "" : device.modelData.address)
                    }
                }
            }
        }
        IrisText {
            Layout.leftMargin: Math.round(10 * root.d)
            visible: Vpn.tailscaleDevices.length > 6
            text: Translation.tr("+%1 more").arg(Vpn.tailscaleDevices.length - 6)
            color: IrisStyle.muted
            font.pixelSize: IrisStyle.typeFootnote
        }
    }
    Repeater {
        model: Vpn.profiles
        ColumnLayout {
            id: profile
            required property var modelData
            readonly property var facts: Vpn.profileDetails[profile.modelData.uuid] ?? null
            Layout.fillWidth: true
            spacing: Math.round(4 * root.d)
            VpnRow {
                Layout.fillWidth: true
                label: profile.modelData.name
                detail: profile.modelData.active && profile.facts?.address ? profile.modelData.type + " · " + root.hide(profile.facts.address) : profile.modelData.type
                on: profile.modelData.active
                onToggled: Vpn.toggleProfile(profile.modelData.uuid)
            }
            VpnFacts {
                Layout.fillWidth: true
                visible: Vpn.details && profile.modelData.active && profile.facts !== null
                VpnFact { Layout.fillWidth: true; label: Translation.tr("Address"); value: profile.facts?.address ?? ""; shown: root.hide(value); onCopy: value => root.copy(value) }
                VpnFact { Layout.fillWidth: true; label: Translation.tr("Gateway"); value: profile.facts?.gateway ?? ""; shown: root.hide(value); onCopy: value => root.copy(value) }
                VpnFact { Layout.fillWidth: true; label: Translation.tr("DNS"); value: profile.facts?.dns ?? ""; shown: root.hide(value); onCopy: value => root.copy(value) }
                VpnFact { Layout.fillWidth: true; label: Translation.tr("Interface"); value: profile.facts?.device ?? ""; onCopy: value => root.copy(value) }
                VpnFact { Layout.fillWidth: true; label: Translation.tr("Traffic"); value: root.traffic(profile.facts?.device ?? ""); onCopy: value => root.copy(value) }
            }
            VpnFacts {
                Layout.fillWidth: true
                visible: Vpn.details
                VpnFact {
                    Layout.fillWidth: true
                    label: Translation.tr("Connect automatically")
                    glyph: ""
                    value: profile.modelData.autoconnect ? Translation.tr("On") : Translation.tr("Off")
                    onCopy: Vpn.setAutoconnect(profile.modelData.uuid, !profile.modelData.autoconnect)
                }
                VpnFact {
                    Layout.fillWidth: true
                    visible: Vpn.canCreate
                    label: Translation.tr("Settings")
                    glyph: "open_in_new"
                    value: Vpn.hasEditor ? Translation.tr("NetworkManager") : "nmtui"
                    onCopy: Vpn.editProfile(profile.modelData.uuid)
                }
            }
        }
    }
    IrisText {
        Layout.fillWidth: true
        visible: Vpn.errorText.length > 0
        text: Vpn.errorText
        color: IrisStyle.danger
        wrapMode: Text.WordWrap
        maximumLineCount: 3
        elide: Text.ElideRight
        font.pixelSize: IrisStyle.typeMeta
    }
    IrisText {
        Layout.fillWidth: true
        visible: Vpn.hasNmcli && !Vpn.canCreate
        text: Vpn.missingEditorText
        color: IrisStyle.muted
        wrapMode: Text.WordWrap
        font.pixelSize: IrisStyle.typeMeta
    }
    IrisActionRow {
        Layout.fillWidth: true
        visible: Vpn.hasNmcli
        spacing: Math.round(6 * root.d)
        IrisButton {
            Layout.fillWidth: true
            visible: Vpn.canCreate
            enabled: !Vpn.busy
            text: Vpn.hasEditor ? Translation.tr("New WireGuard…") : Translation.tr("New VPN…")
            buttonRadius: IrisStyle.radiusTile
            onClicked: Vpn.newProfile("wireguard")
        }
        IrisButton {
            Layout.fillWidth: true
            visible: Vpn.hasEditor && Vpn.hasVpnPlugins
            enabled: !Vpn.busy
            text: Translation.tr("Other VPN…")
            buttonRadius: IrisStyle.radiusTile
            onClicked: Vpn.newProfile("vpn")
        }
        IrisButton {
            Layout.fillWidth: true
            enabled: !Vpn.busy
            text: Translation.tr("Import a file…")
            buttonRadius: IrisStyle.radiusTile
            onClicked: Vpn.chooseProfileFile()
        }
    }
    IrisButton {
        Layout.fillWidth: true
        visible: Vpn.needsTailscaleOperator
        enabled: !Vpn.busy
        text: Translation.tr("Let me control Tailscale")
        buttonRadius: IrisStyle.radiusTile
        implicitHeight: Math.round(34 * root.d)
        onClicked: Vpn.fixTailscaleOperator()
    }

    component VpnRow: MouseArea {
        id: vpnRow
        property string label: ""
        property string detail: ""
        property bool on: false
        signal toggled()
        implicitHeight: Math.round(42 * root.d)
        cursorShape: Qt.PointingHandCursor
        hoverEnabled: true
        enabled: !Vpn.busy
        Accessible.role: Accessible.CheckBox
        Accessible.name: vpnRow.label
        Accessible.checked: vpnRow.on
        onClicked: vpnRow.toggled()
        Rectangle {
            anchors.fill: parent
            radius: IrisStyle.radiusTile
            color: vpnRow.containsMouse ? IrisStyle.fillHover : IrisStyle.fillQuiet
            Behavior on color { ColorAnimation { duration: IrisStyle.duration(120); easing.type: IrisStyle.feedbackEasing } }
        }
        RowLayout {
            anchors.fill: parent
            anchors.leftMargin: Math.round(12 * root.d)
            anchors.rightMargin: Math.round(10 * root.d)
            spacing: 10 * root.d
            ColumnLayout {
                Layout.fillWidth: true
                spacing: 0
                IrisText {
                    Layout.fillWidth: true
                    text: vpnRow.label
                    elide: Text.ElideRight
                    font.pixelSize: IrisStyle.typeLabel
                    font.weight: vpnRow.on ? Font.DemiBold : Font.Normal
                }
                IrisText {
                    Layout.fillWidth: true
                    visible: vpnRow.detail.length > 0
                    text: vpnRow.detail
                    color: IrisStyle.muted
                    elide: Text.ElideRight
                    font.pixelSize: IrisStyle.typeFootnote
                }
            }
            Rectangle {
                implicitWidth: Math.round(38 * root.d)
                implicitHeight: Math.round(22 * root.d)
                radius: height / 2
                color: vpnRow.on ? IrisStyle.identity.green : IrisStyle.fillHover
                Behavior on color { ColorAnimation { duration: IrisStyle.duration(140); easing.type: IrisStyle.feedbackEasing } }
                Rectangle {
                    y: Math.round(3 * root.d)
                    x: vpnRow.on ? parent.width - width - Math.round(3 * root.d) : Math.round(3 * root.d)
                    width: parent.height - Math.round(6 * root.d)
                    height: width
                    radius: width / 2
                    color: IrisStyle.onTint
                    Behavior on x { NumberAnimation { duration: IrisStyle.duration(140); easing.type: IrisStyle.feedbackEasing } }
                }
            }
        }
    }

    // One fact of a connection: its name quiet on the left, the value on the right; a tap copies it.
    component VpnFact: MouseArea {
        id: fact
        property string label: ""
        property string value: ""
        property string shown: fact.value
        property string glyph: "content_copy"
        signal copy(string value)
        implicitHeight: Math.round(28 * root.d)
        hoverEnabled: true
        cursorShape: fact.value.length > 0 ? Qt.PointingHandCursor : Qt.ArrowCursor
        visible: fact.value.length > 0
        onClicked: fact.copy(fact.value)
        Accessible.role: Accessible.Button
        Accessible.name: fact.label + " " + fact.shown
        Rectangle {
            anchors.fill: parent
            radius: IrisStyle.radiusRow
            color: fact.containsMouse ? IrisStyle.fillHover : ColorUtils.applyAlpha(IrisStyle.fillHover, 0)
            Behavior on color { ColorAnimation { duration: IrisStyle.duration(120); easing.type: IrisStyle.feedbackEasing } }
        }
        RowLayout {
            anchors.fill: parent
            anchors.leftMargin: Math.round(10 * root.d)
            anchors.rightMargin: Math.round(8 * root.d)
            spacing: Math.round(8 * root.d)
            IrisText {
                text: fact.label
                color: IrisStyle.muted
                font.pixelSize: IrisStyle.typeMeta
            }
            IrisText {
                Layout.fillWidth: true
                horizontalAlignment: Text.AlignRight
                text: fact.shown
                elide: Text.ElideMiddle
                font.pixelSize: IrisStyle.typeMeta
                font.features: ({ "tnum": 1 })
            }
            MaterialSymbol {
                visible: fact.glyph.length > 0
                text: fact.glyph
                iconSize: Math.round(14 * root.d)
                color: IrisStyle.muted
                opacity: fact.containsMouse ? 1 : 0
                Behavior on opacity { NumberAnimation { duration: IrisStyle.duration(120); easing.type: IrisStyle.feedbackEasing } }
            }
        }
    }
    // A connection's facts rest on one quiet fill under its row.
    component VpnFacts: Rectangle {
        default property alias facts: factColumn.data
        implicitHeight: factColumn.implicitHeight + Math.round(8 * root.d)
        radius: IrisStyle.radiusTile
        color: IrisStyle.fillQuiet
        ColumnLayout {
            id: factColumn
            anchors.fill: parent
            anchors.margins: Math.round(4 * root.d)
            spacing: 0
        }
    }

}
