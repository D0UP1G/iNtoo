#!/usr/bin/env python3
"""Exercise the stage's production card Loader and morph gate with a tiny child.

Native Qt checks residency, bindings and readiness; the child contains no real
cards or desktop services. The optional shell startup check covers those imports.
"""

import os
from pathlib import Path
import re
import shutil
import subprocess
import sys
import tempfile

ROOT = Path(__file__).resolve().parent.parent
STAGE = ROOT / "modules/iris/stage/IrisStage.qml"
MORPH = ROOT / "modules/iris/components/IrisMorphSurface.qml"


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
    stage = STAGE.read_text(encoding="utf-8")
    morph = MORPH.read_text(encoding="utf-8")
    scroller = block(stage, r"Flickable\s*\{\s*id:\s*cardScroller\b")
    loader = block(scroller, r"Loader\s*\{\s*id:\s*cardContent\b")
    if "sourceComponent:" in loader or "IrisCardContent {" in stage:
        raise ValueError("stage still compiles the card content inline")
    if 'Qt.resolvedUrl("IrisCardContent.qml")' not in loader:
        raise ValueError("card Loader must load the external production child")
    lifecycle = "\n".join(line(stage, pattern) for pattern in (
        r"^    property var cardAnchor:.*$",
        r"^    onCardRequestChanged:.*$",
        r"^    readonly property string cardKind:.*$",
        r"^    readonly property bool cardOpen:.*$",
        r"^    readonly property bool cardPresent:.*$",
    ))
    gate = "\n".join((
        line(morph, r"^    readonly property bool canArm:.*$"),
        block(morph, r"function arm\(\): void\s*\{"),
        block(morph, r"Timer\s*\{\s*id:\s*armLater\b"),
        block(morph, r"Timer\s*\{\s*id:\s*captureLater\b"),
        line(morph, r"^    onCanArmChanged:.*$"),
        block(morph, r"onOpenChanged:\s*\{"),
        line(morph, r"^    Component.onCompleted:.*$"),
    ))
    fixture = directory / "Fixture"
    fixture.mkdir()
    (fixture / "qmldir").write_text(
        "module Fixture\nsingleton IrisStyle 1.0 IrisStyle.qml\n"
        "singleton IrisMotionMeter 1.0 IrisMotionMeter.qml\n", encoding="utf-8")
    (fixture / "IrisStyle.qml").write_text('''pragma Singleton
import QtQuick
QtObject { property color wallpaperLight: "#8899aa" }
''', encoding="utf-8")
    (fixture / "IrisMotionMeter.qml").write_text('''pragma Singleton
import QtQuick
QtObject {
    function watching(surface: string): bool { return false }
    function mark(event: string): void {}
}
''', encoding="utf-8")
    (directory / "state.js").write_text(
        ".pragma library\nvar created = 0\nvar destroyed = 0\n", encoding="utf-8")
    (directory / "IrisCardContent.qml").write_text('''pragma ComponentBehavior: Bound
import QtQuick
import "state.js" as State

Item {
    id: root
    property string kind: ""
    property string screenName: ""
    property bool contentActive: true
    property real contentHeight: 0
    property bool bleeds: false
    property color light: "transparent"
    property real widthOnCompleted: -1
    property string kindOnCompleted
    property string screenOnCompleted
    property bool activeOnCompleted: false
    property int serial: 0
    implicitHeight: root.contentHeight
    signal navigate()
    Component.onCompleted: {
        root.widthOnCompleted = root.width
        root.kindOnCompleted = root.kind
        root.screenOnCompleted = root.screenName
        root.activeOnCompleted = root.contentActive
        root.serial = ++State.created
    }
    Component.onDestruction: ++State.destroyed
}
''', encoding="utf-8")
    (directory / "MorphGate.qml").write_text('''pragma ComponentBehavior: Bound
import QtQuick
import "Fixture"

Item {
    id: root
    property bool open: false
    property bool contentReady: false
    property bool armed: false
    property bool settles: true
    property real presentation: 0
    property real progress: 0
    property bool launched: false
    property int launchFrames: 0
    property string motionSurface: "cards"
    property bool fromIsland: false
    property string originOwner: ""
    property int starts: 0
    function captureOrigin(): void { ++root.starts }
''' + gate + "\n}\n", encoding="utf-8")
    readiness = line(stage, r"^        contentReady: cardContent\.contentHeight.*$")
    height = line(stage, r"^        height: Math\.round\(Math\.min\(root\.height \* 0\.72, cardContent.*$")
    above_mask = line(stage, r"^                opacity: cardContent\.bleeds && cardScroller\.moreAbove.*$")
    below_mask = line(stage, r"^                opacity: cardContent\.bleeds && cardScroller\.moreBelow.*$")
    mask_properties = "\n".join((
        above_mask.replace("opacity:", "readonly property real aboveMask:", 1),
        below_mask.replace("opacity:", "readonly property real belowMask:", 1),
    ))
    (directory / "CardHost.qml").write_text('''pragma ComponentBehavior: Bound
import QtQuick
import "Fixture"

Item {
    id: root
    width: 360
    height: 600
    property string screenName: "screen-a"
    property var cardRequest: null
    property real cardPad: 12
    property int closeCalls: 0
    property alias loader: cardContent
    property alias scroller: cardScroller
    property alias surface: card
    function closeCard(): void { ++root.closeCalls; root.cardRequest = null }
''' + lifecycle + "\n" + mask_properties + '''
    MorphGate {
        id: card
        open: root.cardOpen
        width: root.width
''' + readiness + "\n" + height + "\n" + scroller + "\n    }\n}\n",
        encoding="utf-8")
    (directory / "tst_cardcontent.qml").write_text('''pragma ComponentBehavior: Bound
import QtQuick
import QtTest
import "Fixture"
import "state.js" as State

TestCase {
    id: root
    name: "IrisCardContentLoader"
    when: windowShown
    visible: true
    width: 640
    height: 600
    Component { id: hostComponent; CardHost {} }

    function checkInitialProperties(host, kind, screen, active) {
        const child = host.loader.item
        verify(child !== null)
        compare(child.kindOnCompleted, kind)
        compare(child.screenOnCompleted, screen)
        compare(child.activeOnCompleted, active)
        compare(child.widthOnCompleted, host.scroller.width)
        compare(child.width, host.scroller.width)
    }

    function test_cancel_before_ready() {
        const created = State.created
        const host = createTemporaryObject(hostComponent, root)
        verify(host !== null)
        host.cardRequest = { kind: "weather" }
        compare(host.loader.asynchronous, true)
        verify(host.loader.source.toString().endsWith("/IrisCardContent.qml"))
        if (host.loader.status !== Loader.Loading) {
            host.destroy()
            skip("Qt completed the tiny child before Loading cancellation could be observed")
            return
        }
        compare(host.loader.item, null)
        compare(host.loader.contentHeight, 0)
        compare(host.surface.contentReady, false)
        compare(host.surface.armed, false)
        host.cardRequest = null
        // Closing retains the anchor and must not cancel an in-flight load.
        compare(host.cardKind, "weather")
        compare(host.loader.active, true)
        host.cardAnchor = null
        compare(host.loader.active, false)
        compare(host.loader.item, null)
        compare(host.loader.status, Loader.Null)
        compare(host.loader.source.toString(), "")
        wait(50)
        compare(State.created, created)
        compare(host.loader.item, null)
        host.cardRequest = { kind: "network" }
        tryCompare(host.loader, "status", Loader.Ready)
        compare(State.created, created + 1)
        checkInitialProperties(host, "network", "screen-a", true)
    }

    function test_deferred_lifecycle() {
        const created = State.created
        const destroyed = State.destroyed
        const host = createTemporaryObject(hostComponent, root)
        verify(host !== null)
        compare(host.cardKind, "")
        compare(host.loader.active, false)
        compare(host.loader.asynchronous, true)
        compare(host.loader.source.toString(), "")
        compare(host.loader.status, Loader.Null)
        compare(host.loader.item, null)
        compare(host.loader.contentHeight, 0)
        compare(host.loader.bleeds, false)
        compare(host.loader.light, IrisStyle.wallpaperLight)
        compare(host.surface.contentReady, false)
        compare(host.scroller.contentHeight, 0)
        compare(host.scroller.interactive, false)
        compare(host.scroller.clip, true)
        compare(host.scroller.anchors.leftMargin, 12)
        compare(host.aboveMask, 0)
        compare(host.belowMask, 0)
        wait(30)
        compare(State.created, created)
        compare(host.surface.armed, false)
        compare(host.surface.starts, 0)

        host.cardRequest = { kind: "weather" }
        tryCompare(host.loader, "status", Loader.Ready)
        const child = host.loader.item
        const serial = child.serial
        compare(State.created, created + 1)
        checkInitialProperties(host, "weather", "screen-a", true)
        compare(host.loader.contentHeight, 0)
        compare(host.surface.contentReady, false)
        wait(30)
        compare(host.surface.armed, false)
        compare(host.surface.starts, 0)

        child.contentHeight = 160
        compare(host.loader.contentHeight, 160)
        compare(host.loader.height, 160)
        compare(host.surface.contentReady, true)
        compare(host.surface.height, 184)
        compare(host.scroller.contentHeight, 160)
        tryCompare(host.surface, "armed", true)
        compare(host.surface.starts, 1)
        child.contentHeight = 0
        compare(host.surface.contentReady, false)
        child.contentHeight = 900
        compare(host.surface.height, 432)
        compare(host.scroller.contentHeight, 900)
        compare(host.scroller.interactive, true)
        compare(host.scroller.moreBelow, true)
        wait(30)
        compare(host.surface.starts, 1)
        child.bleeds = true
        compare(host.loader.bleeds, true)
        compare(host.scroller.clip, false)
        compare(host.scroller.anchors.leftMargin, 0)
        compare(host.belowMask, 1)
        host.scroller.contentY = 20
        compare(host.aboveMask, 1)
        child.light = "#ff3366"
        compare(host.loader.light, child.light)
        child.light = "transparent"
        compare(host.loader.light, child.light)

        host.width = 420
        tryCompare(host.loader, "width", 420)
        tryCompare(child, "width", 420)
        host.screenName = "screen-b"
        host.cardRequest = { kind: "calendar" }
        compare(child.kind, "calendar")
        compare(child.screenName, "screen-b")
        compare(host.loader.item, child)
        compare(child.serial, serial)
        host.loader.updateSource()
        host.loader.updateSource()
        compare(State.created, created + 1)

        // Navigate closes the request; fading content is kept by cardAnchor.
        host.surface.progress = 0.5
        child.navigate()
        compare(host.closeCalls, 1)
        compare(host.cardOpen, false)
        compare(host.cardPresent, true)
        compare(child.contentActive, true)
        compare(host.loader.item, child)
        host.surface.progress = 0
        compare(host.cardPresent, false)
        compare(child.contentActive, false)
        compare(host.loader.active, true)
        compare(host.cardKind, "calendar")
        compare(host.loader.item, child)
        compare(State.destroyed, destroyed)
        host.screenName = "screen-c"
        compare(child.screenName, "screen-c")
        host.cardRequest = { kind: "sound" }
        compare(host.loader.item, child)
        compare(child.contentActive, true)
        compare(child.kind, "sound")
        tryCompare(host.surface, "armed", true)
        compare(host.surface.starts, 2)

        host.closeCard()
        host.cardAnchor = { kind: "" }
        tryCompare(host.loader, "item", null)
        compare(host.loader.active, false)
        compare(host.loader.source.toString(), "")
        compare(host.loader.status, Loader.Null)
        compare(host.loader.contentHeight, 0)
        compare(host.loader.bleeds, false)
        compare(host.loader.light, IrisStyle.wallpaperLight)
        compare(host.surface.contentReady, false)
        compare(host.aboveMask, 0)
        compare(host.belowMask, 0)
        tryVerify(() => State.destroyed === destroyed + 1)

        host.width = 500
        host.cardRequest = { kind: "network" }
        tryCompare(host.loader, "status", Loader.Ready)
        compare(State.created, created + 2)
        verify(host.loader.item.serial !== serial)
        checkInitialProperties(host, "network", "screen-c", true)
        host.destroy()
        tryVerify(() => State.destroyed === destroyed + 2)
    }

    function test_close_while_loading() {
        const created = State.created
        const host = createTemporaryObject(hostComponent, root)
        verify(host !== null)
        host.cardRequest = { kind: "calendar" }
        if (host.loader.status !== Loader.Loading) {
            host.destroy()
            skip("Qt completed the tiny child before Loading close could be observed")
            return
        }
        host.closeCard()
        compare(host.cardPresent, false)
        compare(host.cardKind, "calendar")
        compare(host.loader.active, true)
        tryCompare(host.loader, "status", Loader.Ready)
        compare(State.created, created + 1)
        checkInitialProperties(host, "calendar", "screen-a", false)
        compare(host.cardRequest, null)
        compare(host.cardAnchor.kind, "calendar")
        host.loader.item.contentHeight = 160
        compare(host.surface.contentReady, true)
        compare(host.surface.canArm, false)
        wait(30)
        compare(host.surface.armed, false)
        compare(host.surface.starts, 0)
        compare(host.loader.active, true)
        compare(host.cardKind, "calendar")
        compare(host.loader.item.contentActive, false)
    }

    function test_initial_anchor_data() {
        return [{ tag: "open", open: true }, { tag: "retained", open: false }]
    }

    function test_initial_anchor(data) {
        const created = State.created
        const request = { kind: "media" }
        const host = createTemporaryObject(hostComponent, root, {
            cardAnchor: request,
            cardRequest: data.open ? request : null,
            screenName: "initial-screen"
        })
        verify(host !== null)
        tryCompare(host.loader, "status", Loader.Ready)
        compare(host.loader.active, true)
        compare(host.cardPresent, data.open)
        compare(State.created, created + 1)
        checkInitialProperties(host, "media", "initial-screen", data.open)
        compare(host.surface.contentReady, false)
        compare(host.surface.armed, false)
        host.loader.updateSource()
        compare(State.created, created + 1)
    }
}
''', encoding="utf-8")


def main():
    candidates = [os.environ.get("QMLTESTRUNNER", ""),
                  "/usr/lib/qt6/bin/qmltestrunner", "/usr/lib64/qt6/bin/qmltestrunner",
                  "/usr/lib/x86_64-linux-gnu/qt6/bin/qmltestrunner",
                  shutil.which("qmltestrunner") or ""]
    with tempfile.TemporaryDirectory(prefix="inir-cardcontent-loader-") as folder:
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
    print("Iris card content Loader: no Qt 6 qmltestrunner found; native lifecycle not executed")
    return 0


if __name__ == "__main__":
    sys.exit(main())
