#!/usr/bin/env python3
"""Exercise deferred Weather payload/frame with stateful service fixtures."""
import importlib.util
import os
from pathlib import Path
import re
import shutil
import subprocess
import sys
import tempfile

ROOT = Path(__file__).resolve().parent.parent
spec = importlib.util.spec_from_file_location("focus", ROOT / "scripts/test-iris-focus-section.py")
focus = importlib.util.module_from_spec(spec)
spec.loader.exec_module(focus)


def write_harness(directory):
    focus.write_harness(directory)
    content = (focus.SIDEBAR / "IrisSidebarSectionContent.qml").read_text()
    component = focus.helper.block(content, r"Component\s*\{\s*id: weatherComponent\b")
    if 'setSource(Qt.resolvedUrl("IrisSidebarWeatherSection.qml")' not in component or "sectionHost: root" not in component:
        raise ValueError("weather URL or initial host boundary missing")
    if re.search(r"\b(?:Weather|active|asynchronous)\b", component):
        raise ValueError("weather must be a synchronous resident URL loader")
    weather = (focus.SIDEBAR / "IrisSidebarWeatherSection.qml").read_text()
    if re.search(r"\broot\b", weather) or "Timer {" in weather:
        raise ValueError("weather retains external root or changes scheduling")
    if weather.count("Weather.forceRefresh()") != 1:
        raise ValueError("weather refresh must only belong to its action")
    weather = focus.isolated(weather).replace('import QtQuick\n', 'import QtQuick\nimport "state.js" as State\n', 1)
    weather = weather.replace("id: weather\n", "id: weather\n    Component.onCompleted: { ++State.created; State.initialHost = weather.sectionHost }\n    Component.onDestruction: ++State.destroyed\n", 1)
    (directory / "IrisSidebarWeatherSection.qml").write_text(weather)
    (directory / "state.js").write_text(".pragma library\nvar created = 0\nvar destroyed = 0\nvar initialHost = null\n")
    style_file = directory / "Fixture/IrisStyle.qml"
    style = style_file.read_text().replace('property var identity: ({orange: "orange", teal: "teal"})',
        'property var identity: ({orange: "orange", teal: "teal", indigo: "indigo", blue: "blue", lavender: "lavender"})')
    style = style[:style.rfind("}")] + '''property color textSecondary: "gray"
property color textTertiary: "darkgray"
function strongOf(value) { return "black" }
function skyLight(value) { return Qt.lighter(value === "night_clear" ? "lavender" : "lightblue", 1) }
}\n'''
    style_file.write_text(style)
    fixtures = {
        "Weather": '''property bool enabled: true
property bool busy: false
property bool night: false
property bool showVisibleCity: true
property string visibleCity: "Gentoo City"
property int refreshes: 0
property var data: ({temp: "21°C", tempFeelsLike: "20°C", description: "Sunny", wCode: 0, humidity: "42%"})
function isNightNow() { return night }
function hasRunningRequests() { return busy }
function forceRefresh() { ++refreshes }''',
        "Icons": '''function getWeatherIcon(code, night) { return code === -1 ? null : night ? "night_clear" : "sunny" }'''
    }
    with (directory / "Fixture/qmldir").open("a") as qml:
        for name, body in fixtures.items():
            qml.write(f"singleton {name} 1.0 {name}.qml\n")
            (directory / f"Fixture/{name}.qml").write_text("pragma Singleton\nimport QtQuick\nQtObject {\n" + body + "\n}\n")
    api = content[content.index("    required property string kind"):content.index("    sourceComponent:")]
    selection = focus.helper.block(content, r"sourceComponent:\s*\{")
    ids = re.findall(r'case "[^"]+": return (\w+)', selection)
    others = "\n".join("Component { id: " + name + "; Item { implicitHeight: 20 } }" for name in ids if name != "weatherComponent")
    payload = ('pragma ComponentBehavior: Bound\nimport QtQuick\nimport "Fixture"\nLoader {\nid: root\n'
               + api + selection + component + others + "\n}\n")
    (directory / "WeatherPayload.qml").write_text(payload)
    (directory / "Deferred/WeatherPayload.qml").write_text(payload)
    (directory / "Deferred/IrisSidebarWeatherSection.qml").write_text("import QtQuick\nItem { impossible syntax !!! }\n")
    (directory / "tst_weather.qml").write_text('''pragma ComponentBehavior: Bound
import QtQuick
import QtTest
import "Fixture"
import "Deferred" as Deferred
import "state.js" as State
TestCase {
    id: test
    name: "IrisSidebarWeatherSection"
    visible: true
    width: 700; height: 900
    when: windowShown
    Component { id: payload; WeatherPayload {} }
    Component { id: boundary; Deferred.WeatherPayload {} }
    function create(kind = "weather", expanded = true) {
        const value = createTemporaryObject(payload, test, {kind: kind, expanded: expanded, width: 420})
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
    function withProperty(item, name, expected) {
        const found = descendants(item).find(child => child[name] === expected)
        verify(found !== undefined, "missing " + name + "=" + expected)
        return found
    }
    function texts(item) { return descendants(item).filter(child => child.text !== undefined).map(child => child.text) }
    function forecast(temp = "21°C", code = 0, description = "Sunny") {
        Weather.data = {temp: temp, tempFeelsLike: "20°C", description: description, wCode: code, humidity: "42%"}
    }
    function init() {
        IrisStyle.density = 1
        Weather.enabled = true; Weather.busy = false; Weather.night = false
        Weather.showVisibleCity = true; Weather.visibleCity = "Gentoo City"
        Weather.refreshes = 0; forecast()
        State.created = 0; State.destroyed = 0; State.initialHost = null
    }
    function test_compile_boundary_no_creation_until_selected() {
        const value = create("media")
        compare(State.created, 0); compare(Weather.refreshes, 0)
        value.expanded = false; value.contentActive = false; value.bare = true
        compare(State.created, 0)
        const broken = createTemporaryObject(boundary, test, {kind: "media"})
        verify(broken !== null); compare(broken.status, Loader.Ready)
        compare(broken.item.implicitHeight, 20)
        ignoreWarning(/.*IrisSidebarWeatherSection.qml.*Expected token.*/)
        broken.kind = "weather"
        compare(broken.item.status, Loader.Error); compare(broken.item.item, null)
        compare(State.created, 0); compare(Weather.refreshes, 0)
        value.kind = "weather"
        const weather = ready(value)
        compare(State.created, 1); compare(State.initialHost, value)
        compare(weather.sectionHost, value); compare(Weather.refreshes, 0)
    }
    function test_enabled_unavailable_busy_and_forecast() {
        const value = create(), weather = ready(value)
        const summary = findChild(weather, "summaryColumn"), body = findChild(weather, "body")
        compare(weather.hasData, true)
        compare(withProperty(summary, "text", "Sunny  ·  Feels like 20°C").text, "Sunny  ·  Feels like 20°C")
        for (const text of ["21°C", "Sunny", "Feels like 20°C", "42%"])
            verify(texts(body).includes(text), "missing forecast text " + text)
        compare(body.children[0].visible, true); compare(body.children[1].visible, true)
        forecast("--°C")
        compare(weather.hasData, false)
        compare(body.children[0].visible, false); compare(body.children[1].visible, false)
        compare(withProperty(body, "text", "Weather unavailable. Try refreshing.").visible, true)
        compare(withProperty(summary, "text", "Fetching your forecast…").text, "Fetching your forecast…")
        Weather.busy = true
        compare(withProperty(body, "text", "Fetching your forecast…").visible, true)
        forecast("--°F"); compare(weather.hasData, false)
        Weather.enabled = false
        compare(withProperty(body, "text", "Enable weather in Settings to see your forecast.").visible, true)
        compare(withProperty(summary, "text", "Weather is off").text, "Weather is off")
        forecast()
        compare(weather.hasData, true); compare(body.children[0].visible, false)
        Weather.enabled = true
        compare(body.children[0].visible, true); compare(Weather.refreshes, 0)
    }
    function test_city_privacy_and_collapsed_temperature() {
        const value = create("weather", false), weather = ready(value)
        compare(weather.title, "Gentoo City"); compare(weather.detail, "21°C")
        Weather.showVisibleCity = false; compare(weather.title, "Weather")
        Weather.showVisibleCity = true; Weather.visibleCity = ""; compare(weather.title, "Weather")
        Weather.visibleCity = "Private City"; compare(weather.title, "Private City")
        value.expanded = true; compare(weather.detail, "")
        value.expanded = false; forecast("17°C"); compare(weather.detail, "17°C")
        forecast("--°C"); compare(weather.detail, "")
        compare(Weather.refreshes, 0)
    }
    function test_day_night_icon_tint_and_fallbacks() {
        const value = create(), weather = ready(value), body = findChild(weather, "body")
        compare(weather.night, false); compare(weather.condition, "sunny"); compare(weather.glyph, "sunny")
        compare(weather.tint, IrisStyle.skyLight("sunny"))
        compare(withProperty(body, "iconSize", 46).text, "sunny")
        compare(withProperty(body, "iconSize", 46).color, IrisStyle.secondaryAccent)
        Weather.night = true
        compare(weather.night, true); compare(weather.condition, "night_clear"); compare(weather.glyph, "night_clear")
        compare(weather.tint, IrisStyle.skyLight("night_clear"))
        compare(withProperty(body, "iconSize", 46).text, "night_clear")
        compare(withProperty(body, "iconSize", 46).color, Qt.lighter(IrisStyle.identity.lavender, 1))
        forecast("21°C", -1)
        compare(weather.condition, ""); compare(weather.glyph, "bedtime")
        compare(weather.tint.toString(), "#4b0082")
        compare(withProperty(body, "iconSize", 46).text, "cloud")
        Weather.night = false; compare(weather.glyph, "partly_cloudy_day")
        compare(weather.tint.toString(), "#0000ff")
        forecast("--°C")
        compare(weather.condition, ""); compare(weather.glyph, "partly_cloudy_day")
        Weather.night = true; compare(weather.glyph, "bedtime")
        compare(Weather.refreshes, 0)
    }
    function test_refresh_enabled_and_one_click() {
        const value = create(), weather = ready(value), refresh = withProperty(weather, "materialIcon", "refresh")
        compare(refresh.enabled, true)
        Weather.busy = true; compare(refresh.enabled, false)
        wait(250); mouseClick(refresh); compare(Weather.refreshes, 0)
        Weather.busy = false; Weather.enabled = false; compare(refresh.enabled, false)
        mouseClick(refresh); compare(Weather.refreshes, 0)
        Weather.enabled = true; compare(refresh.enabled, true)
        wait(250)
        mouseClick(refresh)
        compare(Weather.refreshes, 1)
    }
    function test_geometry_morph_density_residency_no_implicit_refresh() {
        const value = create("weather", false), weather = ready(value)
        wait(250)
        compare(weather.width, 420); compare(value.item.width, 420)
        compare(value.implicitHeight, weather.implicitHeight)
        const collapsed = weather.implicitHeight
        const header = findChild(weather, "header")
        let toggles = 0
        value.toggleRequested.connect(() => ++toggles)
        header.clicked(null); compare(toggles, 1)
        value.expanded = true
        tryCompare(value, "morphing", true)
        tryCompare(value, "morphing", false, 1000)
        verify(weather.implicitHeight > collapsed)
        value.width = 500; compare(weather.width, 500)
        value.contentActive = false; value.expanded = false
        compare(ready(value), weather); compare(State.created, 1)
        forecast("19°C", 0, "Clear")
        compare(weather.detail, "19°C"); compare(weather.hasData, true)
        value.bare = true; compare(weather.open, true); compare(weather.radius, 0); compare(header.enabled, false)
        IrisStyle.density = 1.5; compare(weather.pad, 21); compare(weather.gap, 15)
        const body = findChild(weather, "body")
        compare(withProperty(body, "text", "humidity_percentage").iconSize, 21)
        compare(withProperty(body, "text", "sunny").iconSize, 69)
        value.bare = false; value.expanded = true; value.contentActive = true
        compare(ready(value), weather); compare(State.created, 1); compare(Weather.refreshes, 0)
        value.kind = "media"; wait(0); compare(State.destroyed, 1)
        compare(Weather.refreshes, 0)
        value.kind = "weather"
        const reopened = ready(value)
        compare(State.created, 2); compare(State.initialHost, value); compare(reopened.hasData, true)
        compare(withProperty(reopened, "text", "19°C").text, "19°C")
        compare(Weather.refreshes, 0)
    }
}
''')


def main():
    runners = [os.environ.get("QMLTESTRUNNER", ""), "/usr/lib/qt6/bin/qmltestrunner",
               "/usr/lib64/qt6/bin/qmltestrunner", "/usr/lib/x86_64-linux-gnu/qt6/bin/qmltestrunner",
               shutil.which("qmltestrunner") or ""]
    with tempfile.TemporaryDirectory(prefix="inir-weather-section-") as folder:
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
            result = subprocess.run([runner, "-input", str(directory / "tst_weather.qml")],
                                    env=environment, capture_output=True, text=True, timeout=30)
            output = result.stdout + result.stderr
            print(output, end="")
            return 1 if result.returncode or "FAIL!" in output else 0
    print("Iris Weather: no Qt 6 qmltestrunner found; native checks not executed")
    return 0


if __name__ == "__main__":
    sys.exit(main())
