#!/usr/bin/env python3
"""Native Qt checks of deferred notification payload, frame and real handlers.

Styled controls and Notifications are fixtures; production grouping, action
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
    component = helper.block(content, r"Component\s*\{\s*id: notificationsComponent\b")
    if "NoticeCard" in content or "NoticeStack" in content or "unfoldedApps" in content:
        raise ValueError("notifications implementation remains inline")
    if 'setSource(Qt.resolvedUrl("IrisSidebarNotificationsSection.qml")' not in component:
        raise ValueError("missing deferred notifications URL")
    if "sectionHost: root" not in component or re.search(r"\b(?:active|asynchronous|expanded|contentActive)\s*:", component):
        raise ValueError("notification payload must load synchronously with host and remain resident")
    frame = (SIDEBAR / "IrisSidebarSectionFrame.qml").read_text()
    section = (SIDEBAR / "IrisSidebarNotificationsSection.qml").read_text()
    if re.search(r"\broot\b", section):
        raise ValueError("external notifications references former root")
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
property var identity: ({pink: "pink"})
function weight(value) { return value }
function secondaryOf(value) { return "gray" }
function strongOf(value) { return "black" }
function iconRadius(value) { return value / 4 }
function duration(value) { return value }''',
        "Translation": "function tr(value) { return value }",
        "ColorUtils": "function mix(a, b, weight) { return a }",
        "Notifications": '''property var list: []
property var appNameList: []
property var groupsByAppName: ({})
property bool manualDndActive: false
property var calls: []
function toggleSilent() { calls = [...calls, ["dnd"]]; manualDndActive = !manualDndActive }
function discardNotification(id) { calls = [...calls, ["dismiss", id]] }
function discardNotificationsForApp(app) { calls = [...calls, ["app", app]] }
function discardAllNotifications() { calls = [...calls, ["all"]] }
function attemptInvokeAction(id, action) { calls = [...calls, ["action", id, action]] }
function setItems(items) {
    list = items
    const groups = {}; const names = []
    for (const item of items) {
        if (!groups[item.appName]) { groups[item.appName] = {notifications: []}; names.push(item.appName) }
        groups[item.appName].notifications.push(item)
    }
    groupsByAppName = groups; appNameList = names
}''',
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
        "IrisNotificationIcon": 'Item { property real size: 22; property bool showImage: false; property string appName; property string appIcon; property string summary; property bool critical: false; implicitHeight: size; implicitWidth: size }',
    }
    for name, body in stubs.items():
        (directory / (name + ".qml")).write_text("import QtQuick\nimport QtQuick.Controls\n" + body)
    for name in ("header", "body", "summaryColumn"):
        frame = frame.replace("id: " + name + "\n", f'id: {name}\n        objectName: "{name}"\n')
    for name in ("stack", "notice"):
        section = section.replace("id: " + name + "\n", f'id: {name}\n        objectName: "{name}"\n')
    (directory / "IrisSidebarSectionFrame.qml").write_text(isolated(frame))
    (directory / "state.js").write_text(".pragma library\nvar created = 0\nvar destroyed = 0\n")
    section = isolated(section).replace('import QtQuick\n', 'import QtQuick\nimport "state.js" as State\n', 1)
    section = section.replace("id: notifications\n", "id: notifications\n    Component.onCompleted: ++State.created\n    Component.onDestruction: ++State.destroyed\n", 1)
    (directory / "IrisSidebarNotificationsSection.qml").write_text(section)
    api = content[content.index("    required property string kind"):content.index("    sourceComponent:")]
    selection = helper.block(content, r"sourceComponent:\s*\{")
    ids = re.findall(r'case "[^"]+": return (\w+)', selection)
    others = "\n".join("Component { id: " + identifier + "; Item { implicitHeight: 20 } }"
                       for identifier in ids if identifier != "notificationsComponent")
    payload = ('pragma ComponentBehavior: Bound\nimport QtQuick\nimport "Fixture"\nLoader {\nid: root\n'
               + api + selection + component + others + "\n}\n")
    (directory / "SectionPayload.qml").write_text(payload)
    deferred = directory / "Deferred"
    deferred.mkdir()
    (deferred / "Fixture").symlink_to(fixture, target_is_directory=True)
    (deferred / "SectionPayload.qml").write_text(payload)
    (deferred / "IrisSidebarNotificationsSection.qml").write_text("import QtQuick\nItem { impossible syntax !!! }\n")
    (directory / "tst_notifications.qml").write_text(QML_TEST)


QML_TEST = '''pragma ComponentBehavior: Bound
import QtQuick
import QtTest
import "Fixture"
import "Deferred" as Deferred
import "state.js" as State
TestCase {
    id: test
    name: "IrisSidebarNotificationsSection"
    visible: true; width: 700; height: 900
    when: windowShown
    Component { id: payload; SectionPayload {} }
    Component { id: boundary; Deferred.SectionPayload {} }
    function create(expanded = true) {
        const value = createTemporaryObject(payload, test, {kind: "notifications", width: 420, expanded: expanded})
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
    function stacks(item) { return descendants(item).filter(child => child.objectName === "stack") }
    function cards(item) { return descendants(item).filter(child => child.objectName === "notice") }
    function note(id, app = "Chat") {
        return {notificationId: id, appName: app, summary: "Summary " + id, body: "Body " + id,
                actions: [{identifier: "default", text: "Default"}, {identifier: "reply", text: "Reply"}]}
    }
    function init() {
        Notifications.setItems([]); Notifications.calls = []; Notifications.manualDndActive = false
        IrisStyle.density = 1
    }
    function test_compile_boundary() {
        const value = createTemporaryObject(boundary, test, {kind: "media"})
        verify(value !== null); compare(value.status, Loader.Ready); compare(value.item.implicitHeight, 20)
        ignoreWarning(/.*IrisSidebarNotificationsSection.qml.*Expected token.*/)
        value.kind = "notifications"
        compare(value.item.status, Loader.Error); compare(value.item.item, null)
    }
    function test_service_updates_and_order() {
        const value = create(false); const body = section(value)
        compare(body.latest, null); compare(body.detail, "")
        verify(descendants(findChild(body, "summaryColumn")).some(child => child.text === "You're all caught up."))
        Notifications.setItems([note(10), note(20), note(30), note(40, "Mail")])
        compare(body.latest.notificationId, 40); compare(body.detail, "4")
        verify(descendants(findChild(body, "summaryColumn")).some(child => child.text === "Summary 40"))
        compare(stacks(body).length, 2); compare(stacks(body)[0].appName, "Chat")
        const chat = stacks(body)[0]
        compare(chat.count, 3); compare(chat.items[0].notificationId, 30)
        compare(chat.folded, true); compare(cards(chat).length, 1)
        compare(cards(chat)[0].notification.notificationId, 30); compare(cards(chat)[0].hidden, 2)
        chat.toggle(); compare(chat.unfolded, true); compare(chat.folded, false)
        compare(cards(chat).length, 3); compare(cards(chat)[2].notification.notificationId, 10)
        button(chat, "text", "Show less").clicked(); compare(chat.unfolded, false)
        cards(chat)[0].unfold(); compare(chat.unfolded, true)
        Notifications.setItems([note(50)])
        compare(section(value), body); compare(body.latest.notificationId, 50)
        compare(stacks(body)[0].count, 1); compare(stacks(body)[0].unfolded, true)
        compare(cards(body).length, 1); compare(cards(body)[0].hidden, 0)
        compare(Notifications.calls.length, 0)
    }
    function test_actions_route_ids_and_navigation() {
        Notifications.setItems([note(11), note(22)])
        const value = create(); const body = section(value)
        let navigations = 0; value.navigate.connect(() => ++navigations)
        button(body, "materialIcon", "notifications_active").clicked()
        compare(Notifications.manualDndActive, true)
        compare(button(body, "materialIcon", "notifications_off").selected, true)
        const chat = stacks(body)[0]; chat.toggle()
        const first = cards(chat)[0]; const second = cards(chat)[1]
        button(first, "materialIcon", "close").clicked()
        button(second, "text", "Open").clicked()
        button(first, "text", "Reply").clicked()
        button(chat, "materialIcon", "clear_all").clicked()
        const all = descendants(body).find(child => child.Accessible.name === "Clear all")
        verify(all !== undefined); all.clicked()
        compare(navigations, 2)
        compare(JSON.stringify(Notifications.calls), JSON.stringify([["dnd"], ["dismiss", 22], ["action", 11, "default"], ["action", 22, "reply"], ["app", "Chat"], ["all"]]))
    }
    function test_frame_residency_and_kind_reset() {
        Notifications.setItems([note(1), note(2), note(3)])
        const value = create(false); const body = section(value)
        wait(250)
        compare(body.width, 420); compare(value.implicitHeight, body.implicitHeight)
        const collapsed = body.implicitHeight
        let toggles = 0; value.toggleRequested.connect(() => ++toggles)
        findChild(body, "header").clicked(null); compare(toggles, 1)
        stacks(body)[0].toggle(); compare(body.unfoldedApps.Chat, true)
        value.expanded = true; tryCompare(value, "morphing", true); tryCompare(value, "morphing", false, 1000)
        verify(body.implicitHeight > collapsed)
        value.width = 500; compare(body.width, 500)
        compare(value.implicitHeight, body.implicitHeight)
        value.expanded = false; value.contentActive = false
        compare(section(value), body); compare(body.unfoldedApps.Chat, true)
        value.contentActive = true; value.expanded = true
        compare(section(value), body); compare(stacks(body)[0].unfolded, true)
        value.bare = true; compare(body.radius, 0); compare(findChild(body, "header").enabled, false)
        IrisStyle.density = 1.5; compare(body.pad, 21)
        compare(Notifications.calls.length, 0)
        const created = State.created; const destroyed = State.destroyed
        value.kind = "media"; wait(0); compare(State.destroyed, destroyed + 1)
        value.kind = "notifications"
        compare(State.created, created + 1)
        compare(section(value).unfoldedApps.Chat, undefined)
        compare(stacks(section(value))[0].folded, true)
        compare(Notifications.calls.length, 0)
    }
}
'''


def main():
    runners = [os.environ.get("QMLTESTRUNNER", ""), "/usr/lib/qt6/bin/qmltestrunner",
               "/usr/lib64/qt6/bin/qmltestrunner", "/usr/lib/x86_64-linux-gnu/qt6/bin/qmltestrunner",
               shutil.which("qmltestrunner") or ""]
    with tempfile.TemporaryDirectory(prefix="inir-notifications-section-") as folder:
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
            result = subprocess.run([runner, "-input", str(directory / "tst_notifications.qml")],
                                    env=environment, capture_output=True, text=True, timeout=30)
            output = result.stdout + result.stderr
            print(output, end="")
            return 1 if result.returncode or "FAIL!" in output else 0
    print("Iris notifications: no Qt 6 qmltestrunner found; native checks not executed")
    return 0


if __name__ == "__main__":
    sys.exit(main())
