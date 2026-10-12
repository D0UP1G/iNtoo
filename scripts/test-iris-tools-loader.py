#!/usr/bin/env python3
"""Exercise the production tools Loader in native Qt 6 with a tiny child."""

import os
from pathlib import Path
import re
import shutil
import subprocess
import sys
import tempfile

ROOT = Path(__file__).resolve().parent.parent
ISLAND = ROOT / "modules/iris/bar/IrisIsland.qml"


def loader_declaration():
    source = ISLAND.read_text(encoding="utf-8")
    start = re.search(r"Loader\s*\{\s*id:\s*toolsLoader\b", source)
    if not start:
        raise ValueError("production toolsLoader declaration is missing")
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
                    raise ValueError("toolsLoader still compiles an inline child")
                return declaration
    raise ValueError("production toolsLoader declaration is unbalanced")


def write_harness(directory):
    (directory / "state.js").write_text(
        ".pragma library\nvar created = 0\nvar destroyed = 0\n", encoding="utf-8")
    (directory / "IrisTools.qml").write_text('''pragma ComponentBehavior: Bound
import QtQuick
import "state.js" as State

Item {
    id: root
    signal activityRequested()
    property int serial: 0
    property real widthOnCompleted: -1
    implicitHeight: 42
    Component.onCompleted: {
        root.widthOnCompleted = root.width
        root.serial = ++State.created
    }
    Component.onDestruction: ++State.destroyed
}
''', encoding="utf-8")
    (directory / "ToolsHost.qml").write_text('''pragma ComponentBehavior: Bound
import QtQuick
import QtQuick.Layouts

Item {
    id: root
    width: 320
    height: 100
    property bool resident: false
    property bool current: false
    property string page: "tools"
    property alias loader: toolsLoader
    QtObject {
        id: toolsPage
        readonly property bool resident: root.resident
        readonly property bool current: root.current
    }
    ColumnLayout {
        anchors.fill: parent
''' + loader_declaration() + '''
    }
}
''', encoding="utf-8")
    (directory / "tst_tools.qml").write_text('''pragma ComponentBehavior: Bound
import QtQuick
import QtTest
import "state.js" as State

TestCase {
    id: root
    name: "IrisToolsLoader"
    when: windowShown
    width: 640
    height: 480
    Component { id: hostComponent; ToolsHost {} }

    function test_deferred_lifecycle() {
        const created = State.created
        const destroyed = State.destroyed
        const host = createTemporaryObject(hostComponent, root)
        verify(host !== null)
        compare(host.loader.active, false)
        compare(host.loader.asynchronous, true)
        compare(host.loader.source.toString(), "")
        compare(host.loader.item, null)
        tryCompare(host.loader, "width", 320)
        wait(30)
        compare(State.created, created)

        // Prefetch while resident, before becoming the current page.
        host.resident = true
        compare(host.loader.asynchronous, true)
        tryCompare(host.loader, "status", Loader.Ready)
        verify(host.loader.source.toString().endsWith("/IrisTools.qml"))
        const child = host.loader.item
        const serial = child.serial
        compare(State.created, created + 1)
        compare(child.widthOnCompleted, 320)
        compare(child.width, 320)
        compare(host.loader.item.implicitHeight, 42)

        host.width = 420
        tryCompare(host.loader, "width", 420)
        tryCompare(child, "width", 420)
        compare(host.loader.item, child)
        compare(State.created, created + 1)
        child.activityRequested()
        compare(host.page, "activity")

        host.current = true
        compare(host.loader.asynchronous, false)
        compare(host.loader.item, child)
        host.current = false
        compare(host.loader.asynchronous, true)
        compare(host.loader.item, child)

        host.resident = false
        tryCompare(host.loader, "item", null)
        compare(host.loader.source.toString(), "")
        tryVerify(() => State.destroyed === destroyed + 1)
        host.width = 500
        tryCompare(host.loader, "width", 500)
        host.current = true
        host.resident = true
        tryCompare(host.loader, "status", Loader.Ready)
        compare(State.created, created + 2)
        verify(host.loader.item.serial !== serial)
        compare(host.loader.item.widthOnCompleted, 500)
        compare(host.loader.item.width, 500)
        host.page = "tools"
        host.loader.item.activityRequested()
        compare(host.page, "activity")

        host.destroy()
        tryVerify(() => State.destroyed === destroyed + 2)
    }

    function test_initial_resident() {
        const created = State.created
        const host = createTemporaryObject(hostComponent, root,
                                           { resident: true, current: true })
        verify(host !== null)
        tryCompare(host.loader, "status", Loader.Ready)
        compare(host.loader.active, true)
        compare(host.loader.asynchronous, false)
        compare(State.created, created + 1)
        verify(host.loader.item.widthOnCompleted >= 0)
        tryCompare(host.loader.item, "width", host.loader.width)
    }
}
''', encoding="utf-8")


def main():
    candidates = [os.environ.get("QMLTESTRUNNER", ""),
                  "/usr/lib/qt6/bin/qmltestrunner", "/usr/lib64/qt6/bin/qmltestrunner",
                  "/usr/lib/x86_64-linux-gnu/qt6/bin/qmltestrunner",
                  shutil.which("qmltestrunner") or ""]
    with tempfile.TemporaryDirectory(prefix="inir-tools-loader-") as folder:
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
    print("Island tools Loader: no Qt 6 qmltestrunner found; native lifecycle not executed")
    return 0


if __name__ == "__main__":
    sys.exit(main())
