#!/usr/bin/env python3
"""Exercise the production VPN card URL/Loaders and moved lifecycle in native Qt.

The host's switch and Loader are real declarations. The external payload retains
its production root properties, lifecycle, functions, and all three VPN helpers;
small UI/service stubs replace desktop imports and the rest of its visual body.
Real IrisVpnCard imports and desktop service side effects are not fully tested.
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


def line(source, pattern):
    match = re.search(pattern, source, re.M)
    if not match:
        raise ValueError("production property is missing: " + pattern)
    return match.group().strip()


def write_harness(directory):
    content = (STAGE / "IrisCardContent.qml").read_text(encoding="utf-8")
    payload = (STAGE / "IrisVpnCard.qml").read_text(encoding="utf-8")
    loader = block(content, r"Loader\s*\{\s*id:\s*body\b")
    wrapper = block(content, r"Component\s*\{\s*id:\s*vpnCard\b")
    switch = block(loader, r"sourceComponent:\s*\{")
    ids = set(re.findall(r'case "[^"]+": return (\w+)', switch))
    if 'case "vpn": return vpnCard' not in switch:
        raise ValueError("VPN must retain the existing card map entry")
    nested = block(wrapper, r"Loader\s*\{")
    if 'source: Qt.resolvedUrl("IrisVpnCard.qml")' not in nested:
        raise ValueError("VPN wrapper must use the production payload URL")
    if "sourceComponent:" in nested or "ColumnLayout" in wrapper:
        raise ValueError("VPN wrapper still compiles the payload inline")
    if "asynchronous: true" in nested:
        raise ValueError("VPN payload must load synchronously")
    if "Vpn." in content or re.search(r"component Vpn(?:Row|Fact|Facts)\b", content):
        raise ValueError("VPN helpers and lifecycle must leave the common card file")
    if not re.search(r"ColumnLayout\s*\{\s*id:\s*root\b", payload):
        raise ValueError("VPN payload must retain a ColumnLayout root")
    api_start = payload.index("ColumnLayout {")
    api_end = payload.index("    CardHeader {", api_start)
    api = payload[api_start:api_end]
    for expected in ("readonly property real d: IrisStyle.density",
                     "Component.onCompleted: Vpn.keepAlive()",
                     "Component.onDestruction: Vpn.releaseKeepAlive()",
                     "property bool masked: true", "property bool copied: false"):
        if expected not in api or payload.count(expected) != 1:
            raise ValueError("moved root declaration must occur once: " + expected)
    if re.search(r"\bvpn\.", payload):
        raise ValueError("moved payload retains a stale lexical vpn reference")
    members = set(re.findall(r"\broot\.(\w+)", payload))
    if members != {"d", "masked", "copied", "copy", "hide", "rate", "traffic"}:
        raise ValueError("unexpected external root dependency: " + str(members))
    helpers = "\n".join(block(payload, r"component " + name + r":\s*\w+\s*\{")
                        for name in ("VpnRow", "VpnFact", "VpnFacts"))
    for dependency in ("42 * root.d", "28 * root.d", "8 * root.d", "4 * root.d"):
        if dependency not in helpers:
            raise ValueError("moved helpers lost scale dependency: " + dependency)

    fixture = directory / "Fixture"
    fixture.mkdir()
    singletons = {
        "IrisStyle": '''QtObject {
    property real density: 1.25
    property real radiusTile: 8
    property real radiusRow: 4
    property color fillHover: "#223344"
    property color fillQuiet: "#112233"
    property color muted: "#778899"
    property color onTint: "white"
    property var identity: ({ green: "#339966" })
    property real typeLabel: 14
    property real typeFootnote: 11
    property real typeMeta: 12
    property int feedbackEasing: Easing.OutCubic
    function duration(value: real): real { return 0 }
}''',
        "Vpn": '''QtObject {
    property int keeps: 0
    property int releases: 0
    property int held: 0
    property bool busy: false
    property var rates: ({})
    signal handOff()
    function keepAlive(): void { ++keeps; ++held }
    function releaseKeepAlive(): void { ++releases; --held }
}''',
        "GlobalStates": 'QtObject { property var irisBubbleCard: null }',
        "Quickshell": 'QtObject { property string clipboardText: "" }',
        "ColorUtils": '''QtObject {
    function applyAlpha(value: color, opacity: real): color {
        return Qt.rgba(value.r, value.g, value.b, opacity)
    }
}''',
    }
    (fixture / "qmldir").write_text("module Fixture\n" + "".join(
        "singleton " + name + " 1.0 " + name + ".qml\n" for name in singletons),
        encoding="utf-8")
    for name, body in singletons.items():
        (fixture / (name + ".qml")).write_text(
            "pragma Singleton\nimport QtQuick\n" + body + "\n", encoding="utf-8")
    (directory / "IrisText.qml").write_text("import QtQuick\nText {}\n", encoding="utf-8")
    (directory / "MaterialSymbol.qml").write_text(
        "import QtQuick\nText { property real iconSize: 14 }\n", encoding="utf-8")
    (directory / "IrisVpnCard.qml").write_text('''pragma ComponentBehavior: Bound
import QtQuick
import QtQuick.Layouts
import "Fixture"
''' + api + helpers + '''
    property alias row: row
    property alias fact: fact
    property alias facts: facts
    VpnRow { id: row; Layout.fillWidth: true; label: "Test VPN" }
    VpnFact { id: fact; Layout.fillWidth: true; label: "IPv4"; value: "10.20.30.40" }
    VpnFacts {
        id: facts
        Layout.fillWidth: true
        VpnFact { Layout.fillWidth: true; label: "Name"; value: "test.local" }
    }
}
''', encoding="utf-8")
    height = line(content, r"^    readonly property real contentHeight:.*$")
    implicit_height = line(content, r"^    implicitHeight: root\.contentHeight$")
    others = "\n".join("Component { id: " + name + "; Item { implicitHeight: 31 } }"
                       for name in sorted(ids - {"vpnCard"}))
    (directory / "CardHost.qml").write_text('''pragma ComponentBehavior: Bound
import QtQuick
Item {
    id: root
    width: 360
    property string kind: ""
    property alias body: body
''' + height + "\n" + implicit_height + "\n" + loader + "\n" + wrapper
        + "\n" + others + "\n}\n", encoding="utf-8")
    (directory / "tst_vpncard.qml").write_text('''pragma ComponentBehavior: Bound
import QtQuick
import QtTest
import "Fixture"
TestCase {
    id: root
    name: "IrisVpnCard"
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
        verify(wrapper.source.toString().endsWith("/IrisVpnCard.qml"))
        const payload = wrapper.item
        verify(payload !== null)
        compare(wrapper.width, host.width)
        compare(payload.width, host.width)
        compare(wrapper.implicitHeight, payload.implicitHeight)
        compare(host.contentHeight, payload.implicitHeight)
        compare(host.implicitHeight, payload.implicitHeight)
        return payload
    }

    function test_lazy_balanced_lifecycle_and_scale() {
        const keeps = Vpn.keeps
        const releases = Vpn.releases
        const held = Vpn.held
        const host = createTemporaryObject(hostComponent, root, { kind: "network" })
        verify(host !== null)
        compare(host.body.status, Loader.Ready)
        compare(host.contentHeight, 31)
        compare(Vpn.keeps, keeps)
        compare(Vpn.releases, releases)
        wait(30)
        compare(Vpn.keeps, keeps)

        host.kind = "vpn"
        const first = ready(host)
        compare(Vpn.keeps, keeps + 1)
        compare(Vpn.held, held + 1)
        compare(Vpn.releases, releases)
        compare(first.masked, true)
        compare(first.copied, false)
        compare(first.d, IrisStyle.density)
        compare(first.spacing, 8 * IrisStyle.density)
        compare(first.row.implicitHeight, Math.round(42 * IrisStyle.density))
        compare(first.fact.implicitHeight, Math.round(28 * IrisStyle.density))
        compare(first.facts.implicitHeight, Math.round(28 * IrisStyle.density) + Math.round(8 * IrisStyle.density))
        const height = host.contentHeight
        IrisStyle.density = 1.5
        compare(first.d, 1.5)
        compare(first.row.implicitHeight, 63)
        compare(first.fact.implicitHeight, 42)
        tryCompare(first.facts, "implicitHeight", 54)
        tryVerify(() => host.contentHeight > height)
        ready(host)
        first.masked = false
        first.copy("10.20.30.40")
        compare(Quickshell.clipboardText, "10.20.30.40")
        compare(first.copied, true)
        compare(first.hide("10.20.30.40"), "10.20.30.40")
        host.width = 480
        compare(ready(host), first)
        wait(30)
        compare(Vpn.keeps, keeps + 1)
        compare(Vpn.releases, releases)

        host.kind = "network"
        compare(host.contentHeight, 31)
        tryCompare(Vpn, "releases", releases + 1)
        compare(Vpn.held, held)
        wait(30)
        compare(Vpn.keeps, keeps + 1)
        compare(Vpn.releases, releases + 1)

        host.kind = "vpn"
        const second = ready(host)
        verify(second !== first)
        compare(Vpn.keeps, keeps + 2)
        compare(Vpn.held, held + 1)
        compare(second.masked, true)
        compare(second.copied, false)
        compare(second.d, 1.5)
        host.kind = ""
        compare(host.body.active, false)
        compare(host.body.item, null)
        compare(host.contentHeight, 0)
        tryCompare(Vpn, "releases", releases + 2)
        compare(Vpn.held, held)
        host.destroy()
        wait(30)
        compare(Vpn.keeps, keeps + 2)
        compare(Vpn.releases, releases + 2)
        IrisStyle.density = 1.25
    }

    function test_parent_destruction_releases_once() {
        const keeps = Vpn.keeps
        const releases = Vpn.releases
        const held = Vpn.held
        const host = createTemporaryObject(hostComponent, root, { kind: "vpn" })
        ready(host)
        compare(Vpn.keeps, keeps + 1)
        host.destroy()
        tryCompare(Vpn, "releases", releases + 1)
        compare(Vpn.held, held)
        wait(30)
        compare(Vpn.releases, releases + 1)
    }
}
''', encoding="utf-8")


def main():
    candidates = [os.environ.get("QMLTESTRUNNER", ""),
                  "/usr/lib/qt6/bin/qmltestrunner", "/usr/lib64/qt6/bin/qmltestrunner",
                  "/usr/lib/x86_64-linux-gnu/qt6/bin/qmltestrunner",
                  shutil.which("qmltestrunner") or ""]
    with tempfile.TemporaryDirectory(prefix="inir-vpn-card-") as folder:
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
    print("Iris VPN card: no Qt 6 qmltestrunner found; native lifecycle not executed")
    return 0


if __name__ == "__main__":
    sys.exit(main())
