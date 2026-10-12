#!/usr/bin/env python3
"""Exercise the production tray card filter, host loader, and residency in Qt."""
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
    payload = (STAGE / "IrisTrayCard.qml").read_text()
    loader = block(content, r"Loader\s*\{\s*id:\s*body\b")
    switch = block(loader, r"sourceComponent:\s*\{")
    ids = set(re.findall(r'case "[^"]+": return (\w+)', switch))
    wrapper = block(content, r"Component\s*\{\s*id:\s*trayCard\b")
    if 'Qt.resolvedUrl("IrisTrayCard.qml")' not in wrapper:
        raise ValueError("tray card must retain its deferred payload URL")
    with tempfile.TemporaryDirectory(prefix="intoo-tray-card-") as temporary:
        directory = Path(temporary)
        fixture = directory / "Fixture"
        fixture.mkdir()
        singletons = {
            "IrisStyle": 'QtObject { property real density: 1.25; property var identity: ({teal: "teal"}) }',
            "Translation": 'QtObject { function tr(value) { return value } }',
            "Status": 'QtObject { readonly property int passive: 0; readonly property int active: 1; readonly property int needsAttention: 2 }',
            "Config": 'QtObject { property var options: ({iris: {tray: {hidePassive: false}}}) }',
            "SystemTray": 'QtObject { property var items: ({values: []}) }',
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
        (directory / "IrisTray.qml").write_text('''import QtQuick
Item {
    property bool showHeader: true
    property var items: []
}''')
        payload = re.sub(r"^import (?:qs\.|Quickshell).*$", 'import "Fixture"', payload, flags=re.M)
        payload = payload.replace("Status.Passive", "Status.passive")
        payload = payload.replace("CardHeader {", 'CardHeader { objectName: "trayHeader"', 1)
        payload = payload.replace("IrisTray {", 'IrisTray { objectName: "trayView";', 1)
        (directory / "IrisTrayCard.qml").write_text(payload)
        declarations = "\n".join("Component { id: " + name + "; Item { implicitHeight: 28 } }"
                                   for name in sorted(ids - {"trayCard"}))
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
''' + loader + "\n" + wrapper + "\n" + declarations + "\n}\n"
        (directory / "CardHost.qml").write_text(host)
        (directory / "tst_tray.qml").write_text('''pragma ComponentBehavior: Bound
import QtQuick
import QtTest
import "Fixture"
TestCase {
    id: test
    name: "IrisTrayCard"
    visible: true
    when: windowShown
    width: 700; height: 500
    Component { id: hostComponent; CardHost {} }
    function ready(host) {
        compare(host.body.status, Loader.Ready)
        const wrapper = host.body.item
        compare(wrapper.status, Loader.Ready)
        compare(wrapper.source.toString().endsWith("/IrisTrayCard.qml"), true)
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
        Config.options = {iris: {tray: {hidePassive: false}}}
        SystemTray.items = {values: []}
    }
    function test_filter_order_header_and_reactivity() {
        const host = createTemporaryObject(hostComponent, test, {kind: "tray"})
        const loaded = ready(host)
        const header = named(loaded.payload, "trayHeader")
        const view = named(loaded.payload, "trayView")
        const active = {id: "first", status: Status.active}
        const passive = {id: "passive", status: Status.passive}
        const attention = {id: "attention", status: Status.needsAttention}
        const emptyId = {id: "", status: Status.active}
        SystemTray.items = {values: [active, null, emptyId, passive, attention]}
        tryCompare(header, "detail", "3 background apps")
        compare(view.showHeader, false)
        compare(view.items.length, 3)
        compare(view.items[0].id, "first")
        compare(view.items[1].id, "passive")
        compare(view.items[2].id, "attention")
        Config.options = {iris: {tray: {hidePassive: true}}}
        tryCompare(view, "items", [active, attention])
        tryCompare(header, "detail", "2 background apps")
        SystemTray.items = {values: []}
        tryCompare(header, "detail", "Background apps")
        compare(view.items.length, 0)
        compare(host.contentHeight, loaded.payload.implicitHeight)
    }
    function test_width_residency_and_recreation() {
        const host = createTemporaryObject(hostComponent, test, {kind: "tray", contentActive: false})
        const first = ready(host).payload
        host.contentActive = true
        compare(host.body.item.item, first)
        host.width = 420
        tryCompare(first, "width", 420)
        host.kind = "unknown"
        tryCompare(host.body, "item", null)
        host.kind = "tray"
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
