#!/usr/bin/env python3
"""Load every deferred iSland section and card through production QML modules."""
import importlib.util
import argparse
import os
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
spec = importlib.util.spec_from_file_location("shell_startup", ROOT / "scripts/test-shell-startup.py")
shell_startup = importlib.util.module_from_spec(spec)
spec.loader.exec_module(shell_startup)

SECTIONS = ("calendar", "notifications", "notes", "tasks", "media", "focus", "mixer", "system", "weather")
CARDS = (
    "weather", "notifications", "calendar", "sound", "mic", "tools", "tray", "media", "visualizer",
    "network", "bluetooth", "vitals", "workspaces", "updates", "anime", "watching", "shellUpdate", "vpn",
)


def main(duration=6, active=False):
    test = shell_startup.ShellStartupTests()
    test.setUpClass()
    test.setUp()
    try:
        if duration > 6:
            print(f"AUDIT_HOST_PID {os.getpid()}", flush=True)
        probe = test.base / "probe/island-production-urls.qml"
        probe.write_text('''pragma ComponentBehavior: Bound
import QtQuick
import Quickshell
import qs.modules.iris.sidebar
import qs.modules.iris.stage
ShellRoot {
    Component { id: sectionFactory; IrisSidebarSection { width: 350; contentActive: ACTIVE; expanded: true } }
    Component { id: cardFactory; IrisCardContent { width: 350; contentActive: ACTIVE } }
    property var objects: []
    Timer {
        interval: 1000
        running: true
        onTriggered: {
            for (const kind of SECTIONS) {
                const object = sectionFactory.createObject(null, {kind})
                if (!object || !object.item || !object.item.item || !object.item.item.item)
                    console.warn("AUDIT_SECTION_URL_FAILED", kind)
                else console.log("AUDIT_SECTION_URL_OK", kind)
                objects.push(object)
            }
            for (const kind of CARDS) {
                const object = cardFactory.createObject(null, {kind})
                if (!object || object.contentHeight <= 0)
                    console.warn("AUDIT_CARD_URL_FAILED", kind, object?.contentHeight)
                else {
                    const snapshot = object.diagnosticSnapshot()
                    const loaders = snapshot?.loaders ?? []
                    if (!loaders.length || loaders.some(loader => loader.status !== "ready" || !loader.itemPresent))
                        console.warn("AUDIT_CARD_DIAGNOSTIC_FAILED", kind, JSON.stringify(snapshot))
                    else if (kind === "sound" && !loaders.some(loader => loader.source.includes("IrisSidebarSectionContent.qml")))
                        console.warn("AUDIT_SOUND_MIXER_DIAGNOSTIC_FAILED", JSON.stringify(snapshot))
                    else console.log("AUDIT_CARD_URL_OK", kind, object.contentHeight)
                }
                objects.push(object)
            }
            console.log("AUDIT_ALL_PAYLOAD_URLS_FINISHED")
        }
    }
}
'''.replace("SECTIONS", repr(list(SECTIONS))).replace("CARDS", repr(list(CARDS)))
           .replace("ACTIVE", "true" if active else "false")
        )
        log = test.run_qml(shell_startup.SOURCE + "/shell.qml", seconds=duration, wayland=True,
                           source_overrides=[("shell.qml", probe)])
        for line in log.splitlines():
            if "scene:" in line or "AUDIT_" in line:
                print(line)
        for kind in SECTIONS:
            test.assertIn("AUDIT_SECTION_URL_OK " + kind, log)
        for kind in CARDS:
            test.assertIn("AUDIT_CARD_URL_OK " + kind, log)
        test.assertNotRegex(
            log,
            r"(?:\w+ is not a type|Type \w+ unavailable|Cannot assign to non-existent property|"
            r"TypeError:|ReferenceError:|AUDIT_\w+_FAILED)",
        )
        print(f"PASS: {len(SECTIONS)} production sidebar URLs and {len(CARDS)} production card selections")
    finally:
        test.doCleanups()


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--duration", type=int, default=6,
                        help="keep the isolated Quickshell probe alive for this many seconds")
    parser.add_argument("--active", action="store_true",
                        help="set contentActive=true for every selected section and card")
    arguments = parser.parse_args()
    main(arguments.duration, arguments.active)
