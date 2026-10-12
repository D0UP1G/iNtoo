pragma ComponentBehavior: Bound

import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import qs.services
import qs.modules.iris.components
import qs.modules.iris.style

IrisSidebarSectionFrame {
    id: notes
    property bool loadingTab: false
    function loadTab(): void {
        notes.loadingTab = true
        noteEditor.text = Notepad.text
        noteTitle.text = Notepad.tabs[Notepad.currentTab]?.title ?? ""
        notes.loadingTab = false
    }
    Component.onCompleted: notes.loadTab()
    Connections {
        target: Notepad
        function onCurrentTabChanged(): void { notes.loadTab() }
        function onTabsChanged(): void { if (noteEditor.text !== Notepad.text) notes.loadTab() }
    }
    title: Translation.tr("Notes")
    glyph: "sticky_note_2"
    tint: IrisStyle.identity.yellow
    detail: Notepad.tabs.length > 1 ? String(Notepad.tabs.length) : ""
    actions: IrisIconButton {
        materialIcon: "add"
        Accessible.name: Translation.tr("New note")
        onClicked: { Notepad.addTab(""); noteEditor.forceActiveFocus() }
    }
    summary: [
        IrisText {
            Layout.fillWidth: true
            text: Notepad.tabs[Notepad.currentTab]?.title || Translation.tr("Untitled")
            font.weight: IrisStyle.weight(Font.DemiBold)
            font.pixelSize: IrisStyle.typeLabel
            elide: Text.ElideRight
        },
        IrisText {
            Layout.fillWidth: true
            text: String(Notepad.text ?? "").trim() || Translation.tr("Write something to keep…")
            textFormat: Text.PlainText
            color: IrisStyle.muted
            maximumLineCount: 2
            wrapMode: Text.WordWrap
            elide: Text.ElideRight
            font.pixelSize: IrisStyle.typeMeta
        }
    ]
    Flickable {
        Layout.fillWidth: true
        visible: Notepad.tabs.length > 1
        implicitHeight: tabs.implicitHeight
        contentWidth: tabs.implicitWidth
        boundsBehavior: Flickable.StopAtBounds
        clip: true
        Row {
            id: tabs
            spacing: 4 * notes.sectionHost.d
            Repeater {
                model: Notepad.tabs
                IrisButton {
                    required property var modelData
                    required property int index
                    text: modelData.title
                    selected: index === Notepad.currentTab
                    implicitHeight: 30 * notes.sectionHost.d
                    onClicked: Notepad.switchTab(index)
                }
            }
        }
    }
    Rectangle {
        Layout.fillWidth: true
        implicitHeight: noteSheet.implicitHeight
        radius: IrisStyle.radiusRow
        color: IrisStyle.field
        border.width: noteTitle.activeFocus || noteEditor.activeFocus ? 1 : 0
        border.color: IrisStyle.hairlineStrong
        ColumnLayout {
            id: noteSheet
            anchors.left: parent.left
            anchors.right: parent.right
            spacing: 0
            TextInput {
                id: noteTitle
                Layout.fillWidth: true
                Layout.topMargin: 11 * notes.sectionHost.d
                Layout.leftMargin: 12 * notes.sectionHost.d
                Layout.rightMargin: 12 * notes.sectionHost.d
                Layout.bottomMargin: 8 * notes.sectionHost.d
                color: IrisStyle.text
                selectionColor: IrisStyle.accentContainer
                selectedTextColor: IrisStyle.inkOnAccentContainer
                font.family: IrisStyle.fontMain
                font.pixelSize: IrisStyle.typeBody
                font.weight: IrisStyle.weight(Font.DemiBold)
                clip: true
                Accessible.name: Translation.tr("Note title")
                onEditingFinished: if (text.trim()) Notepad.setTabTitle(Notepad.currentTab, text.trim())
                IrisText {
                    anchors.verticalCenter: parent.verticalCenter
                    visible: noteTitle.text.length === 0
                    text: Translation.tr("Title")
                    color: IrisStyle.muted
                    font: noteTitle.font
                }
            }
            Rectangle {
                Layout.fillWidth: true
                Layout.leftMargin: 12 * notes.sectionHost.d
                implicitHeight: 1
                color: IrisStyle.hairline
            }
            ScrollView {
                Layout.fillWidth: true
                implicitHeight: 150 * notes.sectionHost.d
                TextArea {
                    id: noteEditor
                    placeholderText: Translation.tr("Write something to keep…")
                    placeholderTextColor: IrisStyle.muted
                    color: IrisStyle.text
                    selectionColor: IrisStyle.accentContainer
                    selectedTextColor: IrisStyle.inkOnAccentContainer
                    font.family: IrisStyle.fontMain
                    font.pixelSize: IrisStyle.typeLabel
                    wrapMode: TextEdit.Wrap
                    textFormat: TextEdit.PlainText
                    leftPadding: 12 * notes.sectionHost.d
                    rightPadding: 12 * notes.sectionHost.d
                    topPadding: 9 * notes.sectionHost.d
                    bottomPadding: 10 * notes.sectionHost.d
                    background: null
                    onTextChanged: if (!notes.loadingTab && activeFocus && text !== Notepad.text) Notepad.setTextValue(text)
                }
            }
        }
    }
}
