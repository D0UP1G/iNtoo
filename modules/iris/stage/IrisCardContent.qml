pragma ComponentBehavior: Bound

import QtQuick
import QtQuick.Layouts
import Quickshell
import Quickshell.Widgets
import qs
import qs.services
import qs.modules.common
import qs.modules.common.functions
import qs.modules.common.widgets
import qs.modules.iris.style
import qs.modules.iris.components
import qs.modules.iris.bar.island
import qs.modules.iris.sidebar

Item {
    id: root

    property string kind: ""
    property string screenName: ""
    property bool contentActive: true
    readonly property real d: IrisStyle.density
    readonly property bool bleeds: ["weather", "notifications", "media", "visualizer"].includes(root.kind)
        || root.kind === "media" || root.kind === "calendar"
    readonly property real contentHeight: body.item?.implicitHeight ?? 0
    readonly property color light: {
        switch (root.kind) {
        case "weather": return IrisStyle.skyLight(Icons.getWeatherIcon(Weather.data.wCode, Weather.isNightNow()) ?? "")
        case "sound": return IrisStyle.identity.indigo
        case "mic": return IrisStyle.identity.orange
        case "tools": return IrisStyle.secondaryAccent
        case "tray": return IrisStyle.identity.teal
        case "calendar": return IrisStyle.identity.red
        case "media":
        case "visualizer": return "transparent"
        case "network": return IrisStyle.identity.blue
        case "bluetooth": return IrisStyle.identity.sky
        case "vitals": return IrisStyle.identity.teal
        case "workspaces": return IrisStyle.identity.purple
        case "updates": return IrisStyle.secondaryAccent
        case "anime": return IrisStyle.identity.pink
        case "watching": return IrisStyle.identity.pink
        case "shellUpdate": return IrisStyle.secondaryAccent
        case "vpn": return IrisStyle.identity.green
        default: return IrisStyle.wallpaperLight
        }
    }
    signal navigate()
    function close(): void { root.navigate() }

    function loaderState(loader: Loader): var {
        let status = "null"
        if (loader.status === Loader.Loading) status = "loading"
        else if (loader.status === Loader.Ready) status = "ready"
        else if (loader.status === Loader.Error) status = "error"
        return { status: status, source: loader.source.toString(), itemPresent: loader.item !== null }
    }

    function diagnosticSnapshot(): var {
        const loaders = []
        let current = body
        for (let depth = 0; current instanceof Loader && depth < 8; depth++) {
            loaders.push(root.loaderState(current))
            const item = current.item
            if (!item) break
            if (typeof item.diagnosticSnapshot === "function") {
                const nested = item.diagnosticSnapshot()
                if (nested && Array.isArray(nested.loaders)) loaders.push(...nested.loaders)
                break
            }
            if (!(item instanceof Loader)) break
            current = item
        }
        return { actualKind: root.kind, contentHeight: root.contentHeight,
                 bodyStatus: body.status === Loader.Ready ? "ready"
                     : body.status === Loader.Loading ? "loading"
                     : body.status === Loader.Error ? "error" : "null",
                 bodyItemPresent: body.item !== null, loaders: loaders }
    }

    implicitHeight: root.contentHeight

    Loader {
        id: body
        width: root.width
        active: root.kind.length > 0
        sourceComponent: {
            switch (root.kind) {
            case "weather": return weatherCard
            case "notifications": return notificationsCard
            case "calendar": return calendarCard
            case "sound": return soundCard
            case "mic": return micCard
            case "tools": return toolsCard
            case "tray": return trayCard
            case "media":
            case "visualizer": return mediaCard
            case "network": return networkCard
            case "bluetooth": return bluetoothCard
            case "vitals": return vitalsCard
            case "workspaces": return workspacesCard
            case "updates": return updatesCard
            case "anime": return animeCard
            case "watching": return watchingCard
            case "shellUpdate": return shellUpdateCard
            case "vpn": return vpnCard
            default: return null
            }
        }
    }

    component SectionCard: IrisSidebarSection {
        bare: true
        expanded: true
        contentActive: root.contentActive
        onNavigate: root.close()
    }

    Component {
        id: weatherCard
        SectionCard { kind: "weather" }
    }
    Component {
        id: notificationsCard
        SectionCard { kind: "notifications" }
    }
    Component {
        id: calendarCard
        SectionCard { kind: "calendar" }
    }
    Component {
        id: soundCard
        Loader {
            Component.onCompleted: setSource(Qt.resolvedUrl("IrisAudioCard.qml"), {
                input: false,
                contentActive: Qt.binding(() => root.contentActive),
                closeRequestedCallback: () => root.close()
            })
        }
    }
    Component {
        id: micCard
        Loader {
            Component.onCompleted: setSource(Qt.resolvedUrl("IrisAudioCard.qml"), {
                input: true,
                contentActive: Qt.binding(() => root.contentActive),
                closeRequestedCallback: () => root.close()
            })
        }
    }
    Component {
        id: toolsCard
        Loader {
            Component.onCompleted: setSource(Qt.resolvedUrl("IrisToolsCard.qml"), {
                activityRequestedCallback: () => {
                    root.close()
                    GlobalStates.irisIslandPageRequest = "activity"
                }
            })
        }
    }
    Component {
        id: trayCard
        Loader { source: Qt.resolvedUrl("IrisTrayCard.qml") }
    }

    Component {
        id: networkCard
        Loader {
            Component.onCompleted: setSource(Qt.resolvedUrl("IrisNetworkCard.qml"))
        }
    }
    Component {
        id: bluetoothCard
        Loader {
            Component.onCompleted: setSource(Qt.resolvedUrl("IrisBluetoothCard.qml"))
        }
    }
    Component {
        id: vitalsCard
        Loader {
            Component.onCompleted: setSource(Qt.resolvedUrl("IrisVitalsCard.qml"), {
                contentActive: Qt.binding(() => root.contentActive)
            })
        }
    }
    Component {
        id: workspacesCard
        Loader {
            Component.onCompleted: setSource(Qt.resolvedUrl("IrisWorkspacesCard.qml"), {
                screenName: Qt.binding(() => root.screenName),
                closeRequestedCallback: () => root.close()
            })
        }
    }
    Component {
        id: updatesCard
        Loader {
            Component.onCompleted: setSource(Qt.resolvedUrl("IrisUpdatesCard.qml"), {
                closeRequestedCallback: () => root.close()
            })
        }
    }
    Component {
        id: vpnCard
        Loader {
            source: Qt.resolvedUrl("IrisVpnCard.qml")
        }
    }
    Component {
        id: shellUpdateCard
        Loader {
            Component.onCompleted: setSource(Qt.resolvedUrl("IrisShellUpdateCard.qml"), {
                closeRequestedCallback: () => root.close()
            })
        }
    }
    Component {
        id: animeCard
        Loader { source: Qt.resolvedUrl("IrisAnimeCard.qml") }
    }
    Component {
        id: watchingCard
        Loader {
            id: watchingLoader
            source: Qt.resolvedUrl("IrisWatchingCard.qml")
            Connections {
                target: watchingLoader.item
                function onCloseRequested(): void { root.close() }
            }
        }
    }
    Component {
        id: mediaCard
        Loader {
            id: mediaLoader
            Component.onCompleted: mediaLoader.setSource(Qt.resolvedUrl("IrisPlayerCard.qml"), {
                contentActive: Qt.binding(() => root.contentActive)
            })
        }
    }
}
