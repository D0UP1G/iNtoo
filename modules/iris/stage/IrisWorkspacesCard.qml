pragma ComponentBehavior: Bound

import QtQuick
import QtQuick.Layouts
import qs.services
import qs.modules.common.widgets
import qs.modules.iris.style
import qs.modules.iris.components
import qs.modules.iris.pieces

ColumnLayout {
    id: root
    required property string screenName
    required property var closeRequestedCallback
    readonly property real d: IrisStyle.density
            readonly property var workspaces: (NiriService.allWorkspaces ?? [])
                .filter(ws => ws.output === root.screenName && !MinimizedWindows.isStashWorkspace(ws.id))
                .slice().sort((a, b) => Number(a.idx ?? 0) - Number(b.idx ?? 0))
            readonly property var windows: (NiriService.windows ?? [])
                .filter(window => root.workspaces.some(ws => ws.id === window.workspace_id))
            spacing: 10 * root.d
            CardHeader {
                glyph: "grid_view"
                tint: IrisStyle.identity.purple
                title: Translation.tr("Workspaces")
                detail: Translation.tr("%1 workspaces · %2 windows").arg(root.workspaces.length).arg(root.windows.length)
                IrisIconButton {
                    materialIcon: "space_dashboard"
                    Accessible.name: Translation.tr("Open overview")
                    onClicked: { root.closeRequestedCallback(); NiriService.toggleOverview() }
                }
            }
            Repeater {
                model: root.workspaces
                RowLayout {
                    id: workspaceRow
                    required property var modelData
                    Layout.fillWidth: true
                    spacing: 8 * root.d
                    Rectangle {
                        Layout.preferredWidth: Math.round(44 * root.d)
                        Layout.preferredHeight: Layout.preferredWidth
                        radius: IrisStyle.iconRadius(width)
                        color: workspaceRow.modelData.is_active ? IrisStyle.tintFill(IrisStyle.accent) : IrisStyle.fillQuiet
                        border.width: workspaceRow.modelData.is_active ? 0 : 1
                        border.color: IrisStyle.border
                        IrisText {
                            anchors.centerIn: parent
                            text: workspaceRow.modelData.idx ?? "–"
                            color: workspaceRow.modelData.is_active ? IrisStyle.accent : IrisStyle.text
                            font.family: IrisStyle.fontNumbers
                            font.features: ({ "tnum": 1 })
                            font.pixelSize: IrisStyle.typeBody
                            font.weight: IrisStyle.weight(Font.Bold)
                        }
                        MouseArea {
                            anchors.fill: parent
                            cursorShape: Qt.PointingHandCursor
                            onClicked: {
                                root.closeRequestedCallback()
                                NiriService.switchToWorkspaceById(workspaceRow.modelData.id)
                            }
                        }
                    }
                    IrisColumnStrip {
                        Layout.fillWidth: true
                        Layout.preferredHeight: Math.round(44 * root.d)
                        workspaceId: workspaceRow.modelData.id
                        windows: root.windows
                        outputName: root.screenName
                        active: workspaceRow.modelData.is_active ?? false
                        onWindowActivated: windowId => { root.closeRequestedCallback(); NiriService.focusWindow(windowId) }
                    }
                }
            }
        }
