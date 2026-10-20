#!/usr/bin/env python3
"""Regression checks for companion transport, per-action approval and safe keybind migration."""

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

SERVE_STUB = r'''import json, os, sys, time
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer

if "--version" in sys.argv:
    print("1.18.35")
    raise SystemExit(0)

if "serve" not in sys.argv:
    raise SystemExit(1)

capture = os.environ.get("CAPTURE")

def read_capture():
    with open(capture) as f:
        return json.load(f)

def write_capture(data):
    with open(capture + ".tmp", "w") as f:
        json.dump(data, f)
    os.replace(capture + ".tmp", capture)

write_capture({
    "argv": sys.argv,
    "config": json.loads(os.environ.get("OPENCODE_CONFIG_CONTENT", "{}")),
    "cwd": os.getcwd(),
    "pwd": os.environ.get("PWD", ""),
    "database": os.environ.get("OPENCODE_DB", ""),
    "pid": os.getpid(),
    "messages": [],
    "permissions": [],
})

scenario = json.loads(os.environ.get("SCENARIO", "[]"))


class Handler(BaseHTTPRequestHandler):
    def log_message(self, *args):
        pass

    def _json(self, code, obj):
        body = json.dumps(obj).encode()
        self.send_response(code)
        self.send_header("Content-Type", "application/json")
        self.send_header("Content-Length", str(len(body)))
        self.end_headers()
        self.wfile.write(body)

    def do_POST(self):
        length = int(self.headers.get("Content-Length", 0))
        body = json.loads(self.rfile.read(length)) if length else {}
        if self.path == "/session":
            self._json(200, {"id": "ses_test"})
        elif self.path.startswith("/session/") and (self.path.endswith("/message")
                                                    or self.path.endswith("/prompt_async")):
            data = read_capture()
            data["messages"].append(body)
            write_capture(data)
            self._json(200, {"info": {"id": "msg_test"}, "parts": []})
        elif "/permissions/" in self.path:
            data = read_capture()
            data["permissions"].append(body)
            write_capture(data)
            self._json(200, True)
        else:
            self._json(404, {})

    def do_GET(self):
        if self.path == "/event":
            self.send_response(200)
            self.send_header("Content-Type", "text/event-stream")
            self.end_headers()
            for event in scenario:
                payload = json.dumps(event)
                self.wfile.write(("event: " + event["type"] + "\n").encode())
                self.wfile.write(("data: " + payload + "\n\n").encode())
                self.wfile.flush()
            time.sleep(3600)
        else:
            self._json(404, {})


server = ThreadingHTTPServer(("127.0.0.1", 0), Handler)
print("opencode server listening on http://127.0.0.1:%d" % server.server_address[1], flush=True)
server.serve_forever()
'''


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

    def serve_stub(self, scenario):
        self.env["SCENARIO"] = json.dumps(scenario)
        path = self.bin / "opencode"
        path.write_text("#!" + sys.executable + "\n" + SERVE_STUB)
        path.chmod(0o755)

    def request(self, **values):
        result = subprocess.run([sys.executable, str(BRIDGE)], env=self.env,
                                input=json.dumps({"message": "Hello", **values}) + "\n",
                                text=True, capture_output=True, timeout=15)
        return result, [json.loads(line) for line in result.stdout.splitlines()]

    def capture(self):
        return json.loads((self.home / "capture.json").read_text())

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

    def test_stream_and_no_history_without_opt_in(self):
        (self.home / ".bash_history").write_text("echo PRIVATE_HISTORY_MARKER\n")
        self.serve_stub([
            {"type": "message.updated", "properties": {"info": {"id": "m1", "role": "assistant"}}},
            {"type": "message.part.updated",
             "properties": {"part": {"id": "p1", "messageID": "m1", "type": "text", "text": "Hello"},
                            "delta": "Hello"}},
            {"type": "message.part.updated",
             "properties": {"part": {"id": "p1", "messageID": "m1", "type": "text", "text": "Hello world"},
                            "delta": " world"}},
            {"type": "session.idle", "properties": {"sessionID": "ses_test"}},
        ])
        result, events = self.request(message="Literal $(touch /tmp/should-not-exist)", model="test/model")
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual([e["text"] for e in events if e["type"] == "text"], ["Hello", " world"])
        capture = self.capture()
        prompt = capture["messages"][0]["parts"][0]["text"]
        self.assertNotIn("PRIVATE_HISTORY_MARKER", prompt)
        self.assertIn("$(touch /tmp/should-not-exist)", prompt)
        self.assertNotIn("Literal", " ".join(capture["argv"]))
        self.assertEqual(capture["cwd"], capture["pwd"])
        self.assertEqual(capture["database"], str(self.home / "state/inir/assistant/opencode.db"))
        self.assertEqual(capture["config"]["permission"], {"*": "deny"})
        agent = capture["messages"][0]["agent"]
        self.assertEqual(capture["config"]["agent"][agent]["permission"], {"*": "deny"})

    def test_ask_mode_sets_per_action_permissions(self):
        self.serve_stub([{"type": "session.idle", "properties": {"sessionID": "ses_test"}}])
        result, _events = self.request(model="test/model", actions="ask")
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(self.capture()["config"]["permission"], {"*": "ask"})

    def test_default_mode_stays_denied(self):
        self.serve_stub([{"type": "session.idle", "properties": {"sessionID": "ses_test"}}])
        result, _events = self.request(model="test/model")
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(self.capture()["config"]["permission"], {"*": "deny"})

    def test_history_is_attached_when_requested(self):
        (self.home / ".bash_history").write_text("echo HISTORY_MARKER\n")
        self.serve_stub([{"type": "session.idle", "properties": {"sessionID": "ses_test"}}])
        self.request(includeHistory=True, model="test/model")
        self.assertIn("HISTORY_MARKER", self.capture()["messages"][0]["parts"][0]["text"])

    def test_ollama_default_requires_the_runtime(self):
        with patch.dict(os.environ, self.env, clear=True):
            with self.assertRaises(RuntimeError) as raised:
                companion.ensure_ollama_model("qwen3.5:0.8b")
        self.assertIn("Ollama is not installed", str(raised.exception))

    def test_explicit_provider_configures_endpoint_and_model(self):
        self.serve_stub([{"type": "session.idle", "properties": {"sessionID": "ses_test"}}])
        result, _events = self.request(provider="deepseek", apiBase="https://api.deepseek.com/v1",
                                       apiFormat="openai", model="deepseek-chat")
        self.assertEqual(result.returncode, 0, result.stderr)
        capture = self.capture()
        self.assertEqual(capture["messages"][0]["model"],
                         {"providerID": "deepseek", "modelID": "deepseek-chat"})
        provider = capture["config"]["provider"]["deepseek"]
        self.assertEqual(provider["options"]["baseURL"], "https://api.deepseek.com/v1")
        self.assertEqual(provider["npm"], "@ai-sdk/openai-compatible")
        self.assertIn("deepseek-chat", provider["models"])

    def test_provider_without_base_keeps_opencode_endpoint(self):
        self.serve_stub([{"type": "session.idle", "properties": {"sessionID": "ses_test"}}])
        result, _events = self.request(provider="openai", model="gpt-4.1-mini")
        self.assertEqual(result.returncode, 0, result.stderr)
        capture = self.capture()
        self.assertNotIn("provider", capture["config"])
        self.assertEqual(capture["messages"][0]["model"],
                         {"providerID": "openai", "modelID": "gpt-4.1-mini"})

    def test_provider_mismatch_does_not_fall_back_to_ollama(self):
        result, events = self.request(provider="openai", model="ollama/qwen3.5:0.8b")
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("selected provider openai", events[-1]["text"])
        self.assertFalse((self.home / "capture.json").exists())

    def test_cloud_provider_requires_a_model(self):
        result, events = self.request(provider="openai", model="")
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("Choose a model for openai", events[-1]["text"])
        self.assertFalse((self.home / "capture.json").exists())

    def test_nested_model_id_is_preserved(self):
        self.serve_stub([{"type": "session.idle", "properties": {"sessionID": "ses_test"}}])
        result, _events = self.request(provider="openrouter", model="openrouter/vendor/model:free")
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(self.capture()["messages"][0]["model"],
                         {"providerID": "openrouter", "modelID": "vendor/model:free"})

    def test_catalog_lists_opencode_and_installed_ollama_models(self):
        self.stub('''import json, os, sys
from pathlib import Path
Path(os.environ["CAPTURE"]).write_text(json.dumps({"argv": sys.argv,
    "config": json.loads(os.environ["OPENCODE_CONFIG_CONTENT"]), "cwd": os.getcwd(),
    "database": os.environ["OPENCODE_DB"]}))
print("openai/gpt-4.1-mini\\nopenrouter/vendor/model:free\\nopenai/gpt-4.1-mini\\ninvalid output")
''')
        events = []
        with patch.dict(os.environ, self.env, clear=True), \
                patch.object(companion, "ollama_json", return_value={"models": [{"name": "local-model:latest"}]}), \
                patch.object(companion, "ensure_ollama_model", side_effect=AssertionError("Discovery must not pull models")), \
                patch.object(companion, "emit", side_effect=lambda kind, **values: events.append({"type": kind, **values})):
            companion.discover_models({})
        self.assertEqual(events, [{"type": "catalog", "error": "", "models": [
            "ollama/local-model:latest", "ollama/qwen3.5:0.8b",
            "openai/gpt-4.1-mini", "openrouter/vendor/model:free"]}])
        capture = json.loads((self.home / "capture.json").read_text())
        self.assertEqual(capture["argv"][1:], ["models", "--pure"])
        self.assertEqual(capture["database"], str(self.home / "state/inir/assistant/opencode.db"))
        self.assertEqual(capture["config"]["permission"], {"*": "deny"})

    def test_catalog_registers_custom_endpoint(self):
        self.stub('''import json, os, sys
from pathlib import Path
Path(os.environ["CAPTURE"]).write_text(json.dumps({"argv": sys.argv,
    "config": json.loads(os.environ["OPENCODE_CONFIG_CONTENT"]), "cwd": os.getcwd()}))
''')
        with patch.dict(os.environ, self.env, clear=True), \
                patch.object(companion, "ollama_json", side_effect=OSError("Stopped")), \
                patch.object(companion, "emit"):
            companion.discover_models({"provider": "custom", "model": "custom/my-model",
                                       "apiBase": "http://localhost:8000/v1", "apiFormat": "openai"})
        capture = json.loads((self.home / "capture.json").read_text())
        self.assertEqual(capture["config"]["provider"]["custom"]["options"]["baseURL"],
                         "http://localhost:8000/v1")

    def test_catalog_keeps_ollama_models_when_opencode_fails(self):
        events = []
        with patch.dict(os.environ, self.env, clear=True), \
                patch.object(companion, "opencode_binary", side_effect=RuntimeError("OpenCode unavailable")), \
                patch.object(companion, "ollama_json", return_value={"models": [{"name": "local-model:latest"}]}), \
                patch.object(companion, "emit", side_effect=lambda kind, **values: events.append({"type": kind, **values})):
            companion.discover_models({})
        self.assertEqual(events[0]["models"], ["ollama/local-model:latest", "ollama/qwen3.5:0.8b"])
        self.assertIn("OpenCode unavailable", events[0]["error"])

    def test_model_and_empty_message_errors(self):
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

    def test_assistant_auth_uses_the_private_database(self):
        executable = self.bin / "opencode"
        executable.write_text("#!" + sys.executable + "\n" + '''import json, os, sys
from pathlib import Path
Path(os.environ["CAPTURE"]).write_text(json.dumps({"database": os.environ.get("OPENCODE_DB"), "args": sys.argv[1:]}))
''')
        executable.chmod(0o755)
        result = subprocess.run([str(ROOT / "scripts/intoo"), "assistant", "auth"],
                                env=self.env, text=True, capture_output=True, timeout=10)
        self.assertEqual(result.returncode, 0, result.stderr)
        actual = json.loads((self.home / "capture.json").read_text())
        self.assertEqual(actual["database"], str(self.home / "state/inir/assistant/opencode.db"))
        self.assertEqual(actual["args"], ["auth", "login"])

    def test_server_startup_failure_reports_error(self):
        self.stub("import sys; sys.exit(1)\n")
        result, events = self.request()
        self.assertNotEqual(result.returncode, 0)
        self.assertEqual(events[-1]["type"], "error")
        self.assertNotIn("\x1b", events[-1]["text"])

    def test_permission_flow_approves_once(self):
        self.serve_stub([
            {"type": "permission.asked", "properties": {"id": "perm1", "permission": "bash",
             "patterns": ["touch /tmp/x"], "metadata": {"command": "touch /tmp/x"}}},
            {"type": "message.updated", "properties": {"info": {"id": "m1", "role": "assistant"}}},
            {"type": "message.part.updated",
             "properties": {"part": {"id": "p1", "messageID": "m1", "type": "text", "text": "Done"}}},
            {"type": "session.idle", "properties": {"sessionID": "ses_test"}},
        ])
        child = subprocess.Popen([sys.executable, str(BRIDGE)], env=self.env, text=True,
                                 stdin=subprocess.PIPE, stdout=subprocess.PIPE,
                                 stderr=subprocess.PIPE)
        events = []
        try:
            child.stdin.write('{"message":"Do it","model":"test/model","actions":"ask"}\n')
            child.stdin.flush()
            responded = False
            deadline = time.monotonic() + 15
            while time.monotonic() < deadline:
                line = child.stdout.readline()
                if not line:
                    break
                events.append(json.loads(line))
                if events[-1].get("type") == "permission" and not responded:
                    responded = True
                    child.stdin.write('{"response":"once"}\n')
                    child.stdin.flush()
            child.stdin.close()
            child.wait(timeout=15)
        finally:
            if child.poll() is None:
                child.kill()
                child.communicate()
            else:
                child.stdout.close()
                child.stderr.close()
        self.assertEqual(child.returncode, 0)
        permission = next(e for e in events if e["type"] == "permission")
        self.assertEqual(permission["tool"], "bash")
        self.assertEqual(permission["pattern"], "touch /tmp/x")
        self.assertEqual(self.capture()["permissions"], [{"response": "once"}])

    def test_permission_flow_rejects_on_stdin_eof(self):
        self.serve_stub([
            {"type": "permission.updated", "properties": {"id": "perm1", "type": "edit",
             "title": "Edit README.md", "pattern": "README.md", "metadata": {}}},
            {"type": "message.updated", "properties": {"info": {"id": "m1", "role": "assistant"}}},
            {"type": "message.part.updated",
             "properties": {"part": {"id": "p1", "messageID": "m1", "type": "text", "text": "Blocked"}}},
            {"type": "session.idle", "properties": {"sessionID": "ses_test"}},
        ])
        # stdin closes after the request, so the pending permission defaults to reject.
        result, events = self.request(model="test/model", actions="ask")
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertTrue(any(e["type"] == "permission" for e in events))
        self.assertEqual(self.capture()["permissions"], [{"response": "reject"}])

    def test_text_streams_from_part_deltas(self):
        self.serve_stub([
            {"type": "message.updated", "properties": {"info": {"id": "m1", "role": "assistant"}}},
            {"type": "message.part.updated",
             "properties": {"part": {"id": "p1", "messageID": "m1", "type": "text", "text": ""}}},
            {"type": "message.part.delta",
             "properties": {"messageID": "m1", "partID": "p1", "field": "text", "delta": "Hel"}},
            {"type": "message.part.delta",
             "properties": {"messageID": "m1", "partID": "p1", "field": "text", "delta": "lo"}},
            {"type": "session.idle", "properties": {"sessionID": "ses_test"}},
        ])
        result, events = self.request(model="test/model")
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual([e["text"] for e in events if e["type"] == "text"], ["Hel", "lo"])

    def test_reasoning_and_user_echo_are_not_shown(self):
        self.serve_stub([
            {"type": "message.updated", "properties": {"info": {"id": "u1", "role": "user"}}},
            {"type": "message.part.updated",
             "properties": {"part": {"id": "up", "messageID": "u1", "type": "text", "text": "Hello"}}},
            {"type": "message.updated", "properties": {"info": {"id": "m1", "role": "assistant"}}},
            {"type": "message.part.updated",
             "properties": {"part": {"id": "r1", "messageID": "m1", "type": "reasoning", "text": ""}}},
            {"type": "message.part.delta",
             "properties": {"messageID": "m1", "partID": "r1", "field": "text", "delta": "thinking"}},
            {"type": "message.part.updated",
             "properties": {"part": {"id": "p1", "messageID": "m1", "type": "text", "text": ""}}},
            {"type": "message.part.delta",
             "properties": {"messageID": "m1", "partID": "p1", "field": "text", "delta": "Answer"}},
            {"type": "session.idle", "properties": {"sessionID": "ses_test"}},
        ])
        result, events = self.request(model="test/model")
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual([e["text"] for e in events if e["type"] == "text"], ["Answer"])

    def test_cancellation_terminates_the_server(self):
        self.serve_stub([])
        child = subprocess.Popen([sys.executable, str(BRIDGE)], env=self.env, text=True,
                                 stdin=subprocess.PIPE, stdout=subprocess.PIPE,
                                 stderr=subprocess.PIPE)
        try:
            child.stdin.write('{"message":"Hello","model":"test/model"}\n')
            child.stdin.flush()
            capture = self.home / "capture.json"
            server_pid = None
            deadline = time.monotonic() + 5
            while time.monotonic() < deadline:
                if capture.exists():
                    try:
                        server_pid = int(json.loads(capture.read_text())["pid"])
                        break
                    except (json.JSONDecodeError, KeyError):
                        pass
                time.sleep(0.05)
            self.assertIsNotNone(server_pid)
            child.send_signal(signal.SIGTERM)
            stdout, stderr = child.communicate(timeout=10)
            self.assertEqual(child.returncode, 0, stderr)
            self.assertIn('"done"', stdout)
            with self.assertRaises(ProcessLookupError):
                os.kill(server_pid, 0)
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
