#!/usr/bin/env python3
"""Load the real QML entrypoint without access to the user's desktop or network.

Requires Linux, Bubblewrap, Quickshell, dbus-run-session and timeout; family
startup also needs Sway. The source tree is read-only; HOME and every XDG
directory are temporary. Separate PID and network namespaces keep shell
helpers away from the live session. Imports and configuration use offscreen
Qt; family boot gates use a private headless Wayland compositor.
"""

import json
import os
from pathlib import Path
import re
import shutil
import subprocess
import sys
import tempfile
import time
import unittest

ROOT = Path(__file__).resolve().parents[1]
SOURCE = "/tmp/audit-source"
WORK = "/tmp/audit-work"


def run_headless_qml(path, seconds):
    # This entrypoint is only for the child running inside our Bubblewrap.
    # Refuse to start another compositor if invoked from the host directly.
    if ROOT != Path(SOURCE) or os.environ.get("XDG_RUNTIME_DIR") != WORK + "/runtime":
        raise RuntimeError("headless runner must run inside the audit sandbox")
    config = Path(WORK) / "sway.conf"
    config.write_text('xwayland disable\noutput HEADLESS-1 mode 1280x720\n')
    environment = dict(os.environ, WLR_BACKENDS="headless", WLR_RENDERER="pixman",
                       WLR_HEADLESS_OUTPUTS="1")
    logfile = Path(WORK) / "sway.log"
    with logfile.open("w") as output:
        compositor = subprocess.Popen(["sway", "-c", str(config)], env=environment,
                                      stdout=output, stderr=subprocess.STDOUT)
        try:
            deadline = time.monotonic() + 5
            while time.monotonic() < deadline:
                if compositor.poll() is not None:
                    raise RuntimeError("headless Sway exited\n" + logfile.read_text())
                sockets = [p for p in Path(environment["XDG_RUNTIME_DIR"]).glob("wayland-*")
                           if p.is_socket()]
                if sockets:
                    environment["WAYLAND_DISPLAY"] = sockets[0].name
                    result = subprocess.run(
                        ["timeout", "--kill-after=1s", f"{seconds}s", "qs", "-n", "-p", path],
                        env=environment, timeout=seconds + 3)
                    return result.returncode
                time.sleep(0.05)
            raise RuntimeError("headless Wayland socket unavailable\n" + logfile.read_text())
        finally:
            compositor.terminate()
            try:
                compositor.wait(timeout=2)
            except subprocess.TimeoutExpired:
                compositor.kill()
                compositor.wait(timeout=2)


class ShellStartupTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        required = ("bwrap", "qs", "dbus-run-session", "timeout")
        missing = [command for command in required if not shutil.which(command)]
        if missing:
            message = "native startup prerequisites unavailable: " + ", ".join(missing)
            if os.environ.get("INIR_REQUIRE_NATIVE_STARTUP") == "1":
                raise RuntimeError(message)
            raise unittest.SkipTest(message)

    def setUp(self):
        temporary = tempfile.TemporaryDirectory(prefix="intoo shell audit ")
        self.addCleanup(temporary.cleanup)
        self.base = Path(temporary.name)
        for relative in ("home", "config/inir", "state/quickshell/user", "cache", "runtime", "probe"):
            (self.base / relative).mkdir(parents=True, exist_ok=True)
        (self.base / "runtime").chmod(0o700)
        self.config = json.loads((ROOT / "defaults/config.json").read_text())
        self.config["panelFamily"] = "iris"
        # A nonempty, unknown panel id keeps shell.qml from adding defaults.
        # Family-specific panels are selected below so these checks compile
        # and instantiate the same critical surfaces users actually see.
        self.config["enabledPanels"] = ["audit-no-panels"]
        self.config["shellUpdates"] = {"enable": False}
        self.config["idle"] = {"enable": False}
        self.write_config()
        (self.base / "state/quickshell/user/first_run.txt").write_text("audit fixture\n")

    def write_config(self):
        (self.base / "config/inir/config.json").write_text(json.dumps(self.config))

    def run_qml(self, path, seconds=2, wayland=False, source_overrides=()):
        if wayland and not shutil.which("sway"):
            message = "family startup requires Sway for an isolated headless Wayland backend"
            if os.environ.get("INIR_REQUIRE_NATIVE_STARTUP") == "1":
                self.fail(message)
            self.skipTest(message)
        args = ["bwrap", "--ro-bind", "/", "/", "--unshare-net", "--unshare-pid",
                "--unshare-ipc", "--unshare-uts", "--die-with-parent", "--proc", "/proc",
                "--dev", "/dev", "--tmpfs", "/tmp", "--tmpfs", "/run", "--tmpfs", "/home",
                "--ro-bind", str(ROOT), SOURCE, "--bind", str(self.base), WORK,
                "--chdir", WORK,
                "--setenv", "PATH", "/usr/bin:/bin:/usr/sbin:/sbin"]
        for relative, fixture in source_overrides:
            args += ["--ro-bind", str(fixture), SOURCE + "/" + relative]
        environment = {
            "HOME": WORK + "/home",
            "XDG_CONFIG_HOME": WORK + "/config",
            "XDG_STATE_HOME": WORK + "/state",
            "XDG_CACHE_HOME": WORK + "/cache",
            "XDG_DATA_HOME": WORK + "/data",
            "XDG_RUNTIME_DIR": WORK + "/runtime",
            "QT_QPA_PLATFORM": "wayland" if wayland else "offscreen",
            "QS_DISABLE_CRASH_HANDLER": "1",
            "INIR_DISABLE_HOT_RELOAD": "1",
            "INIR_STANDALONE_WINDOW": "1",
        }
        if wayland:
            environment["QT_QUICK_BACKEND"] = "software"
        for name, value in environment.items():
            args += ["--setenv", name, value]
        for name in ("DBUS_SESSION_BUS_ADDRESS", "DBUS_SYSTEM_BUS_ADDRESS", "NIRI_SOCKET",
                     "WAYLAND_DISPLAY", "WAYLAND_SOCKET", "SWAYSOCK", "DISPLAY", "HYPRLAND_INSTANCE_SIGNATURE",
                     "QS_CONFIG_PATH", "QS_CONFIG_NAME", "QS_MANIFEST",
                     "QML_IMPORT_PATH", "QML2_IMPORT_PATH", "INIR_SESSION_SHELL_PGID"):
            args += ["--unsetenv", name]
        args += ["--", "dbus-run-session", "--"]
        if wayland:
            args += ["python3", SOURCE + "/scripts/test-shell-startup.py",
                     "--headless-runner", path, str(seconds)]
        else:
            args += ["timeout", "--kill-after=1s", f"{seconds}s",
                     shutil.which("qs"), "-n", "-p", path]
        result = subprocess.run(args, text=True, capture_output=True, timeout=seconds + 12)
        log = result.stdout + result.stderr
        self.assertIn("Configuration Loaded", log, log)
        self.assertEqual(result.returncode, 124, log)
        self.assertNotIn("Failed to load configuration", log, log)
        return log

    def module_probe(self, source):
        directory = self.base / "probe"
        # Register only this probe's real dependencies. Importing the full root
        # qmldir pulls in GlobalStates and its unavailable Hyprland dependency,
        # which would hide the independent config-path regression.
        (directory / "qmldir").write_text("module qs\n")
        modules = {
            "modules/common": ("Directories", "Config"),
            "modules/common/functions": ("FileUtils",),
            "services": ("SystemInfo",),
        }
        for relative, names in modules.items():
            target = directory / relative
            target.mkdir(parents=True)
            (target / "qmldir").write_text("\n".join(
                f"singleton {name} 1.0 {name}.qml" for name in names) + "\n")
            for name in names:
                shutil.copy(ROOT / relative / (name + ".qml"), target / (name + ".qml"))
        helper = directory / "scripts/lib"
        helper.mkdir(parents=True)
        shutil.copy(ROOT / "scripts/lib/config-path.sh", helper / "config-path.sh")
        (directory / "audit.qml").write_text(source)
        return WORK + "/probe/audit.qml"

    def shell_family(self, family):
        self.config["panelFamily"] = family
        self.config["enabledPanels"] = {
            "ii": ["iiBackground", "iiBar"],
            "waffle": ["wBackground", "wBar", "wBackdrop"],
            "iris": ["irisBar"],
        }[family]
        self.write_config()
        (self.base / "config/illogical-impulse").symlink_to("inir", target_is_directory=True)
        source_overrides = []
        if family == "iris":
            # Observe actual deferred implementation creation in the real shell.
            # A marker in a temporary bind-mounted copy leaves production QML
            # unchanged and cannot be emitted by critical-host readiness alone.
            relative = "modules/iris/ShellIrisPanelsImpl.qml"
            implementation = (ROOT / relative).read_text()
            self.assertEqual(implementation.count("    id: root\n"), 1)
            fixture = self.base / "probe/ShellIrisPanelsImpl.qml"
            fixture.write_text(implementation.replace(
                "    id: root\n", "    id: root\n"
                '    Component.onCompleted: console.log("AUDIT_IRIS_DEFERRED_CREATED")\n', 1))
            source_overrides.append((relative, fixture))
        log = self.run_qml(SOURCE, seconds=18, wayland=True,
                           source_overrides=source_overrides)
        self.assertRegex(log, r"\[Boot\].*Config.ready", log)
        self.assertRegex(log, r"\[Boot\].*shellEntryReady", log)
        self.assertIn(f"[FamilyLoader] ready {family}", log, log)
        critical_sources = {
            "ii": ["modules/background/Background.qml", "modules/barM3/M3Bar.qml"],
            "waffle": ["modules/waffle/background/WaffleBackground.qml",
                       "modules/waffle/bar/WaffleBar.qml",
                       "modules/waffle/backdrop/WaffleBackdrop.qml"],
            "iris": ["modules/iris/bar/IrisBar.qml",
                     "modules/iris/frame/IrisReservations.qml"],
        }[family]
        for source in critical_sources:
            self.assertIn("[PanelLoader] ready " + source, log, log)
        if family == "iris":
            self.assertIn("AUDIT_IRIS_DEFERRED_CREATED", log, log)
        self.assertNotIn("[PanelLoader] Could not compile", log, log)
        self.assertNotIn("[FamilyLoader] Could not compile", log, log)
        self.assertNotRegex(log, r"(?:Type \w+ unavailable|\w+ is not a type|Failed to create LazyLoader component|Cannot assign to non-existent property)", log)
        self.assertNotRegex(log, r"(?:TypeError|ReferenceError):", log)
        # The boot report is written only after the deferred phases finish.
        reports = list((self.base / "cache").rglob("last-boot.json"))
        self.assertTrue(reports, "late startup did not write its boot report\n" + log)

    def test_a00_shell_ii_loads(self):
        self.shell_family("ii")

    def test_a00_shell_waffle_loads(self):
        self.shell_family("waffle")

    def test_a00_shell_iris_loads(self):
        self.shell_family("iris")

    def test_async_panel_source_lifecycle(self):
        directory = self.base / "probe"
        shutil.copy(ROOT / "modules/common/AsyncPanelLoader.qml",
                    directory / "AsyncPanelLoader.qml")
        for name in ("one", "two"):
            (directory / (name + ".qml")).write_text(
                'import QtQuick\nItem {\n'
                '  Component.onCompleted: console.log("AUDIT_PANEL_CREATED_' + name + '")\n'
                '  Component.onDestruction: console.log("AUDIT_PANEL_DESTROYED_' + name + '")\n'
                '}\n')
        (directory / "invalid.qml").write_text('import QtQuick\nItem { invalidProperty: true }\n')
        (directory / "async.qml").write_text('''import QtQuick
import Quickshell
ShellRoot {
    id: root
    property int step: 0
    property int failures: 0
    AsyncPanelLoader {
        id: panel
        sourcePath: "invalid.qml"
        onFailed: root.failures++
    }
    Timer {
        interval: 100
        repeat: true
        running: true
        onTriggered: {
            if (root.step === 0) {
                if (panel.active || root.failures) throw new Error("inactive source compiled");
                console.log("AUDIT_ASYNC_INACTIVE_OK");
                panel.sourcePath = "one.qml";
                panel.requested = true;
                root.step++;
            } else if (root.step === 1 && panel.active) {
                panel.requested = false;
                root.step++;
            } else if (root.step === 2 && !panel.active) {
                // Cancel a request before its asynchronous compilation settles.
                panel.requested = true;
                panel.requested = false;
                panel.sourcePath = "two.qml";
                panel.requested = true;
                root.step++;
            } else if (root.step === 3 && panel.active) {
                // A source change while requested must replace the old item.
                panel.sourcePath = "one.qml";
                root.step++;
            } else if (root.step === 4 && panel.active) {
                panel.sourcePath = "invalid.qml";
                root.step++;
            } else if (root.step === 5 && root.failures === 1 && !panel.active) {
                console.log("AUDIT_ASYNC_LIFECYCLE_OK");
                root.step++;
            }
        }
    }
}
''')
        log = self.run_qml(WORK + "/probe/async.qml", seconds=3)
        self.assertIn("AUDIT_ASYNC_INACTIVE_OK", log, log)
        self.assertIn("AUDIT_ASYNC_LIFECYCLE_OK", log, log)
        self.assertEqual(log.count("AUDIT_PANEL_CREATED_one"), 2, log)
        self.assertEqual(log.count("AUDIT_PANEL_CREATED_two"), 1, log)
        self.assertIn("AUDIT_PANEL_DESTROYED_one", log, log)
        self.assertIn("AUDIT_PANEL_DESTROYED_two", log, log)
        self.assertNotRegex(log, r"(?:TypeError|ReferenceError|Error):", log)

    def test_iris_tool_loaders_initialize_screen_before_completion(self):
        directory = self.base / "probe"
        (directory / "qmldir").write_text(
            "module qs\nsingleton Config 1.0 Config.qml\n"
            "singleton GlobalStates 1.0 GlobalStates.qml\n"
            "singleton IrisFrame 1.0 IrisFrame.qml\n")
        (directory / "Config.qml").write_text(
            'pragma Singleton\nimport QtQuick\nQtObject {\n'
            '    property bool ready: true\n'
            '    property var options: ({enabledPanels: ["irisPalette"], '
            'iris: {modules: {palette: true}, orbit: {enable: true}}})\n}\n')
        (directory / "GlobalStates.qml").write_text(
            'pragma Singleton\nimport QtQuick\nQtObject {\n'
            '    property bool deferredPanelsReady: true\n'
            '    property bool searchOpen: true\n'
            '    property bool wallpaperSelectorOpen: true\n'
            '    property bool irisOrbitOpen: true\n'
            '    property string wallpaperSelectorTargetMonitor: "AUDIT-1"\n'
            '    property string irisOrbitOutput: "AUDIT-1"\n'
            '    property var focusedScreen: ({name: "AUDIT-1"})\n}\n')
        (directory / "IrisFrame.qml").write_text(
            'pragma Singleton\nimport QtQuick\nQtObject { property int band: 0 }\n')
        source = (ROOT / "modules/iris/bar/IrisBar.qml").read_text()
        loaders = []
        tools = (("spotlightLoader", "palette/IrisPalette.qml"),
                 ("galleryLoader", "wallpaper/IrisWallpaperPicker.qml"),
                 ("orbitLoader", "orbit/IrisOrbit.qml"))
        for identifier, relative in tools:
            start = source.index("                Loader {\n                    id: " + identifier)
            end = source.index("\n                }", start) + len("\n                }")
            loaders.append(source[start:end])
            path = directory / relative
            path.parent.mkdir(exist_ok=True)
            path.write_text(
                'import QtQuick\nItem {\n'
                '    property var screen: null\n'
                '    property var completedScreen: null\n'
                '    Component.onCompleted: {\n'
                '        if (!screen || screen.name !== "AUDIT-1") '
                'throw new Error("screen missing during completion");\n'
                '        completedScreen = screen;\n'
                '        console.log("AUDIT_TOOL_CREATED", "' + identifier + '", screen.marker);\n'
                '    }\n'
                '    Component.onDestruction: console.log("AUDIT_TOOL_DESTROYED", "'
                + identifier + '")\n}\n')
        bar = directory / "bar"
        bar.mkdir()
        (bar / "ToolProbe.qml").write_text(
            'pragma ComponentBehavior: Bound\nimport QtQuick\nimport Quickshell\nimport qs\n'
            'Item {\n    id: root\n    property int step: 0\n'
            '    QtObject { id: firstScreen; property string name: "AUDIT-1"; property int marker: 1 }\n'
            '    QtObject { id: nextScreen; property string name: "AUDIT-1"; property int marker: 2 }\n'
            '    Item {\n        id: barWindow\n        property var screen: firstScreen\n'
            + "\n".join(loaders) + '''
        Timer {
            interval: 50
            running: true
            repeat: true
            onTriggered: {
                const panels = [spotlightLoader, galleryLoader, orbitLoader];
                if (root.step === 0 && panels.every(panel => panel.item)) {
                    if (!panels.every(panel => panel.item.completedScreen === firstScreen))
                        throw new Error("initial screen was not supplied during creation");
                    barWindow.screen = nextScreen;
                    root.step++;
                } else if (root.step === 1) {
                    if (!panels.every(panel => panel.item.screen === nextScreen
                            && panel.item.completedScreen === firstScreen))
                        throw new Error("screen Binding did not update the existing item");
                    GlobalStates.searchOpen = false;
                    GlobalStates.wallpaperSelectorOpen = false;
                    GlobalStates.irisOrbitOpen = false;
                    root.step++;
                } else if (root.step === 2) {
                    if (!panels.every(panel => !panel.item && panel.source.toString().length === 0))
                        throw new Error("closed tools retained their item or source");
                    GlobalStates.searchOpen = true;
                    GlobalStates.wallpaperSelectorOpen = true;
                    GlobalStates.irisOrbitOpen = true;
                    root.step++;
                } else if (root.step === 3 && panels.every(panel => panel.item)) {
                    if (!panels.every(panel => panel.item.completedScreen === nextScreen))
                        throw new Error("reopened tools received an obsolete screen");
                    console.log("AUDIT_TOOL_SCREEN_LIFECYCLE_OK");
                    root.step++;
                }
            }
        }
    }
}
''')
        (directory / "tools.qml").write_text(
            'import Quickshell\nimport "bar"\nShellRoot { ToolProbe {} }\n')
        log = self.run_qml(WORK + "/probe/tools.qml", seconds=3)
        self.assertIn("AUDIT_TOOL_SCREEN_LIFECYCLE_OK", log, log)
        for identifier, _ in tools:
            self.assertEqual(log.count("AUDIT_TOOL_CREATED " + identifier), 2, log)
            self.assertIn("AUDIT_TOOL_DESTROYED " + identifier, log, log)
        self.assertNotRegex(log, r"(?:TypeError|ReferenceError|Error):", log)

    def critical_readiness_probe(self, family, options, broken_source=None, noncreating=False,
                                 switch_from_waffle=False):
        # Use production hosts/loaders with small offscreen child fixtures. This
        # tests native async ordering without importing desktop services/windows.
        directory = self.base / "probe"
        (directory / "qmldir").write_text(
            "module qs\nsingleton GlobalStates 1.0 GlobalStates.qml\n")
        (directory / "GlobalStates.qml").write_text(
            "pragma Singleton\nimport QtQuick\nQtObject {\n"
            "    property bool deferredPanelsReady: true\n"
            "    property bool widgetEditMode: "
            + ("true" if options.get("auditWidgetEditMode") else "false") + "\n}\n")
        common = directory / "modules/common"
        common.mkdir(parents=True)
        (common / "qmldir").write_text(
            "module qs.modules.common\nAsyncPanelLoader 1.0 AsyncPanelLoader.qml\n"
            "singleton Config 1.0 Config.qml\n")
        shutil.copy(ROOT / "modules/common/AsyncPanelLoader.qml", common)
        options = dict(options, panelFamily="waffle" if switch_from_waffle else family)
        (common / "Config.qml").write_text(
            "pragma Singleton\nimport QtQuick\nQtObject {\n"
            "    property bool ready: true\n"
            "    property var options: (" + json.dumps(options) + ")\n"
            "    function setNestedValue(key: string, value: string): void {\n"
            "        options = Object.assign({}, options, {[key]: value});\n"
            '        console.log("AUDIT_FAMILY_SELECTED", value);\n'
            "    }\n}\n")
        style = directory / "modules/iris/style"
        style.mkdir(parents=True)
        (style / "qmldir").write_text(
            "module qs.modules.iris.style\nsingleton IrisGate 1.0 IrisGate.qml\n")
        (style / "IrisGate.qml").write_text(
            "pragma Singleton\nimport QtQuick\nQtObject { property bool official: "
            + ("false" if options.get("auditUnofficial") else "true") + " }\n")
        hosts = {}
        sources = []
        for panel_family, class_name in (("ii", "Ii"), ("iris", "Iris"), ("waffle", "Waffle")):
            relative = f"modules/{panel_family}/critical/Shell{class_name}CriticalPanels.qml"
            text = (ROOT / relative).read_text()
            target = directory / relative
            target.parent.mkdir(parents=True, exist_ok=True)
            target.write_text(text)
            hosts[panel_family] = relative
            sources += re.findall(r'sourcePath: "([^"]+)"', text)
        for source in sources:
            path = directory / source
            path.parent.mkdir(parents=True, exist_ok=True)
            if source == broken_source:
                body = ("required property string missing" if noncreating
                        else "invalidProperty: true")
            else:
                body = 'Component.onCompleted: console.log("AUDIT_CHILD_CREATED", "' + source + '")'
            path.write_text("import QtQuick\nItem { " + body + " }\n")

        shell = (ROOT / "shell.qml").read_text()
        start = shell.index("    component FamilyPanelHostLoader: Item {")
        end = shell.index("    // Same reason as the routers:", start)
        # Accelerate only the extracted test guard: production remains 20 s.
        loader = shell[start:end].replace("interval: 20000", "interval: 1000")
        instances = [f'    FamilyPanelHostLoader {{ family: "{family}"; '
                     f'sourcePath: "{hosts[family]}"; criticalHost: true }}']
        if switch_from_waffle:
            instances.append('    FamilyPanelHostLoader { family: "waffle"; '
                             f'sourcePath: "{hosts["waffle"]}"; criticalHost: true }}')
        controller = '''
    property bool switched: false
    property bool reported: false
    Timer {
        interval: 50
        running: true
        repeat: true
        onTriggered: {
            if (!root.switched && root._lastGoodPanelFamily === "waffle") {
                root.switched = true;
                Config.setNestedValue("panelFamily", "iris");
            } else if (root.switched && !root.reported && Config.options.panelFamily === "waffle") {
                if (root._lastGoodPanelFamily !== "waffle") throw new Error("last-good was overwritten");
                root.reported = true;
                console.log("AUDIT_PREVIOUS_FAMILY_RESTORED");
            }
        }
    }
''' if switch_from_waffle else ""
        (directory / "readiness.qml").write_text(
            "pragma ComponentBehavior: Bound\nimport QtQuick\nimport Quickshell\n"
            "import qs\nimport qs.modules.common\nShellRoot {\n    id: root\n"
            "    function finishFamilyTransition(): void {}\n" + loader
            + "\n".join(instances) + controller + "\n}\n")
        return self.run_qml(WORK + "/probe/readiness.qml", seconds=3)

    def test_critical_readiness_success_waits_for_requested_children(self):
        cases = (
            ("ii", {"enabledPanels": ["iiBackground", "iiBar", "iiDock"]},
             ["modules/background/Background.qml", "modules/bar/Bar.qml", "modules/dock/Dock.qml"]),
            ("ii", {"enabledPanels": ["iiBar"], "bar": {"appearanceStyle": "pill"}},
             ["modules/pill/PillBar.qml"]),
            ("ii", {"enabledPanels": ["iiBar"], "bar": {"appearanceStyle": "m3"}},
             ["modules/barM3/M3Bar.qml"]),
            ("ii", {"enabledPanels": ["iiBar", "iiVerticalBar"], "bar": {"vertical": True}},
             ["modules/verticalBar/VerticalBar.qml"]),
            ("ii", {"enabledPanels": ["iiBar"], "bar": {"appearanceStyle": "pill"},
                    "auditWidgetEditMode": True}, []),
            ("waffle", {"enabledPanels": ["wBar", "wBackground", "wBackdrop"]},
             ["modules/waffle/bar/WaffleBar.qml", "modules/waffle/background/WaffleBackground.qml",
              "modules/waffle/backdrop/WaffleBackdrop.qml"]),
            ("waffle", {"enabledPanels": ["wBar", "wBackdrop"],
                        "waffles": {"background": {"backdrop": {"enable": False}}}},
             ["modules/waffle/bar/WaffleBar.qml"]),
            ("iris", {"enabledPanels": ["irisBar", "irisBackground"],
                      "iris": {"modules": {"desktopWidgets": False}}},
             ["modules/iris/bar/IrisBar.qml", "modules/iris/frame/IrisReservations.qml",
              "modules/iris/background/IrisBackground.qml"]),
            ("iris", {"enabledPanels": []}, ["modules/iris/frame/IrisReservations.qml"]),
        )
        for family, options, requested in cases:
            with self.subTest(family=family, options=options):
                # Each native run gets a fresh fixture tree.
                shutil.rmtree(self.base / "probe")
                (self.base / "probe").mkdir()
                log = self.critical_readiness_probe(family, options)
                ready = log.index("[FamilyLoader] ready " + family)
                for source in requested:
                    self.assertLess(log.index("[PanelLoader] ready " + source), ready, log)
                self.assertEqual(set(re.findall(r"AUDIT_CHILD_CREATED ([^\s]+)", log)),
                                 set(requested), log)
                self.assertEqual(log.count("[FamilyLoader] ready " + family), 1, log)
                self.assertNotIn("Switching back", log, log)
                self.assertNotRegex(log, r"(?:TypeError|ReferenceError|Unable to assign)", log)

    def test_critical_readiness_disabled_children_succeed(self):
        for family in ("ii", "waffle", "iris"):
            with self.subTest(family=family):
                shutil.rmtree(self.base / "probe")
                (self.base / "probe").mkdir()
                log = self.critical_readiness_probe(
                    family, {"enabledPanels": [], "auditUnofficial": True})
                self.assertIn("[FamilyLoader] ready " + family, log, log)
                self.assertNotIn("AUDIT_CHILD_CREATED", log, log)
                self.assertNotIn("Switching back", log, log)

    def test_critical_child_failure_preserves_previous_family(self):
        log = self.critical_readiness_probe(
            "iris", {"enabledPanels": ["wBar", "irisBar"]},
            broken_source="modules/iris/bar/IrisBar.qml", switch_from_waffle=True)
        self.assertIn("[FamilyLoader] ready waffle", log, log)
        self.assertIn("[FamilyLoader] critical panel failed", log, log)
        self.assertIn("Switching back to waffle after iris failed to load", log, log)
        self.assertIn("AUDIT_PREVIOUS_FAMILY_RESTORED", log, log)
        self.assertNotIn("[FamilyLoader] ready iris", log, log)
        self.assertNotIn("AUDIT_FAMILY_SELECTED ii", log, log)

    def test_critical_noncreating_child_keeps_watchdog_armed(self):
        # Synthetic creation failure, not a claim about current panel roots.
        # QQmlComponent is Ready here, but LazyLoader never has an active item.
        log = self.critical_readiness_probe(
            "iris", {"enabledPanels": ["wBar", "irisBar"]},
            broken_source="modules/iris/bar/IrisBar.qml", noncreating=True,
            switch_from_waffle=True)
        self.assertIn("Required property missing was not initialized", log, log)
        self.assertIn("iris critical panels did not become ready", log, log)
        self.assertIn("Switching back to waffle after iris failed to load", log, log)
        self.assertIn("AUDIT_PREVIOUS_FAMILY_RESTORED", log, log)
        self.assertNotIn("[FamilyLoader] ready iris", log, log)

    def test_a00_required_qml_imports_are_available(self):
        # Polkit is deliberately optional: services/PolkitService.qml loads it
        # dynamically and supplies a fallback. All modules below are imported
        # directly by the supported desktop's components.
        modules = ("Quickshell.Hyprland", "Quickshell.Bluetooth", "Quickshell.Networking",
                   "Quickshell.Services.Mpris", "Quickshell.Services.Notifications",
                   "Quickshell.Services.Pam", "Quickshell.Services.Pipewire",
                   "Quickshell.Services.SystemTray", "Quickshell.Services.UPower",
                   "Qt5Compat.GraphicalEffects", "QtMultimedia",
                   "org.kde.kirigami", "org.kde.syntaxhighlighting")
        directory = self.base / "imports"
        directory.mkdir()
        for module in modules:
            with self.subTest(module=module):
                file = directory / (module.replace(".", "_") + ".qml")
                file.write_text("import QtQuick\nimport Quickshell\nimport " + module + "\n"
                                "ShellRoot { Timer { interval: 50; running: true; "
                                'onTriggered: console.log("AUDIT_IMPORT_OK") } }\n')
                log = self.run_qml(WORK + "/imports/" + file.name, seconds=1)
                self.assertIn("AUDIT_IMPORT_OK", log, log)

    def test_a01_required_wayland_types_are_available(self):
        # Quickshell.Wayland can import successfully while individual features
        # are disabled. Compile these types without creating Wayland surfaces
        # so the check also works with Qt's offscreen platform.
        path = self.base / "probe/wayland.qml"
        path.write_text('''import QtQuick
import Quickshell
import Quickshell.Wayland
ShellRoot {
    Component { ScreencopyView {} }
    Component { WlSessionLock {} }
    Component { WlSessionLockSurface {} }
    Timer {
        interval: 50
        running: true
        onTriggered: console.log("AUDIT_WAYLAND_TYPES_OK")
    }
}
''')
        log = self.run_qml(WORK + "/probe/wayland.qml", seconds=1)
        self.assertIn("AUDIT_WAYLAND_TYPES_OK", log, log)

    def test_a01_doctor_runtime_probe_loads(self):
        # Exercise Doctor's actual QML probe in the same isolated runtime.
        # A successful module import alone cannot detect missing lock types.
        doctor = (ROOT / "sdata/lib/doctor.sh").read_text()
        probe = re.search(r"cat > .*?/probe\.qml\" <<'QML'\n(.*?)\nQML",
                          doctor, re.DOTALL)
        self.assertIsNotNone(probe, "Doctor's runtime QML probe was not found")
        path = self.base / "probe/doctor.qml"
        path.write_text(probe.group(1) + "\n")
        self.run_qml(WORK + "/probe/doctor.qml", seconds=3)

    def test_levendist_scores_match_in_native_qml(self):
        directory = self.base / "probe"
        shutil.copy(ROOT / "modules/common/functions/levendist.js",
                    directory / "optimized.js")
        regression = (ROOT / "scripts/test-levendist-performance.js").read_text()
        baseline = re.search(r"const baselineSource = String.raw`(.*?)`;", regression, re.DOTALL)
        self.assertIsNotNone(baseline, "independent scoring baseline is missing")
        (directory / "baseline.js").write_text(baseline.group(1))
        (directory / "scores.qml").write_text('''import QtQuick
import Quickshell
import "baseline.js" as Baseline
import "optimized.js" as Optimized
ShellRoot {
    Component.onCompleted: Qt.callLater(() => {
        const values = ["", "a", "ab", "ba", "firefox", "firefix", "configuration",
            "prefix configuratlon extra text", "😀", "😃", "e\\u0301", "é"];
        let comparisons = 0;
        for (const a of values) for (const b of values) {
            if (Optimized.levenshteinDistance(a, b) !== Baseline.levenshteinDistance(a, b)
                    || Optimized.partialRatio(a, b) !== Baseline.partialRatio(a, b)
                    || Optimized.computeScore(a, b) !== Baseline.computeScore(a, b)
                    || Optimized.computeTextMatchScore(a, b) !== Baseline.computeTextMatchScore(a, b)) {
                console.log("AUDIT_SCORE_MISMATCH", JSON.stringify([a, b]));
                return;
            }
            comparisons++;
        }
        console.log("AUDIT_NATIVE_SCORES", comparisons);
    })
}
''')
        log = self.run_qml(WORK + "/probe/scores.qml", seconds=2)
        self.assertIn("AUDIT_NATIVE_SCORES 144", log, log)
        self.assertNotIn("AUDIT_SCORE_MISMATCH", log, log)

    def test_hyprland_refresh_preserves_requests_in_native_qml(self):
        directory = self.base / "probe"
        (directory / "qmldir").write_text("module qs\n")
        services = directory / "services"
        services.mkdir()
        (services / "qmldir").write_text(
            "module qs.services\nsingleton CompositorService 1.0 CompositorService.qml\n"
            "singleton HyprlandData 1.0 HyprlandData.qml\n")
        (services / "CompositorService.qml").write_text(
            "pragma Singleton\nimport QtQuick\nQtObject { property bool isHyprland: true }\n")
        service = (ROOT / "services/HyprlandData.qml").read_text()
        service = service.replace('"/usr/bin/hyprctl"', '"' + WORK + '/probe/collector.py"')
        (services / "HyprlandData.qml").write_text(service)
        (directory / "collector.py").write_text('''#!/usr/bin/python3
import json
from pathlib import Path
import sys
import time
kind = sys.argv[1]
trace = Path(__file__).with_name("starts.txt")
with trace.open("a") as output:
    output.write(kind + "\\n")
attempt = trace.read_text().splitlines().count(kind)
time.sleep(0.1)
payload = {
    "clients": [{"address": "window", "title": "client" + str(attempt)}],
    "monitors": [{"name": "test"}], "layers": {},
    "workspaces": [{"id": 1}], "activeworkspace": {"id": 1},
}[kind]
print(json.dumps(payload))
''')
        (directory / "collector.py").chmod(0o755)
        (directory / "refresh.qml").write_text('''import QtQuick
import Quickshell
import qs.services
ShellRoot {
    property var service: HyprlandData
    Component.onCompleted: {
        for (let i = 0; i < 50; i++) service.updateAll();
    }
    Timer {
        interval: 30
        running: true
        onTriggered: {
            for (let i = 0; i < 50; i++) HyprlandData.updateWindowList();
        }
    }
    Timer {
        interval: 700
        running: true
        onTriggered: console.log("AUDIT_HYPR_REFRESH", HyprlandData.windowByAddress.window.title,
            HyprlandData.workspaceById[1].id, HyprlandData.activeWorkspace.id)
    }
}
''')
        log = self.run_qml(WORK + "/probe/refresh.qml", seconds=2)
        self.assertIn("AUDIT_HYPR_REFRESH client2 1 1", log, log)
        launches = (directory / "starts.txt").read_text().splitlines()
        self.assertEqual(launches.count("clients"), 2, launches)
        for category in ("monitors", "layers", "workspaces", "activeworkspace"):
            self.assertEqual(launches.count(category), 1, launches)

    def directory_probe(self):
        path = self.module_probe('''import QtQuick
import Quickshell
import qs.modules.common as Common
ShellRoot {
    property string family: Common.Config.options.panelFamily
    Timer {
        interval: 100
        running: Common.Config.ready
        onTriggered: console.log("AUDIT_CONFIG", JSON.stringify({
            path: Common.Config.filePath, family: Common.Config.options.panelFamily
        }))
    }
}
''')
        log = self.run_qml(path)
        match = re.search(r"AUDIT_CONFIG (\{[^\n]+\})", log)
        self.assertIsNotNone(match, "config never became ready\n" + log)
        return json.loads(match[1]), log

    def test_a10_canonical_only_config_matches_cli_resolution(self):
        # A package/manual install may have only the documented canonical path;
        # QML must not create a separate legacy config and silently use defaults.
        result, log = self.directory_probe()
        cli = subprocess.run(["bash", "-c", 'source "$1"; inir_config_file',
                              "audit", str(ROOT / "scripts/lib/config-path.sh")],
                             env=dict(os.environ, HOME=str(self.base / "home"),
                                      XDG_CONFIG_HOME=str(self.base / "config")),
                             text=True, capture_output=True, timeout=5)
        self.assertEqual(cli.returncode, 0, cli.stderr)
        qml_path = Path(result["path"].replace(WORK, str(self.base), 1))
        self.assertEqual(result["family"], "iris", log)
        self.assertEqual(qml_path.resolve(), Path(cli.stdout.strip()).resolve(),
                         "QML and the CLI selected different config files")

    def test_mascot_provider_and_model_selection(self):
        directory = self.base / "probe"
        for name in ("qmldir", "GlobalStates.qml", "modules", "services", "scripts",
                     "assets", "defaults", "translations"):
            (directory / name).symlink_to(SOURCE + "/" + name)
        (directory / "assistant.qml").write_text('''import QtQuick
import Quickshell
import qs.modules.common
import qs.modules.mascotAssistant
import qs.services
ShellRoot {
    MascotAssistantWindow { id: assistant }
    function check(value, message) {
        if (!value) throw new Error(message);
    }
    Timer {
        interval: 200
        running: Config.ready
        onTriggered: {
            try {
                check(MascotAssistant.providerId === "ollama", "default provider");
                check(MascotAssistant.modelReady, "default model");
                MascotAssistant.catalogModels = ["opencode/model-a", "opencode/model-b",
                    "github-copilot/copilot-model", "openrouter/vendor/model:free"];
                Config.setNestedValue("mascot.assistant.apiBase", "https://old-provider.invalid/v1");
                MascotAssistant.selectProvider("opencode");
                check(MascotAssistant.selectedModel === "opencode/model-a", "provider switches model");
                check(Config.options.mascot.assistant.apiBase === "", "old endpoint cleared");
                MascotAssistant.selectModel("model-b");
                check(MascotAssistant.selectedModel === "opencode/model-b", "model choice saved");
                MascotAssistant.selectProvider("openrouter");
                check(MascotAssistant.selectedModel === "openrouter/vendor/model:free", "nested model id");
                MascotAssistant.selectProvider("google");
                check(!MascotAssistant.modelReady, "no model from another provider");
                check(!MascotAssistant.send("blocked", false), "missing model blocks send");
                MascotAssistant.selectModel("manual-model");
                check(MascotAssistant.selectedModel === "google/manual-model", "manual model entry");
                MascotAssistant.selectProvider("ollama");
                check(MascotAssistant.selectedModel === "ollama/qwen3.5:0.8b", "local default restored");
                MascotAssistant.selectProvider("github-copilot");
                check(MascotAssistant.modelOptions[0].value === "github-copilot/copilot-model", "provider filter");
                MascotAssistant.opened = true;
                assistant.editingModel = true;
                check(MascotAssistant.send("selection probe", false), "selected model sends");
                const request = JSON.parse(MascotAssistant.requestData);
                check(request.provider === "github-copilot", "request provider");
                check(request.model === "github-copilot/copilot-model", "request model");
                MascotAssistant.selectProvider("ollama");
                check(MascotAssistant.providerId === "github-copilot", "busy selection locked");
                console.log("AUDIT_ASSISTANT_SELECTION_OK");
            } catch (error) {
                console.error("AUDIT_ASSISTANT_SELECTION_FAILED", error);
            }
        }
    }
}
''')
        log = self.run_qml(WORK + "/probe/assistant.qml", seconds=4, wayland=True)
        self.assertIn("AUDIT_ASSISTANT_SELECTION_OK", log, log)
        self.assertNotRegex(log, r"(?:TypeError|ReferenceError|AUDIT_ASSISTANT_SELECTION_FAILED)", log)
        saved = json.loads((self.base / "config/inir/config.json").read_text())
        self.assertEqual(saved["mascot"]["assistant"]["provider"], "github-copilot")
        self.assertEqual(saved["mascot"]["assistant"]["model"], "github-copilot/copilot-model")

    def test_control_legacy_real_directory_keeps_precedence(self):
        legacy = self.base / "config/illogical-impulse"
        legacy.mkdir()
        (legacy / "config.json").write_text(json.dumps({"panelFamily": "waffle"}))
        result, log = self.directory_probe()
        self.assertEqual(result["family"], "waffle", log)

    def test_control_compatibility_alias_reads_canonical_data(self):
        (self.base / "config/illogical-impulse").symlink_to("inir", target_is_directory=True)
        result, log = self.directory_probe()
        self.assertEqual(result["family"], "iris", log)


if __name__ == "__main__":
    if len(sys.argv) == 4 and sys.argv[1] == "--headless-runner":
        raise SystemExit(run_headless_qml(sys.argv[2], int(sys.argv[3])))
    unittest.main(verbosity=2)
