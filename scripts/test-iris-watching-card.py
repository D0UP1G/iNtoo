#!/usr/bin/env python3
"""Run real watching-card UI and production host Loaders with Qt/service stubs.

Only imports and leaf visual components are substituted; the complete production
IrisWatchingCard body, search state, reload, Resume handler, and forwarding run.
Desktop imports and ani-cli integration still need a real-session check.
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
        raise ValueError("missing production declaration: " + pattern)
    depth = 0
    for token in re.finditer(r'//[^\n]*|/\*.*?\*/|"(?:\\.|[^"\\])*"|'
                             r"'(?:\\.|[^'\\])*'|[{}]", source[start.start():], re.S):
        if token.group() == "{":
            depth += 1
        elif token.group() == "}":
            depth -= 1
            if not depth:
                return source[start.start():start.start() + token.end()]
    raise ValueError("unbalanced production declaration: " + pattern)


def write_harness(directory):
    content = (STAGE / "IrisCardContent.qml").read_text(encoding="utf-8")
    payload = (STAGE / "IrisWatchingCard.qml").read_text(encoding="utf-8")
    loader = block(content, r"Loader\s*\{\s*id:\s*body\b")
    wrapper = block(content, r"Component\s*\{\s*id:\s*watchingCard\b")
    if 'source: Qt.resolvedUrl("IrisWatchingCard.qml")' not in wrapper:
        raise ValueError("watchingCard must load the external file by URL")
    if "asynchronous: true" in wrapper or "contentActive" in wrapper:
        raise ValueError("watching card must retain synchronous resident behavior")
    switch = block(loader, r"sourceComponent:\s*\{")
    ids = set(re.findall(r'case "[^\"]+": return (\w+)', switch))
    others = "\n".join("Component { id: " + name + "; Item { implicitHeight: 31 } }"
                       for name in sorted(ids - {"watchingCard"}))
    fixture = directory / "Fixture"
    fixture.mkdir()
    singletons = {
        "AnimeWatch": '''QtObject {
    property int reloads: 0
    property int resumes: 0
    property var events: []
    property var shows: []
    property var pickItems: []
    property var subtitles: []
    property string query: ""
    property string phase: "idle"
    property string busyEpisode: ""
    property string error: ""
    property string cli: "ani-cli"
    property bool betweenEpisodes: false
    property bool available: true
    property bool running: false
    property bool choosing: false
    property bool busy: false
    property bool canTarget: true
    property bool pickingShow: false
    property bool pickingQuality: false
    property bool pickingEpisode: false
    property int subtitleId: 0
    function record(value: string): void { events = events.concat([value]) }
    function reload(): void { ++reloads }
    function resume(value: var): void { ++resumes; record("resume") }
    function search(value: string): void { query = value; record("search") }
    function cancel(): void {}
    function refresh(): void {}
    function choose(value: var): void {}
    function pickLabel(value: var): string { return String(value) }
    function pickValue(value: var): string { return String(value) }
    function actionLabel(value: var): string { return String(value) }
    function subtitleLabel(value: var, index: int): string { return String(value) }
    function selectSubtitle(value: int): void {}
    function stateOf(value: var): var { return {episode: "2", start: 0} }
    function isBusyFor(value: var): bool { return false }
    function coverOf(value: var): string { return "" }
    function clock(value: real): string { return String(value) }
}''',
        "IrisStyle": '''QtObject {
    property real density: 1
    property var identity: ({pink: "pink"})
    property real radiusTile: 8
    property real radiusChip: 5
    property real typeLabel: 14
    property real typeFootnote: 11
    property real typeMeta: 12
    property color muted: "gray"
    property color textSecondary: "white"
    property color fillHover: "#223344"
    property color fillQuiet: "#112233"
    property color fillActive: "#334455"
    property color veilStrong: "#334455"
    property string fontNumbers: "Sans"
    property int feedbackEasing: Easing.OutCubic
    function duration(value: real): real { return 0 }
    function weight(value: int): int { return value }
    function iconRadius(value: real): real { return 3 }
    function tintFill(value: color): color { return value }
    function tintFillHover(value: color): color { return value }
}''',
        "IrisPieces": 'QtObject { function glyphOf(kind: string, fallback: string): string { return kind } }',
        "Translation": 'QtObject { function tr(value: string): string { return value } }',
    }
    (fixture / "qmldir").write_text("module Fixture\n" + "".join(
        "singleton " + name + " 1.0 " + name + ".qml\n" for name in singletons), encoding="utf-8")
    for name, body in singletons.items():
        (fixture / (name + ".qml")).write_text("pragma Singleton\nimport QtQuick\n" + body + "\n", encoding="utf-8")
    leaves = {
        "CardHeader": 'Item { property string glyph; property color tint; property string title; property string detail; implicitHeight: 45 }',
        "IrisButton": 'Item { property string text; property real buttonRadius; signal clicked() }',
        "IrisIconButton": 'Item { property string materialIcon; signal clicked(); implicitHeight: 20 }',
        "IrisField": '''Item {
    property string text: ""
    property string placeholderText
    property real topPadding
    property real bottomPadding
    property int verticalAlignment
    property font font
    signal accepted()
}''',
        "IrisText": 'Text {}',
        "ClippingRectangle": 'Rectangle {}',
        "IrisImage": 'Image {}',
        "MaterialSymbol": 'Text { property real fill; property real iconSize }',
        "Metric": 'Item { property string value; property string unit; property real pixelSize }',
    }
    for name, body in leaves.items():
        (directory / (name + ".qml")).write_text("import QtQuick\n" + body + "\n", encoding="utf-8")
    # Preserve the production body verbatim, replacing only its desktop imports.
    body = payload[payload.index("ColumnLayout {"):]
    (directory / "IrisWatchingCard.qml").write_text('''pragma ComponentBehavior: Bound
import QtQuick
import QtQuick.Layouts
import "Fixture"
''' + body, encoding="utf-8")
    height = re.search(r"^    readonly property real contentHeight:.*$", content, re.M).group()
    close = re.search(r"^    function close\(\): void.*$", content, re.M).group()
    active = re.search(r"^    property bool contentActive:.*$", content, re.M).group()
    (directory / "CardHost.qml").write_text('''pragma ComponentBehavior: Bound
import QtQuick
import "Fixture"
Item {
    id: root
    width: 360
    property string kind: ""
    property alias body: body
    signal navigate()
''' + height + "\n" + close + "\n" + active + '''
    implicitHeight: root.contentHeight
    onNavigate: AnimeWatch.record("close")
''' + loader + "\n" + wrapper + "\n" + others + "\n}\n", encoding="utf-8")
    (directory / "tst_watchingcard.qml").write_text('''pragma ComponentBehavior: Bound
import QtQuick
import QtTest
import "Fixture"
TestCase {
    id: root
    name: "IrisWatchingCard"
    when: windowShown
    visible: true
    width: 640
    height: 600
    Component { id: hostComponent; CardHost {} }
    SignalSpy { id: closeSpy; signalName: "navigate" }

    function init() {
        AnimeWatch.canTarget = true
        AnimeWatch.events = []
        AnimeWatch.shows = []
        closeSpy.target = null
        closeSpy.clear()
    }
    function ready(host) {
        compare(host.body.status, Loader.Ready)
        compare(host.body.asynchronous, false)
        const wrapper = host.body.item
        compare(wrapper.status, Loader.Ready)
        compare(wrapper.asynchronous, false)
        verify(wrapper.source.toString().endsWith("/IrisWatchingCard.qml"))
        const payload = wrapper.item
        verify(payload !== null)
        compare(wrapper.width, host.width)
        compare(payload.width, host.width)
        tryVerify(() => payload.implicitHeight > 0)
        compare(wrapper.implicitHeight, payload.implicitHeight)
        compare(host.contentHeight, payload.implicitHeight)
        compare(host.implicitHeight, payload.implicitHeight)
        return payload
    }
    function find(item, property, value) {
        if (item[property] === value) return item
        for (let child of item.children) {
            const result = find(child, property, value)
            if (result !== null) return result
        }
        return null
    }
    function test_lazy_reload_state_size_and_destruction() {
        const reloads = AnimeWatch.reloads
        const host = createTemporaryObject(hostComponent, root, {kind: "network"})
        verify(host !== null)
        compare(host.contentHeight, 31)
        wait(20)
        compare(AnimeWatch.reloads, reloads)
        host.kind = "watching"
        const first = ready(host)
        compare(AnimeWatch.reloads, reloads + 1)
        const search = find(first, "placeholderText", "Search anime")
        verify(search !== null)
        search.text = "saved search"
        host.contentActive = false
        compare(ready(host), first)
        compare(search.text, "saved search")
        host.contentActive = true
        compare(ready(host), first)
        compare(AnimeWatch.reloads, reloads + 1)
        host.width = 480
        compare(ready(host), first)
        const height = host.contentHeight
        AnimeWatch.shows = [{title: "test show", total: 12}]
        tryVerify(() => host.contentHeight > height)
        ready(host)
        search.accepted()
        compare(AnimeWatch.query, "saved search")
        compare(search.text, "")
        host.kind = "network"
        compare(host.contentHeight, 31)
        wait(20)
        verify(!Qt.isQtObject(first))
        compare(AnimeWatch.reloads, reloads + 1)
        host.kind = "watching"
        const second = ready(host)
        compare(AnimeWatch.reloads, reloads + 2)
        compare(find(second, "placeholderText", "Search anime").text, "")
        host.kind = ""
        compare(host.body.active, false)
        compare(host.body.item, null)
        compare(host.contentHeight, 0)
        wait(20)
        verify(!Qt.isQtObject(second))
        host.kind = "watching"
        const third = ready(host)
        compare(AnimeWatch.reloads, reloads + 3)
        host.destroy()
        wait(20)
        verify(!Qt.isQtObject(third))
        compare(AnimeWatch.reloads, reloads + 3)
    }
    function test_close_forwarded_once_and_resume_order() {
        const host = createTemporaryObject(hostComponent, root, {kind: "watching"})
        const payload = ready(host)
        closeSpy.target = host
        payload.closeRequested()
        compare(closeSpy.count, 1)
        compare(AnimeWatch.events.join(","), "close")
        AnimeWatch.events = []
        AnimeWatch.canTarget = false
        const resume = find(payload, "text", "Resume in ani-cli")
        verify(resume !== null)
        verify(resume.visible)
        const resumes = AnimeWatch.resumes
        resume.clicked()
        compare(closeSpy.count, 2)
        compare(AnimeWatch.resumes, resumes + 1)
        compare(AnimeWatch.events.join(","), "close,resume")
        compare(host.body.item.item, payload)
    }
}
''', encoding="utf-8")


def main():
    candidates = [os.environ.get("QMLTESTRUNNER", ""),
                  "/usr/lib/qt6/bin/qmltestrunner", "/usr/lib64/qt6/bin/qmltestrunner",
                  "/usr/lib/x86_64-linux-gnu/qt6/bin/qmltestrunner",
                  shutil.which("qmltestrunner") or ""]
    with tempfile.TemporaryDirectory(prefix="inir-watching-card-") as folder:
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
    print("Iris watching card: no Qt 6 qmltestrunner found; native lifecycle not executed")
    return 0


if __name__ == "__main__":
    sys.exit(main())
