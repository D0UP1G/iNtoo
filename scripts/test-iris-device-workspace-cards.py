#!/usr/bin/env python3
"""Exercise extracted iSland network, Bluetooth and workspace payloads in Qt."""
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
    with tempfile.TemporaryDirectory(prefix="intoo-device-cards-") as temporary:
        directory = Path(temporary)
        fixture = directory / "Fixture"
        fixture.mkdir()
        cards = directory / "TestCards"
        cards.mkdir()
        (fixture / "qmldir").write_text("""module Fixture
singleton IrisStyle 1.0 IrisStyle.qml
singleton Translation 1.0 Translation.qml
singleton Network 1.0 Network.qml
singleton BluetoothStatus 1.0 BluetoothStatus.qml
singleton Bluetooth 1.0 Bluetooth.qml
singleton NiriService 1.0 NiriService.qml
singleton MinimizedWindows 1.0 MinimizedWindows.qml
""")
        (fixture / "IrisStyle.qml").write_text('''pragma Singleton
import QtQuick
QtObject {
    property real density: 1
    property var identity: ({blue: "blue", sky: "sky", purple: "purple"})
    property color accent: "blue"
    property color fillQuiet: "#333333"
    property color border: "#555555"
    property color text: "white"
    property color subtext: "gray"
    property string fontNumbers: "Sans"
    property real typeBody: 14
    function iconRadius(value) { return value / 4 }
    function tintFill(value) { return value }
    function weight(value) { return value }
}''')
        (fixture / "Translation.qml").write_text('''pragma Singleton
import QtQuick
QtObject { function tr(value) { return value } }''')
        (fixture / "Network.qml").write_text('''pragma Singleton
import QtQuick
QtObject {
    property bool ethernet: false
    property bool wifiEnabled: true
    property string networkName: "lab"
    property int rescans: 0
    property int toggles: 0
    function rescanWifi() { ++rescans }
    function toggleWifi() { ++toggles; wifiEnabled = !wifiEnabled }
}''')
        (fixture / "BluetoothStatus.qml").write_text('''pragma Singleton
import QtQuick
QtObject {
    property bool enabled: true
    property bool available: true
    function activeDeviceSummary() { return "headphones" }
}''')
        (fixture / "Bluetooth.qml").write_text('''pragma Singleton
import QtQuick
QtObject {
    property var defaultAdapter: QtObject { property bool enabled: true }
}''')
        (fixture / "NiriService.qml").write_text('''pragma Singleton
import QtQuick
QtObject {
    property var allWorkspaces: []
    property var windows: []
    property var events: []
    function toggleOverview() { events = events.concat(["overview"]) }
    function switchToWorkspaceById(id) { events = events.concat(["workspace:" + id]) }
    function focusWindow(id) { events = events.concat(["focus:" + id]) }
}''')
        (fixture / "MinimizedWindows.qml").write_text('''pragma Singleton
import QtQuick
QtObject { function isStashWorkspace(id) { return id === 90 } }''')
        (cards / "CardHeader.qml").write_text('''import QtQuick
import QtQuick.Layouts
RowLayout {
    property string glyph: ""
    property color tint: "transparent"
    property string title: ""
    property string detail: ""
    default property alias contents: row.data
    RowLayout { id: row }
}''')
        (cards / "IrisIconButton.qml").write_text('''import QtQuick
import QtQuick.Controls
Button { property string materialIcon: ""; property bool selected: false }''')
        for name in ("IrisNetworkList", "IrisBluetoothList"):
            (cards / f"{name}.qml").write_text("import QtQuick\nItem { implicitHeight: 36 }\n")
        (cards / "IrisText.qml").write_text('''import QtQuick
Text { property real weight: Font.Normal }''')
        (cards / "IrisColumnStrip.qml").write_text('''import QtQuick
Item {
    property var workspaceId
    property var windows: []
    property string outputName: ""
    property bool active: false
    signal windowActivated(int windowId)
    implicitHeight: 44
}''')

        for filename in ("IrisNetworkCard.qml", "IrisBluetoothCard.qml", "IrisWorkspacesCard.qml"):
            source = (STAGE / filename).read_text()
            source = re.sub(r"^import (?:qs\.|Quickshell).*$", 'import "../Fixture"', source, flags=re.M)
            if filename != "IrisWorkspacesCard.qml":
                source = source.replace('IrisIconButton {', 'IrisIconButton {\n                objectName: "actionButton"', 1)
                if filename == "IrisNetworkCard.qml":
                    source = re.sub(
                        r'(IrisIconButton\s*\{\s*)(?=materialIcon: Network\.wifiEnabled)',
                        r'\1objectName: "toggleButton"\n                ', source, count=1)
            else:
                source = source.replace('IrisIconButton {', 'IrisIconButton {\n                objectName: "overviewButton"', 1)
                source = source.replace('MouseArea {', 'MouseArea {\n                            objectName: "switchWorkspaceTile"', 1)
                source = source.replace('IrisColumnStrip {', 'IrisColumnStrip {\n                        objectName: "workspaceStrip"', 1)
            (cards / filename).write_text(source)

        production = (STAGE / "IrisCardContent.qml").read_text()
        loader = block(production, r"Loader\s*\{\s*id:\s*body\b")
        switch = block(loader, r"sourceComponent:\s*\{")
        loader = loader.replace(switch, '''sourceComponent: {
            switch (root.kind) {
            case "workspaces": return workspacesCard
            default: return null
            }
        }''')
        workspaces_component = block(production, r"Component\s*\{\s*id:\s*workspacesCard\b")
        host = '''pragma ComponentBehavior: Bound
import QtQuick
import QtQuick.Layouts
import "../Fixture"
Item {
    id: root
    property string kind: ""
    property string screenName: "A"
    property bool contentActive: true
    readonly property real d: IrisStyle.density
    readonly property real contentHeight: body.item?.implicitHeight ?? 0
    property alias payloadLoader: body
    signal navigate()
    function close(): void { root.navigate() }
    onNavigate: NiriService.events = NiriService.events.concat(["close"])
''' + loader + "\n" + workspaces_component + "\n}\n"
        (cards / "IrisCardContentTest.qml").write_text(host)

        (cards / "qmldir").write_text("""module TestCards
IrisCardContentTest 1.0 IrisCardContentTest.qml
IrisNetworkCard 1.0 IrisNetworkCard.qml
IrisBluetoothCard 1.0 IrisBluetoothCard.qml
IrisWorkspacesCard 1.0 IrisWorkspacesCard.qml
CardHeader 1.0 CardHeader.qml
IrisIconButton 1.0 IrisIconButton.qml
IrisNetworkList 1.0 IrisNetworkList.qml
IrisBluetoothList 1.0 IrisBluetoothList.qml
IrisText 1.0 IrisText.qml
IrisColumnStrip 1.0 IrisColumnStrip.qml
""")

        (directory / "tst_cards.qml").write_text('''pragma ComponentBehavior: Bound
import QtQuick
import QtQuick.Controls
import QtTest
import "Fixture"
import "TestCards" as Cards
TestCase {
    name: "IrisDeviceWorkspaceCards"
    visible: true
    when: windowShown
    width: 600; height: 500
    Component { id: networkCard; Cards.IrisNetworkCard {} }
    Component { id: bluetoothCard; Cards.IrisBluetoothCard {} }
    Component { id: cardContent; Cards.IrisCardContentTest {} }
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
        Network.rescans = 0; Network.toggles = 0; Network.wifiEnabled = true
        BluetoothStatus.enabled = true; BluetoothStatus.available = true
        Bluetooth.defaultAdapter.enabled = true
        NiriService.allWorkspaces = [
            {id: 7, idx: 5, output: "A", is_active: false},
            {id: 4, idx: 1, output: "A", is_active: true},
            {id: 90, idx: 0, output: "A", is_active: false},
            {id: 8, idx: 2, output: "B", is_active: false}
        ]
        NiriService.windows = [
            {id: 101, workspace_id: 4},
            {id: 102, workspace_id: 7},
            {id: 103, workspace_id: 8}
        ]
        NiriService.events = []
    }
    function test_network_actions_and_layout() {
        const card = createTemporaryObject(networkCard, this, {width: 350})
        verify(card !== null)
        verify(card.implicitHeight > 0)
        const buttons = descendants(card).filter(child => child.objectName === "actionButton" || child.objectName === "toggleButton")
        compare(buttons.length, 2)
        named(card, "actionButton").clicked()
        compare(Network.rescans, 1)
        named(card, "toggleButton").clicked()
        compare(Network.toggles, 1)
        compare(Network.wifiEnabled, false)
    }
    function test_bluetooth_toggle_and_layout() {
        const card = createTemporaryObject(bluetoothCard, this, {width: 350})
        verify(card !== null)
        verify(card.implicitHeight > 0)
        named(card, "actionButton").clicked()
        compare(Bluetooth.defaultAdapter.enabled, false)
    }
    function test_workspace_filter_sort_reactive_output_and_actions() {
        const card = createTemporaryObject(cardContent, this, {width: 350, kind: "workspaces", screenName: "A"})
        verify(card !== null)
        tryVerify(() => card.payloadLoader.item !== null)
        const payload = card.payloadLoader.item.item
        verify(payload !== null)
        compare(payload.screenName, "A")
        compare(payload.workspaces.map(ws => ws.id).join(","), "4,7")
        compare(payload.windows.length, 2)
        verify(card.contentHeight > 0)
        compare(card.contentHeight, payload.implicitHeight)
        compare(payload.width, card.width)
        named(payload, "overviewButton").clicked()
        compare(NiriService.events.join(","), "close,overview")
        card.contentActive = false
        compare(card.payloadLoader.item.item, payload)
        card.contentActive = true
        card.width = 400
        tryCompare(payload, "width", 400)
        card.screenName = "B"
        tryCompare(payload, "screenName", "B")
        compare(payload.workspaces.map(ws => ws.id).join(","), "8")
        compare(payload.windows.length, 1)
        NiriService.allWorkspaces = [{id: 9, idx: 3, output: "B", is_active: false}]
        NiriService.windows = [{id: 103, workspace_id: 9}]
        tryCompare(payload, "workspaces", [{id: 9, idx: 3, output: "B", is_active: false}])
        compare(payload.windows.length, 1)
        const strip = named(payload, "workspaceStrip")
        strip.windowActivated(103)
        compare(NiriService.events.join(","), "close,overview,close,focus:103")
        const tile = named(payload, "switchWorkspaceTile")
        tile.clicked(null)
        compare(NiriService.events.join(","), "close,overview,close,focus:103,close,workspace:9")
        card.kind = "unknown"
        tryCompare(card.payloadLoader, "item", null)
        card.kind = "workspaces"
        tryVerify(() => card.payloadLoader.item?.item !== null)
        verify(card.payloadLoader.item.item !== payload)
        card.kind = ""
        compare(card.payloadLoader.item, null)
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
