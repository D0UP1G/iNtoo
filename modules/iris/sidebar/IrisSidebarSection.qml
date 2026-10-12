pragma ComponentBehavior: Bound

import QtQuick
import qs.modules.iris.style

Loader {
    id: root
    required property string kind
    property bool contentActive: true
    property bool expanded: false
    property bool bare: false
    property bool morphing: root.item?.morphing ?? false
    readonly property real d: IrisStyle.density
    signal navigate()
    signal toggleRequested()

    readonly property bool validKind: ["media", "tasks", "notes", "calendar", "weather",
        "notifications", "focus", "mixer", "system"].includes(root.kind)
    property bool initialized: false

    function diagnosticSnapshot(): var {
        const ownStatus = root.status === Loader.Ready ? "ready"
            : root.status === Loader.Loading ? "loading"
            : root.status === Loader.Error ? "error" : "null"
        const nested = root.item?.diagnosticSnapshot?.()
        const loaders = [{ status: ownStatus, source: root.source.toString(), itemPresent: root.item !== null }]
        if (nested && Array.isArray(nested.loaders)) loaders.push(...nested.loaders)
        return { loaders: loaders }
    }

    function updateSource(): void {
        if (!root.initialized) return
        if (!root.validKind) {
            root.source = ""
        } else if (root.source.toString().length === 0) {
            root.setSource(Qt.resolvedUrl("IrisSidebarSectionContent.qml"), {
                width: Qt.binding(() => root.width),
                kind: Qt.binding(() => root.kind),
                contentActive: Qt.binding(() => root.contentActive),
                expanded: Qt.binding(() => root.expanded),
                bare: Qt.binding(() => root.bare)
            })
        }
    }

    onValidKindChanged: root.updateSource()
    Component.onCompleted: {
        root.initialized = true
        root.updateSource()
    }

    Connections {
        target: root.item
        function onNavigate(): void { root.navigate() }
        function onToggleRequested(): void { root.toggleRequested() }
    }
}
