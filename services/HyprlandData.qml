pragma Singleton
pragma ComponentBehavior: Bound

import QtQuick
import Quickshell
import Quickshell.Io
import Quickshell.Hyprland
import qs.services

/**
 * Provides access to some Hyprland data not available in Quickshell.Hyprland.
 */
Singleton {
    id: root
    property var windowList: []
    property var addresses: []
    property var windowByAddress: ({})
    property var workspaces: []
    property var workspaceIds: []
    property var workspaceById: ({})
    property var activeWorkspace: null
    property var monitors: []
    property var layers: ({})

    // One bit per collector: clients, monitors, layers, workspaces, active workspace.
    property int _pendingRefresh: 0

    function _scheduleRefresh() {
        if (CompositorService.isHyprland && _pendingRefresh !== 0 && !refreshTimer.running)
            refreshTimer.start();
    }

    function _requestRefresh(mask) {
        if (!CompositorService.isHyprland)
            return;
        _pendingRefresh |= mask;
        _scheduleRefresh();
    }

    function _startRefresh(process, mask) {
        if ((_pendingRefresh & mask) === 0 || process.running)
            return;
        // Clear before starting so requests arriving during collection remain pending.
        _pendingRefresh &= ~mask;
        process.running = true;
    }

    function _flushRefresh() {
        if (!CompositorService.isHyprland) {
            _pendingRefresh = 0;
            return;
        }
        _startRefresh(getClients, 1);
        _startRefresh(getMonitors, 2);
        _startRefresh(getLayers, 4);
        _startRefresh(getWorkspaces, 8);
        _startRefresh(getActiveWorkspace, 16);
        // Busy collectors keep their bits; their completion schedules the next tick.
    }

    Timer {
        id: refreshTimer
        interval: 0
        repeat: false
        onTriggered: root._flushRefresh()
    }

    function updateWindowList() {
        _requestRefresh(1);
    }

    function updateLayers() {
        _requestRefresh(4);
    }

    function updateMonitors() {
        _requestRefresh(2);
    }

    function updateWorkspaces() {
        _requestRefresh(8 | 16);
    }

    function updateAll() {
        _requestRefresh(1 | 2 | 4 | 8 | 16);
    }

    function biggestWindowForWorkspace(workspaceId) {
        const windowsInThisWorkspace = HyprlandData.windowList.filter(w => w.workspace.id == workspaceId);
        return windowsInThisWorkspace.reduce((maxWin, win) => {
            const maxArea = (maxWin?.size?.[0] ?? 0) * (maxWin?.size?.[1] ?? 0);
            const winArea = (win?.size?.[0] ?? 0) * (win?.size?.[1] ?? 0);
            return winArea > maxArea ? win : maxWin;
        }, null);
    }

    Component.onCompleted: {
        if (!CompositorService.isHyprland)
            return;
        updateAll();
    }

    Connections {
        target: CompositorService.isHyprland ? Hyprland : null
        enabled: CompositorService.isHyprland

        function onRawEvent(event) {
            // console.log("Hyprland raw event:", event.name);
            updateAll()
        }
    }

    Process {
        id: getClients
        command: ["/usr/bin/hyprctl", "clients", "-j"]
        onRunningChanged: if (!running) root._scheduleRefresh()
        stdout: StdioCollector {
            id: clientsCollector
            onStreamFinished: {
                try {
                    root.windowList = JSON.parse(clientsCollector.text)
                } catch (e) {
                    console.log("[HyprlandData] Failed to parse clients JSON:", e);
                    root.windowList = [];
                }
                let tempWinByAddress = {};
                for (var i = 0; i < root.windowList.length; ++i) {
                    var win = root.windowList[i];
                    tempWinByAddress[win.address] = win;
                }
                root.windowByAddress = tempWinByAddress;
                root.addresses = root.windowList.map(win => win.address);
            }
        }
    }

    Process {
        id: getMonitors
        command: ["/usr/bin/hyprctl", "monitors", "-j"]
        onRunningChanged: if (!running) root._scheduleRefresh()
        stdout: StdioCollector {
            id: monitorsCollector
            onStreamFinished: {
                try {
                    root.monitors = JSON.parse(monitorsCollector.text);
                } catch (e) {
                    console.log("[HyprlandData] Failed to parse monitors JSON:", e);
                    root.monitors = [];
                }
            }
        }
    }

    Process {
        id: getLayers
        command: ["/usr/bin/hyprctl", "layers", "-j"]
        onRunningChanged: if (!running) root._scheduleRefresh()
        stdout: StdioCollector {
            id: layersCollector
            onStreamFinished: {
                try {
                    root.layers = JSON.parse(layersCollector.text);
                } catch (e) {
                    console.log("[HyprlandData] Failed to parse layers JSON:", e);
                    root.layers = {};
                }
            }
        }
    }

    Process {
        id: getWorkspaces
        command: ["/usr/bin/hyprctl", "workspaces", "-j"]
        onRunningChanged: if (!running) root._scheduleRefresh()
        stdout: StdioCollector {
            id: workspacesCollector
            onStreamFinished: {
                try {
                    root.workspaces = JSON.parse(workspacesCollector.text);
                } catch (e) {
                    console.log("[HyprlandData] Failed to parse workspaces JSON:", e);
                    root.workspaces = [];
                }
                let tempWorkspaceById = {};
                for (var i = 0; i < root.workspaces.length; ++i) {
                    var ws = root.workspaces[i];
                    tempWorkspaceById[ws.id] = ws;
                }
                root.workspaceById = tempWorkspaceById;
                root.workspaceIds = root.workspaces.map(ws => ws.id);
            }
        }
    }

    Process {
        id: getActiveWorkspace
        command: ["/usr/bin/hyprctl", "activeworkspace", "-j"]
        onRunningChanged: if (!running) root._scheduleRefresh()
        stdout: StdioCollector {
            id: activeWorkspaceCollector
            onStreamFinished: {
                try {
                    root.activeWorkspace = JSON.parse(activeWorkspaceCollector.text);
                } catch (e) {
                    console.log("[HyprlandData] Failed to parse active workspace JSON:", e);
                    root.activeWorkspace = null;
                }
            }
        }
    }
}
