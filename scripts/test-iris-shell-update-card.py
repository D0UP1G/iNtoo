#!/usr/bin/env python3
"""Exercise the production shell update card loader and behavior in Qt."""
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
    payload = (STAGE / "IrisShellUpdateCard.qml").read_text()
    loader = block(content, r"Loader\s*\{\s*id:\s*body\b")
    switch = block(loader, r"sourceComponent:\s*\{")
    ids = set(re.findall(r'case "[^"]+": return (\w+)', switch))
    wrapper = block(content, r"Component\s*\{\s*id:\s*shellUpdateCard\b")
    if 'Qt.resolvedUrl("IrisShellUpdateCard.qml")' not in wrapper:
        raise ValueError("shell update card must retain its deferred payload URL")

    with tempfile.TemporaryDirectory(prefix="intoo-shell-update-card-") as temporary:
        directory = Path(temporary)
        fixture = directory / "Fixture"
        fixture.mkdir()
        singletons = {
            "IrisStyle": '''QtObject {
    property real density: 1.25
    property color secondaryAccent: "orange"
    property color subtext: "gray"
    property real radiusTile: 8
    property real typeMeta: 12
}''',
            "Translation": 'QtObject { function tr(value) { return value } }',
            "TestControl": 'QtObject { property var events: [] }',
            "ShellUpdates": '''QtObject {
    property string remoteVersion: "v2.0"
    property bool repoDiverged: false
    property int commitsBehind: 2
    property string latestMessage: "A change message"
    property bool isUpdating: false
    function performUpdate() { TestControl.events = TestControl.events.concat(["performUpdate"]) }
    function openOverlay() { TestControl.events = TestControl.events.concat(["openOverlay"]) }
    function dismiss() { TestControl.events = TestControl.events.concat(["dismiss"]) }
}''',
        }
        (fixture / "qmldir").write_text("module Fixture\n" + "".join(
            f"singleton {name} 1.0 {name}.qml\n" for name in singletons))
        for name, body in singletons.items():
            (fixture / f"{name}.qml").write_text("pragma Singleton\nimport QtQuick\n" + body + "\n")
        (directory / "CardHeader.qml").write_text('''import QtQuick
import QtQuick.Layouts
RowLayout {
    property string glyph: ""
    property color tint: "transparent"
    property string title: ""
    property string detail: ""
}''')
        (directory / "IrisText.qml").write_text('''import QtQuick
Text { property real pixelSize: 12 }''')
        (directory / "IrisButton.qml").write_text('''import QtQuick
import QtQuick.Controls
Button { property real buttonRadius: 0; property bool quiet: false }''')
        payload = re.sub(r"^import (?:qs\.|Quickshell).*$", 'import "Fixture"', payload, flags=re.M)
        payload = payload.replace("CardHeader {", 'CardHeader { objectName: "updateHeader"', 1)
        payload = payload.replace("IrisText {", 'IrisText { objectName: "updateMessage"', 1)
        payload = payload.replace("IrisButton {", 'IrisButton { objectName: "updateAction"', 1)
        payload = payload.replace("RowLayout {\n        Layout.fillWidth", 'RowLayout {\n        objectName: "secondaryActions"\n        Layout.fillWidth', 1)
        payload = payload.replace('text: Translation.tr("What changed")', 'objectName: "detailsAction"\n            text: Translation.tr("What changed")', 1)
        payload = payload.replace('text: Translation.tr("Not now")', 'objectName: "dismissAction"\n            text: Translation.tr("Not now")', 1)
        (directory / "IrisShellUpdateCard.qml").write_text(payload)

        declarations = "\n".join("Component { id: " + name + "; Item { implicitHeight: 28 } }"
                                   for name in sorted(ids - {"shellUpdateCard"}))
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
        root.navigate()
    }
''' + loader + "\n" + wrapper + "\n" + declarations + "\n}\n"
        (directory / "CardHost.qml").write_text(host)
        (directory / "tst_shell_update.qml").write_text('''pragma ComponentBehavior: Bound
import QtQuick
import QtTest
import "Fixture"
TestCase {
    id: test
    name: "IrisShellUpdateCard"
    visible: true
    when: windowShown
    width: 700; height: 500
    Component { id: hostComponent; CardHost {} }
    function ready(host) {
        compare(host.body.status, Loader.Ready)
        const wrapper = host.body.item
        compare(wrapper.status, Loader.Ready)
        compare(wrapper.source.toString().endsWith("/IrisShellUpdateCard.qml"), true)
        verify(wrapper.item !== null)
        return {wrapper: wrapper, payload: wrapper.item}
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
        ShellUpdates.remoteVersion = "v2.0"; ShellUpdates.repoDiverged = false
        ShellUpdates.commitsBehind = 2; ShellUpdates.latestMessage = "A change message"
        ShellUpdates.isUpdating = false; TestControl.events = []
    }
    function test_reactive_content_and_residency() {
        const host = createTemporaryObject(hostComponent, test, {kind: "shellUpdate", contentActive: false, width: 360})
        const loaded = ready(host)
        const header = named(loaded.payload, "updateHeader")
        const message = named(loaded.payload, "updateMessage")
        const update = named(loaded.payload, "updateAction")
        compare(header.glyph, "rocket_launch")
        compare(header.title, "iNtoo v2.0")
        compare(header.detail, "2 commits behind")
        compare(message.text, "A change message"); compare(message.visible, true)
        compare(update.enabled, true); compare(update.text, "Update now")
        compare(host.contentHeight, loaded.payload.implicitHeight)
        compare(loaded.payload.width, host.width)
        compare(TestControl.events.join(","), "")
        ShellUpdates.repoDiverged = true
        tryCompare(header, "detail", "Upstream history changed; your local work is kept")
        ShellUpdates.repoDiverged = false; ShellUpdates.commitsBehind = 0
        tryCompare(header, "detail", "Ready to update")
        ShellUpdates.remoteVersion = ""
        tryCompare(header, "title", "New iNtoo")
        ShellUpdates.latestMessage = ""
        tryCompare(message, "visible", false)
        ShellUpdates.isUpdating = true
        tryCompare(update, "enabled", false); compare(update.text, "Updating…")
        ShellUpdates.isUpdating = false
        host.contentActive = true
        compare(host.body.item.item, loaded.payload)
        host.width = 420
        tryCompare(loaded.payload, "width", 420)
    }
    function test_action_order_and_lifecycle() {
        const host = createTemporaryObject(hostComponent, test, {kind: "shellUpdate"})
        const first = ready(host).payload
        named(first, "updateAction").clicked()
        compare(TestControl.events.join(","), "close,performUpdate")
        TestControl.events = []
        named(first, "detailsAction").clicked()
        compare(TestControl.events.join(","), "close,openOverlay")
        TestControl.events = []
        named(first, "dismissAction").clicked()
        compare(TestControl.events.join(","), "close,dismiss")
        host.kind = "unknown"
        tryCompare(host.body, "item", null)
        host.kind = "shellUpdate"
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
