pragma ComponentBehavior: Bound

import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import Quickshell
import Quickshell.Wayland
import qs
import qs.modules.common
import qs.modules.common.widgets
import qs.services

PanelWindow {
    id: root
    property bool editingModel: false
    screen: Quickshell.screens.find(s => s.name === NiriService.currentOutput) ?? Quickshell.screens[0]
    visible: MascotAssistant.opened && !GlobalStates.screenLocked
    color: "transparent"
    anchors {
        bottom: true
        right: true
    }
    margins {
        bottom: 28
        right: 24
    }
    implicitWidth: Math.max(280, Math.min(570, (screen?.width ?? 640) - 48))
    implicitHeight: Math.max(300, Math.min(560, (screen?.height ?? 640) - 56))
    exclusiveZone: 0
    WlrLayershell.namespace: "quickshell:mascotAssistant"
    WlrLayershell.layer: WlrLayer.Overlay
    WlrLayershell.keyboardFocus: visible ? WlrKeyboardFocus.Exclusive : WlrKeyboardFocus.None
    mask: Region {
        item: contents
    }

    function submit(): void {
        if (MascotAssistant.send(input.text, history.includeHistory, actions.allowActions)) {
            input.clear();
            actions.allowActions = false;
        }
    }

    Item {
        id: contents
        anchors.fill: parent
        focus: true
        Keys.onEscapePressed: MascotAssistant.opened = false

        Item {
            id: companion
            anchors {
                left: parent.left
                bottom: parent.bottom
                bottomMargin: 16
            }
            width: 108
            height: 152
            property bool busy: MascotAssistant.busy
            visible: root.width >= 450

            Rectangle {
                id: face
                width: 88
                height: 104
                anchors.horizontalCenter: parent.horizontalCenter
                y: 28
                radius: 30
                color: Appearance.colors.colPrimaryContainer
                border.width: 2
                border.color: Appearance.colors.colPrimary

                Rectangle {
                    width: 4
                    height: 18
                    radius: 2
                    anchors.horizontalCenter: parent.horizontalCenter
                    y: -17
                    color: Appearance.colors.colPrimary
                    Rectangle {
                        width: 12
                        height: 12
                        radius: 6
                        anchors.horizontalCenter: parent.horizontalCenter
                        y: -7
                        color: companion.busy ? Appearance.colors.colSecondary : Appearance.colors.colPrimary
                    }
                }
                Rectangle {
                    x: 10
                    y: 18
                    width: 68
                    height: 43
                    radius: 19
                    color: Appearance.colors.colLayer1
                    Row {
                        anchors.centerIn: parent
                        spacing: 20
                        Repeater {
                            model: 2
                            delegate: Rectangle {
                                required property int index
                                width: 8
                                height: blink.closed ? 2 : 13
                                radius: 4
                                color: Appearance.colors.colPrimary
                                Behavior on height {
                                    NumberAnimation {
                                        duration: 100
                                    }
                                }
                            }
                        }
                    }
                }
                Text {
                    anchors.horizontalCenter: parent.horizontalCenter
                    y: 63
                    text: ">_"
                    color: Appearance.colors.colOnPrimaryContainer
                    font.pixelSize: 22
                    font.bold: true
                    font.family: Appearance.font.family.monospace
                }
                Repeater {
                    model: 2
                    delegate: Rectangle {
                        required property int index
                        x: index === 0 ? 12 : 55
                        y: 99
                        width: 23
                        height: 12
                        radius: 6
                        color: Appearance.colors.colPrimary
                    }
                }
                SequentialAnimation on y {
                    loops: Animation.Infinite
                    running: companion.visible && Appearance.animationsEnabled
                    NumberAnimation {
                        to: 20
                        duration: companion.busy ? 450 : 1100
                        easing.type: Easing.InOutSine
                    }
                    NumberAnimation {
                        to: 28
                        duration: companion.busy ? 450 : 1100
                        easing.type: Easing.InOutSine
                    }
                }
            }
            Timer {
                id: blink
                property bool closed: false
                interval: closed ? 140 : 3600
                running: companion.visible && Appearance.animationsEnabled
                repeat: true
                onTriggered: closed = !closed
            }
        }

        Rectangle {
            id: card
            anchors {
                top: parent.top
                right: parent.right
                bottom: parent.bottom
            }
            width: parent.width - (companion.visible ? 116 : 0)
            radius: Appearance.rounding.normal
            color: Appearance.colors.colLayer1
            border.width: 1
            border.color: Appearance.colors.colOutlineVariant
            opacity: root.visible ? 1 : 0
            Behavior on opacity {
                NumberAnimation {
                    duration: Appearance.animationsEnabled ? 180 : 0
                }
            }

            ColumnLayout {
                anchors.fill: parent
                anchors.margins: 14
                spacing: 8

                RowLayout {
                    Layout.fillWidth: true
                    StyledText {
                        Layout.fillWidth: true
                        text: Translation.tr("iNtoo companion")
                        font.bold: true
                        font.pixelSize: Appearance.font.pixelSize.larger
                    }
                    RippleButton {
                        buttonText: Translation.tr("Clear")
                        enabled: !MascotAssistant.busy
                        onClicked: MascotAssistant.clear()
                    }
                    RippleButton {
                        buttonText: "×"
                        Accessible.name: Translation.tr("Close")
                        Layout.preferredWidth: 36
                        onClicked: MascotAssistant.opened = false
                    }
                }
                RowLayout {
                    Layout.fillWidth: true
                    spacing: 8
                    ColumnLayout {
                        Layout.preferredWidth: card.width * 0.35
                        Layout.minimumWidth: 0
                        spacing: 4
                        StyledText {
                            text: Translation.tr("Provider")
                            font.pixelSize: Appearance.font.pixelSize.smaller
                            color: Appearance.colors.colSubtext
                        }
                        StyledComboBox {
                            Layout.fillWidth: true
                            Layout.minimumWidth: 0
                            enableSettingsSearch: false
                            enabled: !MascotAssistant.busy
                            model: MascotAssistant.providers
                            textRole: "label"
                            valueRole: "value"
                            currentIndex: MascotAssistant.providers.findIndex(provider => provider.value === MascotAssistant.providerId)
                            Accessible.name: Translation.tr("Provider")
                            onActivated: index => {
                                root.editingModel = false;
                                MascotAssistant.selectProvider(MascotAssistant.providers[index].value);
                            }
                        }
                    }
                    ColumnLayout {
                        Layout.fillWidth: true
                        Layout.minimumWidth: 0
                        spacing: 4
                        StyledText {
                            text: Translation.tr("Model")
                            font.pixelSize: Appearance.font.pixelSize.smaller
                            color: Appearance.colors.colSubtext
                        }
                        StyledComboBox {
                            Layout.fillWidth: true
                            Layout.minimumWidth: 0
                            enableSettingsSearch: false
                            enabled: !MascotAssistant.busy && MascotAssistant.modelOptions.length > 0
                            model: MascotAssistant.modelOptions
                            textRole: "label"
                            valueRole: "value"
                            currentIndex: MascotAssistant.modelOptions.findIndex(model => model.value === MascotAssistant.selectedModel)
                            displayText: currentIndex >= 0 ? currentText : Translation.tr("Choose a model")
                            Accessible.name: Translation.tr("Model")
                            ToolTip.visible: hovered && currentIndex >= 0
                            ToolTip.text: MascotAssistant.selectedModel
                            onActivated: index => MascotAssistant.selectModel(MascotAssistant.modelOptions[index].value)
                        }
                    }
                    RippleButton {
                        Layout.alignment: Qt.AlignBottom
                        Layout.preferredWidth: 32
                        Layout.preferredHeight: 38
                        enabled: !MascotAssistant.busy
                        Accessible.name: Translation.tr("Enter model name")
                        contentItem: MaterialSymbol {
                            text: "edit"
                            iconSize: 18
                            color: Appearance.colors.colPrimary
                        }
                        ToolTip.visible: hovered
                        ToolTip.text: Translation.tr("Enter model name")
                        onClicked: {
                            root.editingModel = !root.editingModel;
                            if (root.editingModel)
                                Qt.callLater(() => modelInput.forceActiveFocus());
                        }
                    }
                    RippleButton {
                        Layout.alignment: Qt.AlignBottom
                        Layout.preferredWidth: 32
                        Layout.preferredHeight: 38
                        enabled: !MascotAssistant.loadingModels && !MascotAssistant.busy
                        Accessible.name: Translation.tr("Refresh models")
                        contentItem: MaterialSymbol {
                            text: "refresh"
                            iconSize: 18
                            color: Appearance.colors.colPrimary
                        }
                        ToolTip.visible: hovered
                        ToolTip.text: Translation.tr("Refresh models")
                        onClicked: MascotAssistant.refreshModels()
                    }
                }
                MaterialTextField {
                    id: modelInput
                    visible: root.editingModel
                    Layout.fillWidth: true
                    enabled: !MascotAssistant.busy
                    enableSettingsSearch: false
                    Accessible.name: Translation.tr("Enter model name")
                    placeholderText: MascotAssistant.providerId + "/model"
                    text: MascotAssistant.modelReady ? MascotAssistant.selectedModel : ""
                    onEditingFinished: MascotAssistant.selectModel(text)
                    onAccepted: {
                        MascotAssistant.selectModel(text);
                        root.editingModel = false;
                        input.forceActiveFocus();
                    }
                }
                StyledText {
                    Layout.fillWidth: true
                    visible: MascotAssistant.loadingModels || !MascotAssistant.modelReady || MascotAssistant.catalogError.length > 0
                    text: MascotAssistant.loadingModels ? Translation.tr("Loading models…")
                        : !MascotAssistant.modelReady ? Translation.tr("Choose a model or enter its name. Connect a provider with: inir assistant auth")
                        : Translation.tr("Could not load all models. Enter a model name or refresh after connecting with: inir assistant auth")
                    font.pixelSize: Appearance.font.pixelSize.smaller
                    color: Appearance.colors.colSubtext
                    wrapMode: Text.WordWrap
                }
                StyledText {
                    visible: MascotAssistant.messages.length === 0
                    Layout.fillWidth: true
                    text: Translation.tr("Ask a question, review recent commands, or ask me to help with your system.")
                    wrapMode: Text.WordWrap
                    color: Appearance.colors.colSubtext
                }
                ListView {
                    id: chat
                    Layout.fillWidth: true
                    Layout.fillHeight: true
                    clip: true
                    spacing: 8
                    model: MascotAssistant.messages
                    onCountChanged: Qt.callLater(() => chat.positionViewAtEnd())
                    onContentHeightChanged: if (MascotAssistant.busy)
                        Qt.callLater(() => chat.positionViewAtEnd())
                    ScrollBar.vertical: ScrollBar {}
                    delegate: Rectangle {
                        id: bubble
                        required property var modelData
                        width: chat.width
                        implicitHeight: reply.implicitHeight + 20
                        radius: 12
                        color: modelData.role === "user" ? Appearance.colors.colPrimaryContainer : Appearance.colors.colLayer2
                        TextEdit {
                            id: reply
                            anchors {
                                left: parent.left
                                right: parent.right
                                top: parent.top
                                margins: 10
                            }
                            readOnly: true
                            selectByMouse: true
                            wrapMode: TextEdit.Wrap
                            textFormat: TextEdit.PlainText
                            text: bubble.modelData.text
                            color: bubble.modelData.role === "error" ? Appearance.colors.colError : bubble.modelData.role === "user" ? Appearance.colors.colOnPrimaryContainer : Appearance.colors.colOnLayer1
                            font.family: Appearance.font.family.main
                            font.pixelSize: Appearance.font.pixelSize.normal
                        }
                    }
                }
                StyledText {
                    visible: MascotAssistant.busy
                    text: MascotAssistant.status || Translation.tr("Thinking…")
                    color: Appearance.colors.colPrimary
                }
                RowLayout {
                    Layout.fillWidth: true
                    RippleButton {
                        id: history
                        property bool includeHistory: false
                        Accessible.name: Translation.tr("Command history")
                        contentItem: RowLayout {
                            spacing: 5
                            MaterialSymbol {
                                text: history.includeHistory ? "check_box" : "check_box_outline_blank"
                                iconSize: 18
                                color: Appearance.colors.colPrimary
                            }
                            StyledText {
                                text: Translation.tr("History")
                                font.pixelSize: Appearance.font.pixelSize.smaller
                            }
                        }
                        ToolTip.visible: hovered
                        ToolTip.text: Translation.tr("Attach saved commands from Bash, Zsh and Fish. Terminal output is not included.")
                        onClicked: includeHistory = !includeHistory
                    }
                    RippleButton {
                        id: actions
                        property bool allowActions: false
                        enabled: !MascotAssistant.busy
                        Accessible.name: Translation.tr("Allow system actions")
                        contentItem: RowLayout {
                            spacing: 5
                            MaterialSymbol {
                                text: actions.allowActions ? "check_box" : "check_box_outline_blank"
                                iconSize: 18
                                color: Appearance.colors.colPrimary
                            }
                            StyledText {
                                text: Translation.tr("Allow actions")
                                font.pixelSize: Appearance.font.pixelSize.smaller
                            }
                        }
                        ToolTip.visible: hovered
                        ToolTip.text: Translation.tr("Allow OpenCode to run commands and edit files for the next message.")
                        onClicked: allowActions = !allowActions
                    }
                }
                RippleButton {
                    Layout.fillWidth: true
                    visible: MascotAssistant.messages.length === 0
                    enabled: !MascotAssistant.busy && MascotAssistant.modelReady
                    buttonText: Translation.tr("Explain my recent terminal commands")
                    onClicked: {
                        history.includeHistory = true;
                        input.text = Translation.tr("Explain my recent terminal commands");
                        root.submit();
                    }
                }
                RowLayout {
                    Layout.fillWidth: true
                    MaterialTextField {
                        id: input
                        Layout.fillWidth: true
                        placeholderText: Translation.tr("Message…")
                        maximumLength: 16000
                        enabled: !MascotAssistant.busy
                        enableSettingsSearch: false
                        onAccepted: root.submit()
                        Component.onCompleted: forceActiveFocus()
                    }
                    RippleButton {
                        buttonText: MascotAssistant.busy ? Translation.tr("Stop") : Translation.tr("Send")
                        enabled: MascotAssistant.busy || (MascotAssistant.modelReady && input.text.trim().length > 0)
                        onClicked: MascotAssistant.busy ? MascotAssistant.cancel() : root.submit()
                    }
                }
            }
        }
    }
    onVisibleChanged: if (visible)
        Qt.callLater(() => input.forceActiveFocus())
}
