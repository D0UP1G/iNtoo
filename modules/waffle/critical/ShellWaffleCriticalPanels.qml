pragma ComponentBehavior: Bound

import QtQuick
import qs.modules.common

Item {
    id: root
    property string loadError: ""
    readonly property bool ready: root._completed && root.loadError.length === 0
        && bar.ready && background.ready && backdrop.ready
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

    CriticalPanelLoader {
        id: bar
        identifier: "wBar"
        sourcePath: "modules/waffle/bar/WaffleBar.qml"
    }

    CriticalPanelLoader {
        id: background
        identifier: "wBackground"
        sourcePath: "modules/waffle/background/WaffleBackground.qml"
    }

    CriticalPanelLoader {
        id: backdrop
        identifier: "wBackdrop"
        extraCondition: Config.options?.waffles?.background?.backdrop?.enable ?? true
        sourcePath: "modules/waffle/backdrop/WaffleBackdrop.qml"
    }
}
