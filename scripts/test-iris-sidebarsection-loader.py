#!/usr/bin/env python3
"""Check the production sidebar wrapper in native Qt 6 with tiny section bodies.

The payload API and kind switch are extracted from the real content file. Dummy
bodies isolate Loader lifecycle from Quickshell services and visual components.
"""

import os
from pathlib import Path
import re
import shutil
import subprocess
import sys
import tempfile

ROOT = Path(__file__).resolve().parent.parent
SIDEBAR = ROOT / "modules/iris/sidebar"


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


def write_harness(directory):
    wrapper = (SIDEBAR / "IrisSidebarSection.qml").read_text(encoding="utf-8")
    content = (SIDEBAR / "IrisSidebarSectionContent.qml").read_text(encoding="utf-8")
    declaration = block(wrapper, r"Loader\s*\{\s*id:\s*root\b")
    if "sourceComponent:" in declaration or "Component {" in declaration:
        raise ValueError("sidebar wrapper still compiles inline section bodies")
    if 'Qt.resolvedUrl("IrisSidebarSectionContent.qml")' not in declaration:
        raise ValueError("sidebar wrapper must load the external content file")
    if "readonly property bool morphing" in declaration:
        raise ValueError("public morphing must remain writable")
    api_start = content.index("    required property string kind")
    api_end = content.index("    sourceComponent:", api_start)
    api = content[api_start:api_end]
    source_switch = block(content, r"sourceComponent:\s*\{")
    component_ids = re.findall(r'case "[^"]+": return (\w+)', source_switch)
    if len(component_ids) != 9 or len(set(component_ids)) != 9:
        raise ValueError("expected the nine production sidebar kind implementations")
    for signature in ("required property string kind", "property bool contentActive",
                      "property bool expanded", "property bool bare",
                      "property bool morphing", "readonly property real d",
                      "signal navigate()", "signal toggleRequested()"):
        if signature not in api or signature not in declaration:
            raise ValueError("missing public API declaration: " + signature)

    fixture = directory / "Fixture"
    fixture.mkdir()
    (fixture / "qmldir").write_text(
        "module Fixture\nsingleton IrisStyle 1.0 IrisStyle.qml\n", encoding="utf-8")
    (fixture / "IrisStyle.qml").write_text('''pragma Singleton
import QtQuick
QtObject { property real density: 1.25 }
''', encoding="utf-8")
    (directory / "state.js").write_text('''.pragma library
var created = 0
var destroyed = 0
var bodiesCreated = 0
var bodiesDestroyed = 0
var navigated = 0
var toggled = 0
''', encoding="utf-8")
    (directory / "IrisSidebarSection.qml").write_text(
        'pragma ComponentBehavior: Bound\nimport QtQuick\nimport "Fixture"\n'
        + declaration + "\n", encoding="utf-8")
    components = "\n".join('''    Component {
        id: ''' + identifier + '''
        Item {
            id: body
            property int localState: 0
            property bool morphing: false
            property string kindOnCompleted: ""
            property bool activeOnCompleted: false
            property bool expandedOnCompleted: false
            property bool bareOnCompleted: false
            property real widthOnCompleted: -1
            // Production Section rectangles have no natural horizontal size.
            implicitWidth: 0
            implicitHeight: (root.bare ? 100 : root.expanded ? 80 : 40)
                + (root.contentActive ? 7 : 0) + root.width / 10 + root.kind.length
            Binding { target: root; property: "morphing"; value: body.morphing }
            function navigate(): void { root.navigate() }
            function toggleRequested(): void { root.toggleRequested() }
            Component.onCompleted: {
                body.kindOnCompleted = root.kind
                body.activeOnCompleted = root.contentActive
                body.expandedOnCompleted = root.expanded
                body.bareOnCompleted = root.bare
                body.widthOnCompleted = root.width
                ++State.bodiesCreated
            }
            Component.onDestruction: ++State.bodiesDestroyed
        }
    }
''' for identifier in component_ids)
    payload = '''pragma ComponentBehavior: Bound
import QtQuick
import "Fixture"
import "state.js" as State
Loader {
    id: root
''' + api + source_switch + '''
    property int serial: 0
    property int localState: 0
    property string kindOnCompleted: ""
    property bool activeOnCompleted: false
    property bool expandedOnCompleted: false
    property bool bareOnCompleted: false
    property real widthOnCompleted: -1
    Component.onCompleted: {
        root.kindOnCompleted = root.kind
        root.activeOnCompleted = root.contentActive
        root.expandedOnCompleted = root.expanded
        root.bareOnCompleted = root.bare
        root.widthOnCompleted = root.width
        root.serial = ++State.created
    }
    Component.onDestruction: ++State.destroyed
''' + components + "}\n"
    (directory / "IrisSidebarSectionContent.qml").write_text(payload, encoding="utf-8")
    # This reference uses the original single Loader API and body selection.
    (directory / "OriginalSection.qml").write_text(payload, encoding="utf-8")
    (directory / "tst_sidebarsection.qml").write_text('''pragma ComponentBehavior: Bound
import QtQuick
import QtTest
import "Fixture"
import "state.js" as State

TestCase {
    id: root
    name: "IrisSidebarSectionLoader"
    when: windowShown
    visible: true
    width: 640
    height: 600
    Component {
        id: wrapperComponent
        IrisSidebarSection {
            onNavigate: ++State.navigated
            onToggleRequested: ++State.toggled
        }
    }
    Component { id: originalComponent; OriginalSection {} }

    function checkReady(wrapper) {
        compare(wrapper.active, true)
        compare(wrapper.asynchronous, false)
        compare(wrapper.status, Loader.Ready)
        verify(wrapper.item !== null)
        verify(wrapper.item.item !== null)
        compare(wrapper.item.status, Loader.Ready)
        compare(wrapper.item.asynchronous, false)
        verify(wrapper.source.toString().endsWith("/IrisSidebarSectionContent.qml"))
        compare(wrapper.item.width, wrapper.width)
        compare(wrapper.item.item.width, wrapper.width)
        tryCompare(wrapper, "implicitWidth", wrapper.item.implicitWidth)
        compare(wrapper.implicitHeight, wrapper.item.item.implicitHeight)
        compare(wrapper.height, wrapper.implicitHeight)
    }

    function checkInitial(wrapper, kind, active, expanded, bare, width) {
        checkReady(wrapper)
        for (const child of [wrapper.item, wrapper.item.item]) {
            compare(child.kindOnCompleted, kind)
            compare(child.activeOnCompleted, active)
            compare(child.expandedOnCompleted, expanded)
            compare(child.bareOnCompleted, bare)
            compare(child.widthOnCompleted, width)
        }
    }

    function checkEmpty(wrapper) {
        compare(wrapper.active, true)
        compare(wrapper.asynchronous, false)
        compare(wrapper.item, null)
        compare(wrapper.status, Loader.Null)
        compare(wrapper.source.toString(), "")
        compare(wrapper.sourceComponent, null)
        compare(wrapper.morphing, false)
    }

    function test_initial_data() {
        return ["media", "tasks", "notes", "calendar", "weather", "notifications",
                "focus", "mixer", "system"].map(kind => ({ tag: kind, kind: kind }))
    }

    function test_initial(data) {
        const created = State.created
        const wrapper = createTemporaryObject(wrapperComponent, root, {
            kind: data.kind, width: 347, contentActive: false, expanded: true, bare: true
        })
        verify(wrapper !== null)
        // Immediate comparison establishes synchronous completion, without tryCompare.
        checkInitial(wrapper, data.kind, false, true, true, 347)
        compare(State.created, created + 1)
        compare(wrapper.d, IrisStyle.density)
    }

    function test_invalid_initial_data() {
        return [{ tag: "empty", kind: "" }, { tag: "unknown", kind: "future-section" }]
    }

    function test_invalid_initial(data) {
        const created = State.created
        const wrapper = createTemporaryObject(wrapperComponent, root, { kind: data.kind, width: 333 })
        const original = createTemporaryObject(originalComponent, root, { kind: data.kind, width: 333 })
        verify(wrapper !== null)
        verify(original !== null)
        checkEmpty(wrapper)
        compare(wrapper.item, original.item)
        compare(wrapper.active, original.active)
        compare(wrapper.status, original.status)
        compare(wrapper.sourceComponent, original.sourceComponent)
        compare(wrapper.implicitWidth, original.implicitWidth)
        compare(wrapper.implicitHeight, original.implicitHeight)
        compare(State.created, created + 1) // Only the eager original reference was created.
        wrapper.kind = "notes"
        checkInitial(wrapper, "notes", true, false, false, wrapper.width)
    }

    function test_implicit_width_after_empty() {
        const wrapper = createTemporaryObject(wrapperComponent, root, { kind: "" })
        const original = createTemporaryObject(originalComponent, root, { kind: "" })
        wrapper.kind = "notes"
        original.kind = "notes"
        compare(wrapper.implicitWidth, original.implicitWidth)
        compare(wrapper.width, original.width)
        compare(wrapper.implicitHeight, original.implicitHeight)
        checkReady(wrapper)
        wrapper.kind = ""
        original.kind = ""
        checkEmpty(wrapper)
        compare(wrapper.implicitWidth, original.implicitWidth)
        compare(wrapper.width, original.width)
        compare(wrapper.implicitHeight, original.implicitHeight)
    }

    function test_reactive_residency_and_kind_switch() {
        const created = State.created
        const destroyed = State.destroyed
        const wrapper = createTemporaryObject(wrapperComponent, root, { kind: "notes", width: 320 })
        const payload = wrapper.item
        const firstBody = payload.item
        const serial = payload.serial
        payload.localState = 42
        firstBody.localState = 17
        wrapper.width = 420
        wrapper.contentActive = false
        wrapper.expanded = true
        wrapper.bare = true
        wrapper.visible = false
        compare(wrapper.item, payload)
        compare(payload.item, firstBody)
        compare(firstBody.localState, 17)
        compare(payload.localState, 42)
        compare(payload.width, 420)
        compare(payload.contentActive, false)
        compare(payload.expanded, true)
        compare(payload.bare, true)
        checkReady(wrapper)
        wait(20)
        compare(State.destroyed, destroyed)
        compare(wrapper.item, payload)

        wrapper.kind = "calendar"
        checkReady(wrapper)
        compare(wrapper.item, payload)
        compare(payload.serial, serial)
        compare(payload.localState, 42)
        compare(payload.kind, "calendar")
        verify(payload.item !== firstBody)
        compare(payload.item.kindOnCompleted, "calendar")
        compare(payload.item.activeOnCompleted, false)
        compare(payload.item.expandedOnCompleted, true)
        compare(payload.item.bareOnCompleted, true)
        compare(payload.item.widthOnCompleted, 420)
        wrapper.updateSource()
        wrapper.updateSource()
        compare(State.created, created + 1)
        wrapper.contentActive = true
        wrapper.expanded = false
        wrapper.bare = false
        compare(payload.contentActive, true)
        compare(payload.expanded, false)
        compare(payload.bare, false)
        checkReady(wrapper)

        wrapper.kind = ""
        checkEmpty(wrapper)
        tryCompare(State, "destroyed", destroyed + 1)
        wrapper.kind = "system"
        checkInitial(wrapper, "system", true, false, false, 420)
        verify(wrapper.item.serial !== serial)
        compare(wrapper.item.localState, 0)
        wrapper.item.item.morphing = true
        compare(wrapper.morphing, true)
        wrapper.kind = "unknown"
        checkEmpty(wrapper)
        tryCompare(State, "destroyed", destroyed + 2)
        wrapper.kind = "weather"
        checkReady(wrapper)
        compare(State.created, created + 3)
        wrapper.destroy()
        tryCompare(State, "destroyed", destroyed + 3)
        const reopened = createTemporaryObject(wrapperComponent, root, { kind: "weather", width: 480 })
        checkInitial(reopened, "weather", true, false, false, 480)
        compare(reopened.item.localState, 0)
    }

    function test_signals_morphing_and_sizing() {
        const wrapper = createTemporaryObject(wrapperComponent, root, { kind: "focus", width: 280 })
        const original = createTemporaryObject(originalComponent, root, { kind: "focus", width: 280 })
        const navigated = State.navigated
        const toggled = State.toggled
        compare(wrapper.contentActive, true)
        compare(wrapper.expanded, false)
        compare(wrapper.bare, false)
        compare(wrapper.morphing, false)
        wrapper.item.item.navigate()
        wrapper.item.item.toggleRequested()
        compare(State.navigated, navigated + 1)
        compare(State.toggled, toggled + 1)
        wrapper.item.item.morphing = true
        compare(wrapper.item.morphing, true)
        compare(wrapper.morphing, true)
        wrapper.item.item.morphing = false
        compare(wrapper.morphing, false)
        IrisStyle.density = 1.5
        compare(wrapper.d, 1.5)
        compare(wrapper.item.d, 1.5)
        for (const expanded of [false, true]) {
            wrapper.expanded = expanded
            original.expanded = expanded
            compare(wrapper.implicitWidth, original.implicitWidth)
            compare(wrapper.implicitHeight, original.implicitHeight)
            compare(wrapper.height, original.height)
            compare(wrapper.item.item.width, original.item.width)
        }
        // A consumer can still override the ordinary writable public property.
        wrapper.morphing = true
        compare(wrapper.morphing, true)
        compare(wrapper.item.morphing, false)
        IrisStyle.density = 1.25
    }
}
''', encoding="utf-8")


def main():
    candidates = [os.environ.get("QMLTESTRUNNER", ""),
                  "/usr/lib/qt6/bin/qmltestrunner", "/usr/lib64/qt6/bin/qmltestrunner",
                  "/usr/lib/x86_64-linux-gnu/qt6/bin/qmltestrunner",
                  shutil.which("qmltestrunner") or ""]
    with tempfile.TemporaryDirectory(prefix="inir-sidebarsection-loader-") as folder:
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
    print("Iris sidebar section Loader: no Qt 6 qmltestrunner found; native lifecycle not executed")
    return 0


if __name__ == "__main__":
    sys.exit(main())
