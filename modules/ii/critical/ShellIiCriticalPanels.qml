pragma ComponentBehavior: Bound

import QtQuick
import qs
import qs.modules.common

Item {
    id: root

    readonly property bool barVertical: Config.options?.bar?.vertical ?? false
    readonly property bool barPill: (Config.options?.bar?.appearanceStyle ?? "classic") === "pill"
    readonly property bool barM3: (Config.options?.bar?.appearanceStyle ?? "classic") === "m3"
    readonly property bool barStock: !root.barPill && !root.barM3
    property string loadError: ""
    readonly property bool ready: root._completed && root.loadError.length === 0
        && background.ready && stockBar.ready && pillBar.ready && m3Bar.ready
        && verticalBar.ready && dock.ready
    property bool _completed: false
    signal failed(string path, string errorString)
    Component.onCompleted: root._completed = true

    function reportFailure(path: string, errorString: string): void {
        root.loadError = path + ": " + errorString
        root.failed(path, errorString)
    }

    component CriticalPanelLoader: AsyncPanelLoader {
        required property string identifier
        property bool extraCondition: true
        requested: Config.ready
            && (Config.options?.enabledPanels ?? []).includes(identifier)
            && extraCondition
        onFailed: (path, errorString) => root.reportFailure(path, errorString)
    }

    // Compile only selected sources, including the active bar variant.
    CriticalPanelLoader { id: background; identifier: "iiBackground"; sourcePath: "modules/background/Background.qml" }
    CriticalPanelLoader { id: stockBar; identifier: "iiBar"; extraCondition: !root.barVertical && root.barStock; sourcePath: "modules/bar/Bar.qml" }
    CriticalPanelLoader { id: pillBar; identifier: "iiBar"; extraCondition: !root.barVertical && root.barPill && !GlobalStates.widgetEditMode; sourcePath: "modules/pill/PillBar.qml" }
    CriticalPanelLoader { id: m3Bar; identifier: "iiBar"; extraCondition: !root.barVertical && root.barM3 && !GlobalStates.widgetEditMode; sourcePath: "modules/barM3/M3Bar.qml" }
    CriticalPanelLoader { id: verticalBar; identifier: "iiVerticalBar"; extraCondition: root.barVertical; sourcePath: "modules/verticalBar/VerticalBar.qml" }
    CriticalPanelLoader { id: dock; identifier: "iiDock"; extraCondition: Config.options?.dock?.enable ?? true; sourcePath: "modules/dock/Dock.qml" }
}
