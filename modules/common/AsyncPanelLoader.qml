pragma ComponentBehavior: Bound

import QtQuick
import Quickshell

// Keep unselected panels out of the QML compiler as well as the object tree.
// LazyLoader.source compiles immediately, even when the loader is inactive.
Item {
    id: root

    required property string sourcePath
    property bool requested: false
    readonly property bool active: loader.active
    readonly property bool ready: root._completed && (!root.requested || root.active)
    property var _candidate: null
    property var _component: null
    property bool _completed: false

    signal failed(string path, string errorString)

    function release(): void {
        loader.loading = false
        loader.activeAsync = false
        loader.component = null
        if (root._candidate) {
            root._candidate.destroy()
            root._candidate = null
        }
        if (root._component) {
            root._component.destroy()
            root._component = null
        }
    }

    function sync(): void {
        if (!root._completed) return
        if (!root.requested || root.sourcePath.length === 0) {
            root.release()
            return
        }
        if (root._candidate || root._component) return

        const path = root.sourcePath
        const component = Qt.createComponent(Quickshell.shellPath(path), Component.Asynchronous)
        root._candidate = component
        const finish = () => {
            if (root._candidate !== component || component.status === Component.Loading) return
            root._candidate = null
            if (component.status === Component.Error) {
                const error = component.errorString()
                console.warn("[PanelLoader] Could not compile", path, error)
                component.destroy()
                root.failed(path, error)
                return
            }
            if (!root.requested || root.sourcePath !== path) {
                component.destroy()
                return
            }
            root._component = component
            loader.component = component
            loader.activeAsync = true
        }
        if (component.status === Component.Loading)
            component.statusChanged.connect(finish)
        else
            finish()
    }

    LazyLoader {
        id: loader
        onActiveChanged: {
            if (active)
                console.info("[PanelLoader] ready", root.sourcePath)
        }
    }

    onRequestedChanged: root.sync()
    onSourcePathChanged: {
        if (!root._completed) return
        root.release()
        root.sync()
    }
    Component.onCompleted: {
        root._completed = true
        root.sync()
    }
}
