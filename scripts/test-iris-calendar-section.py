#!/usr/bin/env python3
"""Exercise the real deferred calendar and Section frame in native Qt 6.

Services and styled controls are fixtures; calendar functions, action handlers,
layout, animation, and the production URL Loader remain the real source.
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
spec = importlib.util.spec_from_file_location(
    "sidebar_loader", ROOT / "scripts/test-iris-sidebarsection-loader.py")
helper = importlib.util.module_from_spec(spec)
spec.loader.exec_module(helper)


def isolated(source):
    return re.sub(r"^import (?:qs(?:\.|\s)|Quickshell).*\n", "", source,
                  flags=re.MULTILINE).replace("import QtQuick\n", 'import QtQuick\nimport "Fixture"\n', 1)


def write_harness(directory):
    content = (SIDEBAR / "IrisSidebarSectionContent.qml").read_text()
    calendar_component = helper.block(content, r"Component\s*\{\s*id: calendarComponent\b")
    if "IrisSidebarCalendarSection {" in content or "selectedDate" in calendar_component:
        raise ValueError("calendar still compiles inline")
    if 'setSource(Qt.resolvedUrl("IrisSidebarCalendarSection.qml")' not in calendar_component:
        raise ValueError("calendar URL boundary is missing")
    if "sectionHost: root" not in calendar_component:
        raise ValueError("calendar host must be supplied before Component.onCompleted")
    frame = (SIDEBAR / "IrisSidebarSectionFrame.qml").read_text()
    calendar = (SIDEBAR / "IrisSidebarCalendarSection.qml").read_text()
    if re.search(r"\broot\b", frame + calendar):
        raise ValueError("external section retains an out-of-scope root")
    adapter = helper.block(content, r"component Section: IrisSidebarSectionFrame\s*\{")
    if "sectionHost: root" not in adapter:
        raise ValueError("other sections lost their shared host")
    fixture = directory / "Fixture"
    fixture.mkdir()
    services = {
        "IrisStyle": '''property real density: 1
property color accent: "blue"
property color text: "black"
property color muted: "gray"
property color subtext: "gray"
property color onTint: "white"
property color secondaryAccent: "purple"
property color hairline: "gray"
property color surfaceHigh: "white"
property color fillHover: "lightgray"
property string fontMain: "Sans"
property string fontNumbers: "Sans"
property real typeMeta: 12
property real typeBody: 14
property real typeLabel: 13
property real typeFootnote: 11
property real radiusCard: 12
property real morphDuration: 80
property var morphCurve: [0.4, 0, 0.2, 1, 1, 1]
property int feedbackEasing: Easing.Linear
property var identity: ({red: "red"})
function weight(value) { return value }
function secondaryOf(value) { return "gray" }
function iconRadius(value) { return value / 4 }
function duration(value) { return value }
function tintFillHover(value) { return "lightgray" }''',
        "DateTime": 'property var clock: ({date: new Date(2026, 9, 12, 14, 30)})',
        "Translation": 'function tr(value) { return value }',
        "Network": 'property bool online: true',
        "CalendarSync": '''property bool fetching: false
property bool enabled: true
property var sources: ["test"]
property var events: []
function getEventsForDate(date) { return events.filter(e => new Date(e.startDate).toDateString() === date.toDateString()) }''',
        "Events": '''property var list: []
property var added: []
property var removed: []
function getAllEventsForDate(date) { return list.filter(e => new Date(e.dateTime).toDateString() === date.toDateString()) }
function addEvent(title, body, date, category, priority, reminder, recurrence) {
    added = [...added, {title: title, date: date, category: category, priority: priority, reminder: reminder, recurrence: recurrence}]
}
function removeEvent(id) { removed = [...removed, id] }''',
    }
    (fixture / "qmldir").write_text("module Fixture\n" + "".join(
        f"singleton {name} 1.0 {name}.qml\n" for name in services))
    for name, body in services.items():
        (fixture / (name + ".qml")).write_text(
            "pragma Singleton\nimport QtQuick\nQtObject {\n" + body + "\n}\n")
    stubs = {
        "IrisText": 'Text { enum Role { Body, Meta }\nproperty int role: 0 }',
        "MaterialSymbol": 'Text { property real iconSize: 18; property real fill: 0 }',
        "IrisButton": '''Button { property bool emphasized: false; property bool selected: false; property bool quiet: false
property real buttonRadius: 0; property real buttonRadiusPressed: 0 }''',
        "IrisIconButton": 'Button { property string materialIcon: ""; property real iconSize: 18; implicitWidth: 34; implicitHeight: 34 }',
        "IrisField": "TextField {}",
        "IrisWheelPicker": 'Item { property var columns; property var values; signal moved(int column, int index); implicitWidth: 120; implicitHeight: 100 }',
    }
    for name, body in stubs.items():
        (directory / (name + ".qml")).write_text("import QtQuick\nimport QtQuick.Controls\n" + body)
    # Name existing production ids only in the fixture, to find the real handlers.
    for name in ("header", "body", "summaryColumn"):
        frame = frame.replace("id: " + name + "\n", f'id: {name}\n        objectName: "{name}"\n')
    for name in ("monthGrid", "eventSheet", "eventTitle", "timePicker"):
        calendar = calendar.replace("id: " + name + "\n", f'id: {name}\n        objectName: "{name}"\n')
    (directory / "IrisSidebarSectionFrame.qml").write_text(isolated(frame))
    (directory / "state.js").write_text(".pragma library\nvar created = 0\nvar destroyed = 0\n")
    instrumented = isolated(calendar).replace('import QtQuick\n',
                                              'import QtQuick\nimport "state.js" as State\n', 1)
    instrumented = instrumented.replace("id: calendar\n", "id: calendar\n"
                                       "    Component.onCompleted: ++State.created\n"
                                       "    Component.onDestruction: ++State.destroyed\n", 1)
    (directory / "IrisSidebarCalendarSection.qml").write_text(instrumented)
    api = content[content.index("    required property string kind"):content.index("    sourceComponent:")]
    selection = helper.block(content, r"sourceComponent:\s*\{")
    ids = re.findall(r'case "[^"]+": return (\w+)', selection)
    others = "\n".join("Component { id: " + identifier + "; "
                       + ('Section { title: "Other"; IrisText { text: "Other body" } }'
                          if identifier == "tasksComponent" else 'Item { implicitHeight: 20 }')
                       + " }" for identifier in ids if identifier != "calendarComponent")
    payload = ('pragma ComponentBehavior: Bound\nimport QtQuick\nimport "Fixture"\nLoader {\nid: root\n'
               + api + selection + calendar_component + adapter + others + "\n}\n")
    (directory / "SectionPayload.qml").write_text(payload)
    # This bad URL exists only in a separate temporary fixture directory.
    deferred = directory / "Deferred"
    deferred.mkdir()
    (deferred / "Fixture").symlink_to(fixture, target_is_directory=True)
    (deferred / "SectionPayload.qml").write_text(payload)
    (deferred / "IrisSidebarSectionFrame.qml").symlink_to(directory / "IrisSidebarSectionFrame.qml")
    for name in ("IrisText", "MaterialSymbol"):
        (deferred / (name + ".qml")).symlink_to(directory / (name + ".qml"))
    (deferred / "IrisSidebarCalendarSection.qml").write_text("import QtQuick\nItem { impossible syntax !!! }\n")
    (directory / "tst_calendar.qml").write_text('''pragma ComponentBehavior: Bound
import QtQuick
import QtTest
import "Fixture"
import "Deferred" as Deferred
import "state.js" as State

TestCase {
    id: test
    name: "IrisSidebarCalendarSection"
    visible: true
    width: 700; height: 900
    when: windowShown
    Component { id: payload; SectionPayload {} }
    Component { id: boundary; Deferred.SectionPayload {} }
    function create(kind = "calendar", expanded = true) {
        const value = createTemporaryObject(payload, test, {kind: kind, width: 420, expanded: expanded})
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
    function button(item, property, expected) {
        const found = descendants(item).find(child => child[property] === expected && child.visible)
        verify(found !== undefined, "missing action " + expected)
        return found
    }
    function init() {
        Events.list = []; Events.added = []; Events.removed = []
        CalendarSync.events = []
        IrisStyle.density = 1
    }
    function test_compile_boundary() {
        const value = createTemporaryObject(boundary, test, {kind: "media"})
        verify(value !== null)
        compare(value.status, Loader.Ready)
        compare(value.item.implicitHeight, 20)
        ignoreWarning(/.*IrisSidebarCalendarSection.qml.*Expected token.*/)
        value.kind = "calendar"
        compare(value.item.status, Loader.Error)
        compare(value.item.item, null)
    }
    function test_dates_events_actions() {
        const value = create()
        const cal = ready(value)
        compare(cal.selectedDate.toDateString(), DateTime.clock.date.toDateString())
        compare(cal.month.getDate(), 1)
        compare(cal.offset, (cal.month.getDay() - cal.weekStart + 7) % 7)
        button(cal, "materialIcon", "chevron_right").clicked()
        compare(cal.month.getMonth(), 10)
        button(cal, "materialIcon", "chevron_left").clicked()
        compare(cal.month.getMonth(), 9)
        cal.selectedDate = new Date(2024, 1, 29)
        cal.moveMonth(1)
        compare(cal.selectedDate.getMonth(), 2)
        compare(cal.selectedDate.getDate(), 1)
        button(cal, "materialIcon", "today").clicked()
        Events.list = [{id: 12, title: "Local", dateTime: new Date(2026, 9, 12, 16).toISOString()}]
        CalendarSync.events = [{title: "Remote", startDate: new Date(2026, 9, 12, 9).toISOString()}]
        compare(cal.entries.length, 2)
        compare(cal.entries[0].title, "Remote")
        compare(cal.todayEntries.length, 2)
        compare(cal.timeOf({allDay: true}), "All day")
        button(cal, "materialIcon", "close").clicked()
        compare(Events.removed.length, 1)
        compare(Events.removed[0], 12)
        const grid = findChild(cal, "monthGrid")
        const day = descendants(grid).find(child => child.date !== undefined && child.index === 20)
        verify(day !== undefined)
        day.clicked(null)
        compare(cal.selectedDate.toDateString(), day.date.toDateString())
    }
    function test_composer_submit_cancel_reset() {
        const value = create()
        const cal = ready(value)
        button(cal, "text", "New event").clicked()
        compare(cal.composing, true)
        const title = findChild(cal, "eventTitle")
        const picker = findChild(cal, "timePicker")
        const sheet = findChild(cal, "eventSheet")
        compare(title.text, "")
        compare(picker.hour, 15)
        compare(picker.minuteIndex, 0)
        compare(sheet.reminder, 15)
        title.text = "   "
        sheet.add()
        compare(Events.added.length, 0)
        compare(cal.composing, true)
        title.text = "  Test event  "
        picker.hour = 10; picker.minuteIndex = 3; sheet.reminder = 60
        button(cal, "text", "Add").clicked()
        compare(cal.composing, false)
        compare(Events.added.length, 1)
        compare(Events.added[0].title, "Test event")
        compare(Events.added[0].reminder, 60)
        compare(new Date(Events.added[0].date).getHours(), 10)
        compare(new Date(Events.added[0].date).getMinutes(), 15)
        button(cal, "text", "New event").clicked()
        compare(title.text, "")
        button(cal, "text", "Cancel").clicked()
        compare(cal.composing, false)
        compare(Events.added.length, 1)
        cal.composing = true
        value.expanded = false
        compare(cal.composing, false)
    }
    function test_geometry_toggle_animation_and_residency() {
        const value = create("calendar", false)
        const cal = ready(value)
        wait(250)
        compare(cal.width, 420)
        compare(value.item.width, 420)
        compare(value.implicitHeight, cal.implicitHeight)
        verify(cal.implicitHeight > 0)
        const collapsed = cal.implicitHeight
        cal.selectedDate = new Date(2025, 5, 3)
        let toggles = 0
        value.toggleRequested.connect(() => ++toggles)
        const header = findChild(cal, "header")
        header.clicked(null)
        compare(toggles, 1)
        value.expanded = true
        tryCompare(value, "morphing", true)
        tryCompare(value, "morphing", false, 1000)
        verify(cal.implicitHeight > collapsed)
        value.width = 500
        compare(cal.width, 500)
        compare(value.implicitHeight, cal.implicitHeight)
        value.contentActive = false
        compare(ready(value), cal)
        compare(cal.selectedDate.getFullYear(), 2025)
        value.expanded = false
        compare(ready(value), cal)
        value.expanded = true
        compare(cal.selectedDate.getMonth(), 5)
        value.bare = true
        compare(cal.radius, 0)
        compare(header.enabled, false)
        IrisStyle.density = 1.5
        compare(cal.pad, 21)
    }
    function test_kind_change_recreates_calendar() {
        const value = create()
        const first = ready(value)
        first.selectedDate = new Date(2020, 2, 2)
        first.composing = true
        const created = State.created
        const destroyed = State.destroyed
        value.kind = "media"
        wait(0)
        compare(State.destroyed, destroyed + 1)
        value.kind = "calendar"
        const second = ready(value)
        compare(State.created, created + 1)
        compare(second.selectedDate.toDateString(), DateTime.clock.date.toDateString())
        compare(second.composing, false)
    }
    function test_other_section_adapter() {
        const value = create("tasks", false)
        const section = value.item
        compare(section.sectionHost, value)
        compare(section.open, false)
        let toggles = 0
        value.toggleRequested.connect(() => ++toggles)
        findChild(section, "header").clicked(null)
        compare(toggles, 1)
        value.expanded = true
        compare(section.open, true)
        compare(findChild(section, "body").children.length, 1)
        value.bare = true
        compare(section.radius, 0)
        IrisStyle.density = 1.5
        compare(section.pad, 21)
    }
}
''')


def main():
    runners = [os.environ.get("QMLTESTRUNNER", ""),
               "/usr/lib/qt6/bin/qmltestrunner", "/usr/lib64/qt6/bin/qmltestrunner",
               "/usr/lib/x86_64-linux-gnu/qt6/bin/qmltestrunner",
               shutil.which("qmltestrunner") or ""]
    with tempfile.TemporaryDirectory(prefix="inir-calendar-section-") as folder:
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
            result = subprocess.run([runner, "-input", str(directory / "tst_calendar.qml")],
                                    env=environment, capture_output=True, text=True, timeout=30)
            output = result.stdout + result.stderr
            print(output, end="")
            return 1 if result.returncode or "FAIL!" in output else 0
    print("Iris calendar: no Qt 6 qmltestrunner found; native checks not executed")
    return 0


if __name__ == "__main__":
    sys.exit(main())
