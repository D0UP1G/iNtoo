pragma ComponentBehavior: Bound

import QtQuick
import Quickshell
import Quickshell.Io
import qs
import qs.services

Scope {
    id: root
    IpcHandler {
        target: "assistant"
        function toggle(): void {
            MascotAssistant.opened = !MascotAssistant.opened;
        }
        function open(): void {
            MascotAssistant.opened = true;
        }
        function close(): void {
            MascotAssistant.opened = false;
        }
        function current(): string {
            return MascotAssistant.opened ? "open" : "closed";
        }
    }
    LazyLoader {
        active: MascotAssistant.opened && !GlobalStates.screenLocked
        source: "MascotAssistantWindow.qml"
    }
    Connections {
        target: GlobalStates
        function onScreenLockedChanged(): void {
            if (GlobalStates.screenLocked)
                MascotAssistant.opened = false;
        }
    }
}
