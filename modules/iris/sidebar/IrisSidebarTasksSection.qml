pragma ComponentBehavior: Bound

import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import qs.services
import qs.modules.iris.components
import qs.modules.iris.style

IrisSidebarSectionFrame {
    id: tasks
    component EmptyLabel: IrisText {
        Layout.fillWidth: true
        color: IrisStyle.muted
        wrapMode: Text.WordWrap
        font.pixelSize: IrisStyle.typeMeta
    }
    property bool showCompleted: false
    readonly property var pending: Todo.list.map((task, index) => ({ task: task, index: index })).filter(entry => !entry.task.done)
    readonly property var entries: Todo.list.map((task, index) => ({ task: task, index: index }))
        .filter(entry => tasks.showCompleted || !entry.task.done)
    title: Translation.tr("Tasks")
    glyph: "checklist"
    tint: IrisStyle.identity.orange
    detail: tasks.pending.length > 0 ? String(tasks.pending.length) : ""
    actions: IrisIconButton {
        materialIcon: "done_all"
        selected: tasks.showCompleted
        Accessible.name: Translation.tr("Show completed tasks")
        onClicked: tasks.showCompleted = !tasks.showCompleted
    }
    summary: [
        EmptyLabel { visible: tasks.pending.length === 0; text: Translation.tr("A clear list.") },
        Repeater {
            model: tasks.pending.slice(0, 2)
            RowLayout {
                id: pendingRow
                required property var modelData
                Layout.fillWidth: true
                spacing: 8 * tasks.sectionHost.d
                Rectangle {
                    implicitWidth: Math.round(12 * tasks.sectionHost.d)
                    implicitHeight: implicitWidth
                    radius: width / 2
                    color: "transparent"
                    border.width: Math.max(1, Math.round(1.5 * tasks.sectionHost.d))
                    border.color: IrisStyle.identity.orange
                }
                IrisText { Layout.fillWidth: true; text: pendingRow.modelData.task.content; textFormat: Text.PlainText; elide: Text.ElideRight; font.pixelSize: IrisStyle.typeLabel }
            }
        }
    ]
    RowLayout {
        Layout.fillWidth: true
        IrisField {
            id: taskInput
            Layout.fillWidth: true
            implicitHeight: 38 * tasks.sectionHost.d
            topPadding: 8 * tasks.sectionHost.d
            bottomPadding: 8 * tasks.sectionHost.d
            verticalAlignment: TextInput.AlignVCenter
            placeholderText: Translation.tr("Add a task…")
            font.pixelSize: IrisStyle.typeLabel
            onAccepted: { if (text.trim()) { Todo.addTask(text.trim()); clear() } }
        }
        IrisIconButton {
            materialIcon: "add"
            enabled: taskInput.text.trim().length > 0
            Accessible.name: Translation.tr("Add task")
            onClicked: { Todo.addTask(taskInput.text.trim()); taskInput.clear(); taskInput.forceActiveFocus() }
        }
    }
    EmptyLabel { visible: tasks.entries.length === 0; text: Translation.tr("A clear list. Room for what matters next.") }
    Repeater {
        model: tasks.entries
        RowLayout {
            id: taskRow
            required property var modelData
            Layout.fillWidth: true
            spacing: 8 * tasks.sectionHost.d
            IrisIconButton {
                materialIcon: taskRow.modelData.task.done ? "check_circle" : "radio_button_unchecked"
                selected: taskRow.modelData.task.done
                Accessible.name: taskRow.modelData.task.content
                onClicked: {
                    if (taskRow.modelData.task.done) Todo.markUnfinished(taskRow.modelData.index)
                    else Todo.markDone(taskRow.modelData.index)
                }
            }
            IrisText {
                Layout.fillWidth: true
                text: taskRow.modelData.task.content
                textFormat: Text.PlainText
                wrapMode: Text.Wrap
                font.pixelSize: IrisStyle.typeLabel
                font.strikeout: taskRow.modelData.task.done
                color: taskRow.modelData.task.done ? IrisStyle.muted : IrisStyle.text
            }
        }
    }
}
