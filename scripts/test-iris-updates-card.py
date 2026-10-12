#!/usr/bin/env python3
"""Exercise the production updates card wrapper and backend routing in Qt."""
import os
from pathlib import Path
import re
import shutil
import subprocess
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


def run():
    candidates = [os.environ.get("QMLTESTRUNNER", ""), "/usr/lib/qt6/bin/qmltestrunner",
                  "/usr/lib64/qt6/bin/qmltestrunner", "/usr/lib/x86_64-linux-gnu/qt6/bin/qmltestrunner",
                  shutil.which("qmltestrunner") or ""]
    runner = next((candidate for candidate in candidates if candidate and Path(candidate).is_file()), None)
    if not runner:
        raise SystemExit("qmltestrunner not found")
    content = (STAGE / "IrisCardContent.qml").read_text()
    payload = (STAGE / "IrisUpdatesCard.qml").read_text()
    loader = block(content, r"Loader\s*\{\s*id:\s*body\b")
    switch = block(loader, r"sourceComponent:\s*\{")
    ids = set(re.findall(r'case "[^"]+": return (\w+)', switch))
    wrapper = block(content, r"Component\s*\{\s*id:\s*updatesCard\b")
    if 'Qt.resolvedUrl("IrisUpdatesCard.qml")' not in wrapper:
        raise ValueError("updates card must retain its deferred payload URL")
    if "qs.services.deferred" in content:
        raise ValueError("deferred update services must leave the shared card host")

    with tempfile.TemporaryDirectory(prefix="intoo-updates-card-") as temporary:
        directory = Path(temporary)
        fixture = directory / "Fixture"
        fixture.mkdir()
        singletons = {
            "IrisStyle": '''QtObject {
    property real density: 1.25
    property real typeScale: 1
    property color secondaryAccent: "orange"
    property var identity: ({green: "green"})
    property real radiusTile: 8
}''',
            "Translation": 'QtObject { function tr(value) { return value } }',
            "Config": '''QtObject {
    property var options: ({apps: {update: "configured command"}})
}''',
            "Updates": '''QtObject {
    property int count: 0
    property int refreshes: 0
    function refresh() { ++refreshes; TestControl.events = TestControl.events.concat(["refresh"]) }
}''',
            "PackageSearch": '''QtObject {
    property bool nativeBackend: true
    function shouldUseNativeUpdateCommand(command) {
        TestControl.events = TestControl.events.concat(["predicate:" + command])
        return nativeBackend
    }
    function updateSystem() { TestControl.events = TestControl.events.concat(["nativeUpdate"]) }
}''',
            "ShellExec": '''QtObject {
    function execCmd(command) { TestControl.events = TestControl.events.concat(["exec:" + command]) }
}''',
            "TestControl": '''QtObject {
    property var events: []
    property bool changeCommandOnClose: false
}''',
        }
        (fixture / "qmldir").write_text("module Fixture\n" + "".join(
            f"singleton {name} 1.0 {name}.qml\n" for name in singletons))
        for name, body in singletons.items():
            (fixture / f"{name}.qml").write_text("pragma Singleton\nimport QtQuick\n" + body + "\n")
        stubs = {
            "CardHeader": '''import QtQuick
import QtQuick.Layouts
RowLayout {
    property string glyph: ""
    property color tint: "transparent"
    property string title: ""
    property string detail: ""
    default property alias contents: row.data
    RowLayout { id: row }
}''',
            "IrisNumber": '''import QtQuick
Text { property real pixelSize: 12; property int weight: Font.Normal }''',
            "IrisIconButton": '''import QtQuick
import QtQuick.Controls
Button { property string materialIcon: "" }''',
            "IrisButton": '''import QtQuick
import QtQuick.Controls
Button { property real buttonRadius: 0 }''',
        }
        for name, body in stubs.items():
            (directory / f"{name}.qml").write_text(body + "\n")
        payload = re.sub(r"^import (?:qs\.|Quickshell).*$", 'import "Fixture"', payload, flags=re.M)
        payload = payload.replace("import QtQuick.Layouts\n", 'import QtQuick.Layouts\nimport "Fixture"\n')
        payload = payload.replace("CardHeader {", 'CardHeader {\n        objectName: "updatesHeader"', 1)
        payload = payload.replace("IrisNumber {", 'IrisNumber {\n        objectName: "updateCount"', 1)
        payload = payload.replace("IrisIconButton {", 'IrisIconButton {\n            objectName: "refreshButton"', 1)
        payload = payload.replace("IrisButton {", 'IrisButton {\n        objectName: "updateButton"', 1)
        (directory / "IrisUpdatesCard.qml").write_text(payload)

        declarations = "\n".join("Component { id: " + name + "; Item { implicitHeight: 28 } }"
                                   for name in sorted(ids - {"updatesCard"}))
        host = '''pragma ComponentBehavior: Bound
import QtQuick
import "Fixture"
Item {
    id: root
    width: 360
    property string kind: ""
    property bool contentActive: true
    readonly property real d: IrisStyle.density
    readonly property real contentHeight: body.item?.implicitHeight ?? 0
    property alias body: body
    signal navigate()
    function close(): void {
        TestControl.events = TestControl.events.concat(["close"])
        if (TestControl.changeCommandOnClose)
            Config.options = {apps: {update: "changed after close"}}
        root.navigate()
    }
''' + loader + "\n" + wrapper + "\n" + declarations + "\n}\n"
        (directory / "CardHost.qml").write_text(host)
        (directory / "tst_updates.qml").write_text('''pragma ComponentBehavior: Bound
import QtQuick
import QtTest
import "Fixture"
TestCase {
    id: test
    name: "IrisUpdatesCard"
    visible: true
    when: windowShown
    width: 700; height: 500
    Component { id: hostComponent; CardHost {} }
    function ready(host) {
        compare(host.body.status, Loader.Ready)
        const wrapper = host.body.item
        verify(wrapper !== null)
        compare(wrapper.status, Loader.Ready)
        compare(wrapper.source.toString().endsWith("/IrisUpdatesCard.qml"), true)
        const payload = wrapper.item
        verify(payload !== null)
        return {wrapper: wrapper, payload: payload}
    }
    function descendants(item) {
        let found = []
        for (const child of item.children) found = found.concat([child], descendants(child))
        return found
    }
    function named(item, name) {
        const value = descendants(item).find(child => child.objectName === name)
        verify(value !== undefined, "missing " + name)
        return value
    }
    function init() {
        Updates.count = 0; Updates.refreshes = 0
        PackageSearch.nativeBackend = true
        Config.options = {apps: {update: "configured command"}}
        TestControl.events = []; TestControl.changeCommandOnClose = false
    }
    function test_reactive_count_options_refresh_and_residency() {
        const host = createTemporaryObject(hostComponent, test, {kind: "updates", contentActive: false, width: 360})
        verify(host !== null)
        const loaded = ready(host)
        const header = named(loaded.payload, "updatesHeader")
        const number = named(loaded.payload, "updateCount")
        const refresh = named(loaded.payload, "refreshButton")
        const update = named(loaded.payload, "updateButton")
        compare(header.glyph, "task_alt")
        compare(header.detail, "Everything is up to date")
        compare(number.text, "0")
        compare(update.visible, false)
        compare(host.contentHeight, loaded.payload.implicitHeight)
        compare(loaded.payload.width, host.width)
        compare(TestControl.events.join(","), "")
        refresh.clicked()
        compare(Updates.refreshes, 1)
        compare(TestControl.events.join(","), "refresh")
        Updates.count = 3
        tryCompare(update, "visible", true)
        compare(header.glyph, "deployed_code_update")
        compare(header.title, "Updates")
        compare(header.detail, "3 packages waiting")
        compare(number.text, "3")
        Updates.count = 0
        tryCompare(update, "visible", false)
        compare(header.glyph, "task_alt")
        compare(header.detail, "Everything is up to date")
        host.contentActive = true
        compare(host.body.item.item, loaded.payload)
        host.width = 420
        tryCompare(loaded.payload, "width", 420)
    }
    function test_close_then_native_backend_and_config_read_order() {
        const host = createTemporaryObject(hostComponent, test, {kind: "updates"})
        const loaded = ready(host)
        Updates.count = 1
        TestControl.changeCommandOnClose = true
        named(loaded.payload, "updateButton").clicked()
        compare(TestControl.events.join(","), "close,predicate:changed after close,nativeUpdate")
        compare(host.body.item.item, loaded.payload)
    }
    function test_close_then_external_backend_and_fallback_command() {
        const host = createTemporaryObject(hostComponent, test, {kind: "updates"})
        const loaded = ready(host)
        Updates.count = 1
        PackageSearch.nativeBackend = false
        Config.options = {apps: {}}
        named(loaded.payload, "updateButton").clicked()
        compare(TestControl.events.join(","), "close,predicate:kitty -e sudo pacman -Syu,exec:kitty -e sudo pacman -Syu")
    }
    function test_kind_change_and_empty_destroy_payload() {
        const host = createTemporaryObject(hostComponent, test, {kind: "updates"})
        const first = ready(host).payload
        host.kind = "unknown"
        tryCompare(host.body, "item", null)
        host.kind = "updates"
        tryVerify(() => host.body.item?.item !== null)
        verify(host.body.item.item !== first)
        host.kind = ""
        compare(host.body.item, null)
    }
}
''')
        env = os.environ.copy()
        env["QT_QPA_PLATFORM"] = "offscreen"
        result = subprocess.run([runner, "-input", str(directory)], env=env, text=True)
        if result.returncode:
            raise SystemExit(result.returncode)


if __name__ == "__main__":
    run()
