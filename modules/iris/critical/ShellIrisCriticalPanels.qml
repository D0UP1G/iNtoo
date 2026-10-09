pragma ComponentBehavior: Bound

import QtQuick
import Quickshell
import qs.modules.common
import qs.modules.iris.background
import qs.modules.iris.bar
import qs.modules.iris.frame
import qs.modules.iris.style

Item {
    id: root

    component CriticalPanelLoader: LazyLoader {
        required property string identifier
        property bool extraCondition: true
        readonly property bool enabledPanel: Config.ready && IrisGate.official
            && (Config.options?.enabledPanels ?? []).includes(identifier)
            && extraCondition
        loading: enabledPanel
        activeAsync: enabledPanel
    }

    CriticalPanelLoader {
        identifier: "irisBackground"
        extraCondition: !(Config.options?.iris?.modules?.desktopWidgets ?? true)
        component: IrisBackground {}
    }

    LazyLoader {
        loading: Config.ready && IrisGate.official
        activeAsync: Config.ready && IrisGate.official
        component: IrisReservations {}
    }

    CriticalPanelLoader {
        identifier: "irisBar"
        component: IrisBar {}
    }
}
