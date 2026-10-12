#!/usr/bin/env python3
"""Run the real anime payload and production card Loaders in native Qt.

Desktop services and leaf widgets are replaced by small stubs. Production root
properties, lifecycle, bindings, layout, Repeater and actions remain intact.
This does not exercise AniList networking or real Quickshell widget rendering.
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
            if depth == 0:
                return source[start.start():start.start() + token.end()]
    raise ValueError("unbalanced production declaration: " + pattern)


def harness(directory):
    host = (STAGE / "IrisCardContent.qml").read_text()
    payload = (STAGE / "IrisAnimeCard.qml").read_text()
    loader = block(host, r"Loader\s*\{\s*id:\s*body\b")
    wrapper = block(host, r"Component\s*\{\s*id:\s*animeCard\b")
    if 'case "anime": return animeCard' not in loader:
        raise ValueError("anime card map entry changed")
    if 'source: Qt.resolvedUrl("IrisAnimeCard.qml")' not in wrapper:
        raise ValueError("anime wrapper must load its external URL")
    if "asynchronous: true" in wrapper or "ColumnLayout" in wrapper:
        raise ValueError("anime wrapper must be synchronous and defer its payload")
    if "AnimeService." in host:
        raise ValueError("anime payload remains in the common card file")
    if payload.count("Component.onCompleted: AnimeService.fetchTopAiring()") != 1:
        raise ValueError("payload creation must retain one fetch hook")
    if "readonly property real d: IrisStyle.density" not in payload:
        raise ValueError("payload lost its reactive density")
    members = set(re.findall(r"\broot\.(\w+)", payload))
    if members != {"d", "shows", "next"} or re.search(r"\bairing\.", payload):
        raise ValueError("unexpected lexical dependency: " + str(members))
    fixture = directory / "Fixture"
    fixture.mkdir()
    singletons = {
        "AnimeService": '''QtObject {
    property int fetches: 0
    property int invalidations: 0
    property int destructions: 0
    property string lastError: ""
    property bool loading: false
    function fetchTopAiring(): void { ++fetches }
    function invalidateTopCache(): void { ++invalidations }
}''',
        "IrisStyle": '''QtObject {
    property real density: 1.25
    property var identity: ({ pink: "#ff6699" })
    property real radiusTile: 8
    property color fillHover: "#223344"
    property color fillQuiet: "#112233"
    property color muted: "#8899aa"
    property color subtext: "#aabbcc"
    property real typeLabel: 14
    property real typeFootnote: 11
    property real typeMeta: 12
    property string fontNumbers: "sans-serif"
    property int feedbackEasing: Easing.OutCubic
    function duration(value: real): real { return 0 }
    function iconRadius(value: real): real { return 4 }
    function weight(value: int): int { return value }
}''',
        "Config": 'QtObject { property var options: ({ iris: { anime: { shows: 5 } } }) }',
        "Network": 'QtObject { property bool online: true; property string offlineReason: "Offline" }',
        "Translation": 'QtObject { function tr(value: string): string { return value } }',
        "IrisPieces": '''QtObject {
    property var animeUpcoming: [
        { id: 1, title: "One", nextEpisode: 2, airingAt: 123 },
        { id: 2, title: "Two", nextEpisode: 3, airingAt: 456 }
    ]
    property var animeNext: animeUpcoming[0]
    property var animeFollowing: []
    property int toggles: 0
    property int lastToggle: 0
    function glyphOf(kind: string, fallback: string): string { return "movie" }
    function animeWait(value: real): string { return "soon" }
    function animeFollows(value: int): bool { return animeFollowing.includes(value) }
    function animeToggleFollow(value: int): void {
        ++toggles; lastToggle = value; animeFollowing = [value]
    }
}''',
    }
    (fixture / "qmldir").write_text("module Fixture\n" + "".join(
        "singleton " + name + " 1.0 " + name + ".qml\n" for name in singletons))
    for name, body in singletons.items():
        (fixture / (name + ".qml")).write_text("pragma Singleton\nimport QtQuick\n" + body)
    widgets = {
        "CardHeader": '''Item {
    property string glyph
    property color tint
    property string title
    property string detail
    implicitHeight: 32
}''',
        "IrisIconButton": '''Item {
    property string materialIcon
    signal clicked()
    objectName: "refresh"
}''',
        "ClippingRectangle": "Rectangle {}",
        "IrisImage": "Image {}",
        "IrisText": "Text {}",
        "MaterialSymbol": "Text { property real iconSize; property real fill }",
    }
    for name, body in widgets.items():
        (directory / (name + ".qml")).write_text("import QtQuick\n" + body)
    # Observe destruction without replacing production completion or actions.
    body = payload[payload.index("ColumnLayout {"):]
    body = body.replace("    id: root\n", "    id: root\n"
                        "    Component.onDestruction: ++AnimeService.destructions\n", 1)
    (directory / "IrisAnimeCard.qml").write_text(
        'pragma ComponentBehavior: Bound\nimport QtQuick\nimport QtQuick.Layouts\n'
        'import "Fixture"\n' + body)
    ids = set(re.findall(r'case "[^\"]+": return (\w+)', loader))
    other_cards = "\n".join("Component { id: " + name + "; Item { implicitHeight: 31 } }"
                            for name in sorted(ids - {"animeCard"}))
    height = re.search(r"^    readonly property real contentHeight:.*$", host, re.M).group()
    implicit = re.search(r"^    implicitHeight: root.contentHeight$", host, re.M).group()
    (directory / "CardHost.qml").write_text('''pragma ComponentBehavior: Bound
import QtQuick
Item {
    id: root
    width: 360
    property string kind: ""
    property bool contentActive: true
    property alias body: body
''' + height + "\n" + implicit + "\n" + loader + "\n" + wrapper + "\n" + other_cards + "\n}\n")
    (directory / "tst_anime.qml").write_text('''pragma ComponentBehavior: Bound
import QtQuick
import QtTest
import "Fixture"
TestCase {
    id: root
    name: "IrisAnimeCard"
    when: windowShown
    visible: true
    width: 640
    height: 600
    Component { id: hostComponent; CardHost {} }

    function ready(host) {
        compare(host.body.status, Loader.Ready)
        compare(host.body.asynchronous, false)
        const wrapper = host.body.item
        compare(wrapper.status, Loader.Ready)
        compare(wrapper.asynchronous, false)
        verify(wrapper.source.toString().endsWith("/IrisAnimeCard.qml"))
        const payload = wrapper.item
        verify(payload !== null)
        compare(payload.width, host.width)
        compare(wrapper.width, host.width)
        compare(wrapper.implicitHeight, payload.implicitHeight)
        compare(host.contentHeight, payload.implicitHeight)
        compare(host.implicitHeight, payload.implicitHeight)
        return payload
    }

    function test_lazy_creation_dimensions_actions_and_reopen() {
        const fetches = AnimeService.fetches
        const destructions = AnimeService.destructions
        const host = createTemporaryObject(hostComponent, root, { kind: "network" })
        verify(host !== null)
        compare(host.contentHeight, 31)
        wait(30)
        compare(AnimeService.fetches, fetches)

        host.kind = "anime"
        const first = ready(host)
        compare(AnimeService.fetches, fetches + 1)
        compare(first.shows.length, 2)
        compare(first.d, 1.25)
        tryVerify(() => host.contentHeight > 32)
        const height = host.contentHeight
        IrisStyle.density = 1.5
        compare(first.d, 1.5)
        tryVerify(() => host.contentHeight > height)
        host.width = 480
        compare(ready(host), first)
        host.contentActive = false
        wait(30)
        compare(AnimeService.fetches, fetches + 1)
        compare(AnimeService.destructions, destructions)
        const refresh = findChild(first, "refresh")
        verify(refresh !== null)
        const invalidations = AnimeService.invalidations
        refresh.clicked()
        compare(AnimeService.invalidations, invalidations + 1)
        compare(AnimeService.fetches, fetches + 2)
        const row = first.children.find(child => child.modelData?.id === 1)
        verify(row !== undefined)
        compare(row.followed, false)
        const toggles = IrisPieces.toggles
        row.clicked(null)
        compare(IrisPieces.toggles, toggles + 1)
        compare(IrisPieces.lastToggle, 1)
        compare(row.followed, true)

        host.kind = "network"
        compare(host.contentHeight, 31)
        tryCompare(AnimeService, "destructions", destructions + 1)
        host.kind = "anime"
        const second = ready(host)
        verify(second !== first)
        compare(AnimeService.fetches, fetches + 3)
        compare(second.d, 1.5)
        host.kind = ""
        compare(host.body.item, null)
        compare(host.contentHeight, 0)
        tryCompare(AnimeService, "destructions", destructions + 2)
        wait(30)
        compare(AnimeService.fetches, fetches + 3)
        IrisStyle.density = 1.25
        IrisPieces.animeFollowing = []
    }

    function test_parent_destruction_releases_payload_once() {
        const fetches = AnimeService.fetches
        const destructions = AnimeService.destructions
        const host = createTemporaryObject(hostComponent, root, { kind: "anime" })
        ready(host)
        compare(AnimeService.fetches, fetches + 1)
        host.destroy()
        tryCompare(AnimeService, "destructions", destructions + 1)
        wait(30)
        compare(AnimeService.destructions, destructions + 1)
        compare(AnimeService.fetches, fetches + 1)
    }
}
''')


def main():
    runners = [os.environ.get("QMLTESTRUNNER", ""), "/usr/lib/qt6/bin/qmltestrunner",
               "/usr/lib64/qt6/bin/qmltestrunner", "/usr/lib/x86_64-linux-gnu/qt6/bin/qmltestrunner",
               shutil.which("qmltestrunner") or ""]
    with tempfile.TemporaryDirectory(prefix="inir-anime-card-") as folder:
        directory = Path(folder)
        harness(directory)
        env = dict(os.environ, QT_QPA_PLATFORM="offscreen", QML_DISABLE_DISK_CACHE="1")
        probe = directory / "probe.qml"
        probe.write_text('import QtQuick\nimport QtTest\n'
                         'TestCase { name: "Probe"; function test_ok() { verify(true) } }\n')
        for runner in dict.fromkeys(runners):
            if not runner or not Path(runner).is_file():
                continue
            version = subprocess.run([runner, "-input", str(probe)], env=env,
                                     capture_output=True, text=True, timeout=30)
            if "Using QtTest library 6" not in version.stdout + version.stderr:
                continue
            result = subprocess.run([runner, "-input", folder], env=env,
                                    capture_output=True, text=True, timeout=30)
            output = result.stdout + result.stderr
            print(output, end="")
            return 1 if result.returncode or "FAIL!" in output else 0
    print("Iris anime card: no Qt 6 qmltestrunner found; native lifecycle not executed")
    return 0


if __name__ == "__main__":
    sys.exit(main())
