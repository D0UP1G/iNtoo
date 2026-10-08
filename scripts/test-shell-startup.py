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
        # Keep hardware-dependent panels closed while the actual component
        # tree compiles and the deferred startup phases run.
        self.config["enabledPanels"] = ["audit-no-panels"]
        self.config["shellUpdates"] = {"enable": False}
        self.config["idle"] = {"enable": False}
        self.write_config()
        (self.base / "state/quickshell/user/first_run.txt").write_text("audit fixture\n")

    def write_config(self):
        (self.base / "config/inir/config.json").write_text(json.dumps(self.config))

    def run_qml(self, path, seconds=2, wayland=False):
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
        self.write_config()
        (self.base / "config/illogical-impulse").symlink_to("inir", target_is_directory=True)
        log = self.run_qml(SOURCE, seconds=5, wayland=True)
        self.assertRegex(log, r"\[Boot\].*Config.ready", log)
        self.assertRegex(log, r"\[Boot\].*shellEntryReady", log)
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
