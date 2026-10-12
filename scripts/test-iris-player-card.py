#!/usr/bin/env python3
"""Exercise production media card loading and its shared media/visualizer instance.

The card map, URL Loader, and external media layout are production declarations.
Leaf widgets and services are stubs; real MPRIS, effects, and playback aren't run.
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
    source = (STAGE / "IrisCardContent.qml").read_text(encoding="utf-8")
    payload = (STAGE / "IrisPlayerCard.qml").read_text(encoding="utf-8")
    body = block(source, r"Loader\s*\{\s*id:\s*body\b")
    wrapper = block(source, r"Component\s*\{\s*id:\s*mediaCard\b")
    switch = block(body, r"sourceComponent:\s*\{")
    ids = set(re.findall(r'case "[^\"]+": return (\w+)', switch))
    if 'case "media":\n            case "visualizer": return mediaCard' not in switch:
        raise ValueError("media and visualizer must share the same map entry")
    if 'Qt.resolvedUrl("IrisPlayerCard.qml")' not in wrapper:
        raise ValueError("media wrapper must defer the player URL")
    if "sourceComponent:" in wrapper or "asynchronous: true" in wrapper:
        raise ValueError("media payload must use a synchronous URL Loader")
    if "contentActive: Qt.binding(() => root.contentActive)" not in wrapper:
        raise ValueError("contentActive must be passed as an initial reactive property")
    if "ColorQuantizer" in source or "IrisMediaBackdrop" in source:
        raise ValueError("heavy player declarations remain in the common card file")
    if any(value not in payload for value in (
            "import qs.modules.iris.components", "import Quickshell",
            "import qs.modules.common.widgets")):
        raise ValueError("player imports must resolve both media components and quantizer")
    player = block(payload, r"Item\s*\{\s*id:\s*player\b")
    if re.search(r"\broot\.", player):
        raise ValueError("player retains an external lexical root dependency")
    player = player.replace("id: player", """id: player
    property alias card: card
    property alias artLoader: artLoader
    property alias quantizer: tintQuantizer
    property alias fallback: fallback""", 1)
    player = player.replace("Rectangle {", "Rectangle { id: fallback", 1)
    fixture = directory / "Fixture"
    fixture.mkdir()
    singletons = {
        "Stats": '''QtObject {
    property int creates: 0
    property int destroys: 0
    property int backdrops: 0
    property int backdropsDestroyed: 0
    property bool initialActive: true
}''',
        "IrisStyle": '''QtObject {
    property real density: 1.25
    property real cardPad: 30
    property color surfaceHigh: "#112233"
    function artTintOf(colors: var): color { return colors[0] || "#8899aa" }
    function tintFill(tint: color): color { return tint }
}''',
        "Config": 'QtObject { property var options: ({}) }',
        "MediaArtwork": 'QtObject { property string displaySource: "" }',
    }
    (fixture / "qmldir").write_text("module Fixture\n" + "".join(
        "singleton " + name + " 1.0 " + name + ".qml\n" for name in singletons), encoding="utf-8")
    for name, value in singletons.items():
        (fixture / (name + ".qml")).write_text("pragma Singleton\nimport QtQuick\n" + value + "\n", encoding="utf-8")
    (directory / "ColorQuantizer.qml").write_text('''import QtQuick
QtObject {
    property string source: ""
    property int depth: 0
    property int rescaleSize: 0
    property var colors: ["#336699"]
}
''', encoding="utf-8")
    (directory / "IrisMediaCard.qml").write_text('''import QtQuick
import "Fixture"
Item {
    property bool showBackground: true
    property bool active: true
    property color tint: "white"
    property real position: 0
    implicitHeight: 170
    Component.onCompleted: { ++Stats.creates; Stats.initialActive = active }
    Component.onDestruction: ++Stats.destroys
}
''', encoding="utf-8")
    (directory / "IrisMediaBackdrop.qml").write_text('''import QtQuick
import "Fixture"
Item {
    property string source: ""
    property real strength: 0
    Component.onCompleted: ++Stats.backdrops
    Component.onDestruction: ++Stats.backdropsDestroyed
}
''', encoding="utf-8")
    (directory / "IrisPlayerCard.qml").write_text('''pragma ComponentBehavior: Bound
import QtQuick
import "Fixture"
''' + player + "\n", encoding="utf-8")
    height = line(source, r"^    readonly property real contentHeight:.*$")
    implicit_height = line(source, r"^    implicitHeight: root\.contentHeight$")
    others = "\n".join("Component { id: " + name + "; Item { implicitHeight: 31 } }"
                       for name in sorted(ids - {"mediaCard"}))
    (directory / "CardHost.qml").write_text('''pragma ComponentBehavior: Bound
import QtQuick
Item {
    id: root
    width: 360
    property string kind: ""
    property bool contentActive: true
    property alias body: body
''' + height + "\n" + implicit_height + "\n" + body + "\n" + wrapper
        + "\n" + others + "\n}\n", encoding="utf-8")
    (directory / "tst_playercard.qml").write_text('''pragma ComponentBehavior: Bound
import QtQuick
import QtTest
import "Fixture"
TestCase {
    id: root
    name: "IrisPlayerCard"
    when: windowShown
    visible: true
    width: 640
    height: 600
    Component { id: hostComponent; CardHost {} }
    function init() {
        IrisStyle.density = 1.25
        IrisStyle.cardPad = 30
        MediaArtwork.displaySource = ""
        Config.options = ({})
    }
    function ready(host) {
        compare(host.body.status, Loader.Ready)
        const wrapper = host.body.item
        verify(wrapper !== null)
        compare(wrapper.asynchronous, false)
        compare(wrapper.status, Loader.Ready)
        verify(wrapper.source.toString().endsWith("/IrisPlayerCard.qml"))
        const player = wrapper.item
        verify(player !== null)
        compare(wrapper.width, host.width)
        compare(player.width, host.width)
        compare(wrapper.implicitHeight, player.implicitHeight)
        compare(host.contentHeight, player.implicitHeight)
        compare(host.implicitHeight, player.implicitHeight)
        return player
    }
    function test_shared_instance_activity_and_geometry() {
        const creates = Stats.creates
        const destroys = Stats.destroys
        const host = createTemporaryObject(hostComponent, root, { kind: "network", contentActive: false })
        compare(host.contentHeight, 31)
        compare(Stats.creates, creates)
        host.kind = "media"
        const player = ready(host)
        compare(Stats.creates, creates + 1)
        compare(Stats.initialActive, false)
        compare(player.contentActive, false)
        compare(player.card.active, false)
        compare(player.card.showBackground, false)
        compare(player.quantizer.depth, 2)
        compare(player.quantizer.rescaleSize, 48)
        compare(player.quantizer.source, "")
        compare(player.card.tint, player.tint)
        compare(player.inset, 12)
        compare(host.contentHeight, 194)
        tryCompare(player.card, "width", 336)
        compare(player.card.anchors.margins, 12)
        player.card.position = 42
        host.kind = "visualizer"
        compare(ready(host), player)
        compare(player.card.position, 42)
        compare(Stats.creates, creates + 1)
        compare(Stats.destroys, destroys)
        host.contentActive = true
        compare(player.contentActive, true)
        compare(player.card.active, true)
        host.contentActive = false
        compare(ready(host), player)
        compare(player.card.position, 42)
        host.width = 480
        compare(ready(host), player)
        tryCompare(player.card, "width", 456)
        IrisStyle.density = 1.5
        compare(player.d, 1.5)
        compare(player.inset, 9)
        compare(host.contentHeight, 188)
        tryCompare(player.card, "width", 462)
        IrisStyle.cardPad = 10
        compare(player.inset, 0)
        compare(host.contentHeight, 170)
        host.kind = "media"
        compare(ready(host), player)
        host.kind = "network"
        compare(host.contentHeight, 31)
        tryCompare(Stats, "destroys", destroys + 1)
        host.kind = "visualizer"
        verify(ready(host) !== player)
        compare(Stats.creates, creates + 2)
        host.kind = ""
        compare(host.body.item, null)
        compare(host.contentHeight, 0)
        tryCompare(Stats, "destroys", destroys + 2)
    }
    function test_cover_and_background_gates() {
        const backdrops = Stats.backdrops
        const destroyed = Stats.backdropsDestroyed
        const host = createTemporaryObject(hostComponent, root, { kind: "media" })
        const player = ready(host)
        compare(player.artLoader.active, false)
        compare(player.artLoader.item, null)
        compare(player.fallback.visible, true)
        compare(Stats.backdrops, backdrops)
        MediaArtwork.displaySource = "cover.png"
        compare(player.quantizer.source, "cover.png")
        compare(player.artLoader.active, true)
        compare(player.artLoader.status, Loader.Ready)
        compare(player.fallback.visible, false)
        compare(Stats.backdrops, backdrops + 1)
        const art = player.artLoader.item
        compare(art.children[0].source, "cover.png")
        compare(art.children[0].strength, 0.9)
        const gradient = art.children[1].gradient
        compare(gradient.orientation, Gradient.Horizontal)
        compare(gradient.stops.length, 2)
        compare(gradient.stops[0].position, 0)
        compare(gradient.stops[0].color, IrisStyle.tintFill(player.tint))
        compare(gradient.stops[1].position, 1)
        compare(gradient.stops[1].color, Qt.rgba(0, 0, 0, 0))
        host.contentActive = false
        compare(player.artLoader.item, art)
        compare(Stats.backdropsDestroyed, destroyed)
        MediaArtwork.displaySource = "another.png"
        compare(player.artLoader.item, art)
        compare(art.children[0].source, "another.png")
        Config.options = ({ iris: { player: { artworkBackground: false } } })
        compare(player.artLoader.active, false)
        compare(player.artLoader.item, null)
        compare(player.fallback.visible, true)
        tryCompare(Stats, "backdropsDestroyed", destroyed + 1)
        compare(ready(host), player)
        Config.options = ({ iris: { player: { artworkBackground: true } } })
        compare(player.artLoader.active, true)
        compare(Stats.backdrops, backdrops + 2)
        MediaArtwork.displaySource = ""
        compare(player.artLoader.active, false)
        tryCompare(Stats, "backdropsDestroyed", destroyed + 2)
        host.destroy()
    }
}
''', encoding="utf-8")


def main():
    candidates = [os.environ.get("QMLTESTRUNNER", ""),
                  "/usr/lib/qt6/bin/qmltestrunner", "/usr/lib64/qt6/bin/qmltestrunner",
                  "/usr/lib/x86_64-linux-gnu/qt6/bin/qmltestrunner",
                  shutil.which("qmltestrunner") or ""]
    with tempfile.TemporaryDirectory(prefix="inir-player-card-") as folder:
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
    print("Iris player card: no Qt 6 qmltestrunner found; native lifecycle not executed")
    return 0


if __name__ == "__main__":
    sys.exit(main())
