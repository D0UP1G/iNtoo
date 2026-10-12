#!/usr/bin/env python3
"""Exercise the deferred real System frame/rings and ResourceUsage ownership."""
import importlib.util
import os
from pathlib import Path
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
    component = focus.helper.block(content, r"Component\s*\{\s*id: systemComponent\b")
    if "ResourceUsage" in component or "active:" in component or "asynchronous:" in component:
        raise ValueError("System host must remain a synchronous resident URL loader")
    system_file = directory / "IrisSidebarSystemSection.qml"
    system = system_file.read_text()
    system = system.replace("import QtQuick\n", 'import QtQuick\nimport "state.js" as State\n', 1)
    system = system.replace("Component.onCompleted: systemCard.syncPolling()",
                            "Component.onCompleted: { ++State.created; systemCard.syncPolling() }")
    system = system.replace("Component.onDestruction: if (systemCard.holding) ResourceUsage.releaseKeepAlive()",
                            "Component.onDestruction: { ++State.destroyed; if (systemCard.holding) ResourceUsage.releaseKeepAlive() }")
    system_file.write_text(system)
    deferred = directory / "Deferred/IrisSidebarSystemSection.qml"
    deferred.unlink()
    deferred.write_text("import QtQuick\nItem { impossible syntax !!! }\n")
    (directory / "tst_system.qml").write_text('''pragma ComponentBehavior: Bound
import QtQuick
import QtTest
import "Fixture"
import "Deferred" as Deferred
import "state.js" as State
TestCase {
    id: test
    name: "IrisSidebarSystemSection"
    visible: true
    width: 700; height: 900
    when: windowShown
    Component { id: payload; SectionPayload {} }
    Component { id: boundary; Deferred.SectionPayload {} }
    function create(active = true, kind = "system") {
        const value = createTemporaryObject(payload, test, {kind: kind, contentActive: active, width: 420})
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
    function texts(item) { return descendants(item).filter(child => child.text !== undefined).map(child => child.text) }
    function init() {
        IrisStyle.density = 1
        ResourceUsage.acquired = 0; ResourceUsage.released = 0
        ResourceUsage.cpuUsage = 0.5; ResourceUsage.memoryUsedPercentage = 0.4
        ResourceUsage.tempPercentage = 0.2; ResourceUsage.diskUsedPercentage = 0.8
        ResourceUsage.maxTemp = 50; ResourceUsage.tempWarningThreshold = 85
        State.created = 0; State.destroyed = 0
    }
    function test_compile_boundary_and_no_non_system_creation() {
        const value = create(true, "media")
        compare(State.created, 0); compare(ResourceUsage.acquired, 0)
        value.expanded = true; value.bare = true; value.contentActive = false
        compare(State.created, 0); compare(ResourceUsage.acquired, 0)
        const broken = createTemporaryObject(boundary, test, {kind: "media"})
        verify(broken !== null); compare(broken.status, Loader.Ready)
        compare(broken.item.implicitHeight, 20)
        ignoreWarning(/.*IrisSidebarSystemSection.qml.*Expected token.*/)
        broken.kind = "system"
        compare(broken.item.status, Loader.Error); compare(broken.item.item, null)
        compare(State.created, 0); compare(ResourceUsage.acquired, 0)
    }
    function test_initial_false_idempotence_and_balance() {
        const value = create(false), system = ready(value)
        compare(State.created, 1); compare(system.sectionHost, value)
        compare(system.polling, false); compare(system.holding, false)
        compare(ResourceUsage.acquired, 0); compare(ResourceUsage.released, 0)
        system.syncPolling(); system.syncPolling()
        compare(ResourceUsage.acquired, 0)
        value.contentActive = true
        compare(system.holding, true); compare(ResourceUsage.acquired, 1)
        system.syncPolling(); system.syncPolling(); value.contentActive = true
        compare(ResourceUsage.acquired, 1)
        value.contentActive = false
        compare(system.holding, false); compare(ResourceUsage.released, 1)
        system.syncPolling(); value.contentActive = false
        compare(ResourceUsage.released, 1); compare(ready(value), system)
        value.kind = "media"
        wait(0); compare(State.destroyed, 1); compare(ResourceUsage.released, 1)
    }
    function test_destroy_holding_and_reopen_new_consumer() {
        const value = create(), system = ready(value)
        compare(system.holding, true); compare(ResourceUsage.acquired, 1)
        value.kind = "media"
        wait(0); compare(State.destroyed, 1); compare(ResourceUsage.released, 1)
        value.kind = "system"
        const reopened = ready(value)
        compare(State.created, 2); compare(ResourceUsage.acquired, 2)
        compare(reopened.holding, true)
        value.destroy()
        wait(0); compare(State.destroyed, 2); compare(ResourceUsage.released, 2)
    }
    function test_geometry_flags_do_not_register() {
        const value = create(), system = ready(value)
        wait(250)
        compare(system.expandable, false); compare(system.open, true)
        compare(system.width, 420); compare(value.item.width, 420)
        compare(value.implicitHeight, system.implicitHeight)
        verify(system.implicitHeight > 0)
        value.expanded = true; value.expanded = false; value.bare = true
        value.width = 500; IrisStyle.density = 1.5
        compare(system.width, 500); compare(system.pad, 21); compare(system.radius, 0)
        compare(ready(value), system); compare(State.created, 1)
        compare(ResourceUsage.acquired, 1); compare(ResourceUsage.released, 0)
        for (const ring of rings(system)) { compare(ring.density, 1.5); compare(ring.stroke, 8) }
        value.contentActive = false
        value.expanded = true; value.bare = false; value.width = 380; IrisStyle.density = 0.8
        compare(system.polling, false); compare(system.holding, false)
        compare(ResourceUsage.acquired, 1); compare(ResourceUsage.released, 1)
        compare(ready(value), system)
    }
    function test_metrics_clamp_warning_and_rendered_values() {
        const value = create(false), system = ready(value)
        let rs = rings(system)
        compare(rs.length, 4)
        const levels = [0.5, 0.4, 0.2, 0.8]
        for (let i = 0; i < 4; ++i) {
            compare(rs[i].progress, levels[i]); compare(rs[i].tint, IrisStyle.accent)
            compare(rs[i].data[1].pathElements[0].sweepAngle, 360 * levels[i])
        }
        for (const text of ["CPU", "Memory", "Heat", "Disk", "50%", "40%", "50°", "80%"])
            verify(texts(system).includes(text), "missing metric text " + text)
        ResourceUsage.cpuUsage = 2; ResourceUsage.memoryUsedPercentage = -2
        ResourceUsage.tempPercentage = 0.7; ResourceUsage.tempWarningThreshold = 70
        ResourceUsage.maxTemp = 92; ResourceUsage.diskUsedPercentage = 0.9
        rs = rings(system)
        compare(rs[0].progress, 1); compare(rs[0].tint, IrisStyle.danger)
        compare(rs[1].progress, 0); compare(rs[1].tint, IrisStyle.accent)
        compare(rs[2].progress, 0.7); compare(rs[2].tint, IrisStyle.danger)
        compare(rs[3].progress, 0.9); compare(rs[3].tint, IrisStyle.danger)
        for (const text of ["200%", "-200%", "92°", "90%"])
            verify(texts(system).includes(text), "missing updated metric text " + text)
        ResourceUsage.cpuUsage = Number.NaN; ResourceUsage.diskUsedPercentage = 0.899
        rs = rings(system)
        compare(rs[0].progress, 0); compare(rs[3].tint, IrisStyle.accent)
        compare(ResourceUsage.acquired, 0); compare(ResourceUsage.released, 0)
    }
}
''')


def main():
    runners = [os.environ.get("QMLTESTRUNNER", ""), "/usr/lib/qt6/bin/qmltestrunner",
               "/usr/lib64/qt6/bin/qmltestrunner", "/usr/lib/x86_64-linux-gnu/qt6/bin/qmltestrunner",
               shutil.which("qmltestrunner") or ""]
    with tempfile.TemporaryDirectory(prefix="inir-system-section-") as folder:
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
            result = subprocess.run([runner, "-input", str(directory / "tst_system.qml")],
                                    env=environment, capture_output=True, text=True, timeout=30)
            output = result.stdout + result.stderr
            print(output, end="")
            return 1 if result.returncode or "FAIL!" in output else 0
    print("Iris System: no Qt 6 qmltestrunner found; native checks not executed")
    return 0


if __name__ == "__main__":
    sys.exit(main())
