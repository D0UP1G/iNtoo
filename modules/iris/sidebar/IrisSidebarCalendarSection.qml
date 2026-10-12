pragma ComponentBehavior: Bound

import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import qs.services
import qs.modules.common
import qs.modules.iris.components
import qs.modules.iris.style

IrisSidebarSectionFrame {
    id: calendar
    property date selectedDate: DateTime.clock.date
    property bool composing: false
    readonly property date month: new Date(selectedDate.getFullYear(), selectedDate.getMonth(), 1)
    readonly property int weekStart: Qt.locale().firstDayOfWeek % 7
    readonly property int offset: (month.getDay() - calendar.weekStart + 7) % 7
    function eventsOn(date): var {
        Events.list; CalendarSync.events
        return Events.getAllEventsForDate(date)
            .concat(CalendarSync.getEventsForDate(date))
            .sort((a, b) => new Date(a.dateTime ?? a.startDate) - new Date(b.dateTime ?? b.startDate))
    }
    function timeOf(entry): string {
        return entry.allDay ? Translation.tr("All day") : Qt.formatTime(new Date(entry.dateTime ?? entry.startDate), "hh:mm")
    }
    readonly property var entries: calendar.eventsOn(calendar.selectedDate)
    readonly property var todayEntries: calendar.eventsOn(DateTime.clock.date)
    function moveMonth(delta: int): void {
        calendar.selectedDate = new Date(calendar.month.getFullYear(), calendar.month.getMonth() + delta, 1)
    }
    onOpenChanged: if (!open) calendar.composing = false

    title: Qt.locale().toString(calendar.open ? calendar.month : DateTime.clock.date, "MMMM yyyy")
    glyph: "calendar_month"
    tint: IrisStyle.identity.red
    detail: !calendar.open && calendar.todayEntries.length > 0 ? String(calendar.todayEntries.length) : ""
    actions: [
        IrisIconButton { materialIcon: "chevron_left"; Accessible.name: Translation.tr("Previous month"); onClicked: calendar.moveMonth(-1) },
        IrisIconButton { materialIcon: "today"; Accessible.name: Translation.tr("Today"); onClicked: calendar.selectedDate = DateTime.clock.date },
        IrisIconButton { materialIcon: "chevron_right"; Accessible.name: Translation.tr("Next month"); onClicked: calendar.moveMonth(1) }
    ]

    summary: [
        Row {
            id: weekStrip
            Layout.fillWidth: true
            readonly property date start: {
                const today = DateTime.clock.date
                const shift = (today.getDay() - calendar.weekStart + 7) % 7
                return new Date(today.getFullYear(), today.getMonth(), today.getDate() - shift)
            }
            Repeater {
                model: 7
                Column {
                    id: stripDay
                    required property int index
                    readonly property date date: new Date(weekStrip.start.getFullYear(), weekStrip.start.getMonth(), weekStrip.start.getDate() + stripDay.index)
                    readonly property bool today: stripDay.date.toDateString() === DateTime.clock.date.toDateString()
                    width: weekStrip.width / 7
                    spacing: 3 * calendar.sectionHost.d
                    IrisText {
                        anchors.horizontalCenter: parent.horizontalCenter
                        text: Qt.locale().dayName(stripDay.date.getDay(), Locale.ShortFormat).slice(0, 2)
                        color: stripDay.today ? IrisStyle.identity.red : IrisStyle.muted
                        font.pixelSize: IrisStyle.typeFootnote
                        font.weight: stripDay.today ? Font.DemiBold : Font.Normal
                    }
                    Rectangle {
                        anchors.horizontalCenter: parent.horizontalCenter
                        width: Math.round(28 * calendar.sectionHost.d)
                        height: width
                        radius: width / 2
                        color: stripDay.today ? IrisStyle.identity.red : "transparent"
                        IrisText {
                            anchors.centerIn: parent
                            text: String(stripDay.date.getDate())
                            color: stripDay.today ? IrisStyle.onTint : IrisStyle.text
                            font.family: IrisStyle.fontNumbers
                            font.pixelSize: IrisStyle.typeLabel
                            font.weight: stripDay.today ? Font.Bold : Font.Medium
                        }
                    }
                    Rectangle {
                        anchors.horizontalCenter: parent.horizontalCenter
                        width: Math.round(4 * calendar.sectionHost.d)
                        height: width
                        radius: width / 2
                        color: calendar.eventsOn(stripDay.date).length > 0 ? IrisStyle.secondaryAccent : "transparent"
                    }
                }
            }
        },
        IrisText {
            Layout.fillWidth: true
            text: calendar.todayEntries.length === 0 ? Translation.tr("Nothing scheduled today")
                : calendar.timeOf(calendar.todayEntries[0]) + "  ·  " + (calendar.todayEntries[0].title ?? "")
            color: calendar.todayEntries.length === 0 ? IrisStyle.muted : IrisStyle.subtext
            font.pixelSize: IrisStyle.typeMeta
            elide: Text.ElideRight
        }
    ]

    GridLayout {
        id: monthGrid
        Layout.fillWidth: true
        columns: 7
        columnSpacing: 0
        rowSpacing: 2 * calendar.sectionHost.d
        Repeater {
            model: 7
            Item {
                id: weekday
                required property int index
                Layout.fillWidth: true
                Layout.preferredWidth: 1
                implicitHeight: weekdayLabel.implicitHeight
                IrisText {
                    id: weekdayLabel
                    anchors.horizontalCenter: parent.horizontalCenter
                    text: Qt.locale().dayName((calendar.weekStart + weekday.index) % 7, Locale.ShortFormat).slice(0, 2)
                    color: IrisStyle.muted
                    font.pixelSize: IrisStyle.typeFootnote
                }
            }
        }
        Repeater {
            model: 42
            MouseArea {
                id: day
                required property int index
                readonly property date date: new Date(calendar.month.getFullYear(), calendar.month.getMonth(), day.index - calendar.offset + 1)
                readonly property bool today: day.date.toDateString() === DateTime.clock.date.toDateString()
                readonly property bool chosen: day.date.toDateString() === calendar.selectedDate.toDateString()
                readonly property bool inMonth: day.date.getMonth() === calendar.month.getMonth()
                readonly property bool hasEvents: day.inMonth && calendar.eventsOn(day.date).length > 0
                Layout.fillWidth: true
                Layout.preferredWidth: 1
                Layout.preferredHeight: Math.round(34 * calendar.sectionHost.d)
                hoverEnabled: true
                cursorShape: Qt.PointingHandCursor
                Accessible.role: Accessible.Button
                Accessible.name: Qt.formatDate(day.date, "dddd, d MMMM yyyy")
                onClicked: calendar.selectedDate = day.date
                Rectangle {
                    anchors.centerIn: parent
                    width: Math.round(30 * calendar.sectionHost.d)
                    height: width
                    radius: width / 2
                    color: day.today ? IrisStyle.identity.red
                        : day.chosen ? IrisStyle.tintFillHover(IrisStyle.accent)
                        : day.containsMouse ? IrisStyle.fillHover : "transparent"
                    Behavior on color { ColorAnimation { duration: IrisStyle.duration(110); easing.type: IrisStyle.feedbackEasing } }
                }
                IrisText {
                    anchors.centerIn: parent
                    text: String(day.date.getDate())
                    color: day.today ? IrisStyle.onTint : day.chosen ? IrisStyle.accent : IrisStyle.text
                    opacity: day.inMonth ? 1 : 0.32
                    font.family: IrisStyle.fontNumbers
                    font.pixelSize: IrisStyle.typeLabel
                    font.weight: day.today || day.chosen ? Font.Bold : Font.Medium
                }
                Rectangle {
                    visible: day.hasEvents && !day.today
                    anchors.horizontalCenter: parent.horizontalCenter
                    anchors.bottom: parent.bottom
                    width: Math.round(4 * calendar.sectionHost.d)
                    height: width
                    radius: width / 2
                    color: IrisStyle.secondaryAccent
                }
            }
        }
    }

    Rectangle { Layout.fillWidth: true; implicitHeight: 1; color: IrisStyle.hairline }

    RowLayout {
        Layout.fillWidth: true
        IrisText {
            Layout.fillWidth: true
            text: Qt.locale().toString(calendar.selectedDate, "dddd d MMMM")
            font.weight: IrisStyle.weight(Font.DemiBold)
            font.pixelSize: IrisStyle.typeLabel
            elide: Text.ElideRight
        }
        IrisText {
            visible: CalendarSync.fetching || (CalendarSync.enabled && CalendarSync.sources.length > 0 && !Network.online)
            text: CalendarSync.fetching ? Translation.tr("Syncing…") : Translation.tr("Offline")
            role: IrisText.Meta
        }
        IrisButton {
            visible: !calendar.composing
            text: Translation.tr("New event")
            implicitHeight: Math.round(28 * calendar.sectionHost.d)
            buttonRadius: height / 2
            buttonRadiusPressed: height / 2
            onClicked: { calendar.composing = true; eventTitle.forceActiveFocus() }
        }
    }
    EmptyLabel { visible: calendar.entries.length === 0 && !calendar.composing; text: Translation.tr("Nothing scheduled for this day.") }
    Repeater {
        model: calendar.entries
        RowLayout {
            id: entryRow
            required property var modelData
            readonly property bool local: entryRow.modelData.dateTime !== undefined && entryRow.modelData.id !== undefined
            Layout.fillWidth: true
            spacing: 10 * calendar.sectionHost.d
            IrisText {
                Layout.preferredWidth: Math.round(44 * calendar.sectionHost.d)
                text: calendar.timeOf(entryRow.modelData)
                color: IrisStyle.subtext
                font.family: IrisStyle.fontNumbers
                font.pixelSize: IrisStyle.typeMeta
                font.weight: IrisStyle.weight(Font.DemiBold)
            }
            Rectangle { implicitWidth: 3 * calendar.sectionHost.d; implicitHeight: Math.round(20 * calendar.sectionHost.d); radius: width / 2; color: entryRow.modelData.color || IrisStyle.secondaryAccent }
            IrisText { Layout.fillWidth: true; text: entryRow.modelData.title ?? ""; elide: Text.ElideRight; font.weight: IrisStyle.weight(Font.Medium) }
            IrisIconButton {
                visible: entryRow.local
                materialIcon: "close"
                iconSize: Math.round(15 * calendar.sectionHost.d)
                Accessible.name: Translation.tr("Remove event")
                onClicked: Events.removeEvent(entryRow.modelData.id)
            }
        }
    }

    ColumnLayout {
        id: eventSheet
        Layout.fillWidth: true
        visible: calendar.composing
        spacing: 12 * calendar.sectionHost.d
        readonly property var reminders: [
            { label: Translation.tr("No alert"), minutes: 0 },
            { label: "5 min", minutes: 5 },
            { label: "15 min", minutes: 15 },
            { label: "1 h", minutes: 60 }
        ]
        property int reminder: 15
        Connections {
            target: calendar
            function onComposingChanged(): void {
                if (!calendar.composing) return
                eventTitle.clear()
                timePicker.hour = (DateTime.clock.date.getHours() + 1) % 24
                timePicker.minuteIndex = 0
                eventSheet.reminder = 15
            }
        }
        function add(): void {
            const title = eventTitle.text.trim()
            if (!title) return
            const dayDate = calendar.selectedDate
            const when = new Date(dayDate.getFullYear(), dayDate.getMonth(), dayDate.getDate(), timePicker.hour, timePicker.minuteIndex * 5)
            Events.addEvent(title, "", when.toISOString(), "general", "normal", eventSheet.reminder, "none")
            calendar.composing = false
        }

        IrisField {
            id: eventTitle
            Layout.fillWidth: true
            implicitHeight: Math.round(40 * calendar.sectionHost.d)
            horizontalAlignment: TextInput.AlignHCenter
            verticalAlignment: TextInput.AlignVCenter
            font.pixelSize: IrisStyle.typeBody
            onAccepted: eventSheet.add()
            IrisText {
                anchors.centerIn: parent
                visible: eventTitle.text.length === 0
                text: Translation.tr("Event name")
                color: IrisStyle.muted
                font.pixelSize: IrisStyle.typeBody
            }
        }

        IrisWheelPicker {
            id: timePicker
            Layout.alignment: Qt.AlignHCenter
            columns: [{ count: 24 }, { count: 12, step: 5 }]
            property int hour: 0
            property int minuteIndex: 0
            values: [timePicker.hour, timePicker.minuteIndex]
            onMoved: (column, index) => column === 0 ? timePicker.hour = index : timePicker.minuteIndex = index
        }

        Row {
            Layout.alignment: Qt.AlignHCenter
            spacing: 4 * calendar.sectionHost.d
            Repeater {
                model: eventSheet.reminders
                IrisButton {
                    required property var modelData
                    text: modelData.label
                    selected: eventSheet.reminder === modelData.minutes
                    quiet: !selected
                    implicitHeight: Math.round(28 * calendar.sectionHost.d)
                    buttonRadius: height / 2
                    buttonRadiusPressed: height / 2
                    onClicked: eventSheet.reminder = modelData.minutes
                }
            }
        }

        RowLayout {
            Layout.fillWidth: true
            spacing: 8 * calendar.sectionHost.d
            IrisButton {
                Layout.fillWidth: true
                text: Translation.tr("Cancel")
                implicitHeight: Math.round(36 * calendar.sectionHost.d)
                buttonRadius: height / 2
                buttonRadiusPressed: height / 2
                onClicked: calendar.composing = false
            }
            IrisButton {
                Layout.fillWidth: true
                text: Translation.tr("Add")
                emphasized: true
                enabled: eventTitle.text.trim().length > 0
                implicitHeight: Math.round(36 * calendar.sectionHost.d)
                buttonRadius: height / 2
                buttonRadiusPressed: height / 2
                onClicked: eventSheet.add()
            }
        }
    }

    component EmptyLabel: IrisText {
        Layout.fillWidth: true
        color: IrisStyle.muted
        wrapMode: Text.WordWrap
        font.pixelSize: IrisStyle.typeMeta
    }
}
