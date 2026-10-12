#!/usr/bin/env python3
"""Exercise the deferred production mixer and its real frame with Qt 6."""
import importlib.util
import os
from pathlib import Path
import re
import shutil
import subprocess
import sys
import tempfile

ROOT = Path(__file__).resolve().parent.parent
SIDEBAR = ROOT / "modules/iris/sidebar"
spec = importlib.util.spec_from_file_location("sidebar", ROOT / "scripts/test-iris-sidebarsection-loader.py")
helper = importlib.util.module_from_spec(spec)
spec.loader.exec_module(helper)


def isolated(source):
    return re.sub(r"^import (?:qs(?:\.|\s)|Quickshell).*\n", "", source, flags=re.MULTILINE).replace(
        "import QtQuick\n", 'import QtQuick\nimport "Fixture"\n', 1)


def write_harness(directory):
    content = (SIDEBAR / "IrisSidebarSectionContent.qml").read_text()
    component = helper.block(content, r"Component\s*\{\s*id: mixerComponent\b")
    if 'setSource(Qt.resolvedUrl("IrisSidebarMixerSection.qml")' not in component or "sectionHost: root" not in component:
        raise ValueError("mixer URL boundary or initial host missing")
    if "Audio" in component or re.search(r"\bactive\s*:", component):
        raise ValueError("mixer compiles inline or is no longer resident")
    mixer = (SIDEBAR / "IrisSidebarMixerSection.qml").read_text()
    if re.search(r"\broot\b", mixer):
        raise ValueError("mixer retains an out-of-scope root")
    frame = (SIDEBAR / "IrisSidebarSectionFrame.qml").read_text()
    for name in ("header", "body", "summaryColumn"):
        frame = frame.replace("id: " + name + "\n", f'id: {name}\n        objectName: "{name}"\n')
    (directory / "IrisSidebarSectionFrame.qml").write_text(isolated(frame))
    mixer = isolated(mixer).replace("import QtQuick\n", 'import QtQuick\nimport "state.js" as State\n', 1)
    mixer = mixer.replace("id: mixer\n", "id: mixer\n    Component.onCompleted: ++State.created\n    Component.onDestruction: ++State.destroyed\n", 1)
    (directory / "IrisSidebarMixerSection.qml").write_text(mixer)
    (directory / "state.js").write_text(".pragma library\nvar created = 0\nvar destroyed = 0\nvar names = []\nvar icons = []\nvar paths = []\nvar playback = 0\n")
    fixture = directory / "Fixture"
    fixture.mkdir()
    services = {
        "IrisStyle": '''property real density: 1
property color accent: "blue"
property color text: "black"
property color muted: "gray"
property color onTint: "white"
property color surfaceHigh: "white"
property string fontNumbers: "Sans"
property real typeMeta: 12
property real typeBody: 14
property real radiusCard: 12
property real morphDuration: 80
property var morphCurve: [0.4, 0, 0.2, 1, 1, 1]
property int feedbackEasing: Easing.Linear
property var identity: ({indigo: "purple"})
function weight(value) { return value }
function secondaryOf(value) { return "gray" }
function iconRadius(value) { return value / 4 }
function duration(value) { return value }''',
        "Translation": "function tr(value) { return value }",
        "Audio": '''property var outputAppNodes: []
function appNodeDisplayName(node) { State.names.push(node); return "App " + node.label }
function togglePlayback() { ++State.playback }''',
        "MprisController": '''function streamIconName(node) { State.icons.push(node); return "icon-" + node.label }
function playPause() { ++State.playback }
function next() { ++State.playback }''',
        "Quickshell": '''function iconPath(name, fallback) { State.paths.push([name, fallback]); return name + ":" + fallback }''',
    }
    (fixture / "qmldir").write_text("module Fixture\n" + "".join(
        f"singleton {name} 1.0 {name}.qml\n" for name in services))
    for name, body in services.items():
        (fixture / (name + ".qml")).write_text('pragma Singleton\nimport QtQuick\nimport "../state.js" as State\nQtObject {\n' + body + "\n}\n")
    controls = {
        "IrisText": "Text {}",
        "MaterialSymbol": "Text { property real iconSize: 18; property real fill: 0 }",
        "IrisImage": 'Item { objectName: "image"; property string source: "" }',
        "IrisScrubber": 'Item { objectName: "scrubber"; property color fillColor; property real value: 0; signal moved(real next); implicitHeight: 20 }',
        "IrisIconButton": 'Button { property string materialIcon: ""; property bool selected: false; implicitWidth: 34; implicitHeight: 34 }',
    }
    for name, body in controls.items():
        (directory / (name + ".qml")).write_text("import QtQuick\nimport QtQuick.Controls\n" + body)
    api = content[content.index("    required property string kind"):content.index("    sourceComponent:")]
    selection = helper.block(content, r"sourceComponent:\s*\{")
    ids = re.findall(r'case "[^"]+": return (\w+)', selection)
    others = "\n".join("Component { id: " + name + "; Item { implicitHeight: 20 } }" for name in ids
                       if name != "mixerComponent")
    payload = ('pragma ComponentBehavior: Bound\nimport QtQuick\nimport "Fixture"\nLoader {\nid: root\n'
               + api + selection + component + others + "\n}\n")
    (directory / "SectionPayload.qml").write_text(payload)
    deferred = directory / "Deferred"
    deferred.mkdir()
    (deferred / "SectionPayload.qml").write_text(payload)
    (deferred / "Fixture").symlink_to(fixture, target_is_directory=True)
    (deferred / "state.js").symlink_to(directory / "state.js")
    (deferred / "IrisSidebarMixerSection.qml").write_text("import QtQuick\nItem { impossible syntax !!! }\n")
    (directory / "tst_mixer.qml").write_text('''pragma ComponentBehavior: Bound
import QtQuick
import QtTest
import "Fixture"
import "Deferred" as Deferred
import "state.js" as State
TestCase {
    id: test
    name: "IrisSidebarMixerSection"
    visible: true
    width: 700; height: 900
    when: windowShown
    Component { id: payload; SectionPayload {} }
    Component { id: boundary; Deferred.SectionPayload {} }
    Component {
        id: audioComponent
        QtObject { property real volume: 0.5; property bool muted: false }
    }
    Component {
        id: nodeComponent
        QtObject { property string label; property var audio }
    }
    function node(label, volume = 0.5, muted = false) {
        const audio = createTemporaryObject(audioComponent, test, {volume: volume, muted: muted})
        return createTemporaryObject(nodeComponent, test, {label: label, audio: audio})
    }
    function create(expanded = true) {
        const value = createTemporaryObject(payload, test, {kind: "mixer", expanded: expanded, width: 420})
        verify(value !== null); compare(value.status, Loader.Ready)
        return value
    }
    function ready(value) {
        compare(value.item.status, Loader.Ready); verify(value.item.item !== null)
        return value.item.item
    }
    function descendants(item) {
        let result = []
        for (const child of item.children) result = result.concat([child], descendants(child))
        return result
    }
    function rows(mixer) { return descendants(mixer).filter(child => child.modelData !== undefined && child.muted !== undefined) }
    function mute(row) { return descendants(row).find(child => child.materialIcon !== undefined) }
    function scrubber(row) { return findChild(row, "scrubber") }
    function images(item) { return descendants(item).filter(child => child.objectName === "image") }
    function init() {
        Audio.outputAppNodes = []
        IrisStyle.density = 1
        State.names = []; State.icons = []; State.paths = []; State.playback = 0
    }
    function test_compile_boundary() {
        const value = createTemporaryObject(boundary, test, {kind: "media"})
        verify(value !== null); compare(value.status, Loader.Ready); compare(value.item.implicitHeight, 20)
        ignoreWarning(/.*IrisSidebarMixerSection.qml.*Expected token.*/)
        value.kind = "mixer"
        compare(value.item.status, Loader.Error); compare(value.item.item, null)
    }
    function test_filter_empty_two_nine_summary_names_and_icons() {
        const value = create(false), mixer = ready(value)
        compare(mixer.streams.length, 0); compare(mixer.detail, ""); compare(rows(mixer).length, 0)
        const summary = findChild(mixer, "summaryColumn")
        compare(images(summary).length, 0)
        verify(descendants(summary).some(child => child.text === "No app is playing sound." && child.visible))
        const first = node("alpha"), second = node("beta")
        const invalid = createTemporaryObject(nodeComponent, test, {label: "invalid", audio: null})
        Audio.outputAppNodes = [null, first, invalid, second]
        compare(mixer.streams.length, 2); compare(mixer.detail, "2"); compare(rows(mixer).length, 2)
        compare(images(summary).length, 2)
        compare(rows(mixer)[0].modelData, first); compare(rows(mixer)[1].modelData, second)
        for (const [index, item] of [first, second].entries()) {
            const row = rows(mixer)[index]
            verify(descendants(row).some(child => child.text === "App " + item.label))
            compare(images(row)[0].source, "icon-" + item.label + ":audio-x-generic")
            compare(images(summary)[index].source, images(row)[0].source)
            verify(State.names.includes(item)); verify(State.icons.includes(item))
        }
        verify(!State.names.includes(invalid)); verify(!State.icons.includes(invalid))
        verify(State.paths.every(call => call[1] === "audio-x-generic"))
        const many = []
        for (let i = 0; i < 9; ++i) many.push(node(String(i)))
        Audio.outputAppNodes = many
        compare(rows(mixer).length, 9); compare(images(summary).length, 8); compare(mixer.detail, "9")
        compare(images(summary)[7].source, "icon-7:audio-x-generic")
        Audio.outputAppNodes = []
        compare(rows(mixer).length, 0); compare(images(summary).length, 0)
    }
    function test_external_audio_updates_and_actual_handlers() {
        const first = node("first", 0.2), second = node("second", 0.8, true)
        Audio.outputAppNodes = [first, second]
        const value = create(), mixer = ready(value)
        const a = rows(mixer)[0], b = rows(mixer)[1]
        compare(scrubber(a).value, 0.2); compare(scrubber(b).value, 0.8)
        compare(mute(b).materialIcon, "volume_off"); compare(mute(b).selected, true)
        compare(images(b)[0].opacity, 0.45); compare(scrubber(b).fillColor, IrisStyle.muted)
        first.audio.volume = 2; compare(scrubber(a).value, 1)
        first.audio.volume = 0.4; compare(scrubber(a).value, 0.4)
        first.audio.muted = true
        compare(a.muted, true); compare(mute(a).selected, true); compare(images(a)[0].opacity, 0.45)
        scrubber(a).moved(0.7)
        compare(first.audio.volume, 0.7); compare(second.audio.volume, 0.8)
        mute(a).clicked()
        compare(first.audio.muted, false); compare(second.audio.muted, true)
        compare(mute(a).materialIcon, "volume_up"); compare(images(a)[0].opacity, 1)
        scrubber(b).moved(0.3); mute(b).clicked()
        compare(second.audio.volume, 0.3); compare(second.audio.muted, false)
        compare(first.audio.volume, 0.7); compare(first.audio.muted, false)
        const invalid = createTemporaryObject(nodeComponent, test, {label: "gone", audio: null})
        a.modelData = invalid
        compare(scrubber(a).value, 0); compare(a.muted, false)
        scrubber(a).moved(0.6); mute(a).clicked()
        compare(invalid.audio, null); compare(first.audio.volume, 0.7); compare(second.audio.volume, 0.3)
        compare(State.playback, 0)
    }
    function test_geometry_residency_and_recreation() {
        const first = node("persistent", 0.25)
        Audio.outputAppNodes = [first]
        const value = create(false), mixer = ready(value)
        compare(mixer.sectionHost, value); compare(mixer.width, 420)
        wait(250); compare(value.implicitHeight, mixer.implicitHeight)
        const collapsed = mixer.implicitHeight
        const header = findChild(mixer, "header")
        let toggles = 0
        value.toggleRequested.connect(() => ++toggles)
        header.clicked(null); compare(toggles, 1)
        value.expanded = true
        tryCompare(value, "morphing", false, 1000)
        wait(250); verify(mixer.implicitHeight > collapsed)
        value.width = 500; compare(mixer.width, 500)
        value.contentActive = false; compare(ready(value), mixer)
        value.expanded = false; compare(ready(value), mixer)
        value.bare = true; compare(mixer.open, true); compare(mixer.radius, 0); compare(header.enabled, false)
        IrisStyle.density = 1.5
        compare(mixer.pad, 21); compare(images(findChild(mixer, "summaryColumn"))[0].width, 33)
        const created = State.created, destroyed = State.destroyed
        first.audio.volume = 0.9; first.audio.muted = true
        value.kind = "media"
        wait(0); compare(State.destroyed, destroyed + 1)
        compare(first.audio.volume, 0.9); compare(first.audio.muted, true)
        value.kind = "mixer"
        const reopened = ready(value)
        compare(State.created, created + 1); compare(reopened.sectionHost, value)
        compare(scrubber(rows(reopened)[0]).value, 0.9); compare(mute(rows(reopened)[0]).selected, true)
        compare(State.playback, 0)
    }
}
''')


def main():
    runners = [os.environ.get("QMLTESTRUNNER", ""), "/usr/lib/qt6/bin/qmltestrunner",
               "/usr/lib64/qt6/bin/qmltestrunner", "/usr/lib/x86_64-linux-gnu/qt6/bin/qmltestrunner",
               shutil.which("qmltestrunner") or ""]
    with tempfile.TemporaryDirectory(prefix="inir-mixer-section-") as folder:
        directory = Path(folder)
        write_harness(directory)
        environment = dict(os.environ, QT_QPA_PLATFORM="offscreen", QML_DISABLE_DISK_CACHE="1")
        probe = directory / "probe.qml"
        probe.write_text('import QtQuick\nimport QtTest\nTestCase { name: "Probe"; function test_ok() { verify(true) } }\n')
        for runner in dict.fromkeys(runners):
            if not runner or not Path(runner).is_file():
                continue
            version = subprocess.run([runner, "-input", str(probe)], env=environment,
                                     capture_output=True, text=True, timeout=30)
            if "Using QtTest library 6" not in version.stdout + version.stderr:
                continue
            result = subprocess.run([runner, "-input", str(directory / "tst_mixer.qml")],
                                    env=environment, capture_output=True, text=True, timeout=30)
            output = result.stdout + result.stderr
            print(output, end="")
            return 1 if result.returncode or "FAIL!" in output else 0
    print("Iris mixer: no Qt 6 qmltestrunner found; native checks not executed")
    return 0


if __name__ == "__main__":
    sys.exit(main())
