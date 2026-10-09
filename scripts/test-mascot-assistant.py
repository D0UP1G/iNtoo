#!/usr/bin/env python3
"""Regression checks for companion transport, per-message tools and safe keybind migration."""

import importlib.util
import json
import os
from pathlib import Path
import signal
import subprocess
import sys
import tempfile
import time
import unittest
from unittest.mock import patch

ROOT = Path(__file__).resolve().parents[1]
BRIDGE = ROOT / "scripts/ai/mascot-assistant.py"
KEYBIND = ROOT / "scripts/ai/assistant-keybind.py"
INSTALLER = ROOT / "sdata/lib/install-opencode.sh"
spec = importlib.util.spec_from_file_location("companion", BRIDGE)
companion = importlib.util.module_from_spec(spec)
spec.loader.exec_module(companion)


class CompanionTests(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory()
        self.addCleanup(self.temporary.cleanup)
        self.home = Path(self.temporary.name)
        self.bin = self.home / "bin"
        self.bin.mkdir()
        self.env = {**os.environ, "HOME": str(self.home), "PATH": str(self.bin) + ":/usr/bin:/bin",
                    "XDG_CONFIG_HOME": str(self.home / "config"),
                    "XDG_STATE_HOME": str(self.home / "state"),
                    "XDG_DATA_HOME": str(self.home / "data"),
                    "XDG_BIN_HOME": str(self.home / "local-bin"),
                    "CAPTURE": str(self.home / "capture.json")}
        self.env.pop("HISTFILE", None)

    def stub(self, body):
        path = self.bin / "opencode"
        path.write_text("#!" + sys.executable + "\n" + '''import sys
if "--version" in sys.argv:
    print("1.18.35")
    raise SystemExit(0)
''' + body)
        path.chmod(0o755)

    def request(self, **values):
        result = subprocess.run([sys.executable, str(BRIDGE)], env=self.env,
                                input=json.dumps({"message": "Hello", **values}) + "\n",
                                text=True, capture_output=True, timeout=10)
        return result, [json.loads(line) for line in result.stdout.splitlines()]

    def test_history_is_bounded_and_filters_obvious_credentials(self):
        (self.home / ".bash_history").write_text("\n".join("echo " + str(i) for i in range(50))
                                                  + "\nexport API_KEY=secret\ncurl -H 'Authorization: Bearer xxx'\n")
        with patch.dict(os.environ, self.env, clear=True):
            history = companion.recent_commands(self.home, self.home / "config")
        self.assertEqual(history[0]["commands"], ["echo " + str(i) for i in range(30, 50)])

    def test_zsh_and_fish_histories(self):
        (self.home / ".zsh_history").write_text(": 1700000000:0;pwd\n")
        fish = self.home / "data/fish"
        fish.mkdir(parents=True)
        (fish / "fish_history").write_text("- cmd: ls -la\n  when: 1700000000\n")
        with patch.dict(os.environ, self.env, clear=True):
            history = companion.recent_commands(self.home, self.home / "config")
        self.assertEqual([item["commands"] for item in history], [["pwd"], ["ls -la"]])

    def capture_stub(self):
        self.stub('''import json, os, sys
from pathlib import Path
Path(os.environ["CAPTURE"]).write_text(json.dumps({"input": sys.stdin.read(), "argv": sys.argv,
    "config": json.loads(os.environ["OPENCODE_CONFIG_CONTENT"]), "cwd": os.getcwd(), "pwd": os.environ["PWD"]}))
print(json.dumps({"type": "text", "part": {"text": "Hello"}}), flush=True)
print(json.dumps({"type": "text", "part": {"text": " world"}}), flush=True)
''')

    def test_stream_and_no_history_without_opt_in(self):
        (self.home / ".bash_history").write_text("echo PRIVATE_HISTORY_MARKER\n")
        self.capture_stub()
        result, events = self.request(message="Literal $(touch /tmp/should-not-exist)", model="test/model")
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual([e["text"] for e in events if e["type"] == "text"], ["Hello", " world"])
        capture = json.loads((self.home / "capture.json").read_text())
        self.assertNotIn("PRIVATE_HISTORY_MARKER", capture["input"])
        self.assertIn("$(touch /tmp/should-not-exist)", capture["input"])
        self.assertNotIn("Literal", " ".join(capture["argv"]))
        self.assertEqual(capture["cwd"], capture["pwd"])
        self.assertEqual(capture["config"]["permission"], {"*": "deny"})
        agent = capture["argv"][capture["argv"].index("--agent") + 1]
        self.assertEqual(capture["config"]["agent"][agent]["permission"], {"*": "deny"})

    def test_actions_do_not_carry_into_next_message(self):
        self.capture_stub()
        self.request(allowActions=True)
        capture = json.loads((self.home / "capture.json").read_text())
        self.assertEqual(capture["config"]["permission"]["bash"], "allow")
        self.request()
        capture = json.loads((self.home / "capture.json").read_text())
        self.assertEqual(capture["config"]["permission"], {"*": "deny"})

    def test_history_is_attached_when_requested(self):
        (self.home / ".bash_history").write_text("echo HISTORY_MARKER\n")
        self.capture_stub()
        self.request(includeHistory=True)
        capture = json.loads((self.home / "capture.json").read_text())
        self.assertIn("HISTORY_MARKER", capture["input"])

    def test_model_and_empty_message_errors(self):
        self.capture_stub()
        for values in ({"model": "invalid model"}, {"message": " "}):
            result, events = self.request(**values)
            self.assertNotEqual(result.returncode, 0)
            self.assertEqual(events[0]["type"], "error")
        self.assertFalse((self.home / "capture.json").exists())

    def test_unsupported_opencode_major_version_is_rejected(self):
        self.stub('import sys; print("2.0.0"); raise SystemExit(0)')
        executable = self.bin / "opencode"
        source = executable.read_text().replace('print("1.18.35")', 'print("2.0.0")')
        executable.write_text(source)
        result, events = self.request()
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("needs OpenCode 1.x", events[0]["text"])

    def test_nonzero_and_empty_output_report_errors(self):
        for body in ("import sys; print('provider unavailable', file=sys.stderr); sys.exit(1)", "pass"):
            self.stub(body)
            result, events = self.request()
            self.assertNotEqual(result.returncode, 0)
            self.assertEqual(events[-1]["type"], "error")

    def test_cancellation_terminates_opencode(self):
        self.stub('''import os, time
from pathlib import Path
Path(os.environ["CAPTURE"]).write_text(str(os.getpid()))
time.sleep(30)
''')
        child = subprocess.Popen([sys.executable, str(BRIDGE)], env=self.env, text=True,
                                 stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=subprocess.PIPE)
        try:
            child.stdin.write('{"message":"Hello"}\n')
            child.stdin.flush()
            capture = self.home / "capture.json"
            deadline = time.monotonic() + 5
            while not capture.exists() and time.monotonic() < deadline:
                time.sleep(0.05)
            self.assertTrue(capture.exists())
            pid = int(capture.read_text())
            child.send_signal(signal.SIGTERM)
            stdout, stderr = child.communicate(timeout=5)
            self.assertEqual(child.returncode, 0, stderr)
            self.assertIn('"done"', stdout)
            with self.assertRaises(ProcessLookupError):
                os.kill(pid, 0)
        finally:
            if child.poll() is None:
                child.kill()
                child.communicate()

    def keybind(self, check=False):
        return subprocess.run([sys.executable, str(KEYBIND), *( ["--check"] if check else []),
                               str(self.home / "config.kdl")], capture_output=True, text=True)

    def test_keybind_migration_follows_includes_and_is_idempotent(self):
        config = self.home / "config.kdl"
        (self.home / "config.d").mkdir()
        config.write_text('include "config.d/*.kdl"\n')
        binds = self.home / "config.d" / "binds.kdl"
        binds.write_text('binds {\n // Mod+S is free\n /- Mod+S {spawn "old";}\n Mod+A {spawn "test";}\n}\n')
        original = binds.read_text()
        self.assertEqual(self.keybind(check=True).returncode, 0)
        self.assertEqual(binds.read_text(), original)
        self.assertEqual(self.keybind().returncode, 0)
        installed = binds.read_text()
        self.assertIn('"assistant" "toggle"', installed)
        self.assertEqual(self.keybind(check=True).returncode, 1)
        self.assertEqual(self.keybind().returncode, 1)
        self.assertEqual(binds.read_text(), installed)

    def test_keybind_migration_preserves_conflicts_in_other_includes(self):
        (self.home / "config.kdl").write_text('include "binds.kdl"\ninclude "custom.kdl"\n')
        binds = self.home / "binds.kdl"
        original = 'binds {Mod+A {spawn "test";}}\n'
        binds.write_text(original)
        (self.home / "custom.kdl").write_text('binds {"Super+S" {spawn "custom";}}\n')
        self.assertEqual(self.keybind().returncode, 1)
        self.assertEqual(binds.read_text(), original)

    def test_installer_uses_user_prefix_and_keeps_existing_binary(self):
        npm = self.bin / "npm"
        npm.write_text("#!" + sys.executable + "\n" + '''import os, sys
from pathlib import Path
args = sys.argv[1:]
prefix = Path(args[args.index("--prefix") + 1])
marker = Path(os.environ["CAPTURE"])
marker.write_text(marker.read_text() + "install\\n" if marker.exists() else "install\\n")
binary = prefix / "bin/opencode"
binary.parent.mkdir(parents=True, exist_ok=True)
binary.write_text("#!/bin/sh\\nprintf 'opencode\\\\n'\\n")
binary.chmod(0o755)
''')
        npm.chmod(0o755)
        install = f'''source {str(INSTALLER)!r}
ask=false
OS_GROUP_ID=generic
v() {{ "$@"; }}
pkg_sudo() {{ "$@"; }}
log_warning() {{ printf '%s\\n' "$*" >&2; }}
log_error() {{ printf '%s\\n' "$*" >&2; }}
log_success() {{ :; }}
install_opencode'''
        result = subprocess.run(["bash", "-c", install], env=self.env, text=True, capture_output=True)
        self.assertEqual(result.returncode, 0, result.stderr)
        target = Path(self.env["XDG_BIN_HOME"]) / "opencode"
        installed = Path(self.env["XDG_DATA_HOME"]) / "inir/opencode/bin/opencode"
        self.assertTrue(target.is_symlink())
        self.assertEqual(target.resolve(), installed)
        self.assertTrue(os.access(installed, os.X_OK))
        result = subprocess.run(["bash", "-c", install], env=self.env, text=True, capture_output=True)
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(Path(self.env["CAPTURE"]).read_text(), "install\n")


if __name__ == "__main__":
    unittest.main()
