#!/usr/bin/env python3
"""Exercise the real vitals payload, card switch, and URL wrapper in Qt 6.

Desktop services and visual leaves are stubs. The monitor keeps its production
sync/lifecycle, with the Quickshell window visibility guard replaced by ordinary
item visibility. This does not exercise actual machine resource polling.
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
    payload = (STAGE / "IrisVitalsCard.qml").read_text(encoding="utf-8")
    loader = block(content, r"Loader\s*\{\s*id:\s*body\b")
    wrapper = block(content, r"Component\s*\{\s*id:\s*vitalsCard\b")
    switch = block(loader, r"sourceComponent:\s*\{")
    ids = set(re.findall(r'case "[^\"]+": return (\w+)', switch))
    if 'case "vitals": return vitalsCard' not in switch:
        raise ValueError("vitals must retain the card map entry")
    if 'setSource(Qt.resolvedUrl("IrisVitalsCard.qml"), {' not in wrapper:
        raise ValueError("vitals must load the external payload URL")
    if "ColumnLayout" in wrapper or "asynchronous: true" in wrapper:
        raise ValueError("vitals must be an external synchronous payload")
    if "ResourceUsageMonitor" in content or "ResourceUsage." in content:
        raise ValueError("resource monitoring must leave the common card file")
    for required in ("property bool contentActive: true",
                     "readonly property real d: IrisStyle.density",
                     "ResourceUsageMonitor { target: vitalsBody; active: vitalsBody.contentActive }"):
        if payload.count(required) != 1:
            raise ValueError("moved body declaration must occur once: " + required)
    if re.search(r"\broot\.", payload):
        raise ValueError("moved payload retains a stale host reference")

    fixture = directory / "Fixture"
    fixture.mkdir()
    singletons = {
        "IrisStyle": '''QtObject {
    property real density: 1.25
    property var identity: ({ teal: "#338899" })
    property color danger: "#cc3333"
    property color accent: "#3388cc"
    property real typeHeadline: 20
    property int morphDuration: 0
    property var moveCurve: [0.2, 0, 0, 1, 1, 1]
}''',
        "Translation": 'QtObject { function tr(text: string): string { return text } }',
        "ResourceUsage": '''QtObject {
    property real cpuUsage: 0.25
    property real memoryUsedPercentage: 0.5
    property real tempPercentage: 0.6
    property real maxTemp: 60
    property real tempWarningThreshold: 85
    property real diskUsedPercentage: 0.7
    property int creates: 0
    property int destroys: 0
    property int keeps: 0
    property int releases: 0
    property int held: 0
    property var initialActive: []
    property var monitor: null
    function keepAlive(): void { ++keeps; ++held }
    function releaseKeepAlive(): void { ++releases; --held }
}''',
    }
    (fixture / "qmldir").write_text("module Fixture\n" + "".join(
        "singleton " + name + " 1.0 " + name + ".qml\n" for name in singletons),
        encoding="utf-8")
    for name, body in singletons.items():
        (fixture / (name + ".qml")).write_text(
            "pragma Singleton\nimport QtQuick\n" + body + "\n", encoding="utf-8")

    stubs = {
        "CardHeader": '''Item {
    property string glyph: ""
    property color tint: "white"
    property string title: ""
    property string detail: ""
    implicitHeight: 32
    implicitWidth: 120
}''',
        "ProgressRing": 'Item { property real stroke: 2; property color tint: "white"; property real progress: 0 }',
        "MaterialSymbol": 'Text { property real fill: 0; property real iconSize: 13 }',
        "IrisText": 'Text {}',
        "Metric": '''Text {
    property string value: ""
    property string unit: ""
    property real pixelSize: 20
    property int weight: Font.Bold
    text: value + unit
    font.pixelSize: pixelSize
    font.weight: weight
}''',
    }
    for name, body in stubs.items():
        (directory / (name + ".qml")).write_text("import QtQuick\n" + body + "\n", encoding="utf-8")
    monitor = (ROOT / "modules/common/widgets/ResourceUsageMonitor.qml").read_text(encoding="utf-8")
    monitor = monitor.replace("import Quickshell\n", "").replace("import qs.services", 'import "Fixture"')
    monitor = monitor.replace("active && (!target || (target.visible\n"
                              "        && (!target.QsWindow.window || target.QsWindow.window.visible)))",
                              "active && (!target || target.visible)")
    monitor = monitor.replace("Component.onCompleted: sync()", '''Component.onCompleted: {
        ++ResourceUsage.creates
        ResourceUsage.monitor = root
        ResourceUsage.initialActive = ResourceUsage.initialActive.concat([active])
        sync()
    }''')
    monitor = monitor.replace("Component.onDestruction: if (_holding) ResourceUsage.releaseKeepAlive()", '''Component.onDestruction: {
        ++ResourceUsage.destroys
        if (ResourceUsage.monitor === root) ResourceUsage.monitor = null
        if (_holding) ResourceUsage.releaseKeepAlive()
    }''')
    (directory / "ResourceUsageMonitor.qml").write_text(monitor, encoding="utf-8")
    payload = re.sub(r"^import qs.*\n", "", payload, flags=re.M)
    payload = payload.replace("import QtQuick.Layouts\n", 'import QtQuick.Layouts\nimport "Fixture"\n')
    (directory / "IrisVitalsCard.qml").write_text(payload, encoding="utf-8")
    declarations = "\n".join(re.search(pattern, content, re.M).group().strip() for pattern in (
        r"^    readonly property real contentHeight:.*$",
        r"^    implicitHeight: root\.contentHeight$"))
    others = "\n".join("Component { id: " + name + "; Item { implicitHeight: 31 } }"
                       for name in sorted(ids - {"vitalsCard"}))
    (directory / "CardHost.qml").write_text('''pragma ComponentBehavior: Bound
import QtQuick
Item {
    id: root
    width: 360
    property string kind: ""
    property bool contentActive: true
    property alias body: body
''' + declarations + "\n" + loader + "\n" + wrapper + "\n" + others + "\n}\n", encoding="utf-8")
    (directory / "tst_vitalscard.qml").write_text('''pragma ComponentBehavior: Bound
import QtQuick
import QtTest
import "Fixture"
TestCase {
    id: root
    name: "IrisVitalsCard"
    when: windowShown
    visible: true
    width: 640
    height: 600
    Component { id: hostComponent; CardHost {} }

    function ready(host) {
        compare(host.body.status, Loader.Ready)
        compare(host.body.asynchronous, false)
        const wrapper = host.body.item
        verify(wrapper !== null)
        compare(wrapper.asynchronous, false)
        compare(wrapper.status, Loader.Ready)
        verify(wrapper.source.toString().endsWith("/IrisVitalsCard.qml"))
        const payload = wrapper.item
        verify(payload !== null)
        compare(wrapper.width, host.width)
        compare(payload.width, host.width)
        compare(wrapper.implicitHeight, payload.implicitHeight)
        compare(host.contentHeight, payload.implicitHeight)
        compare(host.implicitHeight, payload.implicitHeight)
        compare(ResourceUsage.monitor.target, payload)
        compare(ResourceUsage.monitor.active, host.contentActive)
        return payload
    }

    function test_other_kinds_and_initial_inactive_then_reactive_activity() {
        const created = ResourceUsage.creates
        const destroyed = ResourceUsage.destroys
        const keeps = ResourceUsage.keeps
        const releases = ResourceUsage.releases
        const host = createTemporaryObject(hostComponent, root, { kind: "network", contentActive: false })
        verify(host !== null)
        compare(host.contentHeight, 31)
        compare(ResourceUsage.creates, created)
        wait(30)
        compare(ResourceUsage.keeps, keeps)

        host.kind = "vitals"
        const first = ready(host)
        compare(ResourceUsage.creates, created + 1)
        compare(ResourceUsage.initialActive[created], false)
        compare(ResourceUsage.keeps, keeps)
        compare(ResourceUsage.monitor.monitoring, false)
        compare(first.d, IrisStyle.density)
        tryVerify(() => host.contentHeight > 32)
        const height = host.contentHeight
        IrisStyle.density = 1.5
        compare(first.d, 1.5)
        compare(first.spacing, 15)
        tryVerify(() => host.contentHeight > height)
        host.width = 480
        compare(ready(host), first)
        host.contentActive = true
        compare(first.contentActive, true)
        compare(ResourceUsage.monitor.monitoring, true)
        compare(ResourceUsage.keeps, keeps + 1)
        compare(ResourceUsage.held, 1)
        host.contentActive = false
        compare(ResourceUsage.monitor.monitoring, false)
        compare(ResourceUsage.releases, releases + 1)
        compare(ResourceUsage.held, 0)
        compare(ready(host), first)
        compare(ResourceUsage.creates, created + 1)

        host.kind = "sound"
        compare(host.contentHeight, 31)
        tryCompare(ResourceUsage, "destroys", destroyed + 1)
        compare(ResourceUsage.releases, releases + 1)
        host.contentActive = true
        host.kind = "vitals"
        const second = ready(host)
        verify(second !== first)
        compare(ResourceUsage.initialActive[created + 1], true)
        compare(ResourceUsage.creates, created + 2)
        compare(ResourceUsage.keeps, keeps + 2)
        compare(ResourceUsage.held, 1)
        host.kind = ""
        compare(host.body.item, null)
        compare(host.contentHeight, 0)
        tryCompare(ResourceUsage, "destroys", destroyed + 2)
        compare(ResourceUsage.releases, releases + 2)
        compare(ResourceUsage.held, 0)
        host.destroy()
        wait(30)
        compare(ResourceUsage.destroys, destroyed + 2)
        IrisStyle.density = 1.25
    }

    function test_initial_active_and_parent_destruction() {
        const created = ResourceUsage.creates
        const destroyed = ResourceUsage.destroys
        const keeps = ResourceUsage.keeps
        const releases = ResourceUsage.releases
        const host = createTemporaryObject(hostComponent, root, { kind: "vitals", contentActive: true })
        ready(host)
        compare(ResourceUsage.initialActive[created], true)
        compare(ResourceUsage.keeps, keeps + 1)
        compare(ResourceUsage.held, 1)
        host.destroy()
        tryCompare(ResourceUsage, "destroys", destroyed + 1)
        compare(ResourceUsage.releases, releases + 1)
        compare(ResourceUsage.held, 0)
    }

    function test_initial_inactive_and_parent_destruction() {
        const created = ResourceUsage.creates
        const destroyed = ResourceUsage.destroys
        const keeps = ResourceUsage.keeps
        const releases = ResourceUsage.releases
        const host = createTemporaryObject(hostComponent, root, { kind: "vitals", contentActive: false })
        ready(host)
        compare(ResourceUsage.initialActive[created], false)
        compare(ResourceUsage.keeps, keeps)
        host.destroy()
        tryCompare(ResourceUsage, "destroys", destroyed + 1)
        compare(ResourceUsage.releases, releases)
        compare(ResourceUsage.held, 0)
    }
}
''', encoding="utf-8")


def main():
    candidates = [os.environ.get("QMLTESTRUNNER", ""),
                  "/usr/lib/qt6/bin/qmltestrunner", "/usr/lib64/qt6/bin/qmltestrunner",
                  "/usr/lib/x86_64-linux-gnu/qt6/bin/qmltestrunner",
                  shutil.which("qmltestrunner") or ""]
    with tempfile.TemporaryDirectory(prefix="inir-vitals-card-") as folder:
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
    print("Iris vitals card: no Qt 6 qmltestrunner found; native lifecycle not executed")
    return 0


if __name__ == "__main__":
    sys.exit(main())
