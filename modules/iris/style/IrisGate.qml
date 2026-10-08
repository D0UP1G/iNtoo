pragma Singleton

import QtQuick
import Quickshell
import Quickshell.Io

// Island surfaces open only when the host shell is iNtoo: its own shell.qml loads
// ShellIrisPanels.qml. A host that embeds the module under a different
// shell.qml keeps them closed.
QtObject {
    id: root

    readonly property bool official: hostFile.text().includes("ShellIrisPanels.qml")

    property FileView hostFile: FileView {
        path: Quickshell.shellPath("shell.qml")
        blockLoading: true
        printErrors: false
    }

    Component.onCompleted: {
        if (!root.official)
            console.warn("[IrisGate] Island only runs inside iNtoo; this host is not iNtoo, so its surfaces stay closed.")
    }
}
