pragma ComponentBehavior: Bound

import QtQuick
import qs.modules.common
import qs.modules.iris.style

Item {
    id: root
    property string loadError: ""
    readonly property bool ready: root._completed && root.loadError.length === 0
        && background.ready && reservations.ready && bar.ready
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
        requested: Config.ready && IrisGate.official
            && (Config.options?.enabledPanels ?? []).includes(identifier)
            && extraCondition
        onFailed: (path, errorString) => root.reportFailure(path, errorString)
    }

    CriticalPanelLoader {
        id: background
        identifier: "irisBackground"
        extraCondition: !(Config.options?.iris?.modules?.desktopWidgets ?? true)
        sourcePath: "modules/iris/background/IrisBackground.qml"
    }

    AsyncPanelLoader {
        id: reservations
        requested: Config.ready && IrisGate.official
        sourcePath: "modules/iris/frame/IrisReservations.qml"
        onFailed: (path, errorString) => root.reportFailure(path, errorString)
    }

    CriticalPanelLoader {
        id: bar
        identifier: "irisBar"
        sourcePath: "modules/iris/bar/IrisBar.qml"
    }
}
