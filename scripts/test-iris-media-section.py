#!/usr/bin/env python3
"""Exercise the production deferred media host with a native Qt card fixture.

The real host, URL boundary, animation and toggle handlers are used. Actual
MPRIS playback and the styled IrisMediaCard implementation are not exercised.
"""

import importlib.util
import os
from pathlib import Path
import re
import shutil
import subprocess
import sys
import tempfile

ROOT = Path(__file__).resolve().parent.parent
SIDEBAR = ROOT / "modules/iris/sidebar"
spec = importlib.util.spec_from_file_location(
    "sidebar_loader", ROOT / "scripts/test-iris-sidebarsection-loader.py")
helper = importlib.util.module_from_spec(spec)
spec.loader.exec_module(helper)


def isolated(source):
    return re.sub(r"^import (?:qs(?:\.|\s)|Quickshell).*\n", "", source,
                  flags=re.MULTILINE).replace("import QtQuick\n", 'import QtQuick\nimport "Fixture"\n', 1)


def write_harness(directory):
    content = (SIDEBAR / "IrisSidebarSectionContent.qml").read_text()
    component = helper.block(content, r"Component\s*\{\s*id: mediaComponent\b")
    if "IrisMediaCard {" in component or 'setSource(Qt.resolvedUrl("IrisSidebarMediaSection.qml")' not in component:
        raise ValueError("media must resolve its external host only when selected")
    if "sectionHost: root" not in component:
        raise ValueError("media host must be supplied at creation")
    media = (SIDEBAR / "IrisSidebarMediaSection.qml").read_text()
    if re.search(r"\broot\b", media):
        raise ValueError("external media host retains an out-of-scope root")
    fixture = directory / "Fixture"
    fixture.mkdir()
    (fixture / "qmldir").write_text("module Fixture\nsingleton IrisStyle 1.0 IrisStyle.qml\nsingleton Translation 1.0 Translation.qml\n")
    (fixture / "IrisStyle.qml").write_text('''pragma Singleton
import QtQuick
QtObject {
    property real density: 1
    property int morphDuration: 120
    property var morphCurve: [0.4, 0, 0.2, 1, 1, 1]
}
''')
    (fixture / "Translation.qml").write_text('pragma Singleton\nimport QtQuick\nQtObject { function tr(value) { return value } }\n')
    (directory / "state.js").write_text(".pragma library\nvar created = 0\nvar destroyed = 0\n")
    (directory / "IrisMediaCard.qml").write_text('''import QtQuick
import "state.js" as State
Item {
    property bool active: true
    property bool compact: false
    property bool showBackground: false
    property int localState: 0
    property var hostOnCompleted: null
    property bool activeOnCompleted: false
    property bool compactOnCompleted: false
    implicitHeight: compact ? 80 : 260
    Component.onCompleted: {
        ++State.created
        hostOnCompleted = parent.sectionHost
        activeOnCompleted = active
        compactOnCompleted = compact
    }
    Component.onDestruction: ++State.destroyed
}
''')
    (directory / "IrisIconButton.qml").write_text('''import QtQuick
import QtQuick.Controls
Button { property string materialIcon: ""; property real iconSize: 18; implicitWidth: 32; implicitHeight: 32 }
''')
    media = media.replace("id: mediaCard\n", 'id: mediaCard\n        objectName: "mediaCard"\n', 1)
    (directory / "IrisSidebarMediaSection.qml").write_text(isolated(media))
    api = content[content.index("    required property string kind"):content.index("    sourceComponent:")]
    selection = helper.block(content, r"sourceComponent:\s*\{")
    ids = re.findall(r'case "[^"]+": return (\w+)', selection)
    others = "\n".join("Component { id: " + identifier + "; Item { implicitHeight: 20 } }"
                       for identifier in ids if identifier != "mediaComponent")
    payload = ('pragma ComponentBehavior: Bound\nimport QtQuick\nimport "Fixture"\nLoader {\nid: root\n'
               + api + selection + component + others + "\n}\n")
    (directory / "IrisSidebarSectionContent.qml").write_text(payload)
    (directory / "IrisSidebarSection.qml").write_text(isolated((SIDEBAR / "IrisSidebarSection.qml").read_text()))
    deferred = directory / "Deferred"
    deferred.mkdir()
    (deferred / "Fixture").symlink_to(fixture, target_is_directory=True)
    (deferred / "IrisSidebarSectionContent.qml").write_text(payload)
    (deferred / "IrisSidebarMediaSection.qml").write_text("import QtQuick\nItem { impossible syntax !!! }\n")
    (directory / "tst_media.qml").write_text('''pragma ComponentBehavior: Bound
import QtQuick
import QtTest
import "Fixture"
import "Deferred" as Deferred
import "state.js" as State

TestCase {
    id: test
    name: "IrisSidebarMediaSection"
    visible: true
    width: 700; height: 900
    when: windowShown
    Component { id: wrapper; IrisSidebarSection {} }
    Component { id: boundary; Deferred.IrisSidebarSectionContent {} }
    function create(expanded = false, active = true) {
        const value = createTemporaryObject(wrapper, test, {
            kind: "media", width: 420, expanded: expanded, contentActive: active
        })
        verify(value !== null)
        compare(value.status, Loader.Ready)
        return value
    }
    function host(value) {
        compare(value.item.status, Loader.Ready)
        compare(value.item.item.status, Loader.Ready)
        verify(value.item.item.item !== null)
        return value.item.item.item
    }
    function card(value) { return findChild(host(value), "mediaCard") }
    function action(value, type) {
        const found = host(value).children.find(child => type === "compact"
            ? child.materialIcon === "unfold_less" : child.cursorShape === Qt.PointingHandCursor)
        verify(found !== undefined)
        return found
    }
    function init() { IrisStyle.density = 1 }
    function test_compile_boundary() {
        const value = createTemporaryObject(boundary, test, {kind: "tasks"})
        verify(value !== null)
        compare(value.status, Loader.Ready)
        compare(value.item.implicitHeight, 20)
        ignoreWarning(/.*IrisSidebarMediaSection.qml.*Expected token.*/)
        value.kind = "media"
        compare(value.item.status, Loader.Error)
        compare(value.item.item, null)
    }
    function test_initial_properties_and_geometry() {
        const value = create(false, false)
        const media = host(value)
        const player = card(value)
        compare(media.sectionHost, value.item)
        compare(player.hostOnCompleted, value.item)
        compare(player.activeOnCompleted, false)
        compare(player.compactOnCompleted, true)
        compare(player.showBackground, true)
        compare(media.clip, true)
        compare(media.width, 420)
        compare(player.width, 420)
        compare(value.implicitHeight, 80)
        value.width = 550
        compare(media.width, 550)
        compare(player.width, 550)
        compare(action(value, "full").width, 550 * 0.55)
        value.expanded = true
        tryCompare(value, "implicitHeight", 260)
        compare(player.compact, false)
        value.expanded = false
        tryCompare(value, "implicitHeight", 80)
    }
    function test_activity_expansion_preserve_instance_state() {
        const value = create()
        const media = host(value)
        const player = card(value)
        const created = State.created
        const destroyed = State.destroyed
        player.localState = 42
        value.contentActive = false
        compare(player.active, false)
        compare(host(value), media)
        compare(card(value), player)
        value.expanded = true
        compare(player.compact, false)
        value.contentActive = true
        compare(player.active, true)
        value.expanded = false
        compare(player.compact, true)
        compare(player.localState, 42)
        compare(State.created, created)
        compare(State.destroyed, destroyed)
    }
    function test_real_toggle_handlers_and_density() {
        const value = create()
        let toggles = 0
        value.toggleRequested.connect(() => ++toggles)
        const full = action(value, "full")
        const compact = action(value, "compact")
        compare(full.visible, true)
        compare(compact.visible, false)
        compare(full.Accessible.name, "Full player")
        full.clicked(null)
        compare(toggles, 1)
        value.expanded = true
        compare(full.visible, false)
        compare(compact.visible, true)
        compare(compact.Accessible.name, "Compact player")
        compact.clicked()
        compare(toggles, 2)
        compare(compact.iconSize, 16)
        compare(compact.anchors.margins, 8)
        IrisStyle.density = 1.5
        compare(value.d, 1.5)
        compare(value.item.d, 1.5)
        compare(compact.iconSize, 24)
        compare(compact.anchors.margins, 12)
    }
    function test_height_animation_and_morphing() {
        const value = create()
        tryCompare(host(value), "animate", true)
        wait(150)
        compare(value.morphing, false)
        value.expanded = true
        tryCompare(value, "morphing", true)
        verify(value.implicitHeight < 260)
        tryCompare(value, "implicitHeight", 260)
        tryCompare(value, "morphing", false)
        value.expanded = false
        tryCompare(value, "morphing", true)
        tryCompare(value, "implicitHeight", 80)
        tryCompare(value, "morphing", false)
    }
    function test_kind_destruction_and_reopen() {
        const value = create()
        const created = State.created
        const destroyed = State.destroyed
        card(value).localState = 42
        value.kind = "tasks"
        wait(0)
        compare(State.destroyed, destroyed + 1)
        compare(value.item.item.implicitHeight, 20)
        value.kind = "media"
        compare(State.created, created + 1)
        compare(card(value).localState, 0)
        value.kind = ""
        wait(0)
        compare(State.destroyed, destroyed + 2)
        compare(value.item, null)
        value.kind = "media"
        compare(State.created, created + 2)
        compare(card(value).localState, 0)
    }
}
''')


def main():
    runners = [os.environ.get("QMLTESTRUNNER", ""),
               "/usr/lib/qt6/bin/qmltestrunner", "/usr/lib64/qt6/bin/qmltestrunner",
               "/usr/lib/x86_64-linux-gnu/qt6/bin/qmltestrunner",
               shutil.which("qmltestrunner") or ""]
    with tempfile.TemporaryDirectory(prefix="inir-media-section-") as folder:
        directory = Path(folder)
        write_harness(directory)
        environment = dict(os.environ, QT_QPA_PLATFORM="offscreen", QML_DISABLE_DISK_CACHE="1")
        probe = directory / "probe.qml"
        probe.write_text('import QtQuick\nimport QtTest\nTestCase { name: "Probe"; function test_ok() { verify(true) } }\n')
        for runner in dict.fromkeys(runners):
            if not runner or not Path(runner).is_file():
                continue
            version = subprocess.run([runner, "-input", str(probe)], env=environment,
                                     capture_output=True, text=True, timeout=30)
            if "Using QtTest library 6" not in version.stdout + version.stderr:
                continue
            result = subprocess.run([runner, "-input", str(directory / "tst_media.qml")],
                                    env=environment, capture_output=True, text=True, timeout=30)
            output = result.stdout + result.stderr
            print(output, end="")
            return 1 if result.returncode or "FAIL!" in output else 0
    print("Iris media section: no Qt 6 qmltestrunner found; native checks not executed")
    return 0


if __name__ == "__main__":
    sys.exit(main())
