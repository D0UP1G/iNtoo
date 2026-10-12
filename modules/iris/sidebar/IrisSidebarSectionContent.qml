pragma ComponentBehavior: Bound

import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import QtQuick.Shapes
import Quickshell
import qs
import qs.services
import qs.modules.common
import qs.modules.common.functions
import qs.modules.common.widgets
import qs.modules.iris.components
import qs.modules.iris.style

Loader {
    id: root
    required property string kind
    property bool contentActive: true
    property bool expanded: false
    property bool bare: false
    property bool morphing: false
    readonly property real d: IrisStyle.density
    signal navigate()
    signal toggleRequested()

    function diagnosticSnapshot(): var {
        const loaders = []
        let current = root
        for (let depth = 0; current instanceof Loader && depth < 8; depth++) {
            const status = current.status === Loader.Ready ? "ready"
                : current.status === Loader.Loading ? "loading"
                : current.status === Loader.Error ? "error" : "null"
            loaders.push({ status: status, source: current.source.toString(), itemPresent: current.item !== null })
            const item = current.item
            if (!item) break
            if (typeof item.diagnosticSnapshot === "function") {
                const nested = item.diagnosticSnapshot()
                if (nested && Array.isArray(nested.loaders)) loaders.push(...nested.loaders)
                break
            }
            if (!(item instanceof Loader)) break
            current = item
        }
        return { loaders: loaders }
    }
    sourceComponent: {
        switch (root.kind) {
        case "media": return mediaComponent
        case "tasks": return tasksComponent
        case "notes": return notesComponent
        case "calendar": return calendarComponent
        case "weather": return weatherComponent
        case "notifications": return notificationsComponent
        case "focus": return focusComponent
        case "mixer": return mixerComponent
        case "system": return systemComponent
        default: return null
        }
    }

    component EmptyLabel: IrisText {
        Layout.fillWidth: true
        color: IrisStyle.muted
        wrapMode: Text.WordWrap
        font.pixelSize: IrisStyle.typeMeta
    }

    component Section: IrisSidebarSectionFrame {
        sectionHost: root
    }

        Component {
        id: mediaComponent
        Loader {
            // Resolve only when selected; retain player state while collapsed.
            Component.onCompleted: setSource(Qt.resolvedUrl("IrisSidebarMediaSection.qml"), {
                sectionHost: root
            })
        }
    }

    Component {
        id: tasksComponent
        Loader {
            // Resolve only when selected; keep tasks resident while collapsed.
            Component.onCompleted: setSource(Qt.resolvedUrl("IrisSidebarTasksSection.qml"), {
                sectionHost: root
            })
        }
    }

    Component {
        id: notesComponent
        Loader {
            // Resolve only when selected; keep notes resident while collapsed.
            Component.onCompleted: setSource(Qt.resolvedUrl("IrisSidebarNotesSection.qml"), {
                sectionHost: root
            })
        }
    }

    Component {
        id: calendarComponent
        Loader {
            // Resolve only when selected; keep the calendar resident while collapsed.
            Component.onCompleted: setSource(Qt.resolvedUrl("IrisSidebarCalendarSection.qml"), {
                sectionHost: root
            })
        }
    }

    Component {
        id: weatherComponent
        Loader {
            // Resolve only when selected; retain weather content while collapsed.
            Component.onCompleted: setSource(Qt.resolvedUrl("IrisSidebarWeatherSection.qml"), {
                sectionHost: root
            })
        }
    }

    Component {
        id: notificationsComponent
        Loader {
            // Resolve only when selected; retain unfolded apps while collapsed.
            Component.onCompleted: setSource(Qt.resolvedUrl("IrisSidebarNotificationsSection.qml"), {
                sectionHost: root
            })
        }
    }

    Component {
        id: focusComponent
        Loader {
            // Resolve only when selected; service state survives collapse.
            Component.onCompleted: setSource(Qt.resolvedUrl("IrisSidebarFocusSection.qml"), {
                sectionHost: root
            })
        }
    }

    Component {
        id: mixerComponent
        Loader {
            // Resolve only when selected; retain streams while collapsed.
            Component.onCompleted: setSource(Qt.resolvedUrl("IrisSidebarMixerSection.qml"), {
                sectionHost: root
            })
        }
    }

    Component {
        id: systemComponent
        Loader {
            // Resolve only when selected; polling follows the host's contentActive.
            Component.onCompleted: setSource(Qt.resolvedUrl("IrisSidebarSystemSection.qml"), {
                sectionHost: root
            })
        }
    }
}
