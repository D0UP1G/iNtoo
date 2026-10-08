#!/usr/bin/env python3
"""Check init routing, user configuration, and supervised process lifecycles."""
import importlib.util
import json
import os
from pathlib import Path
import socket
import signal
import shutil
import subprocess
import tempfile
import time
import unittest

ROOT = Path(__file__).resolve().parents[1]
BACKEND = ROOT / "scripts/lib/session-backend.sh"
SUPERVISOR = ROOT / "scripts/lib/session-supervisor.sh"
spec = importlib.util.spec_from_file_location("session_startup", ROOT / "scripts/lib/session-startup.py")
startup = importlib.util.module_from_spec(spec)
spec.loader.exec_module(startup)


class SessionTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory(prefix="inir openrc ")
        self.addCleanup(self.temp.cleanup)
        self.base = Path(self.temp.name)
        self.bin = self.base / "bin"
        self.runtime = self.base / "runtime"
        self.config = self.base / "config"
        self.payload = self.base / "payload"
        for directory in (self.bin, self.runtime, self.config / "niri", self.payload):
            directory.mkdir(parents=True)
        (self.payload / "shell.qml").write_text("import QtQuick\nItem {}\n")
        (self.config / "niri/config.kdl").write_text('// user config\nspawn-at-startup "user-tool"\n')
        self.calls = self.base / "calls"
        self.env = dict(os.environ, PATH=f"{self.bin}:{os.environ['PATH']}",
                        HOME=str(self.base), XDG_RUNTIME_DIR=str(self.runtime),
                        XDG_CONFIG_HOME=str(self.config), INIR_INIT_SYSTEM="openrc",
                        INIR_TEST_CALLS=str(self.calls), INIR_TEST_BASE=str(self.base),
                        INIR_QS_BIN=str(self.bin / "qs"), INIR_RUNTIME_DIR=str(self.payload),
                        INIR_LAUNCHER_PATH=str(ROOT / "scripts/inir"),
                        DBUS_SESSION_BUS_ADDRESS=f"unix:path={self.runtime}/bus",
                        WAYLAND_DISPLAY="wayland-test", NIRI_SOCKET=str(self.runtime / "niri-test.sock"),
                        INIR_RESTART_POLL_DELAY="0.05", INIR_RESTART_MAX_POLLS="100")
        for name in ("systemctl", "systemd-run", "loginctl", "wlsunset", "xembedsniproxy", "pkill", "pgrep"):
            self.stub(name, '#!/usr/bin/env python3\nimport json, os, sys\n'
                      'with open(os.environ["INIR_TEST_CALLS"], "a") as f:\n'
                      ' f.write(json.dumps([os.path.basename(sys.argv[0]), *sys.argv[1:]]) + "\\n")\n')
        self.sockets = []
        for name in ("wayland-test", "niri-test.sock"):
            sock = socket.socket(socket.AF_UNIX)
            sock.bind(str(self.runtime / name))
            self.sockets.append(sock)
            self.addCleanup(sock.close)
        self.processes = []
        self.addCleanup(self.stop_processes)

    def stub(self, name, content):
        path = self.bin / name
        path.write_text(content)
        path.chmod(0o755)
        return path

    def run_cmd(self, args, expected=0, env=None):
        result = subprocess.run(args, env=env or self.env, text=True, capture_output=True, timeout=15)
        self.assertEqual(result.returncode, expected, result.stdout + result.stderr)
        return result.stdout

    def bash(self, code, expected=0):
        return self.run_cmd(["bash", "-c", f'source "{BACKEND}"\n{code}'], expected)

    def recorded(self):
        return [json.loads(line) for line in self.calls.read_text().splitlines()] if self.calls.exists() else []

    def stop_processes(self):
        for process in self.processes:
            if process.poll() is None:
                process.terminate()
            try:
                process.wait(timeout=5)
            except subprocess.TimeoutExpired:
                process.kill()
                process.wait()

    def wait_until(self, condition):
        deadline = time.monotonic() + 7
        while time.monotonic() < deadline:
            try:
                if condition():
                    return
            except (FileNotFoundError, ValueError):
                pass
            time.sleep(0.05)
        self.fail("Timed out waiting for process state")

    def fake_qs(self, fail=False):
        return self.stub("qs", '''#!/usr/bin/env python3
import json, os, signal, sys, time
from pathlib import Path
base = Path(os.environ['INIR_TEST_BASE'])
pid_file = base / 'child.pid'
args = sys.argv[1:]
if '--version' in args:
 print('Quickshell test fixture'); raise SystemExit(0)
if 'list' in args:
 if os.environ.get('INIR_TEST_QS_NO_LIST') == '1':
  print('No running instances'); raise SystemExit(0)
 try:
  os.kill(int(pid_file.read_text()), 0)
  print('Process ID: ' + pid_file.read_text())
 except (OSError, ValueError): print('No running instances')
 raise SystemExit(0)
if 'kill' in args:
 try: os.kill(int(pid_file.read_text()), signal.SIGTERM)
 except (OSError, ValueError): pass
 raise SystemExit(1)
if 'log' in args:
 print('Failed to load configuration' if os.environ.get('INIR_TEST_QS_LOAD_FAIL') == '1' else 'Configuration Loaded')
 raise SystemExit(0)
with (base / 'starts').open('a') as f:
 f.write(json.dumps({'pid': os.getpid(), 'scale': os.environ.get('QT_SCALE_FACTOR'),
                    'rules': os.environ.get('QT_LOGGING_RULES'), 'args': args}) + '\\n')
pid_file.write_text(str(os.getpid()))
def stop(*args):
 pid_file.unlink(missing_ok=True)
 raise SystemExit(0)
signal.signal(signal.SIGTERM, stop)
signal.signal(signal.SIGINT, stop)
if os.environ.get('INIR_TEST_QS_FAIL') == '1':
 pid_file.unlink(missing_ok=True)
 raise SystemExit(1)
while True: time.sleep(0.1)
''')

    def start_supervisor(self, fail=False):
        qs = self.fake_qs()
        env = dict(self.env, INIR_TEST_QS_FAIL="1" if fail else "0", INIR_MAX_RAPID_RESTARTS="2")
        process = subprocess.Popen(["bash", str(SUPERVISOR), str(self.payload), str(qs)],
                                   env=env, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
        self.processes.append(process)
        return process

    def test_backend_rejects_installed_systemctl_on_openrc(self):
        self.bash("! inir_uses_systemd")
        self.assertEqual(self.recorded(), [])

    def test_backend_detects_user_manager_socket(self):
        (self.runtime / "systemd").mkdir()
        sock = socket.socket(socket.AF_UNIX)
        sock.bind(str(self.runtime / "systemd/private"))
        self.addCleanup(sock.close)
        self.env["INIR_INIT_SYSTEM"] = "auto"
        self.bash("inir_uses_systemd")

    def test_power_routes_exactly_once(self):
        for backend, binary in (("openrc", "loginctl"), ("systemd", "systemctl")):
            self.env["INIR_INIT_SYSTEM"] = backend
            for action in ("suspend", "hibernate", "reboot", "poweroff", "reboot-firmware"):
                before = len(self.recorded())
                self.run_cmd(["bash", str(ROOT / "scripts/session-power.sh"), action])
                calls = self.recorded()[before:]
                expected = ["reboot", "--firmware-setup"] if action == "reboot-firmware" else [action, "-i"]
                self.assertEqual(calls, [[binary, *expected]])

    def test_power_rejects_unrecognized_action(self):
        self.run_cmd(["bash", str(ROOT / "scripts/session-power.sh"), "invalid"], expected=2)
        self.assertEqual(self.recorded(), [])

    def test_daemon_openrc_start_and_stop(self):
        helper = str(ROOT / "scripts/session-daemon.sh")
        self.run_cmd(["bash", helper, "wlsunset", "start", "-t", "4500"])
        self.run_cmd(["bash", helper, "wlsunset", "stop"])
        self.assertEqual(self.recorded(), [["wlsunset", "-t", "4500"], ["pkill", "-u", str(os.getuid()), "-x", "wlsunset"]])

    def test_daemon_systemd_ownership(self):
        self.env["INIR_INIT_SYSTEM"] = "systemd"
        self.run_cmd(["bash", str(ROOT / "scripts/session-daemon.sh"), "xembedsniproxy", "start"])
        call = self.recorded()[0]
        self.assertEqual(call[0], "systemd-run")
        self.assertIn("--property=BindsTo=inir.service", call)
        self.assertIn("--setenv=QT_QPA_PLATFORM=xcb", call)

    def test_startup_idempotence_and_user_config(self):
        file = self.config / "niri/config.kdl"
        startup.sync(self.config, '/home/test user/bin/inir', "enable", True)
        first = file.read_text()
        startup.sync(self.config, '/home/test user/bin/inir', "enable", True)
        self.assertEqual(first, file.read_text())
        self.assertIn('spawn-at-startup "user-tool"', first)
        self.assertEqual(first.count('"run" "--session"'), 1)
        self.assertEqual(first.count('spawn-at-startup "gentoo-pipewire-launcher"'), 1)
        self.assertTrue(startup.sync(self.config, "inir", "status"))

    def test_startup_uses_only_active_includes(self):
        directory = self.config / "niri/config.d"
        directory.mkdir()
        target = directory / "50-startup.kdl"
        target.write_text('// inactive user file\n')
        startup.sync(self.config, "inir", "enable")
        self.assertNotIn('"--session"', target.read_text())
        root = self.config / "niri/config.kdl"
        root.write_text('include "config.d/50-startup.kdl"\nspawn-at-startup "inir" "start"\n')
        startup.sync(self.config, "inir", "enable")
        self.assertNotIn('"start"', root.read_text())
        self.assertIn('"--session"', target.read_text())

    def test_switch_to_systemd_removes_managed_startup(self):
        startup.sync(self.config, "inir", "enable", True)
        startup.sync(self.config, "inir", "disable")
        file = self.config / "niri/config.kdl"
        self.assertNotIn("OpenRC", file.read_text())
        self.assertNotIn("gentoo-pipewire-launcher", file.read_text())
        self.assertIn("user-tool", file.read_text())
        first = file.read_bytes()
        startup.sync(self.config, "inir", "disable")
        self.assertEqual(first, file.read_bytes())

    def test_startup_preserves_custom_audio(self):
        file = self.config / "niri/config.kdl"
        file.write_text('spawn-at-startup "gentoo-pipewire-launcher"\n')
        startup.sync(self.config, "inir", "enable", True)
        self.assertEqual(file.read_text().count('spawn-at-startup "gentoo-pipewire-launcher"'), 1)
        startup.sync(self.config, "inir", "disable")
        self.assertIn("gentoo-pipewire-launcher", file.read_text())

    def test_startup_preserves_unrelated_config_bytes(self):
        root = self.config / 'niri/config.kdl'
        directory = self.config / 'niri/config.d'
        directory.mkdir()
        startup_file = directory / '50-startup.kdl'
        unrelated = directory / '70-binds.kdl'
        root.write_text('include "config.d/50-startup.kdl"\ninclude "config.d/70-binds.kdl"\n\n\n')
        startup_file.write_text('// user startup\n\n\n')
        unrelated.write_text('// user bindings and intentional whitespace\n\n\n')
        before = {file: file.read_bytes() for file in (root, startup_file, unrelated)}
        startup.sync(self.config, 'inir', 'enable', True)
        self.assertEqual(root.read_bytes(), before[root])
        self.assertEqual(unrelated.read_bytes(), before[unrelated])
        startup.sync(self.config, 'inir', 'disable')
        self.assertEqual({file: file.read_bytes() for file in before}, before)

    def test_startup_handles_include_cycles(self):
        (self.config / "niri/config.kdl").write_text('include "config.kdl"\n')
        startup.sync(self.config, "inir", "enable")
        self.assertTrue(startup.sync(self.config, "inir", "status"))

    def test_startup_escaped_launcher_path_and_valid_kdl(self):
        launcher = '/home/тест/with "quotes"\\dir/inir'
        startup.sync(self.config, launcher, "enable")
        file = self.config / "niri/config.kdl"
        first = file.read_bytes()
        startup.sync(self.config, launcher, "enable")
        self.assertEqual(file.read_bytes(), first)
        self.assertTrue(startup.sync(self.config, launcher, "status"))
        if (Path('/usr/bin/niri')).exists():
            self.run_cmd(['niri', 'validate', '--config', str(file)])

    def test_startup_missing_config_does_not_create_unloaded_file(self):
        (self.config / "niri/config.kdl").unlink()
        with self.assertRaises(FileNotFoundError):
            startup.sync(self.config, "inir", "enable")

    def test_supervisor_single_instance_and_signal_cleanup(self):
        first = self.start_supervisor()
        self.wait_until(lambda: (self.base / "child.pid").exists())
        second = self.start_supervisor()
        self.assertEqual(second.wait(timeout=4), 0)
        self.assertEqual(len((self.base / "starts").read_text().splitlines()), 1)
        first.terminate()
        self.assertEqual(first.wait(timeout=4), 0)
        self.assertFalse((self.base / "child.pid").exists())
        self.assertEqual(list((self.runtime / "inir").glob("*.pid")), [])

    def test_supervisor_stops_at_session_end(self):
        process = self.start_supervisor()
        self.wait_until(lambda: (self.base / "child.pid").exists())
        (self.runtime / "niri-test.sock").unlink()
        self.assertEqual(process.wait(timeout=4), 0)
        self.assertFalse((self.base / "child.pid").exists())

    def test_supervisor_limits_crash_restarts(self):
        process = self.start_supervisor(fail=True)
        self.assertEqual(process.wait(timeout=6), 1)
        self.assertEqual(len((self.base / "starts").read_text().splitlines()), 2)
        self.assertEqual(list((self.runtime / "inir").glob("*.pid")), [])

    def test_supervisor_recovers_one_crash(self):
        process = self.start_supervisor()
        self.wait_until(lambda: (self.base / "child.pid").exists())
        pid = int((self.base / "child.pid").read_text())
        os.kill(pid, 15)
        self.wait_until(lambda: (self.base / "child.pid").exists() and int((self.base / "child.pid").read_text()) != pid)
        self.assertIsNone(process.poll())

    def test_supervisor_cleans_helpers_but_preserves_detached_apps(self):
        qs = self.stub('qs', '''#!/usr/bin/env python3
import os, subprocess, time
from pathlib import Path
base = Path(os.environ['INIR_TEST_BASE'])
helper = subprocess.Popen(['sleep', '60'])
app = subprocess.Popen(['sleep', '60'], start_new_session=True)
(base / 'helper.pid').write_text(str(helper.pid))
(base / 'app.pid').write_text(str(app.pid))
while True: time.sleep(0.1)
''')
        process = subprocess.Popen(['bash', str(SUPERVISOR), str(self.payload), str(qs)], env=self.env,
                                   stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
        self.processes.append(process)
        self.wait_until(lambda: (self.base / 'app.pid').exists())
        helper = int((self.base / 'helper.pid').read_text())
        app = int((self.base / 'app.pid').read_text())
        self.addCleanup(lambda: os.kill(app, signal.SIGTERM) if Path(f'/proc/{app}').exists() else None)
        process.terminate()
        self.assertEqual(process.wait(timeout=8), 0)
        state = Path(f'/proc/{helper}/stat')
        self.assertTrue(not state.exists() or state.read_text().split()[2] == 'Z')
        os.kill(app, 0)

    def test_cli_restart_replaces_supervisor_and_preserves_caller(self):
        self.fake_qs()
        process = subprocess.Popen(['bash', str(ROOT / 'scripts/inir'), 'run', '--session'], env=self.env,
                                   stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
        self.processes.append(process)
        self.wait_until(lambda: (self.base / 'child.pid').exists())
        old_pid = int((self.base / 'child.pid').read_text())
        self.run_cmd(['bash', str(ROOT / 'scripts/inir'), 'restart', '--quiet'])
        self.wait_until(lambda: (self.base / 'child.pid').exists() and int((self.base / 'child.pid').read_text()) != old_pid)
        self.run_cmd(['bash', str(ROOT / 'scripts/inir'), 'stop'])
        self.assertFalse((self.base / 'child.pid').exists())

    def test_lifecycle_caller_escapes_shell_process_group(self):
        # Re-execution must retain arguments and use a different process group.
        target = self.base / 'escaped.sh'
        target.write_text('''#!/bin/bash
source "''' + str(BACKEND) + '''"
inir_escape_session_group "$0" "$@"
[[ "$(ps -o pgid= -p $$ | tr -d ' ')" != "$INIR_SESSION_SHELL_PGID" ]]
[[ "$1" == 'argument with spaces' ]]
''')
        self.run_cmd(['setsid', 'bash', '-c', 'export INIR_SESSION_SHELL_PGID=$$; exec bash "$@"',
                      'fixture', str(target), 'argument with spaces'])

    def test_cli_daemon_run_is_supervised(self):
        self.fake_qs()
        try:
            self.run_cmd(['bash', str(ROOT / 'scripts/inir'), 'run', '--daemon'])
            self.assertTrue((self.base / 'child.pid').exists())
            self.assertEqual(len(list((self.runtime / 'inir').glob('*.pid'))), 1)
        finally:
            self.run_cmd(['bash', str(ROOT / 'scripts/inir'), 'stop'])
        self.assertFalse((self.base / 'child.pid').exists())

    def test_openrc_update_verifies_loaded_configuration(self):
        self.fake_qs()
        code = f'''source "{ROOT}/sdata/lib/robust-update.sh"
REPO_ROOT="{ROOT}"
restart_shell_and_verify 3
'''
        try:
            self.bash(code)
            self.env['INIR_TEST_QS_LOAD_FAIL'] = '1'
            self.bash(code, expected=1)
        finally:
            self.run_cmd(['bash', str(ROOT / 'scripts/inir'), 'stop'])

    def test_native_quickshell_session_without_display_hardware(self):
        qs = Path('/usr/bin/qs')
        if not qs.is_file() or not shutil.which('dbus-run-session'):
            self.skipTest('Native runtime smoke test needs Quickshell and dbus-run-session')
        (self.payload / 'shell.qml').write_text(r'''import QtQuick
import Quickshell
import Quickshell.Io
ShellRoot {
    Process {
        running: true
        command: ["bash", "-c", "echo $$ > \"$INIR_TEST_BASE/native-helper.pid\"; exec sleep 30"]
    }
    Timer {
        interval: 100
        running: true
        onTriggered: {
            Quickshell.execDetached(["bash", "-c", "echo $$ > \"$INIR_TEST_BASE/native-app.pid\"; exec sleep 30"])
            console.log("inir native runtime ready")
        }
    }
}
''')
        env = dict(self.env, INIR_QS_BIN=str(qs), QT_QPA_PLATFORM='offscreen')
        env.pop('DBUS_SESSION_BUS_ADDRESS', None)
        script = '''launcher="$1"
app=""
cleanup() {
    bash "$launcher" stop >/dev/null 2>&1 || true
    [[ -z "$app" ]] || kill "$app" 2>/dev/null || true
    [[ ! -f "$INIR_TEST_BASE/native-app.pid" ]] || kill "$(cat "$INIR_TEST_BASE/native-app.pid")" 2>/dev/null || true
}
trap cleanup EXIT
bash "$launcher" run --daemon || exit 1
for ((poll = 0; poll < 20; poll++)); do
    log="$(bash "$launcher" logs --full 2>&1)"
    if [[ "$log" == *"Configuration Loaded"* && "$log" == *"inir native runtime ready"* && -s "$INIR_TEST_BASE/native-app.pid" ]]; then
        app="$(cat "$INIR_TEST_BASE/native-app.pid")"
        helper="$(cat "$INIR_TEST_BASE/native-helper.pid")"
        bash "$launcher" restart --quiet || exit 1
        kill -0 "$app" || exit 1
        state="$(ps -o stat= -p "$helper")"
        [[ -z "$state" || "$state" == Z* ]] || exit 1
        exit 0
    fi
    sleep 0.05
done
printf '%s\\n' "$log" >&2
exit 1
'''
        self.run_cmd(['dbus-run-session', '--', 'bash', '-c', script, 'fixture', str(ROOT / 'scripts/inir')], env=env)

    def test_stale_pid_is_rejected(self):
        state = self.bash(f'inir_session_state_path "{self.payload}"').strip()
        Path(state).parent.mkdir()
        Path(state + ".pid").write_text(f"{os.getpid()} 0\n")
        self.bash(f'inir_session_supervisor_pid "{self.payload}"', expected=1)

    def test_service_status_sees_supervisor_without_a_live_shell_child(self):
        process = self.start_supervisor()
        state = self.bash(f'inir_session_state_path "{self.payload}"').strip()
        self.wait_until(lambda: Path(state + ".pid").exists())
        env = dict(self.env, INIR_TEST_QS_NO_LIST="1")
        result = subprocess.run([str(ROOT / "scripts/inir"), "service", "status"],
                                env=env, text=True, capture_output=True, timeout=10)
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
        self.assertIn("iNtoo is running", result.stdout)
        self.assertIsNone(process.poll(), "the fixture supervisor should still own restart intent")

    def test_supervisor_can_be_stopped_from_another_checkout(self):
        process = self.start_supervisor()
        self.wait_until(lambda: (self.base / 'child.pid').exists())
        pid = self.bash(f'_INIR_SESSION_LIB_DIR="/different/checkout/scripts/lib"\ninir_session_supervisor_pid "{self.payload}"').strip()
        self.assertEqual(pid, str(process.pid))

    def test_cli_session_applies_environment_and_stops(self):
        self.fake_qs()
        process = subprocess.Popen(["bash", str(ROOT / "scripts/inir"), "run", "--session"],
                                   env=self.env, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
        self.processes.append(process)
        self.wait_until(lambda: (self.base / "child.pid").exists())
        start = json.loads((self.base / "starts").read_text().splitlines()[0])
        self.assertEqual(start["scale"], "1")
        self.assertIn("quickshell.dbus.properties=false", start["rules"])
        self.assertNotIn("-d", start["args"])
        self.run_cmd(["bash", str(ROOT / "scripts/inir"), "stop"])
        self.assertEqual(process.wait(timeout=5), 0)
        self.assertFalse((self.base / "child.pid").exists())
        self.assertFalse(any(call[0] == "systemctl" for call in self.recorded()))

    def test_cli_requires_dbus_session(self):
        self.fake_qs()
        env = dict(self.env, DBUS_SESSION_BUS_ADDRESS="")
        result = subprocess.run(["bash", str(ROOT / "scripts/inir"), "run", "--session"],
                                env=env, text=True, capture_output=True, timeout=10)
        self.assertEqual(result.returncode, 1)
        self.assertIn("dbus-run-session", result.stderr)

    def test_cli_service_startup_management(self):
        self.run_cmd(["bash", str(ROOT / "scripts/inir"), "service", "enable"])
        self.assertTrue(startup.sync(self.config, "inir", "status"))
        self.fake_qs()
        self.run_cmd(["bash", str(ROOT / "scripts/inir"), "service", "disable"])
        self.assertFalse(startup.sync(self.config, "inir", "status"))

    def test_openrc_system_services_preserve_other_display_manager(self):
        root = self.base / "system"
        for service in ("dbus", "elogind", "display-manager", "ydotool"):
            script = root / "etc/init.d" / service
            script.parent.mkdir(parents=True, exist_ok=True)
            script.write_text("#!/bin/sh\n")
            script.chmod(0o755)
        conf = root / "etc/conf.d/display-manager"
        conf.parent.mkdir(parents=True)
        conf.write_text('DISPLAYMANAGER="lightdm"\n')
        self.stub("sddm", "#!/bin/sh\nexit 0\n")
        self.stub("getent", "#!/bin/sh\nprintf 'input:x:123:\\n'\n")
        code = f'''source "{ROOT}/sdata/lib/openrc.sh"
v() {{ "$@"; }}
pkg_sudo() {{ case "$1" in install) "$@" ;; *) printf '%s\\n' "$*" >> "$INIR_TEST_CALLS" ;; esac; }}
log_warning() {{ :; }}; log_error() {{ echo "$*" >&2; }}
log_info() {{ :; }}; log_success() {{ :; }}; tui_info() {{ :; }}
setup_openrc_services "{root}"
setup_openrc_services "{root}"
'''
        self.bash(code)
        self.assertEqual(conf.read_text(), 'DISPLAYMANAGER="lightdm"\n')
        calls = self.calls.read_text()
        self.assertIn("rc-update add dbus default", calls)
        self.assertIn("rc-service elogind start", calls)
        self.assertNotIn("display-manager default", calls)
        ydotool = (root / "etc/conf.d/ydotool").read_text()
        self.assertEqual(ydotool.count("command_args="), 1)
        self.assertIn("--socket-own=0:123 --socket-perm=0660", ydotool)

    def test_gentoo_session_dependencies_and_use_flags(self):
        system = self.base / 'portage-root'
        (system / 'etc/portage').mkdir(parents=True)
        self.env['INIR_TEST_PORTAGE_ROOT'] = str(system)
        self.stub('portageq', '''#!/bin/sh
case "$*" in
 'envvar ARCH') echo amd64 ;;
 'envvar PORTAGE_CONFIGROOT') echo "$INIR_TEST_PORTAGE_ROOT" ;;
 *) exit 1 ;;
esac
''')
        self.stub('eselect', '#!/bin/sh\nprintf "[1] guru\\n"\n')
        self.stub('emerge', '#!/usr/bin/env python3\nimport os, json, sys\n'
                  'with open(os.environ["INIR_TEST_CALLS"], "a") as f: f.write(json.dumps(sys.argv[1:]) + "\\n")\n')
        source = (ROOT / 'sdata/dist-gentoo/install-deps.sh').read_text().rsplit('\ngentoo_install_selected', 1)[0]
        helper = self.base / 'gentoo-functions.sh'
        helper.write_text(source)
        code = f'''ask=false; SKIP_SYSUPDATE=true
INSTALL_AUDIO=false; INSTALL_TOOLKIT=false; INSTALL_SCREENCAPTURE=false; INSTALL_FONTS=false
v() {{ "$@"; }}; pkg_sudo() {{ "$@"; }}
log_warning() {{ :; }}; log_error() {{ echo "$*" >&2; }}; log_success() {{ :; }}
source "{helper}"
gentoo_install_selected
'''
        self.bash(code)
        calls = self.recorded()
        self.assertIn('--changed-use', calls[0])
        self.assertNotIn('--noreplace', calls[0])
        self.assertIn('sys-auth/elogind', calls[0])
        self.assertIn('sys-auth/pambase', calls[0])
        use = system / 'etc/portage/package.use/intoo-session'
        self.assertIn('sys-auth/pambase elogind -systemd', use.read_text())
        self.assertIn('media-video/pipewire sound-server elogind -systemd', use.read_text())
        first = use.read_bytes()
        self.bash(code)
        self.assertEqual(first, use.read_bytes())
        # Single-file Portage configuration preserves unrelated user entries.
        for file in use.parent.iterdir():
            file.unlink()
        use.parent.rmdir()
        single = system / 'etc/portage/package.use'
        single.write_text('app-misc/example custom-flag\n')
        self.bash(code)
        first = single.read_bytes()
        self.bash(code)
        self.assertEqual(first, single.read_bytes())
        self.assertIn('app-misc/example custom-flag', single.read_text())
        self.env['INIR_INIT_SYSTEM'] = 'systemd'
        self.bash(code)
        text = single.read_text()
        self.assertIn('gui-wm/niri dbus screencast systemd', text)
        self.assertNotIn('sys-auth/pambase elogind -systemd', text)

    def test_gentoo_browser_migration_uses_portage_and_can_be_skipped(self):
        installed = self.base / 'browser-integration-installed'
        self.env['INIR_TEST_BROWSER_PACKAGE'] = str(installed)
        self.env['REPO_ROOT'] = str(ROOT)
        self.env['OS_GROUP_ID'] = 'gentoo'
        self.stub('portageq', '''#!/usr/bin/env python3
import os, sys
if sys.argv[1:] != ["has_version", "/", "kde-plasma/plasma-browser-integration"]:
    raise SystemExit(2)
raise SystemExit(0 if os.path.exists(os.environ["INIR_TEST_BROWSER_PACKAGE"]) else 1)
''')
        self.stub('emerge', '''#!/usr/bin/env python3
import json, os, sys
with open(os.environ["INIR_TEST_CALLS"], "a") as calls:
    calls.write(json.dumps(["emerge", *sys.argv[1:]]) + "\\n")
open(os.environ["INIR_TEST_BROWSER_PACKAGE"], "w").close()
''')
        migration = ROOT / 'sdata/migrations/029-plasma-browser-integration.sh'
        migration_id = '029-plasma-browser-integration'
        status_code = f'''source "{migration}"
source "{ROOT / 'sdata/lib/migrations.sh'}"
[[ "$MIGRATION_REQUIRED" == false ]]
[[ "$(get_migration_real_status {migration_id})" == pending ]]
mark_migration_skipped {migration_id}
[[ "$(get_migration_real_status {migration_id})" == skipped ]]
'''
        self.run_cmd(['bash', '-c', status_code])

        apply_code = f'''source "{migration}"
pkg_sudo() {{ "$@"; }}
[[ "$MIGRATION_REQUIRED" == false ]]
migration_check
migration_apply
! migration_check
'''
        self.run_cmd(['bash', '-c', apply_code])
        self.assertEqual(self.recorded(), [[
            'emerge', '--verbose', '--noreplace', '--ask=n',
            'kde-plasma/plasma-browser-integration',
        ]])

    def test_migration_yesforall_alias_applies_remaining_once(self):
        applied = self.base / 'migration-attempts'
        self.env['INIR_TEST_MIGRATION_ATTEMPTS'] = str(applied)
        code = f'''source "{ROOT / 'sdata/lib/migrations.sh'}"
HAS_GUM=false
get_pending_migrations() {{ printf 'test-one\\ntest-two\\n'; }}
show_migration_card() {{ :; }}
tui_title() {{ :; }}; tui_info() {{ :; }}; tui_subtitle() {{ :; }}; tui_success() {{ :; }}
load_migration() {{ MIGRATION_ID="$1"; MIGRATION_TITLE="$1"; }}
apply_migration() {{ printf '%s\\n' "$1" >> "$INIR_TEST_MIGRATION_ATTEMPTS"; return 1; }}
is_migration_applied() {{ return 1; }}
is_migration_skipped() {{ return 1; }}
run_migrations_interactive
'''
        result = subprocess.run(['bash', '-c', code], input='yesforall\n', env=self.env,
                                text=True, capture_output=True, timeout=10)
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
        self.assertEqual(applied.read_text().splitlines(), ['test-one', 'test-two'])


if __name__ == "__main__":
    unittest.main()
