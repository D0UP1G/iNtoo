#!/usr/bin/env python3
"""Exercise the production desktop Loader in native Qt 6 with a tiny child."""

import os
from pathlib import Path
import re
import shutil
import subprocess
import sys
import tempfile

ROOT = Path(__file__).resolve().parent.parent
ISLAND = ROOT / "modules/iris/bar/IrisIsland.qml"


def production_declarations():
    source = ISLAND.read_text(encoding="utf-8")
    page = re.search(r'Page\s*\{\s*id:\s*desktopPage\b(.*?)Loader\s*\{\s*id:\s*desktopLoader\b',
                     source, re.S)
    start = re.search(r"Loader\s*\{\s*id:\s*desktopLoader\b", source)
    if not page or not start:
        raise ValueError("production desktopPage/desktopLoader declaration is missing")
    # Ignore braces inside comments and strings when extracting the real block.
    tokens = re.finditer(r'//[^\n]*|/\*.*?\*/|"(?:\\.|[^"\\])*"|'
                         r"'(?:\\.|[^'\\])*'|[{}]", source[start.start():], re.S)
    depth = 0
    for token in tokens:
        if token.group() == "{":
            depth += 1
        elif token.group() == "}":
            depth -= 1
            if depth == 0:
                declaration = source[start.start():start.start() + token.end()]
                if "sourceComponent:" in declaration:
                    raise ValueError("desktopLoader still compiles an inline child")
                return page.group(1), declaration
    raise ValueError("production desktopLoader declaration is unbalanced")


def write_harness(directory):
    page_properties, loader = production_declarations()
    island = directory / "island"
    island.mkdir()
    (directory / "state.js").write_text(
        ".pragma library\nvar created = 0\nvar destroyed = 0\n", encoding="utf-8")
    (island / "IslandDesktopPage.qml").write_text('''pragma ComponentBehavior: Bound
import QtQuick
import "../state.js" as State

Item {
    id: root
    required property Item island
    property real navOffset: 0
    readonly property bool showBanner: root.island.banner
    property Item islandOnCompleted
    property real navOffsetOnCompleted: -1
    property real widthOnCompleted: -1
    property int serial: 0
    implicitHeight: root.island.childHeight
    Component.onCompleted: {
        root.islandOnCompleted = root.island
        root.navOffsetOnCompleted = root.navOffset
        root.widthOnCompleted = root.width
        root.serial = ++State.created
    }
    Component.onDestruction: ++State.destroyed
}
''', encoding="utf-8")
    (directory / "DesktopHost.qml").write_text('''pragma ComponentBehavior: Bound
import QtQuick
import QtQuick.Layouts

Item {
    id: root
    width: 320
    height: 200
    property bool resident: false
    property bool current: false
    property bool banner: false
    property real childHeight: 42
    property real navHeight: 30
    property real rowSpacing: 14
    property real d: 1
    property alias loader: desktopLoader
    property alias pageItem: desktopPage
    Item { id: navFrame; height: root.navHeight }
    GridLayout {
        id: expandedContent
        anchors.fill: parent
        columns: 1
        rowSpacing: root.rowSpacing
        ColumnLayout {
            id: desktopPage
            Layout.fillWidth: true
            Layout.alignment: Qt.AlignTop
            property string name
            property bool bleeds
            readonly property bool resident: root.resident
            readonly property bool current: root.current
''' + page_properties + loader + '''
        }
    }
}
''', encoding="utf-8")
    (directory / "tst_desktop.qml").write_text('''pragma ComponentBehavior: Bound
import QtQuick
import QtQuick.Layouts
import QtTest
import "state.js" as State

TestCase {
    id: root
    name: "IrisDesktopLoader"
    when: windowShown
    visible: true
    width: 640
    height: 480
    Component { id: hostComponent; DesktopHost {} }

    function checkInitialProperties(host) {
        const child = host.loader.item
        verify(child !== null)
        compare(child.islandOnCompleted, host)
        compare(child.island, host)
        compare(child.navOffsetOnCompleted, host.navHeight + host.rowSpacing)
        compare(child.navOffset, host.navHeight + host.rowSpacing)
        // Layout can assign its final width after an initial resident completes.
        verify(child.widthOnCompleted >= 0)
        tryCompare(child, "width", host.loader.width)
    }

    function test_cancel_before_ready() {
        const created = State.created
        const host = createTemporaryObject(hostComponent, root)
        verify(host !== null)
        host.resident = true
        compare(host.loader.asynchronous, true)
        verify(host.loader.source.toString().endsWith("/island/IslandDesktopPage.qml"))
        if (host.loader.status !== Loader.Loading) {
            host.destroy()
            skip("Qt completed the tiny child before cancellation could be observed")
            return
        }
        host.resident = false
        compare(host.loader.item, null)
        compare(host.loader.status, Loader.Null)
        compare(host.loader.source.toString(), "")
        wait(50)
        compare(State.created, created)
        compare(host.loader.item, null)
        compare(host.pageItem.bleeds, false)
        host.navHeight = 35
        host.resident = true
        tryCompare(host.loader, "status", Loader.Ready)
        compare(State.created, created + 1)
        checkInitialProperties(host)
    }

    function test_deferred_lifecycle() {
        const created = State.created
        const destroyed = State.destroyed
        const host = createTemporaryObject(hostComponent, root)
        verify(host !== null)
        compare(host.loader.active, false)
        compare(host.loader.asynchronous, true)
        compare(host.loader.source.toString(), "")
        compare(host.loader.item, null)
        compare(host.loader.Layout.fillWidth, true)
        compare(host.loader.Layout.preferredHeight, 0)
        compare(host.pageItem.bleeds, false)
        compare(host.pageItem.spacing, 14)
        tryCompare(host.loader, "width", 320)
        wait(30)
        compare(State.created, created)

        // Prefetch while resident, before becoming the current page.
        host.resident = true
        compare(host.loader.active, true)
        compare(host.loader.asynchronous, true)
        tryCompare(host.loader, "status", Loader.Ready)
        verify(host.loader.source.toString().endsWith("/island/IslandDesktopPage.qml"))
        const child = host.loader.item
        const serial = child.serial
        compare(State.created, created + 1)
        checkInitialProperties(host)
        compare(child.widthOnCompleted, 320)
        compare(host.loader.Layout.preferredHeight, 42)
        tryCompare(host.pageItem, "implicitHeight", 42)

        host.width = 420
        host.navHeight = 50
        host.rowSpacing = 18
        tryCompare(host.loader, "width", 420)
        tryCompare(child, "width", 420)
        tryCompare(child, "navOffset", 68)
        compare(host.loader.item, child)
        compare(child.serial, serial)
        compare(State.created, created + 1)

        host.childHeight = 96
        tryCompare(child, "implicitHeight", 96)
        tryCompare(host.loader.Layout, "preferredHeight", 96)
        tryCompare(host.pageItem, "implicitHeight", 96)
        host.banner = true
        compare(child.showBanner, true)
        compare(host.pageItem.bleeds, true)
        host.banner = false
        compare(host.pageItem.bleeds, false)
        host.d = 1.5
        compare(host.pageItem.spacing, 21)

        host.current = true
        compare(host.loader.asynchronous, false)
        compare(host.loader.item, child)
        host.current = false
        compare(host.loader.asynchronous, true)
        compare(host.loader.item, child)

        host.banner = true
        host.resident = false
        tryCompare(host.loader, "item", null)
        compare(host.loader.active, false)
        compare(host.loader.source.toString(), "")
        compare(host.loader.Layout.preferredHeight, 0)
        compare(host.pageItem.bleeds, false)
        tryVerify(() => State.destroyed === destroyed + 1)
        host.width = 500
        host.navHeight = 25
        host.current = true
        host.resident = true
        tryCompare(host.loader, "status", Loader.Ready)
        tryCompare(host.loader, "width", 500)
        compare(State.created, created + 2)
        verify(host.loader.item.serial !== serial)
        checkInitialProperties(host)
        compare(host.loader.Layout.preferredHeight, 96)
        compare(host.pageItem.bleeds, true)

        host.destroy()
        tryVerify(() => State.destroyed === destroyed + 2)
    }

    function test_initial_resident_data() {
        return [{ tag: "current", current: true },
                { tag: "prefetch", current: false }]
    }

    function test_initial_resident(data) {
        const created = State.created
        const host = createTemporaryObject(hostComponent, root,
                    { resident: true, current: data.current, navHeight: 55,
                      rowSpacing: 17, banner: true })
        verify(host !== null)
        tryCompare(host.loader, "status", Loader.Ready)
        compare(host.loader.active, true)
        compare(host.loader.asynchronous, !data.current)
        compare(State.created, created + 1)
        checkInitialProperties(host)
        compare(host.pageItem.bleeds, true)
    }

    function test_resident_becomes_current() {
        const created = State.created
        const host = createTemporaryObject(hostComponent, root,
                                           { navHeight: 61, rowSpacing: 19 })
        verify(host !== null)
        host.resident = true
        compare(host.loader.asynchronous, true)
        host.current = true
        compare(host.loader.asynchronous, false)
        tryCompare(host.loader, "status", Loader.Ready)
        compare(State.created, created + 1)
        checkInitialProperties(host)
    }
}
''', encoding="utf-8")


def main():
    candidates = [os.environ.get("QMLTESTRUNNER", ""),
                  "/usr/lib/qt6/bin/qmltestrunner", "/usr/lib64/qt6/bin/qmltestrunner",
                  "/usr/lib/x86_64-linux-gnu/qt6/bin/qmltestrunner",
                  shutil.which("qmltestrunner") or ""]
    with tempfile.TemporaryDirectory(prefix="inir-desktop-loader-") as folder:
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
    print("Island desktop Loader: no Qt 6 qmltestrunner found; native lifecycle not executed")
    return 0


if __name__ == "__main__":
    sys.exit(main())
