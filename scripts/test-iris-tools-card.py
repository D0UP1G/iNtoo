#!/usr/bin/env python3
"""Run the real moved tools card and host Loader/switch with Qt service/UI stubs.

The IrisTools leaf is a stub: timer UI actions and real desktop imports are not
exercised here. Its completion emission checks initial callback installation.
"""

import os
from pathlib import Path
import re
import shutil
import subprocess
import sys
import tempfile

ROOT = Path(__file__).resolve().parent.parent
STAGE = ROOT / "modules/iris/stage"


def block(source, pattern):
    start = re.search(pattern, source)
    if not start:
        raise ValueError("production declaration is missing: " + pattern)
    tokens = re.finditer(r'//[^\n]*|/\*.*?\*/|"(?:\\.|[^"\\])*"|'
                         r"'(?:\\.|[^'\\])*'|[{}]", source[start.start():], re.S)
    depth = 0
    for token in tokens:
        if token.group() == "{":
            depth += 1
        elif token.group() == "}":
            depth -= 1
            if depth == 0:
                return source[start.start():start.start() + token.end()]
    raise ValueError("production declaration is unbalanced: " + pattern)


def write_harness(directory):
    content = (STAGE / "IrisCardContent.qml").read_text(encoding="utf-8")
    payload = (STAGE / "IrisToolsCard.qml").read_text(encoding="utf-8")
    loader = block(content, r"Loader\s*\{\s*id:\s*body\b")
    wrapper = block(content, r"Component\s*\{\s*id:\s*toolsCard\b")
    switch = block(loader, r"sourceComponent:\s*\{")
    ids = set(re.findall(r'case "[^"]+": return (\w+)', switch))
    if 'case "tools": return toolsCard' not in switch:
        raise ValueError("tools must retain its card map entry")
    if 'setSource(Qt.resolvedUrl("IrisToolsCard.qml"), {' not in wrapper:
        raise ValueError("tools wrapper must use the external URL and initial callback")
    if "sourceComponent:" in wrapper or "ColumnLayout" in wrapper or "asynchronous: true" in wrapper:
        raise ValueError("tools payload must leave the common card file and load synchronously")
    if re.search(r"\bIrisTools\s*\{", content):
        raise ValueError("tools UI must leave the common card file")
    if "required property var activityRequestedCallback" not in payload:
        raise ValueError("callback must be available before the leaf is created")
    fixture = directory / "Fixture"
    fixture.mkdir()
    singletons = {
        "IrisStyle": '''QtObject {
    property real density: 1.25
    property var identity: ({ orange: "#ee9900" })
}''',
        "TimerService": '''QtObject {
    property bool countdownRunning: false
    property bool pomodoroRunning: false
    property bool stopwatchRunning: false
    property int created: 0
    property int destroyed: 0
    property bool emitOnCompletion: false
}''',
        "Translation": '''QtObject { function tr(value: string): string { return value } }''',
        "GlobalStates": '''QtObject { property string irisIslandPageRequest: "none" }''',
    }
    (fixture / "qmldir").write_text("module Fixture\n" + "".join(
        "singleton " + name + " 1.0 " + name + ".qml\n" for name in singletons), encoding="utf-8")
    for name, body in singletons.items():
        (fixture / (name + ".qml")).write_text(
            "pragma Singleton\nimport QtQuick\n" + body + "\n", encoding="utf-8")
    (directory / "CardHeader.qml").write_text('''import QtQuick
Item {
    objectName: "header"
    property string glyph: ""
    property string title: ""
    property string detail: ""
    property color tint: "transparent"
    implicitHeight: 30
    implicitWidth: 80
}
''', encoding="utf-8")
    (directory / "IrisTools.qml").write_text('''import QtQuick
import "Fixture"
Item {
    objectName: "tools"
    signal activityRequested()
    property int localSelection: 0
    implicitHeight: 120
    implicitWidth: 200
    Component.onCompleted: {
        ++TimerService.created
        if (TimerService.emitOnCompletion) activityRequested()
    }
    Component.onDestruction: ++TimerService.destroyed
}
''', encoding="utf-8")
    # Keep the complete moved layout, replacing only its module imports.
    payload = re.sub(r"^import qs[^\n]*\n", "", payload, flags=re.M)
    payload = payload.replace("import QtQuick.Layouts\n", 'import QtQuick.Layouts\nimport "Fixture"\n')
    (directory / "IrisToolsCard.qml").write_text(payload, encoding="utf-8")
    height = re.search(r"^    readonly property real contentHeight:.*$", content, re.M).group()
    implicit_height = re.search(r"^    implicitHeight: root\.contentHeight$", content, re.M).group()
    close = re.search(r"^    function close\(\): void .*?$", content, re.M).group()
    others = "\n".join("Component { id: " + name + "; Item { implicitHeight: 31 } }"
                       for name in sorted(ids - {"toolsCard"}))
    (directory / "CardHost.qml").write_text('''pragma ComponentBehavior: Bound
import QtQuick
import "Fixture"
Item {
    id: root
    width: 360
    property string kind: ""
    property bool contentActive: true
    property alias body: body
    property int navigations: 0
    property string pageAtNavigate: ""
    signal navigate()
    onNavigate: {
        ++navigations
        pageAtNavigate = GlobalStates.irisIslandPageRequest
    }
''' + height + "\n" + implicit_height + "\n" + close + "\n" + loader
        + "\n" + wrapper + "\n" + others + "\n}\n", encoding="utf-8")
    (directory / "tst_toolscard.qml").write_text('''pragma ComponentBehavior: Bound
import QtQuick
import QtTest
import "Fixture"
TestCase {
    id: root
    name: "IrisToolsCard"
    when: windowShown
    visible: true
    width: 640
    height: 600
    Component { id: hostComponent; CardHost {} }

    function init() {
        IrisStyle.density = 1.25
        TimerService.countdownRunning = false
        TimerService.pomodoroRunning = false
        TimerService.stopwatchRunning = false
        TimerService.emitOnCompletion = false
        GlobalStates.irisIslandPageRequest = "none"
    }

    function ready(host) {
        compare(host.body.status, Loader.Ready)
        compare(host.body.asynchronous, false)
        const wrapper = host.body.item
        compare(wrapper.status, Loader.Ready)
        compare(wrapper.asynchronous, false)
        verify(wrapper.source.toString().endsWith("/IrisToolsCard.qml"))
        const card = wrapper.item
        verify(card !== null)
        compare(card.width, host.width)
        compare(wrapper.implicitHeight, card.implicitHeight)
        compare(host.contentHeight, card.implicitHeight)
        compare(host.implicitHeight, card.implicitHeight)
        return card
    }

    function test_lazy_state_timer_header_and_reopen() {
        const created = TimerService.created
        const destroyed = TimerService.destroyed
        const host = createTemporaryObject(hostComponent, root, { kind: "network" })
        verify(host !== null)
        compare(host.contentHeight, 31)
        compare(TimerService.created, created)
        host.kind = "tools"
        const first = ready(host)
        const header = findChild(first, "header")
        const tools = findChild(first, "tools")
        verify(header !== null)
        verify(tools !== null)
        compare(TimerService.created, created + 1)
        compare(header.glyph, "timer")
        compare(header.title, "Timers")
        compare(header.tint, IrisStyle.identity.orange)
        compare(header.detail, "Tap a dial to start, scroll to adjust")
        tryCompare(first, "implicitHeight", 150 + 14 * IrisStyle.density)
        IrisStyle.density = 1.5
        compare(first.d, 1.5)
        compare(first.spacing, 21)
        tryCompare(host, "contentHeight", 171)
        host.width = 420
        tryCompare(tools, "width", 420)
        tools.localSelection = 4
        for (const name of ["countdownRunning", "pomodoroRunning", "stopwatchRunning"]) {
            TimerService[name] = true
            compare(header.detail, "Running")
            TimerService[name] = false
            compare(header.detail, "Tap a dial to start, scroll to adjust")
        }
        TimerService.stopwatchRunning = true
        host.contentActive = false
        compare(host.body.item.item, first)
        compare(tools.localSelection, 4)
        compare(TimerService.created, created + 1)
        compare(TimerService.destroyed, destroyed)
        compare(header.detail, "Running")
        host.kind = "network"
        compare(host.contentHeight, 31)
        tryCompare(TimerService, "destroyed", destroyed + 1)
        compare(TimerService.stopwatchRunning, true)
        host.kind = "tools"
        const second = ready(host)
        verify(second !== first)
        compare(findChild(second, "tools").localSelection, 0)
        compare(findChild(second, "header").detail, "Running")
        compare(TimerService.created, created + 2)
        host.kind = ""
        compare(host.body.item, null)
        compare(host.contentHeight, 0)
        tryCompare(TimerService, "destroyed", destroyed + 2)
        compare(TimerService.stopwatchRunning, true)
    }

    function test_activity_forwarding_order_once_per_event() {
        const host = createTemporaryObject(hostComponent, root, { kind: "tools" })
        const tools = findChild(ready(host), "tools")
        compare(host.navigations, 0)
        for (let index = 1; index <= 3; ++index) {
            GlobalStates.irisIslandPageRequest = "before-" + index
            tools.activityRequested()
            compare(host.navigations, index)
            compare(host.pageAtNavigate, "before-" + index)
            compare(GlobalStates.irisIslandPageRequest, "activity")
        }
        host.kind = "network"
        host.kind = "tools"
        findChild(ready(host), "tools").activityRequested()
        compare(host.navigations, 4)
    }

    function test_callback_installed_before_leaf_completion() {
        TimerService.emitOnCompletion = true
        const host = createTemporaryObject(hostComponent, root, { kind: "tools" })
        ready(host)
        compare(host.navigations, 1)
        compare(host.pageAtNavigate, "none")
        compare(GlobalStates.irisIslandPageRequest, "activity")
        host.kind = "network"
        GlobalStates.irisIslandPageRequest = "reopening"
        host.kind = "tools"
        ready(host)
        compare(host.navigations, 2)
        compare(host.pageAtNavigate, "reopening")
        compare(GlobalStates.irisIslandPageRequest, "activity")
    }

    function test_parent_destruction_destroys_leaf_once() {
        const destroyed = TimerService.destroyed
        const host = createTemporaryObject(hostComponent, root, { kind: "tools" })
        ready(host)
        host.destroy()
        tryCompare(TimerService, "destroyed", destroyed + 1)
        wait(10)
        compare(TimerService.destroyed, destroyed + 1)
    }
}
''', encoding="utf-8")


def main():
    candidates = [os.environ.get("QMLTESTRUNNER", ""),
                  "/usr/lib/qt6/bin/qmltestrunner", "/usr/lib64/qt6/bin/qmltestrunner",
                  "/usr/lib/x86_64-linux-gnu/qt6/bin/qmltestrunner",
                  shutil.which("qmltestrunner") or ""]
    with tempfile.TemporaryDirectory(prefix="inir-tools-card-") as folder:
        directory = Path(folder)
        write_harness(directory)
        environment = dict(os.environ, QT_QPA_PLATFORM="offscreen", QML_DISABLE_DISK_CACHE="1")
        probe = directory / "probe.qml"
        probe.write_text('import QtQuick\nimport QtTest\n'
                         'TestCase { name: "Probe"; function test_ok() { verify(true) } }\n', encoding="utf-8")
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
    print("Iris tools card: no Qt 6 qmltestrunner found; native test not executed")
    return 0


if __name__ == "__main__":
    sys.exit(main())
