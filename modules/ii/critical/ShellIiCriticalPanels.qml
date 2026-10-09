pragma ComponentBehavior: Bound

import QtQuick
import Quickshell
import qs
import qs.modules.bar
import qs.modules.barM3 as BarM3
import qs.modules.dock
import qs.modules.pill
import qs.modules.verticalBar
import qs.modules.common

Item {
    id: root

    readonly property bool barVertical: Config.options?.bar?.vertical ?? false
    readonly property bool barPill: (Config.options?.bar?.appearanceStyle ?? "classic") === "pill"
    readonly property bool barM3: (Config.options?.bar?.appearanceStyle ?? "classic") === "m3"
    readonly property bool barStock: !root.barPill && !root.barM3

    component CriticalPanelLoader: LazyLoader {
        required property string identifier
        property bool extraCondition: true
        readonly property bool enabledPanel: Config.ready
            && (Config.options?.enabledPanels ?? []).includes(identifier)
            && extraCondition
        loading: enabledPanel
        activeAsync: enabledPanel
    }

    // Background.qml pulls in the full desktop widget library. Keep that
    // dependency tree out of this startup file and let QQmlComponent compile
    // it asynchronously once the Material family and panel are selected.
    component LazySourcePanelHost: Item {
        required property string sourcePath
        required property bool requested
        property var candidate: null

        function sync(): void {
            if (!requested) {
                if (candidate) {
                    candidate.destroy()
                    candidate = null
                }
                loader.loading = false
                loader.activeAsync = false
                loader.component = null
                return
            }
            if (candidate || loader.component) return
            const component = Qt.createComponent(Quickshell.shellPath(sourcePath), Component.Asynchronous)
            candidate = component
            const finish = () => {
                if (candidate !== component) return
                if (component.status === Component.Loading) return
                candidate = null
                if (component.status === Component.Error) {
                    console.warn("[PanelLoader] Could not compile", sourcePath, component.errorString())
                    component.destroy()
                    return
                }
                if (!requested) {
                    component.destroy()
                    return
                }
                loader.component = component
                loader.loading = true
                loader.activeAsync = true
            }
            if (component.status === Component.Loading)
                component.statusChanged.connect(finish)
            else
                finish()
        }

        LazyLoader { id: loader }
        onRequestedChanged: sync()
        Component.onCompleted: sync()
    }

    LazySourcePanelHost {
        sourcePath: "modules/background/Background.qml"
        requested: Config.ready && (Config.options?.enabledPanels ?? []).includes("iiBackground")
    }
    CriticalPanelLoader { identifier: "iiBar"; extraCondition: !root.barVertical && root.barStock; component: Bar {} }
    CriticalPanelLoader { identifier: "iiBar"; extraCondition: !root.barVertical && root.barPill && !GlobalStates.widgetEditMode; component: PillBar {} }
    CriticalPanelLoader { identifier: "iiBar"; extraCondition: !root.barVertical && root.barM3 && !GlobalStates.widgetEditMode; component: BarM3.M3Bar {} }
    CriticalPanelLoader { identifier: "iiVerticalBar"; extraCondition: root.barVertical; component: VerticalBar {} }
    CriticalPanelLoader { identifier: "iiDock"; extraCondition: Config.options?.dock?.enable ?? true; component: Dock {} }
}
