pragma ComponentBehavior: Bound

import QtQuick
import QtQuick.Layouts
import qs.services
import qs.modules.common
import qs.modules.common.widgets
import qs.modules.iris.components
import qs.modules.iris.style

IrisSidebarSectionFrame {
    id: focusTimer
    readonly property bool ticking: TimerService.pomodoroRunning && !TimerService.pomodoroPaused
    readonly property int secondsLeft: TimerService.pomodoroSecondsLeft
    readonly property real progress: TimerService.pomodoroLapDuration > 0 ? 1 - focusTimer.secondsLeft / TimerService.pomodoroLapDuration : 0
    readonly property string clock: Math.floor(focusTimer.secondsLeft / 60) + ":" + String(focusTimer.secondsLeft % 60).padStart(2, "0")
    readonly property string phase: TimerService.pomodoroLongBreak ? Translation.tr("Long break")
        : TimerService.pomodoroBreak ? Translation.tr("Break") : Translation.tr("Focus")
    readonly property color phaseTint: TimerService.pomodoroBreak ? IrisStyle.success : IrisStyle.secondaryAccent
    title: Translation.tr("Focus timer")
    glyph: "timer"
    tint: IrisStyle.identity.orange
    detail: TimerService.pomodoroRunning && !focusTimer.open ? focusTimer.clock : ""
    summary: RowLayout {
        Layout.fillWidth: true
        spacing: 10 * focusTimer.sectionHost.d
        IrisSidebarGaugeRing {
            density: focusTimer.sectionHost.d
            Layout.preferredWidth: Math.round(26 * focusTimer.sectionHost.d)
            Layout.preferredHeight: Layout.preferredWidth
            stroke: Math.round(3 * focusTimer.sectionHost.d)
            tint: focusTimer.phaseTint
            progress: focusTimer.progress
        }
        IrisText {
            text: focusTimer.clock
            color: focusTimer.ticking ? focusTimer.phaseTint : IrisStyle.text
            font.family: IrisStyle.fontNumbers
            font.pixelSize: 20 * IrisStyle.typeScale
            font.weight: IrisStyle.weight(Font.Bold)
        }
        IrisText { Layout.fillWidth: true; text: focusTimer.phase; color: IrisStyle.muted; font.pixelSize: IrisStyle.typeMeta }
        IrisIconButton {
            materialIcon: focusTimer.ticking ? "pause" : "play_arrow"
            Accessible.name: focusTimer.ticking ? Translation.tr("Pause") : Translation.tr("Start")
            onClicked: TimerService.togglePomodoro()
        }
    }
    Item {
        Layout.fillWidth: true
        implicitHeight: Math.round(132 * focusTimer.sectionHost.d)
        IrisSidebarGaugeRing {
            density: focusTimer.sectionHost.d
            anchors.centerIn: parent
            width: Math.round(128 * focusTimer.sectionHost.d)
            height: width
            stroke: Math.round(8 * focusTimer.sectionHost.d)
            tint: focusTimer.phaseTint
            progress: focusTimer.progress
        }
        Column {
            anchors.centerIn: parent
            IrisText {
                anchors.horizontalCenter: parent.horizontalCenter
                text: focusTimer.clock
                font.family: IrisStyle.fontNumbers
                font.pixelSize: 28 * IrisStyle.typeScale
                font.weight: IrisStyle.weight(Font.Bold)
            }
            IrisText { anchors.horizontalCenter: parent.horizontalCenter; text: focusTimer.phase; color: focusTimer.phaseTint; font.pixelSize: IrisStyle.typeMeta; font.weight: IrisStyle.weight(Font.DemiBold) }
        }
    }
    Row {
        Layout.alignment: Qt.AlignHCenter
        spacing: 6 * focusTimer.sectionHost.d
        Repeater {
            model: TimerService.cyclesBeforeLongBreak
            Rectangle {
                required property int index
                width: Math.round(6 * focusTimer.sectionHost.d)
                height: width
                radius: width / 2
                color: index < TimerService.pomodoroCycle % TimerService.cyclesBeforeLongBreak ? IrisStyle.secondaryAccent : IrisStyle.fillHover
            }
        }
    }
    RowLayout {
        Layout.fillWidth: true
        spacing: 8 * focusTimer.sectionHost.d
        IrisButton {
            Layout.fillWidth: true
            text: Translation.tr("Reset")
            implicitHeight: Math.round(36 * focusTimer.sectionHost.d)
            buttonRadius: height / 2
            buttonRadiusPressed: height / 2
            onClicked: TimerService.resetPomodoro()
        }
        IrisButton {
            Layout.fillWidth: true
            emphasized: true
            text: focusTimer.ticking ? Translation.tr("Pause") : Translation.tr("Start")
            implicitHeight: Math.round(36 * focusTimer.sectionHost.d)
            buttonRadius: height / 2
            buttonRadiusPressed: height / 2
            onClicked: TimerService.togglePomodoro()
        }
    }
}
