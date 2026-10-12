#!/usr/bin/env python3
"""Native Qt checks of deferred task payload, frame and real handlers.

Styled controls and Todo are fixtures; production filtering, original-index action
routing, residency, size animation and URL selection execute unchanged.
"""
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
spec = importlib.util.spec_from_file_location("sidebar_loader", ROOT / "scripts/test-iris-sidebarsection-loader.py")
helper = importlib.util.module_from_spec(spec)
spec.loader.exec_module(helper)


def isolated(source):
    return re.sub(r"^import (?:qs(?:\.|\s)|Quickshell).*\n", "", source,
                  flags=re.MULTILINE).replace("import QtQuick\n", 'import QtQuick\nimport "Fixture"\n', 1)


def write_harness(directory):
    content = (SIDEBAR / "IrisSidebarSectionContent.qml").read_text()
    component = helper.block(content, r"Component\s*\{\s*id: tasksComponent\b")
    if "taskInput" in content or "showCompleted" in content:
        raise ValueError("tasks implementation remains inline")
    if 'setSource(Qt.resolvedUrl("IrisSidebarTasksSection.qml")' not in component:
        raise ValueError("missing deferred tasks URL")
    if "sectionHost: root" not in component or re.search(r"\b(?:active|asynchronous|expanded|contentActive)\s*:", component):
        raise ValueError("tasks payload must load synchronously with host and remain resident")
    frame = (SIDEBAR / "IrisSidebarSectionFrame.qml").read_text()
    section = (SIDEBAR / "IrisSidebarTasksSection.qml").read_text()
    if re.search(r"\broot\b", section):
        raise ValueError("external tasks references former root")
    fixture = directory / "Fixture"
    fixture.mkdir()
    services = {
        "IrisStyle": '''property real density: 1
property color accent: "blue"
property color text: "black"
property color muted: "gray"
property color onTint: "white"
property color surfaceHigh: "white"
property color surfaceHighest: "white"
property color fillHover: "lightgray"
property string fontNumbers: "Sans"
property real typeMeta: 12
property real typeBody: 14
property real typeLabel: 13
property real radiusCard: 12
property real radiusRow: 8
property real morphDuration: 80
property var morphCurve: [0.4, 0, 0.2, 1, 1, 1]
property int feedbackEasing: Easing.Linear
property var identity: ({orange: "orange"})
function weight(value) { return value }
function secondaryOf(value) { return "gray" }
function strongOf(value) { return "black" }
function iconRadius(value) { return value / 4 }
function duration(value) { return value }''',
        "Translation": "function tr(value) { return value }",
        "Todo": """property var list: []
property var calls: []
function addTask(value) { calls = [...calls, ["add", value]]; list = [...list, {content: value, done: false}] }
function markDone(index) {
    calls = [...calls, ["done", index]]
    const next = list.slice(); next[index] = Object.assign({}, next[index], {done: true}); list = next
}
function markUnfinished(index) {
    calls = [...calls, ["unfinished", index]]
    const next = list.slice(); next[index] = Object.assign({}, next[index], {done: false}); list = next
}""",
    }
    (fixture / "qmldir").write_text("module Fixture\n" + "".join(
        f"singleton {name} 1.0 {name}.qml\n" for name in services))
    for name, body in services.items():
        (fixture / (name + ".qml")).write_text("pragma Singleton\nimport QtQuick\nQtObject {\n" + body + "\n}\n")
    stubs = {
        "IrisText": 'Text { enum Role { Body, Meta }\nproperty int role: 0 }',
        "MaterialSymbol": 'Text { property real iconSize: 18; property real fill: 0 }',
        "IrisIconButton": 'Button { property string materialIcon: ""; property bool selected: false; property real iconSize: 18; implicitWidth: 34; implicitHeight: 34 }',
        "IrisField": 'TextField {}',
    }
    for name, body in stubs.items():
        (directory / (name + ".qml")).write_text("import QtQuick\nimport QtQuick.Controls\n" + body)
    for name in ("header", "body", "summaryColumn"):
        frame = frame.replace("id: " + name + "\n", f'id: {name}\n        objectName: "{name}"\n')
    for name in ("taskInput", "taskRow", "pendingRow"):
        section = section.replace("id: " + name + "\n", f'id: {name}\n        objectName: "{name}"\n')
    (directory / "IrisSidebarSectionFrame.qml").write_text(isolated(frame))
    (directory / "state.js").write_text(".pragma library\nvar created = 0\nvar destroyed = 0\n")
    section = isolated(section).replace('import QtQuick\n', 'import QtQuick\nimport "state.js" as State\n', 1)
    section = section.replace("id: tasks\n", "id: tasks\n    Component.onCompleted: ++State.created\n    Component.onDestruction: ++State.destroyed\n", 1)
    (directory / "IrisSidebarTasksSection.qml").write_text(section)
    api = content[content.index("    required property string kind"):content.index("    sourceComponent:")]
    selection = helper.block(content, r"sourceComponent:\s*\{")
    ids = re.findall(r'case "[^"]+": return (\w+)', selection)
    others = "\n".join("Component { id: " + identifier + "; Item { implicitHeight: 20 } }"
                       for identifier in ids if identifier != "tasksComponent")
    payload = ('pragma ComponentBehavior: Bound\nimport QtQuick\nimport "Fixture"\nLoader {\nid: root\n'
               + api + selection + component + others + "\n}\n")
    (directory / "SectionPayload.qml").write_text(payload)
    deferred = directory / "Deferred"
    deferred.mkdir()
    (deferred / "Fixture").symlink_to(fixture, target_is_directory=True)
    (deferred / "SectionPayload.qml").write_text(payload)
    (deferred / "IrisSidebarTasksSection.qml").write_text("import QtQuick\nItem { impossible syntax !!! }\n")
    (directory / "tst_tasks.qml").write_text(QML_TEST)


QML_TEST = '''pragma ComponentBehavior: Bound
import QtQuick
import QtTest
import "Fixture"
import "Deferred" as Deferred
import "state.js" as State
TestCase {
    id: test
    name: "IrisSidebarTasksSection"
    visible: true; width: 700; height: 900
    when: windowShown
    Component { id: payload; SectionPayload {} }
    Component { id: boundary; Deferred.SectionPayload {} }
    function create(expanded = true) {
        const value = createTemporaryObject(payload, test, {kind: "tasks", width: 420, expanded: expanded})
        verify(value !== null); compare(value.status, Loader.Ready)
        compare(value.item.status, Loader.Ready); verify(value.item.item !== null)
        compare(value.item.item.sectionHost, value)
        return value
    }
    function section(value) { return value.item.item }
    function descendants(item) {
        let result = []
        for (const child of item.children) result = result.concat([child], descendants(child))
        return result
    }
    function button(item, icon) {
        const found = descendants(item).find(child => child.materialIcon === icon)
        verify(found !== undefined, "missing handler " + icon)
        return found
    }
    function rows(item) { return descendants(item).filter(child => child.objectName === "taskRow") }
    function pendingRows(item) { return descendants(item).filter(child => child.objectName === "pendingRow") }
    function tasksData() {
        return [{content: "Done zero", done: true}, {content: "Pending one", done: false},
                {content: "Done two", done: true}, {content: "Pending three", done: false}]
    }
    function init() { Todo.list = []; Todo.calls = []; IrisStyle.density = 1 }
    function test_compile_boundary() {
        const value = createTemporaryObject(boundary, test, {kind: "media"})
        verify(value !== null); compare(value.status, Loader.Ready); compare(value.item.implicitHeight, 20)
        ignoreWarning(/.*IrisSidebarTasksSection.qml.*Expected token.*/)
        value.kind = "tasks"
        compare(value.item.status, Loader.Error); compare(value.item.item, null)
    }
    function test_filtered_indices_and_completion_actions() {
        Todo.list = tasksData()
        const value = create(); const body = section(value)
        compare(Todo.calls.length, 0); compare(body.detail, "2")
        compare(body.pending.length, 2); compare(body.entries.length, 2)
        compare(rows(body).length, 2); compare(pendingRows(body).length, 2)
        compare(rows(body)[0].modelData.index, 1); compare(rows(body)[1].modelData.index, 3)
        compare(pendingRows(body)[0].modelData.task.content, "Pending one")
        button(rows(body)[0], "radio_button_unchecked").clicked()
        compare(rows(body)[0].modelData.index, 3)
        button(rows(body)[0], "radio_button_unchecked").clicked()
        compare(JSON.stringify(Todo.calls), JSON.stringify([["done", 1], ["done", 3]]))
        compare(rows(body).length, 0); compare(body.detail, "")
        Todo.list = tasksData(); Todo.calls = []
        button(body, "done_all").clicked()
        compare(body.showCompleted, true); compare(button(body, "done_all").selected, true)
        compare(rows(body).length, 4)
        compare(rows(body).map(row => row.modelData.index).join(","), "0,1,2,3")
        button(rows(body)[0], "check_circle").clicked()
        button(rows(body)[2], "check_circle").clicked()
        compare(JSON.stringify(Todo.calls), JSON.stringify([["unfinished", 0], ["unfinished", 2]]))
    }
    function test_enter_trim_and_add_focus() {
        const value = create(); const body = section(value)
        const input = findChild(body, "taskInput"); const add = button(body, "add")
        input.text = "   "; compare(add.enabled, false)
        input.forceActiveFocus(); keyClick(Qt.Key_Return)
        compare(Todo.calls.length, 0); compare(input.text, "   ")
        mouseClick(add); compare(Todo.calls.length, 0)
        input.text = "  A new task  "; compare(add.enabled, true)
        keyClick(Qt.Key_Return)
        compare(JSON.stringify(Todo.calls), JSON.stringify([["add", "A new task"]]))
        compare(input.text, ""); compare(input.activeFocus, true)
        // The accepted handler itself never requests focus; button addition does.
        const show = button(body, "done_all"); show.forceActiveFocus()
        input.text = " Signal "; input.accepted()
        compare(show.activeFocus, true); compare(input.activeFocus, false)
        input.text = "  From button  "; add.clicked()
        compare(input.text, ""); compare(input.activeFocus, true)
        compare(JSON.stringify(Todo.calls), JSON.stringify([["add", "A new task"], ["add", "Signal"], ["add", "From button"]]))
    }
    function test_assignment_updates_empty_labels_without_writes() {
        const value = create(); const body = section(value)
        const summary = findChild(body, "summaryColumn"); const content = findChild(body, "body")
        const shortEmpty = descendants(summary).find(child => child.text === "A clear list.")
        const longEmpty = descendants(content).find(child => child.text === "A clear list. Room for what matters next.")
        verify(shortEmpty !== undefined); verify(longEmpty !== undefined)
        compare(shortEmpty.visible, false) // summary hidden when expanded
        compare(longEmpty.visible, true)
        value.expanded = false; wait(300); compare(shortEmpty.visible, true)
        Todo.list = tasksData(); compare(body.pending.length, 2); compare(rows(body).length, 2)
        compare(shortEmpty.visible, false); compare(longEmpty.visible, false)
        Todo.list = [{content: "Only complete", done: true}]
        compare(body.detail, ""); compare(body.pending.length, 0); compare(body.entries.length, 0)
        compare(shortEmpty.visible, true)
        value.expanded = true; wait(300); compare(longEmpty.visible, true)
        button(body, "done_all").clicked(); compare(body.entries.length, 1)
        compare(longEmpty.visible, false)
        Todo.list = []; compare(body.entries.length, 0); compare(longEmpty.visible, true)
        compare(Todo.calls.length, 0)
    }
    function test_frame_residency_and_kind_reset() {
        Todo.list = tasksData()
        const value = create(false); const body = section(value)
        wait(250)
        compare(body.width, 420); compare(value.implicitHeight, body.implicitHeight)
        const collapsed = body.implicitHeight
        let toggles = 0; value.toggleRequested.connect(() => ++toggles)
        findChild(body, "header").clicked(null); compare(toggles, 1)
        button(body, "done_all").clicked(); compare(body.showCompleted, true)
        findChild(body, "taskInput").text = "Draft"
        value.expanded = true; tryCompare(value, "morphing", true); tryCompare(value, "morphing", false, 1000)
        verify(body.implicitHeight > collapsed)
        value.width = 500; compare(body.width, 500)
        compare(value.implicitHeight, body.implicitHeight)
        value.expanded = false; value.contentActive = false
        compare(section(value), body); compare(body.showCompleted, true)
        compare(findChild(body, "taskInput").text, "Draft")
        value.bare = true; compare(body.open, true); compare(body.radius, 0)
        compare(findChild(body, "header").enabled, false)
        compare(section(value), body); compare(body.showCompleted, true)
        value.bare = false; value.contentActive = true; value.expanded = true
        compare(section(value), body); compare(findChild(body, "taskInput").text, "Draft")
        IrisStyle.density = 1.5; compare(body.pad, 21)
        compare(findChild(body, "taskInput").implicitHeight, 57)
        compare(rows(body)[0].spacing, 12)
        compare(Todo.calls.length, 0)
        const created = State.created; const destroyed = State.destroyed
        value.kind = "media"; wait(0); compare(State.destroyed, destroyed + 1)
        value.kind = "tasks"; compare(State.created, created + 1)
        compare(section(value).showCompleted, false)
        compare(findChild(section(value), "taskInput").text, "")
        compare(Todo.calls.length, 0)
    }
}
'''


def main():
    runners = [os.environ.get("QMLTESTRUNNER", ""), "/usr/lib/qt6/bin/qmltestrunner",
               "/usr/lib64/qt6/bin/qmltestrunner", "/usr/lib/x86_64-linux-gnu/qt6/bin/qmltestrunner",
               shutil.which("qmltestrunner") or ""]
    with tempfile.TemporaryDirectory(prefix="inir-tasks-section-") as folder:
        directory = Path(folder)
        write_harness(directory)
        environment = dict(os.environ, QT_QPA_PLATFORM="offscreen", QML_DISABLE_DISK_CACHE="1")
        probe = directory / "probe.qml"
        probe.write_text('import QtQuick\nimport QtTest\nTestCase { name: "Probe"; function test_ok() { verify(true) } }\n')
        for runner in dict.fromkeys(runners):
            if not runner or not Path(runner).is_file():
                continue
            version = subprocess.run([runner, "-input", str(probe)], env=environment, capture_output=True, text=True, timeout=30)
            if "Using QtTest library 6" not in version.stdout + version.stderr:
                continue
            result = subprocess.run([runner, "-input", str(directory / "tst_tasks.qml")],
                                    env=environment, capture_output=True, text=True, timeout=30)
            output = result.stdout + result.stderr
            print(output, end="")
            return 1 if result.returncode or "FAIL!" in output else 0
    print("Iris tasks: no Qt 6 qmltestrunner found; native checks not executed")
    return 0


if __name__ == "__main__":
    sys.exit(main())
