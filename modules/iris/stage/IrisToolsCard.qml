pragma ComponentBehavior: Bound

import QtQuick
import QtQuick.Layouts
import qs.services
import qs.modules.iris.style
import qs.modules.iris.bar

ColumnLayout {
    id: root

    readonly property real d: IrisStyle.density
    required property var activityRequestedCallback

    spacing: 14 * root.d
    CardHeader {
        glyph: "timer"
        tint: IrisStyle.identity.orange
        title: Translation.tr("Timers")
        detail: TimerService.countdownRunning || TimerService.pomodoroRunning || TimerService.stopwatchRunning
            ? Translation.tr("Running") : Translation.tr("Tap a dial to start, scroll to adjust")
    }
    IrisTools {
        Layout.fillWidth: true
        onActivityRequested: root.activityRequestedCallback()
    }
}
