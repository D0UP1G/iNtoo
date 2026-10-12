#!/usr/bin/env python3
"""Execute preview mappings and the Settings lookup in native Qt 6 QML.

Only the consumers' JS imports and lookup expressions are loaded; the desktop
services and the large preview scene tree are not needed for this regression.
"""

import os
from pathlib import Path
import re
import shutil
import subprocess
import sys
import tempfile

ROOT = Path(__file__).resolve().parent.parent
PREVIEW = ROOT / "modules/iris/preview/IrisGroupPreview.qml"
SETTINGS = ROOT / "modules/iris/settings/IrisSettings.qml"
HELPER = ROOT / "modules/iris/preview/groupScenes.js"

# Freeze the mappings that existed before removing the invisible sceneProbe.
EXPECTED = '''.pragma library
var sections = ({ dock: "dock", player: "player", desktop: "widgets", sidebars: "panels", controlCenter: "controlCenter", spotlight: "spotlight", bubbles: "bubbles",
    bar: "islandEdge", appearance: "light", motion: "motion", lock: "lock", frameMusic: "frame", notifications: "feedback", sound: "feedback" })
var groups = ({
    "bar/Visibility": "islandReserve", "bar/Interaction": "islandInteraction",
    "bar/Shape": "shapes", "bar/Layout": "islandEdge", "bar/Bar": "barZones",
    "appearance/Light": "light", "appearance/Shape": "shapes", "appearance/Colour theme": "glass", "appearance/Scheme": "glass", "appearance/Dark look": "glass", "appearance/Ink look": "glass", "appearance/Light look": "glass", "appearance/Corners per surface": "fusion", "appearance/Glass": "glass", "appearance/Edges": "glass",
    "appearance/Menus": "menus", "appearance/Settings": "settings",
    "appearance/Material": "glass", "appearance/Material per surface": "glass", "appearance/Look": "fusion",
    "appearance/Adaptive": "fusion", "appearance/Accent": "controlCenter", "appearance/Highlight": "controlCenter",
    "appearance/Faces": "typography", "appearance/Text": "typography", "appearance/Frame": "frame",
    "desktop/Desktop menu": "menus",
    "motion/Motion": "motion", "motion/Curve": "motion", "motion/Timing": "motion",
    "motion/Per surface": "motion", "motion/Style per surface": "motion", "motion/Touch": "motion",
    "frameMusic/On the edges": "frame", "frameMusic/Frame response": "frame", "frameMusic/Finish": "frame",
    "lock/Scene": "lock", "lock/Type": "lock", "lock/Clock": "lock", "lock/At a glance": "lock",
    "lock/Now playing": "lock", "lock/Activity": "lock", "lock/Sign in": "lock", "lock/Status": "lock",
    "bar/Desktop page": "islandPage", "bar/Pages": "islandPage", "bar/Player page": "islandPage",
    "bubbles/Behaviour": "bubbles", "bubbles/Floating": "bubbles", "bubbles/On the contour": "bubbles", "bubbles/Size": "bubbles",
    "bubbles/Cards": "cards", "bubbles/Card contents": "cards", "bubbles/Joining": "joining", "bubbles/Tray": "tray",
    "dock/Visibility": "dock", "dock/Icons": "dock", "dock/Look": "dock",
    "desktop/Widgets": "widgets", "desktop/Overview backdrop": "backdrop", "desktop/Wallpaper gallery": "gallery",
    "spotlight/Spotlight": "spotlight", "controlCenter/Control Center": "controlCenter",
    "sidebars/Panel look": "panels",
    "notifications/Banners": "feedback", "notifications/Notifications": "feedback", "sound/Feedback": "feedback",
    "player/Player": "player", "player/Bubble": "player"
})
'''


def consumer_import(path, source):
    match = re.search(r'^import "([^"]+)" as GroupScenes$', source, re.MULTILINE)
    if not match or (path.parent / match.group(1)).resolve() != HELPER:
        raise ValueError(f"{path.name} must import the shared groupScenes.js")
    return match.group(0)


def write_harness(directory):
    preview = PREVIEW.read_text(encoding="utf-8")
    settings = SETTINGS.read_text(encoding="utf-8")
    if "sceneProbe" in settings:
        raise ValueError("Settings recreated the invisible preview tree")
    method = re.search(r'function sceneFor\([^)]*\): string\s*\{[^{}]*\}', preview)
    staged = re.search(r'readonly property bool staged: ([^\n]+)',
                       settings.split("component GroupRow: Item {", 1)[1])
    if not method or not staged:
        raise ValueError("preview API or GroupRow staged expression is missing")

    # Preserve the real relative import paths in a tiny copy of the source tree.
    preview_dir = directory / "preview"
    settings_dir = directory / "settings"
    preview_dir.mkdir()
    settings_dir.mkdir()
    shutil.copyfile(HELPER, preview_dir / HELPER.name)
    (preview_dir / "PreviewLookup.qml").write_text(
        'pragma ComponentBehavior: Bound\nimport QtQuick\n'
        + consumer_import(PREVIEW, preview) + '\nQtObject {\n'
        + method.group(0) + '\n}\n', encoding="utf-8")
    (settings_dir / "SettingsLookup.qml").write_text(
        'pragma ComponentBehavior: Bound\nimport QtQuick\n'
        + consumer_import(SETTINGS, settings) + '''
Item {
    id: root
    property string section: ""
    property var modelData: ({ key: "" })
    readonly property bool staged: groupRow.staged
    QtObject {
        id: groupRow
        readonly property var modelData: root.modelData
        readonly property bool staged: ''' + staged.group(1) + '''
    }
}
''', encoding="utf-8")
    (directory / "expected.js").write_text(EXPECTED, encoding="utf-8")
    (directory / "tst_scenes.qml").write_text('''pragma ComponentBehavior: Bound
import QtQuick
import QtTest
import "expected.js" as Expected
import "preview/groupScenes.js" as GroupScenes
import "preview"
import "settings"

TestCase {
    id: root
    name: "IrisGroupScenes"
    PreviewLookup { id: preview }
    SettingsLookup { id: settings }

    function test_lookup_data() {
        const rows = []
        for (const section of Object.keys(Expected.sections))
            rows.push({ tag: section, section: section, group: "", expected: Expected.sections[section] })
        for (const key of Object.keys(Expected.groups)) {
            const parts = key.split("/")
            rows.push({ tag: key, section: parts[0], group: parts[1], expected: Expected.groups[key] })
        }
        rows.push({ tag: "empty", section: "", group: "", expected: "" })
        rows.push({ tag: "unknown section", section: "unknown", group: "", expected: "" })
        rows.push({ tag: "unknown group", section: "bar", group: "unknown", expected: "" })
        rows.push({ tag: "case sensitive", section: "bar", group: "visibility", expected: "" })
        rows.push({ tag: "unmapped section group", section: "unknown", group: "Visibility", expected: "" })
        return rows
    }

    function test_lookup(data) {
        compare(GroupScenes.sceneFor(data.section, data.group), data.expected)
        compare(preview.sceneFor(data.section, data.group), data.expected)
        settings.section = data.section
        settings.modelData = { key: data.group }
        compare(settings.staged, data.expected.length > 0)
    }

    function test_settings_updates() {
        settings.section = "bar"
        settings.modelData = { key: "Visibility" }
        compare(settings.staged, true)
        settings.modelData = { key: "unknown" }
        compare(settings.staged, false)
        settings.modelData = { key: "Feedback" }
        compare(settings.staged, false)
        settings.section = "sound"
        compare(settings.staged, true)
        settings.modelData = { key: 42 }
        compare(settings.staged, false)
    }
}
''', encoding="utf-8")


def main():
    candidates = [os.environ.get("QMLTESTRUNNER", ""),
                  "/usr/lib/qt6/bin/qmltestrunner", "/usr/lib64/qt6/bin/qmltestrunner",
                  "/usr/lib/x86_64-linux-gnu/qt6/bin/qmltestrunner",
                  shutil.which("qmltestrunner") or ""]
    with tempfile.TemporaryDirectory(prefix="inir-group-scenes-") as folder:
        directory = Path(folder)
        write_harness(directory)
        environment = dict(os.environ, QT_QPA_PLATFORM="offscreen", QML_DISABLE_DISK_CACHE="1")
        probe = directory / "probe.qml"
        probe.write_text('import QtQuick\nimport QtTest\n'
                         'TestCase { name: "Probe"; function test_ok() { verify(true) } }\n',
                         encoding="utf-8")
        for runner in dict.fromkeys(candidates):
            if not runner or not Path(runner).is_file():
                continue
            version = subprocess.run([runner, "-input", str(probe)], env=environment,
                                     capture_output=True, text=True, timeout=30)
            if "Using QtTest library 6" not in version.stdout + version.stderr:
                continue
            result = subprocess.run([runner, "-input", folder], env=environment,
                                    capture_output=True, text=True, timeout=30)
            output = result.stdout + result.stderr
            print(output, end="")
            return 1 if result.returncode != 0 or "FAIL!" in output else 0
    print("Island group scenes: no Qt 6 qmltestrunner found; native lookup not executed")
    return 0


if __name__ == "__main__":
    sys.exit(main())
