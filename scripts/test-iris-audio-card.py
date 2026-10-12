#!/usr/bin/env python3
"""Exercise the production sound/mic wrappers and shared audio payload in Qt."""
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
    payload = (STAGE / "IrisAudioCard.qml").read_text()
    loader = block(content, r"Loader\s*\{\s*id:\s*body\b")
    switch = block(loader, r"sourceComponent:\s*\{")
    ids = set(re.findall(r'case "[^"]+": return (\w+)', switch))
    wrappers = {kind: block(content, rf"Component\s*\{{\s*id:\s*{kind}Card\b")
                for kind in ("sound", "mic")}
    for kind in wrappers:
        if f'Qt.resolvedUrl("IrisAudioCard.qml")' not in wrappers[kind]:
            raise ValueError(f"{kind} must load the common audio payload URL")
    if "component LevelCard:" not in payload or payload.count("LevelCard {") != 2:
        raise ValueError("audio payload must contain one shared LevelCard and two selected uses")

    with tempfile.TemporaryDirectory(prefix="intoo-audio-card-") as temporary:
        directory = Path(temporary)
        fixture = directory / "Fixture"
        fixture.mkdir()
        singletons = {
            "IrisStyle": '''QtObject {
    property real density: 1.25
    property var identity: ({orange: "orange", indigo: "indigo"})
    property color subtext: "gray"
    property real typeLabel: 12
}''',
            "Translation": 'QtObject { function tr(value) { return value } }',
            "Config": '''QtObject {
    property var options: ({iris: {appearance: {surfaces: {cards: {header: true, devices: true, mixer: true}}}}})
}''',
            "Audio": '''QtObject {
    property bool micMuted: false
    property real micVolume: 0.35
    property real value: 0.72
    property var sink: QtObject { property var audio: QtObject { property bool muted: false } }
    property var source: QtObject { property string name: "Studio mic" }
    property var defaultSink: QtObject { property string name: "Studio output" }
    property var actions: []
    function friendlyDeviceName(device) { return device?.name ?? "" }
    function setSinkVolume(value) { actions = actions.concat(["sink:" + value]) }
    function setSourceVolume(value) { actions = actions.concat(["source:" + value]) }
    function toggleMute() { actions = actions.concat(["mute"]) }
    function toggleMicMute() { actions = actions.concat(["mic-mute"]) }
}''',
            "TestControl": '''QtObject {
    property bool emitMixerNavigate: false
    property var mixerInitial: null
    property var events: []
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
Text { property real pixelSize: 12; property int weight: Font.Normal; font.pixelSize: pixelSize }''',
            "IrisCapsuleSlider": '''import QtQuick
Item {
    property bool muted: false
    property string icon: ""
    property real value: 0
    signal moved(real next)
    signal iconClicked()
    implicitHeight: 48
}''',
            "IrisDeviceList": '''import QtQuick
Item {
    property bool outputs: false
    property bool inputs: false
    implicitHeight: 30
}''',
            "IrisSidebarSection": '''import QtQuick
import "Fixture"
Item {
    property bool bare: false
    property bool expanded: false
    property bool contentActive: true
    property string kind: ""
    signal navigate()
    implicitHeight: 54
    Component.onCompleted: {
        if (kind === "mixer") {
            TestControl.mixerInitial = [bare, expanded, contentActive]
            if (TestControl.emitMixerNavigate) navigate()
        }
    }
}''',
        }
        for name, body in stubs.items():
            (directory / f"{name}.qml").write_text(body + "\n")

        payload = re.sub(r"^import (?:qs\.|Quickshell).*$", 'import "Fixture"', payload, flags=re.M)
        payload = payload.replace("import QtQuick.Layouts\n", 'import QtQuick.Layouts\nimport "Fixture"\n')
        payload = payload.replace("IrisCapsuleSlider {", 'IrisCapsuleSlider {\n            objectName: "audioSlider"')
        payload = payload.replace("IrisDeviceList {", 'IrisDeviceList {\n            objectName: "deviceList"')
        payload = payload.replace("id: level\n", 'id: level\n        objectName: "levelCard"\n', 1)
        payload = payload.replace("CardHeader {", 'CardHeader {\n            objectName: "audioHeader"', 1)
        payload = payload.replace("IrisNumber {", 'IrisNumber {\n                objectName: "volumeNumber"', 1)
        payload = payload.replace("SectionCard {", 'SectionCard {\n                objectName: "mixerSection"')
        (directory / "IrisAudioCard.qml").write_text(payload)

        declarations = "\n".join("Component { id: " + name + "; Item { implicitHeight: 28 } }"
                                   for name in sorted(ids - {"soundCard", "micCard"}))
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
    function close(): void { root.navigate() }
    onNavigate: TestControl.events = TestControl.events.concat(["close"])
''' + loader + "\n" + wrappers["sound"] + "\n" + wrappers["mic"] + "\n" + declarations + "\n}\n"
        host = re.sub(r"^import (?:qs\.|Quickshell).*$", 'import "Fixture"', host, flags=re.M)
        (directory / "CardHost.qml").write_text(host)
        (directory / "tst_audio.qml").write_text('''pragma ComponentBehavior: Bound
import QtQuick
import QtTest
import "Fixture"
TestCase {
    id: test
    name: "IrisAudioCard"
    visible: true
    when: windowShown
    width: 700; height: 500
    Component { id: hostComponent; CardHost {} }
    function ready(host) {
        compare(host.body.status, Loader.Ready)
        const wrapper = host.body.item
        verify(wrapper !== null)
        compare(wrapper.status, Loader.Ready)
        compare(wrapper.source.toString().endsWith("/IrisAudioCard.qml"), true)
        const payload = wrapper.item
        verify(payload !== null)
        compare(payload.status, Loader.Ready)
        verify(payload.item !== null)
        return {wrapper: wrapper, payload: payload, content: payload.item}
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
        Audio.micMuted = false; Audio.micVolume = 0.35; Audio.value = 0.72
        Audio.sink.audio.muted = false
        Audio.actions = []
        Config.options = {iris: {appearance: {surfaces: {cards: {}}}}}
        TestControl.emitMixerNavigate = false
        TestControl.mixerInitial = null
        TestControl.events = []
    }
    function test_audio_bindings_controls_and_device_options() {
        const host = createTemporaryObject(hostComponent, test, {kind: "sound", contentActive: false, width: 360})
        verify(host !== null)
        const loaded = ready(host)
        compare(loaded.payload.input, false)
        compare(loaded.payload.contentActive, false)
        const slider = named(loaded.content, "audioSlider")
        const level = named(loaded.content, "levelCard")
        const header = named(loaded.content, "audioHeader")
        const number = named(loaded.content, "volumeNumber")
        const mixer = named(loaded.content, "mixerSection")
        compare(slider.value, 0.72)
        compare(slider.icon, "volume_up")
        compare(number.text, "72%")
        compare(header.detail, "Studio output")
        compare(header.visible, true)
        compare(slider.implicitHeight, 48 * IrisStyle.density)
        compare(level.spacing, 12 * IrisStyle.density)
        compare(loaded.content.spacing, 6 * IrisStyle.density)
        compare(loaded.content.width, host.width)
        compare(host.contentHeight, loaded.payload.implicitHeight)
        const devices = named(loaded.content, "deviceList")
        compare(devices.outputs, true); compare(devices.inputs, false)
        compare(devices.visible, true)
        compare(named(loaded.content, "mixerSection").visible, true)
        Config.options = {iris: {appearance: {surfaces: {cards: {header: false, devices: false, mixer: false}}}}}
        tryCompare(mixer, "visible", false)
        compare(named(loaded.content, "mixerSection"), mixer)
        tryCompare(header, "visible", false)
        compare(devices.visible, false)
        Config.options = {iris: {appearance: {surfaces: {cards: {header: true, devices: true, mixer: true}}}}}
        tryCompare(devices, "visible", true)
        tryCompare(mixer, "visible", true)
        tryCompare(header, "visible", true)
        Audio.value = 1.4
        tryCompare(slider, "value", 1)
        tryCompare(number, "text", "100%")
        Audio.value = 0.2
        tryCompare(slider, "icon", "volume_mute")
        Audio.sink.audio.muted = true
        tryCompare(slider, "muted", true)
        tryCompare(slider, "icon", "volume_off")
        slider.moved(0.61)
        slider.iconClicked()
        compare(Audio.actions.join(","), "sink:0.61,mute")
        compare(TestControl.events.join(","), "")
        host.contentActive = true
        compare(loaded.payload.contentActive, true)
        compare(mixer.contentActive, true)
        compare(host.body.item.item, loaded.payload)
    }
    function test_mic_initial_state_controls_and_mixer_absence() {
        const host = createTemporaryObject(hostComponent, test, {kind: "mic", contentActive: false})
        verify(host !== null)
        const loaded = ready(host)
        compare(loaded.payload.input, true)
        compare(loaded.payload.contentActive, false)
        compare(TestControl.mixerInitial, null)
        const slider = named(loaded.content, "audioSlider")
        const header = named(loaded.content, "audioHeader")
        const number = named(loaded.content, "volumeNumber")
        compare(slider.value, 0.35)
        compare(number.text, "35%")
        compare(header.detail, "Studio mic")
        compare(slider.icon, "mic")
        const devices = named(loaded.content, "deviceList")
        compare(devices.inputs, true); compare(devices.outputs, false)
        Audio.micMuted = true
        tryCompare(slider, "muted", true)
        tryCompare(slider, "icon", "mic_off")
        Audio.micVolume = 1.5
        tryCompare(slider, "value", 1)
        tryCompare(number, "text", "100%")
        Audio.source.name = "USB mic"
        tryCompare(header, "detail", "USB mic")
        slider.moved(0.42)
        slider.iconClicked()
        compare(Audio.actions.join(","), "source:0.42,mic-mute")
    }
    function test_sound_mixer_callback_initialization_and_card_recreation() {
        TestControl.emitMixerNavigate = true
        const host = createTemporaryObject(hostComponent, test, {kind: "sound", contentActive: false})
        verify(host !== null)
        const loaded = ready(host)
        compare(TestControl.mixerInitial.join(","), "true,true,false")
        compare(TestControl.events.join(","), "close")
        TestControl.emitMixerNavigate = false
        const payload = loaded.payload
        const mixer = named(loaded.content, "mixerSection")
        mixer.navigate()
        compare(TestControl.events.join(","), "close,close")
        host.kind = "mic"
        tryVerify(() => host.body.item.item !== payload)
        const mic = ready(host)
        compare(mic.payload.input, true)
        host.kind = "sound"
        tryVerify(() => host.body.item.item !== mic.payload)
        compare(ready(host).payload.input, false)
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
