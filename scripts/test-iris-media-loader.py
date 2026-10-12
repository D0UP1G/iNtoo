#!/usr/bin/env python3
"""Exercise the production media Loader in native Qt 6 with a tiny child.

This checks Loader lifetime and cover registration, not MPRIS or media UI.
"""

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
    page = re.search(r'Page\s*\{\s*id:\s*mediaPage\b(.*?)Loader\s*\{\s*id:\s*mediaLoader\b',
                     source, re.S)
    start = re.search(r"Loader\s*\{\s*id:\s*mediaLoader\b", source)
    if not page or not start:
        raise ValueError("production mediaPage/mediaLoader declaration is missing")
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
                    raise ValueError("mediaLoader still compiles an inline child")
                return page.group(1), declaration
    raise ValueError("production mediaLoader declaration is unbalanced")


def write_harness(directory):
    page_properties, loader = production_declarations()
    island = directory / "island"
    island.mkdir()
    (directory / "state.js").write_text(
        ".pragma library\nvar created = 0\nvar destroyed = 0\n", encoding="utf-8")
    (island / "IslandMediaPage.qml").write_text('''pragma ComponentBehavior: Bound
import QtQuick
import "../state.js" as State

Item {
    id: page
    required property Item island
    property Item islandOnCompleted
    property real widthOnCompleted: -1
    property int serial: 0
    readonly property Item pageCoverItem: pageCover
    implicitHeight: page.island.childHeight
    Item { id: pageCover }
    Component.onCompleted: {
        page.islandOnCompleted = page.island
        page.widthOnCompleted = page.width
        page.serial = ++State.created
        page.island.pageCover = pageCover
    }
    Component.onDestruction: ++State.destroyed
}
''', encoding="utf-8")
    (directory / "MediaHost.qml").write_text('''pragma ComponentBehavior: Bound
import QtQuick
import QtQuick.Layouts

Item {
    id: root
    width: 320
    height: 200
    property bool resident: false
    property bool current: false
    property real childHeight: 42
    property real d: 1
    property Item pageCover: null
    property alias loader: mediaLoader
    property alias pageItem: mediaPage
    ColumnLayout {
        anchors.fill: parent
        ColumnLayout {
            id: mediaPage
            Layout.fillWidth: true
            Layout.alignment: Qt.AlignTop
            property string name
            readonly property bool resident: root.resident
            readonly property bool current: root.current
''' + page_properties + loader + '''
        }
    }
}
''', encoding="utf-8")
    (directory / "tst_media.qml").write_text('''pragma ComponentBehavior: Bound
import QtQuick
import QtQuick.Layouts
import QtTest
import "state.js" as State

TestCase {
    id: root
    name: "IrisMediaLoader"
    when: windowShown
    visible: true
    width: 640
    height: 480
    Component { id: hostComponent; MediaHost {} }

    function checkInitialProperties(host) {
        const child = host.loader.item
        verify(child !== null)
        compare(child.islandOnCompleted, host)
        compare(child.island, host)
        verify(child.widthOnCompleted >= 0)
        tryCompare(child, "width", host.loader.width)
        verify(child.pageCoverItem !== null)
        compare(host.pageCover, child.pageCoverItem)
        compare(host.pageCover.parent, child)
    }

    function test_cancel_before_ready() {
        const created = State.created
        const host = createTemporaryObject(hostComponent, root)
        verify(host !== null)
        host.resident = true
        compare(host.loader.asynchronous, true)
        verify(host.loader.source.toString().endsWith("/island/IslandMediaPage.qml"))
        if (host.loader.status !== Loader.Loading) {
            host.destroy()
            skip("Qt completed the tiny child before Loading cancellation could be observed")
            return
        }
        host.resident = false
        compare(host.loader.item, null)
        compare(host.loader.status, Loader.Null)
        compare(host.loader.source.toString(), "")
        wait(50)
        compare(State.created, created)
        compare(host.loader.item, null)
        compare(host.pageCover, null)
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
        compare(host.loader.status, Loader.Null)
        compare(host.loader.item, null)
        compare(host.loader.Layout.fillWidth, true)
        compare(host.loader.Layout.preferredHeight, 0)
        compare(host.pageCover, null)
        compare(host.pageItem.spacing, 14)
        tryCompare(host.loader, "width", 320)
        wait(30)
        compare(State.created, created)

        // Prefetch while resident, before becoming the current page.
        host.resident = true
        compare(host.loader.active, true)
        compare(host.loader.asynchronous, true)
        tryCompare(host.loader, "status", Loader.Ready)
        verify(host.loader.source.toString().endsWith("/island/IslandMediaPage.qml"))
        const child = host.loader.item
        const serial = child.serial
        compare(State.created, created + 1)
        checkInitialProperties(host)
        compare(child.widthOnCompleted, 320)
        compare(host.loader.Layout.preferredHeight, 42)
        tryCompare(host.pageItem, "implicitHeight", 42)

        host.width = 420
        tryCompare(host.loader, "width", 420)
        tryCompare(child, "width", 420)
        compare(host.loader.item, child)
        compare(child.serial, serial)
        compare(State.created, created + 1)

        host.childHeight = 96
        tryCompare(child, "implicitHeight", 96)
        tryCompare(host.loader.Layout, "preferredHeight", 96)
        tryCompare(host.pageItem, "implicitHeight", 96)
        host.d = 1.5
        compare(host.pageItem.spacing, 21)

        host.current = true
        compare(host.loader.asynchronous, false)
        compare(host.loader.item, child)
        compare(host.pageCover, child.pageCoverItem)
        host.current = false
        compare(host.loader.asynchronous, true)
        compare(host.loader.item, child)
        compare(host.pageCover, child.pageCoverItem)
        compare(State.created, created + 1)
        compare(State.destroyed, destroyed)

        host.resident = false
        tryCompare(host.loader, "item", null)
        compare(host.loader.active, false)
        compare(host.loader.status, Loader.Null)
        compare(host.loader.source.toString(), "")
        compare(host.loader.Layout.preferredHeight, 0)
        tryVerify(() => State.destroyed === destroyed + 1)
        tryCompare(host, "pageCover", null)

        host.width = 500
        host.current = true
        host.resident = true
        tryCompare(host.loader, "status", Loader.Ready)
        tryCompare(host.loader, "width", 500)
        compare(State.created, created + 2)
        verify(host.loader.item.serial !== serial)
        checkInitialProperties(host)
        compare(host.loader.item.width, 500)
        compare(host.loader.Layout.preferredHeight, 96)

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
                    { resident: true, current: data.current, childHeight: 73 })
        verify(host !== null)
        tryCompare(host.loader, "status", Loader.Ready)
        compare(host.loader.active, true)
        compare(host.loader.asynchronous, !data.current)
        compare(State.created, created + 1)
        checkInitialProperties(host)
        compare(host.loader.Layout.preferredHeight, 73)
    }

    function test_resident_becomes_current() {
        const created = State.created
        const host = createTemporaryObject(hostComponent, root)
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
    with tempfile.TemporaryDirectory(prefix="inir-media-loader-") as folder:
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
    print("Island media Loader: no Qt 6 qmltestrunner found; native lifecycle not executed")
    return 0


if __name__ == "__main__":
    sys.exit(main())
