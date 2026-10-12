#!/usr/bin/env python3
"""Exercise real focus/frame/ring and System lifecycles using Qt 6 fixtures."""
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
    focus_component = helper.block(content, r"Component\s*\{\s*id: focusComponent\b")
    system_component = helper.block(content, r"Component\s*\{\s*id: systemComponent\b")
    adapter = helper.block(content, r"component Section: IrisSidebarSectionFrame\s*\{")
    if 'setSource(Qt.resolvedUrl("IrisSidebarFocusSection.qml")' not in focus_component or "sectionHost: root" not in focus_component:
        raise ValueError("focus URL boundary or initial host missing")
    if "TimerService" in focus_component or re.search(r"\bactive\s*:", focus_component):
        raise ValueError("focus compiles inline or no longer stays resident")
    focus = (SIDEBAR / "IrisSidebarFocusSection.qml").read_text()
    ring = (SIDEBAR / "IrisSidebarGaugeRing.qml").read_text()
    frame = (SIDEBAR / "IrisSidebarSectionFrame.qml").read_text()
    if re.search(r"\broot\b", focus + ring):
        raise ValueError("extracted section retains an out-of-scope root")
    module_definition = (SIDEBAR / "qmldir").read_text()
    for registration in (
        "IrisSidebarSectionFrame 1.0 IrisSidebarSectionFrame.qml",
        "IrisSidebarGaugeRing 1.0 IrisSidebarGaugeRing.qml",
    ):
        if registration not in module_definition:
            raise ValueError(f"shared sidebar type not registered: {registration}")
    system = (SIDEBAR / "IrisSidebarSystemSection.qml").read_text()
    if 'setSource(Qt.resolvedUrl("IrisSidebarSystemSection.qml")' not in system_component:
        raise ValueError("system URL boundary missing")
    if "density: systemCard.sectionHost.d" not in system:
        raise ValueError("system ring lost host density")
    fixture = directory / "Fixture"
    fixture.mkdir()
    services = {
        "IrisStyle": '''property real density: 1
property color accent: "blue"
property color text: "black"
property color muted: "gray"
property color onTint: "white"
property color secondaryAccent: "purple"
property color success: "green"
property color danger: "red"
property color surfaceHigh: "white"
property color fillHover: "lightgray"
property string fontNumbers: "Sans"
property real typeScale: 1
property real typeMeta: 12
property real typeBody: 14
property real typeLabel: 13
property real typeFootnote: 11
property real radiusCard: 12
property real morphDuration: 80
property var morphCurve: [0.4, 0, 0.2, 1, 1, 1]
property int feedbackEasing: Easing.Linear
property var identity: ({orange: "orange", teal: "teal"})
function weight(value) { return value }
function secondaryOf(value) { return "gray" }
function iconRadius(value) { return value / 4 }
function duration(value) { return value }
function tintFill(value) { return "lightgray" }''',
        "Translation": 'function tr(value) { return value }',
        "TimerService": '''property bool pomodoroRunning: true
property bool pomodoroPaused: false
property bool pomodoroLongBreak: false
property bool pomodoroBreak: false
property int pomodoroSecondsLeft: 120
property int pomodoroLapDuration: 300
property int cyclesBeforeLongBreak: 4
property int pomodoroCycle: 2
property int toggled: 0
property int resets: 0
function togglePomodoro() { ++toggled }
function resetPomodoro() { ++resets }''',
        "ResourceUsage": '''property real cpuUsage: 0.5
property real memoryUsedPercentage: 0.4
property real tempPercentage: 0.2
property real diskUsedPercentage: 0.8
property int maxTemp: 50
property int tempWarningThreshold: 85
property int acquired: 0
property int released: 0
function keepAlive() { ++acquired }
function releaseKeepAlive() { ++released }'''
    }
    (fixture / "qmldir").write_text("module Fixture\n" + "".join(
        f"singleton {name} 1.0 {name}.qml\n" for name in services))
    for name, body in services.items():
        (fixture / (name + ".qml")).write_text("pragma Singleton\nimport QtQuick\nQtObject {\n" + body + "\n}\n")
    controls = {
        "IrisText": "Text {}",
        "MaterialSymbol": "Text { property real iconSize: 18; property real fill: 0 }",
        "IrisButton": "Button { property bool emphasized: false; property real buttonRadius: 0; property real buttonRadiusPressed: 0 }",
        "IrisIconButton": 'Button { property string materialIcon: ""; implicitWidth: 34; implicitHeight: 34 }',
    }
    for name, body in controls.items():
        (directory / (name + ".qml")).write_text("import QtQuick\nimport QtQuick.Controls\n" + body)
    for name in ("header", "body", "summaryColumn"):
        frame = frame.replace("id: " + name + "\n", f'id: {name}\n        objectName: "{name}"\n')
    # Names and counters expose production objects without replacing their handlers.
    ring = ring.replace("id: gaugeRing\n", 'id: gaugeRing\n    objectName: "gaugeRing"\n')
    focus = isolated(focus).replace('import QtQuick\n', 'import QtQuick\nimport "state.js" as State\n', 1)
    focus = focus.replace("id: focusTimer\n", "id: focusTimer\n    Component.onCompleted: ++State.created\n    Component.onDestruction: ++State.destroyed\n", 1)
    (directory / "state.js").write_text(".pragma library\nvar created = 0\nvar destroyed = 0\n")
    (directory / "IrisSidebarFocusSection.qml").write_text(focus)
    (directory / "IrisSidebarSystemSection.qml").write_text(isolated(system))
    (directory / "IrisSidebarSectionFrame.qml").write_text(isolated(frame))
    (directory / "IrisSidebarGaugeRing.qml").write_text(isolated(ring))
    api = content[content.index("    required property string kind"):content.index("    sourceComponent:")]
    selection = helper.block(content, r"sourceComponent:\s*\{")
    ids = re.findall(r'case "[^"]+": return (\w+)', selection)
    others = "\n".join("Component { id: " + name + "; Item { implicitHeight: 20 } }" for name in ids
                       if name not in ("focusComponent", "systemComponent"))
    payload = ('pragma ComponentBehavior: Bound\nimport QtQuick\nimport QtQuick.Layouts\nimport "Fixture"\nLoader {\nid: root\n'
               + api + selection + focus_component + system_component + adapter + others + "\n}\n")
    (directory / "SectionPayload.qml").write_text(payload)
    deferred = directory / "Deferred"
    deferred.mkdir()
    (deferred / "SectionPayload.qml").write_text(payload)
    (deferred / "Fixture").symlink_to(fixture, target_is_directory=True)
    for name in ("IrisSidebarSectionFrame", "IrisSidebarGaugeRing", "IrisSidebarSystemSection", *controls):
        (deferred / (name + ".qml")).symlink_to(directory / (name + ".qml"))
    (deferred / "IrisSidebarFocusSection.qml").write_text("import QtQuick\nItem { impossible syntax !!! }\n")
    (directory / "tst_focus.qml").write_text('''pragma ComponentBehavior: Bound
import QtQuick
import QtQuick.Shapes
import QtTest
import "Fixture"
import "Deferred" as Deferred
import "state.js" as State
TestCase {
    id: test
    name: "IrisSidebarFocusSection"
    visible: true
    width: 700; height: 900
    when: windowShown
    Component { id: payload; SectionPayload {} }
    Component { id: boundary; Deferred.SectionPayload {} }
    Component { id: ringComponent; IrisSidebarGaugeRing {} }
    function create(kind = "focus", expanded = true) {
        const value = createTemporaryObject(payload, test, {kind: kind, expanded: expanded, width: 420})
        verify(value !== null)
        compare(value.status, Loader.Ready)
        return value
    }
    function ready(value) {
        compare(value.item.status, Loader.Ready)
        verify(value.item.item !== null)
        return value.item.item
    }
    function descendants(item) {
        let result = []
        for (const child of item.children) result = result.concat([child], descendants(child))
        return result
    }
    function rings(item) { return descendants(item).filter(child => child.objectName === "gaugeRing") }
    function button(item, property, expected) {
        const found = descendants(item).find(child => child[property] === expected)
        verify(found !== undefined, "missing action " + expected)
        return found
    }
    function init() {
        IrisStyle.density = 1
        TimerService.pomodoroRunning = true; TimerService.pomodoroPaused = false
        TimerService.pomodoroBreak = false; TimerService.pomodoroLongBreak = false
        TimerService.pomodoroSecondsLeft = 120; TimerService.pomodoroLapDuration = 300
        TimerService.toggled = 0; TimerService.resets = 0
        ResourceUsage.acquired = 0; ResourceUsage.released = 0
    }
    function test_compile_boundary() {
        const value = createTemporaryObject(boundary, test, {kind: "media"})
        verify(value !== null)
        compare(value.status, Loader.Ready)
        compare(value.item.implicitHeight, 20)
        ignoreWarning(/.*IrisSidebarFocusSection.qml.*Expected token.*/)
        value.kind = "focus"
        compare(value.item.status, Loader.Error)
        compare(value.item.item, null)
    }
    function test_phase_clock_progress_and_actions() {
        const value = create("focus", false)
        const focus = ready(value)
        compare(focus.sectionHost, value)
        compare(focus.clock, "2:00"); compare(focus.detail, "2:00")
        compare(focus.phase, "Focus"); compare(focus.ticking, true)
        compare(focus.progress, 0.6)
        const summary = findChild(focus, "summaryColumn")
        const clockText = button(summary, "text", "2:00")
        compare(clockText.color, focus.phaseTint)
        compare(button(summary, "text", "Focus").text, "Focus")
        const dots = descendants(focus).filter(child => child.index !== undefined)
        compare(dots.length, 4)
        for (let i = 0; i < dots.length; ++i) {
            compare(dots[i].width, 6)
            compare(dots[i].color, i < 2 ? IrisStyle.secondaryAccent : IrisStyle.fillHover)
        }
        const rs = rings(focus)
        compare(rs.length, 2)
        for (const ring of rs) compare(ring.progress, 0.6)
        button(focus, "materialIcon", "pause").clicked()
        compare(TimerService.toggled, 1)
        TimerService.pomodoroPaused = true
        compare(focus.ticking, false)
        compare(clockText.color, IrisStyle.text)
        button(focus, "materialIcon", "play_arrow").clicked()
        compare(TimerService.toggled, 2)
        value.expanded = true
        compare(focus.detail, "")
        button(focus, "text", "Start").clicked()
        compare(TimerService.toggled, 3)
        button(focus, "text", "Reset").clicked()
        compare(TimerService.resets, 1)
        TimerService.pomodoroPaused = false
        button(focus, "text", "Pause").clicked()
        compare(TimerService.toggled, 4)
        TimerService.pomodoroBreak = true
        compare(focus.phase, "Break"); compare(focus.phaseTint, IrisStyle.success)
        compare(button(summary, "text", "Break").text, "Break")
        for (const ring of rs) compare(ring.tint, IrisStyle.success)
        TimerService.pomodoroLongBreak = true
        compare(focus.phase, "Long break")
        compare(button(summary, "text", "Long break").text, "Long break")
        TimerService.pomodoroSecondsLeft = 61
        compare(focus.clock, "1:01")
        TimerService.pomodoroRunning = false
        value.expanded = false
        compare(focus.detail, ""); compare(focus.ticking, false)
        compare(button(focus, "materialIcon", "play_arrow").materialIcon, "play_arrow")
    }
    function test_ring_geometry_and_clamp() {
        const ring = createTemporaryObject(ringComponent, test, {width: 100, height: 100})
        verify(ring !== null)
        compare(ring.preferredRendererType, Shape.CurveRenderer)
        compare(ring.stroke, 5); compare(ring.start, -90)
        compare(ring.data.length, 2)
        const track = ring.data[0], fill = ring.data[1]
        compare(track.strokeWidth, 5); compare(track.fillColor, "#00000000")
        compare(fill.capStyle, ShapePath.RoundCap)
        const arc = fill.pathElements[0]
        compare(arc.centerX, 50); compare(arc.radiusX, 47.5)
        compare(arc.startAngle, -90); compare(arc.sweepAngle, 0)
        ring.progress = 0.5; compare(arc.sweepAngle, 180)
        ring.progress = -0.5; compare(arc.sweepAngle, 0)
        ring.progress = 1.5; compare(arc.sweepAngle, 360)
        ring.start = 12; compare(arc.startAngle, 12)
        ring.tint = "red"; compare(fill.strokeColor, "#ff0000")
        ring.density = 1.5; compare(ring.stroke, 8); compare(arc.radiusX, 46)
        const value = create()
        const focus = ready(value)
        TimerService.pomodoroLapDuration = 0
        compare(focus.progress, 0)
        for (const r of rings(focus)) compare(r.data[1].pathElements[0].sweepAngle, 0)
        TimerService.pomodoroLapDuration = 100; TimerService.pomodoroSecondsLeft = 200
        compare(focus.progress, -1)
        for (const r of rings(focus)) compare(r.data[1].pathElements[0].sweepAngle, 0)
        TimerService.pomodoroSecondsLeft = -50
        compare(focus.progress, 1.5)
        for (const r of rings(focus)) compare(r.data[1].pathElements[0].sweepAngle, 360)
    }
    function test_geometry_morph_toggle_residency() {
        const value = create("focus", false)
        const focus = ready(value)
        wait(250)
        compare(focus.width, 420); compare(value.item.width, 420)
        compare(value.implicitHeight, focus.implicitHeight)
        const collapsed = focus.implicitHeight
        let toggles = 0
        value.toggleRequested.connect(() => ++toggles)
        const header = findChild(focus, "header")
        header.clicked(null); compare(toggles, 1)
        value.expanded = true
        tryCompare(value, "morphing", true)
        tryCompare(value, "morphing", false, 1000)
        verify(focus.implicitHeight > collapsed)
        value.width = 500; compare(focus.width, 500)
        value.contentActive = false; compare(ready(value), focus)
        value.expanded = false; compare(ready(value), focus)
        value.expanded = true; compare(ready(value), focus)
        value.bare = true; compare(focus.radius, 0); compare(header.enabled, false)
        IrisStyle.density = 1.5; compare(focus.pad, 21)
        const rs = rings(focus)
        compare(rs[0].stroke, 5); compare(rs[1].stroke, 12)
        for (const ring of rs) compare(ring.density, 1.5)
        compare(TimerService.toggled, 0); compare(TimerService.resets, 0)
    }
    function test_recreation_keeps_service_state() {
        const value = create()
        const created = State.created, destroyed = State.destroyed
        TimerService.pomodoroPaused = true; TimerService.pomodoroSecondsLeft = 17
        value.kind = "media"
        wait(0); compare(State.destroyed, destroyed + 1)
        value.kind = "focus"
        const focus = ready(value)
        compare(State.created, created + 1)
        compare(focus.clock, "0:17"); compare(focus.ticking, false)
        compare(TimerService.toggled, 0); compare(TimerService.resets, 0)
    }
    function test_system_shared_rings_and_polling() {
        const value = create("system", false)
        const system = ready(value)
        compare(system.sectionHost, value); compare(system.open, true)
        compare(ResourceUsage.acquired, 1); compare(ResourceUsage.released, 0)
        const rs = rings(system)
        compare(rs.length, 4)
        compare(rs[0].progress, 0.5); compare(rs[1].progress, 0.4)
        compare(rs[2].progress, 0.2); compare(rs[3].progress, 0.8)
        IrisStyle.density = 1.5
        for (const ring of rs) { compare(ring.density, 1.5); compare(ring.stroke, 8) }
        ResourceUsage.cpuUsage = 2
        compare(rings(system)[0].progress, 1); compare(rings(system)[0].tint, IrisStyle.danger)
        ResourceUsage.memoryUsedPercentage = -2
        compare(rings(system)[1].progress, 0)
        value.contentActive = false
        compare(ResourceUsage.released, 1); compare(ready(value), system)
        value.expanded = true; compare(ResourceUsage.released, 1)
        value.contentActive = true; compare(ResourceUsage.acquired, 2)
        value.kind = "media"
        wait(0); compare(ResourceUsage.released, 2)
        value.kind = "system"; compare(ResourceUsage.acquired, 3)
        value.kind = "media"; wait(0); compare(ResourceUsage.released, 3)
    }
}
''')


def main():
    runners = [os.environ.get("QMLTESTRUNNER", ""), "/usr/lib/qt6/bin/qmltestrunner",
               "/usr/lib64/qt6/bin/qmltestrunner", "/usr/lib/x86_64-linux-gnu/qt6/bin/qmltestrunner",
               shutil.which("qmltestrunner") or ""]
    with tempfile.TemporaryDirectory(prefix="inir-focus-section-") as folder:
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
            result = subprocess.run([runner, "-input", str(directory / "tst_focus.qml")],
                                    env=environment, capture_output=True, text=True, timeout=30)
            output = result.stdout + result.stderr
            print(output, end="")
            return 1 if result.returncode or "FAIL!" in output else 0
    print("Iris focus: no Qt 6 qmltestrunner found; native checks not executed")
    return 0


if __name__ == "__main__":
    sys.exit(main())
