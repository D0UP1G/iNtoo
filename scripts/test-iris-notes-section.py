#!/usr/bin/env python3
"""Native Qt checks of deferred notes, real frame and Notepad write guards.

Notepad and styled controls are fixtures; real loading/edit/action handlers,
frame animations and the production URL boundary execute in Qt.
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
    component = helper.block(content, r"Component\s*\{\s*id: notesComponent\b")
    if "loadingTab" in content or "noteEditor" in content:
        raise ValueError("notes implementation remains inline")
    if 'setSource(Qt.resolvedUrl("IrisSidebarNotesSection.qml")' not in component:
        raise ValueError("missing deferred notes URL")
    if "sectionHost: root" not in component or re.search(r"\b(?:active|asynchronous|expanded|contentActive)\s*:", component):
        raise ValueError("notes must load synchronously with host and remain resident")
    frame = (SIDEBAR / "IrisSidebarSectionFrame.qml").read_text()
    section = (SIDEBAR / "IrisSidebarNotesSection.qml").read_text()
    if re.search(r"\broot\b", section):
        raise ValueError("external notes references former root")
    fixture = directory / "Fixture"
    fixture.mkdir()
    services = {
        "IrisStyle": '''property real density: 1
property color accent: "blue"
property color text: "black"
property color muted: "gray"
property color onTint: "white"
property color surfaceHigh: "white"
property color field: "white"
property color hairline: "gray"
property color hairlineStrong: "gray"
property color accentContainer: "lightblue"
property color inkOnAccentContainer: "black"
property string fontMain: "Sans"
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
property var identity: ({yellow: "yellow"})
function weight(value) { return value }
function secondaryOf(value) { return "gray" }
function strongOf(value) { return "black" }
function iconRadius(value) { return value / 4 }
function duration(value) { return value }''',
        "Translation": "function tr(value) { return value }",
        "Notepad": """property int currentTab: 0
property var tabs: [{title: "Note 1", text: "First"}, {title: "Note 2", text: "Second"}]
readonly property string text: tabs[currentTab]?.text ?? ""
property var calls: []
function setTextValue(value) {
    calls = [...calls, ["text", value]]
    const t = tabs.slice(); t[currentTab] = Object.assign({}, t[currentTab], {text: value}); tabs = t
}
function setTabTitle(index, value) {
    calls = [...calls, ["title", index, value]]
    const t = tabs.slice(); t[index] = Object.assign({}, t[index], {title: value}); tabs = t
}
function addTab(title) {
    calls = [...calls, ["add", title]]
    const t = tabs.slice(); t.push({title: title || `Note ${t.length + 1}`, text: ""}); tabs = t; currentTab = t.length - 1
}
function switchTab(index) { calls = [...calls, ["switch", index]]; currentTab = index }
""",
    }
    (fixture / "qmldir").write_text("module Fixture\n" + "".join(
        f"singleton {name} 1.0 {name}.qml\n" for name in services))
    for name, body in services.items():
        (fixture / (name + ".qml")).write_text("pragma Singleton\nimport QtQuick\nQtObject {\n" + body + "\n}\n")
    stubs = {
        "IrisText": 'Text { enum Role { Body, Meta }\nproperty int role: 0 }',
        "MaterialSymbol": 'Text { property real iconSize: 18; property real fill: 0 }',
        "IrisButton": 'Button { property bool selected: false; property bool quiet: false }',
        "IrisIconButton": 'Button { property string materialIcon: ""; property bool selected: false; property real iconSize: 18; implicitWidth: 34; implicitHeight: 34 }',
    }
    for name, body in stubs.items():
        (directory / (name + ".qml")).write_text("import QtQuick\nimport QtQuick.Controls\n" + body)
    for name in ("header", "body", "summaryColumn"):
        frame = frame.replace("id: " + name + "\n", f'id: {name}\n        objectName: "{name}"\n')
    for name in ("noteTitle", "noteEditor", "tabs"):
        section = section.replace("id: " + name + "\n", f'id: {name}\n        objectName: "{name}"\n')
    (directory / "IrisSidebarSectionFrame.qml").write_text(isolated(frame))
    (directory / "state.js").write_text(".pragma library\nvar created = 0\nvar destroyed = 0\n")
    section = isolated(section).replace('import QtQuick\n', 'import QtQuick\nimport "state.js" as State\n', 1)
    section = section.replace("Component.onCompleted: notes.loadTab()", "Component.onCompleted: { ++State.created; notes.loadTab() }\n    Component.onDestruction: ++State.destroyed", 1)
    (directory / "IrisSidebarNotesSection.qml").write_text(section)
    api = content[content.index("    required property string kind"):content.index("    sourceComponent:")]
    selection = helper.block(content, r"sourceComponent:\s*\{")
    ids = re.findall(r'case "[^"]+": return (\w+)', selection)
    others = "\n".join("Component { id: " + identifier + "; Item { implicitHeight: 20 } }"
                       for identifier in ids if identifier != "notesComponent")
    payload = ('pragma ComponentBehavior: Bound\nimport QtQuick\nimport "Fixture"\nLoader {\nid: root\n'
               + api + selection + component + others + "\n}\n")
    (directory / "SectionPayload.qml").write_text(payload)
    deferred = directory / "Deferred"
    deferred.mkdir()
    (deferred / "Fixture").symlink_to(fixture, target_is_directory=True)
    (deferred / "SectionPayload.qml").write_text(payload)
    (deferred / "IrisSidebarNotesSection.qml").write_text("import QtQuick\nItem { impossible syntax !!! }\n")
    (directory / "tst_notes.qml").write_text(QML_TEST)


QML_TEST = r'''pragma ComponentBehavior: Bound
import QtQuick
import QtTest
import "Fixture"
import "Deferred" as Deferred
import "state.js" as State
TestCase {
    id: test
    name: "IrisSidebarNotesSection"
    visible: true; width: 700; height: 900
    when: windowShown
    Component { id: payload; SectionPayload {} }
    Component { id: boundary; Deferred.SectionPayload {} }
    Item { id: focusSink; focus: true }
    function create(expanded = true) {
        const value = createTemporaryObject(payload, test, {kind: "notes", width: 420, expanded: expanded})
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
    function button(item, property, expected) {
        const found = descendants(item).find(child => child[property] === expected)
        verify(found !== undefined, "missing handler " + expected)
        return found
    }
    function init() {
        focusSink.forceActiveFocus()
        Notepad.currentTab = 0
        Notepad.tabs = [{title: "Note 1", text: "First"}, {title: "Note 2", text: "Second"}]
        Notepad.calls = []
        IrisStyle.density = 1
    }
    function test_compile_boundary() {
        const value = createTemporaryObject(boundary, test, {kind: "media"})
        verify(value !== null); compare(value.status, Loader.Ready); compare(value.item.implicitHeight, 20)
        ignoreWarning(/.*IrisSidebarNotesSection.qml.*Expected token.*/)
        value.kind = "notes"
        compare(value.item.status, Loader.Error); compare(value.item.item, null)
    }
    function test_initial_load_and_programmatic_switch_no_write() {
        const value = create(); const body = section(value)
        const editor = findChild(body, "noteEditor"); const title = findChild(body, "noteTitle")
        compare(editor.text, "First"); compare(title.text, "Note 1"); compare(body.loadingTab, false)
        compare(body.detail, "2"); compare(Notepad.calls.length, 0)
        editor.forceActiveFocus(); compare(editor.activeFocus, true)
        Notepad.currentTab = 1
        compare(editor.text, "Second"); compare(title.text, "Note 2")
        compare(Notepad.calls.length, 0)
        Notepad.tabs = [{title: "Remote 1", text: "External A"}, {title: "Remote 2", text: "External B"}]
        compare(editor.text, "External B"); compare(title.text, "Remote 2")
        compare(Notepad.calls.length, 0); compare(body.loadingTab, false)
        // Same-text tabs updates preserve the existing unsaved local title.
        title.text = "Local title"
        Notepad.tabs = [{title: "Remote 1", text: "External A"}, {title: "Renamed", text: "External B"}]
        compare(title.text, "Local title"); compare(Notepad.calls.length, 0)
    }
    function test_focus_write_guard_and_title_save() {
        const value = create(); const body = section(value)
        const editor = findChild(body, "noteEditor"); const title = findChild(body, "noteTitle")
        focusSink.forceActiveFocus(); compare(editor.activeFocus, false)
        editor.text = "Unfocused edit"
        compare(Notepad.text, "First"); compare(Notepad.calls.length, 0)
        body.loadTab(); compare(editor.text, "First"); compare(Notepad.calls.length, 0)
        editor.forceActiveFocus(); compare(editor.activeFocus, true)
        editor.text = "Focused edit"
        compare(Notepad.text, "Focused edit")
        compare(JSON.stringify(Notepad.calls), JSON.stringify([["text", "Focused edit"]]))
        body.loadTab(); compare(Notepad.calls.length, 1)
        title.text = "   "; title.editingFinished(); compare(Notepad.calls.length, 1)
        title.text = "  Saved title  "; title.editingFinished()
        compare(Notepad.tabs[0].title, "Saved title")
        compare(JSON.stringify(Notepad.calls[1]), JSON.stringify(["title", 0, "Saved title"]))
    }
    function test_add_switch_and_focus() {
        const value = create(); const body = section(value)
        const editor = findChild(body, "noteEditor"); const title = findChild(body, "noteTitle")
        button(body, "text", "Note 2").clicked()
        compare(Notepad.currentTab, 1); compare(editor.text, "Second"); compare(title.text, "Note 2")
        button(body, "materialIcon", "add").clicked()
        compare(Notepad.tabs.length, 3); compare(Notepad.currentTab, 2)
        compare(editor.text, ""); compare(title.text, "Note 3"); compare(editor.activeFocus, true)
        compare(JSON.stringify(Notepad.calls), JSON.stringify([["switch", 1], ["add", ""]]))
        editor.text = "New content"
        compare(Notepad.tabs[2].text, "New content")
        button(body, "text", "Note 1").clicked()
        compare(editor.text, "First"); compare(title.text, "Note 1")
        compare(JSON.stringify(Notepad.calls), JSON.stringify([["switch", 1], ["add", ""], ["text", "New content"], ["switch", 0]]))
    }
    function test_frame_residency_and_kind_reset() {
        const value = create(false); const body = section(value)
        const title = findChild(body, "noteTitle"); const editor = findChild(body, "noteEditor")
        wait(250)
        compare(body.width, 420); compare(value.implicitHeight, body.implicitHeight)
        const collapsed = body.implicitHeight
        let toggles = 0; value.toggleRequested.connect(() => ++toggles)
        findChild(body, "header").clicked(null); compare(toggles, 1)
        value.expanded = true; tryCompare(value, "morphing", true); tryCompare(value, "morphing", false, 1000)
        verify(body.implicitHeight > collapsed)
        value.width = 500; compare(body.width, 500)
        compare(value.implicitHeight, body.implicitHeight)
        // Unfocused edits stay local until save and must survive visibility gates.
        focusSink.forceActiveFocus(); title.text = "Local unsaved title"; editor.text = "Local unsaved body"
        value.expanded = false; value.contentActive = false
        compare(section(value), body); compare(title.text, "Local unsaved title"); compare(editor.text, "Local unsaved body")
        value.contentActive = true; value.expanded = true
        compare(section(value), body); compare(title.text, "Local unsaved title"); compare(editor.text, "Local unsaved body")
        value.bare = true; compare(body.radius, 0); compare(findChild(body, "header").enabled, false)
        IrisStyle.density = 1.5; compare(body.pad, 21)
        compare(findChild(body, "tabs").spacing, 6)
        compare(Notepad.calls.length, 0)
        const created = State.created; const destroyed = State.destroyed
        value.kind = "media"; wait(0); compare(State.destroyed, destroyed + 1)
        value.kind = "notes"
        compare(State.created, created + 1)
        compare(findChild(section(value), "noteTitle").text, "Note 1")
        compare(findChild(section(value), "noteEditor").text, "First")
        compare(Notepad.calls.length, 0)
    }
}
'''


def main():
    runners = [os.environ.get("QMLTESTRUNNER", ""), "/usr/lib/qt6/bin/qmltestrunner",
               "/usr/lib64/qt6/bin/qmltestrunner", "/usr/lib/x86_64-linux-gnu/qt6/bin/qmltestrunner",
               shutil.which("qmltestrunner") or ""]
    with tempfile.TemporaryDirectory(prefix="inir-notes-section-") as folder:
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
            result = subprocess.run([runner, "-input", str(directory / "tst_notes.qml")],
                                    env=environment, capture_output=True, text=True, timeout=30)
            output = result.stdout + result.stderr
            print(output, end="")
            return 1 if result.returncode or "FAIL!" in output else 0
    print("Iris notes: no Qt 6 qmltestrunner found; native checks not executed")
    return 0


if __name__ == "__main__":
    sys.exit(main())
